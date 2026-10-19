use loom_core::{Core, PermissionDocument, PermissionScope, SavePermissionRequest, SessionKey};
use loom_runtime::policy::{PermissionConfig, PermissionPolicy, PolicyLayers, PolicyScope};
use std::path::PathBuf;

fn decode(value: &serde_json::Value) -> Result<PermissionConfig, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "权限配置无法编码")?;
    PermissionConfig::parse(&bytes).map_err(|e| e.to_string())
}
pub(super) fn layers(core: &Core, key: &SessionKey) -> Result<PolicyLayers, String> {
    let documents = core
        .permission_documents(&key.workspace_id)
        .map_err(|e| e.to_string())?;
    let view = core.get_session(key).map_err(|e| e.to_string())?;
    let mut layers = PolicyLayers::default();
    for source in &view.metadata.enabled_sources {
        layers
            .sources
            .insert(source.clone(), PermissionConfig::default());
    }
    for document in documents {
        if matches!(&document.scope,PermissionScope::Source{source_id,..} if !layers.sources.contains_key(source_id))
        {
            continue;
        }
        let Some(config) = document.config else {
            continue;
        };
        let config = decode(&config)?;
        match document.scope {
            PermissionScope::Application => layers.application = Some(config),
            PermissionScope::Workspace { .. } => layers.workspace = config,
            PermissionScope::Source { source_id, .. }
                if layers.sources.contains_key(&source_id) =>
            {
                layers.sources.insert(source_id, config);
            }
            _ => {}
        }
    }
    Ok(layers)
}
/// Validation uses the same compiler as execution. No arbitrary path is writable
/// through this API and no active tool approval is modified behind the caller.
pub(super) fn save(
    core: &Core,
    request: SavePermissionRequest,
    validation_root: PathBuf,
) -> Result<PermissionDocument, String> {
    if let Some(value) = &request.config {
        let config = decode(value)?;
        let mut layers = PolicyLayers::default();
        match &request.scope {
            PermissionScope::Application => layers.application = Some(config),
            PermissionScope::Workspace { .. } => layers.workspace = config,
            PermissionScope::Source { source_id, .. } => {
                layers.sources.insert(source_id.clone(), config);
            }
        }
        PermissionPolicy::new(
            layers,
            PolicyScope::native(validation_root.clone(), validation_root),
        )
        .map_err(|e| e.to_string())?;
    }
    core.save_permission_document(request)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::*;
    use serde_json::json;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "loom-host-permissions-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p.canonicalize().unwrap())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn id(v: &str) -> Id {
        Id::new(v).unwrap()
    }
    #[test]
    fn host_validation_rejects_bad_regex_and_only_active_source_rules_are_loaded() {
        let f = Fixture::new();
        let core = Core::open(&f.0).unwrap();
        core.create_workspace(id("w"), "Workspace".into()).unwrap();
        let key = SessionKey {
            workspace_id: id("w"),
            session_id: id("s"),
        };
        core.create_session(CreateSession {
            key: key.clone(),
            title: None,
            project_id: None,
            permission_mode: PermissionMode::Ask,
            connection: Default::default(),
        })
        .unwrap();
        let request = SavePermissionRequest {
            scope: PermissionScope::Workspace {
                workspace_id: id("w"),
            },
            operation_id: id("invalid"),
            expected_revision: None,
            config: Some(json!({"allowedBashPatterns":["["]})),
        };
        assert!(save(&core, request, f.0.clone()).is_err());
        assert!(core.permission_documents(&id("w")).unwrap().is_empty());
        save(
            &core,
            SavePermissionRequest {
                scope: PermissionScope::Source {
                    workspace_id: id("w"),
                    source_id: id("alpha"),
                },
                operation_id: id("source"),
                expected_revision: None,
                config: Some(json!({"allowedMcpPatterns":["custom"]})),
            },
            f.0.clone(),
        )
        .unwrap();
        assert!(layers(&core, &key).unwrap().sources.is_empty());
        let view = core.get_session(&key).unwrap();
        core.patch_session(
            &key,
            view.last_seq,
            SessionPatch {
                enabled_sources: Some(vec![id("alpha")]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            layers(&core, &key).unwrap().sources[&id("alpha")].allowed_mcp_patterns[0].pattern(),
            "custom"
        );
    }
}
