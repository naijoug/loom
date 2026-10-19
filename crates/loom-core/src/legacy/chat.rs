use super::{source::MAX_FILE, *};
use std::collections::BTreeSet;

pub(super) fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::corrupt(format!("legacy field {key} must be a string")))
}
fn number(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .filter(|n| *n <= crate::MAX_SAFE_SEQUENCE)
        .ok_or_else(|| {
            CoreError::corrupt(format!(
                "legacy field {key} must be a safe unsigned integer"
            ))
        })
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::corrupt(format!("legacy field {key} must be an array")))
}
fn optional_array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    match value.get(key) {
        None => Ok(&[]),
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| CoreError::corrupt(format!("legacy field {key} must be an array"))),
    }
}
fn identifier(value: &Value, key: &str) -> Result<String> {
    let id = string(value, key)?;
    Id::new(id)?;
    Ok(id.into())
}
fn validate_message(message: &Value) -> Result<()> {
    identifier(message, "id")?;
    if !matches!(string(message, "role")?, "user" | "assistant") {
        return Err(CoreError::corrupt("unknown legacy message role"));
    }
    string(message, "content")?;
    string(message, "status")?;
    number(message, "createdAtMs")?;
    for part in optional_array(message, "parts")? {
        match string(part, "type")? {
            "text" => {
                string(part, "text")?;
            }
            "tool" => {
                string(part, "name")?;
            }
            "error" => {
                string(part, "message")?;
            }
            _ => return Err(CoreError::corrupt("unknown legacy message part")),
        }
    }
    Ok(())
}
pub(super) fn validate(value: &Value, project: &str, id: &str, version: u64) -> Result<()> {
    if string(value, "id")? != id || string(value, "projectPath")? != project {
        return Err(CoreError::corrupt(
            "legacy document identity does not match project/session",
        ));
    }
    if value.get("schemaVersion").and_then(Value::as_u64) != Some(version) {
        return Err(CoreError::corrupt("unsupported legacy chat schema"));
    }
    string(value, "title")?;
    string(value, "agentId")?;
    number(value, "createdAtMs")?;
    number(value, "updatedAtMs")?;
    let mut messages = BTreeMap::new();
    for message in array(value, "messages")? {
        validate_message(message)?;
        if messages
            .insert(string(message, "id")?, string(message, "role")?)
            .is_some()
        {
            return Err(CoreError::corrupt("duplicate legacy message ID"));
        }
    }
    let mut turn_ids = BTreeSet::new();
    let mut requests = BTreeSet::new();
    let mut owned_messages = BTreeSet::new();
    let turns = optional_array(value, "turns")?;
    for turn in turns {
        if !turn_ids.insert(identifier(turn, "id")?)
            || !requests.insert(identifier(turn, "clientRequestId")?)
        {
            return Err(CoreError::corrupt("duplicate legacy turn/request ID"));
        }
        for (field, role) in [
            ("userMessageId", "user"),
            ("assistantMessageId", "assistant"),
        ] {
            let message = identifier(turn, field)?;
            if messages.get(message.as_str()) != Some(&role) || !owned_messages.insert(message) {
                return Err(CoreError::corrupt("legacy turn message ownership mismatch"));
            }
        }
        if !matches!(
            string(turn, "status")?,
            "starting"
                | "running"
                | "cancelling"
                | "completed"
                | "failed"
                | "cancelled"
                | "timed_out"
                | "interrupted"
        ) {
            return Err(CoreError::corrupt("unknown legacy turn status"));
        }
        number(turn, "acceptedAtMs")?;
        let terminal = !matches!(
            string(turn, "status")?,
            "starting" | "running" | "cancelling"
        );
        if terminal != turn.get("finishedAtMs").is_some_and(|v| !v.is_null()) {
            return Err(CoreError::corrupt(
                "legacy turn terminal timestamp mismatch",
            ));
        }
        for field in ["startedAtMs", "finishedAtMs"] {
            if turn.get(field).is_some_and(|v| !v.is_null()) {
                number(turn, field)?;
            }
        }
    }
    let mut receipts = BTreeSet::new();
    let mut receipt_turns = BTreeSet::new();
    for receipt in optional_array(value, "sendReceipts")? {
        let request = identifier(receipt, "clientRequestId")?;
        if !receipts.insert(request.clone())
            || !receipt_turns.insert(identifier(receipt, "turnId")?)
        {
            return Err(CoreError::corrupt("duplicate legacy send receipt"));
        }
        for (field, role) in [
            ("userMessageId", "user"),
            ("assistantMessageId", "assistant"),
        ] {
            if messages.get(string(receipt, field)?) != Some(&role) {
                return Err(CoreError::corrupt(
                    "legacy receipt references missing/wrong-role message",
                ));
            }
        }
        // Older v1 documents can have receipts without durable turns.
        if !turns.is_empty()
            && !turns.iter().any(|t| {
                t.get("clientRequestId") == receipt.get("clientRequestId")
                    && t.get("id") == receipt.get("turnId")
                    && t.get("userMessageId") == receipt.get("userMessageId")
                    && t.get("assistantMessageId") == receipt.get("assistantMessageId")
            })
        {
            return Err(CoreError::corrupt("legacy receipt/turn mismatch"));
        }
    }
    if let Some(active) = value.get("activeTurnId").filter(|v| !v.is_null()) {
        let active = active
            .as_str()
            .ok_or_else(|| CoreError::corrupt("invalid legacy active turn"))?;
        if !turns.is_empty() && !turn_ids.contains(active) {
            return Err(CoreError::corrupt("missing legacy active turn"));
        }
    }
    Ok(())
}

