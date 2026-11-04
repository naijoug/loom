//! OpenAI Responses API: `POST {base}/responses` with `stream: true`. Nothing
//! is stored upstream (`store: false`).
use super::{error_code, sse::Event, ApiCall, Step};
use serde_json::{json, Value};

pub(super) fn endpoint(base_url: &str) -> String {
    format!("{base_url}/responses")
}

pub(super) fn body(call: &ApiCall) -> Value {
    let mut body = json!({
        "model": call.model,
        "input": call.prompt,
        "stream": true,
        "store": false,
    });
    if let Some(system) = &call.system {
        body["instructions"] = system.as_str().into();
    }
    body
}

pub(super) fn steps(event: &Event) -> Vec<Step> {
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return vec![Step::Invalid];
    };
    match value["type"].as_str().or(event.name.as_deref()) {
        Some("response.output_text.delta") => value["delta"]
            .as_str()
            .map(|text| vec![Step::Text(text.to_owned())])
            .unwrap_or_default(),
        Some("response.completed") => vec![Step::Done],
        Some("response.incomplete") => vec![Step::Truncated, Step::Done],
        Some("response.failed") => vec![Step::Failed(error_code(
            &value["response"]["error"]["code"],
        ))],
        Some("error") => vec![Step::Failed(error_code(&value["code"]))],
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
    fn text_completion_incomplete_and_failure_are_recognised() {
        assert_eq!(
            steps(&event(
                r#"{"type":"response.output_text.delta","delta":"Hi"}"#
            )),
            vec![Step::Text("Hi".into())]
        );
        assert_eq!(
            steps(&event(r#"{"type":"response.completed","response":{}}"#)),
            vec![Step::Done]
        );
        assert_eq!(
            steps(&event(
                r#"{"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"}}}"#
            )),
            vec![Step::Truncated, Step::Done]
        );
        assert_eq!(
            steps(&event(
                r#"{"type":"response.failed","response":{"error":{"code":"server_error","message":"echo"}}}"#
            )),
            vec![Step::Failed(Some("server_error".into()))]
        );
        assert_eq!(steps(&event("not json")), vec![Step::Invalid]);
    }
}
