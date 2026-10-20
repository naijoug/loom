use serde_json::Value;
use std::path::Path;

pub const ADAPTER_CODEX: &str = "codex_cli";
pub const ADAPTER_CLAUDE_CODE: &str = "claude_code_cli";
pub const ADAPTER_OPENCODE: &str = "opencode_cli";
pub const ADAPTER_GROK: &str = "grok_cli";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedSession {
    pub session_id: Option<String>,
    pub resume_command: Option<String>,
}

pub fn capture_session_from_lines(
    command: &str,
    stdout_lines: &[String],
    stderr_lines: &[String],
) -> CapturedSession {
    let session_id = stdout_lines
        .iter()
        .chain(stderr_lines.iter())
        .find_map(|line| find_session_id_in_line(line));
    let resume_command = session_id.as_deref().and_then(|session_id| {
        adapter_type_for_command(command)
            .and_then(|adapter_type| resume_command_for_adapter(adapter_type, command, session_id))
    });

    CapturedSession {
        session_id,
        resume_command,
    }
}

pub fn capture_session_for_adapter(
    adapter: &str,
    command: &str,
    stdout_lines: &[String],
    stderr_lines: &[String],
) -> CapturedSession {
    let mut captured = capture_session_from_lines(command, stdout_lines, stderr_lines);
    captured.resume_command = captured
        .session_id
        .as_deref()
        .and_then(|id| resume_command_for_adapter(adapter, command, id));
    captured
}

pub fn resume_command_for_adapter(
    adapter_type: &str,
    command: &str,
    session_id: &str,
) -> Option<String> {
    match adapter_type {
        ADAPTER_CLAUDE_CODE => Some(format!("{command} --resume {session_id}")),
        ADAPTER_CODEX => Some(format!("{command} resume {session_id}")),
        ADAPTER_GROK => Some(format!("{command} --resume {session_id}")),
        ADAPTER_OPENCODE => Some(format!("{command} --session {session_id}")),
        _ => None,
    }
}

pub fn adapter_type_for_command(command: &str) -> Option<&'static str> {
    let command_name = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command);

    match command_name {
        "codex" => Some(ADAPTER_CODEX),
        "claude" | "claude-code" => Some(ADAPTER_CLAUDE_CODE),
        "grok" => Some(ADAPTER_GROK),
        "opencode" => Some(ADAPTER_OPENCODE),
        _ => None,
    }
}

pub fn find_session_id_in_line(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| find_session_id(&value))
        .or_else(|| {
            line.strip_prefix("Session started:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

pub fn find_session_id(value: &Value) -> Option<String> {
    string_field_deep(
        value,
        &[
            "session_id",
            "sessionId",
            "sessionID",
            "thread_id",
            "threadId",
            "conversation_id",
        ],
    )
    .map(str::to_string)
    .or_else(|| {
        let event = string_field(value, "type")?;
        if event == "thread.started" {
            string_field_deep(value, &["id"]).map(str::to_string)
        } else {
            None
        }
    })
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn string_field_deep<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(text) = map.get(*key).and_then(Value::as_str) {
                    return Some(text);
                }
            }
            map.values()
                .find_map(|child| string_field_deep(child, keys))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| string_field_deep(child, keys)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_codex_thread_session_and_resume_command() {
        let stdout = vec![r#"{"type":"thread.started","thread_id":"codex-thread-1"}"#.to_string()];

        let captured = capture_session_from_lines("codex", &stdout, &[]);

        assert_eq!(captured.session_id.as_deref(), Some("codex-thread-1"));
        assert_eq!(
            captured.resume_command.as_deref(),
            Some("codex resume codex-thread-1")
        );
    }

    #[test]
    fn captures_claude_session_and_resume_command() {
        let stdout = vec![r#"{"type":"system","session_id":"claude-session-1"}"#.to_string()];

        let captured = capture_session_from_lines("/usr/local/bin/claude", &stdout, &[]);

        assert_eq!(captured.session_id.as_deref(), Some("claude-session-1"));
        assert_eq!(
            captured.resume_command.as_deref(),
            Some("/usr/local/bin/claude --resume claude-session-1")
        );
    }

    #[test]
    fn keeps_session_without_resume_for_unrecognized_command() {
        let stdout = vec![r#"{"sessionId":"session-1"}"#.to_string()];

        let captured = capture_session_from_lines("/bin/sh", &stdout, &[]);

        assert_eq!(captured.session_id.as_deref(), Some("session-1"));
        assert_eq!(captured.resume_command, None);
    }
}