pub(super) fn replay(
    bytes: &[u8],
    project: &str,
    id: &str,
    issues: &mut Vec<LegacyIssue>,
) -> Result<Value> {
    let end = bytes
        .iter()
        .rposition(|c| *c == b'\n')
        .map(|n| n + 1)
        .unwrap_or(0);
    if end < bytes.len() {
        issues.push(LegacyIssue::warning(
            "incomplete_tail",
            "events.jsonl",
            "未提交的末尾字节不参与回放；源文件保持原样",
        ));
    }
    let mut session = Value::Null;
    let mut seq = 0;
    for line in bytes[..end].split_inclusive(|c| *c == b'\n') {
        if line.len() >= MAX_FILE {
            return Err(CoreError::corrupt("legacy journal record exceeds budget"));
        }
        let event: Value = parse(line)?;
        seq += 1;
        if number(&event, "schemaVersion")? != 2
            || number(&event, "seq")? != seq
            || string(&event, "projectKey")? != project
            || string(&event, "sessionId")? != id
        {
            return Err(CoreError::corrupt(
                "legacy journal version, sequence or scope mismatch",
            ));
        }
        number(&event, "timestampMs")?;
        let payload = event
            .get("payload")
            .ok_or_else(|| CoreError::corrupt("legacy event payload missing"))?;
        match (seq, string(&event, "kind")?) {
            (1, "session_created") => {
                session = payload.clone();
                validate(&session, project, id, 2)?;
                if number(&session, "lastSeq")? != 1 || number(&session, "revision")? != 1 {
                    return Err(CoreError::corrupt("invalid legacy creation sequence"));
                }
            }
            (1, _) => return Err(CoreError::corrupt("legacy journal lacks creation record")),
            (_, "session_patch") => {
                let before = session.clone();
                let fields = payload
                    .get("fields")
                    .and_then(Value::as_object)
                    .ok_or_else(|| CoreError::corrupt("invalid legacy patch fields"))?;
                for (field, value) in fields {
                    if ![
                        "title",
                        "agentId",
                        "permissionMode",
                        "updatedAtMs",
                        "resumeCommand",
                        "resumeHandle",
                        "activeTurnId",
                        "turnStatus",
                        "promotedTaskId",
                        "status",
                        "flagged",
                        "sendReceipts",
                        "turns",
                    ]
                    .contains(&field.as_str())
                    {
                        return Err(CoreError::corrupt(
                            "legacy patch modifies immutable/unknown field",
                        ));
                    }
                    session[field] = value.clone();
                }
                let messages = session
                    .get_mut("messages")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| CoreError::corrupt("missing legacy messages"))?;
                for changed in optional_array(payload, "changedMessages")? {
                    validate_message(changed)?;
                    let found = messages
                        .iter_mut()
                        .find(|m| m.get("id") == changed.get("id"))
                        .ok_or_else(|| {
                            CoreError::corrupt("legacy patch changes missing message")
                        })?;
                    *found = changed.clone();
                }
                for appended in optional_array(payload, "appendedMessages")? {
                    validate_message(appended)?;
                    messages.push(appended.clone());
                }
                validate(&session, project, id, 2)?;
                validate_transition(&before, &session)?;
            }
            (_, "stream") => {
                if string(payload, "sessionId")? != id
                    || session.get("activeTurnId").and_then(Value::as_str)
                        != Some(string(payload, "turnId")?)
                {
                    return Err(CoreError::corrupt(
                        "legacy stream belongs to another session/turn",
                    ));
                }
                let message_id = string(payload, "messageId")?;
                let message = session["messages"]
                    .as_array_mut()
                    .and_then(|messages| {
                        messages
                            .iter_mut()
                            .find(|m| m["id"] == message_id && m["role"] == "assistant")
                    })
                    .ok_or_else(|| CoreError::corrupt("legacy stream assistant missing"))?;
                let done = payload
                    .get("done")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| CoreError::corrupt("invalid legacy stream done flag"))?;
                let delta = string(payload, "delta")?;
                if !done {
                    message["content"] =
                        Value::String(format!("{}{delta}", string(message, "content")?));
                }
                if let Some(part) = payload.get("part").filter(|p| !p.is_null()) {
                    if message.get("parts").is_none() {
                        message["parts"] = serde_json::json!([]);
                    }
                    message["parts"]
                        .as_array_mut()
                        .ok_or_else(|| CoreError::corrupt("invalid legacy parts"))?
                        .push(part.clone());
                }
                validate_message(message)?;
                session["updatedAtMs"] = event["timestampMs"].clone();
            }
            _ => {
                return Err(CoreError::corrupt(
                    "duplicate creation or unknown legacy journal event",
                ))
            }
        }
        session["lastSeq"] = seq.into();
        session["revision"] = seq.into();
    }
    if seq == 0 {
        return Err(CoreError::corrupt(
            "legacy journal has no committed creation record",
        ));
    }
    Ok(session)
}

