//! Pure generation helpers: prompts, output extraction, validation, references
//! and Rust-rendered pages. No processes or Tauri state here.
use super::{inventory::Inventory, models::*};
use crate::source_files as source;
pub use crate::source_files::{page_refs, ref_stats};
use std::{collections::HashSet, path::Path};

pub const PROMPT_VERSION: u32 = 1;
const ARCHITECTURE_PROMPT: &str = include_str!("prompts/architecture.md");
const MODULE_PROMPT: &str = include_str!("prompts/module.md");
const GLOSSARY_PROMPT: &str = include_str!("prompts/glossary.md");
const REPAIR_PROMPT: &str = include_str!("prompts/repair.md");

pub fn language_name(language: &str) -> &'static str {
    if language == "zh-CN" {
        "Simplified Chinese"
    } else {
        "English"
    }
}

fn module_range(depth: &str) -> &'static str {
    match depth {
        "quick" => "4-6",
        "deep" => "8-14",
        _ => "6-10",
    }
}

fn page_length(depth: &str, language: &str) -> &'static str {
    match (depth, language == "zh-CN") {
        ("quick", true) => "400-800 Chinese characters",
        ("quick", false) => "300-600 words",
        ("deep", true) => "1200-2000 Chinese characters",
        ("deep", false) => "900-1500 words",
        (_, true) => "700-1200 Chinese characters",
        (_, false) => "500-900 words",
    }
}

pub fn architecture_prompt(study: &Study, inventory_summary: &str) -> String {
    ARCHITECTURE_PROMPT
        .replace("{{MODULE_RANGE}}", module_range(&study.depth))
        .replace("{{LANGUAGE}}", language_name(&study.language))
        .replace("{{INVENTORY}}", inventory_summary)
}

fn project_line(architecture: &Architecture) -> String {
    format!(
        "{} — {}",
        architecture.project.name, architecture.project.summary
    )
}

