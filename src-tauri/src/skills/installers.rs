use super::{
    catalog,
    error::{ErrorCode, Result, SkillsError},
    files,
    instructions::{self, ResourcePreview},
    operations::Operation,
    package::{self, EntryState},
    repository::{Repository, SourceKind},
    targets::{InstallTarget, TargetContext},
    transactions::{self, Change, Outcome},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MANIFEST: &str = ".skills-linker-manifest.json";
const LEGACY: &str = ".skills-linker-manifest.tsv";
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    Copy,
    Symlink,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Fail,
    Skip,
    Overwrite,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Install,
    Uninstall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Installation {
    pub installation_id: String,
    pub skill_id: String,
    pub source_id: String,
    pub relative_path: String,
    pub install_name: String,
    pub source_dir: PathBuf,
    pub mode: InstallMode,
    pub revision: Option<String>,
    pub source_digest: String,
    pub installed_state: EntryState,
    pub installed_at_ms: u128,
    pub updated_at_ms: u128,
    #[serde(default)]
    pub managed_resources: Vec<ManagedResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedResource {
    pub path: PathBuf,
    pub marker: String,
    pub content_digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    version: u32,
    skills: BTreeMap<String, Installation>,
}
impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: 2,
            skills: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub target: InstallTarget,
    pub skill_id: String,
    pub destination: PathBuf,
    pub state: String,
    pub installation: Option<Installation>,
    pub update_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instruction_checks: Vec<instructions::InstructionCheck>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallRequest {
    pub skill_id: String,
    pub target_ids: Vec<String>,
    pub mode: InstallMode,
    pub conflict_policy: ConflictPolicy,
    pub project_path: Option<String>,
    #[serde(default)]
    pub with_slash_commands: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UninstallRequest {
    pub installation_ids: Vec<String>,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanItem {
    pub target: InstallTarget,
    pub install_name: String,
    pub destination: PathBuf,
    pub before: EntryState,
    pub manifest_before: EntryState,
    pub previous: Option<Installation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_changes: Vec<ResourcePreview>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub with_slash_commands: bool,
    pub decision: String,
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedPackage {
    pub skill_id: String,
    pub source_id: String,
    pub relative_path: String,
    pub name: String,
    pub directory: PathBuf,
    pub revision: Option<String>,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallPlan {
    pub schema_version: u32,
    pub plan_id: String,
    pub action: PlanAction,
    pub project_path: Option<PathBuf>,
    pub mode: InstallMode,
    pub created_at_ms: u128,
    pub source: Option<ResolvedPackage>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub coordinated: bool,
    pub items: Vec<PlanItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResultItem {
    pub target_id: String,
    pub destination: PathBuf,
    pub status: String,
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_directory: Option<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReceipt {
    pub plan_id: String,
    pub operation_id: String,
    pub items: Vec<InstallResultItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryItem {
    pub operation_id: String,
    pub resources: Vec<PathBuf>,
    pub can_recover: bool,
    pub message: Option<String>,
}

fn recovery_authority(
    repo: &Repository,
    context: &TargetContext,
    pending: &super::coordination::PendingTransaction,
) -> Result<Vec<PathBuf>> {
    let targets = context
        .targets()?
        .into_iter()
        .filter(|t| t.blocked_reason.is_none())
        .collect::<Vec<_>>();
    let mut roots = targets
        .iter()
        .map(|t| t.skills_dir.clone())
        .collect::<Vec<_>>();
    let mut instructions = Vec::new();
    let mut wrappers = Vec::new();
    for target in &targets {
        if let Ok(path) = context.instruction_path(target) {
            roots.push(path.parent().unwrap().to_path_buf());
            instructions.push(path);
        }
        if let Ok(Some(path)) = context.wrapper_path(target, "recovery") {
            roots.push(path.parent().unwrap().to_path_buf());
            wrappers.push(path.parent().unwrap().to_path_buf());
        }
    }
    let ledger = instructions::ledger_directory(repo);
    roots.push(ledger.clone());
    roots.sort();
    roots.dedup();
    for path in transactions::resource_targets(pending, &roots)? {
        let parent = path
            .parent()
            .ok_or_else(|| SkillsError::invalid("Missing resource parent"))?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let skill = targets.iter().any(|t| t.skills_dir == parent)
            && (name == MANIFEST || validate_name(name).is_ok());
        let wrapper = wrappers.iter().any(|p| p == parent)
            && name
                .strip_suffix(".md")
                .is_some_and(|n| validate_name(n).is_ok());
        let ownership = parent == ledger
            && name
                .strip_suffix(".json")
                .is_some_and(|n| n.len() == 64 && n.bytes().all(|b| b.is_ascii_hexdigit()));
        if !skill && !wrapper && !ownership && !instructions.contains(&path) {
            return Err(SkillsError::recovery(
                "Recovery resource is outside known installation paths",
            ));
        }
    }
    Ok(roots)
}

pub fn recovery_items(repo: &Repository, context: &TargetContext) -> Result<Vec<RecoveryItem>> {
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    Ok(writer
        .pending()?
        .into_iter()
        .map(|pending| {
            let check = recovery_authority(repo, context, &pending);
            RecoveryItem {
                operation_id: pending.operation_id,
                resources: pending.write_set,
                can_recover: check.is_ok(),
                message: check.err().map(|e| e.message),
            }
        })
        .collect())
}

pub fn recover_one(repo: &Repository, context: &TargetContext, id: &str) -> Result<Outcome> {
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    let pending = writer
        .pending()?
        .into_iter()
        .find(|p| p.operation_id == id)
        .ok_or_else(|| SkillsError::invalid("Unknown pending operation"))?;
    let roots = recovery_authority(repo, context, &pending)?;
    transactions::recover(&writer, &pending, &roots)
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('.')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(SkillsError::invalid("Skill install name must contain only letters, digits, dots, underscores or hyphens, and cannot start with a dot"));
    }
    Ok(())
}

fn load_manifest(target: &InstallTarget) -> Result<Manifest> {
    if let Some(reason) = &target.blocked_reason {
        return Err(SkillsError::invalid(reason.clone()));
    }
    let path = target.skills_dir.join(MANIFEST);
    files::regular_file(&path)?;
    if !path.try_exists()? {
        if target.skills_dir.join(LEGACY).try_exists()? {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Legacy installation manifest is read-only; automatic adoption is not supported",
            ));
        }
        return Ok(Manifest::default());
    }
    let value: serde_json::Value = files::read_json(&path)?;
    if value["version"] == 1 {
        return Err(SkillsError::new(
            ErrorCode::Conflict,
            "Legacy installation manifest is read-only; automatic adoption is not supported",
        ));
    }
    let manifest: Manifest = serde_json::from_value(value)
        .map_err(|_| SkillsError::recovery("Malformed installation manifest"))?;
    if manifest.version != 2 {
        return Err(SkillsError::recovery(
            "Unsupported installation manifest version",
        ));
    }
    for (name, entry) in &manifest.skills {
        validate_name(name)?;
        for resource in &entry.managed_resources {
            if !resource.path.is_absolute()
                || !matches!(resource.content_digest.len(), 64)
                || !resource
                    .content_digest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit())
                || (resource.marker != *name && resource.marker != format!("slash:{name}"))
            {
                return Err(SkillsError::recovery(
                    "Invalid managed instruction reference",
                ));
            }
        }
        files::validate_id(&entry.installation_id)?;
        if name != &entry.install_name || entry.installed_state == EntryState::Missing {
            return Err(SkillsError::recovery(
                "Invalid installation ownership record",
            ));
        }
        let (source_id, relative) = catalog::decode_id(&entry.skill_id)?;
        let valid_digest =
            |digest: &str| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit());
        let valid_state = match (&entry.mode, &entry.installed_state) {
            (InstallMode::Copy, EntryState::Directory { digest }) => valid_digest(digest),
            (InstallMode::Symlink, EntryState::Symlink { target }) => target == &entry.source_dir,
            _ => false,
        };
        if source_id != entry.source_id
            || relative != entry.relative_path
            || !entry.source_dir.is_absolute()
            || !valid_digest(&entry.source_digest)
            || !valid_state
            || entry.installation_id != installation_id(&target.skills_dir.join(name))
        {
            return Err(SkillsError::recovery(
                "Installation identity or fingerprint is invalid",
            ));
        }
    }
    Ok(manifest)
}

pub fn resolve(repo: &Repository, skill_id: &str) -> Result<ResolvedPackage> {
    let (source_id, relative) = catalog::decode_id(skill_id)?;
    let source = repo.get_source(&source_id)?;
    let source_root = repo.source_directory(&source)?;
    let root = if source.kind == SourceKind::GithubApi {
        source_root.clone()
    } else {
        catalog::scan_root(&source_root)?
    };
    let file = catalog::resolve_file(&root, &relative)?;
    if file.file_name().is_none_or(|n| n != "SKILL.md") {
        return Err(SkillsError::invalid("Not a skill entry"));
    }
    let directory = file.parent().unwrap().to_path_buf();
    let detail = repo.detail(skill_id)?;
    let front = catalog::frontmatter(&detail.content)?.0;
    let fallback = if relative == "SKILL.md" {
        source
            .name
            .rsplit('/')
            .next()
            .unwrap_or("skill")
            .to_string()
    } else {
        directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    let name = front
        .get("name")
        .or_else(|| detail.manifest.get("id"))
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or(fallback);
    validate_name(&name)?;
    Ok(ResolvedPackage {
        skill_id: skill_id.into(),
        source_id,
        relative_path: relative,
        name,
        digest: package::tree_digest(&directory, true)?,
        directory,
        revision: if source.kind != SourceKind::Local {
            source.revision
        } else {
            None
        },
    })
}

pub fn statuses(
    repo: &Repository,
    context: &TargetContext,
    skill_id: &str,
) -> Result<Vec<InstallStatus>> {
    let package = resolve(repo, skill_id)?;
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    context
        .targets()?
        .into_iter()
        .map(|target| {
            let destination = target.skills_dir.join(&package.name);
            let mut status = InstallStatus {
                target: target.clone(),
                skill_id: skill_id.into(),
                destination,
                state: "absent".into(),
                installation: None,
                instruction_checks: Vec::new(),
                update_available: false,
                message: None,
            };
            if let Some(reason) = &target.blocked_reason {
                status.state = "unavailable".into();
                status.message = Some(reason.clone());
                return Ok(status);
            }
            if let Err(error) = writer.check_write_set(std::slice::from_ref(&target.skills_dir)) {
                status.state = "recovery_required".into();
                status.message = Some(error.message);
                return Ok(status);
            }
            let manifest = match load_manifest(&target) {
                Ok(m) => m,
                Err(error) => {
                    status.state = if error.code == ErrorCode::Conflict {
                        "legacy"
                    } else {
                        "unavailable"
                    }
                    .into();
                    status.message = Some(error.message);
                    return Ok(status);
                }
            };
            let entry = manifest
                .skills
                .values()
                .find(|entry| entry.skill_id == skill_id)
                .or_else(|| manifest.skills.get(&package.name));
            if let Some(entry) = entry {
                status.destination = target.skills_dir.join(&entry.install_name);
                let actual = package::state(&status.destination)?;
                status.state = if entry.skill_id != skill_id {
                    "conflict"
                } else if actual == EntryState::Missing {
                    "missing"
                } else if actual != entry.installed_state {
                    "modified"
                } else {
                    "installed"
                }
                .into();
                status.update_available = entry.skill_id == skill_id
                    && (entry.revision != package.revision
                        || (entry.mode == InstallMode::Copy
                            && entry.source_digest != package.digest));
                let instruction_state = validate_owned_resources(
                    context,
                    &target,
                    &entry.install_name,
                    &entry.managed_resources,
                )
                .and_then(|_| {
                    writer.check_write_set(
                        &entry
                            .managed_resources
                            .iter()
                            .map(|r| r.path.clone())
                            .collect::<Vec<_>>(),
                    )
                });
                if let Err(error) = instruction_state {
                    status.state = "recovery_required".into();
                    status.message = Some(error.message);
                } else {
                    status.instruction_checks =
                        instructions::diagnose_installation(repo, context, &target, entry);
                }
                status.installation = Some(entry.clone());
            } else if package::state(&status.destination)? != EntryState::Missing {
                status.state = "conflict".into();
                status.message = Some("Destination is not managed by Loom".into());
            }
            Ok(status)
        })
        .collect()
}

pub fn inventory(repo: &Repository, context: &TargetContext) -> Result<Vec<InstallStatus>> {
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    inventory_locked(repo, context, &writer)
}

fn inventory_locked(
    repo: &Repository,
    context: &TargetContext,
    writer: &super::coordination::WriterGuard<'_>,
) -> Result<Vec<InstallStatus>> {
    let mut result = Vec::new();
    for target in context.targets()? {
        let manifest = match writer
            .check_write_set(std::slice::from_ref(&target.skills_dir))
            .and_then(|_| load_manifest(&target))
        {
            Ok(m) => m,
            Err(error) => {
                result.push(InstallStatus {
                    target: target.clone(),
                    skill_id: String::new(),
                    destination: target.skills_dir.clone(),
                    state: if error.code == ErrorCode::Conflict {
                        "legacy"
                    } else if error.code == ErrorCode::RecoveryRequired {
                        "recovery_required"
                    } else {
                        "unavailable"
                    }
                    .into(),
                    installation: None,
                    instruction_checks: Vec::new(),
                    update_available: false,
                    message: Some(error.message),
                });
                continue;
            }
        };
        for entry in manifest.skills.values() {
            let destination = target.skills_dir.join(&entry.install_name);
            let actual = package::state(&destination)?;
            let instruction_error = validate_owned_resources(
                context,
                &target,
                &entry.install_name,
                &entry.managed_resources,
            )
            .and_then(|_| {
                writer.check_write_set(
                    &entry
                        .managed_resources
                        .iter()
                        .map(|r| r.path.clone())
                        .collect::<Vec<_>>(),
                )
            })
            .err();
            let checks = if instruction_error.is_none() {
                instructions::diagnose_installation(repo, context, &target, entry)
            } else {
                Vec::new()
            };
            result.push(InstallStatus {
                target: target.clone(),
                skill_id: entry.skill_id.clone(),
                destination,
                state: if instruction_error.is_some() {
                    "recovery_required"
                } else if actual == EntryState::Missing {
                    "missing"
                } else if actual != entry.installed_state {
                    "modified"
                } else {
                    "installed"
                }
                .into(),
                installation: Some(entry.clone()),
                instruction_checks: checks,
                update_available: false,
                message: instruction_error.map(|e| e.message),
            });
        }
    }
    Ok(result)
}

fn plan_path(repo: &Repository, id: &str) -> Result<PathBuf> {
    files::validate_id(id)?;
    Ok(repo.root.join("plans").join(format!("{id}.json")))
}
pub fn load_plan(repo: &Repository, id: &str) -> Result<InstallPlan> {
    files::read_json(&plan_path(repo, id)?)
}
fn installation_id(destination: &Path) -> String {
    format!(
        "installed-{}",
        &files::digest(destination.to_string_lossy().as_bytes())[..32]
    )
}
fn new_plan(
    action: PlanAction,
    context: &TargetContext,
    mode: InstallMode,
    source: Option<ResolvedPackage>,
) -> InstallPlan {
    InstallPlan {
        schema_version: 1,
        plan_id: format!(
            "{}-{}",
            crate::models::IdGenerator::default().next("install"),
            std::process::id()
        ),
        action,
        project_path: context.project().map(Path::to_path_buf),
        mode,
        created_at_ms: crate::models::now_ms(),
        source,
        coordinated: false,
        items: Vec::new(),
    }
}
fn save_plan(repo: &Repository, plan: &InstallPlan) -> Result<()> {
    files::private_directory(&repo.root.join("plans"))?;
    files::atomic_json(&plan_path(repo, &plan.plan_id)?, plan)
}

pub fn plan_install(
    repo: &Repository,
    context: &TargetContext,
    input: &InstallRequest,
) -> Result<InstallPlan> {
    let source = resolve(repo, &input.skill_id)?;
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    let mut plan = new_plan(
        PlanAction::Install,
        context,
        input.mode,
        Some(source.clone()),
    );
    for target in context.selected(&input.target_ids)? {
        let destination = target.skills_dir.join(&source.name);
        let before = package::state(&destination)?;
        let mut item = PlanItem {
            target: target.clone(),
            install_name: source.name.clone(),
            destination,
            before: before.clone(),
            manifest_before: package::state(&target.skills_dir.join(MANIFEST))?,
            previous: None,
            resource_changes: Vec::new(),
            with_slash_commands: input.with_slash_commands,
            decision: "install".into(),
            message: None,
        };
        let check = (|| {
            writer.check_write_set(std::slice::from_ref(&target.skills_dir))?;
            if item.destination.starts_with(&source.directory)
                || source.directory.starts_with(&item.destination)
            {
                return Err(SkillsError::invalid("Installation overlaps its source"));
            }
            let manifest = load_manifest(&target)?;
            if manifest
                .skills
                .values()
                .any(|e| e.skill_id == source.skill_id && e.install_name != source.name)
            {
                return Err(SkillsError::new(
                    ErrorCode::Conflict,
                    "Install name changed; uninstall the previous name before reinstalling",
                ));
            }
            item.previous = manifest.skills.get(&source.name).cloned();
            if before != EntryState::Missing || item.previous.is_some() {
                if input.conflict_policy == ConflictPolicy::Skip {
                    item.decision = "skip".into();
                    return Ok(());
                }
                if input.conflict_policy != ConflictPolicy::Overwrite {
                    return Err(SkillsError::new(
                        ErrorCode::Conflict,
                        "Installation already exists; choose replacement explicitly",
                    ));
                }
                let previous = item.previous.as_ref().ok_or_else(|| {
                    SkillsError::new(
                        ErrorCode::Conflict,
                        "Unmanaged destination cannot be overwritten",
                    )
                })?;
                item.decision = "replace".into();
                if before != previous.installed_state && before != EntryState::Missing {
                    item.message=Some("Modified managed content will be replaced; the previous content is retained in a recovery backup".into());
                }
            }
            Ok(())
        })();
        if let Err(error) = check {
            item.decision = "blocked".into();
            item.message = Some(error.message);
        }
        plan.items.push(item);
    }
    fill_resource_previews(repo, context, &writer, &mut plan)?;
    save_plan(repo, &plan)?;
    Ok(plan)
}

fn fill_resource_previews(
    repo: &Repository,
    context: &TargetContext,
    writer: &super::coordination::WriterGuard<'_>,
    plan: &mut InstallPlan,
) -> Result<()> {
    let mut participants = Vec::new();
    let mut indices = Vec::new();
    let mut paths = std::collections::BTreeSet::new();
    let mut overlap = false;
    for (index, item) in plan.items.iter_mut().enumerate() {
        if matches!(item.decision.as_str(), "blocked" | "skip") {
            continue;
        }
        match resource_participant(
            context,
            &item.target,
            &item.install_name,
            item.previous.as_ref(),
            plan.source.as_ref(),
            item.with_slash_commands,
        ) {
            Ok(participant) => {
                let unique = participant
                    .previous
                    .iter()
                    .map(|r| &r.path)
                    .chain(participant.desired.iter().map(|r| &r.path))
                    .collect::<std::collections::BTreeSet<_>>();
                for path in unique {
                    if !paths.insert(path.clone()) {
                        overlap = true;
                    }
                }
                participants.push(participant);
                indices.push(index);
            }
            Err(error) => {
                item.decision = "blocked".into();
                item.message = Some(error.message);
            }
        }
    }
    plan.coordinated = overlap && participants.len() > 1;
    let mut previews = Vec::new();
    for participant in &participants {
        let resource_plan = instructions::plan_member(
            repo,
            participant,
            if plan.coordinated { &participants } else { &[] },
        );
        let result = resource_plan.and_then(|resources| {
            let preview = resources.preview();
            writer.check_write_set(&preview.iter().map(|r| r.path.clone()).collect::<Vec<_>>())?;
            Ok(preview)
        });
        previews.push(result);
    }
    if plan.coordinated {
        if let Some(error) = previews.iter().find_map(|r| r.as_ref().err()) {
            for index in indices {
                plan.items[index].decision = "blocked".into();
                plan.items[index].message = Some(error.message.clone());
            }
            return Ok(());
        }
    }
    for (index, result) in indices.into_iter().zip(previews) {
        match result {
            Ok(preview) => plan.items[index].resource_changes = preview,
            Err(error) => {
                plan.items[index].decision = "blocked".into();
                plan.items[index].message = Some(error.message);
            }
        }
    }
    Ok(())
}

pub fn plan_uninstall(
    repo: &Repository,
    context: &TargetContext,
    input: &UninstallRequest,
) -> Result<InstallPlan> {
    if input.installation_ids.is_empty() || input.installation_ids.len() > 12 {
        return Err(SkillsError::invalid(
            "Choose managed installations to remove",
        ));
    }
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    let all = inventory_locked(repo, context, &writer)?;
    let mut plan = new_plan(PlanAction::Uninstall, context, InstallMode::Copy, None);
    let mut seen = std::collections::BTreeSet::new();
    for id in &input.installation_ids {
        if !seen.insert(id) {
            continue;
        }
        let status = all
            .iter()
            .find(|s| {
                s.installation
                    .as_ref()
                    .is_some_and(|e| &e.installation_id == id)
            })
            .ok_or_else(|| SkillsError::invalid("Unknown managed installation"))?;
        let previous = status.installation.clone().unwrap();
        let before = package::state(&status.destination)?;
        let mut blocked = writer
            .check_write_set(std::slice::from_ref(&status.target.skills_dir))
            .err()
            .map(|e| e.message)
            .or_else(|| {
                (status.state == "modified")
                    .then(|| "Installed content has user changes; uninstall refused".into())
            });
        let resource_changes = match resource_plan(
            repo,
            context,
            &status.target,
            &previous.install_name,
            Some(&previous),
            None,
            false,
        ) {
            Ok(resources) => {
                let preview = resources.preview();
                if let Err(error) = writer
                    .check_write_set(&preview.iter().map(|r| r.path.clone()).collect::<Vec<_>>())
                {
                    blocked = Some(error.message);
                }
                preview
            }
            Err(error) => {
                blocked = Some(error.message);
                Vec::new()
            }
        };
        plan.items.push(PlanItem {
            target: status.target.clone(),
            install_name: previous.install_name.clone(),
            destination: status.destination.clone(),
            before,
            manifest_before: package::state(&status.target.skills_dir.join(MANIFEST))?,
            previous: Some(previous),
            resource_changes,
            with_slash_commands: false,
            decision: if blocked.is_some() {
                "blocked"
            } else {
                "uninstall"
            }
            .into(),
            message: blocked,
        });
    }
    save_plan(repo, &plan)?;
    Ok(plan)
}

pub fn apply(
    repo: &Repository,
    context: &TargetContext,
    plan_id: &str,
    operation: &mut Operation,
) -> Result<InstallReceipt> {
    apply_with_prior(repo, context, plan_id, operation, &BTreeMap::new())
}

fn apply_with_prior(
    repo: &Repository,
    context: &TargetContext,
    plan_id: &str,
    operation: &mut Operation,
    prior: &BTreeMap<PathBuf, (EntryState, EntryState)>,
) -> Result<InstallReceipt> {
    let mut plan: InstallPlan = files::read_json(&plan_path(repo, plan_id)?)?;
    // Carry forward only verified writes made by this reviewed batch. Source,
    // destination and ownership decisions are never regenerated or rewritten.
    for item in &mut plan.items {
        rebase_reviewed_state(
            &item.target.skills_dir.join(MANIFEST),
            &mut item.manifest_before,
            prior,
        )?;
        for resource in &mut item.resource_changes {
            rebase_reviewed_state(&resource.path, &mut resource.before, prior)?;
        }
    }

    if plan.schema_version != 1
        || plan.items.is_empty()
        || plan.items.len() > 12
        || plan.plan_id != plan_id
        || plan.project_path.as_deref() != context.project()
    {
        return Err(SkillsError::invalid("Installation plan context mismatch"));
    }
    let result_path = repo
        .root
        .join("plans")
        .join(format!("{plan_id}.result.json"));
    files::regular_file(&result_path)?;
    if result_path.try_exists()? {
        return files::read_json(&result_path);
    }
    if crate::models::now_ms().saturating_sub(plan.created_at_ms) > 10 * 60 * 1000 {
        return Err(SkillsError::new(
            ErrorCode::Conflict,
            "Installation plan expired; review a new plan",
        ));
    }
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    if plan.coordinated {
        let receipt = apply_coordinated(repo, context, &writer, &plan, operation)?;
        files::atomic_json(&result_path, &receipt)?;
        return Ok(receipt);
    }
    let mut receipt = InstallReceipt {
        plan_id: plan_id.into(),
        operation_id: operation.record.operation_id.clone(),
        items: Vec::new(),
    };
    let mut manifests = BTreeMap::<PathBuf, EntryState>::new();
    for (index, original) in plan.items.iter().enumerate() {
        let mut item = original.clone();
        let manifest_path = item.target.skills_dir.join(MANIFEST);
        if let Some(state) = manifests.get(&manifest_path) {
            item.manifest_before = state.clone();
        }
        for resource in &mut item.resource_changes {
            if let Some(state) = manifests.get(&resource.path) {
                resource.before = state.clone();
            }
        }
        let result = if let Err(error) = operation.check_cancelled() {
            Err(error)
        } else if item.decision == "blocked" {
            Err(SkillsError::new(
                ErrorCode::Conflict,
                item.message.clone().unwrap_or_default(),
            ))
        } else if item.decision == "skip" {
            Ok("skipped".into())
        } else {
            apply_item(repo, context, &writer, &plan, &item, index, operation)
        };
        if result
            .as_ref()
            .is_ok_and(|status| matches!(status.as_str(), "installed" | "uninstalled"))
        {
            for resource in &item.resource_changes {
                manifests.insert(
                    resource.path.clone(),
                    transactions::committed_state(
                        &repo.root,
                        &format!("{}-{index}", plan.plan_id),
                        &resource.path,
                    )?,
                );
            }
            manifests.insert(
                manifest_path.clone(),
                transactions::committed_state(
                    &repo.root,
                    &format!("{}-{index}", plan.plan_id),
                    &manifest_path,
                )?,
            );
        }
        let (status, message) = match result {
            Ok(status) => (status, None),
            Err(error) => (
                if error.code == ErrorCode::Conflict {
                    "conflict"
                } else if error.code == ErrorCode::Cancelled {
                    "cancelled"
                } else if error.code == ErrorCode::RecoveryRequired {
                    "recovery_required"
                } else {
                    "failed"
                }
                .into(),
                Some(error.message),
            ),
        };
        let backup_directory = if matches!(status.as_str(), "installed" | "uninstalled")
            && item.before != EntryState::Missing
        {
            Some(
                transactions::artifact(
                    &item.destination,
                    &format!("{}-{index}", plan.plan_id),
                    0,
                    "backup",
                )?
                .parent()
                .unwrap()
                .to_path_buf(),
            )
        } else {
            None
        };
        receipt.items.push(InstallResultItem {
            target_id: item.target.id.clone(),
            destination: item.destination.clone(),
            status,
            message,
            backup_directory,
        });
    }
    files::atomic_json(&result_path, &receipt)?;
    Ok(receipt)
}

fn rebase_reviewed_state(
    path: &Path,
    state: &mut EntryState,
    prior: &BTreeMap<PathBuf, (EntryState, EntryState)>,
) -> Result<()> {
    if let Some((before, after)) = prior.get(path) {
        if state != before && state != after {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Reviewed plan has a different starting resource state",
            ));
        }
        *state = after.clone();
    }
    Ok(())
}

pub struct ReviewedResult {
    pub plan_id: String,
    pub result: Result<InstallReceipt>,
}
/// Apply a sequence of already reviewed plans without replanning. Metadata
/// fingerprints may advance only through this batch's verified commit journals.
pub fn apply_reviewed_batch(
    repo: &Repository,
    context: &TargetContext,
    plan_ids: &[String],
) -> Vec<ReviewedResult> {
    let mut prior = BTreeMap::<PathBuf, (EntryState, EntryState)>::new();
    let mut results = Vec::new();
    for id in plan_ids {
        let result = (|| {
            let plan = load_plan(repo, id)?;
            let mut operation = Operation::begin(
                repo,
                "managed-installations",
                if plan.action == PlanAction::Install {
                    "install"
                } else {
                    "uninstall"
                },
            )?;
            let result = apply_with_prior(repo, context, id, &mut operation, &prior);
            let status = match &result {
                Err(e) => Err(e.clone()),
                Ok(receipt)
                    if receipt.items.iter().any(|i| {
                        !["installed", "uninstalled", "skipped"].contains(&i.status.as_str())
                    }) =>
                {
                    Err(SkillsError::new(
                        ErrorCode::Conflict,
                        "Some reviewed changes did not apply",
                    ))
                }
                Ok(_) => Ok(()),
            };
            operation.finish(status)?;
            let receipt = result?;
            for (index, item) in receipt.items.iter().enumerate() {
                if !["installed", "uninstalled"].contains(&item.status.as_str()) {
                    continue;
                }
                let transaction = if plan.coordinated {
                    format!("{}-group", plan.plan_id)
                } else {
                    format!("{}-{index}", plan.plan_id)
                };
                for change in transactions::committed_changes(&repo.root, &transaction)? {
                    prior
                        .entry(change.target)
                        .and_modify(|state| state.1 = change.after.clone())
                        .or_insert((change.before, change.after));
                }
                if plan.coordinated {
                    break;
                }
            }
            Ok(receipt)
        })();
        results.push(ReviewedResult {
            plan_id: id.clone(),
            result,
        });
    }
    results
}

fn validate_owned_resources(
    context: &TargetContext,
    target: &InstallTarget,
    name: &str,
    owned: &[ManagedResource],
) -> Result<()> {
    for resource in owned {
        let expected = if resource.marker == name {
            Some(context.instruction_path(target)?)
        } else if resource.marker == format!("slash:{name}") {
            context.wrapper_path(target, name)?
        } else {
            None
        };
        if expected.as_ref() != Some(&resource.path) {
            return Err(SkillsError::recovery(
                "Instruction resource is outside its installation scope",
            ));
        }
    }
    Ok(())
}

fn resource_plan(
    repo: &Repository,
    context: &TargetContext,
    target: &InstallTarget,
    name: &str,
    previous: Option<&Installation>,
    source: Option<&ResolvedPackage>,
    slash: bool,
) -> Result<instructions::ResourcePlan> {
    let participant = resource_participant(context, target, name, previous, source, slash)?;
    instructions::plan_member(repo, &participant, &[])
}

fn resource_participant(
    context: &TargetContext,
    target: &InstallTarget,
    name: &str,
    previous: Option<&Installation>,
    source: Option<&ResolvedPackage>,
    slash: bool,
) -> Result<instructions::Participant> {
    let owned = previous
        .map(|p| p.managed_resources.as_slice())
        .unwrap_or_default();
    validate_owned_resources(context, target, name, owned)?;
    let desired = source
        .map(|s| instructions::desired(context, target, s, slash))
        .transpose()?
        .unwrap_or_default();
    let destination = target.skills_dir.join(name);
    for path in owned
        .iter()
        .map(|r| &r.path)
        .chain(desired.iter().map(|r| &r.path))
    {
        if path.starts_with(&destination) || source.is_some_and(|s| path.starts_with(&s.directory))
        {
            return Err(SkillsError::invalid(
                "Instruction resource overlaps a skill package",
            ));
        }
    }
    Ok(instructions::Participant {
        owner: instructions::owner(target, name)?,
        previous: owned.to_vec(),
        desired,
    })
}

fn completed_transaction(
    repo: &Repository,
    id: &str,
    action: PlanAction,
) -> Result<Option<String>> {
    if let Some(receipt) = transactions::receipt(&repo.root, id)? {
        if receipt.outcome != Outcome::Committed {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Previous attempt was rolled back; review a new plan",
            ));
        }
        return Ok(Some(
            if action == PlanAction::Install {
                "installed"
            } else {
                "uninstalled"
            }
            .into(),
        ));
    }
    Ok(None)
}

fn apply_item(
    repo: &Repository,
    context: &TargetContext,
    writer: &super::coordination::WriterGuard<'_>,
    plan: &InstallPlan,
    item: &PlanItem,
    index: usize,
    operation: &Operation,
) -> Result<String> {
    let id = format!("{}-{index}", plan.plan_id);
    if let Some(status) = completed_transaction(repo, &id, plan.action)? {
        return Ok(status);
    }
    let (pending, roots) = prepare_items(repo, context, writer, plan, &[item], &id, operation)?;
    commit_or_recover(writer, &pending, &roots)?;
    Ok(if plan.action == PlanAction::Install {
        "installed"
    } else {
        "uninstalled"
    }
    .into())
}

fn commit_or_recover(
    writer: &super::coordination::WriterGuard<'_>,
    pending: &super::coordination::PendingTransaction,
    roots: &[PathBuf],
) -> Result<()> {
    if let Err(error) = transactions::commit(writer, pending, roots) {
        if let Err(recovery) = transactions::recover(writer, pending, roots) {
            return Err(SkillsError::recovery(format!(
                "{}; {}",
                error.message, recovery.message
            )));
        }
        return Err(error);
    }
    Ok(())
}

fn apply_coordinated(
    repo: &Repository,
    context: &TargetContext,
    writer: &super::coordination::WriterGuard<'_>,
    plan: &InstallPlan,
    operation: &Operation,
) -> Result<InstallReceipt> {
    if plan.action != PlanAction::Install {
        return Err(SkillsError::invalid("Unsupported coordinated action"));
    }
    let items = plan
        .items
        .iter()
        .filter(|i| !matches!(i.decision.as_str(), "blocked" | "skip"))
        .collect::<Vec<_>>();
    let id = format!("{}-group", plan.plan_id);
    let result: Result<String> = (|| {
        if let Some(status) = completed_transaction(repo, &id, plan.action)? {
            return Ok(status);
        }
        if items.is_empty() {
            return Ok("skipped".into());
        }
        let (pending, roots) = prepare_items(repo, context, writer, plan, &items, &id, operation)?;
        commit_or_recover(writer, &pending, &roots)?;
        Ok("installed".to_string())
    })();
    let mut receipt = InstallReceipt {
        plan_id: plan.plan_id.clone(),
        operation_id: operation.record.operation_id.clone(),
        items: Vec::new(),
    };
    for item in &plan.items {
        let (status, message) = if item.decision == "blocked" {
            ("conflict".into(), item.message.clone())
        } else if item.decision == "skip" {
            ("skipped".into(), None)
        } else {
            match &result {
                Ok(status) => (status.clone(), None),
                Err(error) => (
                    result_state(&error.code).into(),
                    Some(error.message.clone()),
                ),
            }
        };
        let backup_directory = if status == "installed" && item.before != EntryState::Missing {
            Some(
                transactions::artifact(&item.destination, &id, 0, "backup")?
                    .parent()
                    .unwrap()
                    .to_path_buf(),
            )
        } else {
            None
        };
        receipt.items.push(InstallResultItem {
            target_id: item.target.id.clone(),
            destination: item.destination.clone(),
            status,
            message,
            backup_directory,
        });
    }
    Ok(receipt)
}
fn result_state(code: &ErrorCode) -> &'static str {
    match code {
        ErrorCode::Conflict => "conflict",
        ErrorCode::Cancelled => "cancelled",
        ErrorCode::RecoveryRequired => "recovery_required",
        _ => "failed",
    }
}

