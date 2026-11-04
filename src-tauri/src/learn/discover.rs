//! Discover chat on the Learn start page: one fresh, tool-less agent call per
//! turn suggests repositories and search queries; Loom then looks them up on
//! GitHub and returns only repositories that exist. Turns are not stored as a
//! conversation, but each one leaves a redacted execution record under
//! `<appData>/learn/discover/runs/<runId>/`.
use super::{github, models::*, store::valid_id};
use crate::{
    agent::{
        self, AdapterInvocationRequest, AgentStage, Limits, Observation, ProcessKind, ProcessOwner,
        RunSpec, STOP_REASON_ABORT, STOP_REASON_TIMEOUT,
    },
    models::{now_ms, AgentConfig},
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

const PROMPT: &str = include_str!("prompts/discover.md");
const MAX_MESSAGES: usize = 12;
const MAX_CONVERSATION_CHARS: usize = 16_000;
const MAX_MESSAGE_CHARS: usize = 4_000;
const MAX_REPOS: usize = 8;
const MAX_QUERIES: usize = 2;
const MAX_CARDS: usize = 12;
const KEEP_RUNS: usize = 50;
const TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// Checks the request shape: a valid run id and a conversation ending with a user message.
pub fn validate(input: &DiscoverInput) -> Result<(), String> {
    if !valid_id(&input.run_id) {
        return Err("Invalid run id".into());
    }
    if !matches!(input.language.as_str(), "en" | "zh-CN") {
        return Err("Unsupported language".into());
    }
    if input
        .messages
        .iter()
        .any(|m| !matches!(m.role.as_str(), "user" | "assistant"))
    {
        return Err("Unsupported message role".into());
    }
    match input.messages.last() {
        Some(m) if m.role == "user" && !m.text.trim().is_empty() => Ok(()),
        _ => Err("Type a message first".into()),
    }
}

/// The most recent messages that fit the budget, oldest first, each labelled.
pub fn conversation(messages: &[DiscoverMessage]) -> String {
    let mut used = 0;
    let mut kept = Vec::new();
    for message in messages.iter().rev().take(MAX_MESSAGES) {
        let text = clip(message.text.trim(), MAX_MESSAGE_CHARS);
        let block = format!(
            "<{role}>\n{text}\n</{role}>",
            role = if message.role == "user" {
                "user"
            } else {
                "assistant"
            }
        );
        let size = block.chars().count();
        if used + size > MAX_CONVERSATION_CHARS && !kept.is_empty() {
            break;
        }
        used += size;
        kept.push(block);
    }
    kept.reverse();
    kept.join("\n\n")
}

pub fn prompt(input: &DiscoverInput) -> String {
    PROMPT
        .replace(
            "{{LANGUAGE}}",
            super::generate::language_name(&input.language),
        )
        .replace("{{CONVERSATION}}", &conversation(&input.messages))
}

/// Suggestions parsed from the agent's final ```json block.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Suggestions {
    pub repos: Vec<String>,
    pub queries: Vec<String>,
}

/// Splits the answer into visible Markdown and the trailing suggestions block.
/// A missing or malformed block leaves the text intact and yields `None`.
pub fn split_answer(text: &str) -> (String, Option<Suggestions>) {
    let Some(start) = text.rfind("```json") else {
        return (text.trim().to_string(), None);
    };
    let body = &text[start + 7..];
    let Some(end) = body.find("```") else {
        return (text.trim().to_string(), None);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body[..end].trim()) else {
        return (text.trim().to_string(), None);
    };
    let strings = |key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut seen = HashSet::new();
    let repos = strings("repos")
        .into_iter()
        .map(|r| {
            r.trim()
                .trim_start_matches("https://github.com/")
                .trim_end_matches('/')
                .to_string()
        })
        .filter(|r| github::valid_full_name(r) && seen.insert(r.to_ascii_lowercase()))
        .take(MAX_REPOS)
        .collect();
    let queries = strings("queries")
        .iter()
        .filter_map(|q| github::clean_query(q))
        .take(MAX_QUERIES)
        .collect();
    let visible = format!("{}{}", &text[..start], &body[end + 3..]);
    (
        visible.trim().to_string(),
        Some(Suggestions { repos, queries }),
    )
}

