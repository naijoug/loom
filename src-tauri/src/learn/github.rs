//! Read-only GitHub repository search for the Learn start page: "rising"
//! repositories (newly created ones sorted by stars) and lookups for repositories
//! an agent suggested. Only `api.github.com` is contacted: the repository search and,
//! for a suggested name the search misses, `/repos/<owner>/<name>` plus at most one
//! rename hop to `/repositories/<id>`. Redirects are never followed automatically and
//! error bodies are never echoed.
use super::models::GithubRepoCard;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const SEARCH_URL: &str = "https://api.github.com/search/repositories";
const REPOS_URL: &str = "https://api.github.com/repos/";
const RENAMED_PREFIX: &str = "https://api.github.com/repositories/";
const API_VERSION: &str = "2022-11-28";
const MAX_RESPONSE: usize = 4 * 1024 * 1024;
const TRENDING_TTL: Duration = Duration::from_secs(10 * 60);
pub const PERIODS: &[&str] = &["day", "week", "month"];
const MAX_QUERY_CHARS: usize = 200;
const MAX_LANGUAGE_CHARS: usize = 40;

/// `owner/repo` with GitHub's allowed characters.
pub fn valid_full_name(name: &str) -> bool {
    let Some((owner, repo)) = name.split_once('/') else {
        return false;
    };
    let owner_ok = !owner.is_empty()
        && owner.len() <= 39
        && !owner.starts_with('-')
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let repo_ok = !repo.is_empty()
        && repo.len() <= 100
        && repo != "."
        && repo != ".."
        && repo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    owner_ok && repo_ok
}

/// Language filter as typed in GitHub's `language:` qualifier (`C++`, `Jupyter Notebook`, …).
pub fn valid_language(language: &str) -> bool {
    !language.is_empty()
        && language.chars().count() <= MAX_LANGUAGE_CHARS
        && language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '+' | '#' | '-' | '.' | '_'))
}

/// Free-text search query from an agent: control characters removed, length bounded.
pub fn clean_query(query: &str) -> Option<String> {
    let cleaned: String = query
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned: String = cleaned.chars().take(MAX_QUERY_CHARS).collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// UTC calendar date `days` days before `now_ms` (Howard Hinnant's civil-from-days).
pub fn date_days_ago(now_ms: u128, days: u64) -> String {
    let z = ((now_ms / 86_400_000) as u64).saturating_sub(days) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn trending_query(
    period: &str,
    language: Option<&str>,
    now_ms: u128,
) -> Result<String, String> {
    let days = match period {
        "day" => 1,
        "week" => 7,
        "month" => 30,
        _ => return Err("Unsupported period".into()),
    };
    let mut query = format!("created:>={}", date_days_ago(now_ms, days));
    if let Some(language) = language {
        if !valid_language(language) {
            return Err("Unsupported language filter".into());
        }
        if language.contains(' ') {
            query.push_str(&format!(" language:\"{language}\""));
        } else {
            query.push_str(&format!(" language:{language}"));
        }
    }
    Ok(query)
}

/// One search for several named repositories: `repo:` qualifiers are OR-ed.
pub fn repos_query(names: &[String]) -> Option<String> {
    let names: Vec<String> = names
        .iter()
        .filter(|n| valid_full_name(n))
        .map(|n| format!("repo:{n}"))
        .collect();
    (!names.is_empty()).then(|| names.join(" "))
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    full_name: String,
    description: Option<String>,
    language: Option<String>,
    #[serde(default)]
    stargazers_count: u64,
    #[serde(default)]
    forks_count: u64,
    #[serde(default)]
    topics: Vec<String>,
    pushed_at: Option<String>,
    created_at: Option<String>,
    #[serde(default)]
    fork: bool,
    #[serde(default)]
    archived: bool,
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

fn card(item: Item) -> Option<GithubRepoCard> {
    if !valid_full_name(&item.full_name) {
        return None;
    }
    let (owner, name) = item.full_name.split_once('/').unwrap();
    Some(GithubRepoCard {
        url: format!("https://github.com/{}", item.full_name),
        owner: owner.into(),
        name: name.into(),
        description: item
            .description
            .filter(|d| !d.trim().is_empty())
            .map(|d| clip(&d, 300)),
        language: item.language.map(|l| clip(&l, 40)),
        stars: item.stargazers_count,
        forks: item.forks_count,
        topics: item
            .topics
            .into_iter()
            .take(6)
            .map(|t| clip(&t, 40))
            .collect(),
        pushed_at: item.pushed_at,
        created_at: item.created_at,
        fork: item.fork,
        archived: item.archived,
        full_name: item.full_name,
    })
}

/// Parses a search response into cards; entries with an unexpected name are dropped.
pub fn parse_search(body: &[u8]) -> Result<Vec<GithubRepoCard>, String> {
    let response: SearchResponse =
        serde_json::from_slice(body).map_err(|_| "Malformed GitHub search response".to_string())?;
    Ok(response.items.into_iter().filter_map(card).collect())
}

/// Where a renamed repository now lives: GitHub answers `/repos/<old>` with 301 and
/// `{"url": "https://api.github.com/repositories/<id>"}`. Anything else is refused.
pub fn renamed_target(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Moved {
        url: String,
    }
    let moved: Moved = serde_json::from_slice(body).ok()?;
    let id = moved.url.strip_prefix(RENAMED_PREFIX)?;
    (!id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit()))
        .then_some(moved.url)
}

pub const QUERY_REJECTED: &str = "GitHub could not run this search query";

fn status_error(status: u16, rate_limited: bool) -> String {
    match status {
        401 => "GitHub rejected the token in GITHUB_TOKEN / GH_TOKEN".into(),
        403 | 429 if rate_limited || status == 429 => {
            "GitHub search rate limit reached; try again in a minute (set GITHUB_TOKEN for a higher limit)".into()
        }
        403 => "GitHub refused the search request".into(),
        422 => QUERY_REJECTED.into(),
        300..=399 => "GitHub redirected the search request".into(),
        503 => "GitHub search is temporarily unavailable".into(),
        _ => format!("GitHub search failed (HTTP {status})"),
    }
}

fn token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"].into_iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn client() -> Result<reqwest::Client, String> {
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
    if let Some(token) = token() {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| "Invalid GITHUB_TOKEN / GH_TOKEN value".to_string())?;
        value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    reqwest::Client::builder()
        .default_headers(headers)
        .user_agent("Loom-Learn/0.1")
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "Could not initialize the GitHub client".to_string())
}

