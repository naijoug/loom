use super::{
    error::{Result, SkillsError},
    package,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallTarget {
    pub id: String,
    pub tool_id: String,
    pub label: String,
    pub scope: String,
    pub skills_dir: PathBuf,
    pub exists: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Clone)]
pub struct TargetContext {
    home: PathBuf,
    project: Option<PathBuf>,
}

impl TargetContext {
    pub fn new(home: &Path, project: Option<&Path>) -> Result<Self> {
        let home = home.canonicalize()?;
        if !home.is_dir() || home.parent().is_none() {
            return Err(SkillsError::invalid("Invalid user home"));
        }
        let project = project.map(|p| p.canonicalize()).transpose()?;
        if project
            .as_ref()
            .is_some_and(|p| !p.is_dir() || p.parent().is_none())
        {
            return Err(SkillsError::invalid("Invalid project root"));
        }
        Ok(Self { home, project })
    }
    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }
    pub fn targets(&self) -> Result<Vec<InstallTarget>> {
        let mut result = Vec::new();
        let mut paths = std::collections::BTreeSet::new();
        for (scope, root) in [
            ("global", Some(self.home.as_path())),
            ("project", self.project.as_deref()),
        ] {
            let Some(root) = root else {
                continue;
            };
            for (tool, label, folder) in [
                ("codex", "Codex / ChatGPT", ".codex"),
                ("claude-code", "Claude Code", ".claude"),
                ("amp", "Amp / Codex", ".agents"),
            ] {
                let (path, blocked_reason) =
                    match safe_child_directory(root, &[folder, "skills"], false) {
                        Ok(path) => (path, None),
                        Err(error) => (root.join(folder).join("skills"), Some(error.message)),
                    };
                if !paths.insert(path.clone()) {
                    continue;
                }
                result.push(InstallTarget {
                    id: format!("{tool}-{scope}"),
                    tool_id: tool.into(),
                    label: label.into(),
                    scope: scope.into(),
                    exists: path.is_dir(),
                    skills_dir: path,
                    blocked_reason,
                });
            }
        }
        Ok(result)
    }
    pub fn selected(&self, ids: &[String]) -> Result<Vec<InstallTarget>> {
        if ids.is_empty() || ids.len() > 12 {
            return Err(SkillsError::invalid(
                "Choose one or more installation targets",
            ));
        }
        let targets = self.targets()?;
        let mut selected = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for id in ids {
            let id = id.replace("chatgpt-", "codex-");
            let target = targets.iter().find(|t| t.id == id).ok_or_else(|| {
                SkillsError::invalid("Unknown or unavailable installation target")
            })?;
            if let Some(reason) = &target.blocked_reason {
                return Err(SkillsError::invalid(reason.clone()));
            }
            if seen.insert(target.skills_dir.clone()) {
                selected.push(target.clone());
            }
        }
        Ok(selected)
    }
    pub fn ensure(&self, target: &InstallTarget) -> Result<()> {
        let current = self.selected(std::slice::from_ref(&target.id))?.remove(0);
        if current.skills_dir != target.skills_dir {
            return Err(SkillsError::invalid("Installation target changed"));
        }
        let root = if target.scope == "global" {
            &self.home
        } else {
            self.project
                .as_ref()
                .ok_or_else(|| SkillsError::invalid("Project target unavailable"))?
        };
        let relative = target
            .skills_dir
            .strip_prefix(root)
            .map_err(|_| SkillsError::invalid("Installation target escaped its root"))?;
        let parts = relative
            .iter()
            .map(|p| {
                p.to_str()
                    .ok_or_else(|| SkillsError::invalid("Non-UTF-8 target"))
            })
            .collect::<Result<Vec<_>>>()?;
        safe_child_directory(root, &parts, true)?;
        Ok(())
    }
}

fn safe_child_directory(root: &Path, segments: &[&str], create: bool) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for segment in segments {
        if segment.is_empty() || matches!(*segment, "." | "..") || segment.contains(['/', '\\']) {
            return Err(SkillsError::invalid("Invalid target directory component"));
        }
        path.push(segment);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => (),
            Ok(_) => {
                return Err(SkillsError::invalid(
                    "Installation directory must not traverse a symbolic link or file",
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if create {
                    fs::create_dir(&path)?;
                    package::set_mode(&path, 0o700)?;
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path)
}
