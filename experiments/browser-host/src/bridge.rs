//! M0 native Chromium host protocol. This is not the final BrowserHost API.
use crate::shared::simple_handler::SimpleHandler;
use cef::*;
use serde_json::{Value as Json, json};
use std::io::{BufRead, Read};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
pub fn begin_shutdown() {
    SHUTTING_DOWN.store(true, Ordering::SeqCst);
}
pub fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}
pub fn emit(value: Json) {
    println!("{}", value);
}

wrap_dev_tools_message_observer! {
    pub struct ProbeObserver;
    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, message_id: i32, success: i32, result: Option<&[u8]>) {
            let result: Json = result.and_then(|bytes| serde_json::from_slice(bytes).ok()).unwrap_or(Json::Null);
            emit(json!({"kind":"cdpResult","id":message_id,"success":success != 0,"result":result}));
        }
    }
}

wrap_task! {
    struct ProbeCommand { inner: Arc<Mutex<SimpleHandler>>, request: String }
    impl Task {
        fn execute(&self) {
            let Ok(value) = serde_json::from_str::<Json>(&self.request) else { emit(json!({"kind":"error","message":"invalid JSON"})); return; };
            let mut handler = self.inner.lock().expect("browser handler lock");
            if value["kind"] == "close" {
                begin_shutdown();
                handler.close_all_browsers(false);
                return;
            }
            if value["kind"] == "show" { handler.show_main_window(); emit(json!({"kind":"shown"})); return; }
            let Some(mut browser) = handler.probe_browser() else { emit(json!({"kind":"error","message":"browser not ready"})); return; };
            if value["kind"] == "windowClose" {
                if let Some(window) = browser_view_get_for_browser(Some(&mut browser)).and_then(|view| view.window()) {
                    window.close(); return;
                }
                emit(json!({"kind":"error","message":"no native view window"})); return;
            }
            if value["kind"] == "hide" {
                if let Some(window) = browser_view_get_for_browser(Some(&mut browser)).and_then(|view| view.window()) {
                    window.hide(); emit(json!({"kind":"hidden"})); return;
                }
                emit(json!({"kind":"error","message":"no native view window"})); return;
            }
            let Some(host) = browser.host() else { return; };
            let Some(method) = value["method"].as_str() else { emit(json!({"kind":"error","message":"missing method"})); return; };
            // The probe controls only its fixture. No public debugging port is opened.
            if !["Runtime.evaluate", "Accessibility.getFullAXTree", "Page.getNavigationHistory"].contains(&method) {
                emit(json!({"kind":"error","message":"method not in M0 probe"})); return;
            }
            let id = value["id"].as_i64().filter(|id| *id > 0 && *id <= i32::MAX as i64);
            let Some(id) = id else { emit(json!({"kind":"error","message":"invalid ID"})); return; };
            let params = value.get("params").cloned().unwrap_or(json!({}));
            let mut params = parse_json(Some(&CefString::from(params.to_string().as_str())), JsonParserOptions::RFC).and_then(|value| value.dictionary());
            host.execute_dev_tools_method(id as i32, Some(&CefString::from(method)), params.as_mut());
        }
    }
}

pub fn start() {
    std::thread::spawn(|| {
        let input = std::io::stdin();
        let mut reader = input.lock();
        loop {
            let mut bytes = Vec::new();
            let Ok(size) = reader
                .by_ref()
                .take(1024 * 1024 + 1)
                .read_until(b'\n', &mut bytes)
            else {
                break;
            };
            if size == 0 {
                break;
            }
            if size > 1024 * 1024 || !bytes.ends_with(b"\n") {
                emit(json!({"kind":"error","message":"oversized or incomplete frame"}));
                break;
            }
            let Some(inner) = SimpleHandler::instance() else {
                emit(json!({"kind":"error","message":"not ready"}));
                continue;
            };
            let Ok(request) = String::from_utf8(bytes) else {
                continue;
            };
            let mut task = ProbeCommand::new(inner, request);
            post_task(ThreadId::UI, Some(&mut task));
        }
        if let Some(inner) = SimpleHandler::instance() {
            let mut task = ProbeCommand::new(inner, "{\"kind\":\"close\"}".into());
            post_task(ThreadId::UI, Some(&mut task));
        }
    });
}