/// Common preparation path for a single target and a coordinated shared update.
/// This creates staging and a pending record; it never commits a live target.
pub(super) fn prepare_items(
    repo: &Repository,
    context: &TargetContext,
    writer: &super::coordination::WriterGuard<'_>,
    plan: &InstallPlan,
    items: &[&PlanItem],
    id: &str,
    operation: &Operation,
) -> Result<(super::coordination::PendingTransaction, Vec<PathBuf>)> {
    if items.is_empty() || (items.len() > 1 && !plan.coordinated) {
        return Err(SkillsError::invalid(
            "Invalid coordinated transaction members",
        ));
    }
    operation.check_cancelled()?;
    let source = if plan.action == PlanAction::Install {
        let expected = plan
            .source
            .as_ref()
            .ok_or_else(|| SkillsError::invalid("Missing install source"))?;
        let actual = resolve(repo, &expected.skill_id)?;
        if actual.directory != expected.directory
            || actual.digest != expected.digest
            || actual.revision != expected.revision
            || actual.name != expected.name
        {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Skill source changed after preview",
            ));
        }
        Some(actual)
    } else {
        None
    };
    let mut participants = Vec::new();
    let mut destinations = std::collections::BTreeSet::new();
    let mut manifest_paths = std::collections::BTreeSet::new();
    for item in items {
        let target = context
            .selected(std::slice::from_ref(&item.target.id))?
            .remove(0);
        validate_name(&item.install_name)?;
        if target.skills_dir != item.target.skills_dir
            || item.destination != target.skills_dir.join(&item.install_name)
            || !destinations.insert(item.destination.clone())
            || !manifest_paths.insert(target.skills_dir.join(MANIFEST))
            || source.as_ref().is_some_and(|s| s.name != item.install_name)
        {
            return Err(SkillsError::invalid(
                "Planned target mismatch or duplicate manifest",
            ));
        }
        writer.check_write_set(std::slice::from_ref(&target.skills_dir))?;
        if package::state(&item.destination)? != item.before
            || package::state(&target.skills_dir.join(MANIFEST))? != item.manifest_before
        {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Target changed after preview; review a new plan",
            ));
        }
        load_manifest(&target)?;
        participants.push(resource_participant(
            context,
            &target,
            &item.install_name,
            item.previous.as_ref(),
            source.as_ref(),
            item.with_slash_commands,
        )?);
    }
    let mut resource_plans = Vec::new();
    let mut edits = BTreeMap::<PathBuf, instructions::Edit>::new();
    for (item, participant) in items.iter().zip(&participants) {
        let resources = instructions::plan_member(repo, participant, &participants)?;
        let current = resources.preview();
        if current.len() != item.resource_changes.len()
            || current
                .iter()
                .zip(&item.resource_changes)
                .any(|(a, b)| a.path != b.path || a.before != b.before)
        {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Instruction resources changed after preview; review a new plan",
            ));
        }
        for edit in &resources.edits {
            if destinations.iter().any(|d| edit.path.starts_with(d)) {
                return Err(SkillsError::invalid(
                    "Shared resource overlaps an installation",
                ));
            }
            if let Some(existing) = edits.get(&edit.path) {
                if existing.before != edit.before
                    || existing.content != edit.content
                    || existing.mode != edit.mode
                {
                    return Err(SkillsError::new(
                        ErrorCode::Conflict,
                        "Selected installations require incompatible resource changes",
                    ));
                }
            } else {
                edits.insert(edit.path.clone(), edit.clone());
            }
        }
        resource_plans.push(resources);
    }
    let shared = instructions::ResourcePlan {
        resources: Vec::new(),
        edits: edits.into_values().collect(),
    };
    writer.check_write_set(
        &shared
            .edits
            .iter()
            .map(|e| e.path.clone())
            .collect::<Vec<_>>(),
    )?;
    // Preserve historical single-item journal ordering for existing retry/recovery records.
    let shared = if items.len() == 1 {
        instructions::ResourcePlan {
            resources: Vec::new(),
            edits: resource_plans[0].edits.clone(),
        }
    } else {
        shared
    };
    let mut staged_paths = Vec::new();
    for (index, item) in items.iter().enumerate() {
        staged_paths.push(transactions::artifact(
            &item.destination,
            id,
            index * 2,
            "staged",
        )?);
        staged_paths.push(transactions::artifact(
            &item.target.skills_dir.join(MANIFEST),
            id,
            index * 2 + 1,
            "staged",
        )?);
    }
    for (index, edit) in shared.edits.iter().enumerate() {
        staged_paths.push(transactions::artifact(
            &edit.path,
            id,
            items.len() * 2 + index,
            "staged",
        )?);
    }
    // Do not clean up artifacts that predated this attempt.
    for path in &staged_paths {
        let backup_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("staged-"))
            .ok_or_else(|| SkillsError::invalid("Invalid staging filename"))?;
        if package::state(path)? != EntryState::Missing
            || package::state(&path.with_file_name(format!("backup-{backup_name}")))?
                != EntryState::Missing
        {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Transaction artifact already exists; inspect previous attempt",
            ));
        }
    }
    let prepared: Result<(super::coordination::PendingTransaction, Vec<PathBuf>)> = (|| {
        let mut changes = Vec::new();
        for (item, resources) in items.iter().zip(&resource_plans) {
            operation.check_cancelled()?;
            changes.extend(stage_target(
                context,
                plan,
                item,
                source.as_ref(),
                &resources.resources,
                id,
                changes.len(),
            )?);
        }
        changes.extend(shared.stage(id, changes.len())?);
        operation.check_cancelled()?;
        let mut roots = changes
            .iter()
            .map(|c| c.target.parent().unwrap().to_path_buf())
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();
        Ok((transactions::prepare(repo, writer, id, changes)?, roots))
    })();
    if let Err(error) = &prepared {
        if writer.pending()?.iter().any(|p| p.operation_id == id) {
            return Err(SkillsError::recovery(error.message.clone()));
        }
        for path in staged_paths {
            if fs::symlink_metadata(&path).is_ok() {
                let _ = package::remove_entry(&path);
            }
        }
    }
    prepared
}

