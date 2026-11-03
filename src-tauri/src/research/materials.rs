//! Model-visible material exchange. This service can only read published files.
use super::{target_models::*, target_store::TargetStore};
use serde::{Deserialize, Serialize};

pub const CONTEXT_CHARS: usize = 48_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextRef {
    pub allocation_key: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Context {
    pub run_id: String,
    pub scope_digest: String,
    manifest: String,
    pages: Vec<String>,
    omitted_pages: u32,
}
impl Context {
    pub fn new(run_id: &str, scope_digest: String, manifest: String) -> Result<Self, String> {
        if manifest.chars().count() > 20_000 {
            return Err("material_manifest_limit".into());
        }
        Ok(Self {
            run_id: run_id.into(),
            scope_digest,
            manifest,
            pages: Vec::new(),
            omitted_pages: 0,
        })
    }
    fn text(&self) -> String {
        format!("PUBLISHED MATERIAL MANIFEST\n{}\nRETAINED MATERIAL PAGES\n{}\nOlder omitted pages: {}. Omitted content is unknown in this input and may be re-requested within the remaining budget.", self.manifest,self.pages.join("\nMATERIAL PAGE\n"),self.omitted_pages)
    }
    pub fn render(&self) -> Result<String, String> {
        let text = self.text();
        if self.pages.len() > 4 || text.chars().count() > CONTEXT_CHARS {
            return Err("material_context_limit".into());
        }
        Ok(text)
    }
    pub fn push(&mut self, page: String) -> Result<(), String> {
        let mut next = self.clone();
        next.pages.push(page);
        while next.pages.len() > 1
            && (next.pages.len() > 4 || next.text().chars().count() > CONTEXT_CHARS)
        {
            next.pages.remove(0);
            next.omitted_pages = next.omitted_pages.saturating_add(1);
        }
        next.render()?;
        *self = next;
        Ok(())
    }
}

pub fn scope_digest(run: &RunView) -> Result<String, String> {
    Ok(super::target_store::digest(
        &serde_json::to_vec(&(
            &run.spec.run_id,
            &run.spec.config_digest,
            &run.state.snapshot_ids,
            &run.state.evidence_ids,
        ))
        .map_err(|e| e.to_string())?,
    ))
}
pub fn load_context(store: &TargetStore, run_id: &str) -> Result<Option<Context>, String> {
    let run = store.run(run_id)?;
    let Some(reference) = &run.state.material_context else {
        return Ok(None);
    };
    if !super::store::valid_id(&reference.allocation_key) {
        return Err("invalid_material_context_reference".into());
    }
    let path = super::files::path(
        &store.root()?,
        &format!(
            "runs/{run_id}/steps/{}/context.json",
            reference.allocation_key
        ),
    )?;
    let bytes = super::files::read_bytes(&path, 1024 * 1024)?;
    if super::target_store::digest(&bytes) != reference.sha256 {
        return Err("material_context_integrity_mismatch".into());
    }
    let context: Context =
        serde_json::from_slice(&bytes).map_err(|_| "invalid_material_context")?;
    if context.run_id != run_id || context.scope_digest != scope_digest(&run)? {
        return Err("material_context_scope_mismatch".into());
    }
    context.render()?;
    Ok(Some(context))
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Envelope {
    Answer { markdown: String },
    NeedMaterial { requests: Vec<MaterialRequest> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialResponse {
    pub request: MaterialRequest,
    pub material: Material,
}

pub fn parse(text: &str) -> Result<Envelope, String> {
    if text.len() > 1024 * 1024 {
        return Err("model_envelope_limit".into());
    }
    let envelope: Envelope =
        serde_json::from_str(text.trim()).map_err(|_| "invalid_model_envelope")?;
    match &envelope {
        Envelope::Answer { markdown } if markdown.trim().is_empty() => {
            return Err("empty_model_answer".into())
        }
        Envelope::NeedMaterial { requests } if requests.is_empty() || requests.len() > 4 => {
            return Err("material_request_limit".into())
        }
        _ => {}
    }
    Ok(envelope)
}

/// Count serialized text (including escaping and provenance), not only file text.
/// Fail atomically instead of silently dropping requests or changing line ranges.
pub fn read_batch(
    store: &TargetStore,
    run_id: &str,
    requests: &[MaterialRequest],
) -> Result<String, String> {
    if requests.is_empty() || requests.len() > 4 {
        return Err("material_request_limit".into());
    }
    let responses = requests
        .iter()
        .map(|request| {
            let mut material = store.material(run_id, request)?;
            material.text = crate::agent::redact_sensitive_text(&material.text);
            Ok(MaterialResponse {
                request: request.clone(),
                material,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let text = serde_json::to_string(&responses).map_err(|e| e.to_string())?;
    if text.chars().count() > CONTEXT_CHARS {
        return Err("material_context_limit".into());
    }
    Ok(text)
}

/// Persist this with run step metadata. Recovery must not reset exchanges.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeBudget {
    pub calls: u32,
    pub reads: u32,
    pub corrections: u32,
}
impl ExchangeBudget {
    pub fn before_call(&mut self) -> Result<(), String> {
        if self.calls >= 5 {
            return Err("report_call_budget_exhausted".into());
        }
        self.calls += 1;
        Ok(())
    }
    pub fn before_read(&mut self) -> Result<(), String> {
        if self.reads >= 3 {
            return Err("material_exchange_budget_exhausted".into());
        }
        self.reads += 1;
        Ok(())
    }
    pub fn before_correction(&mut self) -> Result<(), String> {
        if self.corrections >= 1 {
            return Err("model_correction_budget_exhausted".into());
        }
        self.corrections += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelope_cannot_smuggle_actions_or_unbounded_requests() {
        assert!(parse(r#"{"kind":"answer","markdown":"Fact"}"#).is_ok());
        for invalid in [
            r#"{"kind":"answer","markdown":"Fact","command":"sh"}"#,
            r#"{"kind":"need_material","requests":[]}"#,
            r#"{"kind":"execute","command":"sh"}"#,
            r#"{"kind":"need_material","requests":[{"kind":"evidence","evidenceId":"e","offset":0,"limit":4,"path":"/private"}]}"#,
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn report_exchange_limits_survive_serialization() {
        let mut budget = ExchangeBudget::default();
        budget.before_call().unwrap();
        for _ in 0..3 {
            budget.before_read().unwrap();
            budget.before_call().unwrap();
        }
        budget.before_correction().unwrap();
        budget.before_call().unwrap();
        let mut recovered: ExchangeBudget =
            serde_json::from_str(&serde_json::to_string(&budget).unwrap()).unwrap();
        assert!(recovered.before_call().is_err());
        assert!(recovered.before_read().is_err());
        assert!(recovered.before_correction().is_err());
    }
    #[test]
    fn context_retains_prior_reads_and_evicts_whole_pages_with_notice() {
        let mut context =
            Context::new("run-a", "scope".into(), "immutable manifest".into()).unwrap();
        context.push(format!("FIRST{}", "a".repeat(9000))).unwrap();
        context.push(format!("SECOND{}", "b".repeat(9000))).unwrap();
        assert!(context.render().unwrap().contains("FIRST"));
        context.push(format!("THIRD{}", "c".repeat(32000))).unwrap();
        let text = context.render().unwrap();
        assert!(
            text.contains("SECOND")
                && text.contains("THIRD")
                && text.contains("immutable manifest")
        );
        assert!(!text.contains("FIRST"));
        assert!(text.contains("Older omitted pages: 1"));
        assert!(text.chars().count() <= CONTEXT_CHARS);
        let before = serde_json::to_string(&context).unwrap();
        assert!(context.push("文".repeat(48_000)).is_err());
        assert_eq!(serde_json::to_string(&context).unwrap(), before);
    }
}
