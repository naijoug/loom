//! Source identity is independent of transport and installation targets.
use super::{
    error::{Result, SkillsError},
    files,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSource {
    pub source_id: String,
    pub provider: String,
    pub namespace: String,
    pub url: String,
    pub clone_url: String,
    pub legacy_id: String,
}

pub fn normalize_git_url(input: &str) -> Result<GitSource> {
    let input = input.trim();
    if input.len() > 2048
        || input
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err(SkillsError::invalid("Invalid repository URL"));
    }
    let (host, raw_path, transport) = if let Some(rest) = input.strip_prefix("git@") {
        let (host, path) = rest
            .split_once(':')
            .ok_or_else(|| SkillsError::invalid("Invalid SSH repository URL"))?;
        (host, path, "scp")
    } else if let Some(rest) = input.strip_prefix("ssh://git@") {
        let (host, path) = rest
            .split_once('/')
            .ok_or_else(|| SkillsError::invalid("Invalid SSH repository URL"))?;
        (host, path, "ssh")
    } else if let Some(rest) = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
    {
        let (host, path) = rest
            .split_once('/')
            .ok_or_else(|| SkillsError::invalid("Repository URL needs a namespace"))?;
        (host, path, "https")
    } else {
        return Err(SkillsError::invalid(
            "Use a GitHub or GitLab HTTPS/SSH repository URL",
        ));
    };
    let host = host.to_ascii_lowercase();
    let provider =
        match host.as_str() {
            "github.com" => "github",
            "gitlab.com" => "gitlab",
            _ => return Err(SkillsError::invalid(
                "Only github.com and gitlab.com are supported; URL credentials are not accepted",
            )),
        };
    let path = raw_path.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = path.split('/').collect();
    let end = if transport == "https" {
        if provider == "github" {
            if parts.len() > 2 && !matches!(parts.get(2), Some(&"tree") | Some(&"blob")) {
                return Err(SkillsError::invalid("Unsupported GitHub URL path"));
            }
            2
        } else {
            parts.iter().position(|p| *p == "-").unwrap_or(parts.len())
        }
    } else {
        parts.len()
    };
    if end < 2 || end > parts.len() || (provider == "github" && end != 2) {
        return Err(SkillsError::invalid(
            "Repository URL needs an owner/namespace and repository",
        ));
    }
    let repo_parts = &parts[..end];
    if repo_parts.iter().any(|part| {
        part.is_empty()
            || *part == "."
            || *part == ".."
            || part.starts_with('-')
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    }) {
        return Err(SkillsError::invalid("Invalid repository namespace"));
    }
    let namespace = repo_parts.join("/");
    let url = format!("https://{host}/{namespace}");
    let clone_url = match transport {
        "scp" => format!("git@{host}:{namespace}.git"),
        "ssh" => format!("ssh://git@{host}/{namespace}.git"),
        _ => format!("{url}.git"),
    };
    let identity = format!("{provider}:{namespace}");
    // Retain source path case to avoid silently merging distinct namespaces.
    // legacy_id is a case-insensitive identity key; sources that differ only
    // by case are rejected as conflicts instead of being merged.
    Ok(GitSource {
        source_id: format!("src-{}", &files::digest(identity.as_bytes())[..32]),
        provider: provider.into(),
        namespace,
        url,
        clone_url,
        legacy_id: identity.to_ascii_lowercase(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_changes_keep_identity_and_nested_gitlab_namespace() {
        for (host, path) in [
            ("github.com", "owner/repo"),
            ("gitlab.com", "group/subgroup/repo"),
        ] {
            let https = normalize_git_url(&format!("https://{host}/{path}.git/")).unwrap();
            let scp = normalize_git_url(&format!("git@{host}:{path}.git")).unwrap();
            let ssh = normalize_git_url(&format!("ssh://git@{host}/{path}.git")).unwrap();
            assert_eq!(https.source_id, scp.source_id);
            assert_eq!(https.source_id, ssh.source_id);
            assert_eq!(ssh.namespace, path);
            assert_eq!(scp.clone_url, format!("git@{host}:{path}.git"));
            assert_eq!(
                normalize_git_url(&format!("http://{host}/{path}"))
                    .unwrap()
                    .url,
                https.url
            );
        }
        let page = normalize_git_url("https://gitlab.com/group/subgroup/repo/-/tree/main").unwrap();
        assert_eq!(page.namespace, "group/subgroup/repo");
        assert_eq!(page.legacy_id, "gitlab:group/subgroup/repo");
        assert_eq!(
            normalize_git_url("https://github.com/owner/repo/tree/main/skills")
                .unwrap()
                .namespace,
            "owner/repo"
        );
    }

    #[test]
    fn credentials_traversal_unknown_hosts_and_shell_fragments_are_rejected() {
        for input in [
            "https://token@github.com/a/b",
            "https://example.com/a/b",
            "file:///tmp/repo",
            "https://github.com/a/../x",
            "https://gitlab.com/g//r",
            "https://github.com/a/b;touch",
            "git@github.com:a/b\n--upload-pack=evil",
            "https://github.com/a/b?token=secret",
            "ssh://git@github.com/a/b/c",
        ] {
            assert!(normalize_git_url(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn old_case_alias_does_not_silently_merge_source_identity() {
        let first = normalize_git_url("https://gitlab.com/Group/repo").unwrap();
        let second = normalize_git_url("https://gitlab.com/group/repo").unwrap();
        assert_eq!(first.legacy_id, second.legacy_id);
        assert_ne!(first.source_id, second.source_id);
    }
}
