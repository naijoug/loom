//! fzf supplies selections only. Numeric row identities are validated locally;
//! source text never becomes a shell command, fzf binding or filesystem action.
use super::*;
use crate::skills::operations::ExecutionRecord;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::Duration,
};

pub(super) trait Terminal {
    fn ensure_fzf(&mut self, allow_install: bool) -> Result<bool>;
    fn choose(
        &mut self,
        prompt: &str,
        header: &str,
        rows: &[String],
        multi: bool,
        all: bool,
    ) -> Result<Option<Vec<usize>>>;
    fn input(&mut self, prompt: &str) -> Result<Option<String>>;
    fn show(&mut self, text: &str) -> Result<()>;
}
pub(super) struct SystemTerminal {
    repository: Repository,
    search: Vec<PathBuf>,
    fzf: Option<PathBuf>,
}
impl SystemTerminal {
    pub(super) fn new(repository: Repository) -> Self {
        Self {
            repository,
            search: std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .filter(|p| p.is_absolute())
                .collect(),
            fzf: None,
        }
    }
    fn executable(&self, name: &str) -> Option<PathBuf> {
        self.search.iter().map(|dir| dir.join(name)).find(|p| {
            let Ok(meta) = std::fs::metadata(p) else {
                return false;
            };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                meta.is_file() && meta.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                meta.is_file()
            }
        })
    }
    fn picker_command(
        &self,
        prompt: &str,
        header: &str,
        multi: bool,
        all: bool,
        query: bool,
    ) -> Result<Command> {
        let fzf = self
            .fzf
            .as_ref()
            .ok_or_else(|| SkillsError::invalid("fzf is unavailable"))?;
        let mut command = Command::new(fzf);
        command
            .args([
                "--height=75%",
                "--layout=reverse",
                "--border",
                "--info=inline",
                "--delimiter=\t",
                "--with-nth=2..",
                "--prompt",
                prompt,
                "--header",
                header,
            ])
            .env_remove("FZF_DEFAULT_OPTS")
            .env_remove("FZF_DEFAULT_OPTS_FILE")
            .env_remove("FZF_DEFAULT_COMMAND")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if multi {
            command.arg("--multi");
        }
        if all {
            command.args(["--bind", "start:select-all"]);
        }
        if query {
            command.args(["--print-query", "--phony"]);
        }
        Ok(command)
    }
    fn select_raw(
        &self,
        prompt: &str,
        header: &str,
        rows: &[String],
        multi: bool,
        all: bool,
        query: bool,
    ) -> Result<Option<String>> {
        let mut command = self.picker_command(prompt, header, multi, all, query)?;
        let input = rows
            .iter()
            .enumerate()
            .map(|(i, row)| format!("{i}\t{}\n", single_line(row)))
            .collect::<String>();
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().unwrap();
        let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
        let output = child.wait_with_output()?;
        let write_result = writer
            .join()
            .map_err(|_| SkillsError::invalid("fzf input worker failed"))?;
        if matches!(output.status.code(), Some(1 | 130)) {
            return Ok(None);
        }
        if !output.status.success() {
            return Err(SkillsError::invalid("fzf exited with an error"));
        }
        write_result?;
        Ok(Some(String::from_utf8(output.stdout).map_err(|_| {
            SkillsError::invalid("fzf returned invalid UTF-8")
        })?))
    }
}
impl Terminal for SystemTerminal {
    fn ensure_fzf(&mut self, allow_install: bool) -> Result<bool> {
        self.fzf = self.executable("fzf");
        if self.fzf.is_some() || !allow_install {
            return Ok(self.fzf.is_some());
        }
        let brew = self.executable("brew").ok_or_else(|| {
            SkillsError::invalid("fzf and Homebrew were not found; install fzf manually")
        })?;
        self.show("Installing fzf through the explicitly requested Homebrew operation…\n")?;
        install_fzf(&self.repository, &brew, Duration::from_secs(600))?;
        self.fzf = self.executable("fzf");
        if self.fzf.is_none() {
            return Err(SkillsError::invalid(
                "Homebrew finished but fzf is not on PATH; update PATH and retry",
            ));
        }
        Ok(true)
    }
    fn choose(
        &mut self,
        prompt: &str,
        header: &str,
        rows: &[String],
        multi: bool,
        all: bool,
    ) -> Result<Option<Vec<usize>>> {
        let Some(output) = self.select_raw(prompt, header, rows, multi, all, false)? else {
            return Ok(None);
        };
        let mut indices = Vec::new();
        for line in output.lines() {
            let index = line
                .split('\t')
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|i| *i < rows.len())
                .ok_or_else(|| SkillsError::invalid("fzf selected an unknown row"))?;
            if !indices.contains(&index) {
                indices.push(index);
            }
        }
        if (!multi && indices.len() != 1) || indices.is_empty() {
            return Err(SkillsError::invalid("fzf returned an invalid selection"));
        }
        Ok(Some(indices))
    }
    fn input(&mut self, prompt: &str) -> Result<Option<String>> {
        Ok(self
            .select_raw(
                prompt,
                "Type the target directory, then press Enter",
                &["Use this path".into()],
                false,
                false,
                true,
            )?
            .and_then(|s| {
                s.lines()
                    .next()
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_owned)
            }))
    }
    fn show(&mut self, text: &str) -> Result<()> {
        std::io::stderr()
            .write_all(safe_text(text).as_bytes())
            .map_err(Into::into)
    }
}
fn single_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn safe_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(*c, '\n' | '\t'))
        .collect()
}
fn output(text: String, json_mode: bool, code: i32) -> Output {
    Output {
        text: if json_mode { text } else { safe_text(&text) },
        exit_code: code,
        _lifecycle: None,
    }
}
fn one(ui: &mut impl Terminal, prompt: &str, header: &str, rows: &[&str]) -> Result<Option<usize>> {
    Ok(ui
        .choose(
            prompt,
            header,
            &rows.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            false,
            false,
        )?
        .map(|v| v[0]))
}

