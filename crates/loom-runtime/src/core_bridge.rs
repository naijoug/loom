use crate::*;
use loom_core::{
    ApprovalStatus, Core, FinishStatus, Id, PermissionMode, Role, RuntimeContext, RuntimeUpdate,
    TextKind, TokenUsage, ToolApprovalSpec, ToolPolicyDecision, ToolStatus, TurnLease, TurnStatus,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc},
};
use zeroize::Zeroizing;

pub enum Authorization {
    Allow { reason: String },
    Deny { reason: String },
    Ask,
}

/// Trusted host execution bridge for an approved external tool. The worker
/// never receives an executable path or transport; it only receives a result.
pub trait ExternalToolRunner {
    fn call(&mut self, name: &str, input: &Value) -> std::result::Result<Value, RuntimeError>;
}

impl<T: ExternalToolRunner + ?Sized> ExternalToolRunner for &mut T {
    fn call(&mut self, name: &str, input: &Value) -> std::result::Result<Value, RuntimeError> {
        (**self).call(name, input)
    }
}
/// Required trusted policy. No default policy: calling code must classify tools
/// against the frozen mode/version. Never pass raw arguments into audit logs.
pub trait ToolPolicy {
    fn remember_offer(
        &self,
        _context: &RuntimeContext,
        _name: &str,
        _input: &Value,
    ) -> Result<Option<loom_core::RememberOffer>, RuntimeError> {
        Ok(None)
    }
    fn validate_context(&self, _context: &RuntimeContext) -> Result<(), RuntimeError> {
        Ok(())
    }
    fn evaluate(
        &mut self,
        context: &RuntimeContext,
        name: &str,
        input: &Value,
    ) -> Result<Authorization, RuntimeError>;
}
pub struct SessionRunOptions {
    pub run_id: Id,
    pub tools: Vec<String>,
    pub max_turns: u32,
    pub context_bytes: usize,
    pub external_tools: Vec<crate::protocol::ExternalTool>,
}
fn core_error(error: loom_core::CoreError) -> RuntimeError {
    RuntimeError::new("core", error.to_string())
}

/// Consumes a lease, records exactly one terminal outcome after process cleanup.
/// Dropping the lease on any early error still interrupts through production core.
#[allow(clippy::too_many_arguments)]
pub fn run_session(
    core: &Core,
    lease: TurnLease,
    command: &WorkerCommand,
    options: SessionRunOptions,
    credential: Option<&str>,
    stop: Arc<AtomicBool>,
    policy: &mut impl ToolPolicy,
    external_runner: Option<&mut dyn ExternalToolRunner>,
) -> Result<RunOutcome, RuntimeError> {
    let context = core
        .runtime_context(&lease, options.context_bytes)
        .map_err(core_error)?;
    let result =
        (|| {
            policy.validate_context(&context)?;
            let config =
                RunConfig {
                    scope: Scope {
                        workspace_id: lease.key().workspace_id.clone(),
                        session_id: lease.key().session_id.clone(),
                        turn_id: lease.turn_id().clone(),
                        run_id: options.run_id.clone(),
                    },
                    connection: context.connection.config.clone(),
                    model: context.binding.connection.model.clone().ok_or_else(|| {
                        RuntimeError::new("configuration", "accepted model missing")
                    })?,
                    thinking: context.binding.connection.thinking.clone(),
                    cwd: context.working_directory.clone(),
                    runtime_directory: context.runtime_directory.clone(),
                    prompt: context.prompt.clone(),
                    history: context
                        .history
                        .iter()
                        .map(|m| HistoryMessage {
                            role: if m.role == Role::User {
                                "user"
                            } else {
                                "assistant"
                            }
                            .into(),
                            text: m.text.clone(),
                        })
                        .collect(),
                    tools: options.tools,
                    external_tools: options.external_tools,
                    max_turns: options.max_turns,
                };
            run(
                command,
                &config,
                credential,
                stop,
                &mut CoreHandler {
                    core,
                    lease: &lease,
                    context: &context,
                    policy,
                    pending: BTreeMap::new(),
                    queued: vec![],
                    sequence: 0,
                    run_id: format!("{:x}", Sha256::digest(options.run_id.as_str().as_bytes())),
                    redactions: Zeroizing::new(crate::supervisor::credential_forms(
                        credential.unwrap_or_default(),
                    )),
                    external_runner: external_runner.map(|runner| Box::new(runner) as _),
                },
            )
        })()
        .map_err(|error: RuntimeError| {
            RuntimeError::new(
                error.code,
                crate::supervisor::clean_text(&error.message, credential)
                    .chars()
                    .take(2048)
                    .collect::<String>(),
            )
        });
    let (status, error) = if matches!(result, Ok(RunOutcome::Cancelled)) {
        (FinishStatus::Cancelled, None)
    } else {
        match &result {
            Ok(_) => (FinishStatus::Completed, None),
            Err(e) => (FinishStatus::Failed, Some(e.to_string())),
        }
    };
    let view = core
        .settle_runtime_turn(&lease, status, error)
        .map_err(core_error)?;
    if view
        .turns
        .iter()
        .any(|t| &t.id == lease.turn_id() && t.status == TurnStatus::Cancelled)
    {
        Ok(RunOutcome::Cancelled)
    } else {
        result
    }
}
struct CoreHandler<'a, P> {
    core: &'a Core,
    lease: &'a TurnLease,
    context: &'a RuntimeContext,
    policy: &'a mut P,
    pending: BTreeMap<Id, ToolApprovalSpec>,
    queued: Vec<ToolDecision>,
    sequence: u64,
    run_id: String,
    redactions: Zeroizing<Vec<String>>,
    external_runner: Option<Box<dyn ExternalToolRunner + 'a>>,
}

