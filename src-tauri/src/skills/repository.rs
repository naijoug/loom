use super::{
    catalog::{self, CatalogIssue, SkillDetail, SkillSummary},
    coordination::Coordinator,
    error::{ErrorCode, Result, SkillsError},
    files, snapshots,
    sources::GitSource,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Local,
    Git,
    GithubApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub local_path: Option<PathBuf>,
    pub git: Option<GitSource>,
    pub revision: Option<String>,
    pub updated_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceLibrary {
    pub schema_version: u32,
    pub sources: Vec<Source>,
}
impl Default for SourceLibrary {
    fn default() -> Self {
        Self {
            schema_version: 1,
            sources: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub sources: Vec<Source>,
    pub skills: Vec<SkillSummary>,
    pub issues: Vec<CatalogIssue>,
}

#[derive(Clone)]
pub struct Repository {
    pub root: PathBuf,
    pub coordinator: Coordinator,
}

impl Repository {
    pub fn open(data_dir: &Path, coordination_dir: &Path) -> Result<Self> {
        Ok(Self {
            root: files::private_directory(data_dir)?,
            coordinator: Coordinator::open(coordination_dir)?,
        })
    }
    fn library_path(&self) -> PathBuf {
        self.root.join("library.json")
    }
    pub fn load(&self) -> Result<SourceLibrary> {
        let path = self.library_path();
        files::regular_file(&path)?;
        if !path.try_exists()? {
            return Ok(SourceLibrary::default());
        }
        let library: SourceLibrary = files::read_json(&path)?;
        if library.schema_version != 1 {
            return Err(SkillsError::recovery("Unsupported Skills library schema"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for source in &library.sources {
            files::validate_id(&source.id)?;
            if !seen.insert(&source.id) {
                return Err(SkillsError::recovery(
                    "Duplicate source identity in library",
                ));
            }
            match source.kind {
                SourceKind::Local
                    if source
                        .local_path
                        .as_ref()
                        .is_some_and(|p| p.is_absolute() && p.parent().is_some())
                        && source.git.is_none()
                        && source.revision.is_none()
                        && source.default_branch.is_none() => {}
                SourceKind::Git | SourceKind::GithubApi if source.local_path.is_none() => {
                    let git = source
                        .git
                        .as_ref()
                        .ok_or_else(|| SkillsError::recovery("Git source metadata missing"))?;
                    let normalized = super::sources::normalize_git_url(&git.clone_url)?;
                    let expected_id = if source.kind == SourceKind::GithubApi {
                        super::github_api::source_id(git)?
                    } else {
                        git.source_id.clone()
                    };
                    if normalized != *git || source.id != expected_id {
                        return Err(SkillsError::recovery("Git source identity mismatch"));
                    }
                    if source.kind == SourceKind::GithubApi {
                        super::github_api::validate_branch(
                            source.default_branch.as_deref().ok_or_else(|| {
                                SkillsError::recovery("GitHub API default branch missing")
                            })?,
                        )?;
                    } else if source.default_branch.is_some() {
                        return Err(SkillsError::recovery(
                            "Unexpected API metadata on a Git source",
                        ));
                    }
                    files::validate_id(
                        source
                            .revision
                            .as_deref()
                            .ok_or_else(|| SkillsError::recovery("Git revision missing"))?,
                    )?;
                }
                _ => return Err(SkillsError::recovery("Invalid source metadata")),
            }
        }
        Ok(library)
    }

    pub fn list(&self) -> Result<LibraryView> {
        let _lease = self.coordinator.enter()?;
        let library = self.load()?;
        let mut result = LibraryView {
            sources: library.sources,
            skills: vec![],
            issues: vec![],
        };
        for source in &result.sources {
            match self.source_directory(source).and_then(|directory| {
                if source.kind == SourceKind::GithubApi {
                    catalog::scan_repository(&source.id, &directory)
                } else {
                    catalog::scan(&source.id, &directory)
                }
            }) {
                Ok(catalog) => {
                    result.skills.extend(catalog.skills);
                    result.issues.extend(catalog.issues);
                }
                Err(error) => result.issues.push(CatalogIssue {
                    source_id: source.id.clone(),
                    relative_path: String::new(),
                    message: error.message,
                }),
            }
        }
        result.skills.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        Ok(result)
    }

    pub fn source_directory(&self, source: &Source) -> Result<PathBuf> {
        match source.kind {
            SourceKind::Local => Ok(source
                .local_path
                .clone()
                .ok_or_else(|| SkillsError::recovery("Local source path missing"))?),
            SourceKind::Git | SourceKind::GithubApi => {
                let revision = source
                    .revision
                    .as_deref()
                    .ok_or_else(|| SkillsError::recovery("Git source revision missing"))?;
                let path = snapshots::snapshot_path(&self.root, &source.id, revision)?;
                let snapshot = snapshots::verify(&path)?;
                if snapshot.source_id != source.id || snapshot.revision != revision {
                    return Err(SkillsError::recovery("Snapshot identity mismatch"));
                }
                Ok(path.join("contents"))
            }
        }
    }

    pub fn detail(&self, skill_id: &str) -> Result<SkillDetail> {
        let _lease = self.coordinator.enter()?;
        let (source_id, relative) = catalog::decode_id(skill_id)?;
        let source = self.get_source(&source_id)?;
        if source.kind == SourceKind::GithubApi {
            catalog::detail_repository(&source.id, &self.source_directory(&source)?, &relative)
        } else {
            catalog::detail(&source.id, &self.source_directory(&source)?, &relative)
        }
    }

    pub fn get_source(&self, id: &str) -> Result<Source> {
        self.load()?
            .sources
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| SkillsError::invalid("Unknown skill source"))
    }

    pub fn add_local(&self, path: &Path) -> Result<Source> {
        if !path.is_absolute() {
            return Err(SkillsError::invalid("Choose an absolute local folder"));
        }
        let path = path.canonicalize()?;
        catalog::scan_root(&path)?;
        let id = format!(
            "src-{}",
            &files::digest(path.to_string_lossy().as_bytes())[..32]
        );
        let source = Source {
            id,
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            kind: SourceKind::Local,
            local_path: Some(path),
            git: None,
            revision: None,
            updated_at_ms: crate::models::now_ms(),
            default_branch: None,
        };
        // Validate before persisting; individual malformed packages remain visible
        // as issues when other packages are usable, including a valid empty folder.
        catalog::scan(&source.id, source.local_path.as_ref().unwrap())?;
        let lease = self.coordinator.enter()?;
        let writer = self.coordinator.writer_wait(&lease)?;
        writer.check_write_set(&[self.library_path()])?;
        let mut library = self.load()?;
        if let Some(existing) = library.sources.iter().find(|s| s.id == source.id) {
            return Ok(existing.clone());
        }
        library.sources.push(source.clone());
        files::atomic_json(&self.library_path(), &library)?;
        Ok(source)
    }

    pub fn remove_source(&self, id: &str) -> Result<()> {
        let lease = self.coordinator.enter()?;
        let writer = self.coordinator.writer_wait(&lease)?;
        writer.check_write_set(&[self.library_path()])?;
        let mut library = self.load()?;
        if !library.sources.iter().any(|s| s.id == id) {
            return Err(SkillsError::invalid("Unknown skill source"));
        }
        library.sources.retain(|s| s.id != id);
        files::atomic_json(&self.library_path(), &library)?;
        // Never delete a local source or snapshots: installed links and rollback
        // records may still use them. Physical cleanup is deliberately separate.
        Ok(())
    }

    pub fn commit_git(
        &self,
        source: Source,
        expected_revision: Option<&str>,
        staging: &Path,
        snapshot: &snapshots::Snapshot,
    ) -> Result<()> {
        let git = source
            .git
            .as_ref()
            .ok_or_else(|| SkillsError::invalid("Remote source metadata missing"))?;
        let normalized = super::sources::normalize_git_url(&git.clone_url)?;
        let expected_id = match source.kind {
            SourceKind::Git => git.source_id.clone(),
            SourceKind::GithubApi => super::github_api::source_id(git)?,
            SourceKind::Local => {
                return Err(SkillsError::invalid(
                    "Cannot publish a remote snapshot as a local source",
                ))
            }
        };
        if normalized != *git
            || source.id != expected_id
            || source.local_path.is_some()
            || snapshot.source_id != source.id
            || source.revision.as_deref() != Some(snapshot.revision.as_str())
        {
            return Err(SkillsError::invalid(
                "Remote source and snapshot identities do not match",
            ));
        }
        if source.kind == SourceKind::GithubApi {
            super::github_api::validate_branch(
                source
                    .default_branch
                    .as_deref()
                    .ok_or_else(|| SkillsError::invalid("GitHub default branch missing"))?,
            )?;
        } else if source.default_branch.is_some() {
            return Err(SkillsError::invalid("Unexpected API branch metadata"));
        }
        let lease = self.coordinator.enter()?;
        let writer = self.coordinator.writer_wait(&lease)?;
        writer.check_write_set(&[self.library_path()])?;
        let mut library = self.load()?;
        let existing = library.sources.iter().find(|s| s.id == source.id);
        if existing.and_then(|s| s.revision.as_deref()) != expected_revision {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Source changed while refreshing; reload before retrying",
            ));
        }
        if let Some(other) = library.sources.iter().find(|s| {
            s.id != source.id
                && s.kind == source.kind
                && s.git.as_ref().is_some_and(|g| {
                    source
                        .git
                        .as_ref()
                        .is_some_and(|v| v.legacy_id == g.legacy_id)
                })
        }) {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                format!("Source identity conflicts with {}", other.name),
            ));
        }
        snapshots::publish(&self.root, staging, snapshot, &writer)?;
        library.sources.retain(|s| s.id != source.id);
        library.sources.push(source);
        files::atomic_json(&self.library_path(), &library)
    }

    pub fn prepare_directory(&self, operation_id: &str) -> Result<PathBuf> {
        files::validate_id(operation_id)?;
        files::private_directory(&self.root.join("staging"))?;
        let path = self.root.join("staging").join(operation_id);
        if path.try_exists()? {
            return Err(SkillsError::new(
                ErrorCode::Conflict,
                "Operation staging already exists",
            ));
        }
        fs::create_dir(&path)?;
        Ok(path)
    }
}
