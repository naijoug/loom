use loom_core::{Id, PermissionMode};
use loom_runtime::{policy::*, Authorization, RuntimeError};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "loom-policy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
    fn scope(&self) -> PolicyScope {
        PolicyScope::native(self.0.clone(), self.0.clone())
    }
    fn policy(&self, layers: PolicyLayers) -> PermissionPolicy {
        PermissionPolicy::new(layers, self.scope()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn id(s: &str) -> Id {
    Id::new(s).unwrap()
}
fn config(v: Value) -> PermissionConfig {
    PermissionConfig::parse(&serde_json::to_vec(&v).unwrap()).unwrap()
}
fn decision(p: &mut PermissionPolicy, m: PermissionMode, n: &str, v: Value) -> &'static str {
    match p.decide(m, n, &v).unwrap() {
        Authorization::Allow { .. } => "allow",
        Authorization::Deny { .. } => "deny",
        Authorization::Ask => "ask",
    }
}
#[test]
fn native_modes_distinguish_readonly_mutation_and_unregistered_tools() {
    let f = Fixture::new();
    let mut p = f.policy(PolicyLayers::default());
    for mode in [
        PermissionMode::Explore,
        PermissionMode::Ask,
        PermissionMode::Execute,
        PermissionMode::Guarded,
    ] {
        let mutation = match mode {
            PermissionMode::Explore => "deny",
            PermissionMode::Execute => "allow",
            _ => "ask",
        };
        assert_eq!(
            decision(&mut p, mode, "read", json!({"path":"file"})),
            "allow"
        );
        assert_eq!(
            decision(&mut p, mode, "write", json!({"path":"file"})),
            mutation
        );
        assert_eq!(
            decision(&mut p, mode, "bash", json!({"command":"git status | head"})),
            "allow"
        );
        assert_eq!(
            decision(&mut p, mode, "bash", json!({"command":"find . -delete"})),
            mutation
        );
        assert_eq!(decision(&mut p, mode, "missing_tool", json!({})), "deny");
    }
}
#[test]
fn workspace_and_active_source_rules_are_additive_and_source_patterns_do_not_escape() {
    let f = Fixture::new();
    let mut layers = PolicyLayers {
        workspace: config(
            json!({"allowedBashPatterns":["^workspace-read$"],"allowedApiEndpoints":[{"method":"POST","path":"^/search$"}]}),
        ),
        ..Default::default()
    };
    layers.sources.insert(
        id("alpha"),
        config(
            json!({"allowedMcpPatterns":["custom|delete"],"allowedBashPatterns":["^source-read$"]}),
        ),
    );
    layers
        .sources
        .insert(id("beta"), PermissionConfig::default());
    let mut p = f.policy(layers);
    for command in ["ls", "workspace-read", "source-read"] {
        assert_eq!(
            decision(
                &mut p,
                PermissionMode::Explore,
                "bash",
                json!({"command":command})
            ),
            "allow"
        );
    }
    assert_eq!(
        decision(
            &mut p,
            PermissionMode::Explore,
            "mcp__alpha__delete_record",
            json!({})
        ),
        "allow"
    );
    assert_eq!(
        decision(
            &mut p,
            PermissionMode::Explore,
            "mcp__beta__delete_record",
            json!({})
        ),
        "deny"
    );
    assert_eq!(
        decision(
            &mut p,
            PermissionMode::Execute,
            "mcp__inactive__get_item",
            json!({})
        ),
        "deny"
    );
    for mode in [PermissionMode::Explore, PermissionMode::Ask] {
        assert_eq!(
            decision(
                &mut p,
                mode,
                "api_alpha",
                json!({"method":"POST","path":"/search"})
            ),
            "allow"
        );
        assert_eq!(
            decision(
                &mut p,
                mode,
                "mcp__alpha__api_call",
                json!({"method":"get","path":"/anything"})
            ),
            "allow"
        );
        assert_eq!(
            decision(
                &mut p,
                mode,
                "api_alpha",
                json!({"method":"DELETE","path":"/search"})
            ),
            if mode == PermissionMode::Ask {
                "ask"
            } else {
                "deny"
            }
        );
    }
}
#[test]
fn write_globs_match_literal_paths_and_resist_sibling_and_symlink_escapes() {
    let f = Fixture::new();
    let outside = Fixture::new();
    fs::create_dir(f.0.join("allowed")).unwrap();
    let layers = PolicyLayers {
        workspace: config(json!({"allowedWritePaths":[format!("{}/allowed/**",f.0.display())]})),
        ..Default::default()
    };
    let mut p = f.policy(layers);
    for mode in [PermissionMode::Explore, PermissionMode::Ask] {
        assert_eq!(
            decision(
                &mut p,
                mode,
                "write",
                json!({"path":"allowed/new/child.txt"})
            ),
            "allow"
        );
        assert_eq!(
            decision(
                &mut p,
                mode,
                "write",
                json!({"path":"allowed/../outside.txt"})
            ),
            if mode == PermissionMode::Ask {
                "ask"
            } else {
                "deny"
            }
        );
        assert_eq!(
            decision(
                &mut p,
                mode,
                "write",
                json!({"path":"allowed-sibling/file"})
            ),
            if mode == PermissionMode::Ask {
                "ask"
            } else {
                "deny"
            }
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside.0, f.0.join("allowed/link")).unwrap();
        assert_eq!(
            decision(
                &mut p,
                PermissionMode::Explore,
                "write",
                json!({"path":"allowed/link/file"})
            ),
            "deny"
        );
    }
}
#[test]
fn explore_session_folder_exception_does_not_silently_grant_ask_writes() {
    let f = Fixture::new();
    let mut scope = f.scope();
    scope.plans_directory = Some(f.0.join("plans"));
    scope.data_directory = Some(f.0.join("data"));
    let mut p = PermissionPolicy::new(PolicyLayers::default(), scope).unwrap();
    for path in ["plans/plan.md", "data/result.json"] {
        assert_eq!(
            decision(
                &mut p,
                PermissionMode::Explore,
                "write",
                json!({"path":path})
            ),
            "allow"
        );
        assert_eq!(
            decision(&mut p, PermissionMode::Ask, "write", json!({"path":path})),
            "ask"
        );
    }
    assert_eq!(
        decision(
            &mut p,
            PermissionMode::Explore,
            "write",
            json!({"path":"plans-other/plan.md"})
        ),
        "deny"
    );
}
struct Check {
    active: bool,
    answer: i32,
}
impl GuardedCheck for Check {
    fn active(&self) -> bool {
        self.active
    }
    fn assess(&mut self, _: GuardedCall<'_>) -> Result<Option<Vec<GuardedRisk>>, RuntimeError> {
        match self.answer {
            0 => Ok(None),
            1 => Ok(Some(vec![GuardedRisk::External])),
            _ => Err(RuntimeError::new("fixture", "no answer")),
        }
    }
}
#[test]
fn guarded_inactive_and_transient_failure_have_distinct_upstream_semantics() {
    let f = Fixture::new();
    let outside = Fixture::new();
    let mut p = f.policy(PolicyLayers {
        workspace: config(json!({"allowedWritePaths":[format!("{}/**",outside.0.display())]})),
        ..Default::default()
    });
    for (active, answer, expected) in [
        (false, 0, "ask"),
        (true, 0, "allow"),
        (true, 1, "ask"),
        (true, 2, "allow"),
    ] {
        p.set_guarded_check(Some(Box::new(Check { active, answer })));
        assert_eq!(
            decision(
                &mut p,
                PermissionMode::Guarded,
                "bash",
                json!({"command":"touch file"})
            ),
            expected
        );
        assert_eq!(
            decision(
                &mut p,
                PermissionMode::Execute,
                "bash",
                json!({"command":"touch file"})
            ),
            "allow"
        );
        if active {
            assert_eq!(
                decision(
                    &mut p,
                    PermissionMode::Guarded,
                    "write",
                    json!({"path":"local.txt"})
                ),
                "allow"
            );
            assert_eq!(
                decision(
                    &mut p,
                    PermissionMode::Guarded,
                    "write",
                    json!({"path":outside.0.join("outside.txt")})
                ),
                "ask"
            );
        }
    }
}
#[test]
fn snapshot_identity_changes_with_rules_scope_and_active_sources() {
    let f = Fixture::new();
    let plain = f.policy(PolicyLayers::default());
    let changed = f.policy(PolicyLayers {
        workspace: config(json!({"allowedBashPatterns":["^custom$"]})),
        ..Default::default()
    });
    assert_ne!(plain.snapshot_digest(), changed.snapshot_digest());
    let mut layers = PolicyLayers::default();
    layers
        .sources
        .insert(id("alpha"), PermissionConfig::default());
    assert_ne!(plain.snapshot_digest(), f.policy(layers).snapshot_digest());
    assert!(PermissionConfig::parse(
        br#"{"allowedApiEndpoints":[{"method":"INVALID","path":"/"}]}"#
    )
    .is_err());
    assert!(PermissionPolicy::new(
        PolicyLayers {
            workspace: config(json!({"allowedBashPatterns":["["]})),
            ..Default::default()
        },
        f.scope()
    )
    .is_err());
}

#[test]
fn default_mcp_verbs_match_actual_upstream_without_source_name_or_substring_leakage() {
    let f = Fixture::new();
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/craft-mcp-cases.json")).unwrap();
    let mut layers = PolicyLayers::default();
    for source in ["alpha", "budget", "slack"] {
        layers
            .sources
            .insert(id(source), PermissionConfig::default());
    }
    let mut policy = f.policy(layers);
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        for mode in [PermissionMode::Explore, PermissionMode::Ask] {
            let expected = if case["allowed"] == true {
                "allow"
            } else if mode == PermissionMode::Ask {
                "ask"
            } else {
                "deny"
            };
            assert_eq!(
                decision(&mut policy, mode, name, json!({})),
                expected,
                "{name}"
            );
        }
    }
}

