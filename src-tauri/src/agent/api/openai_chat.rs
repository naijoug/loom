//! OpenAI Chat Completions: `POST {base}/chat/completions` with `stream: true`,
//! the protocol most OpenAI-compatible gateways speak.
use super::{error_code, sse::Event, ApiCall, Step};
use serde_json::{json, Value};

pub(super) fn endpoint(base_url: &str) -> String {
    format!("{base_url}/chat/completions")
}

pub(super) fn body(call: &ApiCall) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = &call.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.push(json!({ "role": "user", "content": call.prompt }));
    json!({ "model": call.model, "stream": true, "messages": messages })
}

/// A finish reason ends the answer; gateways differ on whether `[DONE]` follows.
pub(super) fn steps(event: &Event) -> Vec<Step> {
    if event.data.trim() == "[DONE]" {
        return vec![Step::Done];
    }
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return vec![Step::Invalid];
    };
    if value["error"].is_object() {
        let error = &value["error"];
        let code = error_code(&error["code"]).or_else(|| error_code(&error["type"]));
        return vec![Step::Failed(code)];
    }
    let choice = &value["choices"][0];
    let mut steps = Vec::new();
    if let Some(text) = choice["delta"]["content"]
        .as_str()
        .filter(|t| !t.is_empty())
    {
        steps.push(Step::Text(text.to_owned()));
    }
    match choice["finish_reason"].as_str() {
        Some("length") => steps.extend([Step::Truncated, Step::Done]),
        Some(_) => steps.push(Step::Done),
        None => {}
    }
    steps
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
    fn deltas_finish_reasons_and_errors_are_recognised() {
        assert_eq!(
            steps(&event(
                r#"{"choices":[{"index":0,"delta":{"role":"assistant","content":"Hi"},"finish_reason":null}]}"#
            )),
            vec![Step::Text("Hi".into())]
        );
        assert_eq!(
            steps(&event(
                r#"{"choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}"#
            )),
            vec![Step::Text("!".into()), Step::Done]
        );
        assert_eq!(
            steps(&event(
                r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#
            )),
            vec![Step::Truncated, Step::Done]
        );
        assert_eq!(steps(&event("[DONE]")), vec![Step::Done]);
        assert_eq!(
            steps(&event(
                r#"{"error":{"message":"echo","type":"invalid_request_error"}}"#
            )),
            vec![Step::Failed(Some("invalid_request_error".into()))]
        );
    }
}
