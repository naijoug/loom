//! Limited continuous research. Single-shot runs never enter this module.
//! A next action may only name a frozen target action and already published ids.
use super::{files, store::valid_id, target_models::*, target_store::TargetStore};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};

pub const NO_PROGRESS_LIMIT: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Support,
    Refute,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hypothesis {
    pub id: String,
    pub claim: String,
    pub verdict: Verdict,
    pub evidence_ids: Vec<String>,
    pub round: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AllowedAction {
    InspectApp,
    AnalyzeJs,
    CapturePage,
    InspectWebScripts,
    ReadEvidence { evidence_id: String },
    Finish { markdown: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopState {
    pub stagnant_rounds: u32,
    pub identities: BTreeSet<String>,
    pub hypotheses: Vec<Hypothesis>,
}
impl Default for LoopState {
    fn default() -> Self {
        Self {
            stagnant_rounds: 0,
            identities: BTreeSet::new(),
            hypotheses: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bounds<'a> {
    pub target: &'a Target,
    pub rounds: u32,
    pub max_rounds: u32,
    pub evidence_ids: &'a [String],
    pub snapshot_ids: &'a [String],
}

#[derive(Clone, Debug)]
pub enum Step {
    Continue {
        action: AllowedAction,
        identity: String,
        no_progress: bool,
        hypotheses: Vec<Hypothesis>,
    },
    Finish {
        markdown: String,
        hypotheses: Vec<Hypothesis>,
    },
    Stop {
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavedLoop {
    stagnant_rounds: u32,
    identities: BTreeSet<String>,
    hypotheses: Vec<SavedHypothesis>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SavedHypothesis {
    id: String,
    claim: String,
    verdict: String,
    evidence_ids: Vec<String>,
    round: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    kind: String,
    action: String,
    #[serde(default)]
    evidence_id: Option<String>,
    #[serde(default)]
    markdown: Option<String>,
    hypotheses: Vec<IncomingHypothesis>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IncomingHypothesis {
    id: String,
    claim: String,
    verdict: String,
    evidence_ids: Vec<String>,
}

pub fn iteration_allowed() -> bool {
    if std::env::var("LOOM_RESEARCH_ITERATION").as_deref() != Ok("1") {
        return false;
    }
    #[cfg(test)]
    {
        return true;
    }
    #[cfg(not(test))]
    {
        cfg!(target_os = "macos") && std::env::var("LOOM_RESEARCH_TARGETS").as_deref() == Ok("1")
    }
}

/// Advertised to the UI. Linux tests can admit a loop without showing the switch.
pub fn iteration_advertised() -> bool {
    cfg!(target_os = "macos")
        && std::env::var("LOOM_RESEARCH_TARGETS").as_deref() == Ok("1")
        && std::env::var("LOOM_RESEARCH_ITERATION").as_deref() == Ok("1")
}

pub fn load(store: &TargetStore, run_id: &str) -> Result<LoopState, String> {
    let path = loop_path(store, run_id)?;
    if !path.exists() {
        return Ok(LoopState::default());
    }
    let saved: SavedLoop = files::read_json(&path, 1024 * 1024)?;
    Ok(LoopState {
        stagnant_rounds: saved.stagnant_rounds,
        identities: saved.identities,
        hypotheses: saved
            .hypotheses
            .into_iter()
            .map(|item| {
                Ok(Hypothesis {
                    id: item.id,
                    claim: item.claim,
                    verdict: parse_verdict(&item.verdict)?,
                    evidence_ids: item.evidence_ids,
                    round: item.round,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

pub fn save(store: &TargetStore, run_id: &str, state: &LoopState) -> Result<(), String> {
    let path = loop_path(store, run_id)?;
    if let Some(parent) = path.parent() {
        files::directory(parent)?;
    }
    let saved = SavedLoop {
        stagnant_rounds: state.stagnant_rounds,
        identities: state.identities.clone(),
        hypotheses: state
            .hypotheses
            .iter()
            .map(|item| SavedHypothesis {
                id: item.id.clone(),
                claim: item.claim.clone(),
                verdict: match item.verdict {
                    Verdict::Support => "support",
                    Verdict::Refute => "refute",
                    Verdict::Unknown => "unknown",
                }
                .into(),
                evidence_ids: item.evidence_ids.clone(),
                round: item.round,
            })
            .collect(),
    };
    files::write_json(&path, &saved)
}

pub fn decide(text: &str, bounds: &Bounds<'_>, loop_state: &LoopState) -> Result<Step, String> {
    let envelope: Envelope =
        serde_json::from_str(extract_json(text)?).map_err(|_| "next_action_invalid".to_string())?;
    if envelope.kind != "next_action" {
        return Err("next_action_invalid".into());
    }
    let hypotheses = hypotheses(&envelope.hypotheses, bounds.rounds, bounds.evidence_ids)?;
    let action = allowed_action(&envelope, bounds)?;
    if let AllowedAction::Finish { markdown } = action {
        return Ok(Step::Finish {
            markdown,
            hypotheses,
        });
    }
    if bounds.rounds.saturating_add(1) > bounds.max_rounds {
        return Ok(Step::Stop {
            reason: "budget_exhausted",
        });
    }
    let identity = action_identity(&action, bounds);
    // Re-reading a published id, or repeating the same snapshot or URL, is not new coverage.
    let adds_coverage = matches!(
        action,
        AllowedAction::InspectApp
            | AllowedAction::AnalyzeJs
            | AllowedAction::CapturePage
            | AllowedAction::InspectWebScripts
    ) && !loop_state.identities.contains(&identity);
    if !adds_coverage && loop_state.stagnant_rounds.saturating_add(1) >= NO_PROGRESS_LIMIT {
        return Ok(Step::Stop {
            reason: "no_progress",
        });
    }
    Ok(Step::Continue {
        action,
        identity,
        no_progress: !adds_coverage,
        hypotheses,
    })
}

pub fn apply_continue(
    state: &mut LoopState,
    identity: String,
    no_progress: bool,
    hypotheses: Vec<Hypothesis>,
) {
    if no_progress {
        state.stagnant_rounds = state.stagnant_rounds.saturating_add(1);
    } else {
        state.stagnant_rounds = 0;
    }
    state.identities.insert(identity);
    state.hypotheses.extend(hypotheses);
}

pub fn render_hypotheses(hypotheses: &[Hypothesis]) -> String {
    if hypotheses.is_empty() {
        return "No hypothesis was recorded. Unobserved behavior remains unknown.".into();
    }
    hypotheses
        .iter()
        .map(|item| {
            let verdict = match item.verdict {
                Verdict::Support => "support",
                Verdict::Refute => "refute",
                Verdict::Unknown => "unknown",
            };
            format!("- {verdict} `{}`: {}", item.id, item.claim)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn allowed_action(envelope: &Envelope, bounds: &Bounds<'_>) -> Result<AllowedAction, String> {
    match envelope.action.as_str() {
        "inspect_app"
            if matches!(bounds.target, Target::App { .. }) && !bounds.snapshot_ids.is_empty() =>
        {
            Ok(AllowedAction::InspectApp)
        }
        "analyze_js"
            if matches!(bounds.target, Target::App { .. }) && !bounds.snapshot_ids.is_empty() =>
        {
            Ok(AllowedAction::AnalyzeJs)
        }
        "capture_page" if matches!(bounds.target, Target::Website { .. }) => {
            Ok(AllowedAction::CapturePage)
        }
        "inspect_web_scripts" if matches!(bounds.target, Target::Website { .. }) => {
            Ok(AllowedAction::InspectWebScripts)
        }
        "read_evidence" => {
            let evidence_id = envelope
                .evidence_id
                .as_deref()
                .filter(|id| bounds.evidence_ids.iter().any(|known| known == id))
                .ok_or("next_action_unsupported")?
                .to_string();
            Ok(AllowedAction::ReadEvidence { evidence_id })
        }
        "finish" => {
            let markdown = envelope.markdown.as_deref().unwrap_or("").trim();
            if markdown.is_empty() || markdown.chars().count() > 48_000 {
                return Err("next_action_invalid".into());
            }
            Ok(AllowedAction::Finish {
                markdown: markdown.into(),
            })
        }
        "inspect_app" | "analyze_js" | "capture_page" | "inspect_web_scripts" => {
            Err("next_action_unsupported".into())
        }
        _ => Err("next_action_invalid".into()),
    }
}

fn hypotheses(
    items: &[IncomingHypothesis],
    round: u32,
    evidence_ids: &[String],
) -> Result<Vec<Hypothesis>, String> {
    if items.is_empty() || items.len() > 4 {
        return Err("next_action_invalid".into());
    }
    items
        .iter()
        .map(|item| {
            if !valid_id(&item.id)
                || item.claim.trim().is_empty()
                || item.claim.chars().count() > 500
            {
                return Err("next_action_invalid".into());
            }
            if item
                .evidence_ids
                .iter()
                .any(|id| !evidence_ids.iter().any(|known| known == id))
            {
                return Err("next_action_unsupported".into());
            }
            Ok(Hypothesis {
                id: item.id.clone(),
                claim: item.claim.trim().into(),
                verdict: parse_verdict(&item.verdict)?,
                evidence_ids: item.evidence_ids.clone(),
                round,
            })
        })
        .collect()
}

fn parse_verdict(value: &str) -> Result<Verdict, String> {
    match value {
        "support" => Ok(Verdict::Support),
        "refute" => Ok(Verdict::Refute),
        "unknown" => Ok(Verdict::Unknown),
        _ => Err("next_action_invalid".into()),
    }
}

pub fn action_identity(action: &AllowedAction, bounds: &Bounds<'_>) -> String {
    match action {
        AllowedAction::ReadEvidence { evidence_id } => format!("read_evidence\n{evidence_id}"),
        AllowedAction::InspectApp => format!("inspect_app\n{}", bounds.snapshot_ids.join(",")),
        AllowedAction::AnalyzeJs => format!("analyze_js\n{}", bounds.snapshot_ids.join(",")),
        AllowedAction::CapturePage | AllowedAction::InspectWebScripts => {
            let Target::Website { url, .. } = bounds.target else {
                return "website-action-without-website".into();
            };
            format!("{}\n{url}", action_name(action))
        }
        AllowedAction::Finish { .. } => "finish".into(),
    }
}

fn action_name(action: &AllowedAction) -> &'static str {
    match action {
        AllowedAction::InspectApp => "inspect_app",
        AllowedAction::AnalyzeJs => "analyze_js",
        AllowedAction::CapturePage => "capture_page",
        AllowedAction::InspectWebScripts => "inspect_web_scripts",
        AllowedAction::ReadEvidence { .. } => "read_evidence",
        AllowedAction::Finish { .. } => "finish",
    }
}

fn extract_json(text: &str) -> Result<&str, String> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        return Ok(trimmed);
    }
    let start = trimmed.find('{').ok_or("next_action_invalid")?;
    let end = trimmed.rfind('}').ok_or("next_action_invalid")?;
    if end < start {
        return Err("next_action_invalid".into());
    }
    Ok(&trimmed[start..=end])
}

fn loop_path(store: &TargetStore, run_id: &str) -> Result<PathBuf, String> {
    if !valid_id(run_id) {
        return Err("invalid_run_id".into());
    }
    store.path(&format!("runs/{run_id}/iteration.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(verdict: &str) -> String {
        format!(
            r#"{{"id":"h-{verdict}","claim":"The pinned source exports a value.","verdict":"{verdict}","evidenceIds":["evidence-a"]}}"#
        )
    }

    #[test]
    fn hypotheses_cover_support_refute_and_unknown_and_reject_escape() {
        let target = Target::App {
            path: "/tmp/app".into(),
        };
        let evidence = vec!["evidence-a".to_string()];
        let snapshots = vec!["snapshot-a".to_string()];
        let bounds = Bounds {
            target: &target,
            rounds: 1,
            max_rounds: 5,
            evidence_ids: &evidence,
            snapshot_ids: &snapshots,
        };
        let raw = format!(
            r#"{{"kind":"next_action","action":"finish","markdown":"Done.","hypotheses":[{},{},{}]}}"#,
            one("support"),
            one("refute"),
            one("unknown")
        );
        let step = decide(&raw, &bounds, &LoopState::default()).unwrap();
        let Step::Finish { hypotheses, .. } = step else {
            panic!("expected finish");
        };
        assert_eq!(
            hypotheses
                .iter()
                .map(|item| match item.verdict {
                    Verdict::Support => "support",
                    Verdict::Refute => "refute",
                    Verdict::Unknown => "unknown",
                })
                .collect::<Vec<_>>(),
            ["support", "refute", "unknown"]
        );
        for escaped in [
            r#"{"kind":"next_action","action":"finish","markdown":"x","command":"sh","hypotheses":[{"id":"h-unknown","claim":"c","verdict":"unknown","evidenceIds":[]}]}"#,
            r#"{"kind":"next_action","action":"analyze_js","path":"../secret","hypotheses":[{"id":"h-unknown","claim":"c","verdict":"unknown","evidenceIds":[]}]}"#,
            r#"{"kind":"next_action","action":"finish","markdown":"x","budget":{"maxRounds":99},"hypotheses":[{"id":"h-unknown","claim":"c","verdict":"unknown","evidenceIds":[]}]}"#,
            r#"{"kind":"next_action","action":"read_evidence","evidenceId":"evidence-other","hypotheses":[{"id":"h-unknown","claim":"c","verdict":"unknown","evidenceIds":[]}]}"#,
            r#"{"kind":"next_action","action":"capture_page","hypotheses":[{"id":"h-unknown","claim":"c","verdict":"unknown","evidenceIds":[]}]}"#,
            r#"{"kind":"answer","markdown":"skip the loop"}"#,
        ] {
            let error = decide(escaped, &bounds, &LoopState::default()).unwrap_err();
            assert!(
                error == "next_action_invalid" || error == "next_action_unsupported",
                "{escaped} -> {error}"
            );
        }
    }

    #[test]
    fn repeated_read_stops_after_two_rounds_without_new_evidence() {
        let target = Target::Github {
            url: "https://github.com/example/fixture".into(),
            git_ref: None,
        };
        let evidence = vec!["evidence-a".to_string()];
        let snapshots = vec!["snapshot-a".to_string()];
        let raw = r#"{"kind":"next_action","action":"read_evidence","evidenceId":"evidence-a","hypotheses":[{"id":"h-unknown","claim":"No new file was observed.","verdict":"unknown","evidenceIds":["evidence-a"]}]}"#;
        let mut state = LoopState::default();
        let bounds = Bounds {
            target: &target,
            rounds: 1,
            max_rounds: 5,
            evidence_ids: &evidence,
            snapshot_ids: &snapshots,
        };
        let Step::Continue {
            identity,
            no_progress,
            hypotheses,
            ..
        } = decide(raw, &bounds, &state).unwrap()
        else {
            panic!("the first re-read is still inside the no-progress limit");
        };
        assert!(no_progress);
        apply_continue(&mut state, identity, true, hypotheses);
        assert!(matches!(
            decide(raw, &bounds, &state).unwrap(),
            Step::Stop {
                reason: "no_progress"
            }
        ));
        assert!(matches!(state.hypotheses[0].verdict, Verdict::Unknown));
    }

    #[test]
    fn another_round_past_the_budget_does_not_run() {
        let target = Target::App {
            path: "/tmp/app".into(),
        };
        let snapshots = vec!["snapshot-a".to_string()];
        let bounds = Bounds {
            target: &target,
            rounds: 1,
            max_rounds: 1,
            evidence_ids: &[],
            snapshot_ids: &snapshots,
        };
        let raw = r#"{"kind":"next_action","action":"analyze_js","hypotheses":[{"id":"h-unknown","claim":"Need another pass.","verdict":"unknown","evidenceIds":[]}]}"#;
        assert!(matches!(
            decide(raw, &bounds, &LoopState::default()).unwrap(),
            Step::Stop {
                reason: "budget_exhausted"
            }
        ));
    }

    #[test]
    fn saved_hypotheses_survive_a_reload() {
        let root = super::super::store::tests::temp_root();
        let store = super::super::store::Store::new(&root);
        let mut research = super::super::store::tests::sample_research("research-loop");
        research.workflow = super::super::models::Workflow::Target;
        research.schema_version = 2;
        research.target = Some(TargetConfig {
            target: Target::Github {
                url: "https://github.com/example/fixture".into(),
                git_ref: None,
            },
            run_ids: vec!["run-loop".into()],
        });
        store.create(&research).unwrap();
        let target = TargetStore::open(store, "research-loop").unwrap();
        let mut state = LoopState::default();
        state.hypotheses.push(Hypothesis {
            id: "h-unknown".into(),
            claim: "Not observed.".into(),
            verdict: Verdict::Unknown,
            evidence_ids: vec![],
            round: 2,
        });
        state.identities.insert("read_evidence\nevidence-a".into());
        state.stagnant_rounds = 1;
        save(&target, "run-loop", &state).unwrap();
        let loaded = load(&target, "run-loop").unwrap();
        assert_eq!(loaded.stagnant_rounds, 1);
        assert_eq!(loaded.hypotheses, state.hypotheses);
        assert!(loaded.identities.contains("read_evidence\nevidence-a"));
    }
}
