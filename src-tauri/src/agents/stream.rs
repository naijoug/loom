//! Preserve protocol envelopes while redacting credentials from logs and projections.
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
