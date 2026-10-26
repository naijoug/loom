use super::*;
use crate::skills::{operations, tests::Fixture};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

fn server(
    status: u16,
    body: String,
    delay: Duration,
) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/translation", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = socket.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..n]);
            if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let _ = tx.send(String::from_utf8(request).unwrap());
        thread::sleep(delay);
        let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nLocation: /must-not-follow\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        let _ = socket.write_all(response.as_bytes());
    });
    (url, rx, handle)
}
fn context(op: &Operation, id: ProviderId) -> Context {
    Context {
        schema_version: 1,
        operation_id: op.record.operation_id.clone(),
        skill_id: "fixture".into(),
        source_id: "fixture".into(),
        source_digest: "digest".into(),
        input_digest: "digest".into(),
        provider_id: id,
        model: Some("fixture".into()),
        target_language: "zh-CN".into(),
        source_mode: SourceMode::Markdown,
        created_at_ms: 1,
    }
}
fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .unwrap()
}
#[tokio::test]
async fn http_transport_covers_success_auth_rate_limit_redirect_invalid_and_oversize() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let f = Fixture::new();
    let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    for (id, status, response, expected) in [
        (ProviderId::OpenAi, 200, json!({"status":"completed","output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"# 翻译"}]}]}).to_string(), None),
        (ProviderId::OpenRouter, 200, json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"# 翻译"}}]}).to_string(), None),
        (ProviderId::OpenAi, 401, "upstream-secret".into(), Some(ErrorCode::AuthRequired)),
        (ProviderId::OpenRouter, 403, "upstream-secret".into(), Some(ErrorCode::AuthRequired)),
        (ProviderId::OpenAi, 429, "upstream-secret".into(), Some(ErrorCode::RateLimited)),
        (ProviderId::OpenRouter, 503, "upstream-secret".into(), Some(ErrorCode::SourceUnavailable)),
        (ProviderId::OpenAi, 302, "upstream-secret".into(), Some(ErrorCode::SourceUnavailable)),
        (ProviderId::OpenAi, 200, "upstream-secret".into(), Some(ErrorCode::InvalidInput)),
        (ProviderId::OpenAi, 200, "x".repeat(MAX_RESULT * 2 + 1), Some(ErrorCode::InvalidInput)),
    ] {
        let (url, received, handle) = server(status, response, Duration::ZERO);
        let mut op = Operation::begin(&repo, "fixture", "translate").unwrap();
        let ctx = context(&op, id);
        let provider = ResolvedProvider { id, model: Some("fixture".into()), api_key: Some("credential-secret".into()) };
        let result = translate_with_client(&mut op, &provider, &ctx, "document-secret", &client(Duration::from_secs(3)), &url).await;
        if let Some(expected) = expected { assert_eq!(result.unwrap_err().code, expected); }
        else { assert_eq!(result.unwrap(), "# 翻译"); }
        let request = received.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(request.starts_with("POST /translation "));
        assert!(request.contains("Bearer credential-secret") && request.contains("document-secret"));
        assert_eq!(op.record.requests.len(), 1, "no retry");
        assert!(op.record.requests[0].finished_at_ms.is_some());
        let log = serde_json::to_string(&op.record).unwrap();
        for secret in ["credential-secret", "document-secret", "upstream-secret"] { assert!(!log.contains(secret)); }
        handle.join().unwrap();
    }
}
#[tokio::test]
async fn http_transport_timeout_and_inflight_cancellation_are_recorded_without_retry() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let f = Fixture::new();
    let repo = Repository::open(&f.0.join("data"), &f.0.join("coord")).unwrap();
    for cancel in [false, true] {
        let (url, received, handle) = server(200, "delayed".into(), Duration::from_millis(250));
        let mut op = Operation::begin(&repo, "fixture", "translate").unwrap();
        let ctx = context(&op, ProviderId::OpenAi);
        let provider = ResolvedProvider {
            id: ProviderId::OpenAi,
            model: Some("fixture".into()),
            api_key: Some("secret".into()),
        };
        let cancellation = if cancel {
            let repo = repo.clone();
            let id = op.record.operation_id.clone();
            Some(thread::spawn(move || {
                received.recv_timeout(Duration::from_secs(2)).unwrap();
                operations::cancel(&repo, &id).unwrap();
            }))
        } else {
            None
        };
        let error = translate_with_client(
            &mut op,
            &provider,
            &ctx,
            "text",
            &client(if cancel {
                Duration::from_secs(3)
            } else {
                Duration::from_millis(40)
            }),
            &url,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.code,
            if cancel {
                ErrorCode::Cancelled
            } else {
                ErrorCode::Timeout
            }
        );
        assert_eq!(op.record.requests.len(), 1);
        assert!(op.record.requests[0].finished_at_ms.is_some());
        assert!(op.record.requests[0].error.is_some());
        if let Some(thread) = cancellation {
            thread.join().unwrap();
        }
        handle.join().unwrap();
    }
}