fn stage_target(
    context: &TargetContext,
    plan: &InstallPlan,
    item: &PlanItem,
    source: Option<&ResolvedPackage>,
    resources: &[ManagedResource],
    id: &str,
    offset: usize,
) -> Result<Vec<Change>> {
    let target = &item.target;
    let manifest_path = target.skills_dir.join(MANIFEST);
    let mut manifest = load_manifest(target)?;
    context.ensure(target)?;
    let staged = transactions::artifact(&item.destination, id, offset, "staged")?;
    let backup = transactions::artifact(&item.destination, id, offset, "backup")?;
    let staged_manifest = transactions::artifact(&manifest_path, id, offset + 1, "staged")?;
    for path in [&staged, &backup, &staged_manifest] {
        if package::state(path)? != EntryState::Missing {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Transaction artifact already exists; inspect previous attempt",
            ));
        }
    }
    transactions::prepare_storage(&item.destination, id)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if fs::metadata(&target.skills_dir)?.dev() != fs::metadata(staged.parent().unwrap())?.dev()
        {
            return Err(SkillsError::invalid("Target mount does not support same-filesystem transaction storage outside its skill directory"));
        }
    }
    if let Some(source) = source {
        match plan.mode {
            InstallMode::Copy => {
                package::copy(&source.directory, &staged)?;
                if package::tree_digest(&staged, false)? != source.digest {
                    return Err(SkillsError::new(
                        ErrorCode::Conflict,
                        "Source changed while copying",
                    ));
                }
            }
            InstallMode::Symlink => package::symlink(&source.directory, &staged)?,
        }
        let now = crate::models::now_ms();
        manifest.skills.insert(
            item.install_name.clone(),
            Installation {
                installation_id: installation_id(&item.destination),
                skill_id: source.skill_id.clone(),
                source_id: source.source_id.clone(),
                relative_path: source.relative_path.clone(),
                install_name: item.install_name.clone(),
                source_dir: source.directory.clone(),
                mode: plan.mode,
                revision: source.revision.clone(),
                source_digest: source.digest.clone(),
                installed_state: package::state(&staged)?,
                installed_at_ms: item.previous.as_ref().map_or(now, |p| p.installed_at_ms),
                updated_at_ms: now,
                managed_resources: resources.to_vec(),
            },
        );
    } else {
        let previous = item
            .previous
            .as_ref()
            .ok_or_else(|| SkillsError::invalid("Missing uninstall ownership"))?;
        if manifest
            .skills
            .get(&item.install_name)
            .is_none_or(|current| current.installation_id != previous.installation_id)
        {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Installation ownership changed",
            ));
        }
        manifest.skills.remove(&item.install_name);
    }
    files::atomic_json(&staged_manifest, &manifest)?;
    Ok(vec![
        Change {
            target: item.destination.clone(),
            before: item.before.clone(),
            after: package::state(&staged)?,
            staged,
            backup,
        },
        Change {
            target: manifest_path.clone(),
            before: item.manifest_before.clone(),
            after: package::state(&staged_manifest)?,
            staged: staged_manifest,
            backup: transactions::artifact(&manifest_path, id, offset + 1, "backup")?,
        },
    ])
}
