//! GitHub API sources materialize complete skill packages at one immutable commit.
//! Only the fixed API origin is contacted; response URLs and redirects are never followed.
use super::{
    catalog,
    error::{ErrorCode, Result, SkillsError},
    files,
    operations::{HttpRequestRecord, Operation, OperationRecord},
    repository::{Repository, Source, SourceKind},
    snapshots,
    sources::GitSource,
};
use base64::Engine;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io::Write,
    path::PathBuf,
    pin::Pin,
    time::Duration,
};

const MAX_TOTAL: u64 = 128 * 1024 * 1024;
const MAX_REQUESTS: usize = 2048;
const API_VERSION: &str = "2022-11-28";

pub fn source_id(source: &GitSource) -> Result<String> {
    if source.provider != "github" {
        return Err(SkillsError::invalid(
            "GitHub API mode only supports github.com repositories",
        ));
    }
    if super::sources::normalize_git_url(&source.clone_url)? != *source {
        return Err(SkillsError::invalid(
            "GitHub API source metadata is not normalized",
        ));
    }
    Ok(format!(
        "src-{}",
        &files::digest(format!("github-api:{}", source.namespace).as_bytes())[..32]
    ))
}
pub fn validate_branch(branch: &str) -> Result<()> {
    if branch.is_empty()
        || branch.len() > 255
        || branch.chars().any(char::is_control)
        || branch.split('/').any(|s| matches!(s, "" | "." | ".."))
    {
        return Err(SkillsError::invalid("Invalid GitHub default branch"));
    }
    Ok(())
}
fn oid(value: &str) -> Result<()> {
    if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(SkillsError::invalid("Invalid GitHub object identity"));
    }
    Ok(())
}
fn unavailable(message: &str) -> SkillsError {
    SkillsError::new(ErrorCode::SourceUnavailable, message)
}

struct Response {
    status: u16,
    rate_limited: bool,
    paginated: bool,
    body: Vec<u8>,
}
trait Transport: Send + Sync {
    fn get<'a>(
        &'a self,
        url: reqwest::Url,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Response>> + Send + 'a>>;
}
struct HttpTransport {
    client: reqwest::Client,
}
fn authorization(token: &str) -> Result<reqwest::header::HeaderValue> {
    let mut header = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| SkillsError::invalid("Invalid GitHub token header"))?;
    header.set_sensitive(true);
    Ok(header)
}
impl HttpTransport {
    fn new(token: Option<&str>) -> Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            reqwest::header::HeaderValue::from_static(API_VERSION),
        );
        if let Some(token) = token {
            headers.insert(reqwest::header::AUTHORIZATION, authorization(token)?);
        }
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent("Loom-Skills/0.1")
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| unavailable("Could not initialize GitHub API client"))?;
        Ok(Self { client })
    }
}
impl Transport for HttpTransport {
    fn get<'a>(
        &'a self,
        url: reqwest::Url,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Response>> + Send + 'a>> {
        Box::pin(async move {
            let mut response = self.client.get(url).send().await.map_err(http_error)?;
            let status = response.status().as_u16();
            let mut rate_limited = response
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|v| v == "0")
                || response
                    .headers()
                    .contains_key(reqwest::header::RETRY_AFTER);
            let paginated = response.headers().contains_key(reqwest::header::LINK);
            let mut body = Vec::new();
            // Error payloads are never echoed. A bounded 403 message may identify
            // a secondary rate limit even when no rate-limit header is present.
            if status == 200 {
                if response.content_length().is_some_and(|n| n > limit as u64) {
                    return Err(unavailable("GitHub API response exceeds the allowed size"));
                }
                while let Some(chunk) = response.chunk().await.map_err(http_error)? {
                    if body.len().saturating_add(chunk.len()) > limit {
                        return Err(unavailable("GitHub API response exceeds the allowed size"));
                    }
                    body.extend_from_slice(&chunk);
                }
            }
            if status == 403 && !rate_limited {
                let mut error_body = Vec::new();
                while let Ok(Some(chunk)) = response.chunk().await {
                    let available = 65536usize.saturating_sub(error_body.len());
                    error_body.extend_from_slice(&chunk[..chunk.len().min(available)]);
                    if error_body.len() == 65536 {
                        break;
                    }
                }
                rate_limited = secondary_rate_limit(&error_body);
            }
            Ok(Response {
                status,
                rate_limited,
                paginated,
                body,
            })
        })
    }
}
fn secondary_rate_limit(bytes: &[u8]) -> bool {
    if bytes.len() > 65536 {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(serde_json::Value::as_str)
                .map(|s| s.to_ascii_lowercase().contains("rate limit"))
        })
        .unwrap_or(false)
}

