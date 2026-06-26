use crate::{
    models::{now_ms, AppSettings},
    storage,
};
use std::{fs, path::Path};
use tauri::AppHandle;

const SETTINGS_FILE: &str = "settings.json";
const MIN_COMMAND_TIMEOUT_SECONDS: u64 = 5;
const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 3600;

fn settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(storage::global_config_dir(app)?.join(SETTINGS_FILE))
}

fn normalize_settings(mut settings: AppSettings) -> AppSettings {
    settings.command_timeout_seconds = settings
        .command_timeout_seconds
        .clamp(MIN_COMMAND_TIMEOUT_SECONDS, MAX_COMMAND_TIMEOUT_SECONDS);
    settings
}

pub fn load_app_settings_from_path(path: &Path) -> Result<AppSettings, String> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read app settings: {error}"))?;
    match serde_json::from_str::<AppSettings>(&content) {
        Ok(settings) => Ok(normalize_settings(settings)),
        Err(error) => {
            let backup_path = path.with_file_name(format!("{SETTINGS_FILE}.bak-{}", now_ms()));
            fs::rename(path, &backup_path)
                .map_err(|rename_error| format!("failed to back up invalid app settings after parse error ({error}): {rename_error}"))?;
            Ok(AppSettings::default())
        }
    }
}

pub fn save_app_settings_to_path(
    path: &Path,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let normalized = normalize_settings(settings);
    storage::atomic_write_json(path, &normalized)?;
    Ok(normalized)
}

pub fn load_app_settings_for_app(app: &AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(app)?;
    load_app_settings_from_path(&path)
}

#[tauri::command]
pub fn load_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    load_app_settings_for_app(&app)
}

#[tauri::command]
pub fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let path = settings_path(&app)?;
    save_app_settings_to_path(&path, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_returns_defaults() {
        let root = std::env::temp_dir().join(format!("loom-settings-missing-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir");
        let path = root.join(SETTINGS_FILE);

        let settings =
            load_app_settings_from_path(&path).expect("missing file should load defaults");

        assert!(matches!(
            settings.theme_mode,
            crate::models::ThemeMode::System
        ));
        assert!(settings.confirm_before_commands);
        assert_eq!(settings.command_timeout_seconds, 600);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn save_then_load_round_trips_and_clamps_timeout() {
        let root = std::env::temp_dir().join(format!("loom-settings-roundtrip-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir");
        let path = root.join(SETTINGS_FILE);

        let saved = save_app_settings_to_path(
            &path,
            AppSettings {
                theme_mode: crate::models::ThemeMode::Dark,
                confirm_before_commands: false,
                command_timeout_seconds: 9999,
            },
        )
        .expect("settings should save");
        assert_eq!(saved.command_timeout_seconds, MAX_COMMAND_TIMEOUT_SECONDS);

        let loaded = load_app_settings_from_path(&path).expect("settings should load");
        assert!(matches!(loaded.theme_mode, crate::models::ThemeMode::Dark));
        assert!(!loaded.confirm_before_commands);
        assert_eq!(loaded.command_timeout_seconds, MAX_COMMAND_TIMEOUT_SECONDS);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn invalid_settings_are_backed_up_and_defaults_returned() {
        let root = std::env::temp_dir().join(format!("loom-settings-invalid-{}", now_ms()));
        fs::create_dir_all(&root).expect("test dir");
        let path = root.join(SETTINGS_FILE);
        fs::write(&path, "{not valid").expect("invalid settings");

        let settings = load_app_settings_from_path(&path).expect("invalid file should recover");

        assert!(matches!(
            settings.theme_mode,
            crate::models::ThemeMode::System
        ));
        assert!(!path.exists());
        assert!(fs::read_dir(&root)
            .expect("read dir")
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.json.bak-")));

        fs::remove_dir_all(root).ok();
    }
}
