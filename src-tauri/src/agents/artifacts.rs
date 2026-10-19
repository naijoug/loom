use super::*;

/// Derive a concise task title from the first successful agent's plan output,
/// preferring the stated goal. Falls back to the first meaningful content line.
#[cfg(test)]
pub(super) fn derive_plan_title(invocations: &[AgentInvocation]) -> Option<String> {
    let output = invocations
        .iter()
        .find(|invocation| {
            invocation.status == "succeeded" && !invocation.raw_output.trim().is_empty()
        })
        .map(|invocation| invocation.raw_output.as_str())?;

    derive_plan_title_from_markdown(output)
}

pub(super) fn derive_plan_title_from_markdown(markdown: &str) -> Option<String> {
    let lines: Vec<&str> = markdown.lines().collect();

    let is_content_line = |line: &str| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("---")
    };

    let goal_text = lines.iter().enumerate().find_map(|(index, line)| {
        let lower = line.to_lowercase();
        if !(lower.contains("goal") || lower.contains("目标")) {
            return None;
        }

        // Inline form: "**Goal:** Add X" / "Goal: Add X".
        if let Some((_, after)) = line.split_once(':') {
            let after = clean_title(after);
            if !after.is_empty() {
                return Some(after);
            }
        }

        // Heading form: take the next content line.
        lines[index + 1..]
            .iter()
            .find(|candidate| is_content_line(candidate))
            .map(|candidate| candidate.trim().to_string())
    });

    let raw_title = goal_text.or_else(|| {
        lines
            .iter()
            .find(|line| is_content_line(line))
            .map(|line| line.trim().to_string())
    })?;

    let title = clean_title(&raw_title);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// Strip Markdown decoration and leading list markers, then cap the length.
pub(super) fn clean_title(text: &str) -> String {
    let stripped = text.replace("**", "").replace(['`', '#'], "");
    let stripped = stripped.trim();
    let stripped = stripped.trim_start_matches(|c: char| {
        c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | '*' | '、' | '：' | ':' | ' ')
    });
    let cleaned = stripped.trim();

    if cleaned.chars().count() <= 64 {
        return cleaned.to_string();
    }

    let truncated: String = cleaned.chars().take(64).collect();
    match truncated.rsplit_once(' ') {
        Some((head, _)) if head.chars().count() > 24 => format!("{head}…"),
        _ => format!("{truncated}…"),
    }
}