pub(super) fn run(
    options: &Options,
    paths: &RuntimePaths,
    ui: &mut impl Terminal,
) -> Result<Output> {
    if options.command == "ng" {
        ng(options, paths, ui)
    } else {
        tui(options, paths, ui)
    }
}
fn ng(options: &Options, paths: &RuntimePaths, ui: &mut impl Terminal) -> Result<Output> {
    let repo = paths.repository()?;
    let (items, _) = catalog_items(&repo, options)?;
    let manual = items
        .iter()
        .filter(|s| s.category == "manual")
        .cloned()
        .collect::<Vec<_>>();
    if options.selected.first().is_some_and(|s| s == "list") {
        return manual_list(&manual, options.json);
    }
    if options.selected.as_slice() == ["plan"] {
        let notes = [
            (
                "global",
                "Install globally; its managed instructions apply after installation.",
            ),
            ("cron", "Connect to an external recurring automation job."),
            ("auto", "Install to enable its managed instructions."),
            (
                "manual",
                "Select explicitly with ng or the installed slash command.",
            ),
        ];
        if options.json {
            return format_output(json!({"categories":notes,"items":items}), true, 0);
        }
        let mut text = String::new();
        for (category, note) in notes {
            text.push_str(&format!("[{category}] {note}\n"));
            for item in items.iter().filter(|i| i.category == category) {
                text.push_str(&format!("- {}: {}\n", item.id, item.title));
            }
            text.push('\n');
        }
        return Ok(output(text, false, 0));
    }
    let (selected, extra) = if options.selected.is_empty() {
        if !ui.ensure_fzf(options.auto_install_fzf)? {
            return manual_list(&manual, options.json);
        }
        if manual.is_empty() {
            return Err(SkillsError::invalid(
                "No manual skills in the selected source",
            ));
        }
        let rows = manual
            .iter()
            .map(|s| format!("{} | {} | {}", s.manual_group, s.id, s.title))
            .collect::<Vec<_>>();
        let Some(choice) = ui.choose(
            "ng>manual> ",
            "Choose a skill to preview its explicit trigger plan",
            &rows,
            false,
            false,
        )?
        else {
            return Ok(output(String::new(), options.json, 1));
        };
        (manual[choice[0]].clone(), String::new())
    } else {
        let args = &options.selected;
        let combined = if args.len() > 1 {
            Some(format!("ng-{}-{}", args[0], args[1]))
        } else {
            None
        };
        let grouped = combined
            .as_ref()
            .filter(|name| items.iter().any(|s| s.aliases.contains(name)));
        if args[0] == "plan" && args.len() > 1 && grouped.is_none() {
            return Err(SkillsError::invalid(format!(
                "No skill named ng-plan-{}",
                args[1]
            )));
        }
        let (name, consumed) = if let Some(name) = grouped {
            (name.as_str(), 2)
        } else {
            (args[0].as_str(), 1)
        };
        (
            select(&items, &[name.into()])?.remove(0),
            args[consumed..].join(" "),
        )
    };
    let suggestion = match selected.category.as_str() {
        "manual" => {
            if extra.is_empty() {
                format!("Use the `{}` skill for this task.", selected.id)
            } else {
                format!("Use the `{}` skill for this task: {extra}", selected.id)
            }
        }
        "global" => "Install globally; do not invoke this passive instruction manually.".into(),
        "auto" => "Install to enable this passive helper; do not invoke it manually.".into(),
        "cron" => {
            "Connect this skill to external scheduled automation; do not invoke it manually.".into()
        }
        _ => return Err(SkillsError::invalid("Unsupported activation category")),
    };
    if options.json {
        return format_output(
            json!({"skill":selected,"suggested_prompt":suggestion,"executes_agent":false}),
            true,
            0,
        );
    }
    Ok(output(format!("Skill: {}\nTitle: {}\nCategory: {}\nDirectory Group: {}\nSummary: {}\n\nTrigger plan:\n{}{}\n",selected.id,selected.title,selected.category,selected.manual_group,selected.summary,if selected.category=="manual"{format!("- Slash command (after installing its wrapper): /{} <args>\n- Suggested prompt: ",selected.id)}else{String::new()},suggestion),false,0))
}
fn manual_list(items: &[Item], as_json: bool) -> Result<Output> {
    if as_json {
        return format_output(json!({"items":items,"count":items.len()}), true, 0);
    }
    let mut text = String::new();
    let mut discovered = BTreeSet::new();
    for item in items {
        discovered.insert(item.manual_group.clone());
    }
    let mut groups = Vec::new();
    for group in ["plan", "review", "tool", "growth", "meta"] {
        if discovered.remove(group) {
            groups.push(group.to_string());
        }
    }
    groups.extend(discovered);
    for group in groups {
        text.push_str(&format!(
            "# {}\n",
            if group.is_empty() { "other" } else { &group }
        ));
        for item in items.iter().filter(|i| i.manual_group == group) {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                group, item.id, item.title, item.summary, item.path
            ));
        }
    }
    Ok(output(text, false, 0))
}

