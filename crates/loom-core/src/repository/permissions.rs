use super::*;
use std::io::Read;
impl Core {
    pub fn permission_documents(&self, workspace_id: &Id) -> Result<Vec<PermissionDocument>> {
        let state = self.lock()?;
        if !state.catalog.workspaces.contains_key(workspace_id) {
            return Err(CoreError::missing("workspace not found"));
        }
        Ok(state
            .catalog
            .permissions
            .values()
            .filter(|p| p.scope.workspace_id().is_none_or(|w| w == workspace_id))
            .cloned()
            .collect())
    }
    /// Trusted host operation: the host must compile/validate runtime rules first.
    pub fn save_permission_document(
        &self,
        input: SavePermissionRequest,
    ) -> Result<PermissionDocument> {
        crate::permissions::validate_permission_document(&input.config)?;
        let mut state = self.lock()?;
        if input
            .scope
            .workspace_id()
            .is_some_and(|w| !state.catalog.workspaces.contains_key(w))
        {
            return Err(CoreError::missing("workspace not found"));
        }
        let previous = state.catalog.permissions.get(&input.scope);
        if let Some(old) = previous.filter(|p| p.operation_id == input.operation_id) {
            return if old.config == input.config {
                Ok(old.clone())
            } else {
                Err(CoreError::conflict(
                    "permission operation reused with different content",
                ))
            };
        }
        if previous.map(|p| p.revision) != input.expected_revision {
            return Err(CoreError::conflict(
                "permission configuration changed; reload before saving",
            ));
        }
        let revision = previous.map_or(1, |p| p.revision + 1);
        if revision > MAX_SAFE_SEQUENCE {
            return Err(CoreError::invalid("permission revision limit reached"));
        }
        let now = journal::now_ms().max(previous.map_or(0, |p| p.updated_at_ms));
        let document = PermissionDocument {
            scope: input.scope,
            revision,
            operation_id: input.operation_id,
            config: input.config,
            updated_at_ms: now,
        };
        let event = state.catalog_journal.prepare(
            CatalogEvent::PermissionSaved {
                document: document.clone(),
            },
            now,
        )?;
        state.catalog_journal.append(&event)?;
        state.catalog.apply(&event)?;
        let workspaces: Vec<_> = match document.scope.workspace_id() {
            Some(w) => vec![w.clone()],
            None => state.catalog.workspaces.keys().cloned().collect(),
        };
        for w in workspaces {
            Self::notify_workspace(&mut state, &w, None, event.seq);
        }
        Ok(document)
    }
    pub fn save_policy_snapshot(
        &self,
        key: &SessionKey,
        snapshot: &serde_json::Value,
    ) -> Result<String> {
        let mut state = self.lock()?;
        self.session(&mut state, key)?;
        let bytes = serde_json::to_vec(snapshot)
            .map_err(|_| CoreError::invalid("invalid policy snapshot"))?;
        if bytes.len() > 1024 * 1024 || !snapshot.is_object() {
            return Err(CoreError::invalid(
                "policy snapshot must be an object of at most 1 MiB",
            ));
        }
        let digest = projection::checksum(snapshot)?;
        let directory = self.session_directory(key)?.join("policies");
        journal::directory(&directory)?;
        let path = directory.join(format!("{digest}.json"));
        journal::regular(&path)?;
        if path.exists() {
            let existing = read_snapshot(&path, &digest)?;
            if existing != *snapshot {
                return Err(CoreError::corrupt("policy snapshot content mismatch"));
            }
        } else {
            journal::atomic_write(&path, snapshot)?;
        }
        Ok(digest)
    }
    pub fn read_policy_snapshot(
        &self,
        key: &SessionKey,
        turn_id: &Id,
    ) -> Result<serde_json::Value> {
        let mut state = self.lock()?;
        let view = self.session(&mut state, key)?.view();
        let digest = view
            .turns
            .iter()
            .find(|t| &t.id == turn_id)
            .and_then(|t| t.binding.as_ref())
            .and_then(|b| b.policy_snapshot_digest.clone())
            .ok_or_else(|| CoreError::missing("turn has no recorded policy snapshot"))?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CoreError::corrupt("invalid policy snapshot digest"));
        }
        let directory = self.session_directory(key)?.join("policies");
        journal::directory(&directory)?;
        read_snapshot(&directory.join(format!("{digest}.json")), &digest)
    }
}
fn read_snapshot(path: &Path, digest: &str) -> Result<serde_json::Value> {
    journal::regular(path)?;
    let mut options = journal::options();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    let file = options.read(true).open(path)?;
    if !file.metadata()?.is_file() {
        return Err(CoreError::corrupt("policy snapshot is not a file"));
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 2).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 + 1 {
        return Err(CoreError::corrupt("policy snapshot exceeds its limit"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| CoreError::corrupt("policy snapshot is invalid"))?;
    if projection::checksum(&value)? != digest {
        return Err(CoreError::corrupt("policy snapshot checksum mismatch"));
    }
    Ok(value)
}
