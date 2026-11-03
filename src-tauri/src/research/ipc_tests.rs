use crate::models::AgentConfig;
pub struct Context {
    pub root: std::path::PathBuf,
    pub agents: Vec<AgentConfig>,
}

use super::{
    commands::*,
    models::Workflow,
    store::{
        tests::{sample_research, temp_root},
        Store,
    },
    target_commands::*,
    target_models::*,
    target_store::TargetStore,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use tauri::{
    test::{mock_builder, mock_context, noop_assets, MockRuntime},
    WebviewWindow,
};

fn app(root: &Path, agents: Vec<AgentConfig>) -> tauri::App<MockRuntime> {
    mock_builder()
        .manage(Context {
            root: root.into(),
            agents,
        })
        .invoke_handler(tauri::generate_handler![
            research_list,
            research_get,
            research_create,
            research_ask,
            research_retry,
            research_cancel,
            research_read_turn,
            research_list_tree,
            research_read_source,
            research_read_job_logs,
            research_delete,
            research_preflight,
            research_start_run,
            research_get_run,
            research_list_runs,
            research_cancel_run,
            research_read_report,
            research_list_report_tree,
            research_read_report_source,
            research_list_evidence,
            research_read_evidence,
            research_ask_run,
            research_retry_run,
            research_resume_run,
            research_export
        ])
        .build(mock_context(noop_assets()))
        .unwrap()
}
fn window(app: &tauri::App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    tauri::WebviewWindowBuilder::new(app, label, Default::default())
        .build()
        .unwrap()
}
fn invoke(window: &WebviewWindow<MockRuntime>, command: &str, args: Value) -> Result<Value, Value> {
    tauri::test::get_ipc_response(
        window,
        tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .map(|body| body.deserialize::<Value>().unwrap())
}
fn assert_error(window: &WebviewWindow<MockRuntime>, command: &str, args: Value, message: &str) {
    let error = invoke(window, command, args).unwrap_err();
    assert!(error.to_string().contains(message), "{command}: {error}");
}
fn target_fixture(root: &Path) -> (TargetStore, RunSpec, String) {
    let store = Store::new(root);
    let mut record = sample_research("research-target");
    let target = Target::App {
        path: root.join("app").to_string_lossy().into_owned(),
    };
    record.workflow = Workflow::Target;
    record.schema_version = 2;
    record.target = Some(TargetConfig {
        target: target.clone(),
        run_ids: vec![],
    });
    store.create(&record).unwrap();
    let target_store = TargetStore::open(store, "research-target").unwrap();
    let spec = RunSpec {
        schema_version: 2,
        research_id: "research-target".into(),
        run_id: "run-a".into(),
        kind: RunKind::Initial,
        input: TargetInput {
            target,
            question: "Explain".into(),
            agent_id: "fake".into(),
            model: None,
            language: "en".into(),
            mode: ResearchMode::Single,
            budget: None,
            tools: None,
        },
        budget: Budget::defaults(ResearchMode::Single),
        config_digest: "fixture".into(),
        agent_config_digest: "fixture".into(),
        target_identity: "fixture".into(),
        parent_report_id: None,
        source_snapshots: vec![],
        source_evidence: vec![],
        created_at_ms: 1,
    };
    let writer = target_store.writer(Some("run-a")).unwrap();
    writer.create_run(&spec).unwrap();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("index.js"), "const immutable=42;\n").unwrap();
    writer
        .snapshot_from(&source, &["index.js".into()], "capture-a", 4096)
        .unwrap();
    let report = writer
        .publish_report("run-a", "[source](loom-src:index.js#L1-L1)")
        .unwrap();
    let mut state = target_store.run("run-a").unwrap().state;
    state.status = RunStatus::Completed;
    writer.save_state(state, "complete").unwrap();
    drop(writer);
    (target_store, spec, report.report_id)
}

#[test]
fn actual_invoke_enforces_workflow_window_and_report_scope() {
    let root = temp_root();
    let (target, _, report) = target_fixture(&root);
    let store = Store::new(&root);
    store.create(&sample_research("research-legacy")).unwrap();
    store
        .write_turn("research-legacy", 1, "No source references.")
        .unwrap();
    let app = app(&root, vec![]);
    let main = window(&app, "main");
    let secondary = window(&app, "secondary");
    invoke(
        &main,
        "research_read_turn",
        json!({"researchId":"research-legacy","turn":1}),
    )
    .unwrap();
    let _ = invoke(
        &main,
        "research_list_tree",
        json!({"researchId":"research-legacy"}),
    );
    assert!(
        !store.workspace_path("research-legacy").unwrap().exists(),
        "read IPC must not create workspace directories"
    );

    for (command, args) in [
        (
            "research_ask",
            json!({"researchId":"research-target","question":"followup"}),
        ),
        ("research_retry", json!({"researchId":"research-target"})),
        ("research_cancel", json!({"researchId":"research-target"})),
        (
            "research_read_turn",
            json!({"researchId":"research-target","turn":1}),
        ),
        (
            "research_list_tree",
            json!({"researchId":"research-target"}),
        ),
        (
            "research_read_source",
            json!({"researchId":"research-target","path":"index.js"}),
        ),
        (
            "research_read_job_logs",
            json!({"researchId":"research-target","jobId":"job-a","stream":"stdout"}),
        ),
    ] {
        assert_error(&main, command, args, "workflow_mismatch");
    }
    assert_error(
        &main,
        "research_create",
        json!({"input":{"question":"x","agentId":"fake","language":"en","target":{"kind":"app","path":"/private"}}}),
        "unknown field",
    );
    for command in ["research_get_run", "research_cancel_run"] {
        assert_error(
            &main,
            command,
            json!({"researchId":"research-legacy","runId":"run-a"}),
            "workflow_mismatch",
        );
    }
    let source=invoke(&main,"research_read_report_source",json!({"researchId":"research-target","reportId":report,"path":"index.js","startLine":1,"endLine":1})).unwrap();
    assert_eq!(source["text"], "const immutable=42;");
    assert_error(
        &main,
        "research_read_report_source",
        json!({"researchId":"research-target","reportId":report,"path":"../private","startLine":1,"endLine":1}),
        "source_outside_snapshot",
    );
    assert_error(
        &main,
        "research_read_report",
        json!({"researchId":"research-target","reportId":"report-foreign"}),
        "unavailable",
    );
    assert_error(
        &main,
        "research_read_evidence",
        json!({"researchId":"research-target","reportId":report,"evidenceId":"evidence-foreign","offset":0,"limit":10}),
        "scope_mismatch",
    );
    for (command, args) in [
        ("research_get", json!({"researchId":"research-target"})),
        (
            "research_cancel_run",
            json!({"researchId":"research-target","runId":"run-a"}),
        ),
        ("research_delete", json!({"researchId":"research-target"})),
        (
            "research_start_run",
            json!({"preflightId":"preflight-a","requestId":"request-a"}),
        ),
    ] {
        assert_error(&secondary, command, args, "main window");
    }
    let shared = json!({"researchId":"research-target","runId":"run-a","reportId":report,"parentReportId":report,"question":"followup","requestId":"request-secondary","preflightId":"preflight-secondary","path":"index.js","startLine":1,"endLine":1,"offset":0,"limit":10,"evidenceId":"evidence-foreign","turn":1,"jobId":"job-a","stream":"stdout"});
    for command in [
        "research_list",
        "research_get",
        "research_ask",
        "research_retry",
        "research_cancel",
        "research_read_turn",
        "research_list_tree",
        "research_read_source",
        "research_read_job_logs",
        "research_delete",
        "research_get_run",
        "research_list_runs",
        "research_cancel_run",
        "research_read_report",
        "research_list_report_tree",
        "research_read_report_source",
        "research_list_evidence",
        "research_read_evidence",
        "research_ask_run",
        "research_retry_run",
        "research_resume_run",
        "research_start_run",
    ] {
        assert_error(&secondary, command, shared.clone(), "main window");
    }
    assert_error(
        &secondary,
        "research_create",
        json!({"input":{"question":"x","agentId":"fake","language":"en"}}),
        "main window",
    );
    assert_error(
        &secondary,
        "research_preflight",
        json!({"spec":{"target":{"kind":"app","path":"/not-read"},"question":"x","agentId":"fake","model":null,"language":"en","mode":"single","budget":null,"tools":null}}),
        "main window",
    );
    assert_error(
        &secondary,
        "research_export",
        json!({"researchId":"research-target","input":{"reportId":report,"evidenceIds":[],"includeRaw":false}}),
        "main window",
    );
    let owner = target.writer(Some("run-a")).unwrap();
    assert_error(
        &main,
        "research_delete",
        json!({"researchId":"research-target"}),
        "research_busy",
    );
    let read = invoke(
        &main,
        "research_get",
        json!({"researchId":"research-target"}),
    )
    .unwrap();
    assert_eq!(read["activeOwner"]["runId"], "run-a");
    drop(owner);
    drop(main);
    drop(secondary);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn fake_agent(root: &Path) -> AgentConfig {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("fake-claude");
    fs::write(&path,r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo '2.1.311 (Claude Code)'; exit 0; fi
cat >/dev/null
printf '%s\n' '{"type":"system","subtype":"init","tools":[],"mcp_servers":[]}'
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"{\"kind\":\"answer\",\"markdown\":\"Only saved evidence was supplied. Runtime remains unknown.\"}"}]}}'
printf '%s\n' '{"type":"result","is_error":false,"result":"{\"kind\":\"answer\",\"markdown\":\"Only saved evidence was supplied. Runtime remains unknown.\"}"}'
"#).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    AgentConfig {
        id: "fake".into(),
        name: "Fake".into(),
        command: path.to_string_lossy().into_owned(),
        args: vec![],
        working_directory_policy: "project_root".into(),
        capabilities: vec![],
        adapter_type: "claude_code_cli".into(),
        can_write_files: false,
        can_run_commands: false,
        enabled: true,
        available: true,
        provider_id: None,
    }
}
fn terminal(main: &WebviewWindow<MockRuntime>, research: &str, run: &str) -> Value {
    let start = Instant::now();
    loop {
        let view = invoke(
            main,
            "research_get_run",
            json!({"researchId":research,"runId":run}),
        )
        .unwrap();
        if ["completed", "failed", "cancelled", "interrupted"]
            .contains(&view["state"]["status"].as_str().unwrap())
            && invoke(main, "research_get", json!({"researchId":research}))
                .unwrap()
                .get("activeOwner")
                .is_none()
        {
            return view;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "run did not finish: {view}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(unix)]
#[test]
fn actual_invoke_followup_is_idempotent_and_never_collects_or_reads_removed_target() {
    let root = temp_root();
    let (target, mut spec, report) = target_fixture(&root);
    let agent = fake_agent(&root);
    spec.agent_config_digest = crate::agent::evidence_execution_identity(&agent).unwrap();
    super::files::write_json(&target.path("runs/run-a/spec.json").unwrap(), &spec).unwrap();
    fs::remove_dir_all(root.join("source")).unwrap();
    let app = app(&root, vec![agent]);
    let main = window(&app, "main");
    let args = json!({"researchId":"research-target","parentReportId":report,"question":"What is unknown?","requestId":"followup-request"});
    let first = invoke(&main, "research_ask_run", args.clone()).unwrap();
    let run = first["spec"]["runId"].as_str().unwrap();
    let repeated = invoke(&main, "research_ask_run", args.clone()).unwrap();
    assert_eq!(repeated["spec"]["runId"], run);
    let done = terminal(&main, "research-target", run);
    assert_eq!(done["state"]["status"], "completed", "{done}");
    assert_eq!(done["state"]["usage"]["calls"], 1);
    assert_eq!(done["state"]["usage"]["collections"], 0);
    assert_eq!(
        done["state"]["snapshotIds"],
        json!(target.run("run-a").unwrap().state.snapshot_ids)
    );
    let mut conflict = args;
    conflict["question"] = json!("different");
    assert_error(&main, "research_ask_run", conflict, "request_id_conflict");
    assert_eq!(
        invoke(
            &main,
            "research_list_runs",
            json!({"researchId":"research-target"})
        )
        .unwrap()
        .as_array()
        .unwrap()
        .len(),
        2
    );
    drop(main);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn fake_rea(root: &Path) -> crate::rea::Config {
    use std::os::unix::fs::PermissionsExt;
    let package = root.join("rea");
    fs::create_dir_all(package.join("scripts")).unwrap();
    fs::write(
        package.join("package.json"),
        r#"{"name":"rea-agents","version":"6.8.0"}"#,
    )
    .unwrap();
    fs::write(package.join("scripts/rea.mjs"), "// fixture entry\n").unwrap();
    let mut tools: Vec<Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/research/rea-6.8.0-selected-schemas.json"
    ))
    .unwrap();
    for n in tools.len()..229 {
        tools.push(json!({"name":format!("fixture_{n}")}));
    }
    fs::write(
        package.join("catalog.json"),
        serde_json::to_vec(&json!({"tools":tools})).unwrap(),
    )
    .unwrap();
    let node = package.join("fake-node");
    fs::write(&node,r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo v26.12.1; exit 0; fi
package=${1%/scripts/rea.mjs}
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  [ -n "$id" ] || continue
  case "$line" in
    *'"method":"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"rea","version":"6.8.0"}}}\n' "$id" ;;
    *'"method":"tools/list"'*) printf '{"jsonrpc":"2.0","id":%s,"result":' "$id"; cat "$package/catalog.json"; printf '}\n' ;;
    *'"name":"binary_session"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"structuredContent":{"result":{"server_identity":{"catalog":{"digests":{"combined_sha256":"4748bd4d8c81aaecf7fceceecfbaf14302cab6cb99cd878c0141b30b9eb577b2"}}}}}}}\n' "$id" ;;
    *'"name":"get_evidence_bundle"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"structuredContent":{"result":{"records":[]}}}}\n' "$id" ;;
    *) printf '{"jsonrpc":"2.0","id":%s,"result":{"structuredContent":{"result":{"evidence_id":"ev_0000000000000000000000000000000000000000000000000000000000000000","normalized_result":{"coverage":{"status":"partial"}},"limitations":["IPC fixture only"]}}}}\n' "$id" ;;
  esac
done
"#).unwrap();
    fs::set_permissions(&node, fs::Permissions::from_mode(0o700)).unwrap();
    crate::rea::Config {
        node,
        entry: package.join("scripts/rea.mjs"),
    }
}

#[cfg(unix)]
#[test]
fn actual_start_replay_rejects_conflicts_and_never_creates_another_run() {
    let root = temp_root();
    let agent = fake_agent(&root);
    let selected = root.join("app");
    fs::create_dir(&selected).unwrap();
    fs::write(selected.join("package.json"), "{}").unwrap();
    let tools = fake_rea(&root);
    let store = Store::new(&root);
    let input = TargetInput {
        target: Target::App {
            path: selected.to_string_lossy().into_owned(),
        },
        question: "Explain".into(),
        agent_id: agent.id.clone(),
        model: None,
        language: "en".into(),
        mode: ResearchMode::Single,
        budget: None,
        tools: Some(tools),
    };
    let app = app(&root, vec![agent]);
    let main = window(&app, "main");
    let prepared = invoke(&main, "research_preflight", json!({"spec":input})).unwrap();
    let args = json!({"preflightId":prepared["preflightId"],"requestId":"start-once"});
    let first = invoke(&main, "research_start_run", args.clone()).unwrap();
    let second = invoke(&main, "research_start_run", args.clone()).unwrap();
    assert_eq!(first["spec"], second["spec"]);
    let research = first["spec"]["researchId"].as_str().unwrap();
    let run = first["spec"]["runId"].as_str().unwrap();
    let done = terminal(&main, research, run);
    assert_eq!(done["state"]["status"], "completed", "{done}");
    assert_eq!(done["state"]["usage"]["calls"], 1);
    assert_eq!(
        invoke(&main, "research_start_run", args).unwrap()["spec"],
        first["spec"]
    );
    assert_error(
        &main,
        "research_start_run",
        json!({"preflightId":"preflight-other","requestId":"start-once"}),
        "request_id_conflict",
    );
    assert_eq!(store.list().items.len(), 1);
    assert_eq!(
        invoke(&main, "research_list_runs", json!({"researchId":research}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    drop(main);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn second_app_invoke_reads_live_state_and_cancels_only_current_run() {
    let root = temp_root();
    let (target, mut spec, report) = target_fixture(&root);
    let agent = fake_agent(&root);
    let path = Path::new(&agent.command);
    let source = fs::read_to_string(path)
        .unwrap()
        .replace("cat >/dev/null", "cat >/dev/null\nsleep 30");
    fs::write(path, source).unwrap();
    spec.agent_config_digest = crate::agent::evidence_execution_identity(&agent).unwrap();
    super::files::write_json(&target.path("runs/run-a/spec.json").unwrap(), &spec).unwrap();
    let first = app(&root, vec![agent.clone()]);
    let second = app(&root, vec![agent]);
    let main = window(&first, "main");
    let reader = window(&second, "main");
    let receipt=invoke(&main,"research_ask_run",json!({"researchId":"research-target","parentReportId":report,"question":"wait","requestId":"cancel-followup"})).unwrap();
    let run = receipt["spec"]["runId"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let view = invoke(
            &reader,
            "research_get_run",
            json!({"researchId":"research-target","runId":run}),
        )
        .unwrap();
        if view["state"]["usage"]["calls"] == 1 {
            break;
        }
        assert_ne!(view["state"]["status"], "failed", "{view}");
        assert!(
            Instant::now() < deadline,
            "model dispatch did not start: {view}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let journal = target.path(&format!("runs/{run}/events.jsonl")).unwrap();
    let before = fs::read(&journal).unwrap();
    let live = invoke(&reader, "research_list", json!({})).unwrap();
    assert_eq!(live["items"][0]["activeOwner"]["runId"], run);
    assert_eq!(fs::read(&journal).unwrap(), before);
    assert_eq!(
        invoke(
            &reader,
            "research_cancel_run",
            json!({"researchId":"research-target","runId":"run-a"})
        )
        .unwrap(),
        false
    );
    assert_eq!(
        invoke(
            &reader,
            "research_cancel_run",
            json!({"researchId":"research-target","runId":run})
        )
        .unwrap(),
        true
    );
    let stopped = terminal(&reader, "research-target", run);
    assert_eq!(stopped["state"]["status"], "cancelled", "{stopped}");
    assert_eq!(
        invoke(
            &reader,
            "research_cancel_run",
            json!({"researchId":"research-target","runId":run})
        )
        .unwrap(),
        false
    );
    drop(main);
    drop(reader);
    drop(first);
    drop(second);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn shutdown_child_rejects_new_admission_and_drains_current_owner() {
    let Ok(root) = std::env::var("LOOM_RESEARCH_SHUTDOWN_FIXTURE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let (target, mut spec, report) = target_fixture(&root);
    let agent = fake_agent(&root);
    let path = Path::new(&agent.command);
    let script = fs::read_to_string(path)
        .unwrap()
        .replace("cat >/dev/null", "cat >/dev/null\nsleep 30");
    fs::write(path, script).unwrap();
    spec.agent_config_digest = crate::agent::evidence_execution_identity(&agent).unwrap();
    super::files::write_json(&target.path("runs/run-a/spec.json").unwrap(), &spec).unwrap();
    let app = app(&root, vec![agent]);
    let main = window(&app, "main");
    let args = json!({"researchId":"research-target","parentReportId":report,"question":"wait","requestId":"shutdown-request"});
    let receipt = invoke(&main, "research_ask_run", args.clone()).unwrap();
    let run = receipt["spec"]["runId"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let view = invoke(
            &main,
            "research_get_run",
            json!({"researchId":"research-target","runId":run}),
        )
        .unwrap();
        if view["state"]["usage"]["calls"] == 1 {
            break;
        }
        assert_ne!(view["state"]["status"], "failed", "{view}");
        assert!(
            Instant::now() < deadline,
            "model dispatch did not start: {view}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    crate::agent::begin_shutdown().unwrap();
    let mut fresh = args.clone();
    fresh["requestId"] = json!("new-after-shutdown");
    assert_error(&main, "research_ask_run", fresh, "退出");
    assert_eq!(
        invoke(&main, "research_ask_run", args).unwrap()["spec"]["runId"],
        run
    );
    let ended = terminal(&main, "research-target", run);
    assert_eq!(ended["state"]["status"], "interrupted", "{ended}");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let (operations, processes) = crate::agent::shutdown_snapshot().unwrap();
        if operations == 0 && processes.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "shutdown ownership not drained");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(target.writer(None).is_ok());
}
#[cfg(unix)]
#[test]
fn actual_invoke_shutdown_isolated_from_other_tests() {
    let root = temp_root();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "research::ipc_tests::shutdown_child_rejects_new_admission_and_drains_current_owner",
            "--nocapture",
        ])
        .env("LOOM_RESEARCH_SHUTDOWN_FIXTURE", &root)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated shutdown test timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn provider_error_exports_partial_evidence_and_keeps_captured_source() {
    let root = temp_root();
    let agent = fake_agent(&root);
    let tools = fake_rea(&root);
    let selected = root.join("app");
    fs::create_dir(&selected).unwrap();
    fs::write(selected.join("index.js"), "export const answer=42;\n").unwrap();
    let partial = json!({"evidence_id":format!("ev_{}","1".repeat(64)),"normalized_result":{"coverage":{"status":"partial"}},"limitations":["Provider timeout fixture"]});
    let script=fs::read_to_string(&tools.node).unwrap().replace("  case \"$line\" in",r#"  case "$line" in
    *'"name":"analyze_javascript_application"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"isError":true,"structuredContent":{"error":{"code":"provider_timeout"}}}}\n' "$id" ;;"#)
        .replace("\"records\":[]",&format!("\"records\":[{partial}]"));
    fs::write(&tools.node, script).unwrap();
    let input = TargetInput {
        target: Target::App {
            path: selected.to_string_lossy().into_owned(),
        },
        question: "Explain".into(),
        agent_id: agent.id.clone(),
        model: None,
        language: "en".into(),
        mode: ResearchMode::Single,
        budget: None,
        tools: Some(tools),
    };
    let app = app(&root, vec![agent]);
    let main = window(&app, "main");
    let preflight = invoke(&main, "research_preflight", json!({"spec":input})).unwrap();
    let started = invoke(
        &main,
        "research_start_run",
        json!({"preflightId":preflight["preflightId"],"requestId":"partial-start"}),
    )
    .unwrap();
    let research = started["spec"]["researchId"].as_str().unwrap();
    let run = started["spec"]["runId"].as_str().unwrap();
    let done = terminal(&main, research, run);
    assert_eq!(done["state"]["status"], "failed");
    assert_eq!(done["state"]["usage"]["calls"], 0);
    assert_eq!(done["state"]["error"], "rea_tool_error:provider_timeout");
    let report = done["state"]["reportIds"][0].as_str().unwrap();
    let evidence = invoke(
        &main,
        "research_list_evidence",
        json!({"researchId":research,"reportId":report}),
    )
    .unwrap();
    assert!(evidence
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["tool"] == "partial_evidence_bundle"));
    let source=invoke(&main,"research_read_report_source",json!({"researchId":research,"reportId":report,"path":"index.js","startLine":1,"endLine":1})).unwrap();
    assert!(source["text"].as_str().unwrap().contains("answer=42"));
    drop(main);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn cancellation_during_version_probe_does_not_wait_for_probe_timeout() {
    let root = temp_root();
    let (target, mut spec, report) = target_fixture(&root);
    let agent = fake_agent(&root);
    let path = Path::new(&agent.command);
    let script = fs::read_to_string(path)
        .unwrap()
        .replace("then echo '2.1.311", "then sleep 30; echo '2.1.311");
    fs::write(path, script).unwrap();
    spec.agent_config_digest = crate::agent::evidence_execution_identity(&agent).unwrap();
    super::files::write_json(&target.path("runs/run-a/spec.json").unwrap(), &spec).unwrap();
    let app = app(&root, vec![agent]);
    let main = window(&app, "main");
    let receipt=invoke(&main,"research_ask_run",json!({"researchId":"research-target","parentReportId":report,"question":"never dispatched","requestId":"cancel-profile"})).unwrap();
    let run = receipt["spec"]["runId"].as_str().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (_, processes) = crate::agent::shutdown_snapshot().unwrap();
        if processes.iter().any(|p|p.run_id.starts_with("evidence-profile-") && matches!(&p.owner,crate::agent::ProcessOwner::Feature{scope_id,run_id,..} if scope_id=="research-target" && run_id==run)){break;}
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        invoke(
            &main,
            "research_cancel_run",
            json!({"researchId":"research-target","runId":run})
        )
        .unwrap(),
        true
    );
    let ended = terminal(&main, "research-target", run);
    assert_eq!(ended["state"]["status"], "cancelled", "{ended}");
    assert_eq!(ended["state"]["usage"]["calls"], 0);
    assert!(ended["state"]["usage"]["activeMs"].as_u64().unwrap() < 15_000);
    drop(main);
    drop(app);
    fs::remove_dir_all(root).unwrap();
}
