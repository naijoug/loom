use crate::{
    models::{now_ms, Task},
    storage,
};
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct PlanHtmlMeta {
    pub title: String,
    pub task_id: String,
    pub generated_at_ms: u128,
    pub agent_names: Vec<String>,
    pub review_count: usize,
}

impl PlanHtmlMeta {
    pub fn from_task(task: &Task) -> Self {
        let mut agent_names = task
            .agent_invocations
            .iter()
            .filter(|invocation| invocation.status == "succeeded")
            .map(|invocation| invocation.agent_name.clone())
            .collect::<Vec<_>>();
        agent_names.dedup();

        Self {
            title: task.title.clone(),
            task_id: task.id.clone(),
            generated_at_ms: now_ms(),
            agent_names,
            review_count: task.plan_reviews.len(),
        }
    }
}

#[tauri::command]
pub fn read_plan_html(project_path: String, md_path: String) -> Result<String, String> {
    let path = validate_plan_markdown_path(Path::new(&project_path), Path::new(&md_path))?;
    let markdown = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read plan markdown: {error}"))?;
    let title = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Plan")
        .to_string();
    Ok(render_plan_html(
        &markdown,
        &PlanHtmlMeta {
            title,
            task_id: "preview".to_string(),
            generated_at_ms: now_ms(),
            agent_names: Vec::new(),
            review_count: 0,
        },
    ))
}

#[tauri::command]
pub fn open_plan_html(project_path: String, html_path: String) -> Result<(), String> {
    let path = validate_plan_html_path(Path::new(&project_path), Path::new(&html_path))?;
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|error| format!("failed to open plan HTML: {error}"))
}

#[tauri::command]
pub fn open_planning_evidence(project_path: String, evidence_path: String) -> Result<(), String> {
    let path =
        validate_planning_evidence_path(Path::new(&project_path), Path::new(&evidence_path))?;
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|error| format!("failed to open planning evidence: {error}"))
}

/// Open a plan Markdown document in a dedicated in-app viewer window. The
/// window loads the `plan-viewer.html` entry, which renders the document via
/// `read_plan_html` (same whitelist and escaping as the inline preview).
#[tauri::command]
pub fn open_plan_viewer(
    app: tauri::AppHandle,
    project_path: String,
    md_path: String,
) -> Result<(), String> {
    use tauri::Manager;

    let path = validate_plan_markdown_path(Path::new(&project_path), Path::new(&md_path))?;
    let label = format!(
        "plan-viewer-{:016x}",
        stable_hash(&path.display().to_string())
    );

    if let Some(window) = app.get_webview_window(&label) {
        return window
            .set_focus()
            .map_err(|error| format!("failed to focus plan viewer window: {error}"));
    }

    let title = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Plan")
        .to_string();
    let url = format!(
        "plan-viewer.html?project={}&path={}",
        encode_query_component(&project_path),
        encode_query_component(&path.display().to_string()),
    );
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App(url.into()))
        .title(format!("Loom Plan — {title}"))
        .inner_size(980.0, 760.0)
        .min_inner_size(560.0, 420.0)
        .build()
        .map_err(|error| format!("failed to open plan viewer window: {error}"))?;
    Ok(())
}

