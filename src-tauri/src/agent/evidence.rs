//! A separately verified, no-tools CLI profile. Ordinary Chat permissions are unchanged.
use super::{AdapterInvocationRequest, AgentStage, PreparedAgentInvocation, ADAPTER_CLAUDE};
use crate::models::AgentConfig;
use crate::tool_process::{ProcessOwner, Session, Spec};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    time::{Duration, SystemTime},
};

const VERIFIED_CLAUDE_VERSION: &str = "2.1.311 (Claude Code)";

/// Cannot be constructed by a feature or deserialized from IPC. Binds a version
/// probe to this executable and saved configuration, and is rechecked at prepare.
#[derive(Clone, Debug)]
pub struct EvidenceProfile {
    program: String,
    modified: SystemTime,
    bytes: u64,
    config_digest: String,
}

fn unsupported(detail: &str) -> String {
    format!("unsupported_execution_profile\n\n{detail}")
}

pub(super) fn supported(agent: &AgentConfig, version: Option<&str>) -> bool {
    agent.adapter_type == ADAPTER_CLAUDE
        && agent.args.is_empty()
        && version.map(str::trim) == Some(VERIFIED_CLAUDE_VERSION)
}

fn config_digest(agent: &AgentConfig) -> Result<String, String> {
    let bytes = serde_json::to_vec(agent).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub fn evidence_execution_identity(agent: &AgentConfig) -> Result<String, String> {
    let path = super::resolve_executable(&agent.command)
        .and_then(|p| std::fs::canonicalize(p).ok())
        .ok_or_else(|| unsupported("Agent executable not found"))?;
    let metadata = std::fs::metadata(&path).map_err(|e| unsupported(&e.to_string()))?;
    let modified = metadata
        .modified()
        .map_err(|e| unsupported(&e.to_string()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| unsupported(&e.to_string()))?
        .as_nanos();
    let identity = serde_json::to_vec(&(
        path,
        metadata.len(),
        modified,
        VERIFIED_CLAUDE_VERSION,
        config_digest(agent)?,
    ))
    .map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(identity)))
}

pub async fn verify_evidence_profile(agent: &AgentConfig) -> Result<EvidenceProfile, String> {
    verify_profile_owned(agent, None, &|| false).await
}
pub async fn verify_evidence_profile_for_run(
    agent: &AgentConfig,
    owner: ProcessOwner,
    cancelled: &impl Fn() -> bool,
) -> Result<EvidenceProfile, String> {
    verify_profile_owned(agent, Some(owner), cancelled).await
}
async fn verify_profile_owned(
    agent: &AgentConfig,
    owner: Option<ProcessOwner>,
    cancelled: &impl Fn() -> bool,
) -> Result<EvidenceProfile, String> {
    if cancelled() {
        return Err(unsupported("Verification cancelled"));
    }
    if !agent.enabled
        || !agent.available
        || agent.adapter_type != ADAPTER_CLAUDE
        || !agent.args.is_empty()
    {
        return Err(unsupported(
            "Evidence-only requires an enabled, verified Claude CLI without extra arguments",
        ));
    }
    let program = super::resolve_executable(&agent.command)
        .ok_or_else(|| unsupported("Agent executable not found"))?;
    let program = std::fs::canonicalize(program).map_err(|e| unsupported(&e.to_string()))?;
    let before = std::fs::metadata(&program).map_err(|e| unsupported(&e.to_string()))?;
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let operation_id = format!(
        "evidence-profile-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let mut process = Session::spawn(Spec {
        operation_id: operation_id.clone(),
        owner: owner.unwrap_or_else(|| ProcessOwner::Feature {
            feature: "agent-diagnostic".into(),
            scope_id: agent.id.clone(),
            run_id: operation_id,
        }),
        program: program.clone(),
        args: vec!["--version".into()],
        cwd: std::fs::canonicalize(std::env::temp_dir())
            .map_err(|e| unsupported(&e.to_string()))?,
        environment: Default::default(),
        timeout: Duration::from_secs(15),
        max_frame_bytes: 4096,
        max_output_bytes: 64 * 1024,
        disk_limit: None,
    })
    .map_err(|e| unsupported(&e))?;
    let bytes = process.frame(cancelled).await;
    let status = if bytes.is_ok() {
        process.wait_exit(cancelled).await
    } else {
        Ok(None)
    };
    process.close().await.map_err(|e| unsupported(&e))?;
    let bytes = bytes.map_err(|e| unsupported(&e))?;
    let version = String::from_utf8_lossy(&bytes);
    if status.map_err(|e| unsupported(&e))? != Some(0) || !supported(agent, Some(&version)) {
        return Err(unsupported(
            "This CLI version has not passed the evidence-only acceptance probe",
        ));
    }
    let profile = EvidenceProfile {
        program: program.to_string_lossy().into_owned(),
        modified: before.modified().map_err(|e| unsupported(&e.to_string()))?,
        bytes: before.len(),
        config_digest: config_digest(agent)?,
    };
    profile.validate(agent)?;
    Ok(profile)
}