fn module_list(architecture: &Architecture) -> String {
    architecture
        .modules
        .iter()
        .map(|m| format!("- {}: {} — {}", m.id, m.title, m.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn module_prompt(study: &Study, architecture: &Architecture, module: &Module) -> String {
    let entry = serde_json::to_string_pretty(module).unwrap_or_default();
    MODULE_PROMPT
        .replace("{{LANGUAGE}}", language_name(&study.language))
        .replace(
            "{{PAGE_LENGTH}}",
            page_length(&study.depth, &study.language),
        )
        .replace("{{PROJECT}}", &project_line(architecture))
        .replace("{{MODULES}}", &module_list(architecture))
        .replace("{{MODULE}}", &entry)
}

pub fn glossary_prompt(study: &Study, architecture: &Architecture) -> String {
    GLOSSARY_PROMPT
        .replace("{{LANGUAGE}}", language_name(&study.language))
        .replace("{{PROJECT}}", &project_line(architecture))
        .replace("{{MODULES}}", &module_list(architecture))
}

pub fn repair_prompt(task: &str, previous: &str, problems: &[String]) -> String {
    let previous: String = previous
        .chars()
        .rev()
        .take(60_000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    REPAIR_PROMPT
        .replace(
            "{{PROBLEMS}}",
            &problems
                .iter()
                .map(|p| format!("- {p}"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .replace("{{PREVIOUS}}", &previous)
        .replace("{{TASK}}", task)
}

/// Last ```json fenced block, else the outermost `{...}` span.
pub fn extract_json(text: &str) -> Option<serde_json::Value> {
    let mut candidates = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```json") {
        let body = &rest[start + 7..];
        let Some(end) = body.find("```") else { break };
        candidates.push(body[..end].trim().to_string());
        rest = &body[end + 3..];
    }
    if let (Some(a), Some(b)) = (text.find('{'), text.rfind('}')) {
        if a < b {
            candidates.push(text[a..=b].to_string());
        }
    }
    candidates.into_iter().rev().find_map(|c| {
        serde_json::from_str::<serde_json::Value>(&c)
            .ok()
            .filter(|v| v.is_object())
    })
}

/// Content between the last `<<<LOOM_PAGE` / `LOOM_PAGE>>>` markers; falls back
/// to the last top-level heading so narration before the page is dropped.
pub fn extract_page(text: &str) -> Option<String> {
    if let Some(start) = text.rfind("<<<LOOM_PAGE") {
        let body = &text[start + "<<<LOOM_PAGE".len()..];
        let body = body.split("LOOM_PAGE>>>").next().unwrap_or(body);
        let page = body.trim();
        return (!page.is_empty()).then(|| page.to_string());
    }
    let start = text
        .match_indices("\n# ")
        .last()
        .map(|(i, _)| i + 1)
        .or_else(|| text.starts_with("# ").then_some(0))?;
    let page = text[start..].trim();
    (!page.is_empty()).then(|| page.to_string())
}

#[derive(Debug)]
pub struct Validated {
    pub architecture: Architecture,
    pub warnings: Vec<String>,
}

fn valid_module_id(id: &str) -> bool {
    super::store::valid_id(id) && id.len() <= 64
}

/// Hard problems trigger a repair round; soft ones are dropped with warnings.
pub fn validate_architecture(
    value: serde_json::Value,
    checkout: &Path,
) -> Result<Validated, Vec<String>> {
    let mut architecture: Architecture = serde_json::from_value(value)
        .map_err(|e| vec![format!("JSON does not match the required shape: {e}")])?;
    architecture.schema_version = ARCHITECTURE_SCHEMA_VERSION;
    let mut problems = Vec::new();
    let mut warnings = Vec::new();
    if architecture.project.name.trim().is_empty() {
        problems.push("project.name is empty".into());
    }
    if architecture.modules.is_empty() || architecture.modules.len() > 40 {
        problems.push(format!(
            "expected 1-40 modules, got {}",
            architecture.modules.len()
        ));
    }
    let mut ids = HashSet::new();
    for module in &architecture.modules {
        if !valid_module_id(&module.id) {
            problems.push(format!(
                "module id '{}' is not lowercase kebab-case",
                module.id
            ));
        }
        if !ids.insert(module.id.clone()) {
            problems.push(format!("duplicate module id '{}'", module.id));
        }
        if !(1..=3).contains(&module.level) {
            problems.push(format!("module '{}' level must be 1, 2 or 3", module.id));
        }
    }
    let mut invalid_refs = 0usize;
    let mut total_refs = 0usize;
    for module in &mut architecture.modules {
        let before = module.paths.len();
        module.paths.retain(|path| {
            source::normalize(path)
                .ok()
                .filter(|p| !p.is_empty())
                .is_some_and(|p| source::resolve(checkout, &p).is_ok())
        });
        for path in &mut module.paths {
            let is_dir = path.ends_with('/');
            *path = source::normalize(path).unwrap_or_default();
            if is_dir {
                path.push('/');
            }
        }
        if module.paths.is_empty() {
            problems.push(format!("module '{}' has no existing paths", module.id));
        } else if module.paths.len() < before {
            warnings.push(format!(
                "module '{}': {} path(s) not found and dropped",
                module.id,
                before - module.paths.len()
            ));
        }
        let unknown: Vec<String> = module
            .depends_on
            .iter()
            .filter(|d| !ids.contains(*d) || **d == module.id)
            .cloned()
            .collect();
        if !unknown.is_empty() {
            warnings.push(format!(
                "module '{}': unknown dependsOn dropped: {}",
                module.id,
                unknown.join(", ")
            ));
            module
                .depends_on
                .retain(|d| ids.contains(d) && *d != module.id);
        }
        total_refs += module.key_files.len();
        module.key_files.retain_mut(|key| {
            match check_range(checkout, &key.path, key.start_line, key.end_line) {
                Ok(path) => {
                    key.path = path;
                    true
                }
                Err(_) => {
                    invalid_refs += 1;
                    false
                }
            }
        });
    }
    total_refs += architecture.project.entry_points.len();
    architecture.project.entry_points.retain_mut(|entry| {
        match check_range(checkout, &entry.path, entry.line.max(1), entry.line.max(1)) {
            Ok(path) => {
                entry.path = path;
                true
            }
            Err(_) => {
                invalid_refs += 1;
                false
            }
        }
    });
    if total_refs > 0 && invalid_refs * 5 > total_refs {
        problems.push(format!(
            "{invalid_refs} of {total_refs} keyFiles/entryPoints reference missing files or out-of-range lines"
        ));
    } else if invalid_refs > 0 {
        warnings.push(format!(
            "{invalid_refs} invalid source reference(s) dropped"
        ));
    }
    for layer in &mut architecture.layers {
        let before = layer.module_ids.len();
        layer.module_ids.retain(|m| ids.contains(m));
        if layer.module_ids.len() < before {
            warnings.push(format!("layer '{}': unknown module ids dropped", layer.id));
        }
    }
    for step in &mut architecture.learning_path {
        step.module_ids.retain(|m| ids.contains(m));
    }
    let levels: HashSet<u8> = architecture.learning_path.iter().map(|s| s.level).collect();
    if architecture.learning_path.is_empty() {
        problems.push("learningPath is empty".into());
    } else if levels.len() < 3 {
        warnings.push(format!(
            "learning path has {} level(s), expected 3",
            levels.len()
        ));
    }
    let unlayered: Vec<&str> = architecture
        .modules
        .iter()
        .filter(|m| {
            !architecture
                .layers
                .iter()
                .any(|l| l.module_ids.contains(&m.id))
        })
        .map(|m| m.id.as_str())
        .collect();
    if !unlayered.is_empty() {
        let ids = unlayered.iter().map(|s| s.to_string()).collect();
        architecture.layers.push(Layer {
            id: "other".into(),
            title: "Other".into(),
            module_ids: ids,
        });
    }
    architecture.modules.sort_by_key(|m| (m.order, m.level));
    if problems.is_empty() {
        Ok(Validated {
            architecture,
            warnings,
        })
    } else {
        Err(problems)
    }
}

fn check_range(checkout: &Path, path: &str, start: u32, end: u32) -> Result<String, &'static str> {
    let normalized = source::normalize(path).map_err(|_| "outside")?;
    if normalized.is_empty() {
        return Err("missing");
    }
    let total = source::line_count(checkout, &normalized).map_err(|_| "missing")?;
    if start == 0 || end < start || end > total {
        return Err("range");
    }
    Ok(normalized)
}

/// Source files covered by module path prefixes.
pub fn coverage(architecture: &Architecture, inventory: &Inventory) -> Coverage {
    let sources: Vec<&str> = inventory
        .files
        .iter()
        .filter(|f| super::inventory::is_source(&f.language))
        .map(|f| f.path.as_str())
        .collect();
    let covered = |file: &str| {
        architecture.modules.iter().any(|m| {
            m.paths.iter().any(|p| {
                let p = p.trim_end_matches('/');
                file == p || file.starts_with(&format!("{p}/"))
            })
        })
    };
    let uncovered: Vec<String> = sources
        .iter()
        .filter(|f| !covered(f))
        .map(|f| f.to_string())
        .collect();
    Coverage {
        source_files: sources.len(),
        covered: sources.len() - uncovered.len(),
        uncovered: uncovered.into_iter().take(200).collect(),
    }
}

/// Module pages need ≥ 3 valid references and < 20 % invalid ones.
pub fn page_problems(markdown: &str, refs: &[SourceRef], min_valid: usize) -> Vec<String> {
    let stats = ref_stats(refs);
    let mut problems = Vec::new();
    if !markdown.trim_start().starts_with('#') {
        problems.push("the page must start with a `#` title".into());
    }
    if stats.valid < min_valid {
        problems.push(format!(
            "only {} valid loom-src source links; at least {min_valid} are required",
            stats.valid
        ));
    }
    if stats.invalid * 5 > refs.len().max(1) {
        let bad: Vec<String> = refs
            .iter()
            .filter(|r| r.status != "valid")
            .take(12)
            .map(|r| format!("{} ({})", r.target, r.status))
            .collect();
        problems.push(format!("invalid source links: {}", bad.join(", ")));
    }
    problems
}

fn src_link(path: &str, start: u32, end: u32) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    if start == end {
        format!("[{name}:{start}](loom-src:{path}#L{start}-L{end})")
    } else {
        format!("[{name}:{start}-{end}](loom-src:{path}#L{start}-L{end})")
    }
}

struct Copy {
    overview: &'static str,
    tech: &'static str,
    entries: &'static str,
    path: &'static str,
    level: [&'static str; 3],
    architecture: &'static str,
    layers: &'static str,
    depends: &'static str,
    key_files: &'static str,
    files: &'static str,
    none: &'static str,
}

fn copy(language: &str) -> Copy {
    if language == "zh-CN" {
        Copy {
            overview: "总览",
            tech: "技术栈",
            entries: "入口",
            path: "学习路径",
            level: ["L1 全局认识", "L2 核心模块", "L3 内部机制"],
            architecture: "架构",
            layers: "分层与模块",
            depends: "依赖",
            key_files: "关键源码",
            files: "包含路径",
            none: "无",
        }
    } else {
        Copy {
            overview: "Overview",
            tech: "Tech stack",
            entries: "Entry points",
            path: "Learning path",
            level: ["L1 Big picture", "L2 Core modules", "L3 Internals"],
            architecture: "Architecture",
            layers: "Layers and modules",
            depends: "Depends on",
            key_files: "Key source",
            files: "Paths",
            none: "none",
        }
    }
}

fn module_title<'a>(architecture: &'a Architecture, id: &'a str) -> &'a str {
    architecture
        .modules
        .iter()
        .find(|m| m.id == id)
        .map(|m| m.title.as_str())
        .unwrap_or(id)
}

pub fn render_overview(architecture: &Architecture, language: &str) -> String {
    let c = copy(language);
    let p = &architecture.project;
    let mut md = format!("# {} — {}\n\n{}\n\n", p.name, c.overview, p.summary);
    if !p.overview.trim().is_empty() {
        md.push_str(p.overview.trim());
        md.push_str("\n\n");
    }
    if !p.tech_stack.is_empty() {
        md.push_str(&format!(
            "## {}\n\n{}\n\n",
            c.tech,
            p.tech_stack.join(" · ")
        ));
    }
    if !p.entry_points.is_empty() {
        md.push_str(&format!("## {}\n\n", c.entries));
        for entry in &p.entry_points {
            md.push_str(&format!(
                "- {} — {}\n",
                src_link(&entry.path, entry.line, entry.line),
                entry.why
            ));
        }
        md.push('\n');
    }
    md.push_str(&format!("## {}\n\n", c.path));
    let mut steps = architecture.learning_path.clone();
    steps.sort_by_key(|s| s.level);
    for step in steps {
        let label = c
            .level
            .get(step.level.saturating_sub(1) as usize)
            .copied()
            .unwrap_or("");
        md.push_str(&format!("### {label}：{}\n\n{}\n\n", step.title, step.goal));
        for (index, id) in step.module_ids.iter().enumerate() {
            md.push_str(&format!(
                "{}. [{}](loom-wiki:module/{id})\n",
                index + 1,
                module_title(architecture, id)
            ));
        }
        md.push('\n');
    }
    md
}

pub fn render_architecture(architecture: &Architecture, language: &str) -> String {
    let c = copy(language);
    let mut md = format!("# {} — {}\n\n", architecture.project.name, c.architecture);
    md.push_str(&format!("## {}\n\n", c.layers));
    for layer in &architecture.layers {
        md.push_str(&format!("### {}\n\n", layer.title));
        for id in &layer.module_ids {
            let Some(module) = architecture.modules.iter().find(|m| &m.id == id) else {
                continue;
            };
            md.push_str(&format!(
                "#### [{}](loom-wiki:module/{})\n\n{}\n\n",
                module.title, module.id, module.summary
            ));
            md.push_str(&format!(
                "- {}：{}\n",
                c.files,
                module
                    .paths
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            let depends = if module.depends_on.is_empty() {
                c.none.to_string()
            } else {
                module
                    .depends_on
                    .iter()
                    .map(|d| format!("[{}](loom-wiki:module/{d})", module_title(architecture, d)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            md.push_str(&format!("- {}：{depends}\n", c.depends));
            if !module.key_files.is_empty() {
                md.push_str(&format!("- {}：\n", c.key_files));
                for key in &module.key_files {
                    md.push_str(&format!(
                        "  - {} — {}\n",
                        src_link(&key.path, key.start_line, key.end_line),
                        key.why
                    ));
                }
            }
            md.push('\n');
        }
    }
    md
}

/// Page list derived from the architecture; readiness is filled by the caller.
pub fn page_plan(architecture: &Architecture, language: &str) -> Vec<PageInfo> {
    let c = copy(language);
    let mut pages = vec![
        PageInfo {
            page_id: "overview".into(),
            title: c.overview.into(),
            kind: "overview".into(),
            module_id: None,
            level: None,
            ready: false,
            refs: None,
        },
        PageInfo {
            page_id: "architecture".into(),
            title: c.architecture.into(),
            kind: "architecture".into(),
            module_id: None,
            level: None,
            ready: false,
            refs: None,
        },
    ];
    for module in &architecture.modules {
        pages.push(PageInfo {
            page_id: format!("module:{}", module.id),
            title: module.title.clone(),
            kind: "module".into(),
            module_id: Some(module.id.clone()),
            level: Some(module.level),
            ready: false,
            refs: None,
        });
    }
    pages.push(PageInfo {
        page_id: "glossary".into(),
        title: if language == "zh-CN" {
            "术语表"
        } else {
            "Glossary"
        }
        .into(),
        kind: "glossary".into(),
        module_id: None,
        level: None,
        ready: false,
        refs: None,
    });
    pages
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::source_files::parse_target;
    use std::fs;

    pub(crate) fn fixture_checkout() -> std::path::PathBuf {
        let root = super::super::store::tests::temp_root();
        fs::create_dir_all(root.join("src/core")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() {\n    core::run();\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/core/mod.rs"),
            (1..=50).map(|i| format!("// {i}\n")).collect::<String>(),
        )
        .unwrap();
        root
    }

    pub(crate) fn architecture_json() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "project": {"name": "Demo", "summary": "A demo.", "overview": "It runs.", "techStack": ["Rust"],
                "entryPoints": [{"path": "src/main.rs", "line": 1, "why": "main"}]},
            "layers": [{"id": "entry", "title": "Entry", "moduleIds": ["cli"]}],
            "modules": [
                {"id": "cli", "title": "CLI", "summary": "Starts.", "level": 1, "order": 1, "paths": ["src/main.rs", "src/missing.rs"],
                 "dependsOn": ["core", "ghost"], "keyFiles": [{"path": "src/main.rs", "startLine": 1, "endLine": 3, "why": "main"}]},
                {"id": "core", "title": "Core", "summary": "Runs.", "level": 2, "order": 2, "paths": ["./src/core/"],
                 "dependsOn": [], "keyFiles": [{"path": "src/core/mod.rs", "startLine": 10, "endLine": 20, "why": "run"}]}
            ],
            "learningPath": [
                {"level": 1, "title": "Start", "goal": "g", "moduleIds": ["cli"]},
                {"level": 2, "title": "Core", "goal": "g", "moduleIds": ["core"]},
                {"level": 3, "title": "Deep", "goal": "g", "moduleIds": ["core", "ghost"]}
            ]
        })
    }

    #[test]
    fn extraction_prefers_last_fence_and_page_markers() {
        let text = "thinking {not json}\n```json\n{\"a\":1}\n```\nmore\n```json\n{\"a\":2}\n```";
        assert_eq!(extract_json(text).unwrap()["a"], 2);
        assert_eq!(extract_json("noise {\"b\": true} tail").unwrap()["b"], true);
        assert!(extract_json("no json here").is_none());
        let page = "Let me read files.\n<<<LOOM_PAGE\n# Title\nbody\nLOOM_PAGE>>>\n";
        assert_eq!(extract_page(page).unwrap(), "# Title\nbody");
        assert_eq!(
            extract_page("I will look.\n# Real\ntext").unwrap(),
            "# Real\ntext"
        );
        assert!(extract_page("no heading").is_none());
    }

    #[test]
    fn architecture_validation_prunes_soft_issues_and_rejects_hard_ones() {
        let checkout = fixture_checkout();
        let validated = validate_architecture(architecture_json(), &checkout).unwrap();
        let a = &validated.architecture;
        assert_eq!(a.modules[0].paths, ["src/main.rs"]);
        assert_eq!(a.modules[1].paths, ["src/core/"]);
        assert_eq!(a.modules[0].depends_on, ["core"]);
        assert_eq!(a.learning_path[2].module_ids, ["core"]);
        assert_eq!(
            a.layers.last().unwrap().module_ids,
            ["core"],
            "unlayered module grouped"
        );
        assert!(validated.warnings.iter().any(|w| w.contains("not found")));

        let mut bad = architecture_json();
        bad["modules"][1]["id"] = "cli".into();
        bad["modules"][0]["keyFiles"][0]["endLine"] = 99.into();
        bad["project"]["entryPoints"][0]["path"] = "../etc/passwd".into();
        let problems = validate_architecture(bad, &checkout).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("duplicate")));
        assert!(problems.iter().any(|p| p.contains("out-of-range")));
        let drift = serde_json::json!({"project": {"name": "x", "entryPoints": ["main.rs"]}});
        assert!(validate_architecture(drift, &checkout).unwrap_err()[0].contains("shape"));
        fs::remove_dir_all(checkout).unwrap();
    }

    #[test]
    fn references_are_validated_and_rendered_pages_use_valid_links() {
        let checkout = fixture_checkout();
        let md = "# Page\n[a](loom-src:src/main.rs#L1-L3) [b](loom-src:src/main.rs#L2-L9) \
                  [c](loom-src:../x#L1-L2) [d](loom-src:src/nope.rs#L1-L1) [e](loom-src:src/core/mod.rs) [a](loom-src:src/main.rs#L1-L3)";
        let refs = page_refs(md, &checkout);
        let statuses: Vec<_> = refs.iter().map(|r| r.status.as_str()).collect();
        assert_eq!(statuses, ["valid", "range", "outside", "missing", "valid"]);
        assert_eq!(
            ref_stats(&refs),
            RefStats {
                valid: 2,
                invalid: 3
            }
        );
        assert!(!page_problems(md, &refs, 3).is_empty());
        assert_eq!(parse_target("a.rs#L4"), ("a.rs".into(), Some((4, 4))));

        let a = validate_architecture(architecture_json(), &checkout)
            .unwrap()
            .architecture;
        for language in ["en", "zh-CN"] {
            for page in [
                render_overview(&a, language),
                render_architecture(&a, language),
            ] {
                let refs = page_refs(&page, &checkout);
                assert!(!refs.is_empty());
                assert!(refs.iter().all(|r| r.status == "valid"), "{page}");
                assert!(page.contains("loom-wiki:module/core"));
            }
        }
        let plan = page_plan(&a, "en");
        assert_eq!(plan.len(), 2 + a.modules.len() + 1);
        assert_eq!(plan[2].page_id, "module:cli");
        fs::remove_dir_all(checkout).unwrap();
    }

    #[test]
    fn coverage_counts_source_prefixes_only() {
        let checkout = fixture_checkout();
        let a = validate_architecture(architecture_json(), &checkout)
            .unwrap()
            .architecture;
        let inventory = super::super::inventory::build(&checkout, "src/main.rs\0src/core/mod.rs\0");
        let coverage = coverage(&a, &inventory);
        assert_eq!((coverage.source_files, coverage.covered), (2, 2));
        fs::remove_dir_all(checkout).unwrap();
    }

    #[test]
    fn prompts_fill_every_placeholder() {
        let study = super::super::store::tests::sample_study("study-p");
        let checkout = fixture_checkout();
        let a = validate_architecture(architecture_json(), &checkout)
            .unwrap()
            .architecture;
        for prompt in [
            architecture_prompt(&study, "files"),
            module_prompt(&study, &a, &a.modules[0]),
            glossary_prompt(&study, &a),
            repair_prompt("task", "previous", &["bad".into()]),
        ] {
            assert!(!prompt.contains("{{"), "{prompt}");
            assert!(prompt.contains("data") || prompt.contains("Problems"));
        }
        fs::remove_dir_all(checkout).unwrap();
    }
}
