use crate::{models::now_ms, storage};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{fs, path::Path};

pub const TASK_SCHEMA_VERSION: u32 = 1;
pub const AGENT_STORE_SCHEMA_VERSION: u32 = 1;
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const TERMINAL_STORE_SCHEMA_VERSION: u32 = 1;
pub const PROJECT_PREFERENCES_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionedDocument<'a, T> {
    schema_version: u32,
    data: &'a T,
}

pub fn read_versioned_json<T>(path: &Path, kind: &str, current_version: u32) -> Result<T, String>
where
    T: DeserializeOwned + Serialize,
{
    let content = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let document = serde_json::from_str::<Value>(&content)
        .map_err(|error| incompatible_store_error(path, kind, format!("invalid JSON: {error}")))?;

    let (stored_version, payload, needs_upgrade) = decode_document(path, kind, document)?;
    if stored_version > current_version {
        return Err(incompatible_store_error(
            path,
            kind,
            format!(
                "schema version {stored_version} is newer than supported version {current_version}"
            ),
        ));
    }
    if stored_version != 0 && stored_version != current_version {
        return Err(incompatible_store_error(
            path,
            kind,
            format!("no migration path from schema version {stored_version} to {current_version}"),
        ));
    }

    let data = serde_json::from_value::<T>(payload).map_err(|error| {
        incompatible_store_error(path, kind, format!("invalid payload: {error}"))
    })?;
    if needs_upgrade {
        write_versioned_json(path, current_version, &data)?;
    }
    Ok(data)
}

pub fn write_versioned_json<T>(path: &Path, current_version: u32, data: &T) -> Result<(), String>
where
    T: Serialize,
{
    storage::atomic_write_json(
        path,
        &VersionedDocument {
            schema_version: current_version,
            data,
        },
    )
}

fn decode_document(path: &Path, kind: &str, document: Value) -> Result<(u32, Value, bool), String> {
    let mut object = match document {
        Value::Object(object) => object,
        legacy => return Ok((0, legacy, true)),
    };
    let Some(version_value) = object.remove("schemaVersion") else {
        return Ok((0, Value::Object(object), true));
    };
    let version = version_value.as_u64().ok_or_else(|| {
        incompatible_store_error(path, kind, "schemaVersion must be an unsigned integer")
    })?;
    let version = u32::try_from(version).map_err(|_| {
        incompatible_store_error(path, kind, "schemaVersion is outside the supported range")
    })?;

    if version == 0 {
        let payload = object.remove("data").unwrap_or(Value::Object(object));
        return Ok((0, payload, true));
    }

    let payload = object.remove("data").ok_or_else(|| {
        incompatible_store_error(path, kind, "versioned document is missing data")
    })?;
    Ok((version, payload, false))
}

fn incompatible_store_error(path: &Path, kind: &str, detail: impl AsRef<str>) -> String {
    match backup_incompatible_store(path, kind) {
        Ok(backup_path) => format!(
            "cannot load {kind} store {}: {}; backup copied to {}",
            path.display(),
            detail.as_ref(),
            backup_path.display()
        ),
        Err(backup_error) => format!(
            "cannot load {kind} store {}: {}; backup also failed: {backup_error}",
            path.display(),
            detail.as_ref()
        ),
    }
}

fn backup_incompatible_store(path: &Path, kind: &str) -> Result<std::path::PathBuf, String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("invalid store file name: {}", path.display()))?;
    let safe_kind = kind
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let backup_path = path.with_file_name(format!("{file_name}.bak-{safe_kind}-{}", now_ms()));
    fs::copy(path, &backup_path).map_err(|error| {
        format!(
            "failed to copy {} to {}: {error}",
            path.display(),
            backup_path.display()
        )
    })?;
    Ok(backup_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        value: String,
    }

    fn test_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("loom-migration-{label}-{}", now_ms()));
        fs::create_dir_all(&root).expect("test root");
        root
    }

    #[test]
    fn legacy_document_is_upgraded_to_versioned_envelope() {
        let root = test_root("legacy");
        let path = root.join("fixture.json");
        fs::write(&path, r#"{"value":"legacy"}"#).expect("legacy fixture");

        let fixture = read_versioned_json::<Fixture>(&path, "fixture", 1)
            .expect("legacy document should migrate");
        let migrated: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("migrated content"))
                .expect("migrated JSON");

        assert_eq!(fixture.value, "legacy");
        assert_eq!(migrated["schemaVersion"], 1);
        assert_eq!(migrated["data"]["value"], "legacy");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn current_document_is_read_without_rewriting() {
        let root = test_root("idempotent");
        let path = root.join("fixture.json");
        let content = "{\n  \"schemaVersion\": 1,\n  \"data\": {\"value\": \"stable\"}\n}\n";
        fs::write(&path, content).expect("versioned fixture");

        let fixture =
            read_versioned_json::<Fixture>(&path, "fixture", 1).expect("current document");

        assert_eq!(fixture.value, "stable");
        assert_eq!(
            fs::read_to_string(&path).expect("unchanged fixture"),
            content
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn future_document_is_preserved_and_backed_up() {
        let root = test_root("future");
        let path = root.join("fixture.json");
        let content = r#"{"schemaVersion":99,"data":{"value":"future"}}"#;
        fs::write(&path, content).expect("future fixture");

        let error = read_versioned_json::<Fixture>(&path, "fixture", 1)
            .expect_err("future schema should be rejected");

        assert!(error.contains("newer than supported"));
        assert_eq!(
            fs::read_to_string(&path).expect("original remains"),
            content
        );
        assert!(fs::read_dir(&root)
            .expect("backup directory")
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("fixture.json.bak-fixture-")));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn invalid_payload_is_backed_up() {
        let root = test_root("invalid");
        let path = root.join("fixture.json");
        fs::write(&path, r#"{"schemaVersion":1,"data":{"wrong":true}}"#).expect("invalid fixture");

        let error = read_versioned_json::<Fixture>(&path, "fixture", 1)
            .expect_err("invalid payload should fail");

        assert!(error.contains("invalid payload"));
        assert!(fs::read_dir(&root)
            .expect("backup directory")
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("fixture.json.bak-fixture-")));
        fs::remove_dir_all(root).ok();
    }
}
