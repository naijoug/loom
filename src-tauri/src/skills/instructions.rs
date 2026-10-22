//! Shared instruction ownership is a set of installations, not a reference count.
//! Planning is read-only. File, ledger and manifest edits are committed together
//! by the installation transaction; this module never mutates live resources.
use super::{
    catalog,
    error::{ErrorCode, Result, SkillsError},
    files,
    installers::{Installation, ManagedResource, ResolvedPackage},
    package::{self, EntryState},
    repository::Repository,
    targets::{InstallTarget, TargetContext},
    transactions::{self, Change},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Instruction,
    Wrapper,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Owner {
    pub installation_id: String,
    pub manifest_path: PathBuf,
    pub install_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ledger {
    schema_version: u32,
    path: PathBuf,
    marker: String,
    kind: ResourceKind,
    content_digest: String,
    owners: BTreeMap<String, Owner>,
}
#[derive(Debug, Clone)]
pub struct DesiredResource {
    pub path: PathBuf,
    pub marker: String,
    pub kind: ResourceKind,
    /// Instruction content without markers, or the entire wrapper.
    pub content: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePreview {
    pub path: PathBuf,
    pub before: EntryState,
}
#[derive(Debug, Clone)]
pub struct Edit {
    pub path: PathBuf,
    pub before: EntryState,
    pub content: Option<Vec<u8>>,
    pub mode: u32,
}
pub struct ResourcePlan {
    pub resources: Vec<ManagedResource>,
    pub edits: Vec<Edit>,
}
impl ResourcePlan {
    pub fn preview(&self) -> Vec<ResourcePreview> {
        self.edits
            .iter()
            .map(|e| ResourcePreview {
                path: e.path.clone(),
                before: e.before.clone(),
            })
            .collect()
    }
    /// Only stage files; the caller must include these changes with the skill
    /// and install manifest in one immutable transaction prepare record.
    pub fn stage(&self, transaction_id: &str, offset: usize) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        for (i, edit) in self.edits.iter().enumerate() {
            if package::state(&edit.path)? != edit.before {
                return Err(conflict("Instruction resource changed after preview"));
            }
            ensure_parent(
                edit.path
                    .parent()
                    .ok_or_else(|| SkillsError::invalid("Missing resource parent"))?,
            )?;
            transactions::prepare_storage(&edit.path, transaction_id)?;
            let staged = transactions::artifact(&edit.path, transaction_id, offset + i, "staged")?;
            let backup = transactions::artifact(&edit.path, transaction_id, offset + i, "backup")?;
            if package::state(&staged)? != EntryState::Missing
                || package::state(&backup)? != EntryState::Missing
            {
                return Err(conflict("Resource staging already exists"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if fs::metadata(edit.path.parent().unwrap())?.dev()
                    != fs::metadata(staged.parent().unwrap())?.dev()
                {
                    return Err(SkillsError::invalid(
                        "Instruction transaction storage must share its filesystem",
                    ));
                }
            }
            if let Some(content) = &edit.content {
                files::atomic_bytes(&staged, content)?;
                package::set_mode(&staged, edit.mode)?;
                fs::File::open(&staged)?.sync_all()?;
            }
            changes.push(Change {
                target: edit.path.clone(),
                before: edit.before.clone(),
                after: package::state(&staged)?,
                staged,
                backup,
            });
        }
        Ok(changes)
    }
}
fn ensure_parent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(SkillsError::invalid(
            "Instruction path traverses a non-directory or symlink",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            ensure_parent(
                path.parent()
                    .ok_or_else(|| SkillsError::invalid("Missing parent"))?,
            )?;
            fs::create_dir(path)?;
            package::set_mode(path, 0o700)
        }
        Err(e) => Err(e.into()),
    }
}
fn conflict(message: &str) -> SkillsError {
    SkillsError::new(ErrorCode::Conflict, message)
}
fn read_text(path: &Path) -> Result<(EntryState, String)> {
    let state = package::state(path)?;
    match state {
        EntryState::Missing => Ok((state, String::new())),
        EntryState::File { .. } => {
            let bytes = files::read_metadata_bytes(path)?;
            let text = String::from_utf8(bytes)
                .map_err(|_| SkillsError::invalid("Instruction file must be UTF-8"))?;
            Ok((state, text))
        }
        _ => Err(conflict("Instruction resource must be a regular file")),
    }
}
pub fn ledger_directory(repo: &Repository) -> PathBuf {
    repo.coordinator.root().join("shared-instructions")
}
fn ledger_path(repo: &Repository, path: &Path, marker: &str) -> Result<PathBuf> {
    let key = serde_json::to_vec(&(path, marker))
        .map_err(|_| SkillsError::invalid("Invalid resource identity"))?;
    Ok(ledger_directory(repo).join(format!("{}.json", files::digest(&key))))
}
fn owner_id(manifest: &Path, name: &str) -> Result<String> {
    super::installers::validate_name(name)?;
    let destination = manifest
        .parent()
        .ok_or_else(|| SkillsError::invalid("Missing manifest parent"))?
        .join(name);
    Ok(format!(
        "installed-{}",
        &files::digest(destination.to_string_lossy().as_bytes())[..32]
    ))
}
pub fn owner(target: &InstallTarget, name: &str) -> Result<Owner> {
    let manifest_path = target.skills_dir.join(".skills-linker-manifest.json");
    Ok(Owner {
        installation_id: owner_id(&manifest_path, name)?,
        manifest_path,
        install_name: name.into(),
    })
}
fn validate_owner(owner: &Owner, resource: &ManagedResource) -> Result<()> {
    if owner
        .manifest_path
        .file_name()
        .is_none_or(|n| n != ".skills-linker-manifest.json")
        || package::entry_path(&owner.manifest_path)? != owner.manifest_path
        || owner.installation_id != owner_id(&owner.manifest_path, &owner.install_name)?
    {
        return Err(SkillsError::recovery("Invalid shared instruction owner"));
    }
    let value: serde_json::Value = files::read_json(&owner.manifest_path).map_err(|_| {
        conflict("Shared instruction owner manifest is missing or unreadable; retain the block")
    })?;
    if value["version"] != 2 {
        return Err(conflict("Shared instruction owner version is unsupported"));
    }
    let entry: Installation = serde_json::from_value(value["skills"][&owner.install_name].clone())
        .map_err(|_| conflict("Shared instruction owner is not verifiable; retain the block"))?;
    if entry.installation_id != owner.installation_id
        || !entry.managed_resources.iter().any(|r| r == resource)
    {
        return Err(conflict(
            "Shared instruction owner does not match its manifest; retain the block",
        ));
    }
    Ok(())
}

/// Exact standalone markers only. Reject duplicate, out-of-order and nested
/// markers before changing any bytes, preserving all text outside the span.
pub fn block_span(text: &str, marker: &str) -> Result<Option<(usize, usize)>> {
    let mut open: Option<&str> = None;
    let mut seen = std::collections::BTreeSet::new();
    for line in text.lines() {
        let Some(value) = line
            .trim_end_matches('\r')
            .strip_prefix("<!-- skills-linker:")
        else {
            continue;
        };
        if let Some(name) = value.strip_suffix(":begin -->") {
            if open.is_some() || !seen.insert(name) {
                return Err(conflict("Nested or duplicate managed instruction blocks"));
            }
            open = Some(name);
        } else if let Some(name) = value.strip_suffix(":end -->") {
            if open != Some(name) {
                return Err(conflict("Mismatched managed instruction markers"));
            }
            open = None;
        }
    }
    if open.is_some() {
        return Err(conflict("Unclosed managed instruction block"));
    }
    let begin = format!("<!-- skills-linker:{marker}:begin -->");
    let end = format!("<!-- skills-linker:{marker}:end -->");
    let (mut starts, mut ends, mut offset) = (Vec::new(), Vec::new(), 0);
    for line in text.split_inclusive('\n') {
        let clean = line.trim_end_matches(['\r', '\n']);
        if clean == begin {
            starts.push((offset, offset + line.len()));
        }
        if clean == end {
            ends.push((offset, offset + line.len()));
        }
        offset += line.len();
    }
    if starts.is_empty() && ends.is_empty() {
        return Ok(None);
    }
    if starts.len() != 1
        || ends.len() != 1
        || starts[0].1 > ends[0].0
        || text[starts[0].1..ends[0].0].contains("<!-- skills-linker:")
    {
        return Err(conflict(
            "Malformed, nested or duplicate instruction block; file left unchanged",
        ));
    }
    Ok(Some((starts[0].0, ends[0].1)))
}
fn expected_block(content: &str, marker: &str, text: &str) -> Result<String> {
    if content.contains("<!-- skills-linker:") {
        return Err(SkillsError::invalid(
            "inject.md must not contain managed block markers",
        ));
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let normalized = content.replace("\r\n", "\n");
    Ok(format!("<!-- skills-linker:{marker}:begin -->{newline}{}{newline}<!-- skills-linker:{marker}:end -->{newline}", normalized.trim_end_matches('\n').replace('\n', newline)))
}
fn existing_content<'a>(
    text: &'a str,
    kind: &ResourceKind,
    marker: &str,
) -> Result<Option<&'a str>> {
    match kind {
        ResourceKind::Instruction => Ok(block_span(text, marker)?.map(|(a, b)| &text[a..b])),
        ResourceKind::Wrapper => Ok((!text.is_empty()).then_some(text)),
    }
}

#[derive(Clone)]
pub struct Participant {
    pub owner: Owner,
    pub previous: Vec<ManagedResource>,
    pub desired: Vec<DesiredResource>,
}

/// Called under the common writer lock for both preview and apply. All existing
/// users must still have matching manifest references; unknown ownership is not zero.
pub fn plan(
    repo: &Repository,
    owner: &Owner,
    previous: &[ManagedResource],
    desired: &[DesiredResource],
) -> Result<ResourcePlan> {
    plan_member(
        repo,
        &Participant {
            owner: owner.clone(),
            previous: previous.to_vec(),
            desired: desired.to_vec(),
        },
        &[],
    )
}

/// A coordinated transaction may replace a shared block only when every
/// existing owner participates and every remaining owner requests identical text.
/// No manifest is changed here; callers commit all returned edits together.
pub fn plan_member(
    repo: &Repository,
    member: &Participant,
    peers: &[Participant],
) -> Result<ResourcePlan> {
    let Participant {
        owner,
        previous,
        desired,
    } = member;
    let mut keys = BTreeMap::new();
    for old in previous {
        let kind = if old.marker.starts_with("slash:") {
            ResourceKind::Wrapper
        } else {
            ResourceKind::Instruction
        };
        if keys
            .insert(
                (old.path.clone(), old.marker.clone()),
                (kind, Some(old), None),
            )
            .is_some()
        {
            return Err(SkillsError::recovery("Duplicate managed resource"));
        }
    }
    for new in desired {
        let entry = keys
            .entry((new.path.clone(), new.marker.clone()))
            .or_insert((new.kind.clone(), None, None));
        if entry.0 != new.kind || entry.2.is_some() {
            return Err(SkillsError::invalid("Duplicate resource intent"));
        }
        entry.2 = Some(new);
    }
    let mut result = ResourcePlan {
        resources: Vec::new(),
        edits: Vec::new(),
    };
    for ((path, marker), (kind, old, new)) in keys {
        let (before, text) = read_text(&path)?;
        let ledger_path = ledger_path(repo, &path, &marker)?;
        let ledger_before = package::state(&ledger_path)?;
        let current = existing_content(&text, &kind, &marker)?;
        let mut ledger = if ledger_before == EntryState::Missing {
            if old.is_some()
                || current.is_some()
                || (kind == ResourceKind::Wrapper && before != EntryState::Missing)
            {
                return Err(conflict("Instruction ownership is unknown; migrate or resolve the existing resource first"));
            }
            Ledger {
                schema_version: 1,
                path: path.clone(),
                marker: marker.clone(),
                kind: kind.clone(),
                content_digest: String::new(),
                owners: BTreeMap::new(),
            }
        } else {
            let ledger: Ledger = files::read_json(&ledger_path)?;
            if ledger.schema_version != 1
                || ledger.path != path
                || ledger.marker != marker
                || ledger.kind != kind
                || ledger.owners.is_empty()
            {
                return Err(SkillsError::recovery("Invalid shared instruction ledger"));
            }
            if current
                .is_none_or(|content| files::digest(content.as_bytes()) != ledger.content_digest)
            {
                return Err(conflict(
                    "Instruction content is missing or changed; retain user edits",
                ));
            }
            let resource = ManagedResource {
                path: path.clone(),
                marker: marker.clone(),
                content_digest: ledger.content_digest.clone(),
            };
            for (id, user) in &ledger.owners {
                if id != &user.installation_id {
                    return Err(SkillsError::recovery(
                        "Invalid shared instruction owner key",
                    ));
                }
                validate_owner(user, &resource)?;
            }
            ledger
        };
        // Include the current member exactly once. Peer identities and their
        // prior references are revalidated against the current ledger, not a count.
        let mut participants = BTreeMap::new();
        participants.insert(owner.installation_id.clone(), member);
        for peer in peers {
            if peer.owner.installation_id == owner.installation_id {
                continue;
            }
            if participants
                .insert(peer.owner.installation_id.clone(), peer)
                .is_some()
            {
                return Err(SkillsError::invalid("Duplicate coordinated installation"));
            }
        }
        let involved = participants
            .values()
            .filter_map(|p| {
                let old = p
                    .previous
                    .iter()
                    .find(|r| r.path == path && r.marker == marker);
                let new = p
                    .desired
                    .iter()
                    .find(|r| r.path == path && r.marker == marker);
                (old.is_some() || new.is_some()).then_some((*p, old, new))
            })
            .collect::<Vec<_>>();
        for (participant, old, _) in &involved {
            if let Some(old) = old {
                if old.content_digest != ledger.content_digest
                    || ledger.owners.get(&participant.owner.installation_id)
                        != Some(&participant.owner)
                {
                    return Err(conflict(
                        "Installation no longer owns this instruction resource",
                    ));
                }
            } else if ledger
                .owners
                .contains_key(&participant.owner.installation_id)
            {
                return Err(conflict(
                    "Instruction ownership is not recorded in the installation",
                ));
            }
        }
        for (participant, _, _) in &involved {
            ledger.owners.remove(&participant.owner.installation_id);
        }
        let mut replacement: Option<String> = None;
        for (_, _, desired) in &involved {
            let Some(desired) = desired else {
                continue;
            };
            if desired.kind != kind {
                return Err(conflict("Shared resource kind changed"));
            }
            let content = match kind {
                ResourceKind::Instruction => expected_block(&desired.content, &marker, &text)?,
                ResourceKind::Wrapper => desired.content.clone(),
            };
            if replacement
                .as_ref()
                .is_some_and(|current| current != &content)
            {
                return Err(conflict(
                    "Selected installations request different shared instruction content",
                ));
            }
            if !ledger.owners.is_empty()
                && files::digest(content.as_bytes()) != ledger.content_digest
            {
                return Err(conflict("Select every installation using this instruction block before updating its content"));
            }
            replacement = Some(content);
            // Add participating owners only after all requests are compared.
        }
        if let Some(content) = &replacement {
            ledger.content_digest = files::digest(content.as_bytes());
            for (participant, _, desired) in &involved {
                if desired.is_some() {
                    ledger.owners.insert(
                        participant.owner.installation_id.clone(),
                        participant.owner.clone(),
                    );
                }
            }
            if new.is_some() {
                result.resources.push(ManagedResource {
                    path: path.clone(),
                    marker: marker.clone(),
                    content_digest: ledger.content_digest.clone(),
                });
            }
        } else if !ledger.owners.is_empty() {
            replacement = current.map(str::to_owned);
        }
        let updated = match kind {
            ResourceKind::Wrapper => replacement,
            ResourceKind::Instruction => match block_span(&text, &marker)? {
                Some((a, b)) => Some(format!(
                    "{}{}{}",
                    &text[..a],
                    replacement.as_deref().unwrap_or(""),
                    &text[b..]
                )),
                None => replacement.map(|block| {
                    format!(
                        "{text}{}{block}",
                        if !text.is_empty() && !text.ends_with('\n') {
                            "\n"
                        } else {
                            ""
                        }
                    )
                }),
            },
        }
        .filter(|text| !text.is_empty());
        // Include unchanged resources too: their fingerprints must remain stable
        // between preview and apply, and recovery verifies the complete write set.
        let mode = match &before {
            EntryState::File { mode, .. } => *mode,
            _ => 0o600,
        };
        result.edits.push(Edit {
            path,
            before,
            content: updated.map(String::into_bytes),
            mode,
        });
        let ledger_content = if ledger.owners.is_empty() {
            None
        } else {
            Some(
                serde_json::to_vec_pretty(&ledger)
                    .map_err(|_| SkillsError::invalid("Cannot encode instruction ownership"))?,
            )
        };
        result.edits.push(Edit {
            path: ledger_path,
            before: ledger_before,
            content: ledger_content,
            mode: 0o600,
        });
    }
    Ok(result)
}

pub fn desired(
    context: &TargetContext,
    target: &InstallTarget,
    source: &ResolvedPackage,
    slash: bool,
) -> Result<Vec<DesiredResource>> {
    let content = files::read_metadata_bytes(&source.directory.join("SKILL.md"))?;
    let text =
        String::from_utf8(content).map_err(|_| SkillsError::invalid("Skill must be UTF-8"))?;
    let front = catalog::frontmatter(&text)?.0;
    let manifest = source.directory.join("skill.yaml");
    let metadata = if manifest.try_exists()? {
        catalog::frontmatter(&format!(
            "---\n{}\n---\n",
            String::from_utf8(files::read_metadata_bytes(&manifest)?)
                .map_err(|_| SkillsError::invalid("Skill manifest must be UTF-8"))?
        ))?
        .0
    } else {
        BTreeMap::new()
    };
    let first = source.relative_path.split('/').next().unwrap_or("");
    let category = if ["auto", "global", "manual", "cron"].contains(&first) {
        first
    } else if let Some(category) = metadata.get("category") {
        category.as_str()
    } else {
        match metadata.get("activation").map(String::as_str) {
            Some("always_on") => "auto",
            Some("scheduled" | "cron") => "cron",
            Some("global") => "global",
            Some("auto") => "auto",
            _ => "manual",
        }
    };
    if matches!(category, "auto" | "global") && source.directory.join("inject.md").try_exists()? {
        let content = String::from_utf8(files::read_metadata_bytes(
            &source.directory.join("inject.md"),
        )?)
        .map_err(|_| SkillsError::invalid("inject.md must be UTF-8"))?;
        return Ok(vec![DesiredResource {
            path: context.instruction_path(target)?,
            marker: source.name.clone(),
            kind: ResourceKind::Instruction,
            content,
        }]);
    }
    if category == "manual" && slash {
        if let Some(path) = context.wrapper_path(target, &source.name)? {
            let marker = format!("slash:{}", source.name);
            let summary = front
                .get("description")
                .or_else(|| metadata.get("summary"))
                .cloned()
                .unwrap_or_else(|| format!("Invoke the {} skill", source.name));
            let header = if target.tool_id == "claude-code" {
                format!(
                    "---\ndescription: {}\nargument-hint: '[extra context]'\n---\n",
                    serde_json::to_string(&summary).unwrap()
                )
            } else {
                String::new()
            };
            // Resolve against the actual installed package, never the old repository layout.
            let installed = target.skills_dir.join(&source.name).join("SKILL.md");
            let args = if target.tool_id == "claude-code" {
                "\n$ARGUMENTS\n"
            } else {
                ""
            };
            return Ok(vec![DesiredResource { path, marker: marker.clone(), kind: ResourceKind::Wrapper,
                content: format!("{header}<!-- skills-linker:{marker} -->\n\nUse the `{}` skill for this request. Read its definition at:\n{}\n{args}", source.name, installed.display()) }]);
        }
    }
    Ok(Vec::new())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionCheck {
    pub path: PathBuf,
    pub state: String,
    pub message: Option<String>,
}
/// Read-only diagnosis. A stale or unverified block is never repaired implicitly.
pub fn diagnose(repo: &Repository, resources: &[ManagedResource]) -> Vec<InstructionCheck> {
    resources
        .iter()
        .map(|resource| {
            let inspect = || -> Result<String> {
                let (_, text) = read_text(&resource.path)?;
                let kind = if resource.marker.starts_with("slash:") {
                    ResourceKind::Wrapper
                } else {
                    ResourceKind::Instruction
                };
                let Some(content) = existing_content(&text, &kind, &resource.marker)? else {
                    return Ok("missing".into());
                };
                if files::digest(content.as_bytes()) != resource.content_digest {
                    return Ok("stale".into());
                }
                let ledger: Ledger =
                    files::read_json(&ledger_path(repo, &resource.path, &resource.marker)?)
                        .map_err(|_| conflict("Instruction ownership could not be verified"))?;
                if ledger.schema_version != 1
                    || ledger.path != resource.path
                    || ledger.marker != resource.marker
                    || ledger.kind != kind
                    || ledger.content_digest != resource.content_digest
                    || ledger.owners.is_empty()
                {
                    return Err(conflict("Instruction ownership does not match its content"));
                }
                for (key, owner) in &ledger.owners {
                    if key != &owner.installation_id {
                        return Err(conflict("Invalid instruction owner"));
                    }
                    validate_owner(owner, resource)?;
                }
                Ok("current".into())
            };
            match inspect() {
                Ok(state) => InstructionCheck {
                    path: resource.path.clone(),
                    state,
                    message: None,
                },
                Err(error) => InstructionCheck {
                    path: resource.path.clone(),
                    state: "malformed".into(),
                    message: Some(error.message),
                },
            }
        })
        .collect()
}

pub fn diagnose_installation(
    repo: &Repository,
    context: &TargetContext,
    target: &InstallTarget,
    entry: &Installation,
) -> Vec<InstructionCheck> {
    let mut checks = diagnose(repo, &entry.managed_resources);
    if checks.is_empty() {
        return checks;
    }
    let source = ResolvedPackage {
        skill_id: entry.skill_id.clone(),
        source_id: entry.source_id.clone(),
        relative_path: entry.relative_path.clone(),
        name: entry.install_name.clone(),
        directory: target.skills_dir.join(&entry.install_name),
        revision: entry.revision.clone(),
        digest: entry.source_digest.clone(),
    };
    let expected = desired(
        context,
        target,
        &source,
        entry
            .managed_resources
            .iter()
            .any(|r| r.marker.starts_with("slash:")),
    );
    for check in &mut checks {
        if check.state != "current" {
            continue;
        }
        let compare = || -> Result<bool> {
            let expected = expected.as_ref().map_err(|_| {
                conflict("Installed skill cannot supply its managed instruction; review its files")
            })?;
            let Some(wanted) = expected.iter().find(|r| r.path == check.path) else {
                return Ok(false);
            };
            let (_, text) = read_text(&check.path)?;
            let content = if wanted.kind == ResourceKind::Instruction {
                expected_block(&wanted.content, &wanted.marker, &text)?
            } else {
                wanted.content.clone()
            };
            Ok(existing_content(&text, &wanted.kind, &wanted.marker)? == Some(content.as_str()))
        };
        match compare() {
            Ok(true) => (),
            Ok(false) => {
                check.state = "stale".into();
                check.message =
                    Some("Installed skill instructions changed; preview an explicit update".into());
            }
            Err(error) => {
                check.state = "malformed".into();
                check.message = Some(error.message);
            }
        }
    }
    checks
}
