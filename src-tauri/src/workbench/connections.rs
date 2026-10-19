use loom_core::{
    ConnectionAuth, Core, CredentialChange, ErrorCode, Id, ModelConnection, SaveConnectionMetadata,
    SaveConnectionRequest,
};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Mutex};
use zeroize::Zeroizing;

type Secret = Zeroizing<Vec<u8>>;
trait Vault: Send + Sync {
    fn backend(&self) -> &'static str;
    fn get(&self, id: &Id) -> Result<Option<Secret>, String>;
    fn put(&self, id: &Id, value: &[u8]) -> Result<(), String>;
}
struct SystemVault {
    service: String,
}
impl SystemVault {
    fn new(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|_| "无法确定凭据库作用域")?;
        let scope = format!("{:x}", Sha256::digest(root.as_os_str().as_encoded_bytes()));
        Ok(Self {
            service: format!("com.naijoug.loom.workbench.credentials.{scope}"),
        })
    }
}
#[cfg(target_os = "macos")]
impl Vault for SystemVault {
    fn backend(&self) -> &'static str {
        "macos_keychain"
    }
    fn get(&self, id: &Id) -> Result<Option<Secret>, String> {
        match security_framework::passwords::get_generic_password(&self.service, id.as_str()) {
            Ok(bytes) => Ok(Some(Zeroizing::new(bytes))),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(error) => Err(format!("系统凭据库读取失败（{}）", error.code())),
        }
    }
    fn put(&self, id: &Id, value: &[u8]) -> Result<(), String> {
        security_framework::passwords::set_generic_password(&self.service, id.as_str(), value)
            .map_err(|error| format!("系统凭据库保存失败（{}）", error.code()))
    }
}
#[cfg(not(target_os = "macos"))]
impl Vault for SystemVault {
    fn backend(&self) -> &'static str {
        let _ = &self.service;
        "unavailable"
    }
    fn get(&self, _: &Id) -> Result<Option<Secret>, String> {
        Err("此平台尚未接入系统凭据库".into())
    }
    fn put(&self, _: &Id, _: &[u8]) -> Result<(), String> {
        Err("此平台尚未接入系统凭据库".into())
    }
}

