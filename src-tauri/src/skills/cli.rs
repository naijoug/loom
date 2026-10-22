//! CLI argument/output compatibility over the same catalog and installation service.
//! No Tauri window, shell evaluation, or independent filesystem installer is used.
use super::{
    catalog,
    error::{ErrorCode, Result, SkillsError},
    files,
    installers::{self, ConflictPolicy, InstallMode, InstallRequest, UninstallRequest},
    operations::Operation,
    package::{self, EntryState},
    repository::Repository,
    runtime_paths::RuntimePaths,
    targets::TargetContext,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const HELP: &str = "Usage: loom-skills tools|list|detail|install|uninstall|status|doctor|pending|recover [options] [skill ...]\n\nOptions:\n  --skills-root DIR      Explicit local source; otherwise use the Loom library\n  --data-dir DIR         Library override (does not change the user lock domain)\n  --category NAME        global|cron|auto|manual\n  --tool NAME            codex|chatgpt|claude|claude-code|amp|custom (default codex)\n                        Repeat or comma-separate tools for a shared update\n  --scope SCOPE          global|project (default global)\n  --project-root DIR     Explicit project grant (default current directory for project scope)\n  --target-dir DIR       Explicit dedicated skills directory (one tool only)\n  --mode MODE            symlink|copy (default symlink)\n  --force               Replace verified managed installs; never delete unmanaged content\n  --with-slash-commands  Create manual Codex/Claude command wrappers\n  --json                Machine-readable output\n  --preview             Return installation/uninstallation plans without applying\n\nInstall without skill names selects all matching source skills. Uninstall without\nnames selects managed installs in the chosen target/source. Unknown ownership and\nuser-modified uninstalls are refused even with --force. recover takes an operation ID.\n";

pub struct Output {
    pub text: String,
    pub exit_code: i32,
    // Keep a real CLI admitted until stdout is flushed and the process exits.
    _lifecycle: Option<super::coordination::LifecycleLease>,
}
#[derive(Debug)]
pub struct Options {
    command: String,
    skills_root: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    category: Option<String>,
    tools: Vec<String>,
    scope: String,
    project: Option<PathBuf>,
    target: Option<PathBuf>,
    mode: InstallMode,
    force: bool,
    slash: bool,
    json: bool,
    preview: bool,
    selected: Vec<String>,
}
fn absolute(value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
impl Options {
    pub fn parse(args: &[String]) -> Result<Self> {
        let command = args.first().map(String::as_str).unwrap_or("help");
        let mut out = Self {
            command: command.into(),
            skills_root: None,
            data_dir: None,
            category: None,
            tools: vec!["codex".into()],
            scope: "global".into(),
            project: None,
            target: None,
            mode: InstallMode::Symlink,
            force: false,
            slash: false,
            json: false,
            preview: false,
            selected: Vec::new(),
        };
        let mut index = 1;
        let mut tools_seen = false;
        while index < args.len() {
            let arg = &args[index];
            index += 1;
            let mut value = || -> Result<&str> {
                let value = args
                    .get(index)
                    .ok_or_else(|| SkillsError::invalid(format!("Missing value for {arg}")))?;
                index += 1;
                Ok(value)
            };
            match arg.as_str() {
                "--skills-root" => out.skills_root = Some(absolute(value()?)?),
                "--data-dir" => out.data_dir = Some(absolute(value()?)?),
                "--category" => out.category = Some(value()?.into()),
                "--scope" => out.scope = value()?.into(),
                "--project-root" => out.project = Some(absolute(value()?)?),
                "--target-dir" => out.target = Some(absolute(value()?)?),
                "--tool" => {
                    if !tools_seen {
                        out.tools.clear();
                        tools_seen = true;
                    }
                    for tool in value()?.split(',') {
                        out.tools.push(
                            match tool {
                                "chatgpt" => "codex",
                                "claude" => "claude-code",
                                tool => tool,
                            }
                            .into(),
                        );
                    }
                }
                "--mode" => {
                    out.mode = match value()? {
                        "copy" => InstallMode::Copy,
                        "symlink" => InstallMode::Symlink,
                        _ => return Err(SkillsError::invalid("Mode must be copy or symlink")),
                    }
                }
                "--force" => out.force = true,
                "--with-slash-commands" => out.slash = true,
                "--json" => out.json = true,
                "--preview" => out.preview = true,
                "--help" | "-h" => out.command = "help".into(),
                "--" => {
                    out.selected.extend_from_slice(&args[index..]);
                    break;
                }
                value if value.starts_with('-') => {
                    return Err(SkillsError::invalid(format!("Unknown option: {value}")))
                }
                value => out.selected.push(value.into()),
            }
        }
        if ["-h", "--help"].contains(&out.command.as_str()) {
            out.command = "help".into();
        }
        if ![
            "help",
            "tools",
            "list",
            "detail",
            "install",
            "uninstall",
            "status",
            "doctor",
            "pending",
            "recover",
        ]
        .contains(&out.command.as_str())
        {
            return Err(SkillsError::invalid(
                "Unknown command; run loom-skills --help",
            ));
        }
        if !["global", "project"].contains(&out.scope.as_str())
            || out
                .tools
                .iter()
                .any(|t| !["codex", "claude-code", "amp", "custom"].contains(&t.as_str()))
        {
            return Err(SkillsError::invalid("Invalid tool or scope"));
        }
        if out
            .category
            .as_ref()
            .is_some_and(|c| !["global", "cron", "auto", "manual"].contains(&c.as_str()))
        {
            return Err(SkillsError::invalid("Unknown category"));
        }
        out.tools.sort();
        out.tools.dedup();
        if out.target.is_some() && out.tools.len() != 1 {
            return Err(SkillsError::invalid("--target-dir requires one tool"));
        }
        if out.tools.iter().any(|t| t == "custom") && out.target.is_none() {
            return Err(SkillsError::invalid("Custom tool requires --target-dir"));
        }
        Ok(out)
    }
}
#[derive(Clone, Serialize)]
struct Item {
    id: String,
    path: String,
    category: String,
    title: String,
    summary: String,
    manual_group: String,
    source_skill_id: String,
    source_id: String,
    #[serde(skip)]
    has_injection: bool,
    #[serde(skip)]
    aliases: Vec<String>,
}
fn local_identity(root: &Path) -> Result<(String, PathBuf)> {
    let root = root.canonicalize()?;
    Ok((
        format!(
            "src-{}",
            &files::digest(root.to_string_lossy().as_bytes())[..32]
        ),
        root,
    ))
}
fn item(root: &Path, skill: &catalog::SkillSummary) -> Result<Item> {
    let detail = catalog::detail(&skill.source_id, root, &skill.relative_path)?;
    let front = catalog::frontmatter(&detail.content)?.0;
    let path = skill
        .relative_path
        .strip_suffix("/SKILL.md")
        .unwrap_or("")
        .to_string();
    let leaf = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&skill.name)
        .to_string();
    let id = detail
        .manifest
        .get("id")
        .or_else(|| front.get("name"))
        .cloned()
        .unwrap_or_else(|| leaf.clone());
    let category = catalog::activation_category(&skill.relative_path, &detail.manifest);
    let group = if category == "manual" {
        path.strip_prefix("manual/")
            .and_then(|p| p.split('/').next())
            .map(str::to_owned)
            .or_else(|| detail.manifest.get("manual_group").cloned())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let mut aliases = vec![
        id.clone(),
        skill.name.clone(),
        leaf.clone(),
        skill.id.clone(),
    ];
    if !group.is_empty() {
        aliases.extend([format!("{group}-{leaf}"), format!("ng-{group}-{leaf}")]);
    }
    Ok(Item {
        id,
        path,
        category,
        title: detail
            .manifest
            .get("title")
            .cloned()
            .unwrap_or_else(|| skill.title.clone()),
        summary: detail
            .manifest
            .get("summary")
            .cloned()
            .unwrap_or_else(|| skill.description.clone()),
        manual_group: group,
        source_skill_id: skill.id.clone(),
        source_id: skill.source_id.clone(),
        has_injection: detail
            .related_files
            .iter()
            .any(|file| file.relative_path == "inject.md"),
        aliases,
    })
}
fn catalog_items(repo: &Repository, options: &Options) -> Result<(Vec<Item>, Vec<Value>)> {
    let mut items = Vec::new();
    let mut issues = Vec::new();
    if let Some(root) = &options.skills_root {
        let (id, root) = local_identity(root)?;
        let scan = catalog::scan(&id, &root)?;
        for skill in scan.skills {
            items.push(item(&root, &skill)?);
        }
        for issue in scan.issues {
            issues.push(serde_json::to_value(issue).unwrap());
        }
    } else {
        let library = repo.list()?;
        for skill in library.skills {
            let source = repo.get_source(&skill.source_id)?;
            items.push(item(&repo.source_directory(&source)?, &skill)?);
        }
        for issue in library.issues {
            issues.push(serde_json::to_value(issue).unwrap());
        }
    }
    items.retain(|i| options.category.as_ref().is_none_or(|c| c == &i.category));
    items.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then(a.source_skill_id.cmp(&b.source_skill_id))
    });
    Ok((items, issues))
}
fn select(items: &[Item], names: &[String]) -> Result<Vec<Item>> {
    if names.is_empty() {
        return Ok(items.to_vec());
    }
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for name in names {
        let matches = items
            .iter()
            .filter(|i| i.aliases.contains(name))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(SkillsError::invalid(format!(
                "Skill {name} is unknown or ambiguous; use its source_skill_id"
            )));
        }
        if seen.insert(matches[0].source_skill_id.clone()) {
            selected.push(matches[0].clone());
        }
    }
    Ok(selected)
}
fn format_output(value: Value, json_mode: bool, exit_code: i32) -> Result<Output> {
    let text = if json_mode {
        format!(
            "{}\n",
            serde_json::to_string_pretty(&value)
                .map_err(|_| SkillsError::invalid("Could not encode CLI output"))?
        )
    } else if value
        .get("plans")
        .and_then(Value::as_array)
        .is_some_and(|plans| !plans.is_empty())
    {
        format!("{}\n", serde_json::to_string_pretty(&value).unwrap())
    } else if let Some(items) = value.get("items").and_then(Value::as_array) {
        items
            .iter()
            .map(|i| {
                format!(
                    "{}\t{}\t{}\n",
                    i["id"].as_str().unwrap_or(""),
                    i["category"].as_str().unwrap_or(""),
                    i["title"].as_str().unwrap_or("")
                )
            })
            .collect()
    } else if let Some(results) = value.get("results").and_then(Value::as_array) {
        results
            .iter()
            .map(|r| {
                format!(
                    "{}: {}{}\n",
                    r["skill"].as_str().unwrap_or(""),
                    r["status"].as_str().unwrap_or(""),
                    r["reason"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(|s| format!(" ({s})"))
                        .unwrap_or_default()
                )
            })
            .collect()
    } else {
        format!("{}\n", serde_json::to_string_pretty(&value).unwrap())
    };
    Ok(Output {
        text,
        exit_code,
        _lifecycle: None,
    })
}
pub fn run(args: &[String]) -> Result<Output> {
    let options = Options::parse(args)?;
    if options.command == "help" {
        return Ok(Output {
            text: HELP.into(),
            exit_code: if args.is_empty() { 2 } else { 0 },
            _lifecycle: None,
        });
    }
    let paths = RuntimePaths::system(options.data_dir.as_deref())?;
    let lease = paths.repository()?.coordinator.enter()?;
    let mut output = run_with_paths(&options, &paths)?;
    output._lifecycle = Some(lease);
    Ok(output)
}
/// Explicit paths are injected by tests; the production entry never accepts a
/// coordination-root flag or mutates HOME to obtain isolated locks.
pub fn run_with_paths(options: &Options, paths: &RuntimePaths) -> Result<Output> {
    let repo = paths.repository()?;
    let _lease = repo.coordinator.enter()?;
    let project = if let Some(project) = &options.project {
        Some(project.clone())
    } else if options.scope == "project" {
        Some(std::env::current_dir()?)
    } else {
        None
    };
    let mut context = TargetContext::new(&paths.home, project.as_deref())?;
    if let Some(path) = &options.target {
        context = context.with_custom(&options.tools[0], &options.scope, path)?;
    }
    let ids = if options.target.is_some() {
        vec!["custom-explicit".into()]
    } else {
        options
            .tools
            .iter()
            .map(|t| format!("{t}-{}", options.scope))
            .collect()
    };
    let targets = context.selected(&ids)?;
    let roots = options
        .skills_root
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    if options.command == "tools" {
        return format_output(
            json!({"tools":context.targets()?,"aliases":{"chatgpt":"codex","claude":"claude-code"}}),
            options.json,
            0,
        );
    }
    if options.command == "pending" {
        return format_output(
            serde_json::to_value(installers::recovery_items(&repo, &context)?).unwrap(),
            options.json,
            0,
        );
    }
    if options.command == "recover" {
        if options.selected.len() != 1 {
            return Err(SkillsError::invalid("recover requires one operation ID"));
        }
        let mut operation = Operation::begin(&repo, "managed-installations", "recover")?;
        let outcome = installers::recover_one(&repo, &context, &options.selected[0]);
        operation.finish(outcome.as_ref().map(|_| ()).map_err(Clone::clone))?;
        return format_output(
            json!({"operationId":options.selected[0],"outcome":outcome?}),
            options.json,
            0,
        );
    }
    if options.command == "uninstall" {
        return uninstall(&repo, &context, options, &ids, &roots);
    }
    let (mut all, issues) = catalog_items(&repo, options)?;
    if options.skills_root.is_none() && matches!(options.command.as_str(), "status" | "doctor") {
        for entry in installers::inventory(&repo, &context)? {
            let Some(owned) = entry.installation else {
                continue;
            };
            if !ids.contains(&entry.target.id)
                || all.iter().any(|s| s.source_skill_id == owned.skill_id)
            {
                continue;
            }
            let metadata = catalog::detail(&owned.source_id, &entry.destination, "SKILL.md")
                .map(|d| d.manifest)
                .unwrap_or_default();
            let category = catalog::activation_category(&owned.relative_path, &metadata);
            if options.category.as_ref().is_some_and(|c| c != &category) {
                continue;
            }
            all.push(Item {
                id: owned.install_name.clone(),
                path: owned.relative_path.trim_end_matches("/SKILL.md").into(),
                category,
                title: owned.install_name.clone(),
                summary: "Installed skill (source unavailable)".into(),
                manual_group: String::new(),
                source_skill_id: owned.skill_id.clone(),
                source_id: owned.source_id,
                has_injection: owned
                    .managed_resources
                    .iter()
                    .any(|r| !r.marker.starts_with("slash:")),
                aliases: vec![owned.install_name, owned.skill_id, owned.installation_id],
            });
        }
    }
    let chosen = select(&all, &options.selected)?;
    if options.command == "list" {
        return format_output(
            json!({"skills_root":roots,"category_filter":options.category,"skills":chosen.iter().map(|i|&i.id).collect::<Vec<_>>(),"count":chosen.len(),"items":chosen,"issues":issues}),
            options.json,
            0,
        );
    }
    if chosen.is_empty() {
        return Err(SkillsError::invalid(
            "No matching skills; add a Loom source or specify --skills-root",
        ));
    }
    if options.command == "detail" {
        if chosen.len() != 1 {
            return Err(SkillsError::invalid("detail requires one skill"));
        }
        let selected = &chosen[0];
        let content = if let Some(root) = &options.skills_root {
            catalog::detail(
                &selected.source_id,
                root,
                &format!(
                    "{}{}SKILL.md",
                    selected.path,
                    if selected.path.is_empty() { "" } else { "/" }
                ),
            )?
            .content
        } else {
            repo.detail(&selected.source_skill_id)?.content
        };
        return Ok(Output {
            text: if options.json {
                format!("{}\n", json!({"skill":selected.id,"content":content}))
            } else {
                content
            },
            exit_code: 0,
            _lifecycle: None,
        });
    }
    let mut names = BTreeSet::new();
    for selected in &chosen {
        if !names.insert(&selected.id) {
            return Err(SkillsError::invalid("Selected sources contain duplicate skill names; select an unambiguous source_skill_id"));
        }
    }
    if options.command == "install" {
        if let Some(root) = &options.skills_root {
            repo.add_local(&root.canonicalize()?)?;
        }
    }
    let mut results = Vec::new();
    let mut plans = Vec::new();
    let mut failed = false;
    for selected in chosen {
        // An explicit root is read-only for listing/diagnosis. Installation
        // registers it so saved source identities are shared with the desktop.
        if options.command == "install" {
            let mut operation = Operation::begin(&repo, &selected.source_id, "install")?;
            let current = installers::statuses(&repo, &context, &selected.source_skill_id)?;
            let resolved = installers::resolve(&repo, &selected.source_skill_id)?;
            let selected_status = current
                .iter()
                .filter(|s| ids.contains(&s.target.id))
                .collect::<Vec<_>>();
            let refresh_live = selected_status.iter().any(|s| s.installation.is_some())
                && selected_status.iter().all(|s| {
                    (s.state == "absent" && s.installation.is_none())
                        || s.installation.as_ref().is_some_and(|i| {
                            i.mode == InstallMode::Symlink
                                && i.source_dir == resolved.directory
                                && s.state == "installed"
                        })
                });
            let policy = if options.force || refresh_live {
                ConflictPolicy::Overwrite
            } else {
                ConflictPolicy::Skip
            };
            let input = InstallRequest {
                skill_id: selected.source_skill_id.clone(),
                target_ids: ids.clone(),
                mode: options.mode,
                conflict_policy: policy,
                project_path: project.as_ref().map(|p| p.to_string_lossy().into_owned()),
                with_slash_commands: options.slash,
            };
            let plan = match installers::plan_install(&repo, &context, &input) {
                Ok(plan) => plan,
                Err(error) => {
                    operation.finish(Err(error.clone()))?;
                    return Err(error);
                }
            };
            if options.preview {
                let blocked = plan.items.iter().any(|i| i.decision == "blocked");
                failed |= blocked;
                plans.push(serde_json::to_value(plan).unwrap());
                operation.finish(if blocked {
                    Err(SkillsError::new(
                        ErrorCode::Conflict,
                        "Preview contains blocked targets",
                    ))
                } else {
                    Ok(())
                })?;
                continue;
            }
            let receipt = installers::apply(&repo, &context, &plan.plan_id, &mut operation);
            finish_receipt(&mut operation, &receipt)?;
            for entry in receipt?.items {
                let status = match entry.status.as_str() {
                    "skipped" => "skip",
                    s => s,
                };
                failed |= !["installed", "skip"].contains(&status);
                results.push(json!({"skill":selected.id,"status":status,"reason":entry.message,"target_id":entry.target_id,"destination":entry.destination}));
            }
        } else {
            let inventory = installers::inventory(&repo, &context)?;
            for target in &targets {
                let entry = inventory.iter().find(|s| {
                    s.target.id == target.id
                        && s.installation
                            .as_ref()
                            .is_some_and(|i| i.skill_id == selected.source_skill_id)
                });
                if let Some(problem) = inventory.iter().find(|s| {
                    s.target.id == target.id && s.installation.is_none() && s.message.is_some()
                }) {
                    failed = true;
                    results.push(json!({"skill":selected.id,"status":problem.state,"reason":problem.message,"target_id":target.id}));
                    continue;
                }
                let destination = entry
                    .map(|s| s.destination.clone())
                    .unwrap_or_else(|| target.skills_dir.join(&selected.id));
                let state = package::state(&destination)?;
                if options.command == "status" {
                    let status = match state {
                        EntryState::Missing => "missing",
                        EntryState::Symlink { .. } => "symlink",
                        EntryState::Directory { .. } => "directory",
                        EntryState::File { .. } => "file",
                    };
                    results.push(json!({"skill":selected.id,"status":status,"reason":format!("managed={}",entry.is_some()),"target_id":target.id,"installation":entry.and_then(|e|e.installation.as_ref()),"diagnostics":entry.map(|e|&e.instruction_checks)}));
                } else {
                    let checks = entry
                        .map(|s| s.instruction_checks.as_slice())
                        .unwrap_or_default();
                    let status = if checks.iter().any(|c| c.state == "malformed") {
                        "malformed"
                    } else if checks.iter().any(|c| c.state == "stale") {
                        "stale"
                    } else if checks.iter().any(|c| c.state == "missing") {
                        "missing"
                    } else if !checks.is_empty() {
                        "current"
                    } else if ["auto", "global"].contains(&selected.category.as_str())
                        && selected.has_injection
                        && entry.is_none()
                    {
                        "missing"
                    } else {
                        "not-applicable"
                    };
                    failed |= !["current", "not-applicable"].contains(&status);
                    results.push(json!({"skill":selected.id,"status":status,"target_id":target.id,"diagnostics":checks}));
                }
            }
        }
    }
    format_output(
        json!({"action":options.command,"skills_root":roots,"target_dir":targets[0].skills_dir,"target_dirs":targets.iter().map(|t|&t.skills_dir).collect::<Vec<_>>(),"results":results,"plans":plans}),
        options.json,
        if failed { 1 } else { 0 },
    )
}
fn uninstall(
    repo: &Repository,
    context: &TargetContext,
    options: &Options,
    ids: &[String],
    roots: &str,
) -> Result<Output> {
    let inventory = installers::inventory(repo, context)?;
    let source_id = options
        .skills_root
        .as_ref()
        .map(|p| {
            files::resource_path(p).map(|p| {
                format!(
                    "src-{}",
                    &files::digest(p.to_string_lossy().as_bytes())[..32]
                )
            })
        })
        .transpose()?;
    let available = inventory
        .iter()
        .filter(|s| {
            ids.contains(&s.target.id)
                && s.installation
                    .as_ref()
                    .is_some_and(|i| source_id.as_ref().is_none_or(|id| id == &i.source_id))
        })
        .collect::<Vec<_>>();
    let mut selected = Vec::new();
    for entry in available {
        let owned = entry.installation.as_ref().unwrap();
        let metadata = catalog::detail(&owned.source_id, &entry.destination, "SKILL.md")
            .map(|d| d.manifest)
            .unwrap_or_default();
        if options
            .category
            .as_ref()
            .is_some_and(|c| c != &catalog::activation_category(&owned.relative_path, &metadata))
        {
            continue;
        }
        if options.selected.is_empty()
            || options.selected.iter().any(|name| {
                name == &owned.install_name
                    || name == &owned.skill_id
                    || name == &owned.installation_id
            })
        {
            selected.push(entry);
        }
    }
    let mut results = Vec::new();
    for name in &options.selected {
        if selected.iter().any(|e| {
            let i = e.installation.as_ref().unwrap();
            name == &i.install_name || name == &i.skill_id || name == &i.installation_id
        }) {
            continue;
        }
        installers::validate_name(name)?;
        for target in context.selected(ids)? {
            if inventory.iter().any(|e| {
                e.target.id == target.id && e.installation.is_none() && e.message.is_some()
            }) || package::state(&target.skills_dir.join(name))? != EntryState::Missing
            {
                return Err(SkillsError::new(ErrorCode::Conflict,format!("No verified managed installation for {name}; unmanaged content is preserved")));
            }
            results.push(json!({"skill":name,"status":"missing","reason":"Destination does not exist","target_id":target.id}));
        }
    }
    if selected.is_empty() && results.is_empty() {
        return Err(SkillsError::invalid("No matching managed installations"));
    }
    let mut plans = Vec::new();
    let mut failed = false;
    // Service batches are bounded; each chunk shares the same transaction engine.
    for chunk in selected.chunks(12) {
        let mut operation = Operation::begin(repo, "managed-installations", "uninstall")?;
        let plan_result = installers::plan_uninstall(
            repo,
            context,
            &UninstallRequest {
                installation_ids: chunk
                    .iter()
                    .map(|e| e.installation.as_ref().unwrap().installation_id.clone())
                    .collect(),
                project_path: context.project().map(|p| p.to_string_lossy().into_owned()),
            },
        );
        let plan = match plan_result {
            Ok(plan) => plan,
            Err(error) => {
                operation.finish(Err(error.clone()))?;
                return Err(error);
            }
        };
        if options.preview {
            let blocked = plan.items.iter().any(|i| i.decision == "blocked");
            failed |= blocked;
            plans.push(serde_json::to_value(plan).unwrap());
            operation.finish(if blocked {
                Err(SkillsError::new(
                    ErrorCode::Conflict,
                    "Preview contains blocked targets",
                ))
            } else {
                Ok(())
            })?;
            continue;
        }
        let receipt = installers::apply(repo, context, &plan.plan_id, &mut operation);
        finish_receipt(&mut operation, &receipt)?;
        for item in receipt?.items {
            let name = item
                .destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let status = if item.status == "uninstalled" {
                "removed"
            } else {
                item.status.as_str()
            };
            failed |= status != "removed";
            results.push(json!({"skill":name,"status":status,"reason":item.message,"target_id":item.target_id}));
        }
    }
    format_output(
        json!({"action":"uninstall","skills_root":roots,"target_dir":context.selected(ids)?[0].skills_dir,"target_dirs":context.selected(ids)?.iter().map(|t|&t.skills_dir).collect::<Vec<_>>(),"results":results,"plans":plans}),
        options.json,
        if failed { 1 } else { 0 },
    )
}

fn finish_receipt(
    operation: &mut Operation,
    receipt: &Result<installers::InstallReceipt>,
) -> Result<()> {
    let outcome = match receipt {
        Err(error) => Err(error.clone()),
        Ok(receipt)
            if receipt
                .items
                .iter()
                .any(|i| !["installed", "uninstalled", "skipped"].contains(&i.status.as_str())) =>
        {
            Err(SkillsError::new(
                ErrorCode::Conflict,
                "One or more CLI installation targets failed; inspect item results",
            ))
        }
        Ok(_) => Ok(()),
    };
    operation.finish(outcome)
}