fn stable_hash(value: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub fn write_task_plan_html(task: &Task) -> Result<Option<String>, String> {
    let (Some(plan_path), Some(markdown)) = (&task.final_plan_path, &task.final_plan) else {
        return Ok(None);
    };

    let html_path = write_plan_html_file(
        Path::new(plan_path),
        markdown,
        &PlanHtmlMeta::from_task(task),
    )?;
    Ok(Some(html_path.display().to_string()))
}

pub fn write_plan_html_file(
    markdown_path: &Path,
    markdown: &str,
    meta: &PlanHtmlMeta,
) -> Result<PathBuf, String> {
    let html_path = markdown_path.with_extension("html");
    let html = render_plan_html(markdown, meta);
    fs::write(&html_path, html).map_err(|error| format!("failed to write plan HTML: {error}"))?;
    Ok(html_path)
}

pub fn render_plan_html(markdown: &str, meta: &PlanHtmlMeta) -> String {
    let headings = extract_headings(markdown);
    let body = render_markdown_body(markdown, &headings);
    let body = wrap_major_sections(&body);
    let toc = render_toc(&headings);
    let agents = if meta.agent_names.is_empty() {
        "No successful planning agents recorded".to_string()
    } else {
        escape_html(&meta.agent_names.join(", "))
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
:root {{
  color-scheme: light dark;
  --bg: #f7f8fb;
  --surface: #ffffff;
  --surface-2: #f1f4f8;
  --text: #17202a;
  --muted: #667085;
  --border: #d9e0ea;
  --accent: #2563eb;
  --risk: #b54708;
  --blocker: #b42318;
  --ok: #027a48;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --bg: #111318;
    --surface: #181b21;
    --surface-2: #20242c;
    --text: #eef2f7;
    --muted: #aab3c2;
    --border: #303642;
    --accent: #7aa2ff;
    --risk: #fdb022;
    --blocker: #ff7a7a;
    --ok: #32d583;
  }}
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  line-height: 1.55;
}}
a {{ color: var(--accent); }}
.layout {{
  display: grid;
  grid-template-columns: minmax(180px, 260px) minmax(0, 1fr);
  min-height: 100vh;
}}
.toc {{
  position: sticky;
  top: 0;
  height: 100vh;
  overflow: auto;
  border-right: 1px solid var(--border);
  background: var(--surface);
  padding: 20px 16px;
}}
.toc h2 {{
  margin: 0 0 14px;
  font-size: 13px;
  letter-spacing: 0;
  text-transform: uppercase;
  color: var(--muted);
}}
.toc a {{
  display: block;
  padding: 6px 0;
  color: var(--muted);
  text-decoration: none;
  font-size: 14px;
}}
.toc a:hover {{ color: var(--accent); }}
.doc {{
  min-width: 0;
  padding: 32px clamp(18px, 5vw, 72px) 72px;
}}
.doc-header {{
  margin: 0 0 24px;
  border-bottom: 1px solid var(--border);
  padding-bottom: 20px;
}}
.doc-header h1 {{
  margin: 0 0 12px;
  font-size: clamp(28px, 4vw, 44px);
  line-height: 1.12;
  letter-spacing: 0;
}}
.meta {{
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  color: var(--muted);
  font-size: 13px;
}}
.badge {{
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--surface-2);
  padding: 3px 8px;
}}
.risk-badge {{
  border-color: color-mix(in srgb, var(--risk) 45%, var(--border));
  color: var(--risk);
}}
.blocker-badge {{
  border-color: color-mix(in srgb, var(--blocker) 45%, var(--border));
  color: var(--blocker);
}}
.plan-body {{
  max-width: 980px;
}}
.plan-section {{
  margin: 0 0 14px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface);
}}
.plan-section > summary {{
  cursor: pointer;
  padding: 14px 16px;
  font-size: 20px;
  font-weight: 700;
}}
.section-body {{
  border-top: 1px solid var(--border);
  padding: 4px 16px 16px;
}}
.plan-body > :not(.plan-section) {{
  max-width: 980px;
}}
h1, h2, h3, h4 {{ letter-spacing: 0; }}
h1, h2, h3 {{ line-height: 1.22; }}
pre, code {{
  font-family: "Geist Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}}
pre {{
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface-2);
  padding: 12px;
}}
code {{
  border-radius: 4px;
  background: var(--surface-2);
  padding: 1px 4px;
}}
pre code {{ background: transparent; padding: 0; }}
table {{
  width: 100%;
  border-collapse: collapse;
  margin: 16px 0;
  font-size: 14px;
}}
th, td {{
  border: 1px solid var(--border);
  padding: 8px 10px;
  vertical-align: top;
}}
th {{
  background: var(--surface-2);
  text-align: left;
}}
blockquote {{
  margin: 16px 0;
  border-left: 4px solid var(--border);
  padding-left: 12px;
  color: var(--muted);
}}
img {{ max-width: 100%; }}
@media (max-width: 760px) {{
  .layout {{ display: block; }}
  .toc {{
    position: static;
    height: auto;
    border-right: 0;
    border-bottom: 1px solid var(--border);
  }}
  .doc {{ padding: 20px 14px 48px; }}
}}
@media print {{
  .layout {{ display: block; }}
  .toc {{ display: none; }}
  .doc {{ padding: 0; }}
  .plan-section {{ break-inside: avoid; }}
}}
</style>
</head>
<body>
<div class="layout">
<nav class="toc" aria-label="Table of contents">
<h2>Contents</h2>
{toc}
</nav>
<main class="doc">
<header class="doc-header">
<h1>{title}</h1>
<div class="meta">
<span class="badge">Task {task_id}</span>
<span class="badge">Generated {generated_at_ms}</span>
<span class="badge">Agents: {agents}</span>
<span class="badge">Reviews: {review_count}</span>
<span class="badge risk-badge">Risk</span>
<span class="badge blocker-badge">Blocker</span>
</div>
</header>
<article class="plan-body">
{body}
</article>
</main>
</div>
</body>
</html>
"#,
        title = escape_html(&meta.title),
        task_id = escape_html(&meta.task_id),
        generated_at_ms = meta.generated_at_ms,
        agents = agents,
        review_count = meta.review_count,
        toc = toc,
        body = body,
    )
}

