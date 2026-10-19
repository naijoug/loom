use super::*;

/// A single consumption of a persisted tool decision. It cannot be serialized,
/// cloned or reconstructed from a frontend response or an old process ID.
pub struct ToolPermit {
    core: Core,
    key: SessionKey,
    turn_id: Id,
    tool_call_id: Id,
    request_id: Id,
    digest: String,
    nonce: u64,
}
impl ToolPermit {
    pub fn tool_call_id(&self) -> &Id {
        &self.tool_call_id
    }
    pub fn request_id(&self) -> &Id {
        &self.request_id
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn is_live(&self) -> Result<bool> {
        let mut state = self.core.lock()?;
        if state.active.get(&self.key) != Some(&self.nonce) {
            return Ok(false);
        }
        let view = self.core.session(&mut state, &self.key)?.view();
        Ok(view.active_turn_id.as_ref() == Some(&self.turn_id)
            && view
                .turns
                .iter()
                .any(|t| t.id == self.turn_id && t.status == TurnStatus::Running)
            && view.approvals.get(&self.request_id).is_some_and(|a| {
                a.status == ApprovalStatus::Consumed && a.binding_digest == self.digest
            }))
    }
}
impl Core {
    pub fn record_tool_policy(
        &self,
        lease: &TurnLease,
        event_id: Id,
        spec: &ToolApprovalSpec,
        decision: ToolPolicyDecision,
        redactions: &[String],
    ) -> Result<u64> {
        self.apply_runtime(
            lease,
            event_id,
            RuntimeUpdate::ToolPolicy {
                tool_call_id: spec.tool_call_id.clone(),
                name: spec.tool_name.clone(),
                input: crate::approval::private_preview(&spec.input, redactions),
                arguments_digest: projection::checksum(&spec.input)?,
                allowed: decision.allowed,
                authorization_ids: vec![],
                reason: redactions
                    .iter()
                    .filter(|s| !s.is_empty())
                    .fold(decision.reason, |text, secret| {
                        text.replace(secret, "[credential redacted]")
                    }),
            },
        )
    }
    pub fn request_tool_approval(
        &self,
        lease: &TurnLease,
        spec: &ToolApprovalSpec,
    ) -> Result<ToolApproval> {
        self.request_tool_approval_redacted(lease, spec, &[])
    }
    pub fn request_tool_approval_redacted(
        &self,
        lease: &TurnLease,
        spec: &ToolApprovalSpec,
        redactions: &[String],
    ) -> Result<ToolApproval> {
        projection::validate_name(&spec.tool_name)?;
        if spec.ttl_ms == 0
            || spec.ttl_ms > 300_000
            || serde_json::to_vec(&spec.input)
                .map_err(|_| CoreError::invalid("invalid tool arguments"))?
                .len()
                > MAX_MESSAGE_BYTES
        {
            return Err(CoreError::invalid(
                "invalid approval TTL or arguments exceed 1 MiB",
            ));
        }
        let mut state = self.lock()?;
        self.check_lease(&state, lease)?;
        let view = self.session(&mut state, &lease.key)?.view();
        let turn = view
            .turns
            .iter()
            .find(|t| t.id == lease.turn_id && t.status == TurnStatus::Running)
            .ok_or_else(|| CoreError::conflict("approval owner is not running"))?;
        let binding = turn
            .binding
            .as_ref()
            .ok_or_else(|| CoreError::corrupt("approval owner lacks binding"))?;
        let arguments_digest = projection::checksum(&spec.input)?;
        let binding_digest = crate::remember::remember_binding(
            crate::approval::approval_binding(
                &lease.key,
                &lease.turn_id,
                &spec.request_id,
                &spec.tool_call_id,
                &spec.tool_name,
                &arguments_digest,
                binding,
            )?,
            spec.remember.as_ref(),
        )?;
        if let Some(existing) = view.approvals.get(&spec.request_id) {
            if existing.binding_digest != binding_digest {
                return Err(CoreError::conflict(
                    "approval request ID reused with different tool arguments or binding",
                ));
            }
            return Ok(existing.clone());
        }
        let now = journal::now_ms().max(view.metadata.updated_at_ms);
        let approval = ToolApproval {
            id: spec.request_id.clone(),
            turn_id: lease.turn_id.clone(),
            tool_call_id: spec.tool_call_id.clone(),
            tool_name: spec.tool_name.clone(),
            preview: crate::approval::private_preview(&spec.input, redactions),
            arguments_digest,
            binding_digest,
            working_directory: binding.working_directory.clone(),
            permission_mode: binding.permission_mode,
            policy_version: binding.policy_version,
            created_at_ms: now,
            expires_at_ms: now
                .checked_add(spec.ttl_ms)
                .ok_or_else(|| CoreError::invalid("approval deadline overflow"))?,
            resolved_at_ms: None,
            status: ApprovalStatus::Pending,
            remember: spec.remember.as_ref().map(|offer| RememberOffer {
                keys: offer
                    .keys
                    .iter()
                    .map(|key| RememberKey {
                        kind: key.kind,
                        digest: key.digest.clone(),
                        label: crate::approval::private_preview(
                            &serde_json::Value::String(key.label.clone()),
                            redactions,
                        )
                        .as_str()
                        .unwrap_or("授权范围")
                        .into(),
                    })
                    .collect(),
            }),
        };
        self.commit(
            &mut state,
            &lease.key,
            SessionEvent::ToolApprovalRequested {
                approval: approval.clone(),
            },
        )?;
        Ok(approval)
    }
    pub fn respond_tool_approval(
        &self,
        key: &SessionKey,
        request_id: &Id,
        expected_digest: &str,
        allowed: bool,
    ) -> Result<ToolApproval> {
        self.respond_tool_approval_with_memory(key, request_id, expected_digest, allowed, false)
    }
    pub fn respond_tool_approval_with_memory(
        &self,
        key: &SessionKey,
        request_id: &Id,
        expected_digest: &str,
        allowed: bool,
        remember: bool,
    ) -> Result<ToolApproval> {
        if remember && !allowed {
            return Err(CoreError::invalid(
                "denied approval cannot grant remembered authorization",
            ));
        }
        let mut state = self.lock()?;
        let approval = self
            .session(&mut state, key)?
            .view()
            .approvals
            .get(request_id)
            .cloned()
            .ok_or_else(|| CoreError::missing("tool approval not found"))?;
        if approval.binding_digest != expected_digest {
            return Err(CoreError::conflict(
                "tool approval digest mismatch; refresh the pending request",
            ));
        }
        if (allowed
            && matches!(
                approval.status,
                ApprovalStatus::Allowed | ApprovalStatus::Consumed
            ))
            || (!allowed && approval.status == ApprovalStatus::Denied)
        {
            if remember
                != view_remembered(&state.sessions[key].view().remembered_grants, request_id)
                && allowed
            {
                return Err(CoreError::conflict(
                    "approval already settled with a different remember choice",
                ));
            }
            return Ok(approval);
        }
        if !state.active.contains_key(key) {
            return Err(CoreError::new(
                ErrorCode::StaleLease,
                "tool approval has no live runtime owner",
            ));
        }
        let remembered_grant = if remember {
            let offer = approval
                .remember
                .clone()
                .ok_or_else(|| CoreError::invalid("approval has no remembered scope"))?;
            let binding = state.sessions[key]
                .view()
                .turns
                .iter()
                .find(|t| t.id == approval.turn_id)
                .and_then(|t| t.binding.as_ref())
                .ok_or_else(|| CoreError::corrupt("missing approval binding"))?;
            Some(RememberedGrant {
                id: request_id.clone(),
                offer,
                context_digest: crate::remember::context_digest(binding)?,
                created_at_ms: journal::now_ms()
                    .max(state.sessions[key].view().metadata.updated_at_ms),
                revoked_at_ms: None,
            })
        } else {
            None
        };
        self.commit(
            &mut state,
            key,
            SessionEvent::ToolApprovalResolved {
                request_id: request_id.clone(),
                remembered_grant,
                resolution: if allowed && remember {
                    ApprovalResolution::AllowRemember
                } else if allowed {
                    ApprovalResolution::Allow
                } else {
                    ApprovalResolution::Deny
                },
            },
        )?;
        if remember {
            state.live_grants.insert((key.clone(), request_id.clone()));
        }
        Ok(state.sessions[key].view().approvals[request_id].clone())
    }
    pub fn consume_tool_approval(
        &self,
        lease: &TurnLease,
        spec: &ToolApprovalSpec,
    ) -> Result<ToolPermit> {
        let mut state = self.lock()?;
        self.check_lease(&state, lease)?;
        let view = self.session(&mut state, &lease.key)?.view();
        let approval = view
            .approvals
            .get(&spec.request_id)
            .ok_or_else(|| CoreError::missing("tool approval not found"))?;
        let binding = view
            .turns
            .iter()
            .find(|t| t.id == lease.turn_id)
            .and_then(|t| t.binding.as_ref())
            .ok_or_else(|| CoreError::corrupt("approval owner lacks binding"))?;
        let actual = crate::remember::remember_binding(
            crate::approval::approval_binding(
                &lease.key,
                &lease.turn_id,
                &spec.request_id,
                &spec.tool_call_id,
                &spec.tool_name,
                &projection::checksum(&spec.input)?,
                binding,
            )?,
            spec.remember.as_ref(),
        )?;
        if actual != approval.binding_digest {
            return Err(CoreError::conflict(
                "tool arguments or execution binding changed after approval",
            ));
        }
        self.commit(
            &mut state,
            &lease.key,
            SessionEvent::ToolApprovalConsumed {
                request_id: spec.request_id.clone(),
            },
        )?;
        Ok(ToolPermit {
            core: self.clone(),
            key: lease.key.clone(),
            turn_id: lease.turn_id.clone(),
            tool_call_id: spec.tool_call_id.clone(),
            request_id: spec.request_id.clone(),
            digest: actual,
            nonce: lease.nonce,
        })
    }
}
fn view_remembered(grants: &BTreeMap<Id, RememberedGrant>, id: &Id) -> bool {
    grants.contains_key(id)
}
