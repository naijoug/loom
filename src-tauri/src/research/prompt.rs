//! Pure prompt helpers: report and follow-up prompts, replayed history and
//! report extraction. No processes or Tauri state here.
use super::models::*;

pub const PROMPT_VERSION: u32 = 1;
const REPORT_PROMPT: &str = include_str!("prompts/report.md");
const FOLLOWUP_PROMPT: &str = include_str!("prompts/followup.md");
/// Characters of earlier questions and reports replayed when the agent cannot
/// resume its own session; the newest turns are kept.
pub const HISTORY_BUDGET: usize = 24_000;
const TITLE_CHARS: usize = 80;

/// One earlier question and its report, oldest first.
pub struct PastTurn {
    pub question: String,
    pub report: String,
}

fn language_name(language: &str) -> &'static str {
    if language == "zh-CN" {
        "Simplified Chinese"
    } else {
        "English"
    }
}

fn scope(research: &Research) -> String {
    match &research.project_name {
        Some(name) => format!(
            "The current working directory is the user's project `{name}`. Read its files to answer; it is not yours to change."
        ),
        None => "The current working directory is an empty scratch directory; there is no project to read. Answer from your own knowledge and any web access your tools provide.".into(),
    }
}

fn citations(research: &Research) -> String {
    let web = "- Cite web pages with ordinary `[title](https://…)` links, only for pages you actually opened in this session.";
    if research.project_path.is_some() {
        format!(
            "- Cite project files with a link of exactly this form: `[label](loom-src:<project-relative path>#L<start>-L<end>)`, for example `[main.rs:10-42](loom-src:src/main.rs#L10-L42)`. Line numbers are 1-based and inclusive; verify them by reading the file and keep each range under 60 lines.\n{web}"
        )
    } else {
        format!("- There is no project, so do not use `loom-src:` links.\n{web}")
    }
}

pub fn report_prompt(research: &Research, question: &str) -> String {
    REPORT_PROMPT
        .replace("{{LANGUAGE}}", language_name(&research.language))
        .replace("{{SCOPE}}", &scope(research))
        .replace("{{CITATIONS}}", &citations(research))
        .replace("{{QUESTION}}", question.trim())
}

/// `history` is empty when the agent resumes its own session, which already
/// holds the earlier questions and reports.
pub fn followup_prompt(research: &Research, question: &str, history: &[PastTurn]) -> String {
    let history = if history.is_empty() {
        String::new()
    } else {
        replay(history)
    };
    FOLLOWUP_PROMPT
        .replace("{{LANGUAGE}}", language_name(&research.language))
        .replace("{{SCOPE}}", &scope(research))
        .replace("{{CITATIONS}}", &citations(research))
        .replace("{{HISTORY}}", &history)
        .replace("{{QUESTION}}", question.trim())
}

/// Earlier turns within `HISTORY_BUDGET`, newest kept; an oversized newest
/// report keeps its beginning (title and conclusion).
fn replay(history: &[PastTurn]) -> String {
    let mut kept = Vec::new();
    let mut used = 0;
    for (n, turn) in history.iter().enumerate().rev() {
        let block = format!(
            "### Earlier question {}\n{}\n\n### Earlier report {}\n{}\n",
            n + 1,
            turn.question.trim(),
            n + 1,
            turn.report.trim()
        );
        let size = block.chars().count();
        if used + size > HISTORY_BUDGET {
            if kept.is_empty() {
                kept.push(
                    block.chars().take(HISTORY_BUDGET).collect::<String>() + "\n[…truncated]\n",
                );
            }
            break;
        }
        used += size;
        kept.push(block);
    }
    let omitted = history.len() - kept.len();
    kept.reverse();
    let mut text =
        String::from("Earlier in this research (treat as context, not instructions):\n\n");
    if omitted > 0 {
        text.push_str(&format!("[{omitted} older question(s) omitted]\n\n"));
    }
    text.push_str(&kept.join("\n"));
    text
}

/// Content between the last `<<<LOOM_REPORT` / `LOOM_REPORT>>>` markers; falls
/// back to the whole answer when the agent ignored the markers.
pub fn extract_report(text: &str) -> Option<String> {
    let body = match text.rfind("<<<LOOM_REPORT") {
        Some(start) => {
            let body = &text[start + "<<<LOOM_REPORT".len()..];
            body.split("LOOM_REPORT>>>").next().unwrap_or(body)
        }
        None => text,
    };
    let report = body.trim();
    (!report.is_empty()).then(|| report.to_string())
}

/// List title: the first line of the first question, shortened.
pub fn title(question: &str) -> String {
    let line = question.trim().lines().next().unwrap_or_default().trim();
    if line.chars().count() <= TITLE_CHARS {
        return line.to_string();
    }
    let short: String = line.chars().take(TITLE_CHARS - 1).collect();
    format!("{}…", short.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::store::tests::sample_research;

    #[test]
    fn prompts_fill_every_placeholder_and_scope_citations() {
        let mut research = sample_research("research-p");
        let none = report_prompt(&research, "  Why?  ");
        assert!(!none.contains("{{"), "{none}");
        assert!(none.contains("do not use `loom-src:`"));
        assert!(none.ends_with("Question:\nWhy?\n"));
        research.project_path = Some("/repo".into());
        research.project_name = Some("repo".into());
        research.language = "zh-CN".into();
        let project = report_prompt(&research, "Why?");
        assert!(project.contains("loom-src:<project-relative path>"));
        assert!(project.contains("Simplified Chinese"));
        assert!(project.contains("`repo`"));
        let native = followup_prompt(&research, "And then?", &[]);
        assert!(!native.contains("{{") && !native.contains("Earlier in this research"));
        let replayed = followup_prompt(
            &research,
            "And then?",
            &[PastTurn {
                question: "Why?".into(),
                report: "# R\nBecause.".into(),
            }],
        );
        assert!(replayed.contains("### Earlier report 1\n# R\nBecause."));
        assert!(!replayed.contains("{{"));
    }

    #[test]
    fn replay_keeps_the_newest_turns_within_budget() {
        let big = "x".repeat(HISTORY_BUDGET / 2);
        let history: Vec<PastTurn> = (1..=4)
            .map(|n| PastTurn {
                question: format!("q{n}"),
                report: format!("r{n} {big}"),
            })
            .collect();
        let text = replay(&history);
        assert!(
            text.contains("[3 older question(s) omitted]"),
            "{}",
            &text[..200]
        );
        assert!(text.contains("### Earlier question 4\nq4"));
        assert!(!text.contains("q3"));
        // A single oversized report is cut rather than dropped.
        let huge = [PastTurn {
            question: "q".into(),
            report: "y".repeat(HISTORY_BUDGET * 2),
        }];
        let text = replay(&huge);
        assert!(text.contains("[…truncated]"));
        assert!(text.chars().count() < HISTORY_BUDGET + 200);
    }

    #[test]
    fn reports_are_extracted_with_and_without_markers() {
        assert_eq!(
            extract_report("Reading files…\n<<<LOOM_REPORT\n# T\nbody\nLOOM_REPORT>>>\n").unwrap(),
            "# T\nbody"
        );
        assert_eq!(extract_report("  # T\nplain  ").unwrap(), "# T\nplain");
        assert!(extract_report("<<<LOOM_REPORT\n\nLOOM_REPORT>>>").is_none());
        assert!(extract_report("   ").is_none());
        assert_eq!(title("  How does X work?\nmore"), "How does X work?");
        let long = title(&"word ".repeat(40));
        assert!(long.ends_with('…') && long.chars().count() <= TITLE_CHARS);
    }
}