fn http_error(error: reqwest::Error) -> SkillsError {
    if error.is_timeout() {
        SkillsError::new(ErrorCode::Timeout, "GitHub API request timed out")
    } else {
        unavailable("GitHub API connection or response failed")
    }
}

struct Endpoint<'a> {
    segments: &'a [&'a str],
    recursive: bool,
    limit: usize,
}

pub struct Client {
    transport: Box<dyn Transport>,
}
impl Client {
    pub fn anonymous() -> Result<Self> {
        Ok(Self {
            transport: Box::new(HttpTransport::new(None)?),
        })
    }
    pub fn from_environment() -> Result<Self> {
        let token = ["GITHUB_TOKEN", "GH_TOKEN"].into_iter().find_map(|key| {
            std::env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
        Ok(Self {
            transport: Box::new(HttpTransport::new(token.as_deref().map(str::trim))?),
        })
    }
    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        op: &mut Operation,
        source: &GitSource,
        endpoint: Endpoint<'_>,
        started: tokio::time::Instant,
        emit: &impl Fn(&OperationRecord),
    ) -> Result<T> {
        let Endpoint {
            segments,
            recursive,
            limit,
        } = endpoint;
        if op.record.requests.len() >= MAX_REQUESTS {
            return Err(unavailable(
                "GitHub API request limit reached; use a Git source for this repository",
            ));
        }
        let mut url = reqwest::Url::parse("https://api.github.com/").unwrap();
        {
            let mut path = url.path_segments_mut().unwrap();
            path.push("repos");
            for part in source.namespace.split('/') {
                path.push(part);
            }
            for segment in segments {
                path.push(segment);
            }
        }
        if recursive {
            url.set_query(Some("recursive=1"));
        }
        op.check_cancelled()?;
        if started.elapsed() > Duration::from_secs(300) {
            return Err(SkillsError::new(
                ErrorCode::Timeout,
                "GitHub API import timed out",
            ));
        }
        op.record.requests.push(HttpRequestRecord {
            method: "GET".into(),
            url: url.to_string(),
            started_at_ms: crate::models::now_ms(),
            finished_at_ms: None,
            status: None,
            error: None,
        });
        op.stage("reading-github-api")?;
        emit(&op.record);
        let future = self.transport.get(url, limit);
        tokio::pin!(future);
        let fetched = loop {
            tokio::select! {
                result=&mut future=>break result,
                _=tokio::time::sleep(Duration::from_millis(40))=>{
                    if let Err(error)=op.check_cancelled(){break Err(error);}
                    if started.elapsed()>Duration::from_secs(300){break Err(SkillsError::new(ErrorCode::Timeout,"GitHub API import timed out"));}
                }
            }
        };
        if let Ok(response) = &fetched {
            op.record.requests.last_mut().unwrap().status = Some(response.status);
        }
        let result=fetched.and_then(|response|{
            match response.status {
                200=>(),
                401=>return Err(SkillsError::new(ErrorCode::AuthRequired,"GitHub API authentication failed; check the backend GITHUB_TOKEN or GH_TOKEN")),
                403|429 if response.rate_limited||response.status==429=>return Err(SkillsError::new(ErrorCode::RateLimited,"GitHub API rate limit reached; retry after the limit resets")),
                403=>return Err(SkillsError::new(ErrorCode::AuthRequired,"GitHub API access denied; check repository token permissions")),
                404=>return Err(unavailable("GitHub repository or object was not found; check the URL and token access")),
                300..=399=>return Err(unavailable("GitHub redirected this repository; import its current canonical URL")),
                _=>return Err(unavailable("GitHub API request failed; the previous source version is retained")),
            }
            if response.paginated{return Err(unavailable("Unexpected paginated GitHub object response; incomplete snapshots are not published"));}
            serde_json::from_slice(&response.body).map_err(|_|unavailable("Malformed GitHub API response"))
        });
        let request = op.record.requests.last_mut().unwrap();
        request.finished_at_ms = Some(crate::models::now_ms());
        request.error = result.as_ref().err().map(|e| e.message.clone());
        op.stage("reading-github-api")?;
        emit(&op.record);
        result
    }
}

#[derive(Deserialize)]
struct Metadata {
    default_branch: String,
    full_name: Option<String>,
}
#[derive(Deserialize)]
struct Object {
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}
#[derive(Deserialize)]
struct Reference {
    #[serde(rename = "ref")]
    name: String,
    object: Object,
}
#[derive(Deserialize)]
struct Commit {
    sha: String,
    tree: TreeIdentity,
}
#[derive(Deserialize)]
struct TreeIdentity {
    sha: String,
}
#[derive(Deserialize)]
struct Tree {
    sha: String,
    truncated: bool,
    tree: Vec<Entry>,
}
#[derive(Clone, Deserialize)]
struct Entry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    size: Option<u64>,
}
#[derive(Deserialize)]
struct Blob {
    sha: String,
    size: u64,
    encoding: String,
    content: String,
}

