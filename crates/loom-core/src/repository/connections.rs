use super::*;
impl Core {
    pub fn list_connections(&self) -> Result<Vec<ModelConnection>> {
        Ok(self.lock()?.catalog.connections.values().cloned().collect())
    }
    pub fn get_connection(&self, id: &Id) -> Result<ModelConnection> {
        self.lock()?
            .catalog
            .connections
            .get(id)
            .cloned()
            .ok_or_else(|| CoreError::missing("model connection not found"))
    }
    /// Trusted host operation, deliberately absent from CoreCommand: the host
    /// must complete vault storage before publishing a credential reference.
    pub fn save_connection_metadata(
        &self,
        input: SaveConnectionMetadata,
    ) -> Result<ModelConnection> {
        input.config.validate()?;
        if (input.config.auth == ConnectionAuth::None) != input.credential_ref.is_none() {
            return Err(CoreError::invalid(
                "connection authentication and credential reference mismatch",
            ));
        }
        let mut state = self.lock()?;
        let previous = state.catalog.connections.get(&input.id);
        if let Some(previous) = previous.filter(|old| old.operation_id == input.operation_id) {
            return if previous.config == input.config
                && previous.credential_ref == input.credential_ref
            {
                Ok(previous.clone())
            } else {
                Err(CoreError::conflict(
                    "connection operation ID reused with different content",
                ))
            };
        }
        if input.expected_revision != previous.map(|old| old.revision) {
            return Err(CoreError::conflict(
                "connection changed; reload before saving",
            ));
        }
        let revision = previous.map(|old| old.revision + 1).unwrap_or(1);
        if revision > MAX_SAFE_SEQUENCE {
            return Err(CoreError::invalid("connection revision limit reached"));
        }
        let now = journal::now_ms().max(previous.map(|p| p.updated_at_ms).unwrap_or(0));
        let connection = ModelConnection {
            id: input.id,
            revision,
            operation_id: input.operation_id,
            created_at_ms: previous.map(|p| p.created_at_ms).unwrap_or(now),
            updated_at_ms: now,
            config: input.config,
            credential_ref: input.credential_ref,
        };
        let event = state.catalog_journal.prepare(
            CatalogEvent::ConnectionSaved {
                connection: connection.clone(),
            },
            now,
        )?;
        state.catalog_journal.append(&event)?;
        state.catalog.apply(&event)?;
        let workspaces: Vec<_> = state.catalog.workspaces.keys().cloned().collect();
        for workspace in workspaces {
            Self::notify_workspace(&mut state, &workspace, None, event.seq);
        }
        Ok(connection)
    }
}
