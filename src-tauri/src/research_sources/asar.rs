//! Bounded ASAR source reads. No bulk extraction, link following or target execution.
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

// Duplicate names make the archive manifest ambiguous; reject them at decode.
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = Value;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
                Ok(v.into())
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
                Ok(v.into())
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
                serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Value, E> {
                Ok(Value::String(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Value, E> {
                Ok(Value::String(v))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = seq.next_element::<UniqueJson>()? {
                    values.push(v.0);
                }
                Ok(Value::Array(values))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate archive key"));
                    }
                    values.insert(key, map.next_value::<UniqueJson>()?.0);
                }
                Ok(Value::Object(values))
            }
        }
        d.deserialize_any(JsonVisitor).map(Self)
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    offset: Option<u64>,
    hash: Option<String>,
}
pub struct Archive {
    path: PathBuf,
    header_end: u64,
    pub sha256: String,
    pub entries: Vec<Entry>,
    pub omissions: Vec<String>,
}
impl Archive {
    pub fn inspect(path: &Path) -> Result<Self, String> {
        let meta = fs::symlink_metadata(path).map_err(|_| "asar_unavailable")?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 512 * 1024 * 1024 {
            return Err("unsupported_or_oversized_asar".into());
        }
        let mut file = File::open(path).map_err(|_| "asar_unavailable")?;
        let mut prefix = [0u8; 16];
        file.read_exact(&mut prefix)
            .map_err(|_| "invalid_asar_header")?;
        let number = |at| u32::from_le_bytes(prefix[at..at + 4].try_into().unwrap()) as u64;
        let (header, payload, json_bytes) = (number(4), number(8), number(12));
        if number(0) != 4
            || !(8..=16 * 1024 * 1024).contains(&header)
            || payload != header - 4
            || json_bytes > payload - 4
            || 8 + header > meta.len()
        {
            return Err("invalid_asar_header".into());
        }
        let mut bytes = vec![0; json_bytes as usize];
        file.read_exact(&mut bytes)
            .map_err(|_| "invalid_asar_header")?;
        let value = serde_json::from_slice::<UniqueJson>(&bytes)
            .map_err(|_| "invalid_asar_json")?
            .0;
        let mut entries = Vec::new();
        let mut omissions = Vec::new();
        walk(
            &value,
            "",
            &(8 + header..meta.len()),
            0,
            &mut 0,
            &mut entries,
            &mut omissions,
        )?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buf = [0; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > 512 * 1024 * 1024 {
                return Err("asar_size_limit".into());
            }
            hash.update(&buf[..n]);
        }
        if total != meta.len()
            || file.metadata().map_err(|e| e.to_string())?.modified().ok() != meta.modified().ok()
        {
            return Err("asar_changed".into());
        }
        Ok(Self {
            path: path.into(),
            header_end: 8 + header,
            sha256: format!("{:x}", hash.finalize()),
            entries,
            omissions,
        })
    }
    pub fn companion_identity(&self) -> Result<Value, String> {
        let root = PathBuf::from(format!("{}.unpacked", self.path.display()));
        let mut files = Vec::new();
        for entry in self.entries.iter().filter(|e| e.offset.is_none()) {
            let path = super::scoped_path(&root, &entry.path)?;
            let meta = fs::symlink_metadata(&path).map_err(|_| "asar_companion_missing")?;
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err("unsafe_asar_companion".into());
            }
            files.push(serde_json::json!({"path":entry.path,"bytes":meta.len(),"modified":format!("{:?}",meta.modified().map_err(|e|e.to_string())?)}));
        }
        Ok(Value::Array(files))
    }
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, String> {
        if entry.size > 16 * 1024 * 1024 {
            return Err("asar_source_size_limit".into());
        }
        let bytes = if let Some(offset) = entry.offset {
            let mut file = File::open(&self.path).map_err(|_| "asar_unavailable")?;
            file.seek(SeekFrom::Start(
                self.header_end
                    .checked_add(offset)
                    .ok_or("asar_range_invalid")?,
            ))
            .map_err(|e| e.to_string())?;
            let mut bytes = vec![0; entry.size as usize];
            file.read_exact(&mut bytes)
                .map_err(|_| "asar_source_changed")?;
            bytes
        } else {
            let root = PathBuf::from(format!("{}.unpacked", self.path.display()));
            super::read_scoped(&root, &entry.path)?
        };
        if bytes.len() as u64 != entry.size {
            return Err("asar_source_changed".into());
        }
        if entry
            .hash
            .as_ref()
            .is_some_and(|hash| *hash != format!("{:x}", Sha256::digest(&bytes)))
        {
            return Err("asar_integrity_mismatch".into());
        }
        Ok(bytes)
    }
}
fn walk(
    node: &Value,
    prefix: &str,
    bounds: &std::ops::Range<u64>,
    depth: usize,
    count: &mut usize,
    entries: &mut Vec<Entry>,
    omissions: &mut Vec<String>,
) -> Result<(), String> {
    if depth > 64 {
        return Err("asar_depth_limit".into());
    }
    let files = node["files"].as_object().ok_or("invalid_asar_tree")?;
    for (name, node) in files {
        *count += 1;
        if *count > 20_000 {
            return Err("asar_inventory_limit".into());
        }
        if name.is_empty()
            || matches!(name.as_str(), "." | "..")
            || name.contains(['/', '\\', '\0'])
        {
            return Err("unsafe_asar_path".into());
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if name.starts_with('.') || node.get("link").is_some() {
            omissions.push(format!("{path}: hidden entry or link omitted"));
            continue;
        }
        if node.get("files").is_some() {
            walk(node, &path, bounds, depth + 1, count, entries, omissions)?;
            continue;
        }
        let size = node["size"].as_u64().ok_or("invalid_asar_size")?;
        let offset = if node["unpacked"] == true {
            None
        } else {
            let offset = node["offset"]
                .as_str()
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .ok_or("invalid_asar_offset")?
                .parse::<u64>()
                .map_err(|_| "invalid_asar_offset")?;
            let end = bounds
                .start
                .checked_add(offset)
                .and_then(|start| start.checked_add(size))
                .ok_or("asar_range_invalid")?;
            if end > bounds.end {
                return Err("asar_range_invalid".into());
            }
            Some(offset)
        };
        if !super::is_source_path(Path::new(&path)) || size > 16 * 1024 * 1024 {
            omissions.push(format!("{path}: non-source or oversized member omitted"));
            continue;
        }
        let hash = if let Some(integrity) = node.get("integrity") {
            let value = integrity["hash"].as_str().ok_or("invalid_asar_integrity")?;
            if integrity["algorithm"] != "SHA256"
                || value.len() != 64
                || !value.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("unsupported_asar_integrity".into());
            }
            Some(value.to_ascii_lowercase())
        } else {
            None
        };
        entries.push(Entry {
            path,
            size,
            offset,
            hash,
        });
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn write_archive(path: &Path, files: &[(&str, &[u8])]) {
        let mut entries = serde_json::Map::new();
        let mut body = Vec::new();
        for (name, bytes) in files {
            entries.insert((*name).into(),serde_json::json!({"size":bytes.len(),"offset":body.len().to_string(),"integrity":{"algorithm":"SHA256","hash":format!("{:x}",Sha256::digest(bytes))}}));
            body.extend_from_slice(bytes);
        }
        let json = serde_json::to_vec(&serde_json::json!({"files":entries})).unwrap();
        let padded = json.len().div_ceil(4) * 4;
        let header = padded + 8;
        let mut bytes = Vec::new();
        for n in [4, header as u32, (header - 4) as u32, json.len() as u32] {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        bytes.extend(json);
        bytes.resize(8 + header, 0);
        bytes.extend(body);
        fs::write(path, bytes).unwrap();
    }
    #[test]
    fn duplicate_header_keys_and_out_of_range_members_are_rejected() {
        assert!(serde_json::from_str::<UniqueJson>(r#"{"files":{},"files":{}}"#).is_err());
        let value = serde_json::json!({"files":{"index.js":{"size":16,"offset":"999999"}}});
        assert!(walk(
            &value,
            "",
            &(32..64),
            0,
            &mut 0,
            &mut Vec::new(),
            &mut Vec::new()
        )
        .is_err());
        let value =
            serde_json::json!({"files":{"folder":{"files":{"..":{"size":1,"offset":"0"}}}}});
        assert!(walk(
            &value,
            "",
            &(32..64),
            0,
            &mut 0,
            &mut Vec::new(),
            &mut Vec::new()
        )
        .is_err());
    }

    #[test]
    fn source_members_are_bounded_and_integrity_checked() {
        let root = std::env::temp_dir().join(format!(
            "loom-asar-{}-{}",
            std::process::id(),
            crate::models::now_ms()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("app.asar");
        write_archive(
            &path,
            &[
                ("index.js", b"export const value=42;"),
                ("asset.bin", b"binary"),
            ],
        );
        let archive = Archive::inspect(&path).unwrap();
        assert_eq!(archive.entries.len(), 1);
        assert_eq!(archive.omissions.len(), 1);
        assert_eq!(
            archive.read(&archive.entries[0]).unwrap(),
            b"export const value=42;"
        );
        let mut data = fs::read(&path).unwrap();
        let at = archive.header_end as usize;
        data[at] = b'X';
        fs::write(&path, data).unwrap();
        assert!(archive.read(&archive.entries[0]).is_err());
        write_archive(&path, &[("../private.js", b"outside")]);
        assert!(Archive::inspect(&path).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
