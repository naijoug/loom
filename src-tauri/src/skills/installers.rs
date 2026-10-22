use super::{
    catalog,
    error::{ErrorCode, Result, SkillsError},
    files,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn recovery_items(repo: &Repository, context: &TargetContext) -> Result<Vec<RecoveryItem>> {
    let lease = repo.coordinator.enter()?;
    let writer = repo.coordinator.writer_wait(&lease)?;
    let roots = context
        .targets()?
        .into_iter()
        .filter(|t| t.blocked_reason.is_none())
        .map(|t| t.skills_dir)
        .collect::<Vec<_>>();
    Ok(writer
        .pending()?
        .into_iter()
        .map(|pending| {
            let can_recover = pending.write_set.iter().all(|path| roots.contains(path));
            RecoveryItem {
                operation_id: pending.operation_id,
                resources: pending.write_set,
                can_recover,
                message: (!can_recover).then(|| {
                    "Select the original registered project to recover this operation".into()
                }),
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
    let roots = context
        .targets()?
        .into_iter()
        .filter(|t| t.blocked_reason.is_none())
        .map(|t| t.skills_dir)
        .collect::<Vec<_>>();
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
                "Legacy installation manifest requires migration before changes",
            ));
        }
        return Ok(Manifest::default());
    }
    let value: serde_json::Value = files::read_json(&path)?;
    if value["version"] == 1 {
        return Err(SkillsError::new(
            ErrorCode::Conflict,
            "Legacy installation manifest requires migration before changes",
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
        if !entry.managed_resources.is_empty() {
            return Err(SkillsError::recovery("This installation contains instruction resources requiring a compatible manager version"));
        }
        validate_name(name)?;
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
    let root = catalog::scan_root(&source_root)?;
    let file = catalog::resolve_file(&root, &relative)?;
    if file.file_name().is_none_or(|n| n != "SKILL.md") {
        return Err(SkillsError::invalid("Not a skill entry"));
    }
    let directory = file.parent().unwrap().to_path_buf();
    let detail = catalog::detail(&source_id, &source_root, &relative)?;
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
        revision: if source.kind == SourceKind::Git {
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
    inventory_locked(context, &writer)
}

fn inventory_locked(
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
                    update_available: false,
                    message: Some(error.message),
                });
                continue;
            }
        };
        for entry in manifest.skills.values() {
            let destination = target.skills_dir.join(&entry.install_name);
            let actual = package::state(&destination)?;
            result.push(InstallStatus {
                target: target.clone(),
                skill_id: entry.skill_id.clone(),
                destination,
                state: if actual == EntryState::Missing {
                    "missing"
                } else if actual != entry.installed_state {
                    "modified"
                } else {
                    "installed"
                }
                .into(),
                installation: Some(entry.clone()),
                update_available: false,
                message: None,
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
    save_plan(repo, &plan)?;
    Ok(plan)
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
    let all = inventory_locked(context, &writer)?;
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
        let blocked = writer
            .check_write_set(std::slice::from_ref(&status.target.skills_dir))
            .err()
            .map(|e| e.message)
            .or_else(|| {
                (status.state == "modified")
                    .then(|| "Installed content has user changes; uninstall refused".into())
            });
        plan.items.push(PlanItem {
            target: status.target.clone(),
            install_name: previous.install_name.clone(),
            destination: status.destination.clone(),
            before,
            manifest_before: package::state(&status.target.skills_dir.join(MANIFEST))?,
            previous: Some(previous),
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
    let plan: InstallPlan = files::read_json(&plan_path(repo, plan_id)?)?;
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

fn apply_item(
    repo: &Repository,
    context: &TargetContext,
    writer: &super::coordination::WriterGuard<'_>,
    plan: &InstallPlan,
    item: &PlanItem,
    index: usize,
    operation: &Operation,
) -> Result<String> {
    let target = context
        .selected(std::slice::from_ref(&item.target.id))?
        .remove(0);
    validate_name(&item.install_name)?;
    if target.skills_dir != item.target.skills_dir
        || item.destination != target.skills_dir.join(&item.install_name)
    {
        return Err(SkillsError::invalid("Planned target mismatch"));
    }
    let id = format!("{}-{index}", plan.plan_id);
    if let Some(receipt) = transactions::receipt(&repo.root, &id)? {
        return if receipt.outcome == Outcome::Committed {
            Ok(if plan.action == PlanAction::Install {
                "installed"
            } else {
                "uninstalled"
            }
            .into())
        } else {
            Err(SkillsError::new(
                ErrorCode::Conflict,
                "Previous attempt was rolled back; review a new plan",
            ))
        };
    }
    writer.check_write_set(std::slice::from_ref(&target.skills_dir))?;
    let manifest_path = target.skills_dir.join(MANIFEST);
    if package::state(&item.destination)? != item.before
        || package::state(&manifest_path)? != item.manifest_before
    {
        return Err(SkillsError::new(
            ErrorCode::Conflict,
            "Target changed after preview; review a new plan",
        ));
    }
    let mut manifest = load_manifest(&target)?;
    let source = if plan.action == PlanAction::Install {
        let expected = plan
            .source
            .as_ref()
            .ok_or_else(|| SkillsError::invalid("Missing install source"))?;
        let actual = resolve(repo, &expected.skill_id)?;
        if actual.directory != expected.directory
            || actual.digest != expected.digest
            || actual.name != item.install_name
            || actual.revision != expected.revision
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
    context.ensure(&target)?;
    let staged = transactions::artifact(&item.destination, &id, 0, "staged")?;
    let backup = transactions::artifact(&item.destination, &id, 0, "backup")?;
    let staged_manifest = transactions::artifact(&manifest_path, &id, 1, "staged")?;
    for path in [&staged, &backup, &staged_manifest] {
        if package::state(path)? != EntryState::Missing {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Transaction artifact already exists; inspect previous attempt",
            ));
        }
    }
    transactions::prepare_storage(&item.destination, &id)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if fs::metadata(&target.skills_dir)?.dev() != fs::metadata(staged.parent().unwrap())?.dev()
        {
            return Err(SkillsError::invalid("Target mount does not support same-filesystem transaction storage outside its skill directory"));
        }
    }
    let prepare = (|| {
        if let Some(source) = &source {
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
            };
            let now = crate::models::now_ms();
            let installation = Installation {
                installation_id: format!(
                    "installed-{}",
                    &files::digest(item.destination.to_string_lossy().as_bytes())[..32]
                ),
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
                managed_resources: Vec::new(),
            };
            manifest
                .skills
                .insert(item.install_name.clone(), installation);
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
        let changes = vec![
            Change {
                target: item.destination.clone(),
                before: item.before.clone(),
                after: package::state(&staged)?,
                staged: staged.clone(),
                backup,
            },
            Change {
                target: manifest_path.clone(),
                before: item.manifest_before.clone(),
                after: package::state(&staged_manifest)?,
                staged: staged_manifest.clone(),
                backup: transactions::artifact(&manifest_path, &id, 1, "backup")?,
            },
        ];
        operation.check_cancelled()?;
        transactions::prepare(repo, writer, &id, changes)
    })();
    let pending = match prepare {
        Ok(p) => p,
        Err(error) => {
            for path in [&staged, &staged_manifest] {
                if fs::symlink_metadata(path).is_ok() {
                    let _ = package::remove_entry(path);
                }
            }
            return Err(error);
        }
    };
    if let Err(error) =
        transactions::commit(writer, &pending, std::slice::from_ref(&target.skills_dir))
    {
        if let Err(recovery) =
            transactions::recover(writer, &pending, std::slice::from_ref(&target.skills_dir))
        {
            return Err(SkillsError::recovery(format!(
                "{}; {}",
                error.message, recovery.message
            )));
        }
        return Err(error);
    }
    Ok(if plan.action == PlanAction::Install {
        "installed"
    } else {
        "uninstalled"
    }
    .into())
}
