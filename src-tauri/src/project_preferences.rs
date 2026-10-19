use crate::{
    agents,
    migrations::{self, PROJECT_PREFERENCES_SCHEMA_VERSION},
    models::{now_ms, AgentConfig},
    storage,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;

const PROJECT_AGENT_PREFERENCES_FILE: &str = "agent-preferences.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAgentPreferences {
    #[serde(default)]
    pub planning_agent_ids: Vec<String>,
    #[serde(default)]
    pub implementation_agent_id: Option<String>,
    #[serde(default)]
    pub review_agent_ids: Vec<String>,
    #[serde(default)]
    pub debugging_agent_id: Option<String>,
    #[serde(default)]
    pub testing_agent_id: Option<String>,
    #[serde(default)]
    pub documentation_agent_id: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u128,
}

fn preferences_path(project_path: &Path) -> std::path::PathBuf {
    storage::project_loom_dir(project_path).join(PROJECT_AGENT_PREFERENCES_FILE)
}

pub(crate) fn load_for_project(project_path: &Path) -> Result<ProjectAgentPreferences, String> {
    let path = preferences_path(project_path);
    if !path.exists() {
        return Ok(ProjectAgentPreferences::default());
    }
    migrations::read_versioned_json(
        &path,
        "project-agent-preferences",
        PROJECT_PREFERENCES_SCHEMA_VERSION,
    )
}

fn agent_supports(agent: &AgentConfig, capabilities: &[&str]) -> bool {
    agent.enabled
        && agent
            .capabilities
            .iter()
            .any(|capability| capabilities.contains(&capability.as_str()))
}

fn normalize_agent_ids(
    ids: Vec<String>,
    agents: &[AgentConfig],
    capabilities: &[&str],
) -> Vec<String> {
    let mut normalized = Vec::new();
    for id in ids {
        if normalized.contains(&id) {
            continue;
        }
        if agents
            .iter()
            .any(|agent| agent.id == id && agent_supports(agent, capabilities))
        {
            normalized.push(id);
        }
    }
    normalized
}

fn normalize_agent_id(
    id: Option<String>,
    agents: &[AgentConfig],
    capabilities: &[&str],
) -> Option<String> {
    id.filter(|id| {
        agents
            .iter()
            .any(|agent| agent.id == *id && agent_supports(agent, capabilities))
    })
}

pub(crate) fn normalize_preferences(
    mut preferences: ProjectAgentPreferences,
    agents: &[AgentConfig],
) -> ProjectAgentPreferences {
    preferences.planning_agent_ids =
        normalize_agent_ids(preferences.planning_agent_ids, agents, &["planning"]);
    preferences.implementation_agent_id = normalize_agent_id(
        preferences.implementation_agent_id,
        agents,
        &["implementation"],
    );
    preferences.review_agent_ids =
        normalize_agent_ids(preferences.review_agent_ids, agents, &["review"]);
    preferences.debugging_agent_id = normalize_agent_id(
        preferences.debugging_agent_id,
        agents,
        &["debugging", "implementation"],
    );
    preferences.testing_agent_id =
        normalize_agent_id(preferences.testing_agent_id, agents, &["testing"]);
    preferences.documentation_agent_id = normalize_agent_id(
        preferences.documentation_agent_id,
        agents,
        &["documentation"],
    );
    preferences
}

pub(crate) fn load_normalized_for_app(
    app: &AppHandle,
    project_path: &Path,
) -> Result<ProjectAgentPreferences, String> {
    let preferences = load_for_project(project_path)?;
    let agents = agents::load_agents(app)?;
    Ok(normalize_preferences(preferences, &agents))
}

#[tauri::command]
pub fn load_project_agent_preferences(
    app: AppHandle,
    project_path: String,
) -> Result<ProjectAgentPreferences, String> {
    load_normalized_for_app(&app, Path::new(&project_path))
}

#[tauri::command]
pub fn save_project_agent_preferences(
    app: AppHandle,
    project_path: String,
    preferences: ProjectAgentPreferences,
) -> Result<ProjectAgentPreferences, String> {
    let mut normalized = normalize_preferences(preferences, &agents::load_agents(&app)?);
    normalized.updated_at_ms = now_ms();
    migrations::write_versioned_json(
        &preferences_path(Path::new(&project_path)),
        PROJECT_PREFERENCES_SCHEMA_VERSION,
        &normalized,
    )?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, capabilities: &[&str], enabled: bool) -> AgentConfig {
        AgentConfig {
            id: id.to_string(),
            name: id.to_string(),
            command: id.to_string(),
            args: Vec::new(),
            working_directory_policy: "project_root".to_string(),
            capabilities: capabilities.iter().map(|value| value.to_string()).collect(),
            adapter_type: "cli".to_string(),
            can_write_files: true,
            can_run_commands: true,
            enabled,
            available: true,
        }
    }

    #[test]
    fn normalization_prunes_duplicates_disabled_and_wrong_capabilities() {
        let agents = vec![
            agent("planner", &["planning"], true),
            agent("builder", &["implementation", "debugging", "testing"], true),
            agent("disabled-reviewer", &["review"], false),
        ];
        let preferences = ProjectAgentPreferences {
            planning_agent_ids: vec![
                "planner".to_string(),
                "planner".to_string(),
                "builder".to_string(),
            ],
            implementation_agent_id: Some("builder".to_string()),
            review_agent_ids: vec!["disabled-reviewer".to_string()],
            debugging_agent_id: Some("builder".to_string()),
            testing_agent_id: Some("builder".to_string()),
            documentation_agent_id: Some("planner".to_string()),
            updated_at_ms: 1,
        };

        let normalized = normalize_preferences(preferences, &agents);

        assert_eq!(normalized.planning_agent_ids, ["planner"]);
        assert_eq!(
            normalized.implementation_agent_id.as_deref(),
            Some("builder")
        );
        assert!(normalized.review_agent_ids.is_empty());
        assert_eq!(normalized.debugging_agent_id.as_deref(), Some("builder"));
        assert_eq!(normalized.testing_agent_id.as_deref(), Some("builder"));
        assert!(normalized.documentation_agent_id.is_none());
    }

    #[test]
    fn missing_preferences_return_defaults() {
        let root = std::env::temp_dir().join(format!("loom-project-prefs-{}", now_ms()));
        let loaded = load_for_project(&root).expect("missing preferences should load");
        assert_eq!(loaded, ProjectAgentPreferences::default());
    }

    #[test]
    fn legacy_preferences_are_migrated_on_load() {
        let root = std::env::temp_dir().join(format!("loom-project-prefs-legacy-{}", now_ms()));
        let preferences = ProjectAgentPreferences {
            planning_agent_ids: vec!["planner".to_string()],
            updated_at_ms: 7,
            ..Default::default()
        };
        storage::atomic_write_json(&preferences_path(&root), &preferences)
            .expect("legacy preferences");

        let loaded = load_for_project(&root).expect("legacy preferences should migrate");
        let stored: serde_json::Value =
            storage::read_json_file(&preferences_path(&root)).expect("migrated preferences");

        assert_eq!(loaded.planning_agent_ids, ["planner"]);
        assert_eq!(stored["schemaVersion"], PROJECT_PREFERENCES_SCHEMA_VERSION);
        assert_eq!(stored["data"]["updatedAtMs"], 7);
        std::fs::remove_dir_all(root).ok();
    }
}