impl EvidenceProfile {
    fn validate(&self, agent: &AgentConfig) -> Result<(), String> {
        let current = super::resolve_executable(&agent.command)
            .and_then(|path| std::fs::canonicalize(path).ok())
            .ok_or_else(|| unsupported("Executable is no longer available"))?;
        let metadata = std::fs::metadata(&current).map_err(|e| unsupported(&e.to_string()))?;
        if current != Path::new(&self.program)
            || metadata.len() != self.bytes
            || metadata.modified().ok() != Some(self.modified)
            || config_digest(agent)? != self.config_digest
        {
            return Err(unsupported(
                "Agent configuration or executable changed; repeat preflight",
            ));
        }
        Ok(())
    }

    pub(super) fn prepare(
        &self,
        agent: &AgentConfig,
        request: &AdapterInvocationRequest<'_>,
    ) -> Result<PreparedAgentInvocation, String> {
        self.validate(agent)?;
        if !agent.enabled
            || !agent.available
            || request.chat_permission_mode != Some("explore")
            || request.stage != AgentStage::Planning
            || request.resume_command.is_some()
            || request.system_append.is_some()
            || request.prompt_file.is_some()
            || request.embed_prompt
        {
            return Err(unsupported(
                "Evidence-only requires a fresh stdin planning invocation without tools or resume",
            ));
        }
        let mut args: Vec<String> = [
            "--safe-mode",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--no-session-persistence",
            "--no-chrome",
            "--disable-slash-commands",
            "--permission-mode",
            "plan",
            "--print",
            "--verbose",
            "--output-format",
            "stream-json",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        if let Some(model) = request.model {
            super::validate_model_name(model)?;
            args.extend(["--model".into(), model.into()]);
        }
        Ok(PreparedAgentInvocation {
            program: self.program.clone(),
            args,
            cwd: request.project_path.to_string_lossy().into_owned(),
            stdin_prompt: true,
            output_mode: "claude_stream_json".into(),
            resumed: false,
            environment: Default::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn agent(path: &Path) -> AgentConfig {
        AgentConfig {
            id: "evidence-fixture".into(),
            name: "Fixture".into(),
            command: path.to_string_lossy().into_owned(),
            args: vec![],
            working_directory_policy: "project_root".into(),
            capabilities: vec![],
            adapter_type: ADAPTER_CLAUDE.into(),
            can_write_files: false,
            can_run_commands: false,
            enabled: true,
            available: true,
        }
    }
    #[tokio::test]
    async fn evidence_profile_rejects_changed_configuration_and_unverified_versions() {
        let dir = std::env::temp_dir().join(format!(
            "loom-evidence-profile-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("claude");
        std::fs::write(&path, "#!/bin/sh\necho '2.1.311 (Claude Code)'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut agent = agent(&path);
        let profile = verify_evidence_profile(&agent).await.unwrap();
        let mut request = AdapterInvocationRequest {
            execution_profile: Some(&profile),
            project_path: &dir,
            prompt: "data",
            prompt_file: None,
            stage: AgentStage::Planning,
            resume_command: None,
            chat_permission_mode: Some("explore"),
            embed_prompt: false,
            system_append: None,
            model: Some("model"),
        };
        let prepared = super::super::prepare_invocation(&agent, &request).unwrap();
        assert!(prepared.args.windows(2).any(|p| p == ["--tools", ""]));
        assert!(prepared
            .args
            .windows(2)
            .any(|p| p == ["--mcp-config", "{\"mcpServers\":{}}"]));
        assert!(prepared.args.contains(&"--safe-mode".into()));
        assert!(prepared.stdin_prompt && !prepared.resumed);
        request.resume_command = Some("session");
        assert!(super::super::prepare_invocation(&agent, &request).is_err());
        request.resume_command = None;
        agent.args.push("--tools=Bash".into());
        assert!(super::super::prepare_invocation(&agent, &request).is_err());
        agent.args.clear();
        std::fs::write(&path, "#!/bin/sh\necho '2.1.312 (Claude Code)'\n").unwrap();
        assert!(super::super::prepare_invocation(&agent, &request).is_err());
        assert!(verify_evidence_profile(&agent).await.is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn fake_profile_runs_through_runner_with_exact_argv_and_bounded_exit_paths() {
        use super::super::{run, Limits, Observation, ProcessKind, ProcessOwner, RunSpec};
        let dir = std::env::temp_dir().join(format!(
            "loom-evidence-runner-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("claude");
        std::fs::write(&path,r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo '2.1.311 (Claude Code)'; exit 0; fi
printf '%s\n' "$@" > args.txt
printf '%s' "$LOOM_PROFILE_FIXTURE" > environment.txt
cat > input.txt
case "$LOOM_PROFILE_FIXTURE" in
  failure) exit 7 ;;
  timeout) sleep 30 ;;
  signal) kill -TERM $$ ;;
esac
printf '%s\n' '{"type":"system","subtype":"init","tools":[],"mcp_servers":[]}'
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"LOOM_FIXTURE_RESULT"}]}}'
printf '%s\n' '{"type":"result","is_error":false,"result":"LOOM_FIXTURE_RESULT"}'
"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let agent = agent(&path);
        let profile = verify_evidence_profile(&agent).await.unwrap();
        for mode in ["success", "failure", "signal", "timeout"] {
            let cwd = dir.join(mode);
            std::fs::create_dir(&cwd).unwrap();
            let request = AdapterInvocationRequest {
                execution_profile: Some(&profile),
                project_path: &cwd,
                prompt: "SUPPLIED_ONLY",
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: None,
                chat_permission_mode: Some("explore"),
                embed_prompt: false,
                system_append: None,
                model: Some("fixture-model"),
            };
            let mut prepared = super::super::prepare_invocation(&agent, &request).unwrap();
            let expected = prepared.args.clone();
            prepared
                .environment
                .insert("LOOM_PROFILE_FIXTURE".into(), mode.into());
            let mut pid = None;
            let outcome = run(
                RunSpec {
                    prepared,
                    run_id: format!("profile-fixture-{}-{mode}", crate::models::now_ms()),
                    owner: ProcessOwner::Feature {
                        feature: "fixture".into(),
                        scope_id: "evidence".into(),
                        run_id: mode.into(),
                    },
                    kind: ProcessKind::Feature,
                    stdin_text: Some("SUPPLIED_ONLY".into()),
                    limits: Limits {
                        // Long enough for the fixture to finish writing args/env/input before the kill; it then sleeps 30s.
                        timeout: Duration::from_millis(if mode == "timeout" { 1000 } else { 3000 }),
                        ..Default::default()
                    },
                    enrich: None,
                },
                || None,
                |_| Ok(()),
                |event| {
                    if let Observation::Started(id) = event {
                        pid = Some(id);
                    }
                    Ok(())
                },
            )
            .await
            .unwrap();
            if let Some(pid) = pid {
                crate::tool_process::confirm_exit(pid).await.unwrap();
            }
            let args = std::fs::read_to_string(cwd.join("args.txt")).unwrap();
            assert_eq!(
                args.split_terminator('\n').collect::<Vec<_>>(),
                expected.iter().map(String::as_str).collect::<Vec<_>>()
            );
            assert_eq!(
                std::fs::read_to_string(cwd.join("environment.txt")).unwrap(),
                mode
            );
            assert_eq!(
                std::fs::read_to_string(cwd.join("input.txt")).unwrap(),
                "SUPPLIED_ONLY"
            );
            if mode == "success" {
                assert_eq!(outcome.status, "complete");
                assert!(outcome.content.contains("LOOM_FIXTURE_RESULT"));
            } else {
                assert_ne!(outcome.status, "complete");
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    #[ignore = "uses the installed Claude CLI and selected model account"]
    async fn real_evidence_profile_through_agent_runner() {
        use super::super::{run, Limits, Observation, ProcessKind, ProcessOwner, RunSpec};
        if std::env::var("LOOM_REAL_EVIDENCE").as_deref() != Ok("1") {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "loom-evidence-real-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let command = std::env::var("LOOM_EVIDENCE_CLI").unwrap_or_else(|_| "claude".into());
        let model =
            std::env::var("LOOM_EVIDENCE_MODEL").expect("select an explicit real-test model");
        let agent = agent(Path::new(&command));
        let profile = super::super::verify_evidence_profile(&agent).await.unwrap();
        let prompt = "Use only the supplied evidence: nineteen plus twenty-three equals forty-two. Reply exactly LOOM_RESULT_42.";
        let request = AdapterInvocationRequest {
            execution_profile: Some(&profile),
            project_path: &dir,
            prompt,
            prompt_file: None,
            stage: AgentStage::Planning,
            resume_command: None,
            chat_permission_mode: Some("explore"),
            embed_prompt: false,
            system_append: None,
            model: Some(&model),
        };
        let prepared = super::super::prepare_invocation(&agent, &request).unwrap();
        let mut initialized = false;
        let outcome = run(
            RunSpec {
                prepared,
                run_id: format!("evidence-real-{}", crate::models::now_ms()),
                owner: ProcessOwner::Feature {
                    feature: "research".into(),
                    scope_id: "r0".into(),
                    run_id: "real-evidence".into(),
                },
                kind: ProcessKind::Feature,
                stdin_text: Some(prompt.into()),
                limits: Limits {
                    timeout: Duration::from_secs(120),
                    ..Limits::default()
                },
                enrich: None,
            },
            || None,
            |_| Ok(()),
            |observation| {
                if let Observation::Log {
                    stderr: false,
                    line,
                } = observation
                {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if value["type"] == "system" && value["subtype"] == "init" {
                            if value["tools"]
                                .as_array()
                                .is_none_or(|items| !items.is_empty())
                                || value["mcp_servers"]
                                    .as_array()
                                    .is_none_or(|items| !items.is_empty())
                            {
                                return Err("evidence profile advertised unexpected tools".into());
                            }
                            initialized = true;
                        }
                    }
                }
                Ok(())
            },
        )
        .await
        .unwrap();
        assert!(initialized, "missing tool inventory");
        assert_eq!(outcome.status, "complete", "{:?}", outcome.error_summary);
        assert!(outcome.content.contains("LOOM_RESULT_42"));
        assert!(!outcome
            .parts
            .iter()
            .any(|p| matches!(p, super::super::MessagePart::Tool { .. })));
        println!(
            "evidence-only real CLI: tools=0, MCP=0, model response verified, exit={:?}",
            outcome.exit_code
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
