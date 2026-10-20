use super::{summarize_json, ChatMessagePart, DiffStats, ToolMetadata};
use serde_json::Value;

fn text(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(key)?.as_str().map(str::to_string))
}

fn kind(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "read" | "read_file" => "read",
        "edit" | "apply_patch" | "file_change" | "str_replace_editor" => "edit",
        "write" | "write_file" => "write",
        "bash" | "command" | "command_execution" | "exec_command" | "shell" => "bash",
        "grep" | "glob" | "search" | "toolsearch" => "search",
        "webfetch" | "websearch" | "web_search" => "web",
        _ => "other",
    }
}

fn bounded_preview(lines: impl Iterator<Item = String>) -> String {
    let mut result = String::new();
    for line in lines.take(40) {
        let remaining = 8192usize.saturating_sub(result.len() + 1);
        if remaining == 0 {
            break;
        }
        let mut end = line.len().min(remaining);
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        result.push_str(&line[..end]);
        result.push('\n');
    }
    result
}

fn metadata(name: &str, input: Option<&Value>, call_id: Option<String>) -> ToolMetadata {
    let mut details = ToolMetadata {
        call_id,
        kind: Some(kind(name).into()),
        ..Default::default()
    };
    if let Some(input) = input {
        details.target = text(
            input,
            &["file_path", "filePath", "path", "command", "pattern", "url"],
        )
        .map(|value| value.chars().take(1024).collect());
        let old = text(input, &["old_string", "oldString", "oldText", "old"]);
        let new = text(
            input,
            &["new_string", "newString", "newText", "new", "content"],
        );
        if let Some(new) = new.filter(|_| matches!(kind(name), "edit" | "write")) {
            let old = old.unwrap_or_default();
            details.diff_stats = Some(DiffStats {
                added: new.lines().count(),
                removed: old.lines().count(),
            });
            details.diff_preview = Some(bounded_preview(
                old.lines()
                    .map(|line| format!("-{line}"))
                    .chain(new.lines().map(|line| format!("+{line}"))),
            ));
        }
    }
    details
}

pub(super) fn content_part(item: &Value) -> Option<ChatMessagePart> {
    match item.get("type")?.as_str()? {
        "tool_use" | "tool_call" | "function_call" => {
            let name = text(item, &["name"])
                .or_else(|| item.pointer("/function/name")?.as_str().map(str::to_string))
                .unwrap_or_else(|| "tool".into());
            let input = item
                .get("input")
                .or_else(|| item.get("arguments"))
                .or_else(|| item.pointer("/function/arguments"));
            Some(ChatMessagePart::Tool {
                metadata: metadata(&name, input, text(item, &["id", "call_id"])),
                name,
                input_summary: input
                    .filter(|value| value.as_object().is_none_or(|object| !object.is_empty()))
                    .and_then(|value| summarize_json(value, 1024)),
                output_summary: None,
                status: Some("running".into()),
            })
        }
        "tool_result" => {
            let call_id = text(item, &["tool_use_id", "call_id"]);
            Some(ChatMessagePart::Tool {
                name: text(item, &["name"])
                    .or_else(|| call_id.clone())
                    .unwrap_or_else(|| "tool_result".into()),
                metadata: ToolMetadata {
                    call_id,
                    ..Default::default()
                },
                input_summary: None,
                output_summary: item
                    .get("content")
                    .or_else(|| item.get("output"))
                    .or_else(|| item.get("result"))
                    .and_then(|value| summarize_json(value, 8192)),
                status: Some(
                    if item.get("is_error").and_then(Value::as_bool) == Some(true) {
                        "error"
                    } else {
                        "done"
                    }
                    .into(),
                ),
            })
        }
        _ => None,
    }
}

