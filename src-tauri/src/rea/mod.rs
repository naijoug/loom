//! Private, serial MCP client for the fixed REA package. No global MCP setup.
use crate::tool_process::{ProcessOwner, Session, Spec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

pub const VERSION: &str = "6.8.0";
const CATALOG: &str = "4748bd4d8c81aaecf7fceceecfbaf14302cab6cb99cd878c0141b30b9eb577b2";
const PROTOCOL: &str = "2025-06-18";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub node: PathBuf,
    pub entry: PathBuf,
}
impl Config {
    pub fn fingerprint(&self) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let config = self.inspect()?;
        let mut hasher = Sha256::new();
        for path in [&config.node, &config.entry] {
            let meta = std::fs::metadata(path).map_err(|_| "rea_runtime_unavailable")?;
            hasher.update(path.to_string_lossy().as_bytes());
            hasher.update(meta.len().to_le_bytes());
            let modified = meta
                .modified()
                .map_err(|_| "rea_runtime_metadata_unavailable")?
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "rea_runtime_metadata_unavailable")?;
            hasher.update(modified.as_nanos().to_le_bytes());
        }
        hasher.update(std::fs::read(&config.entry).map_err(|_| "rea_entry_unavailable")?);
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub async fn verify_runtime(
        &self,
        cwd: &Path,
        operation_id: &str,
        owner: ProcessOwner,
    ) -> Result<String, String> {
        let config = self.inspect()?;
        let mut environment = BTreeMap::new();
        environment.insert("NODE_OPTIONS".into(), String::new());
        let mut process = Session::spawn(Spec {
            operation_id: operation_id.into(),
            owner,
            program: config.node,
            args: vec!["--version".into()],
            cwd: cwd.into(),
            environment,
            timeout: Duration::from_secs(15),
            max_frame_bytes: 1024,
            max_output_bytes: 4096,
            disk_limit: None,
        })?;
        let result = process.frame(|| false).await;
        let exit = if result.is_ok() {
            process.wait_exit(|| false).await
        } else {
            Ok(None)
        };
        process.close().await?;
        let code = exit?;
        let version = String::from_utf8(result?).map_err(|_| "invalid_node_version")?;
        if code != Some(0) || !supported_node(version.trim()) {
            return Err("unsupported_rea_node_version".into());
        }
        Ok(version.trim().into())
    }
    /// Metadata-only preflight; it never installs a runtime or starts a target.
    pub fn inspect(&self) -> Result<Self, String> {
        let node = std::fs::canonicalize(&self.node).map_err(|_| "rea_node_unavailable")?;
        let entry = std::fs::canonicalize(&self.entry).map_err(|_| "rea_entry_unavailable")?;
        if !node.is_file()
            || !entry.is_file()
            || entry.file_name().and_then(|n| n.to_str()) != Some("rea.mjs")
        {
            return Err("invalid_rea_runtime".into());
        }
        let package = entry
            .parent()
            .and_then(Path::parent)
            .ok_or("invalid_rea_package")?
            .join("package.json");
        let bytes = std::fs::read(package).map_err(|_| "rea_package_unavailable")?;
        if bytes.len() > 1024 * 1024 {
            return Err("invalid_rea_package".into());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid_rea_package")?;
        if value["name"] != "rea-agents" || value["version"] != VERSION {
            return Err("unsupported_rea_version".into());
        }
        Ok(Self { node, entry })
    }
}

fn supported_node(version: &str) -> bool {
    let Some(version) = version.strip_prefix('v') else {
        return false;
    };
    let Ok(numbers) = version
        .split('.')
        .map(|s| s.parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    matches!(numbers.as_slice(), [22, minor, patch] if (*minor,*patch) >= (23,4))
        || matches!(numbers.as_slice(), [24, minor, patch] if (*minor,*patch) >= (22,1))
        || matches!(numbers.as_slice(), [major, minor, patch] if (*major,*minor,*patch) >= (26,12,1))
}

#[derive(Clone, Copy, Debug)]
pub enum Tool {
    InspectApp,
    AnalyzeJavascript,
    EvidenceBundle,
}
impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::InspectApp => "inspect_desktop_application",
            Self::AnalyzeJavascript => "analyze_javascript_application",
            Self::EvidenceBundle => "get_evidence_bundle",
        }
    }
}

