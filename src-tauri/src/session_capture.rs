use serde_json::Value;
use std::path::Path;

pub const ADAPTER_CODEX: &str = "codex_cli";
pub const ADAPTER_CLAUDE_CODE: &str = "claude_code_cli";

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

pub fn resume_command_for_adapter(
    adapter_type: &str,
    command: &str,
    session_id: &str,
) -> Option<String> {
    match adapter_type {
        ADAPTER_CLAUDE_CODE => Some(format!("{command} --resume {session_id}")),
        ADAPTER_CODEX => Some(format!("{command} resume {session_id}")),
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

pub fn display_log_lines_for_command(command: &str, line: &str) -> Vec<String> {
    let Some(adapter_type) = adapter_type_for_command(command) else {
        return vec![line.to_string()];
    };
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return vec![line.to_string()];
    };

    match adapter_type {
        ADAPTER_CODEX => codex_log_lines(&value),
        ADAPTER_CLAUDE_CODE => claude_log_lines(&value),
        _ => vec![line.to_string()],
    }
}

pub fn find_session_id(value: &Value) -> Option<String> {
    string_field_deep(
        value,
        &[
            "session_id",
            "sessionId",
            "thread_id",
            "threadId",
            "conversation_id",
        ],
    )
    .map(str::to_string)
    .or_else(|| {
        let event = event_type(value)?;
        if event == "thread.started" {
            string_field_deep(value, &["id"]).map(str::to_string)
        } else {
            None
        }
    })
}

fn event_type(value: &Value) -> Option<String> {
    string_field(value, "type")
        .or_else(|| value.get("msg").and_then(|msg| string_field(msg, "type")))
        .or_else(|| {
            value
                .get("event")
                .and_then(|event| string_field(event, "type"))
        })
        .or_else(|| string_field(value, "event"))
        .map(str::to_string)
}

fn claude_log_lines(value: &Value) -> Vec<String> {
    match event_type(value).as_deref() {
        Some("system") => find_session_id(value)
            .map(|id| vec![format!("Session started: {id}")])
            .unwrap_or_default(),
        Some("assistant") => assistant_text_lines(value),
        Some("stream_event") => string_field_deep(value, &["text"])
            .map(split_log_text)
            .unwrap_or_default(),
        Some("result") => {
            if bool_field(value, "is_error") == Some(true) {
                string_field(value, "result")
                    .map(split_log_text)
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        Some(kind) => vec![format!("[{kind}]")],
        None => Vec::new(),
    }
}

fn codex_log_lines(value: &Value) -> Vec<String> {
    let event = event_type(value).unwrap_or_default();
    if event == "thread.started" {
        return find_session_id(value)
            .map(|id| vec![format!("Session started: {id}")])
            .unwrap_or_default();
    }
    if is_agent_message_event(&event, value) {
        return assistant_text_lines(value);
    }
    if event.contains("exec_command") || event.contains("command") {
        if let Some(command) = string_field_deep(value, &["command", "cmd"]) {
            return vec![format!("$ {command}")];
        }
        if let Some(output) = string_field_deep(value, &["output", "text", "delta"]) {
            return split_log_text(output);
        }
    }
    if event.is_empty() {
        Vec::new()
    } else {
        vec![format!("[{event}]")]
    }
}

fn is_agent_message_event(event: &str, value: &Value) -> bool {
    event.contains("agent_message")
        || event.contains("assistant")
        || contains_type_value(value, &["agent_message", "assistant"])
        || string_field_deep(value, &["role"]) == Some("assistant")
}

fn contains_type_value(value: &Value, expected: &[&str]) -> bool {
    match value {
        Value::Object(map) => {
            map.get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| expected.contains(&value))
                || map
                    .values()
                    .any(|child| contains_type_value(child, expected))
        }
        Value::Array(items) => items
            .iter()
            .any(|child| contains_type_value(child, expected)),
        _ => false,
    }
}

fn assistant_text_lines(value: &Value) -> Vec<String> {
    assistant_text_chunks(value)
        .into_iter()
        .flat_map(|text| split_log_text(&text))
        .collect()
}

fn assistant_text_chunks(value: &Value) -> Vec<String> {
    let mut output = Vec::new();
    collect_text_values(value, &mut output);
    output
        .into_iter()
        .filter(|text| !text.trim().is_empty())
        .collect()
}

fn collect_text_values(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if matches!(
                    key.as_str(),
                    "text" | "message" | "content" | "delta" | "result"
                ) {
                    if let Some(text) = child.as_str() {
                        if !text.trim().is_empty() {
                            output.push(text.to_string());
                        }
                    }
                }
                collect_text_values(child, output);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_text_values(item, output);
            }
        }
        _ => {}
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
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

fn split_log_text(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect()
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

    #[test]
    fn renders_structured_codex_log_lines_for_display() {
        let lines = display_log_lines_for_command(
            "codex",
            r#"{"type":"agent_message","message":"Implemented the fix."}"#,
        );

        assert_eq!(lines, vec!["Implemented the fix."]);
    }

    #[test]
    fn renders_structured_claude_log_lines_for_display() {
        let lines = display_log_lines_for_command(
            "claude",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Checking tests"}]}}"#,
        );

        assert_eq!(lines, vec!["Checking tests"]);
    }
}