/// Looks up the suggestions: named repositories first (in the agent's order), then
/// query results. At most `1 + MAX_QUERIES` search requests; failures become warnings.
/// Named repositories missing from the batch search that are looked up one by one
/// (a renamed repository is only reachable through `/repos/<old name>`).
const MAX_RESOLVE: usize = 3;

async fn lookup(suggestions: &Suggestions, warnings: &mut Vec<String>) -> Vec<GithubRepoCard> {
    let mut cards = Vec::new();
    if let Some(query) = github::repos_query(&suggestions.repos) {
        // A query whose every name is unknown or renamed is rejected (422): nothing found yet.
        let mut found = match github::search(&query, MAX_REPOS as u8, false).await {
            Ok(found) => found,
            Err(error) => {
                if error != github::QUERY_REJECTED {
                    warnings.push(error);
                }
                Vec::new()
            }
        };
        // Suggested name → card name, so renamed repositories keep the agent's order.
        let mut names: Vec<(String, String)> = found
            .iter()
            .map(|c| {
                (
                    c.full_name.to_ascii_lowercase(),
                    c.full_name.to_ascii_lowercase(),
                )
            })
            .collect();
        let missing: Vec<&String> = suggestions
            .repos
            .iter()
            .filter(|r| github::valid_full_name(r))
            .filter(|r| !found.iter().any(|c| c.full_name.eq_ignore_ascii_case(r)))
            .collect();
        let mut not_found = Vec::new();
        for (index, name) in missing.iter().enumerate() {
            if index >= MAX_RESOLVE {
                not_found.push(name.as_str());
                continue;
            }
            match github::repository(name).await {
                Ok(Some(card)) => {
                    if !card.full_name.eq_ignore_ascii_case(name) {
                        warnings.push(format!("Renamed on GitHub: {name} → {}", card.full_name));
                    }
                    names.push((
                        name.to_ascii_lowercase(),
                        card.full_name.to_ascii_lowercase(),
                    ));
                    found.push(card);
                }
                Ok(None) => not_found.push(name.as_str()),
                Err(error) => {
                    warnings.push(error);
                    not_found.push(name.as_str());
                }
            }
        }
        if !not_found.is_empty() {
            warnings.push(format!(
                "Not found on GitHub (may be private or misspelled): {}",
                not_found.join(", ")
            ));
        }
        let rank = |card: &GithubRepoCard| {
            let full = card.full_name.to_ascii_lowercase();
            suggestions
                .repos
                .iter()
                .position(|r| {
                    let r = r.to_ascii_lowercase();
                    names.iter().any(|(asked, got)| *asked == r && *got == full)
                })
                .unwrap_or(usize::MAX)
        };
        found.sort_by_key(rank);
        cards.extend(found);
    }
    for query in &suggestions.queries {
        match github::search(query, 5, false).await {
            Ok(found) => cards.extend(found),
            Err(error) => {
                warnings.push(error);
                break;
            }
        }
    }
    let mut seen = HashSet::new();
    cards
        .into_iter()
        .filter(|c| seen.insert(c.full_name.to_ascii_lowercase()))
        .take(MAX_CARDS)
        .collect()
}

// ---- cancellation registry ----

fn running() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static RUNNING: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    RUNNING.get_or_init(|| Mutex::new(HashMap::new()))
}

struct Registration(String);
impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut map) = running().lock() {
            map.remove(&self.0);
        }
    }
}

