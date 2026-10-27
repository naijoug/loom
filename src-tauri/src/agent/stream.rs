//! Preserve protocol envelopes while redacting credentials from logs and projections.
use super::event::MessagePart;
use serde_json::Value;

pub(crate) fn redact_stream_line(line: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(line) else {
        return super::redact_sensitive_text(line);
    };
    fn redact(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields.iter_mut() {
                    let key = key.to_ascii_lowercase().replace('-', "_");
                    if matches!(
                        key.as_str(),
                        "api_key"
                            | "apikey"
                            | "access_token"
                            | "refresh_token"
                            | "auth_token"
                            | "authorization"
                            | "password"
                            | "passwd"
                            | "secret"
                            | "token"
                    ) && value.is_string()
                    {
                        *value = Value::String("[REDACTED]".into());
                    } else {
                        redact(value);
                    }
                }
            }
            Value::Array(items) => {
                for value in items {
                    redact(value);
                }
            }
            Value::String(text) => *text = super::redact_sensitive_text(text),
            _ => {}
        }
    }
    redact(&mut value);
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_counts_do_not_destroy_structured_frames() {
        let value: Value = serde_json::from_str(&redact_stream_line(r#"{"type":"result","usage":{"input_tokens":100},"result":"hello","api_key":"private"}"#)).unwrap();
        assert_eq!(value["result"], "hello");
        assert_eq!(value["usage"]["input_tokens"], 100);
        assert_eq!(value["api_key"], "[REDACTED]");
    }
}

/// Parse all tool blocks in a stdout frame; providers may batch parallel results.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedLine {
    pub(super) warning: Option<String>,
    pub(super) delta: String,
    pub(crate) parts: Vec<MessagePart>,
    /// The line opens a new assistant text block (a Codex message, a Claude text
    /// block or message, an OpenCode text part); the transcript separates blocks
    /// with a blank line instead of gluing them together.
    pub(super) block_start: bool,
}

pub(crate) fn parse_line(output_mode: &str, line: &str) -> ParsedLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedLine::default();
    }
    if output_mode == "plain" {
        return ParsedLine {
            delta: format!("{line}\n"),
            ..Default::default()
        };
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        if !trimmed.starts_with('{') {
            return ParsedLine {
                delta: trimmed.to_string(),
                ..Default::default()
            };
        }
        return ParsedLine::default();
    };
    let reported_failure = (value["type"] == "result" && value["is_error"] == true)
        || value["type"] == "turn.failed"
        || (output_mode == "opencode_json" && value["type"] == "error");
    if reported_failure {
        let message = value
            .pointer("/error/data/message")
            .or_else(|| value.pointer("/error/message"))
            .or_else(|| value.get("result"))
            .or_else(|| value.get("errors"))
            .or_else(|| value.get("error"))
            .and_then(|value| summarize_json(value, 2000))
            .unwrap_or_else(|| "CLI reported an error".into());
        return ParsedLine {
            parts: vec![MessagePart::Error {
                message,
                code: Some("cli_reported_error".into()),
            }],
            ..Default::default()
        };
    }
    let event = unwrap_stream_event(&value);
    let parts: Vec<MessagePart> = if let Some(blocks) = event
        .pointer("/message/content")
        .or_else(|| event.get("content"))
        .and_then(|v| v.as_array())
    {
        blocks
            .iter()
            .filter_map(tool_part_from_content_item)
            .collect()
    } else {
        extract_tool_or_command_part(&value).into_iter().collect()
    };
    let mut delta = extract_assistant_text(output_mode, trimmed);
    let kind = value
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let known = matches!(
        kind,
        "system"
            | "telemetry"
            | "assistant"
            | "user"
            | "result"
            | "stream_event"
            | "rate_limit_event"
            | "error"
            | "text"
            | "step_start"
            | "step_finish"
            | "tool_use"
            | "tool_result"
            | "tool_call"
            | "function_call"
            | "content_block_start"
            | "content_block_delta"
            | "content_block_stop"
            | "message_start"
            | "message_delta"
            | "message_stop"
            | "ping"
    ) || kind.starts_with("thread.")
        || kind.starts_with("turn.")
        || kind.starts_with("item.");
    let warning = if !known && parts.is_empty() {
        if delta.is_empty() {
            delta = summarize_json(&value, 2000).unwrap_or_default();
        }
        Some(format!(
            "Unknown CLI event '{}' preserved as text",
            kind.chars().take(120).collect::<String>()
        ))
    } else {
        None
    };
    let block_start = match output_mode {
        "codex_json" => {
            value.pointer("/item/type").and_then(|v| v.as_str()) == Some("agent_message")
        }
        "opencode_json" => kind == "text",
        _ => {
            kind == "assistant"
                || (event["type"] == "content_block_start"
                    && event
                        .pointer("/content_block/type")
                        .and_then(|v| v.as_str())
                        == Some("text"))
        }
    };
    ParsedLine {
        warning,
        delta,
        parts,
        block_start,
    }
}