fn safe_relative(path: &str) -> Result<PathBuf> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.eq_ignore_ascii_case(".git"))
    {
        return Err(SkillsError::invalid(
            "GitHub tree contains an unsafe package path",
        ));
    }
    Ok(PathBuf::from(path))
}
fn entries_to_materialize(tree: Tree) -> Result<Vec<Entry>> {
    if tree.truncated {
        return Err(unavailable(
            "GitHub API returned a truncated repository tree; use a Git source instead",
        ));
    }
    if tree.tree.len() > catalog::MAX_ENTRIES {
        return Err(SkillsError::invalid("GitHub tree exceeds 20,000 entries"));
    }
    for entry in &tree.tree {
        if entry.path.rsplit('/').next() == Some("SKILL.md") {
            safe_relative(&entry.path)?;
        }
    }
    let mut seen = BTreeSet::new();
    let roots = tree
        .tree
        .iter()
        .filter(|entry| {
            entry.path.rsplit('/').next() == Some("SKILL.md")
                && !entry
                    .path
                    .split('/')
                    .take(entry.path.split('/').count() - 1)
                    .any(catalog::skip_directory)
        })
        .map(|entry| {
            entry
                .path
                .strip_suffix("SKILL.md")
                .unwrap()
                .trim_end_matches('/')
                .to_string()
        })
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(SkillsError::invalid(
            "GitHub repository contains no discoverable skills",
        ));
    }
    let mut total = 0u64;
    let mut selected = Vec::new();
    for entry in tree.tree {
        if !roots.iter().any(|root| {
            root.is_empty() || entry.path == *root || entry.path.starts_with(&format!("{root}/"))
        }) {
            continue;
        }
        if entry
            .path
            .split('/')
            .any(|p| matches!(p, "node_modules" | "__pycache__" | ".DS_Store"))
        {
            continue;
        }
        safe_relative(&entry.path)?;
        oid(&entry.sha)?;
        if !seen.insert(entry.path.to_lowercase()) {
            return Err(SkillsError::invalid(
                "GitHub packages have duplicate or case-colliding paths",
            ));
        }
        if entry.kind == "tree" && entry.mode == "040000" {
            continue;
        }
        if entry.kind != "blob" || !matches!(entry.mode.as_str(), "100644" | "100755") {
            return Err(SkillsError::invalid(
                "GitHub API packages cannot contain symlinks, submodules or special entries",
            ));
        }
        let size = entry
            .size
            .ok_or_else(|| unavailable("GitHub blob size is missing"))?;
        total = total.saturating_add(size);
        if total > MAX_TOTAL {
            return Err(SkillsError::invalid("GitHub skill packages exceed 128 MiB"));
        }
        selected.push(entry);
    }
    selected.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(selected)
}
fn decode_blob(blob: Blob, entry: &Entry) -> Result<Vec<u8>> {
    if blob.sha != entry.sha
        || blob.size != entry.size.unwrap_or(u64::MAX)
        || blob.encoding != "base64"
    {
        return Err(unavailable(
            "GitHub blob metadata does not match the pinned tree",
        ));
    }
    let encoded = blob
        .content
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| unavailable("GitHub blob has invalid base64 encoding"))?;
    let mut digest = Sha1::new();
    digest.update(format!("blob {}\0", bytes.len()).as_bytes());
    digest.update(&bytes);
    if bytes.len() as u64 != blob.size || format!("{:x}", digest.finalize()) != entry.sha {
        return Err(unavailable(
            "GitHub blob contents do not match their object identity",
        ));
    }
    Ok(bytes)
}