fn tui(options: &Options, paths: &RuntimePaths, ui: &mut impl Terminal) -> Result<Output> {
    if !ui.ensure_fzf(options.auto_install_fzf)? {
        return Err(SkillsError::invalid(
            "fzf was not found; install it manually or explicitly use --auto-install-fzf",
        ));
    }
    let repo = paths.repository()?;
    let (mut items, _) = catalog_items(&repo, options)?;
    if items.is_empty() {
        return Err(SkillsError::invalid(
            "No skills found; add a Loom source or pass --skills-root",
        ));
    }
    let mut current = options.clone();
    current.command = "install".into();
    current.json = true;
    current.preview = false;
    if current.project.is_none() {
        current.project = Some(std::env::current_dir()?);
    }
    current.selected = select(&items, &options.selected)?
        .iter()
        .map(|i| i.source_skill_id.clone())
        .collect();
    let mut completed = 0;
    loop {
        let header=format!("action={} | tool={} | scope={} | mode={:?} | replace={} | wrappers={} | selected={}/{}\nproject={} | custom={}",current.command,current.tools.join(","),current.scope,current.mode,current.force,current.slash,current.selected.len(),items.len(),current.project.as_ref().map(|p|p.display().to_string()).unwrap_or_default(),current.target.as_ref().map(|p|p.display().to_string()).unwrap_or_default());
        let Some(choice) = one(
            ui,
            "tui> ",
            &header,
            &[
                "execute now",
                "preview selected skills",
                "change action",
                "change target",
                "change policy",
                "select skills",
                "select all skills",
                "clear selected skills",
                "reload library",
                "quit",
            ],
        )?
        else {
            return Ok(output(String::new(), options.json, 1));
        };
        match choice {
            0 => {
                if current.selected.is_empty() {
                    ui.show("No skills selected.\n")?;
                    continue;
                }
                let result = (|| -> Result<()> {
                    if matches!(current.command.as_str(), "install" | "uninstall") {
                        let mut preview = current.clone();
                        preview.preview = true;
                        let response = run_with_paths(&preview, paths)?;
                        let value: Value = serde_json::from_str(&response.text)
                            .map_err(|_| SkillsError::invalid("Invalid preview response"))?;
                        let plans: Vec<installers::InstallPlan> =
                            serde_json::from_value(value["plans"].clone())
                                .map_err(|_| SkillsError::invalid("Missing reviewed plans"))?;
                        ui.show(&review_text(&plans))?;
                        if !plans.iter().any(|p| {
                            p.items
                                .iter()
                                .any(|i| !matches!(i.decision.as_str(), "blocked" | "skip"))
                        }) {
                            ui.show("No applicable changes; adjust the selection or policy.\n")?;
                            return Ok(());
                        }
                        if one(
                            ui,
                            "apply> ",
                            "Apply only these reviewed plans; changed targets will be rejected",
                            &["Back", "Apply reviewed changes"],
                        )? != Some(1)
                        {
                            return Ok(());
                        }
                        let receipts = apply_reviewed(paths, &current, &plans)?;
                        for entry in receipts {
                            match entry.result {
                                Ok(receipt) => {
                                    ui.show(&format!(
                                        "{}\n",
                                        serde_json::to_string_pretty(&receipt).unwrap()
                                    ))?;
                                    if receipt.items.iter().all(|i| {
                                        ["installed", "uninstalled", "skipped"]
                                            .contains(&i.status.as_str())
                                    }) && receipt.items.iter().any(|i| i.status != "skipped")
                                    {
                                        completed += 1;
                                    }
                                }
                                Err(error) => ui.show(&format!(
                                    "Plan {} failed: {}\n",
                                    entry.plan_id, error.message
                                ))?,
                            }
                        }
                    } else {
                        let response = run_with_paths(&current, paths)?;
                        ui.show(&response.text)?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    ui.show(&format!("Error: {}\n", error.message))?;
                }
            }
            1 => {
                for selected in &current.selected {
                    let mut detail = current.clone();
                    detail.command = "detail".into();
                    detail.selected = vec![selected.clone()];
                    detail.json = false;
                    match run_with_paths(&detail, paths) {
                        Ok(result) => ui.show(&format!("\n{}\n", result.text))?,
                        Err(error) => ui.show(&format!("{}\n", error.message))?,
                    }
                }
            }
            2 => {
                if let Some(index) = one(
                    ui,
                    "action> ",
                    "Choose an action",
                    &["install", "uninstall", "status", "doctor"],
                )? {
                    current.command = ["install", "uninstall", "status", "doctor"][index].into();
                }
            }
            3 => {
                let rows = [
                    "codex/global",
                    "codex/project",
                    "claude/global",
                    "claude/project",
                    "amp/global",
                    "amp/project",
                    "codex+amp/project",
                    "custom/path",
                ];
                if let Some(index) = one(ui, "target> ", "Choose the scope explicitly", &rows)? {
                    if index == 7 {
                        if let Some(path) = ui.input("target directory> ")? {
                            current.tools = vec!["custom".into()];
                            current.scope = "global".into();
                            current.target = Some(absolute(&path)?);
                        }
                    } else {
                        let (tools, scope) = rows[index].split_once('/').unwrap();
                        current.tools = tools
                            .split('+')
                            .map(|s| {
                                if s == "claude" {
                                    "claude-code".into()
                                } else {
                                    s.into()
                                }
                            })
                            .collect();
                        current.scope = scope.into();
                        current.target = None;
                    }
                }
            }
            4 => {
                if current.command == "install" {
                    if let Some(index) = one(
                        ui,
                        "policy> ",
                        "Replacement only applies to verified managed content",
                        &["safe-symlink", "safe-copy", "force-symlink", "force-copy"],
                    )? {
                        let wrappers = one(
                            ui,
                            "wrappers> ",
                            "Manual Codex/Claude command wrappers",
                            &["off", "on"],
                        )?;
                        if let Some(wrappers) = wrappers {
                            current.mode = if index % 2 == 0 {
                                InstallMode::Symlink
                            } else {
                                InstallMode::Copy
                            };
                            current.force = index >= 2;
                            current.slash = wrappers == 1;
                        }
                    }
                } else {
                    ui.show("Uninstall always protects unowned or user-modified content; read actions have no write policy.\n")?;
                }
            }
            5 => {
                let rows = items
                    .iter()
                    .map(|i| format!("{} | {} | {}", i.id, i.category, i.title))
                    .collect::<Vec<_>>();
                if let Some(indices) = ui.choose(
                    "skills> ",
                    "Tab to mark skills; Enter to confirm",
                    &rows,
                    true,
                    current.selected.len() == items.len(),
                )? {
                    current.selected = indices
                        .iter()
                        .map(|i| items[*i].source_skill_id.clone())
                        .collect();
                }
            }
            6 => current.selected = items.iter().map(|i| i.source_skill_id.clone()).collect(),
            7 => current.selected.clear(),
            8 => {
                items = catalog_items(&repo, options)?.0;
                current
                    .selected
                    .retain(|id| items.iter().any(|i| &i.source_skill_id == id));
            }
            9 => {
                return Ok(if options.json {
                    output(
                        format!("{}\n", json!({"action":"tui","applied_plans":completed})),
                        true,
                        0,
                    )
                } else {
                    output(
                        format!("TUI closed; applied {completed} plan(s).\n"),
                        false,
                        0,
                    )
                })
            }
            _ => return Err(SkillsError::invalid("Unknown menu choice")),
        }
    }
}
fn review_text(plans: &[installers::InstallPlan]) -> String {
    let mut text = String::new();
    for plan in plans {
        text.push_str(&format!(
            "\nPlan {} ({:?}, {:?}){}\n",
            plan.plan_id,
            plan.action,
            plan.mode,
            if plan.coordinated {
                " — shared instructions: apply together"
            } else {
                ""
            }
        ));
        for item in &plan.items {
            text.push_str(&format!(
                "{}: {} → {}\n",
                item.target.label,
                item.decision,
                item.destination.display()
            ));
            for resource in &item.resource_changes {
                if resource.path.extension().is_some_and(|e| e == "md") {
                    text.push_str(&format!("  managed file: {}\n", resource.path.display()));
                }
            }
            if let Some(message) = &item.message {
                text.push_str(&format!("  {message}\n"));
            }
        }
    }
    text
}
fn apply_reviewed(
    paths: &RuntimePaths,
    options: &Options,
    plans: &[installers::InstallPlan],
) -> Result<Vec<installers::ReviewedResult>> {
    let repo = paths.repository()?;
    let mut context = TargetContext::new(&paths.home, options.project.as_deref())?;
    if let Some(path) = &options.target {
        context = context.with_custom(&options.tools[0], &options.scope, path)?;
    }
    Ok(installers::apply_reviewed_batch(
        &repo,
        &context,
        &plans.iter().map(|p| p.plan_id.clone()).collect::<Vec<_>>(),
    ))
}

fn install_fzf(repo: &Repository, brew: &Path, timeout: Duration) -> Result<()> {
    use crate::process_supervisor::{self, ProcessKind, ProcessMetadata, ProcessOwner};
    let mut operation = Operation::begin(repo, "cli-dependencies", "install-fzf")?;
    operation.record.executions.push(ExecutionRecord {
        program: brew.display().to_string(),
        args: vec!["install".into(), "fzf".into()],
        cwd: std::env::current_dir()?,
        started_at_ms: crate::models::now_ms(),
        finished_at_ms: None,
        exit_code: None,
        error: None,
    });
    operation.stage("installing-fzf")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result: Result<()> = runtime.block_on(async {
        let mut guard = process_supervisor::supervisor().begin_operation().map_err(SkillsError::invalid)?;
        let mut command = tokio::process::Command::new(brew);
        command.args(["install", "fzf"]).env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit()).kill_on_drop(true);
        process_supervisor::configure_process_group(&mut command);
        let mut child = command.spawn()?;
        let pid = child.id().ok_or_else(|| SkillsError::invalid("Homebrew process has no ID"))?;
        let run = crate::models::IdGenerator::default().next("skills-dependency");
        let mut metadata = ProcessMetadata::unscoped(&run, ProcessKind::Skills, pid, Some(timeout.as_millis() as u64));
        metadata.owner = ProcessOwner::Skills { operation_id: operation.record.operation_id.clone() };
        if let Err(error) = guard.register(metadata) {
            let _ = process_supervisor::force_terminate_process_group(pid);
            let _ = child.kill().await;
            return Err(SkillsError::invalid(error));
        }
        let began = tokio::time::Instant::now();
        let result = loop {
            tokio::select! {
                result = child.wait() => break result.map_err(SkillsError::from),
                signal = tokio::signal::ctrl_c() => {
                    break Err(match signal {
                        Ok(()) => SkillsError::new(ErrorCode::Cancelled, "fzf installation cancelled"),
                        Err(error) => SkillsError::from(error),
                    });
                },
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if let Err(error) = operation.check_cancelled() { break Err(error); }
                    if began.elapsed() > timeout {
                        break Err(SkillsError::new(ErrorCode::Timeout, "fzf installation timed out"));
                    }
                }
            }
        };
        let cleanup = process_supervisor::force_terminate_process_group(pid);
        if result.is_err() {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
        process_supervisor::supervisor().complete(&run);
        cleanup.map_err(SkillsError::invalid)?;
        operation.record.executions.last_mut().unwrap().exit_code = result.as_ref().ok().and_then(|status| status.code());
        if !result?.success() { return Err(SkillsError::new(ErrorCode::SourceUnavailable, "Homebrew could not install fzf")); }
        Ok(())
    });
    let execution = operation.record.executions.last_mut().unwrap();
    execution.finished_at_ms = Some(crate::models::now_ms());
    execution.error = result.as_ref().err().map(|error| error.message.clone());
    operation.finish(result.clone())?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::tests::Fixture;
    use std::collections::VecDeque;
    struct Setup {
        fixture: Fixture,
        paths: RuntimePaths,
    }
    impl Setup {
        fn new() -> Self {
            let fixture = Fixture::new();
            for path in ["home", "project", "source/manual/plan/review"] {
                std::fs::create_dir_all(fixture.0.join(path)).unwrap();
            }
            std::fs::write(fixture.0.join("source/manual/plan/review/SKILL.md"),"---\nname: ng-plan-review\ndescription: Review a plan fixture.\n---\n# Review\nDo not execute source code.\n").unwrap();
            let paths = RuntimePaths {
                home: fixture.0.join("home"),
                app_data: fixture.0.join("app"),
                library: fixture.0.join("data"),
            };
            Self { fixture, paths }
        }
        fn options(&self, command: &str, args: &[&str]) -> Options {
            let mut values = vec![
                command.into(),
                "--skills-root".into(),
                self.fixture.0.join("source").display().to_string(),
                "--project-root".into(),
                self.fixture.0.join("project").display().to_string(),
                "--scope".into(),
                "project".into(),
            ];
            values.extend(args.iter().map(|s| s.to_string()));
            Options::parse(&values).unwrap()
        }
    }
    struct Scripted {
        choices: VecDeque<Option<Vec<usize>>>,
        shown: String,
        ensure: Vec<bool>,
        available: bool,
        edit_on_apply: Option<PathBuf>,
    }
    impl Scripted {
        fn new(choices: &[usize]) -> Self {
            Self {
                choices: choices.iter().map(|i| Some(vec![*i])).collect(),
                shown: String::new(),
                ensure: Vec::new(),
                available: true,
                edit_on_apply: None,
            }
        }
    }
    impl Terminal for Scripted {
        fn ensure_fzf(&mut self, allow: bool) -> Result<bool> {
            self.ensure.push(allow);
            Ok(self.available)
        }
        fn choose(
            &mut self,
            prompt: &str,
            _header: &str,
            rows: &[String],
            _multi: bool,
            _all: bool,
        ) -> Result<Option<Vec<usize>>> {
            if prompt == "apply> " {
                if let Some(path) = self.edit_on_apply.take() {
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, "User change after preview")?;
                }
            }
            let choice = self.choices.pop_front().expect("Unexpected extra prompt");
            if let Some(indices) = &choice {
                assert!(indices.iter().all(|i| *i < rows.len()));
            }
            Ok(choice)
        }
        fn input(&mut self, _: &str) -> Result<Option<String>> {
            Ok(None)
        }
        fn show(&mut self, text: &str) -> Result<()> {
            self.shown.push_str(text);
            Ok(())
        }
    }
    #[test]
    fn ng_lists_groups_resolves_group_verbs_and_never_executes_extra_text() {
        let s = Setup::new();
        let mut ui = Scripted::new(&[]);
        let list = run(&s.options("ng", &["list"]), &s.paths, &mut ui).unwrap();
        assert!(list.text.contains("# plan"));
        assert!(list.text.contains("ng-plan-review"));
        let marker = s.fixture.0.join("not-executed");
        let extra = format!("$(touch {})", marker.display());
        let output = run(
            &s.options("ng", &["plan", "review", &extra]),
            &s.paths,
            &mut ui,
        )
        .unwrap();
        assert!(output.text.contains(&extra));
        assert!(output.text.contains("Use the `ng-plan-review` skill"));
        assert!(!marker.exists());
        assert!(run(&s.options("ng", &["plan", "unknown"]), &s.paths, &mut ui).is_err());
        assert!(!s.paths.library.join("library.json").exists());
        assert!(ui.ensure.is_empty());
    }
    #[test]
    fn ng_without_fzf_falls_back_to_manual_list_but_tui_reports_missing_dependency() {
        let s = Setup::new();
        let mut ui = Scripted::new(&[]);
        ui.available = false;
        assert!(run(&s.options("ng", &[]), &s.paths, &mut ui)
            .unwrap()
            .text
            .contains("ng-plan-review"));
        assert_eq!(ui.ensure, [false]);
        assert!(run(&s.options("tui", &[]), &s.paths, &mut ui).is_err());
        assert!(ui.ensure.iter().all(|allowed| !*allowed));
        assert!(!s.paths.library.join("library.json").exists());
    }
    #[test]
    fn tui_previews_packages_and_repeats_install_then_uninstall_through_shared_service() {
        let s = Setup::new();
        let mut ui = Scripted::new(&[1, 0, 1, 2, 1, 0, 1, 9]);
        let output = run(&s.options("tui", &[]), &s.paths, &mut ui).unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("applied 2"));
        assert!(ui.shown.contains("Do not execute source code"));
        assert!(ui.shown.contains("Plan install-"));
        assert!(!s
            .fixture
            .0
            .join("project/.codex/skills/ng-plan-review")
            .exists());
        assert!(s
            .fixture
            .0
            .join("source/manual/plan/review/SKILL.md")
            .exists());
        assert!(ui.choices.is_empty());
    }
    #[test]
    fn tui_cancel_does_not_apply_and_apply_uses_the_original_reviewed_fingerprint() {
        let s = Setup::new();
        let mut ui = Scripted::new(&[0, 0, 9]);
        run(&s.options("tui", &[]), &s.paths, &mut ui).unwrap();
        assert!(!s
            .fixture
            .0
            .join("project/.codex/skills/ng-plan-review")
            .exists());
        let path = s
            .fixture
            .0
            .join("project/.codex/skills/ng-plan-review/SKILL.md");
        let mut ui = Scripted::new(&[0, 1, 9]);
        ui.edit_on_apply = Some(path.clone());
        let output = run(&s.options("tui", &[]), &s.paths, &mut ui).unwrap();
        assert!(output.text.contains("applied 0"));
        assert!(ui.shown.contains("conflict"));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "User change after preview"
        );
    }
    #[test]
    fn tui_policy_switch_controls_copy_and_wrappers() {
        let s = Setup::new();
        let mut ui = Scripted::new(&[4, 1, 1, 0, 1, 9]);
        run(&s.options("tui", &[]), &s.paths, &mut ui).unwrap();
        assert!(!std::fs::symlink_metadata(
            s.fixture.0.join("project/.codex/skills/ng-plan-review")
        )
        .unwrap()
        .file_type()
        .is_symlink());
        assert!(s
            .fixture
            .0
            .join("project/.codex/prompts/ng-plan-review.md")
            .exists());
    }
    #[test]
    fn terminal_text_does_not_keep_control_sequences() {
        assert_eq!(single_line("a\n\t\u{1b}b"), "a   b");
        assert_eq!(safe_text("a\u{1b}\u{7}b\nc"), "ab\nc");
    }
    #[cfg(unix)]
    #[test]
    fn fzf_install_is_explicit_and_logs_a_supervised_static_command() {
        use std::os::unix::fs::PermissionsExt;
        let s = Setup::new();
        let bin = s.fixture.0.join("fake-bin");
        std::fs::create_dir(&bin).unwrap();
        let brew = bin.join("brew");
        std::fs::write(&brew,"#!/bin/sh\n[ \"$*\" = 'install fzf' ] || exit 7\ndir=$(dirname \"$0\")\nprintf '#!/bin/sh\\nexit 0\\n' > \"$dir/fzf\"\nchmod +x \"$dir/fzf\"\n").unwrap();
        std::fs::set_permissions(&brew, std::fs::Permissions::from_mode(0o700)).unwrap();
        let repo = s.paths.repository().unwrap();
        let mut ui = SystemTerminal {
            repository: repo.clone(),
            search: vec![bin.clone()],
            fzf: None,
        };
        assert!(!ui.ensure_fzf(false).unwrap());
        assert!(!bin.join("fzf").exists());
        assert!(ui.ensure_fzf(true).unwrap());
        let record = crate::skills::operations::list(&repo).unwrap().remove(0);
        assert_eq!(record.phase, crate::skills::operations::Phase::Committed);
        assert_eq!(record.executions[0].args, ["install", "fzf"]);
        assert_eq!(record.executions[0].exit_code, Some(0));
    }
    #[cfg(unix)]
    #[test]
    fn dependency_timeout_is_logged_as_failed_and_releases_admission() {
        use std::os::unix::fs::PermissionsExt;
        let s = Setup::new();
        let brew = s.fixture.0.join("slow-brew");
        std::fs::write(&brew, "#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&brew, std::fs::Permissions::from_mode(0o700)).unwrap();
        let repo = s.paths.repository().unwrap();
        let error = install_fzf(&repo, &brew, Duration::from_millis(80)).unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        let record = crate::skills::operations::list(&repo).unwrap().remove(0);
        assert_eq!(record.phase, crate::skills::operations::Phase::Failed);
        assert!(record.executions[0].finished_at_ms.is_some());
        assert!(repo.coordinator.try_update().is_ok());
    }
    #[test]
    #[ignore = "requires an installed fzf; validates the real matching engine without a TTY"]
    fn actual_fzf_matches_visible_text_and_returns_stable_row_identity() {
        let s = Setup::new();
        let mut ui = SystemTerminal::new(s.paths.repository().unwrap());
        assert!(ui.ensure_fzf(false).unwrap());
        let mut command = ui
            .picker_command("test> ", "", false, false, false)
            .unwrap();
        command.args(["--filter", "preview selected skills"]);
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"0\texecute now\n1\tpreview selected skills\n2\tquit\n")
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success());
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            "1\tpreview selected skills\n"
        );
    }
}