/// Text needed before a new block so it starts a fresh Markdown paragraph.
pub(super) fn block_separator(content: &str) -> &'static str {
    if content.is_empty() || content.ends_with("\n\n") {
        ""
    } else if content.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    }
}

fn json_event_type(value: &serde_json::Value) -> Option<String> {
    value
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| {
            value
                .get("msg")
                .and_then(|msg| msg.get("type"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            value
                .get("event")
                .and_then(|event| event.get("type"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| value.get("event").and_then(|v| v.as_str()))
        .map(str::to_string)
}

pub(super) fn summarize_json(value: &serde_json::Value, max_chars: usize) -> Option<String> {
    let raw = match value {
        serde_json::Value::String(text) => text.clone(),
        other => serde_json::to_string(other).ok()?,
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= max_chars {
        Some(trimmed.to_string())
    } else {
        Some(format!(
            "{}…",
            trimmed.chars().take(max_chars).collect::<String>()
        ))
    }
}

fn unwrap_stream_event(value: &serde_json::Value) -> &serde_json::Value {
    // Claude Code `--output-format stream-json --include-partial-messages` wraps
    // content_block_* payloads as `{ "type":"stream_event", "event": { ... } }`.
    if json_event_type(value).as_deref() == Some("stream_event") {
        value.get("event").unwrap_or(value)
    } else {
        value
    }
}

fn tool_part_from_content_item(item: &serde_json::Value) -> Option<MessagePart> {
    super::tools::content_part(item)
}

fn extract_tool_or_command_part(value: &serde_json::Value) -> Option<MessagePart> {
    super::tools::event_part(unwrap_stream_event(value))
}

fn extract_assistant_text(output_mode: &str, raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if output_mode == "plain" {
        return trimmed.to_string();
    }
    // streaming_json / claude_stream_json / codex_json share best-effort text extraction
    // Best-effort: collect text-ish fields from JSONL / JSON event streams.
    let mut chunks = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            let value = unwrap_stream_event(&value);
            if output_mode == "opencode_json" {
                if value["type"] == "text" {
                    if let Some(text) = value.pointer("/part/text").and_then(|v| v.as_str()) {
                        chunks.push(text.into());
                    }
                }
                continue;
            }
            // Codex exec emits assistant text inside an agent_message item.
            if value.pointer("/item/type").and_then(|v| v.as_str()) == Some("agent_message") {
                if let Some(text) = value.pointer("/item/text").and_then(|v| v.as_str()) {
                    chunks.push(text.to_string());
                    continue;
                }
            }
            // Claude assistant message: collect every text block in content[].
            if let Some(parts) = value
                .pointer("/message/content")
                .or_else(|| value.pointer("/content"))
                .and_then(|v| v.as_array())
            {
                let mut block_text = Vec::new();
                for item in parts {
                    if item.get("type").and_then(|v| v.as_str()) == Some("text") {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                block_text.push(text.to_string());
                            }
                        }
                    }
                }
                if !block_text.is_empty() {
                    chunks.push(block_text.join("\n\n"));
                    continue;
                }
            }
            if let Some(text) = value
                .pointer("/msg/text")
                .or_else(|| value.pointer("/message/content/0/text"))
                .or_else(|| value.get("text"))
                .and_then(|v| v.as_str())
            {
                chunks.push(text.to_string());
                continue;
            }
            // Claude content_block_delta / stream_event text deltas.
            if let Some(delta) = value
                .pointer("/delta/text")
                .or_else(|| value.pointer("/delta"))
                .or_else(|| value.pointer("/message/delta"))
                .and_then(|v| v.as_str())
            {
                chunks.push(delta.to_string());
                continue;
            }
        }
        // Fallback: keep non-JSON lines
        if !line.starts_with('{') {
            chunks.push(line.to_string());
        }
    }
    chunks.join("")
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parses_grok_style_tool_use_part() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls -la"}}]}}"#;
        let parsed = parse_line("streaming_json", line);
        match parsed.parts.into_iter().next() {
            Some(MessagePart::Tool {
                name,
                input_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "Bash");
                assert!(input_summary.unwrap_or_default().contains("ls -la"));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected tool part, got {other:?}"),
        }
    }

    #[test]
    fn parses_codex_style_command_part() {
        let line = r#"{"type":"item.completed","item":{"type":"command_execution","command":"pwd","output":"/repo"}}"#;
        let value = serde_json::from_str(line).unwrap();
        let part = extract_tool_or_command_part(&value).expect("command part");
        match part {
            MessagePart::Tool {
                name,
                input_summary,
                output_summary,
                ..
            } => {
                assert_eq!(name, "command");
                assert_eq!(input_summary.as_deref(), Some("pwd"));
                assert_eq!(output_summary.as_deref(), Some("/repo"));
            }
            other => panic!("expected tool/command part, got {other:?}"),
        }
    }

    #[test]
    fn text_blocks_are_marked_so_the_runtime_can_separate_them() {
        let codex = parse_line(
            "codex_json",
            r#"{"type":"item.completed","item":{"id":"i1","type":"agent_message","text":"第二段"}}"#,
        );
        assert_eq!(codex.delta, "第二段");
        assert!(codex.block_start);
        let start = parse_line(
            "claude_stream_json",
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        );
        assert!(start.block_start && start.delta.is_empty());
        let delta = parse_line(
            "claude_stream_json",
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}}}"#,
        );
        assert!(!delta.block_start);
        let tool = parse_line(
            "claude_stream_json",
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t","name":"Read","input":{}}}}"#,
        );
        assert!(!tool.block_start);
        let message = parse_line(
            "claude_stream_json",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"一"},{"type":"tool_use","id":"t","name":"Read","input":{}},{"type":"text","text":"二"}]}}"#,
        );
        assert!(message.block_start);
        assert_eq!(message.delta, "一\n\n二");
        assert_eq!(super::block_separator(""), "");
        assert_eq!(super::block_separator("a"), "\n\n");
        assert_eq!(super::block_separator("a\n"), "\n");
        assert_eq!(super::block_separator("a\n\n"), "");
    }

    #[test]
    fn parse_falls_back_to_text_for_non_json() {
        let parsed = parse_line("streaming_json", "hello plain");
        assert_eq!(parsed.delta, "hello plain");
        assert!(parsed.parts.is_empty());
    }

    #[test]
    fn parses_claude_assistant_tool_use_part() {
        let line = r#"{"type":"assistant","message":{"id":"msg_1","role":"assistant","content":[{"type":"text","text":"Looking"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"src/main.rs"}}]}}"#;
        let parsed = parse_line("claude_stream_json", line);
        assert!(parsed.delta.contains("Looking"), "delta={}", parsed.delta);
        match parsed.parts.into_iter().next() {
            Some(MessagePart::Tool {
                name,
                input_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "Read");
                assert!(input_summary.unwrap_or_default().contains("main.rs"));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected tool part, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_stream_event_content_block_tool_use() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_2","name":"Bash","input":{"command":"ls -la"}}}}"#;
        let parsed = parse_line("claude_stream_json", line);
        match parsed.parts.into_iter().next() {
            Some(MessagePart::Tool {
                name,
                input_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "Bash");
                assert!(input_summary.unwrap_or_default().contains("ls -la"));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected tool part, got {other:?}"),
        }
    }

    #[test]
    fn parses_claude_tool_result_part() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"fn main() {}"}]}}"#;
        let parsed = parse_line("claude_stream_json", line);
        match parsed.parts.into_iter().next() {
            Some(MessagePart::Tool {
                name,
                output_summary,
                status,
                ..
            }) => {
                assert_eq!(name, "toolu_1");
                assert_eq!(output_summary.as_deref(), Some("fn main() {}"));
                assert_eq!(status.as_deref(), Some("done"));
            }
            other => panic!("expected tool_result part, got {other:?}"),
        }
    }
}
