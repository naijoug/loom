//! Desktop and headless clients share library defaults and the same lock domain.
use super::{
    error::{Result, SkillsError},
    repository::Repository,
};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct RuntimePaths {
    pub home: PathBuf,
    pub app_data: PathBuf,
    pub library: PathBuf,
}
impl RuntimePaths {
    pub fn system(data_override: Option<&Path>) -> Result<Self> {
        let config: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json"))
            .map_err(|_| SkillsError::invalid("Invalid bundled application configuration"))?;
        let identifier = config["identifier"]
            .as_str()
            .ok_or_else(|| SkillsError::invalid("Missing application identity"))?;
        let home = dirs::home_dir().ok_or_else(|| SkillsError::invalid("User home unavailable"))?;
        let app_data = dirs::data_dir()
            .ok_or_else(|| SkillsError::invalid("Application data directory unavailable"))?
            .join(identifier);
        Self::from_base(home, app_data, data_override)
    }
    pub fn from_base(
        home: PathBuf,
        app_data: PathBuf,
        data_override: Option<&Path>,
    ) -> Result<Self> {
        let library = data_override
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("LOOM_SKILLS_DATA_DIR").map(PathBuf::from))
            .or_else(|| {
                std::env::var_os("LOOM_PROFILE_DIR").map(|p| PathBuf::from(p).join("skills"))
            })
            .unwrap_or_else(|| app_data.join("skills"));
        if !home.is_absolute() || !app_data.is_absolute() || !library.is_absolute() {
            return Err(SkillsError::invalid(
                "Skills runtime paths must be absolute",
            ));
        }
        if home.starts_with(&library) {
            return Err(SkillsError::invalid(
                "Choose a dedicated library directory, not the filesystem or home root",
            ));
        }
        Ok(Self {
            home,
            app_data,
            library,
        })
    }
    pub fn repository(&self) -> Result<Repository> {
        Repository::open(&self.library, &self.app_data.join("skills-coordination"))
    }
}