fn register(run_id: &str) -> Result<(Registration, Arc<AtomicBool>), String> {
    let mut map = running().lock().map_err(|_| "Discover registry poisoned")?;
    if map.contains_key(run_id) {
        return Err("This turn is already running".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    map.insert(run_id.to_string(), cancel.clone());
    Ok((Registration(run_id.to_string()), cancel))
}

pub fn cancel(run_id: &str) -> bool {
    running()
        .lock()
        .ok()
        .and_then(|map| map.get(run_id).cloned())
        .map(|flag| flag.store(true, Ordering::Relaxed))
        .is_some()
}

// ---- execution record ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunRecord {
    run_id: String,
    agent_id: String,
    agent_name: String,
    adapter_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invocation: Option<String>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    started_at_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    finished_at_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    queries: Vec<String>,
    repos: Vec<String>,
}

#[derive(Clone)]
pub struct Discover {
    root: PathBuf,
}

impl Discover {
    pub fn new(app_data: &Path) -> Self {
        Self {
            root: app_data.join("learn").join("discover"),
        }
    }

    fn workspace(&self) -> Result<PathBuf, String> {
        let path = self.root.join("workspace");
        fs::create_dir_all(&path).map_err(|_| "Could not prepare the discover workspace")?;
        Ok(path)
    }

    fn run_dir(&self, run_id: &str) -> Result<PathBuf, String> {
        if !valid_id(run_id) {
            return Err("Invalid run id".into());
        }
        let path = self.root.join("runs").join(run_id);
        fs::create_dir_all(&path).map_err(|_| "Could not create the run record")?;
        Ok(path)
    }

    /// Keeps the newest `KEEP_RUNS` run records.
    fn prune(&self) {
        let Ok(entries) = fs::read_dir(self.root.join("runs")) else {
            return;
        };
        let mut runs: Vec<(std::time::SystemTime, PathBuf)> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| {
                (
                    e.metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH),
                    e.path(),
                )
            })
            .collect();
        runs.sort_by_key(|run| std::cmp::Reverse(run.0));
        for (_, path) in runs.into_iter().skip(KEEP_RUNS) {
            let _ = fs::remove_dir_all(path);
        }
    }

    fn save(dir: &Path, record: &RunRecord) {
        if let Ok(json) = serde_json::to_vec_pretty(record) {
            let _ = fs::write(dir.join("run.json"), json);
        }
    }

    /// Runs one turn and returns the visible answer with the repositories found.
    pub async fn send(
        &self,
        agent_config: &AgentConfig,
        input: &DiscoverInput,
        model: Option<&str>,
    ) -> Result<DiscoverReply, String> {
        validate(input)?;
        let (_registration, cancel) = register(&input.run_id)?;
        let dir = self.run_dir(&input.run_id)?;
        self.prune();
        let cwd = self.workspace()?;
        let prompt = prompt(input);
        let _ = fs::write(dir.join("prompt.md"), agent::redact_sensitive_text(&prompt));
        let mut record = RunRecord {
            run_id: input.run_id.clone(),
            agent_id: agent_config.id.clone(),
            agent_name: agent_config.name.clone(),
            adapter_type: agent_config.adapter_type.clone(),
            model: model.map(str::to_string),
            invocation: None,
            status: "running".into(),
            exit_code: None,
            started_at_ms: now_ms(),
            finished_at_ms: None,
            error: None,
            queries: Vec::new(),
            repos: Vec::new(),
        };
        Self::save(&dir, &record);
        let result = self
            .answer(
                agent_config,
                &prompt,
                &cwd,
                &dir,
                model,
                &cancel,
                &mut record,
            )
            .await;
        let result = match result {
            Ok(text) => {
                let (reply, suggestions) = split_answer(&text);
                let mut warnings = Vec::new();
                let suggestions = suggestions.unwrap_or_else(|| {
                    warnings.push(
                        "The Agent gave no repository list, so nothing was looked up on GitHub"
                            .into(),
                    );
                    Suggestions::default()
                });
                record.queries = suggestions.queries.clone();
                record.repos = suggestions.repos.clone();
                let repos = if cancel.load(Ordering::Relaxed) {
                    Vec::new()
                } else {
                    lookup(&suggestions, &mut warnings).await
                };
                if reply.is_empty() && repos.is_empty() {
                    Err("The Agent returned an empty answer".to_string())
                } else {
                    Ok(DiscoverReply {
                        reply,
                        repos,
                        queries: suggestions.queries,
                        warnings,
                    })
                }
            }
            Err(error) => Err(error),
        };
        record.finished_at_ms = Some(now_ms());
        match &result {
            Ok(reply) => {
                record.status = "complete".into();
                let _ = fs::write(
                    dir.join("result.md"),
                    agent::redact_sensitive_text(&reply.reply),
                );
            }
            Err(error) => {
                record.status = if cancel.load(Ordering::Relaxed) {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                record.error = Some(agent::redact_sensitive_text(error));
            }
        }
        Self::save(&dir, &record);
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn answer(
        &self,
        agent_config: &AgentConfig,
        prompt: &str,
        cwd: &Path,
        dir: &Path,
        model: Option<&str>,
        cancel: &Arc<AtomicBool>,
        record: &mut RunRecord,
    ) -> Result<String, String> {
        let prepared = agent::prepare_invocation(
            agent_config,
            &AdapterInvocationRequest {
                execution_profile: None,
                project_path: cwd,
                prompt,
                prompt_file: None,
                stage: AgentStage::Planning,
                resume_command: None,
                chat_permission_mode: Some("explore"),
                embed_prompt: agent::embeds_prompt(&agent_config.adapter_type),
                system_append: None,
                model,
                needs_tools: false,
            },
        )?;
        record.invocation = Some(describe(&prepared.program, &prepared.args));
        Self::save(dir, record);
        let spec = RunSpec {
            stdin_text: prepared.stdin_prompt.then(|| prompt.to_string()),
            prepared,
            run_id: format!("learn-discover-{}", record.run_id),
            owner: ProcessOwner::Feature {
                feature: "learn-discover".into(),
                scope_id: record.run_id.clone(),
                run_id: record.run_id.clone(),
            },
            kind: ProcessKind::Feature,
            limits: Limits {
                timeout: TIMEOUT,
                ..Limits::default()
            },
            enrich: None,
        };
        let stop = cancel.clone();
        let logs = dir.to_path_buf();
        let outcome = agent::run(
            spec,
            move || stop.load(Ordering::Relaxed).then_some(STOP_REASON_ABORT),
            |_| Ok(()),
            move |observation| {
                if let Observation::Log { stderr, line } = observation {
                    append(
                        &logs.join(if stderr { "stderr.log" } else { "stdout.log" }),
                        &agent::redact_sensitive_text(&line),
                    );
                }
                Ok(())
            },
        )
        .await?;
        record.exit_code = outcome.exit_code;
        match outcome.status.as_str() {
            "complete" => Ok(outcome.content),
            "aborted" if outcome.termination_reason.as_deref() == Some(STOP_REASON_TIMEOUT) => {
                Err(format!(
                    "The Agent did not answer within {} minutes",
                    TIMEOUT.as_secs() / 60
                ))
            }
            "aborted" if cancel.load(Ordering::Relaxed) => Err("Stopped".into()),
            "aborted" => Err("The Agent was stopped".into()),
            _ => {
                agent::forget_startable(Some(&agent_config.id));
                Err(agent::redact_sensitive_text(
                    outcome
                        .error_summary
                        .as_deref()
                        .unwrap_or("The Agent failed"),
                ))
            }
        }
    }
}

fn describe(program: &str, args: &[String]) -> String {
    let args: Vec<String> = args
        .iter()
        .map(|arg| {
            if arg.chars().count() > 160 {
                format!("<{} chars>", arg.chars().count())
            } else {
                agent::redact_sensitive_text(arg)
            }
        })
        .collect();
    format!(
        "{} {}",
        agent::redact_sensitive_text(program),
        args.join(" ")
    )
}

fn append(path: &Path, line: &str) {
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, text: &str) -> DiscoverMessage {
        DiscoverMessage {
            role: role.into(),
            text: text.into(),
        }
    }

    fn input(messages: Vec<DiscoverMessage>) -> DiscoverInput {
        DiscoverInput {
            run_id: "discover-1".into(),
            agent_id: "a".into(),
            model: None,
            language: "en".into(),
            messages,
        }
    }

    #[test]
    fn validation_needs_a_trailing_user_message() {
        assert!(validate(&input(vec![message("user", "agents in rust")])).is_ok());
        assert!(validate(&input(vec![])).is_err());
        assert!(validate(&input(vec![message("assistant", "hi")])).is_err());
        assert!(validate(&input(vec![message("user", "  ")])).is_err());
        assert!(validate(&input(vec![message("system", "x"), message("user", "y")])).is_err());
        let mut bad = input(vec![message("user", "x")]);
        bad.run_id = "../x".into();
        assert!(validate(&bad).is_err());
        bad.run_id = "ok".into();
        bad.language = "fr".into();
        assert!(validate(&bad).is_err());
    }

    #[test]
    fn conversation_keeps_the_latest_messages_within_budget() {
        let many: Vec<_> = (0..20)
            .map(|i| {
                message(
                    if i % 2 == 0 { "user" } else { "assistant" },
                    &format!("m{i}"),
                )
            })
            .collect();
        let text = conversation(&many);
        assert!(!text.contains("m7\n"));
        assert!(text.starts_with("<user>\nm8\n</user>"));
        assert!(text.ends_with("<assistant>\nm19\n</assistant>"));
        let long = vec![
            message("user", &"a".repeat(9_000)),
            message("assistant", &"b".repeat(9_000)),
            message("user", &"b".repeat(9_000)),
            message("assistant", &"b".repeat(9_000)),
            message("user", "latest"),
        ];
        let text = conversation(&long);
        assert!(text.contains("latest"));
        assert!(
            !text.contains("aaaa"),
            "oldest message is dropped over budget"
        );
        assert!(text.chars().count() <= MAX_CONVERSATION_CHARS);
        let prompt = prompt(&input(vec![message("user", "find me x")]));
        assert!(prompt.contains("Reply in English"));
        assert!(prompt.contains("<user>\nfind me x\n</user>"));
    }

    #[test]
    fn answer_block_is_parsed_and_removed() {
        let text = "Try these:\n- `a/b` small\n\n```json\n{\"repos\": [\"a/b\", \"https://github.com/c/d/\", \"A/B\", \"bad name\"], \"queries\": [\"rust  agent\\n\", \"x\", \"y\"]}\n```\n";
        let (visible, suggestions) = split_answer(text);
        assert_eq!(visible, "Try these:\n- `a/b` small");
        let suggestions = suggestions.unwrap();
        assert_eq!(suggestions.repos, vec!["a/b", "c/d"]);
        assert_eq!(suggestions.queries, vec!["rust agent", "x"]);
        let (visible, none) = split_answer("Just text, no block");
        assert_eq!(visible, "Just text, no block");
        assert!(none.is_none());
        let (visible, none) = split_answer("Broken ```json {oops ```");
        assert_eq!(visible, "Broken ```json {oops ```");
        assert!(none.is_none());
    }

    #[test]
    fn runs_register_once_and_cancel() {
        let (registration, flag) = register("discover-cancel-test").unwrap();
        assert!(register("discover-cancel-test").is_err());
        assert!(cancel("discover-cancel-test"));
        assert!(flag.load(Ordering::Relaxed));
        drop(registration);
        assert!(!cancel("discover-cancel-test"));
    }
}