pub(super) fn render_final_plan(
    task_title: &str,
    requirement: &str,
    discussion_summary: &str,
    invocations: &[AgentInvocation],
) -> String {
    let has_successful_agent = invocations
        .iter()
        .any(|invocation| invocation.status == "succeeded");
    let agent_notes = invocations
        .iter()
        .map(|invocation| {
            format!(
                "- **{}** ({}) : {}",
                invocation.agent_name, invocation.status, invocation.output_summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let agent_proposals = invocations
        .iter()
        .filter(|invocation| {
            invocation.status == "succeeded" && !invocation.raw_output.trim().is_empty()
        })
        .map(|invocation| {
            format!(
                "### {}\n\n{}",
                invocation.agent_name,
                invocation.raw_output.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let agent_proposals = if agent_proposals.is_empty() {
        "No successful agent produced a detailed proposal.".to_string()
    } else {
        agent_proposals
    };
    let agent_todos = collect_implementation_todos_from_invocations(invocations);
    let implementation_todo = if !agent_todos.is_empty() {
        agent_todos
            .iter()
            .enumerate()
            .map(|(index, todo)| format!("{}. {}", index + 1, todo))
            .collect::<Vec<_>>()
            .join("\n")
    } else if has_successful_agent {
        "1. Review successful Agent output and select the implementation slice.\n2. Persist planning runs and Agent invocation evidence.\n3. Generate the final plan document and todo list.\n4. Add the implementation handoff view."
            .to_string()
    } else {
        "No implementation todo items were generated because all selected planning agents failed."
            .to_string()
    };
    let acceptance_criteria = if has_successful_agent {
        "- The planning discussion is visible in the right-side conversation stream.\n- The final plan is written to `docs/plans/` in the selected project.\n- The user can confirm the plan and move the task to `ready_to_implement` with actionable todo items."
    } else {
        "- Failed Agent diagnostics are visible in the right-side conversation stream.\n- The final plan is written to `docs/plans/` in the selected project for troubleshooting.\n- The task remains in plan review until a successful planning run is available."
    };

    format!(
        "# {task_title} — Final Plan\n\n## Requirement\n\n{requirement}\n\n## Discussion Summary\n\n{discussion_summary}\n\n## Agent Notes\n\n{agent_notes}\n\n## Agent Proposals\n\n{agent_proposals}\n\n## Implementation Todo\n\n{implementation_todo}\n\n## Acceptance Criteria\n\n{acceptance_criteria}\n"
    )
}

pub(super) fn render_reviewed_final_plan(
    plan: &str,
    reviews: &[PlanReview],
    decisions: &[PlanningDecision],
) -> String {
    let base = plan
        .split("\n## Mutual Plan Reviews\n")
        .next()
        .unwrap_or(plan)
        .trim_end();
    let review_section = if reviews.is_empty() {
        "- No mutual review records captured yet.".to_string()
    } else {
        reviews
            .iter()
            .map(|review| {
                format!(
                    "- **{} → {}** [{} / {}]: {}",
                    review.reviewer_agent_name,
                    review.target_agent_name,
                    review.status,
                    review.severity,
                    review.finding
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let decision_section = if decisions.is_empty() {
        "- No human decisions captured yet.".to_string()
    } else {
        decisions
            .iter()
            .map(|decision| format!("- **{}**: {}", decision.title, decision.content))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "{base}\n\n## Mutual Plan Reviews\n\n{review_section}\n\n## Human Decisions\n\n{decision_section}\n"
    )
}

pub(super) fn collect_implementation_todos_from_invocations(
    invocations: &[AgentInvocation],
) -> Vec<String> {
    let mut todos = Vec::new();

    for invocation in invocations
        .iter()
        .filter(|invocation| invocation.status == "succeeded")
    {
        for todo in implementation_todos_from_agent_output(&invocation.raw_output) {
            let duplicate = todos
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&todo));
            if !duplicate {
                todos.push(todo);
            }
            if todos.len() >= 8 {
                return todos;
            }
        }
    }

    todos
}

pub(super) fn implementation_todos_from_agent_output(output: &str) -> Vec<String> {
    let mut in_section = false;
    let mut todos = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            in_section = is_implementation_todo_heading(trimmed);
            continue;
        }

        if !in_section {
            continue;
        }

        if let Some(todo) = strip_agent_todo_marker(trimmed) {
            if !todo.is_empty() && !todo.to_lowercase().starts_with("no implementation todo") {
                todos.push(todo.to_string());
            }
        }
    }

    todos
}

pub(super) fn is_implementation_todo_heading(line: &str) -> bool {
    let heading = line.trim_start_matches('#').trim().to_lowercase();
    matches!(
        heading.as_str(),
        "implementation todo"
            | "implementation todos"
            | "implementation tasks"
            | "implementation task list"
            | "implementation plan"
            | "实施任务"
            | "实施待办"
            | "实现任务"
            | "实现待办"
            | "开发任务"
            | "任务清单"
    )
}

pub(super) fn strip_agent_todo_marker(line: &str) -> Option<&str> {
    let bullet = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("• "));
    if let Some(value) = bullet {
        return Some(strip_agent_checkbox_marker(value.trim()));
    }

    for marker in [". ", ") ", "、"] {
        if let Some((number, rest)) = line.split_once(marker) {
            if number.chars().all(|char| char.is_ascii_digit()) {
                return Some(strip_agent_checkbox_marker(rest.trim()));
            }
        }
    }

    None
}

pub(super) fn strip_agent_checkbox_marker(line: &str) -> &str {
    ["[ ] ", "[x] ", "[X] "]
        .iter()
        .find_map(|marker| line.strip_prefix(marker))
        .unwrap_or(line)
        .trim()
}

pub(super) fn planning_evidence_dir(
    project_path: &Path,
    task_id: &str,
    planning_run_id: &str,
) -> PathBuf {
    storage::project_loom_dir(project_path)
        .join("planning")
        .join(task_id)
        .join(planning_run_id)
}

pub(super) fn next_project_plan_path(
    project_path: &Path,
    task_title: &str,
) -> Result<PathBuf, String> {
    next_project_plan_path_for_stamp(
        project_path,
        &local_date_string(),
        &local_time_string(),
        task_title,
    )
}

pub(super) fn update_project_plans_index(
    project_path: &Path,
    plan_path: &Path,
    summary: &str,
) -> Result<(), String> {
    let date = plan_path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid plan date path: {}", plan_path.display()))?;
    let file_name = plan_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid plan file name: {}", plan_path.display()))?;
    let index_path = project_path.join("docs").join("PLANS.md");
    let existing = if index_path.exists() {
        fs::read_to_string(&index_path)
            .map_err(|error| format!("failed to read plans index: {error}"))?
    } else {
        "# 计划文档索引\n".to_string()
    };
    let updated = render_updated_plans_index(&existing, date, file_name, summary);

    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create plans index directory: {error}"))?;
    }
    storage::atomic_write_text(&index_path, &updated)
        .map_err(|error| format!("failed to write plans index: {error}"))
}

pub(super) fn render_updated_plans_index(
    existing: &str,
    date: &str,
    file_name: &str,
    summary: &str,
) -> String {
    let entry = format!("- `{file_name}`\n  > {}", summary.trim());
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    if lines.is_empty() {
        lines.push("# 计划文档索引".to_string());
    }

    let heading = format!("## {date}");
    if let Some(index) = lines.iter().position(|line| line.trim() == heading) {
        if date_section_contains_plan(&lines, index, file_name) {
            return ensure_trailing_newline(&update_date_section_plan_summary(
                &lines, index, file_name, summary,
            ));
        }

        let insert_at = date_section_plan_insert_index(&lines, index, file_name);
        lines.insert(insert_at, entry);
        return ensure_trailing_newline(&lines.join("\n"));
    }

    let mut insert_at = plan_date_insert_index(&lines, date);
    if insert_at == lines.len() && lines.last().is_some_and(|line| !line.trim().is_empty()) {
        lines.push(String::new());
        insert_at += 1;
    }
    lines.insert(insert_at, format!("## {date}"));
    lines.insert(insert_at + 1, String::new());
    lines.insert(insert_at + 2, entry);
    lines.insert(insert_at + 3, String::new());

    ensure_trailing_newline(&lines.join("\n"))
}

pub(super) fn date_section_contains_plan(
    lines: &[String],
    heading_index: usize,
    file_name: &str,
) -> bool {
    lines
        .iter()
        .skip(heading_index + 1)
        .take_while(|line| !line.trim_start().starts_with("## "))
        .any(|line| plan_entry_file_name(line).is_some_and(|entry| entry == file_name))
}

pub(super) fn update_date_section_plan_summary(
    lines: &[String],
    heading_index: usize,
    file_name: &str,
    summary: &str,
) -> String {
    let mut updated = lines.to_vec();
    let section_end = lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .find_map(|(index, line)| line.trim_start().starts_with("## ").then_some(index))
        .unwrap_or(lines.len());

    if let Some(entry_index) = lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .take(section_end.saturating_sub(heading_index + 1))
        .find_map(|(index, line)| {
            plan_entry_file_name(line)
                .is_some_and(|entry| entry == file_name)
                .then_some(index)
        })
    {
        let summary_line = format!("  > {}", summary.trim());
        if updated
            .get(entry_index + 1)
            .is_some_and(|line| line.trim_start().starts_with("> "))
        {
            updated[entry_index + 1] = summary_line;
        } else {
            updated.insert(entry_index + 1, summary_line);
        }
    }

    updated.join("\n")
}

pub(super) fn date_section_plan_insert_index(
    lines: &[String],
    heading_index: usize,
    file_name: &str,
) -> usize {
    let section_end = lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .find_map(|(index, line)| line.trim_start().starts_with("## ").then_some(index))
        .unwrap_or(lines.len());

    for (index, line) in lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .take(section_end.saturating_sub(heading_index + 1))
    {
        if let Some(existing_file_name) = plan_entry_file_name(line) {
            if file_name > existing_file_name {
                return index;
            }
        }
    }

    section_end
}

pub(super) fn plan_entry_file_name(line: &str) -> Option<&str> {
    let value = line.trim().strip_prefix("- ")?.trim();
    value
        .strip_prefix('`')
        .and_then(|rest| rest.split_once('`').map(|(file_name, _)| file_name))
        .or_else(|| value.split_whitespace().next())
}

pub(super) fn plan_date_insert_index(lines: &[String], date: &str) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| {
            plan_heading_date(line)
                .filter(|heading_date| date > *heading_date)
                .map(|_| index)
        })
        .unwrap_or(lines.len())
}

pub(super) fn plan_heading_date(line: &str) -> Option<&str> {
    let value = line.trim().strip_prefix("## ")?;
    (value.len() == 10
        && value
            .chars()
            .all(|char| char.is_ascii_digit() || char == '-'))
    .then_some(value)
}

pub(super) fn ensure_trailing_newline(content: &str) -> String {
    let mut content = content.to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content
}

pub(super) fn next_project_plan_path_for_stamp(
    project_path: &Path,
    local_date: &str,
    local_time: &str,
    task_title: &str,
) -> Result<PathBuf, String> {
    let plans_dir = storage::project_plans_dir(project_path).join(local_date);
    let base_name = format!("{}-{}", local_time, slugify_plan_title(task_title));
    let mut candidate = plans_dir.join(format!("{base_name}.md"));
    let mut suffix = 2;

    while candidate.exists() {
        candidate = plans_dir.join(format!("{base_name}-{suffix}.md"));
        suffix += 1;
    }

    Ok(candidate)
}

pub(super) fn slugify_plan_title(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }

        if slug.len() >= 64 {
            break;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "plan".to_string()
    } else {
        slug
    }
}

pub(super) fn local_date_string() -> String {
    platform_local_date().unwrap_or_else(|| utc_date_string(SystemTime::now()))
}

pub(super) fn local_time_string() -> String {
    platform_local_time().unwrap_or_else(|| utc_time_string(SystemTime::now()))
}

#[cfg(not(windows))]
pub(super) fn platform_local_date() -> Option<String> {
    Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 10)
}

#[cfg(windows)]
pub(super) fn platform_local_date() -> Option<String> {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format yyyy-MM-dd"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 10)
}

#[cfg(not(windows))]
pub(super) fn platform_local_time() -> Option<String> {
    Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 5)
}

#[cfg(windows)]
pub(super) fn platform_local_time() -> Option<String> {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format HH:mm"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 5)
}

pub(super) fn utc_date_string(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

pub(super) fn utc_time_string(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let seconds_in_day = seconds.rem_euclid(86_400);
    let hour = seconds_in_day.div_euclid(3_600);
    let minute = seconds_in_day.rem_euclid(3_600).div_euclid(60);
    format!("{hour:02}:{minute:02}")
}

// Howard Hinnant's civil-from-days algorithm, using days since Unix epoch.
pub(super) fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * mp + 2).div_euclid(5) + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };

    (year as i32, month as u32, day as u32)
}