fn validate_transition(before: &Value, after: &Value) -> Result<()> {
    let old_receipts = optional_array(before, "sendReceipts")?;
    if !optional_array(after, "sendReceipts")?.starts_with(old_receipts) {
        return Err(CoreError::corrupt("legacy accepted receipts are immutable"));
    }
    for receipt in old_receipts {
        let old = array(before, "messages")?
            .iter()
            .find(|m| m.get("id") == receipt.get("userMessageId"));
        let new = array(after, "messages")?
            .iter()
            .find(|m| m.get("id") == receipt.get("userMessageId"));
        if old.and_then(|m| m.get("content")) != new.and_then(|m| m.get("content")) {
            return Err(CoreError::corrupt(
                "legacy accepted user content is immutable",
            ));
        }
    }
    let next = optional_array(after, "turns")?;
    for (index, old) in optional_array(before, "turns")?.iter().enumerate() {
        let turn = next
            .get(index)
            .ok_or_else(|| CoreError::corrupt("legacy turns cannot be removed"))?;
        for field in [
            "id",
            "clientRequestId",
            "userMessageId",
            "assistantMessageId",
            "invocation",
            "acceptedAtMs",
            "stdoutLogRef",
            "stderrLogRef",
        ] {
            if old.get(field) != turn.get(field) {
                return Err(CoreError::corrupt(
                    "legacy turn identity/invocation is immutable",
                ));
            }
        }
        for field in ["startedAtMs", "processId"] {
            if old.get(field).is_some_and(|v| !v.is_null()) && old.get(field) != turn.get(field) {
                return Err(CoreError::corrupt(
                    "legacy started turn identity is immutable",
                ));
            }
        }
        match (string(old, "status")?, string(turn, "status")?) {
            ("starting", _) => {}
            ("running", "starting") | ("cancelling", "starting" | "running") => {
                return Err(CoreError::corrupt(
                    "legacy turn state cannot move backwards",
                ))
            }
            ("running" | "cancelling", _) => {}
            _ if old != turn => {
                return Err(CoreError::corrupt("legacy terminal turn is immutable"))
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn load(
    source: &mut source::Source,
    path: &str,
    id: &str,
    version: u32,
    issues: &mut Vec<LegacyIssue>,
) -> Result<Value> {
    let bytes = source.required(path)?;
    if version == 1 {
        let value = parse(&bytes)?;
        validate(&value, &source.project, id, 1)?;
        return Ok(value);
    }
    let value = replay(&bytes, &source.project, id, issues)?;
    let snapshot_path = format!(
        "{}/snapshot.json",
        path.rsplit_once('/').expect("known journal path").0
    );
    if let Some(bytes) = source.read(&snapshot_path)? {
        match parse(&bytes) {
            Ok(snapshot) => {
                // A saved sequence ahead of the journal is evidence of lost history,
                // even when another snapshot field is malformed.
                if snapshot
                    .get("lastSeq")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n > value["lastSeq"].as_u64().unwrap())
                {
                    return Err(CoreError::corrupt(
                        "legacy journal shorter than saved snapshot",
                    ));
                }
                if snapshot.get("schemaVersion").is_some_and(|v| v != 2)
                    || snapshot.get("id").is_some_and(|v| v != id)
                    || snapshot
                        .get("projectPath")
                        .is_some_and(|v| v != &source.project)
                {
                    return Err(CoreError::corrupt("legacy snapshot version/scope mismatch"));
                }
                if snapshot != value {
                    issues.push(LegacyIssue::warning(
                        "stale_snapshot",
                        &snapshot_path,
                        "快照与日志不一致；以日志回放为准，未重写快照",
                    ));
                }
            }
            Err(_) => issues.push(LegacyIssue::warning(
                "invalid_snapshot",
                &snapshot_path,
                "快照不可解析；以完整日志回放为准，未重写快照",
            )),
        }
    }
    Ok(value)
}
