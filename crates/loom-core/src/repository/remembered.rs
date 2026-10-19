use super::*;
impl Core {
    pub fn clear_remembered_grants(&self, key: &SessionKey) -> Result<SessionView> {
        let mut state = self.lock()?;
        let view = self.session(&mut state, key)?.view();
        let grant_ids: Vec<_> = view
            .remembered_grants
            .values()
            .filter(|g| g.revoked_at_ms.is_none())
            .map(|g| g.id.clone())
            .collect();
        if !grant_ids.is_empty() {
            self.commit(
                &mut state,
                key,
                SessionEvent::RememberedGrantsRevoked {
                    grant_ids: grant_ids.clone(),
                    reason: GrantRevocation::User,
                },
            )?;
            for id in grant_ids {
                state.live_grants.remove(&(key.clone(), id));
            }
        }
        Ok(state.sessions[key].view().clone())
    }
    /// Validate remembered keys and persist tool admission under the same lock.
    /// Clearing grants cannot interleave between checking and admission.
    pub fn try_remembered_tool(
        &self,
        lease: &TurnLease,
        event_id: Id,
        spec: &ToolApprovalSpec,
        redactions: &[String],
    ) -> Result<bool> {
        let Some(offer) = &spec.remember else {
            return Ok(false);
        };
        offer.validate()?;
        let mut state = self.lock()?;
        self.check_lease(&state, lease)?;
        let view = self.session(&mut state, lease.key())?.view();
        let binding = view
            .turns
            .iter()
            .find(|t| &t.id == lease.turn_id())
            .and_then(|t| t.binding.as_ref())
            .ok_or_else(|| CoreError::corrupt("runtime binding missing"))?;
        if !matches!(
            binding.permission_mode,
            PermissionMode::Ask | PermissionMode::Guarded
        ) {
            return Ok(false);
        }
        let context = crate::remember::context_digest(binding)?;
        let mut matched = std::collections::BTreeSet::new();
        for key in &offer.keys {
            let grant = view.remembered_grants.values().find(|g| {
                g.revoked_at_ms.is_none()
                    && g.context_digest == context
                    && g.offer.keys.iter().any(|candidate| {
                        candidate.kind == key.kind && candidate.digest == key.digest
                    })
            });
            let Some(grant) = grant else { return Ok(false) };
            matched.insert(grant.id.clone());
        }
        let update = RuntimeUpdate::ToolPolicy {
            tool_call_id: spec.tool_call_id.clone(),
            name: spec.tool_name.clone(),
            input: crate::approval::private_preview(&spec.input, redactions),
            arguments_digest: projection::checksum(&spec.input)?,
            allowed: true,
            reason: "Session remembered authorization".into(),
            authorization_ids: matched.into_iter().collect(),
        };
        self.apply_runtime_locked(&mut state, lease, event_id, update)?;
        Ok(true)
    }
}
