//! Anthropic Messages API: `POST {base}/v1/messages` with `stream: true`.
use super::{error_code, sse::Event, ApiCall, Step};
use serde_json::{json, Value};

/// Messages requires an explicit output budget; this fits every current model.
const MAX_OUTPUT_TOKENS: u32 = 8192;

pub(super) fn endpoint(base_url: &str) -> String {
    format!("{base_url}/v1/messages")
}

pub(super) fn body(call: &ApiCall) -> Value {
    let mut body = json!({
        "model": call.model,
        "max_tokens": MAX_OUTPUT_TOKENS,
        "stream": true,
        "messages": [{ "role": "user", "content": call.prompt }],
    });
    if let Some(system) = &call.system {
        body["system"] = system.as_str().into();
    }
    body
}

pub(super) fn steps(event: &Event) -> Vec<Step> {
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return vec![Step::Invalid];
    };
    match value["type"].as_str().or(event.name.as_deref()) {
        Some("content_block_delta") if value["delta"]["type"] == "text_delta" => value["delta"]
            ["text"]
            .as_str()
            .map(|text| vec![Step::Text(text.to_owned())])
            .unwrap_or_default(),
        Some("message_delta") if value["delta"]["stop_reason"] == "max_tokens" => {
            vec![Step::Truncated]
        }
        Some("message_stop") => vec![Step::Done],
        Some("error") => vec![Step::Failed(error_code(&value["error"]["type"]))],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(data: &str) -> Event {
        Event {
            name: None,
            data: data.into(),
        }
    }

    #[test]
    fn text_deltas_truncation_and_errors_are_recognised() {
        assert_eq!(
            steps(&event(
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#
            )),
            vec![Step::Text("Hi".into())]
        );
        assert!(steps(&event(
            r#"{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"x"}}"#
        ))
        .is_empty());
        assert_eq!(
            steps(&event(
                r#"{"type":"message_delta","delta":{"stop_reason":"max_tokens"}}"#
            )),
            vec![Step::Truncated]
        );
        assert_eq!(
            steps(&event(r#"{"type":"message_stop"}"#)),
            vec![Step::Done]
        );
        assert_eq!(
            steps(&event(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"echo"}}"#
            )),
            vec![Step::Failed(Some("overloaded_error".into()))]
        );
    }
}