#[test]
fn rea_source_is_strictly_partitioned_and_requires_explicit_activation() {
    let f = Fixture::new();
    let mut layers = PolicyLayers::default();
    layers
        .sources
        .insert(id("rea"), PermissionConfig::default());
    let mut p = f.policy(layers);
    assert_eq!(
        decision(
            &mut p,
            PermissionMode::Explore,
            "mcp__rea__get_evidence_bundle",
            json!({})
        ),
        "allow"
    );
    assert_eq!(
        decision(
            &mut p,
            PermissionMode::Explore,
            "mcp__rea__open_binary",
            json!({})
        ),
        "deny"
    );
    assert_eq!(
        decision(
            &mut p,
            PermissionMode::Ask,
            "mcp__rea__open_binary",
            json!({})
        ),
        "ask"
    );
    let mut inactive = f.policy(PolicyLayers::default());
    assert!(matches!(
        inactive
            .decide(
                PermissionMode::Execute,
                "mcp__rea__get_evidence_bundle",
                &json!({})
            )
            .unwrap(),
        Authorization::Deny { .. }
    ));
}

#[test]
fn remember_scope_preserves_case_and_api_source_and_never_offers_dangerous_or_guarded_exemptions() {
    let f = Fixture::new();
    let mut p = f.policy(PolicyLayers::default());
    let a = p
        .remember_scope(
            PermissionMode::Ask,
            "bash",
            &json!({"command":"python Script.py --token private-value"}),
        )
        .unwrap()
        .unwrap();
    let b = p
        .remember_scope(
            PermissionMode::Ask,
            "bash",
            &json!({"command":"python script.py --token private-value"}),
        )
        .unwrap()
        .unwrap();
    assert_ne!(a.keys[0].digest, b.keys[0].digest);
    assert!(!serde_json::to_string(&a).unwrap().contains("private-value"));
    let a = p
        .remember_scope(
            PermissionMode::Ask,
            "api_alpha",
            &json!({"method":"POST","path":"/item"}),
        )
        .unwrap()
        .unwrap();
    let b = p
        .remember_scope(
            PermissionMode::Ask,
            "api_beta",
            &json!({"method":"POST","path":"/item"}),
        )
        .unwrap()
        .unwrap();
    assert_ne!(a.keys[0].digest, b.keys[0].digest);
    for command in [
        "rm file",
        "git push",
        "brew install --cask app",
        "touch file && rm other",
    ] {
        assert!(p
            .remember_scope(PermissionMode::Ask, "bash", &json!({"command":command}))
            .unwrap()
            .is_none());
    }
    p.set_guarded_check(Some(Box::new(Check {
        active: true,
        answer: 1,
    })));
    assert!(p
        .remember_scope(
            PermissionMode::Guarded,
            "bash",
            &json!({"command":"touch file"})
        )
        .unwrap()
        .is_none());
    p.set_guarded_check(None);
    assert!(p
        .remember_scope(
            PermissionMode::Guarded,
            "bash",
            &json!({"command":"touch file"})
        )
        .unwrap()
        .is_some());
}
#[cfg(unix)]
#[test]
fn remembered_directory_selector_changes_when_a_symlink_targets_another_folder() {
    let f = Fixture::new();
    let a = Fixture::new();
    let b = Fixture::new();
    let p = f.policy(PolicyLayers::default());
    let link = f.0.join("link");
    std::os::unix::fs::symlink(&a.0, &link).unwrap();
    let before = p
        .remember_scope(PermissionMode::Ask, "write", &json!({"path":"link/file"}))
        .unwrap()
        .unwrap();
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&b.0, &link).unwrap();
    let after = p
        .remember_scope(PermissionMode::Ask, "write", &json!({"path":"link/file"}))
        .unwrap()
        .unwrap();
    assert_ne!(before.keys[0].digest, after.keys[0].digest);
}