fn render_markdown_body(markdown: &str, headings: &[Heading]) -> String {
    let mut heading_index = 0usize;
    let parser = Parser::new_ext(markdown, markdown_options()).map(|event| match event {
        Event::Html(value) | Event::InlineHtml(value) => Event::Text(value),
        Event::Start(Tag::Heading {
            level,
            id: _,
            classes,
            attrs,
        }) => {
            let id = headings
                .get(heading_index)
                .map(|heading| heading.id.clone());
            heading_index += 1;
            Event::Start(Tag::Heading {
                level,
                id: id.map(CowStr::from),
                classes,
                attrs,
            })
        }
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: sanitize_url(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: sanitize_url(dest_url),
            title,
            id,
        }),
        other => other,
    });

    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_FOOTNOTES);
    options
}

fn render_toc(headings: &[Heading]) -> String {
    let items = headings
        .iter()
        .filter(|heading| heading.level <= 3)
        .map(|heading| {
            format!(
                r##"<a class="toc-level-{level}" href="#{id}">{text}</a>"##,
                level = heading.level,
                id = escape_attr(&heading.id),
                text = escape_html(&heading.text),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    if items.is_empty() {
        "<span class=\"badge\">No headings</span>".to_string()
    } else {
        items
    }
}

fn wrap_major_sections(html: &str) -> String {
    let marker = "<h2 id=\"";
    let mut output = String::new();
    let mut remainder = html;

    let Some(first_index) = remainder.find(marker) else {
        return html.to_string();
    };
    output.push_str(&remainder[..first_index]);
    remainder = &remainder[first_index..];

    while let Some(next_index) = remainder.find(marker) {
        let section = if next_index == 0 {
            if let Some(after_current) = remainder[marker.len()..].find(marker) {
                let end = marker.len() + after_current;
                let value = &remainder[..end];
                remainder = &remainder[end..];
                value
            } else {
                let value = remainder;
                remainder = "";
                value
            }
        } else {
            let value = &remainder[..next_index];
            remainder = &remainder[next_index..];
            value
        };

        if !section.is_empty() {
            output.push_str(&wrap_section(section));
        }

        if remainder.is_empty() {
            break;
        }
    }

    output
}

fn wrap_section(section: &str) -> String {
    let Some(heading_end) = section.find("</h2>") else {
        return section.to_string();
    };
    let heading = &section[..heading_end + 5];
    let body = &section[heading_end + 5..];
    let id = heading
        .split_once("id=\"")
        .and_then(|(_, rest)| rest.split_once('"').map(|(id, _)| id))
        .unwrap_or("");
    let title = heading
        .split_once('>')
        .and_then(|(_, rest)| rest.split_once("</h2>").map(|(title, _)| title))
        .unwrap_or("Section");

    format!(
        r#"<details class="plan-section" open><summary id="{id}">{title}</summary><div class="section-body">{body}</div></details>"#,
        id = escape_attr(id),
        title = title,
        body = body,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Heading {
    level: usize,
    text: String,
    id: String,
}

fn extract_headings(markdown: &str) -> Vec<Heading> {
    let mut headings = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|char| *char == '#').count();
        if !(1..=6).contains(&level) || trimmed.as_bytes().get(level) != Some(&b' ') {
            continue;
        }

        let text = strip_inline_markdown(trimmed[level..].trim());
        if text.is_empty() {
            continue;
        }

        let base_id = slugify_anchor(&text);
        let mut id = base_id.clone();
        let mut suffix = 2;
        while headings.iter().any(|heading: &Heading| heading.id == id) {
            id = format!("{base_id}-{suffix}");
            suffix += 1;
        }
        headings.push(Heading { level, text, id });
    }

    headings
}

fn strip_inline_markdown(value: &str) -> String {
    value
        .trim_matches('#')
        .replace(['`', '*', '_'], "")
        .trim()
        .to_string()
}

fn slugify_anchor(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() || matches!(character, '\u{4e00}'..='\u{9fff}') {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

fn sanitize_url(url: CowStr<'_>) -> CowStr<'_> {
    if is_safe_url(&url) {
        url
    } else {
        CowStr::from("#blocked-unsafe-link")
    }
}

fn is_safe_url(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.starts_with('#')
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
    {
        return true;
    }

    match trimmed.split_once(':') {
        Some((scheme, _)) => matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto" | "file"
        ),
        None => true,
    }
}

fn validate_plan_markdown_path(project_path: &Path, md_path: &Path) -> Result<PathBuf, String> {
    if md_path.extension().and_then(|value| value.to_str()) != Some("md") {
        return Err("plan preview only accepts Markdown files".to_string());
    }

    let project = fs::canonicalize(project_path)
        .map_err(|error| format!("failed to resolve project path: {error}"))?;
    let target = fs::canonicalize(md_path)
        .map_err(|error| format!("failed to resolve plan path: {error}"))?;
    let docs_plans = canonicalize_allowed_dir(&storage::project_plans_dir(&project))?;
    let loom_planning =
        canonicalize_allowed_dir(&storage::project_loom_dir(&project).join("planning"))?;

    if target.starts_with(&docs_plans) || target.starts_with(&loom_planning) {
        Ok(target)
    } else {
        Err("plan preview path must be under docs/plans or .loom/planning".to_string())
    }
}

fn validate_planning_evidence_path(
    project_path: &Path,
    evidence_path: &Path,
) -> Result<PathBuf, String> {
    let project = fs::canonicalize(project_path)
        .map_err(|error| format!("failed to resolve project path: {error}"))?;
    let target = fs::canonicalize(evidence_path)
        .map_err(|error| format!("failed to resolve planning evidence path: {error}"))?;
    let loom_planning =
        canonicalize_allowed_dir(&storage::project_loom_dir(&project).join("planning"))?;

    if target.starts_with(&loom_planning) {
        Ok(target)
    } else {
        Err("planning evidence path must be under .loom/planning".to_string())
    }
}

fn validate_plan_html_path(project_path: &Path, html_path: &Path) -> Result<PathBuf, String> {
    if html_path.extension().and_then(|value| value.to_str()) != Some("html") {
        return Err("plan opener only accepts HTML files".to_string());
    }

    let project = fs::canonicalize(project_path)
        .map_err(|error| format!("failed to resolve project path: {error}"))?;
    let target = fs::canonicalize(html_path)
        .map_err(|error| format!("failed to resolve plan HTML path: {error}"))?;
    let docs_plans = canonicalize_allowed_dir(&storage::project_plans_dir(&project))?;

    if target.starts_with(&docs_plans) {
        Ok(target)
    } else {
        Err("plan HTML path must be under docs/plans".to_string())
    }
}

fn canonicalize_allowed_dir(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        fs::canonicalize(path).map_err(|error| {
            format!(
                "failed to resolve allowed plan directory {}: {error}",
                path.display()
            )
        })
    } else {
        Ok(path.to_path_buf())
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_attr(value: &str) -> String {
    escape_html(value).replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> PlanHtmlMeta {
        PlanHtmlMeta {
            title: "Planning".to_string(),
            task_id: "task-1".to_string(),
            generated_at_ms: 1,
            agent_names: vec!["Codex".to_string(), "Claude".to_string()],
            review_count: 2,
        }
    }

    #[test]
    fn render_plan_html_builds_toc_and_collapsible_sections() {
        let html = render_plan_html(
            "# Plan\n\n## Goal\n\nShip it.\n\n## Risks\n\n- Low",
            &meta(),
        );

        assert!(html.contains("href=\"#goal\""));
        assert!(html.contains("<details class=\"plan-section\" open>"));
        assert!(html.contains("<summary id=\"goal\">Goal</summary>"));
    }

    #[test]
    fn render_plan_html_escapes_raw_html() {
        let html = render_plan_html(
            "## Risk\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>",
            &meta(),
        );

        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("<img src=x onerror=alert(1)>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    }

    #[test]
    fn render_plan_html_blocks_dangerous_links() {
        let html = render_plan_html(
            "[bad](javascript:alert(1))\n\n![bad](data:text/html,boom)",
            &meta(),
        );

        assert!(!html.contains("javascript:alert"));
        assert!(!html.contains("data:text/html"));
        assert!(html.contains("#blocked-unsafe-link"));
    }

    #[test]
    fn validate_plan_markdown_rejects_paths_outside_project() {
        let root = std::env::temp_dir().join(format!("loom-plan-html-{}", now_ms()));
        let allowed_dir = root.join("docs").join("plans").join("2026-06-10");
        let outside_dir = root.join("outside");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        let allowed = allowed_dir.join("plan.md");
        let outside = outside_dir.join("plan.md");
        fs::write(&allowed, "# Allowed").unwrap();
        fs::write(&outside, "# Outside").unwrap();

        assert!(validate_plan_markdown_path(&root, &allowed).is_ok());
        assert!(validate_plan_markdown_path(&root, &outside).is_err());
        assert!(validate_plan_markdown_path(&root, &allowed.with_extension("txt")).is_err());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn validate_planning_evidence_accepts_only_loom_planning_paths() {
        let root = std::env::temp_dir().join(format!("loom-planning-evidence-{}", now_ms()));
        let allowed_dir = root.join(".loom").join("planning").join("task-1");
        let outside_dir = root.join("outside");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::create_dir_all(&outside_dir).unwrap();
        let allowed = allowed_dir.join("agent.stderr.log");
        let outside = outside_dir.join("agent.stderr.log");
        fs::write(&allowed, "fatal: nope").unwrap();
        fs::write(&outside, "fatal: outside").unwrap();

        assert!(validate_planning_evidence_path(&root, &allowed).is_ok());
        assert!(validate_planning_evidence_path(&root, &outside).is_err());

        let _ = fs::remove_dir_all(&root);
    }
}
