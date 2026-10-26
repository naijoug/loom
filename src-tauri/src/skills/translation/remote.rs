use super::*;
use crate::skills::operations::HttpRequestRecord;
use serde_json::{json, Value};
use std::time::Duration;

fn endpoint(id: ProviderId) -> Result<&'static str> {
    match id {
        ProviderId::OpenAi => Ok("https://api.openai.com/v1/responses"),
        ProviderId::OpenRouter => Ok("https://openrouter.ai/api/v1/chat/completions"),
        _ => Err(SkillsError::invalid(
            "Expected a remote translation provider",
        )),
    }
}
fn body(provider: &ResolvedProvider, context: &Context, text: &str) -> Value {
    let system = instructions(&context.target_language);
    if provider.id == ProviderId::OpenAi {
        json!({"model":provider.model,"instructions":system,"input":text,"max_output_tokens":8000,"store":false,"tools":[]})
    } else {
        json!({"model":provider.model,"messages":[{"role":"system","content":system},{"role":"user","content":text}],"max_tokens":8000,"stream":false})
    }
}
fn parse(id: ProviderId, value: Value) -> Result<String> {
    let invalid =
        || SkillsError::invalid("Translation provider returned incomplete or invalid output");
    if !value.get("error").is_none_or(Value::is_null) {
        return Err(invalid());
    }
    let text = match id {
        ProviderId::OpenAi => {
            if value["status"] != "completed" {
                return Err(invalid());
            }
            let output = value["output"].as_array().ok_or_else(invalid)?;
            let mut texts = vec![];
            for item in output {
                match item["type"].as_str() {
                    Some("reasoning") => {}
                    Some("message")
                        if item["role"] == "assistant" && item["status"] == "completed" =>
                    {
                        for part in item["content"].as_array().ok_or_else(invalid)? {
                            if part["type"] != "output_text" {
                                return Err(invalid());
                            }
                            texts.push(part["text"].as_str().ok_or_else(invalid)?);
                        }
                    }
                    _ => return Err(invalid()),
                }
            }
            texts.join("\n")
        }
        ProviderId::OpenRouter => {
            let choices = value["choices"].as_array().ok_or_else(invalid)?;
            if choices.len() != 1
                || choices[0]["finish_reason"] != "stop"
                || choices[0]["message"]["role"] != "assistant"
                || choices[0]["message"]
                    .get("tool_calls")
                    .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
            {
                return Err(invalid());
            }
            choices[0]["message"]["content"]
                .as_str()
                .ok_or_else(invalid)?
                .into()
        }
        _ => return Err(invalid()),
    };
    validated_markdown(text)
}
pub(super) async fn translate(
    op: &mut Operation,
    provider: &ResolvedProvider,
    context: &Context,
    text: &str,
) -> Result<String> {
    let url = endpoint(provider.id)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| SkillsError::invalid("Cannot initialize translation HTTP client"))?;
    translate_with_client(op, provider, context, text, &client, url).await
}
// Only tests inject a loopback endpoint/client; production always uses the fixed
// HTTPS endpoint above, without redirects or frontend-supplied URLs.
async fn translate_with_client(
    op: &mut Operation,
    provider: &ResolvedProvider,
    context: &Context,
    text: &str,
    client: &reqwest::Client,
    url: &str,
) -> Result<String> {
    op.check_cancelled()?;
    let mut auth = reqwest::header::HeaderValue::from_str(&format!(
        "Bearer {}",
        provider.api_key.as_deref().ok_or_else(|| SkillsError::new(
            ErrorCode::AuthRequired,
            "Translation API key missing"
        ))?
    ))
    .map_err(|_| SkillsError::invalid("Translation key is not a valid HTTP credential"))?;
    auth.set_sensitive(true);
    let payload = serde_json::to_vec(&body(provider, context, text))
        .map_err(|_| SkillsError::invalid("Cannot encode translation request"))?;
    op.record.requests.push(HttpRequestRecord {
        method: "POST".into(),
        url: url.into(),
        started_at_ms: crate::models::now_ms(),
        finished_at_ms: None,
        status: None,
        error: None,
    });
    op.stage("requesting")?;
    let work = async {
        let mut response = client
            .post(url)
            .header(reqwest::header::AUTHORIZATION, auth)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(http_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Ok((status, Vec::new()));
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            if bytes.len() + chunk.len() > MAX_RESULT * 2 {
                return Err(SkillsError::invalid(
                    "Translation response exceeds size limit",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status, bytes))
    };
    tokio::pin!(work);
    let result = loop {
        tokio::select! {result=&mut work=>break result,_=tokio::time::sleep(Duration::from_millis(40))=>if let Err(error)=op.check_cancelled(){break Err(error);}}
    };
    let outcome = result.and_then(|(status, bytes)| {
        op.record.requests.last_mut().unwrap().status = Some(status);
        match status {
            200..=299 => parse(
                provider.id,
                serde_json::from_slice(&bytes).map_err(|_| {
                    SkillsError::invalid("Translation provider returned invalid JSON")
                })?,
            ),
            401 | 403 => Err(SkillsError::new(
                ErrorCode::AuthRequired,
                "Translation authentication failed; check provider configuration",
            )),
            429 => Err(SkillsError::new(
                ErrorCode::RateLimited,
                "Translation provider is rate limited; retry later",
            )),
            _ => Err(SkillsError::new(
                ErrorCode::SourceUnavailable,
                "Translation request failed; check provider availability",
            )),
        }
    });
    let request = op.record.requests.last_mut().unwrap();
    request.finished_at_ms = Some(crate::models::now_ms());
    request.error = outcome.as_ref().err().map(|e| e.message.clone());
    op.stage("response_received")?;
    outcome
}
fn http_error(error: reqwest::Error) -> SkillsError {
    if error.is_timeout() {
        SkillsError::new(
            ErrorCode::Timeout,
            "Translation request timed out; no automatic retry was sent",
        )
    } else {
        SkillsError::new(
            ErrorCode::SourceUnavailable,
            "Translation connection failed; no automatic retry was sent",
        )
    }
}
#[cfg(test)]
pub(super) fn fixture_parse(id: ProviderId, value: Value) -> Result<String> {
    parse(id, value)
}
#[cfg(test)]
pub(super) fn fixture_body(provider: &ResolvedProvider, context: &Context, text: &str) -> Value {
    body(provider, context, text)
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