/// GET with the shared client; returns the status and a body capped at `MAX_RESPONSE`.
async fn fetch(url: reqwest::Url) -> Result<(u16, bool, Vec<u8>), String> {
    let mut response = client()?.get(url).send().await.map_err(|e| {
        if e.is_timeout() {
            "GitHub search timed out".to_string()
        } else {
            "Could not reach GitHub; check the network connection".to_string()
        }
    })?;
    let status = response.status().as_u16();
    let rate_limited = response
        .headers()
        .get("x-ratelimit-remaining")
        .is_some_and(|v| v == "0")
        || response
            .headers()
            .contains_key(reqwest::header::RETRY_AFTER);
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE as u64)
    {
        return Err("GitHub search response is too large".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "GitHub search response was interrupted".to_string())?
    {
        if body.len() + chunk.len() > MAX_RESPONSE {
            return Err("GitHub search response is too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, rate_limited, body))
}

/// Runs one repository search: by stars for rising lists, by GitHub's best match
/// for free-text queries (sorting those by stars surfaces popular off-topic repositories).
pub async fn search(
    query: &str,
    per_page: u8,
    by_stars: bool,
) -> Result<Vec<GithubRepoCard>, String> {
    let mut url = reqwest::Url::parse(SEARCH_URL).unwrap();
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", query);
        if by_stars {
            pairs
                .append_pair("sort", "stars")
                .append_pair("order", "desc");
        }
        pairs.append_pair("per_page", &per_page.clamp(1, 50).to_string());
    }
    let (status, rate_limited, body) = fetch(url).await?;
    if status != 200 {
        return Err(status_error(status, rate_limited));
    }
    parse_search(&body)
}

/// One named repository, following a rename once. `Ok(None)` when it does not exist
/// (or is private); errors only for network / rate-limit failures.
pub async fn repository(full_name: &str) -> Result<Option<GithubRepoCard>, String> {
    if !valid_full_name(full_name) {
        return Ok(None);
    }
    let mut url = reqwest::Url::parse(&format!("{REPOS_URL}{full_name}"))
        .map_err(|_| "Invalid repository name".to_string())?;
    for hop in 0..2 {
        let (status, rate_limited, body) = fetch(url).await?;
        match status {
            200 => {
                return Ok(serde_json::from_slice::<Item>(&body).ok().and_then(card));
            }
            301 if hop == 0 => {
                match renamed_target(&body).and_then(|t| reqwest::Url::parse(&t).ok()) {
                    Some(next) => url = next,
                    None => return Ok(None),
                }
            }
            403 | 429 if rate_limited || status == 429 => {
                return Err(status_error(status, rate_limited));
            }
            _ => return Ok(None),
        }
    }
    Ok(None)
}

type Cache = Mutex<HashMap<String, (Instant, u128, Vec<GithubRepoCard>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Rising repositories for one period, cached per query for ten minutes.
/// Returns `(query, fetched_at_ms, cached, repos)`.
pub async fn trending(
    period: &str,
    language: Option<&str>,
    refresh: bool,
) -> Result<(String, u128, bool, Vec<GithubRepoCard>), String> {
    let now = crate::models::now_ms();
    let query = trending_query(period, language, now)?;
    if !refresh {
        if let Some((at, fetched, repos)) = cache().lock().ok().and_then(|c| c.get(&query).cloned())
        {
            if at.elapsed() < TRENDING_TTL {
                return Ok((query, fetched, true, repos));
            }
        }
    }
    let repos = search(&query, 20, true).await?;
    if let Ok(mut cache) = cache().lock() {
        cache.retain(|_, (at, _, _)| at.elapsed() < TRENDING_TTL);
        cache.insert(query.clone(), (Instant::now(), now, repos.clone()));
    }
    Ok((query, now, false, repos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_count_back_in_utc() {
        // 2026-11-03T12:00:00Z
        let now = 1_793_707_200_000;
        assert_eq!(date_days_ago(now, 0), "2026-11-03");
        assert_eq!(date_days_ago(now, 7), "2026-10-27");
        assert_eq!(date_days_ago(now, 30), "2026-10-04");
        assert_eq!(date_days_ago(0, 0), "1970-01-01");
        assert_eq!(date_days_ago(951_825_600_000, 0), "2000-02-29");
    }

    #[test]
    fn trending_query_checks_period_and_language() {
        let now = 1_793_707_200_000;
        assert_eq!(
            trending_query("week", None, now).unwrap(),
            "created:>=2026-10-27"
        );
        assert_eq!(
            trending_query("day", Some("Rust"), now).unwrap(),
            "created:>=2026-11-02 language:Rust"
        );
        assert_eq!(
            trending_query("month", Some("Jupyter Notebook"), now).unwrap(),
            "created:>=2026-10-04 language:\"Jupyter Notebook\""
        );
        assert!(trending_query("year", None, now).is_err());
        assert!(trending_query("day", Some("rust stars:>1"), now).is_err());
        assert!(trending_query("day", Some("\"x"), now).is_err());
    }

    #[test]
    fn names_and_queries_are_validated() {
        assert!(valid_full_name("SWE-agent/mini-swe-agent"));
        assert!(valid_full_name("rust-lang/rust.vim"));
        assert!(!valid_full_name("owner"));
        assert!(!valid_full_name("-x/y"));
        assert!(!valid_full_name("a/.."));
        assert!(!valid_full_name("a/b/c"));
        assert!(!valid_full_name("a/b c"));
        assert_eq!(
            repos_query(&["a/b".into(), "bad name".into(), "c/d".into()]).as_deref(),
            Some("repo:a/b repo:c/d")
        );
        assert_eq!(repos_query(&["x".into()]), None);
        assert_eq!(
            clean_query("  agent\n  framework\u{7}  ").as_deref(),
            Some("agent framework")
        );
        assert_eq!(clean_query("\n\t"), None);
        assert_eq!(clean_query(&"a".repeat(500)).unwrap().len(), 200);
    }

    #[test]
    fn search_response_becomes_cards() {
        let body = br#"{"total_count":2,"items":[
            {"full_name":"o/r","html_url":"https://evil.example/o/r","description":"  A tool  ","language":"Rust",
             "stargazers_count":12,"forks_count":3,"topics":["cli","agents"],"pushed_at":"2026-11-01T00:00:00Z",
             "created_at":"2026-10-30T00:00:00Z","fork":false,"archived":false},
            {"full_name":"bad name/x","stargazers_count":1}
        ]}"#;
        let cards = parse_search(body).unwrap();
        assert_eq!(cards.len(), 1);
        let card = &cards[0];
        assert_eq!(card.url, "https://github.com/o/r");
        assert_eq!(card.owner, "o");
        assert_eq!(card.name, "r");
        assert_eq!(card.description.as_deref(), Some("A tool"));
        assert_eq!(card.stars, 12);
        assert_eq!(card.topics, vec!["cli", "agents"]);
        assert!(parse_search(b"<html>").is_err());
    }

    #[test]
    fn renamed_target_accepts_only_numeric_repository_urls() {
        assert_eq!(
            renamed_target(br#"{"message":"Moved Permanently","url":"https://api.github.com/repositories/20925583"}"#).as_deref(),
            Some("https://api.github.com/repositories/20925583")
        );
        for body in [
            &br#"{"url":"https://evil.example/repositories/1"}"#[..],
            br#"{"url":"https://api.github.com/repositories/1/../../user"}"#,
            br#"{"url":"https://api.github.com/repositories/"}"#,
            br#"{"url":"https://api.github.com/repositories/1?x=y"}"#,
            br#"not json"#,
        ] {
            assert_eq!(renamed_target(body), None);
        }
    }

    #[test]
    fn status_errors_do_not_echo_bodies() {
        assert!(status_error(403, true).contains("rate limit"));
        assert!(status_error(429, false).contains("rate limit"));
        assert_eq!(
            status_error(403, false),
            "GitHub refused the search request"
        );
        assert!(status_error(500, false).contains("500"));
    }
}