pub(super) struct Connections {
    core: Core,
    vault: Box<dyn Vault>,
    mutation: Mutex<()>,
}
impl Connections {
    pub(super) fn new(core: Core, root: &Path) -> Result<Self, String> {
        Ok(Self {
            core,
            vault: Box::new(SystemVault::new(root)?),
            mutation: Mutex::new(()),
        })
    }
    pub(super) fn backend(&self) -> &'static str {
        self.vault.backend()
    }
    /// Only Rust execution code can resolve a frozen immutable vault slot.
    pub(super) fn runtime_secret(
        &self,
        connection: &ModelConnection,
    ) -> Result<Option<Secret>, String> {
        if connection.config.auth == ConnectionAuth::None {
            return Ok(None);
        }
        let slot = connection
            .credential_ref
            .as_ref()
            .ok_or("连接缺少凭据引用")?;
        let secret = self
            .vault
            .get(slot)?
            .ok_or("系统凭据库中已找不到该凭据，请重新输入")?;
        let text = std::str::from_utf8(&secret).map_err(|_| "凭据不是有效文本")?;
        if text.is_empty()
            || text.len() > 64 * 1024
            || text.trim() != text
            || text.contains(['\r', '\n', '\0'])
        {
            return Err("已保存凭据格式无效，请重新输入".into());
        }
        Ok(Some(secret))
    }
    pub(super) fn save(&self, request: SaveConnectionRequest) -> Result<ModelConnection, String> {
        let _guard = self.mutation.lock().map_err(|_| "连接保存器不可用")?;
        // Convert immediately; do not format/debug or serialize the secret input.
        let (secret, keep) = match request.credential {
            CredentialChange::Replace { value } => (Some(Zeroizing::new(value)), false),
            CredentialChange::Keep => (None, true),
            CredentialChange::Clear => (None, false),
        };
        request.config.validate().map_err(|e| e.to_string())?;
        let previous = match self.core.get_connection(&request.id) {
            Ok(connection) => Some(connection),
            Err(e) if e.code == ErrorCode::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        let repeated = previous
            .as_ref()
            .is_some_and(|old| old.operation_id == request.operation_id);
        if !repeated && request.expected_revision != previous.as_ref().map(|old| old.revision) {
            return Err("连接已被修改，请重新加载后保存".into());
        }
        if repeated
            && previous
                .as_ref()
                .is_some_and(|old| old.config != request.config)
        {
            return Err("保存请求 ID 已被不同配置使用".into());
        }
        let credential_ref = if request.config.auth == ConnectionAuth::None {
            if secret.is_some() {
                return Err("无认证连接不能附带凭据".into());
            }
            None
        } else if let Some(secret) = secret {
            if secret.is_empty()
                || secret.len() > 64 * 1024
                || secret.trim() != secret.as_str()
                || secret.contains(['\r', '\n', '\0'])
            {
                return Err("凭据不能为空、包含换行或超过 64 KiB".into());
            }
            let key = serde_json::to_vec(&(&request.id, &request.operation_id))
                .map_err(|_| "无法生成凭据标识")?;
            let slot = Id::new(format!("credential-{:x}", Sha256::digest(key)))
                .map_err(|e| e.to_string())?;
            match self.vault.get(&slot)? {
                Some(existing) if existing.as_slice() != secret.as_bytes() => {
                    return Err("相同保存请求不能替换成另一份凭据".into())
                }
                Some(_) => {}
                None => self.vault.put(&slot, secret.as_bytes())?,
            }
            Some(slot)
        } else if keep {
            let previous = previous.as_ref().ok_or("请先输入此连接的凭据")?;
            if !request.config.same_credential_destination(&previous.config) {
                return Err("更改提供方、认证方式或端点时必须重新输入凭据".into());
            }
            let slot = previous
                .credential_ref
                .clone()
                .ok_or("此连接没有已保存凭据")?;
            if self.vault.get(&slot)?.is_none() {
                return Err("系统凭据库中已找不到该凭据，请重新输入".into());
            }
            Some(slot)
        } else {
            return Err("当前认证方式需要凭据；清除时请选择无认证连接".into());
        };
        // Keep the immutable slot on uncertain journal IO for a safe retry.
        self.core
            .save_connection_metadata(SaveConnectionMetadata {
                id: request.id,
                operation_id: request.operation_id,
                expected_revision: request.expected_revision,
                config: request.config,
                credential_ref,
            })
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::{ConnectionConfig, ConnectionProvider, EndpointApi, MidStreamBehavior};
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc,
        },
    };
    static NEXT: AtomicU64 = AtomicU64::new(1);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "loom-vault-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[derive(Default)]
    struct Memory {
        values: Mutex<BTreeMap<Id, Vec<u8>>>,
        fail: AtomicBool,
        reads: AtomicU64,
        writes: AtomicU64,
    }
    struct MemoryVault(Arc<Memory>);
    impl Vault for MemoryVault {
        fn backend(&self) -> &'static str {
            "test-memory"
        }
        fn get(&self, id: &Id) -> Result<Option<Secret>, String> {
            self.0.reads.fetch_add(1, Ordering::Relaxed);
            Ok(self
                .0
                .values
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .map(Zeroizing::new))
        }
        fn put(&self, id: &Id, value: &[u8]) -> Result<(), String> {
            self.0.writes.fetch_add(1, Ordering::Relaxed);
            if self.0.fail.load(Ordering::Acquire) {
                return Err("test vault unavailable".into());
            }
            self.0
                .values
                .lock()
                .unwrap()
                .insert(id.clone(), value.into());
            Ok(())
        }
    }
    fn id(value: &str) -> Id {
        Id::new(value).unwrap()
    }
    fn config() -> ConnectionConfig {
        ConnectionConfig {
            name: "Fixture".into(),
            provider: ConnectionProvider::PiCompat,
            auth: ConnectionAuth::ApiKey,
            endpoint: Some("https://fixture.invalid/v1".into()),
            pi_provider: None,
            custom_api: Some(EndpointApi::OpenaiCompletions),
            models: vec!["fixture-model".into()],
            default_model: "fixture-model".into(),
            supports_images: false,
            mid_stream_behavior: MidStreamBehavior::Queue,
        }
    }
    fn request(
        operation: &str,
        revision: Option<u64>,
        credential: CredentialChange,
    ) -> SaveConnectionRequest {
        SaveConnectionRequest {
            id: id("fixture"),
            operation_id: id(operation),
            expected_revision: revision,
            config: config(),
            credential,
        }
    }
    fn replace(value: &str) -> CredentialChange {
        CredentialChange::Replace {
            value: value.into(),
        }
    }
    fn make_service(f: &Fixture, memory: Arc<Memory>) -> Connections {
        Connections {
            core: Core::open(&f.0).unwrap(),
            vault: Box::new(MemoryVault(memory)),
            mutation: Mutex::new(()),
        }
    }

    #[test]
    fn secret_is_not_in_catalog_or_reply_and_same_operation_is_retryable() {
        let f = Fixture::new();
        let memory = Arc::new(Memory::default());
        let service = make_service(&f, memory.clone());
        let saved = service
            .save(request("first", None, replace("fixture-private-value")))
            .unwrap();
        assert_eq!(saved.revision, 1);
        assert!(!serde_json::to_string(&saved)
            .unwrap()
            .contains("fixture-private-value"));
        assert!(!fs::read_to_string(f.0.join("catalog/events.jsonl"))
            .unwrap()
            .contains("fixture-private-value"));
        assert_eq!(
            service
                .save(request("first", None, replace("fixture-private-value")))
                .unwrap(),
            saved
        );
        assert_eq!(memory.values.lock().unwrap().len(), 1);
        assert!(service
            .save(request("first", None, replace("different-private-value")))
            .is_err());
    }

    #[test]
    fn vault_failure_leaves_catalog_unchanged_and_stale_edits_do_not_write_secrets() {
        let f = Fixture::new();
        let memory = Arc::new(Memory::default());
        let service = make_service(&f, memory.clone());
        memory.fail.store(true, Ordering::Release);
        assert!(service
            .save(request("first", None, replace("fixture-private-value")))
            .is_err());
        assert!(service.core.list_connections().unwrap().is_empty());
        memory.fail.store(false, Ordering::Release);
        service
            .save(request("first", None, replace("fixture-private-value")))
            .unwrap();
        assert!(service
            .save(request("stale", None, replace("must-not-write")))
            .is_err());
        assert_eq!(memory.values.lock().unwrap().len(), 1);
    }

    #[test]
    fn runtime_reads_the_frozen_slot_and_never_returns_invalid_or_missing_secret_bytes() {
        let f = Fixture::new();
        let memory = Arc::new(Memory::default());
        let service = make_service(&f, memory.clone());
        let first = service
            .save(request("first", None, replace("first-private-value")))
            .unwrap();
        let second = service
            .save(request("second", Some(1), replace("second-private-value")))
            .unwrap();
        assert_eq!(
            service.runtime_secret(&first).unwrap().unwrap().as_slice(),
            b"first-private-value"
        );
        assert_eq!(
            service.runtime_secret(&second).unwrap().unwrap().as_slice(),
            b"second-private-value"
        );
        let slot = first.credential_ref.as_ref().unwrap();
        memory.values.lock().unwrap().remove(slot);
        assert!(service.runtime_secret(&first).is_err());
        memory
            .values
            .lock()
            .unwrap()
            .insert(slot.clone(), vec![255]);
        assert!(service.runtime_secret(&first).is_err());
        memory
            .values
            .lock()
            .unwrap()
            .insert(slot.clone(), b"private\ninvalid".to_vec());
        assert!(service.runtime_secret(&first).is_err());
    }

    #[test]
    fn changing_destination_requires_reentry_and_rotation_keeps_old_slots_immutable() {
        let f = Fixture::new();
        let memory = Arc::new(Memory::default());
        let service = make_service(&f, memory.clone());
        let first = service
            .save(request("first", None, replace("first-private-value")))
            .unwrap();
        let mut changed = request("second", Some(1), CredentialChange::Keep);
        changed.config.endpoint = Some("https://other.invalid/v1".into());
        assert!(service.save(changed).unwrap_err().contains("重新输入"));
        let mut changed = request("second", Some(1), replace("second-private-value"));
        changed.config.endpoint = Some("https://other.invalid/v1".into());
        let second = service.save(changed).unwrap();
        assert_ne!(first.credential_ref, second.credential_ref);
        assert_eq!(
            memory.values.lock().unwrap()[first.credential_ref.as_ref().unwrap()],
            b"first-private-value"
        );
        assert_eq!(
            memory.values.lock().unwrap()[second.credential_ref.as_ref().unwrap()],
            b"second-private-value"
        );
    }

    #[test]
    fn uncertain_catalog_commit_keeps_vault_slot_for_recovery() {
        let f = Fixture::new();
        let memory = Arc::new(Memory::default());
        let service = make_service(&f, memory.clone());
        let marker = f.0.join("catalog/commit.json");
        let backup = marker.with_extension("saved");
        fs::rename(&marker, &backup).unwrap();
        fs::create_dir(&marker).unwrap();
        assert!(service
            .save(request("first", None, replace("fixture-private-value")))
            .is_err());
        assert_eq!(memory.values.lock().unwrap().len(), 1);
        drop(service);
        fs::remove_dir(&marker).unwrap();
        fs::rename(backup, &marker).unwrap();
        let service = make_service(&f, memory.clone());
        let saved = service
            .save(request("first", None, replace("fixture-private-value")))
            .unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(service.core.list_connections().unwrap().len(), 1);
        assert_eq!(memory.values.lock().unwrap().len(), 1);
    }

    #[test]
    fn rename_keeps_existing_secret_and_empty_auth_does_not_contact_the_vault() {
        let f = Fixture::new();
        let memory = Arc::new(Memory::default());
        let service = make_service(&f, memory.clone());
        let first = service
            .save(request("first", None, replace("fixture-private-value")))
            .unwrap();
        let mut renamed = request("rename", Some(1), CredentialChange::Keep);
        renamed.config.name = "Renamed".into();
        let renamed = service.save(renamed).unwrap();
        assert_eq!(renamed.credential_ref, first.credential_ref);
        memory.fail.store(true, Ordering::Release);
        let mut local = request("local", Some(2), CredentialChange::Clear);
        local.config.auth = ConnectionAuth::None;
        let reads = memory.reads.load(Ordering::Relaxed);
        let writes = memory.writes.load(Ordering::Relaxed);
        let local = service.save(local).unwrap();
        assert_eq!(memory.reads.load(Ordering::Relaxed), reads);
        assert_eq!(memory.writes.load(Ordering::Relaxed), writes);
        assert!(local.credential_ref.is_none());
        assert_eq!(memory.values.lock().unwrap().len(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "Explicit local Keychain test; creates and deletes only one synthetic credential in a unique namespace"]
    fn native_keychain_roundtrip_and_catalog_contains_no_secret() {
        let f = Fixture::new();
        let core = Core::open(&f.0).unwrap();
        let service = Connections::new(core.clone(), &f.0).unwrap();
        let native = SystemVault::new(&f.0).unwrap();
        struct Cleanup {
            service: String,
            id: Option<Id>,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Some(id) = &self.id {
                    let _ = security_framework::passwords::delete_generic_password(
                        &self.service,
                        id.as_str(),
                    );
                }
            }
        }
        let mut cleanup = Cleanup {
            service: native.service.clone(),
            id: None,
        };
        let saved = service
            .save(request(
                "native",
                None,
                replace("loom-qa-synthetic-not-a-real-key"),
            ))
            .unwrap();
        let slot = saved.credential_ref.clone().unwrap();
        cleanup.id = Some(slot.clone());
        assert_eq!(
            native.get(&slot).unwrap().unwrap().as_slice(),
            b"loom-qa-synthetic-not-a-real-key"
        );
        assert!(!fs::read_to_string(f.0.join("catalog/events.jsonl"))
            .unwrap()
            .contains("loom-qa-synthetic-not-a-real-key"));
        security_framework::passwords::delete_generic_password(&native.service, slot.as_str())
            .unwrap();
        assert!(native.get(&slot).unwrap().is_none());
        cleanup.id = None;
    }
}