async fn materialize(
    repo: &Repository,
    op: &mut Operation,
    source: &GitSource,
    expected_revision: Option<&str>,
    client: &Client,
    emit: &impl Fn(&OperationRecord),
) -> Result<()> {
    let id = source_id(source)?;
    let started = tokio::time::Instant::now();
    let directory = repo.prepare_directory(&op.record.operation_id)?;
    let result = async {
        let metadata: Metadata = client
            .json(
                op,
                source,
                Endpoint {
                    segments: &[],
                    recursive: false,
                    limit: 1024 * 1024,
                },
                started,
                emit,
            )
            .await?;
        validate_branch(&metadata.default_branch)?;
        if metadata
            .full_name
            .as_ref()
            .is_some_and(|name| !name.eq_ignore_ascii_case(&source.namespace))
        {
            return Err(unavailable(
                "GitHub metadata refers to a different repository",
            ));
        }
        let reference: Reference = client
            .json(
                op,
                source,
                Endpoint {
                    segments: &["git", "ref", "heads", &metadata.default_branch],
                    recursive: false,
                    limit: 1024 * 1024,
                },
                started,
                emit,
            )
            .await?;
        if reference.name != format!("refs/heads/{}", metadata.default_branch)
            || reference.object.kind != "commit"
        {
            return Err(unavailable(
                "GitHub default branch did not resolve to a commit",
            ));
        }
        oid(&reference.object.sha)?;
        let commit: Commit = client
            .json(
                op,
                source,
                Endpoint {
                    segments: &["git", "commits", &reference.object.sha],
                    recursive: false,
                    limit: 8 * 1024 * 1024,
                },
                started,
                emit,
            )
            .await?;
        if commit.sha != reference.object.sha {
            return Err(unavailable("GitHub commit identity changed"));
        }
        oid(&commit.tree.sha)?;
        let tree: Tree = client
            .json(
                op,
                source,
                Endpoint {
                    segments: &["git", "trees", &commit.tree.sha],
                    recursive: true,
                    limit: 8 * 1024 * 1024,
                },
                started,
                emit,
            )
            .await?;
        if tree.sha != commit.tree.sha {
            return Err(unavailable("GitHub tree identity changed"));
        }
        let entries = entries_to_materialize(tree)?;
        let checkout = files::private_directory(&directory.join("materialized"))?;
        let mut blobs = BTreeMap::<String, Vec<u8>>::new();
        for entry in entries {
            op.check_cancelled()?;
            if !blobs.contains_key(&entry.sha) {
                let size = entry.size.unwrap();
                let limit = usize::try_from(size.saturating_mul(2).saturating_add(65536))
                    .map_err(|_| SkillsError::invalid("Blob exceeds response limit"))?;
                let blob: Blob = client
                    .json(
                        op,
                        source,
                        Endpoint {
                            segments: &["git", "blobs", &entry.sha],
                            recursive: false,
                            limit,
                        },
                        started,
                        emit,
                    )
                    .await?;
                blobs.insert(entry.sha.clone(), decode_blob(blob, &entry)?);
            }
            let contents = &blobs[&entry.sha];
            if contents.len() as u64 != entry.size.unwrap() {
                return Err(unavailable("Duplicate Git blob has inconsistent size"));
            }
            let path = checkout.join(safe_relative(&entry.path)?);
            files::private_directory(path.parent().unwrap())?;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(if entry.mode == "100755" { 0o700 } else { 0o600 });
            }
            let mut file = options.open(&path)?;
            file.write_all(contents)?;
            file.sync_all()?;
            #[cfg(not(unix))]
            super::package::set_mode(&path, if entry.mode == "100755" { 0o700 } else { 0o600 })?;
        }
        op.stage("validating")?;
        emit(&op.record);
        let staging = directory.join("snapshot");
        let validation_root = checkout.clone();
        let validation_staging = staging.clone();
        let validation_id = id.clone();
        let revision = commit.sha.clone();
        let snapshot = tokio::task::spawn_blocking(move || {
            let catalog = catalog::scan_repository(&validation_id, &validation_root)?;
            if catalog.skills.is_empty() || !catalog.issues.is_empty() {
                return Err(SkillsError::invalid(
                    "GitHub API snapshot contains invalid or unreadable skill packages",
                ));
            }
            snapshots::prepare(
                &validation_root,
                &validation_staging,
                &validation_id,
                &revision,
            )
        })
        .await
        .map_err(|_| SkillsError::recovery("GitHub API snapshot worker interrupted"))??;
        op.check_cancelled()?;
        op.stage("publishing")?;
        emit(&op.record);
        repo.commit_git(
            Source {
                id,
                name: source.namespace.clone(),
                kind: SourceKind::GithubApi,
                local_path: None,
                git: Some(source.clone()),
                revision: Some(commit.sha),
                updated_at_ms: crate::models::now_ms(),
                default_branch: Some(metadata.default_branch),
            },
            expected_revision,
            &staging,
            &snapshot,
        )
    }
    .await;
    let _ = std::fs::remove_dir_all(&directory);
    result
}

pub async fn import(
    repo: Repository,
    mut op: Operation,
    source: GitSource,
    expected_revision: Option<String>,
    client: Client,
    emit: impl Fn(&OperationRecord),
) {
    let result = materialize(
        &repo,
        &mut op,
        &source,
        expected_revision.as_deref(),
        &client,
        &emit,
    )
    .await;
    if let Err(error) = op.finish(result) {
        op.record.phase = super::operations::Phase::Failed;
        op.record.error = Some(error);
    }
    emit(&op.record);
}

#[cfg(test)]
#[path = "github_api_tests.rs"]
mod tests;