pub struct Client {
    process: Session,
    sequence: u64,
}
impl Client {
    pub async fn open(
        config: &Config,
        cwd: &Path,
        operation_id: &str,
        owner: ProcessOwner,
        timeout: Duration,
        disk_limit: u64,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, String> {
        let config = config.inspect()?;
        let mut environment = BTreeMap::new();
        environment.insert(
            "PATH".into(),
            format!(
                "{}:{}",
                config.node.parent().ok_or("invalid_node_path")?.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
        environment.insert(
            "CLANG_MODULE_CACHE_PATH".into(),
            cwd.join("clang-cache").to_string_lossy().into_owned(),
        );
        environment.insert(
            "SWIFT_MODULECACHE_PATH".into(),
            cwd.join("swift-cache").to_string_lossy().into_owned(),
        );
        let temporary = cwd.join("tmp");
        std::fs::create_dir_all(&temporary).map_err(|e| e.to_string())?;
        environment.insert("TMPDIR".into(), temporary.to_string_lossy().into_owned());
        environment.insert("REA_MCP_INPUT_SCHEMA_PROFILE".into(), "full".into());
        // NODE_OPTIONS can preload arbitrary code before REA; do not inherit it.
        environment.insert("NODE_OPTIONS".into(), String::new());
        let process = Session::spawn(Spec {
            operation_id: operation_id.into(),
            owner,
            program: config.node,
            args: vec![config.entry.to_string_lossy().into_owned(), "mcp".into()],
            cwd: cwd.into(),
            environment,
            timeout,
            max_frame_bytes: 64 * 1024 * 1024,
            max_output_bytes: 128 * 1024 * 1024,
            disk_limit: Some(disk_limit),
        })?;
        let mut client = Self {
            process,
            sequence: 0,
        };
        let handshake = tokio::time::timeout(Duration::from_secs(30), async {
            let initialized = client.request("initialize", json!({"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"loom-research","version":"1"}}), cancelled).await?;
            if initialized["protocolVersion"] != PROTOCOL || initialized["serverInfo"]["name"] != "rea" || initialized["serverInfo"]["version"] != VERSION { return Err("rea_identity_mismatch".into()); }
            client.notify("notifications/initialized", json!({}), cancelled).await?;
            let catalog = client.request("tools/list", json!({}), cancelled).await?;
            let list = catalog["tools"].as_array().ok_or("invalid_rea_catalog")?;
            if list.len() != 229 { return Err("rea_catalog_mismatch".into()); }
            let expected: BTreeMap<String, String> = serde_json::from_str(include_str!("../../../tests/fixtures/research/rea-6.8.0-canonical-schemas.json")).map_err(|_| "invalid_pinned_rea_catalog")?;
            for tool in ["binary_session", "inspect_desktop_application", "analyze_javascript_application", "get_evidence_bundle"] {
                let entry = list.iter().find(|item| item["name"] == tool).ok_or("rea_tool_missing")?;
                if expected.get(tool) != Some(&canonical_digest(&entry["inputSchema"])?) || entry["annotations"]["destructiveHint"] != false {
                    return Err("rea_tool_schema_mismatch".into());
                }
            }
            let session = client.request("tools/call", json!({"name":"binary_session","arguments":{"expected_package_version":VERSION,"expected_catalog_digest":CATALOG,"expected_server_path":config.entry}}), cancelled).await?;
            let session = structured(session)?;
            if session["server_identity"]["catalog"]["digests"]["combined_sha256"] != CATALOG { return Err("rea_catalog_mismatch".into()); }
            Ok::<(), String>(())
        }).await.map_err(|_| "rea_startup_timeout".to_string()).and_then(|r| r);
        if let Err(error) = handshake {
            let cleanup = client.close().await;
            return Err(cleanup
                .err()
                .map_or(error.clone(), |cleanup| format!("{error}; {cleanup}")));
        }
        Ok(client)
    }
    async fn notify(
        &mut self,
        method: &str,
        params: Value,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), String> {
        let mut bytes =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","method":method,"params":params}))
                .map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.process.send(&bytes, cancelled).await
    }
    async fn request(
        &mut self,
        method: &str,
        params: Value,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Value, String> {
        self.sequence += 1;
        let mut bytes = serde_json::to_vec(
            &json!({"jsonrpc":"2.0","id":self.sequence,"method":method,"params":params}),
        )
        .map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.process.send(&bytes, cancelled).await?;
        loop {
            let bytes = self.process.frame(cancelled).await?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid_rea_json")?;
            if value["jsonrpc"] != "2.0" {
                return Err("invalid_rea_jsonrpc".into());
            }
            if value.get("id").is_none() {
                if value["method"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("notifications/"))
                {
                    continue;
                }
                return Err("unexpected_rea_message".into());
            }
            // Never answer server-originated tool/elicitation requests.
            if value.get("method").is_some() || value["id"] != self.sequence {
                return Err("rea_response_identity_mismatch".into());
            }
            if value.get("error").is_some() {
                return Err("rea_protocol_error".into());
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| "rea_result_missing".into());
        }
    }
    pub async fn call(
        &mut self,
        tool: Tool,
        args: Value,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Value, String> {
        let result = tokio::time::timeout(
            Duration::from_secs(120),
            self.request(
                "tools/call",
                json!({"name":tool.name(),"arguments":args}),
                cancelled,
            ),
        )
        .await
        .map_err(|_| "rea_call_timeout")??;
        structured(result)
    }
    pub async fn close(&mut self) -> Result<(), String> {
        self.process.close().await.map(|_| ())
    }
    pub fn diagnostics(&self) -> String {
        crate::agent::redact_sensitive_text(&self.process.diagnostics())
    }
}

fn structured(result: Value) -> Result<Value, String> {
    let is_error = result["isError"] == true;
    let value = if let Some(value) = result.get("structuredContent") {
        value.clone()
    } else {
        let text = result["content"]
            .as_array()
            .and_then(|a| a.iter().find(|v| v["type"] == "text"))
            .and_then(|v| v["text"].as_str())
            .ok_or("rea_structured_result_missing")?;
        serde_json::from_str(text).map_err(|_| "invalid_rea_tool_json")?
    };
    if is_error || value.get("error").is_some() {
        let code = value
            .pointer("/error/code")
            .and_then(Value::as_str)
            .filter(|s| {
                s.len() <= 80
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            })
            .unwrap_or("unknown");
        return Err(format!("rea_tool_error:{code}"));
    }
    Ok(value.get("result").cloned().unwrap_or(value))
}

fn canonical_digest(value: &Value) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let entries: BTreeMap<_, _> =
                    map.iter().map(|(k, v)| (k.clone(), sorted(v))).collect();
                Value::Object(entries.into_iter().collect())
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&sorted(value)).map_err(|e| e.to_string())?)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_errors_and_non_json_are_not_evidence() {
        assert!(structured(json!({"isError":true,"structuredContent":{"result":{}}})).is_err());
        assert!(structured(json!({"content":[{"type":"text","text":"not json"}]})).is_err());
        assert!(structured(json!({"structuredContent":{"error":{"code":"unsupported"}}})).is_err());
        assert_eq!(
            structured(json!({"structuredContent":{"result":{"x":1}}})).unwrap(),
            json!({"x":1})
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn protocol_frames_reject_noise_wrong_ids_and_server_requests() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for(output,expected)in [
            ("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{}}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"value\":42}}","ok"),
            ("not json","invalid_rea_json"),
            ("{\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{}}","rea_response_identity_mismatch"),
            ("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"sampling/createMessage\",\"params\":{}}","rea_response_identity_mismatch"),
            ("{\"jsonrpc\":\"1.0\",\"id\":1,\"result\":{}}","invalid_rea_jsonrpc")
        ] {
            let id=format!("rea-protocol-fixture-{}",NEXT.fetch_add(1,Ordering::Relaxed));
            let script=format!("read request\nprintf '%s\\n' '{}'",output.replace('\'',"'\\''"));
            let process=Session::spawn(Spec {operation_id:id.clone(),owner:ProcessOwner::Feature{feature:"fixture".into(),scope_id:"rea".into(),run_id:id},program:"/bin/sh".into(),args:vec!["-c".into(),script],cwd:std::env::temp_dir(),environment:Default::default(),timeout:Duration::from_secs(2),max_frame_bytes:1024,max_output_bytes:4096,disk_limit:None}).unwrap();
            let mut client=Client{process,sequence:0};
            let result=client.request("fixture",json!({}),&||false).await;
            if expected=="ok" {assert_eq!(result.unwrap()["value"],42);}else{assert_eq!(result.unwrap_err(),expected);}
            client.close().await.unwrap();
        }
    }
    #[test]
    fn partial_tool_errors_keep_a_controlled_reason_for_bundle_recovery() {
        let error=structured(json!({"isError":true,"structuredContent":{"error":{"code":"provider_timeout","details":{"partial_observation":{"evidence_id":"ev_partial"}}}}})).unwrap_err();
        assert_eq!(error, "rea_tool_error:provider_timeout");
        let error = structured(
            json!({"isError":true,"structuredContent":{"error":{"code":"secret = private"}}}),
        )
        .unwrap_err();
        assert_eq!(error, "rea_tool_error:unknown");
    }

    #[tokio::test]
    #[ignore = "real fixed REA partial-evidence probe; no model calls"]
    async fn real_provider_timeout_retains_exportable_evidence() {
        if std::env::var("LOOM_REAL_TARGET").as_deref() != Ok("1") {
            return;
        }
        let root = PathBuf::from(format!(
            "{}-partial",
            std::env::var("LOOM_REAL_TARGET_ROOT").expect("explicit root")
        ));
        assert!(!root.exists());
        std::fs::create_dir_all(root.join("input")).unwrap();
        std::fs::write(root.join("input/package.json"), "{}").unwrap();
        let source = (0..10000)
            .map(|i| format!("export function f{i}(x){{return x+{i};}}\n"))
            .collect::<String>();
        std::fs::write(root.join("input/index.js"), source).unwrap();
        let config = Config {
            node: std::env::var("LOOM_REAL_TARGET_NODE").unwrap().into(),
            entry: std::env::var("LOOM_REAL_TARGET_REA").unwrap().into(),
        };
        let mut client = Client::open(
            &config,
            &root,
            "rea-real-partial",
            ProcessOwner::Feature {
                feature: "research".into(),
                scope_id: "partial-fixture".into(),
                run_id: "partial-fixture".into(),
            },
            Duration::from_secs(120),
            32 * 1024 * 1024,
            &|| false,
        )
        .await
        .unwrap();
        let outcome = client
            .call(
                Tool::AnalyzeJavascript,
                json!({"input_path":root.join("input"),"detail":"summary","analysis_timeout_ms":1}),
                &|| false,
            )
            .await;
        let bundle = client
            .call(Tool::EvidenceBundle, json!({"detail":"complete"}), &|| {
                false
            })
            .await
            .unwrap();
        client.close().await.unwrap();
        let records = bundle["records"].as_array().unwrap();
        std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&json!({"error":outcome.as_ref().err(),"recordCount":records.len(),"records":records})).unwrap()).unwrap();
        assert!(
            !records.is_empty(),
            "provider timeout discarded all partial evidence"
        );
        assert!(
            outcome.is_err() || records.iter().any(|r| r.to_string().contains("partial")),
            "not a partial result"
        );
    }

    #[test]
    fn node_support_matches_fixed_package_range() {
        for version in ["v22.23.4", "v24.22.1", "v26.12.1", "v27.0.0"] {
            assert!(supported_node(version));
        }
        for version in [
            "v22.23.3",
            "v24.22.0",
            "v26.11.0",
            "v25.99.0",
            "v26.12.1-beta",
            "26.12.1",
            "v26.bad.12.1",
        ] {
            assert!(!supported_node(version));
        }
    }
}
