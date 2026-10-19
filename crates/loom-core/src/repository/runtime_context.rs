use super::*;

/// Host data, never available through frontend commands. Excludes secrets,
/// native handles, process IDs and future queued inputs.
pub struct RuntimeContext {
    pub binding: RuntimeBinding,
    pub connection: ModelConnection,
    pub prompt: String,
    pub history: Vec<ContextMessage>,
    pub working_directory: String,
    pub runtime_directory: String,
}
pub struct ContextMessage {
    pub role: Role,
    pub text: String,
}
impl Core {
    /// Resolve the directory before claiming a turn so the lease and every
    /// authorization bind the actual SDK cwd, including projectless sessions.
    pub fn runtime_working_directory(&self, key: &SessionKey) -> Result<String> {
        let path = {
            let mut state = self.lock()?;
            let project = self
                .session(&mut state, key)?
                .view()
                .metadata
                .project_id
                .clone();
            if let Some(project) = project {
                PathBuf::from(&state.catalog.projects[&key.workspace_id][&project].directory)
            } else {
                let path = self.session_directory(key)?.join("work");
                journal::directory(&path)?;
                path
            }
        };
        let actual = path.canonicalize()?;
        if actual != path || !actual.is_dir() {
            return Err(CoreError::conflict("runtime working directory changed"));
        }
        actual
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| CoreError::invalid("runtime path is not UTF-8"))
    }

    pub fn runtime_context(&self, lease: &TurnLease, max_bytes: usize) -> Result<RuntimeContext> {
        let max_bytes = max_bytes.min(MAX_MESSAGE_BYTES);
        let (view, turn, binding, directory) = {
            let mut state = self.lock()?;
            self.check_lease(&state, lease)?;
            let view = self.session(&mut state, lease.key())?.view().clone();
            let turn = view
                .turns
                .iter()
                .find(|t| &t.id == lease.turn_id() && t.status == TurnStatus::Running)
                .cloned()
                .ok_or_else(|| CoreError::conflict("runtime context owner is not running"))?;
            let binding = turn
                .binding
                .clone()
                .ok_or_else(|| CoreError::corrupt("runtime binding missing"))?;
            (view, turn, binding, self.session_directory(lease.key())?)
        };
        let connection = turn
            .configuration
            .resolved_connection
            .as_deref()
            .cloned()
            .ok_or_else(|| CoreError::invalid("runtime requires a frozen model connection"))?;
        let prompt = context_text(
            view.messages
                .iter()
                .find(|m| m.id == turn.receipt.user_message_id)
                .ok_or_else(|| CoreError::corrupt("accepted user message missing"))?,
            &view,
        )?;
        let mut used = prompt.len();
        let mut history = Vec::new();
        let mut add = |role: Role, text: String| -> Result<()> {
            used = used
                .checked_add(text.len() + 32)
                .ok_or_else(|| CoreError::invalid("context byte count overflow"))?;
            if used > max_bytes {
                return Err(CoreError::invalid("conversation exceeds runtime context budget; compact or branch before continuing"));
            }
            if !text.is_empty() {
                history.push(ContextMessage { role, text });
            }
            Ok(())
        };
        if let Some(import) = &view.imported_history {
            let mut offset = 0;
            while offset < import.message_count {
                let page = self.read_import_history(lease.key(), offset, 100)?;
                if page.messages.is_empty() {
                    return Err(CoreError::corrupt("imported context page made no progress"));
                }
                offset += page.messages.len();
                for message in page.messages {
                    let had_content = !message.content.is_empty();
                    let mut text = message.content;
                    for part in message.parts {
                        match part {
                            HistoricalPart::Text { text: part } if !had_content => {
                                text.push_str(&part)
                            }
                            HistoricalPart::Tool {
                                name,
                                input_summary,
                                output_summary,
                                status,
                            } => {
                                let record = serde_json::json!({"name": name, "input": input_summary, "output": output_summary, "status": status});
                                text.push_str(&format!("\n[Historical tool record] {record}"));
                            }
                            HistoricalPart::Error { message, .. } => {
                                text.push_str(&format!("\n[Historical error] {message}"))
                            }
                            _ => {}
                        }
                    }
                    if matches!(
                        message.status.as_str(),
                        "interrupted" | "failed" | "cancelled"
                    ) {
                        text.push_str(&format!(
                            "\n[Historical message status: {}]",
                            message.status
                        ));
                    }
                    add(message.role, text)?;
                }
            }
        }
        for message in &view.messages {
            if message.id == turn.receipt.user_message_id {
                break;
            }
            let Some(owner) = view.turns.iter().find(|t| {
                t.id == message.turn_id
                    && t.status.is_terminal()
                    && t.receipt.accepted_seq < turn.receipt.accepted_seq
            }) else {
                continue;
            };
            let mut text = context_text(message, &view)?;
            if message.role == Role::Assistant && owner.status != TurnStatus::Completed {
                text.push_str(&format!("\n[Previous turn ended: {:?}]", owner.status));
            }
            add(message.role, text)?;
        }
        if used > max_bytes {
            return Err(CoreError::invalid(
                "current input exceeds runtime context budget",
            ));
        }
        let runtime_base = directory.join("runtime");
        journal::directory(&runtime_base)?;
        let runtime_directory = runtime_base.join(lease.turn_id().as_str());
        journal::directory(&runtime_directory)?;
        let working_directory = match &binding.working_directory {
            Some(path) => {
                let actual = Path::new(path).canonicalize()?;
                if actual != Path::new(path) || !actual.is_dir() {
                    return Err(CoreError::conflict("runtime project directory changed"));
                }
                actual
            }
            None => {
                return Err(CoreError::invalid(
                    "SDK execution requires an explicit working-directory binding",
                ))
            }
        };
        {
            let mut state = self.lock()?;
            self.check_lease(&state, lease)?;
            if self
                .session(&mut state, lease.key())?
                .view()
                .turns
                .iter()
                .find(|t| &t.id == lease.turn_id())
                .is_none_or(|t| t.status != TurnStatus::Running)
            {
                return Err(CoreError::conflict(
                    "runtime was cancelled while preparing context",
                ));
            }
        }
        Ok(RuntimeContext {
            binding,
            connection,
            prompt,
            history,
            working_directory: working_directory
                .to_str()
                .ok_or_else(|| CoreError::invalid("runtime path is not UTF-8"))?
                .into(),
            runtime_directory: runtime_directory
                .to_str()
                .ok_or_else(|| CoreError::invalid("runtime path is not UTF-8"))?
                .into(),
        })
    }
}
fn context_text(message: &Message, view: &SessionView) -> Result<String> {
    let mut text = String::new();
    for part in &message.parts {
        match part {
            MessagePart::Text {
                kind: TextKind::Text,
                text: part,
                ..
            } => text.push_str(part),
            MessagePart::Text {
                kind: TextKind::Thinking,
                ..
            } => {}
            MessagePart::Tool { tool_call_id } => {
                let tool = view
                    .tools
                    .get(tool_call_id)
                    .ok_or_else(|| CoreError::corrupt("historical tool record missing"))?;
                text.push_str(&format!(
                    "\n[Previous tool record] {}",
                    serde_json::to_string(tool)
                        .map_err(|_| CoreError::corrupt("cannot encode historical tool"))?
                ));
            }
            MessagePart::Attachment { .. } => {
                return Err(CoreError::invalid(
                    "runtime attachment context is not connected",
                ))
            }
        }
    }
    Ok(text)
}