pub(super) fn event_part(value: &Value) -> Option<ChatMessagePart> {
    if let Some(block) = value.get("content_block") {
        return content_part(block);
    }
    if let Some(content) = value
        .pointer("/message/content")
        .or_else(|| value.get("content"))
        .and_then(Value::as_array)
    {
        return content.iter().find_map(content_part);
    }
    let event = value.get("type").and_then(Value::as_str).unwrap_or("");
    if event == "tool_use" && value.get("part").is_some() {
        let part = &value["part"];
        let state = &part["state"];
        let name = text(part, &["tool"]).unwrap_or_else(|| "tool".into());
        let mut details = metadata(&name, state.get("input"), text(part, &["callID", "id"]));
        details.exit_code = state
            .pointer("/metadata/exit")
            .and_then(Value::as_i64)
            .and_then(|code| i32::try_from(code).ok());
        return Some(ChatMessagePart::Tool {
            metadata: details,
            name,
            input_summary: state
                .get("input")
                .and_then(|input| summarize_json(input, 1024)),
            output_summary: state
                .get("output")
                .or_else(|| state.get("error"))
                .and_then(|output| summarize_json(output, 8192)),
            status: Some(
                match state["status"].as_str() {
                    Some("completed") => "done",
                    Some("error") => "error",
                    _ => "running",
                }
                .into(),
            ),
        });
    }
    let item = value
        .get("item")
        .or_else(|| value.get("msg"))
        .unwrap_or(value);
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or(event);
    if matches!(
        item_type,
        "command_execution"
            | "exec_command"
            | "shell"
            | "file_change"
            | "mcp_tool_call"
            | "web_search"
    ) || event.starts_with("exec_command")
    {
        let name = if matches!(item_type, "command_execution" | "exec_command" | "shell")
            || event.starts_with("exec_command")
        {
            "command"
        } else {
            item_type
        };
        let mut details = metadata(name, Some(item), text(item, &["id", "call_id"]));
        details.exit_code = item
            .get("exit_code")
            .and_then(Value::as_i64)
            .and_then(|code| i32::try_from(code).ok());
        if let Some(changes) = item.get("changes").and_then(Value::as_array) {
            details.target = Some(
                changes
                    .iter()
                    .filter_map(|change| change["path"].as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            let patches = changes
                .iter()
                .filter_map(|change| {
                    change
                        .get("diff")
                        .or_else(|| change.get("patch"))
                        .and_then(Value::as_str)
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !patches.is_empty() {
                details.diff_stats = Some(DiffStats {
                    added: patches
                        .lines()
                        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
                        .count(),
                    removed: patches
                        .lines()
                        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
                        .count(),
                });
                details.diff_preview = Some(bounded_preview(patches.lines().map(str::to_string)));
            }
        }
        let failed = details.exit_code.is_some_and(|code| code != 0) || item["status"] == "failed";
        let done =
            event.contains("completed") || event.ends_with("_end") || item["status"] == "completed";
        return Some(ChatMessagePart::Tool {
            metadata: details,
            name: name.into(),
            input_summary: text(item, &["command", "cmd", "query"]).or_else(|| {
                item.get("arguments")
                    .and_then(|value| summarize_json(value, 1024))
            }),
            output_summary: item
                .get("aggregated_output")
                .or_else(|| item.get("output"))
                .or_else(|| item.get("result"))
                .and_then(|value| summarize_json(value, 8192)),
            status: Some(
                if failed {
                    "error"
                } else if done {
                    "done"
                } else {
                    "running"
                }
                .into(),
            ),
        });
    }
    content_part(item)
}

/// Same call id updates one card in runtime, persisted projection and frontend.
pub(super) fn merge_part(parts: &mut Vec<ChatMessagePart>, incoming: ChatMessagePart) {
    let ChatMessagePart::Tool {
        metadata,
        name,
        input_summary,
        output_summary,
        status,
    } = &incoming
    else {
        parts.push(incoming);
        return;
    };
    if let Some(id) = &metadata.call_id {
        if let Some(ChatMessagePart::Tool { metadata: old, name: old_name, input_summary: old_input, output_summary: old_output, status: old_status }) =
            parts.iter_mut().find(|part| matches!(part, ChatMessagePart::Tool { metadata, .. } if metadata.call_id.as_ref() == Some(id)))
        {
            if name != id && name != "tool_result" && name != "tool" { *old_name = name.clone(); }
            if input_summary.is_some() { *old_input = input_summary.clone(); }
            if output_summary.is_some() { *old_output = output_summary.clone(); }
            if status.as_deref() != Some("running") || old_status.as_deref().is_none_or(|value| value == "running") { *old_status = status.clone(); }
            if metadata.kind.as_deref().is_some_and(|value| value != "other") { old.kind = metadata.kind.clone(); }
            if metadata.target.is_some() { old.target = metadata.target.clone(); }
            if metadata.exit_code.is_some() { old.exit_code = metadata.exit_code; }
            if metadata.diff_stats.is_some() { old.diff_stats = metadata.diff_stats.clone(); old.diff_preview = metadata.diff_preview.clone(); }
            return;
        }
    }
    parts.push(incoming);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn tool_result_pairs_with_call_and_preserves_bounded_diff() {
        let use_part = content_part(&json!({"type":"tool_use","id":"edit-1","name":"Edit","input":{"file_path":"file.txt","old_string":"old","new_string":"新".repeat(10000)}})).unwrap();
        let result =
            content_part(&json!({"type":"tool_result","tool_use_id":"edit-1","content":"done"}))
                .unwrap();
        let mut parts = vec![use_part];
        merge_part(&mut parts, result);
        assert_eq!(parts.len(), 1);
        let ChatMessagePart::Tool {
            name,
            metadata,
            status,
            ..
        } = &parts[0]
        else {
            panic!()
        };
        assert_eq!(name, "Edit");
        assert_eq!(status.as_deref(), Some("done"));
        assert_eq!(metadata.diff_stats.as_ref().unwrap().removed, 1);
        assert!(metadata.diff_preview.as_ref().unwrap().len() <= 8192);
    }
}

#[cfg(test)]
mod fixtures {
    use super::*;
    #[test]
    fn actual_claude_and_codex_frames_pair_without_duplicate_cards() {
        for (mode, input, expected) in [
            (
                "claude_stream_json",
                include_str!("../../tests/fixtures/cli-streams/claude-2.1.304/read-command.jsonl"),
                3,
            ),
            (
                "codex_json",
                include_str!("../../tests/fixtures/cli-streams/codex-0.163.0/read-command.jsonl"),
                3,
            ),
            (
                "opencode_json",
                include_str!("../../tests/fixtures/cli-streams/opencode-1.19.3/read.jsonl"),
                1,
            ),
        ] {
            let mut parts = Vec::new();
            for line in input.lines() {
                let line = crate::agents::stream::redact_stream_line(line);
                for part in super::super::parse_chat_stream_line(mode, &line).parts {
                    merge_part(&mut parts, part);
                }
            }
            assert_eq!(parts.len(), expected, "{mode}");
            for part in parts {
                let ChatMessagePart::Tool {
                    metadata, status, ..
                } = part
                else {
                    panic!("unexpected part")
                };
                assert!(metadata.call_id.is_some());
                assert_eq!(status.as_deref(), Some("done"));
            }
        }
    }
    #[test]
    fn codex_file_change_and_opencode_tool_frames_preserve_results() {
        let part = event_part(&serde_json::json!({"type":"item.completed","item":{"id":"patch-1","type":"file_change","changes":[{"path":"a.rs","diff":"--- a\n+++ b\n-old\n+new"}],"status":"completed"}})).unwrap();
        let ChatMessagePart::Tool { metadata, .. } = part else {
            panic!()
        };
        assert_eq!(
            metadata.diff_stats,
            Some(DiffStats {
                added: 1,
                removed: 1
            })
        );
        let part = event_part(&serde_json::json!({"type":"tool_use","part":{"callID":"open-1","tool":"bash","state":{"status":"completed","input":{"command":"pwd"},"output":"/project","metadata":{"exit":0}}}})).unwrap();
        let ChatMessagePart::Tool {
            metadata,
            status,
            output_summary,
            ..
        } = part
        else {
            panic!()
        };
        assert_eq!(metadata.exit_code, Some(0));
        assert_eq!(status.as_deref(), Some("done"));
        assert_eq!(output_summary.as_deref(), Some("/project"));
    }
}

#[cfg(test)]
mod unknown_events {
    #[test]
    fn unknown_protocol_frames_preserve_text_and_report_a_warning() {
        let parsed = super::super::parse_chat_stream_line(
            "opencode_json",
            r#"{"type":"future_notice","payload":"new content"}"#,
        );
        assert!(parsed.delta.contains("new content"));
        assert!(parsed.warning.as_deref().unwrap().contains("future_notice"));
    }
}