impl<P: ToolPolicy> CoreHandler<'_, P> {
    fn event_id(&mut self) -> Result<Id, RuntimeError> {
        self.sequence += 1;
        Id::new(format!("runtime-{}-{}", self.run_id, self.sequence)).map_err(core_error)
    }
    fn update(&mut self, update: RuntimeUpdate) -> Result<(), RuntimeError> {
        let id = self.event_id()?;
        self.core
            .apply_runtime(self.lease, id, update)
            .map_err(core_error)?;
        Ok(())
    }
}
impl<P: ToolPolicy> Handler for CoreHandler<'_, P> {
    fn event(&mut self, event: &WorkerEvent) -> Result<(), RuntimeError> {
        match event {
            WorkerEvent::Ready => {}
            WorkerEvent::Text {
                part_id,
                text_kind,
                delta,
            } => self.update(RuntimeUpdate::TextDelta {
                part_id: part_id.clone(),
                text_kind: if text_kind == "thinking" {
                    TextKind::Thinking
                } else {
                    TextKind::Text
                },
                delta: delta.clone(),
            })?,
            WorkerEvent::Usage {
                input_tokens,
                output_tokens,
            } => self.update(RuntimeUpdate::Usage {
                usage: TokenUsage {
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                },
            })?,
            WorkerEvent::ToolResult {
                tool_call_id,
                failed,
                output,
            } => self.update(RuntimeUpdate::ToolFinished {
                tool_call_id: tool_call_id.clone(),
                status: if *failed {
                    ToolStatus::Failed
                } else {
                    ToolStatus::Succeeded
                },
                output: output.clone(),
            })?,
            WorkerEvent::ToolRequest {
                request_id,
                tool_call_id,
                name,
                input,
            } => {
                let mut spec = ToolApprovalSpec {
                    request_id: request_id.clone(),
                    tool_call_id: tool_call_id.clone(),
                    tool_name: name.clone(),
                    input: input.clone(),
                    ttl_ms: 240_000,
                    remember: None,
                };
                match self.policy.evaluate(self.context, name, input)? {
                    Authorization::Ask => {
                        spec.remember = self.policy.remember_offer(self.context, name, input)?;
                        let event_id = self.event_id()?;
                        if self
                            .core
                            .try_remembered_tool(self.lease, event_id, &spec, &self.redactions)
                            .map_err(core_error)?
                        {
                            self.queued.push(ToolDecision {
                                request_id: request_id.clone(),
                                tool_call_id: tool_call_id.clone(),
                                allowed: true,
                                output: None,
                            });
                            return Ok(());
                        }
                        if !matches!(
                            self.context.binding.permission_mode,
                            PermissionMode::Ask | PermissionMode::Guarded
                        ) {
                            return Err(RuntimeError::new(
                                "policy",
                                "interactive authorization is unavailable in this permission mode",
                            ));
                        }
                        self.core
                            .request_tool_approval_redacted(self.lease, &spec, &self.redactions)
                            .map_err(core_error)?;
                        self.pending.insert(request_id.clone(), spec);
                    }
                    decision => {
                        let (allowed, reason) = match decision {
                            Authorization::Allow { reason } => (true, reason),
                            Authorization::Deny { reason } => (false, reason),
                            _ => unreachable!(),
                        };
                        let id = self.event_id()?;
                        self.core
                            .record_tool_policy(
                                self.lease,
                                id,
                                &spec,
                                ToolPolicyDecision { allowed, reason },
                                &self.redactions,
                            )
                            .map_err(core_error)?;
                        self.queued.push(ToolDecision {
                            request_id: request_id.clone(),
                            tool_call_id: tool_call_id.clone(),
                            allowed,
                            output: None,
                        });
                    }
                }
            }
            WorkerEvent::Completed { .. } => {}
            WorkerEvent::Failed { .. } => {
                return Err(RuntimeError::new(
                    "protocol",
                    "failure must be handled by supervisor",
                ))
            }
        }
        Ok(())
    }
    fn decisions(&mut self) -> Result<Vec<ToolDecision>, RuntimeError> {
        if self.lease.cancel_requested().map_err(core_error)? {
            return Ok(vec![]);
        }
        if self.pending.is_empty() {
            return Ok(std::mem::take(&mut self.queued));
        }
        let view = self
            .core
            .get_session(self.lease.key())
            .map_err(core_error)?;
        let mut settled = Vec::new();
        for (id, spec) in &self.pending {
            let approval = view
                .approvals
                .get(id)
                .ok_or_else(|| RuntimeError::new("core", "pending tool approval missing"))?;
            match approval.status {
                ApprovalStatus::Pending => continue,
                ApprovalStatus::Allowed => {
                    let permit = self
                        .core
                        .consume_tool_approval(self.lease, spec)
                        .map_err(core_error)?;
                    if !permit.is_live().map_err(core_error)? {
                        return Err(RuntimeError::new("cancelled", "tool permit was revoked"));
                    }
                    self.queued.push(ToolDecision {
                        request_id: id.clone(),
                        tool_call_id: spec.tool_call_id.clone(),
                        allowed: true,
                        output: None,
                    });
                }
                ApprovalStatus::Consumed => {
                    return Err(RuntimeError::new(
                        "core",
                        "tool permit already consumed by another handler",
                    ))
                }
                _ => self.queued.push(ToolDecision {
                    request_id: id.clone(),
                    tool_call_id: spec.tool_call_id.clone(),
                    allowed: false,
                    output: None,
                }),
            }
            settled.push(id.clone());
        }
        for id in settled {
            self.pending.remove(&id);
        }
        Ok(std::mem::take(&mut self.queued))
    }
    fn cancelled(&self) -> bool {
        self.lease.cancel_requested().unwrap_or(true)
    }
    fn external_tool(&mut self, request: &ExternalToolRequest<'_>) -> Result<Value, RuntimeError> {
        let Some(runner) = self.external_runner.as_deref_mut() else {
            return Err(RuntimeError::new(
                "external_tool",
                "external tool runner is unavailable",
            ));
        };
        runner.call(request.name, request.input)
    }
}
