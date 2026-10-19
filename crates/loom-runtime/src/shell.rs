//! Bash permissions derived from Craft v0.14.0's AST/argument policy.
//! Syntax is parsed, never executed. See third-party/craft-agents/ for attribution.
mod arguments;
use fancy_regex::{Regex, RegexBuilder};
use std::time::{Duration, Instant};
use tree_sitter::{Node, ParseOptions, Parser, Tree};

const MAX_COMMAND: usize = 64 * 1024;
const MAX_DEPTH: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellRejection {
    pub code: &'static str,
    pub reason: String,
}
impl ShellRejection {
    fn new(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }
}
impl std::fmt::Display for ShellRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.reason)
    }
}
impl std::error::Error for ShellRejection {}
type Result<T> = std::result::Result<T, ShellRejection>;

pub struct ShellPolicy {
    patterns: Vec<Regex>,
}
impl ShellPolicy {
    /// Caller supplies already-merged trusted defaults and workspace rules.
    /// Invalid expressions fail config activation rather than widen permissions.
    pub fn compile(patterns: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut compiled = Vec::new();
        for pattern in patterns {
            if compiled.len() >= 2048 || pattern.len() > 4096 {
                return Err(ShellRejection::new(
                    "config_limit",
                    "Permission patterns exceed their budget",
                ));
            }
            compiled.push(regex(&pattern)?);
        }
        Ok(Self { patterns: compiled })
    }
    pub fn bundled(craft_cli_available: bool) -> Result<Self> {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../resources/craft-v014-permissions.json"))
                .map_err(|_| {
                    ShellRejection::new("config", "Bundled permission rules are invalid")
                })?;
        let patterns = config["allowedBashPatterns"]
            .as_array()
            .ok_or_else(|| ShellRejection::new("config", "Bundled shell rules missing"))?
            .iter()
            .filter_map(|p| p.as_str().or_else(|| p["pattern"].as_str()))
            .filter(|p| craft_cli_available || !p.contains("craft-agent"))
            .map(str::to_owned);
        Self::compile(patterns)
    }
    pub fn validate(&self, source: &str) -> Result<()> {
        let source = continuations(source)?;
        let tree = parse(&source)?;
        if !tree
            .root_node()
            .named_children(&mut tree.root_node().walk())
            .any(|n| n.kind() != "comment")
        {
            return Err(ShellRejection::new(
                "missing_command",
                "No command to classify",
            ));
        }
        self.node(tree.root_node(), &source, 0)
    }
    fn node(&self, node: Node<'_>, source: &str, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(ShellRejection::new(
                "syntax_limit",
                "Shell nesting exceeds the policy budget",
            ));
        }
        match node.kind() {
            "program" | "list" | "pipeline" | "subshell" | "redirected_statement"
            | "compound_statement" | "negated_command" => {
                if node.kind() == "redirected_statement" && node.child_by_field_name("body").is_none() {
                    return Err(ShellRejection::new("missing_command", "Redirect has no command to classify"));
                }
                for child in node.children(&mut node.walk()) {
                    if child.is_named() {
                        self.node(child, source, depth + 1)?;
                    }
                    else if !matches!(child.kind(), ";" | "&&" | "||" | "|" | "(" | ")" | "{" | "}" | "!") {
                        return Err(ShellRejection::new(
                            "shell_operator", "Background execution or unsupported shell operator",
                        ));
                    }
                }
                Ok(())
            }
            "command" => {
                let argv = command_words(node, source, false)?;
                arguments::inspect(&argv)?;
                let command = argv.join(" ");
                for pattern in &self.patterns {
                    let allowed = pattern.is_match(&command).map_err(|_| ShellRejection::new(
                        "pattern_limit", "Permission pattern exceeded its evaluation budget",
                    ))?;
                    if allowed {
                        return Ok(());
                    }
                }
                Err(ShellRejection::new(
                    "not_allowlisted",
                    "Command is not in the read-only allowlist; switch to Ask or add a permission rule",
                ))
            }
            "file_redirect" => redirect(node, source),
            "comment" => Ok(()),
            _ => Err(ShellRejection::new(
                "unsupported_syntax",
                "Only plain commands, pipelines, command lists and subshells are supported in Explore",
            )),
        }
    }
}
pub(crate) fn regex(pattern: &str) -> Result<Regex> {
    // JS word boundaries/classes are ASCII without the Unicode flag.
    // Keep Unicode matching for dot/negated sets (Rust str must stay UTF-8).
    let mut normalized = String::new();
    let mut chars = pattern.chars();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '[' => {
                in_class = true;
                normalized.push(c);
            }
            ']' => {
                in_class = false;
                normalized.push(c);
            }
            '\\' => match chars.next() {
                Some('b') if !in_class => normalized.push_str(
                    r"(?:(?<![a-zA-Z0-9_])(?=[a-zA-Z0-9_])|(?<=[a-zA-Z0-9_])(?![a-zA-Z0-9_]))",
                ),
                Some('B') if !in_class => normalized.push_str(
                    r"(?:(?<![a-zA-Z0-9_])(?![a-zA-Z0-9_])|(?<=[a-zA-Z0-9_])(?=[a-zA-Z0-9_]))",
                ),
                Some('w') => normalized.push_str("[a-zA-Z0-9_]"),
                Some('W') => normalized.push_str("[^a-zA-Z0-9_]"),
                Some('d') => normalized.push_str("[0-9]"),
                Some('D') => normalized.push_str("[^0-9]"),
                Some(other) => {
                    normalized.push('\\');
                    normalized.push(other);
                }
                None => normalized.push('\\'),
            },
            _ => normalized.push(c),
        }
    }
    RegexBuilder::new(&normalized)
        .backtrack_limit(100_000)
        .delegate_size_limit(2 * 1024 * 1024)
        .delegate_dfa_size_limit(2 * 1024 * 1024)
        .build()
        .map_err(|_| {
            ShellRejection::new(
                "invalid_pattern",
                "Invalid or unsupported permission regular expression",
            )
        })
}
fn continuations(source: &str) -> Result<String> {
    if source.len() > MAX_COMMAND || source.contains('\0') {
        return Err(ShellRejection::new(
            "invalid_input",
            "Shell input contains a NUL or exceeds 64 KiB",
        ));
    }
    let mut result = String::new();
    let mut chars = source.chars().peekable();
    let mut single = false;
    let mut double = false;
    let mut comment = false;
    let mut word_start = true;
    while let Some(c) = chars.next() {
        if comment {
            result.push(c);
            if c == '\n' {
                comment = false;
                word_start = true;
            }
            continue;
        }
        if c == '#' && !single && !double && word_start {
            comment = true;
            result.push(c);
            continue;
        }
        if c == '\\' && !single {
            if chars.peek() == Some(&'\n') {
                chars.next();
                continue;
            }
            result.push(c);
            if let Some(next) = chars.next() {
                result.push(next);
            }
            word_start = false;
            continue;
        }
        if c == '\'' && !double {
            single = !single;
        }
        if c == '"' && !single {
            double = !double;
        }
        word_start = !single && !double && (c.is_whitespace() || ";|&()<>".contains(c));
        result.push(c);
    }
    Ok(result)
}
fn parse(source: &str) -> Result<Tree> {
    if source.len() > MAX_COMMAND || source.contains('\0') {
        return Err(ShellRejection::new(
            "invalid_input",
            "Shell input contains a NUL or exceeds 64 KiB",
        ));
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .map_err(|_| ShellRejection::new("parser", "Bash grammar could not be loaded"))?;
    let deadline = Instant::now() + Duration::from_millis(250);
    let mut progress = |_: &tree_sitter::ParseState| Instant::now() >= deadline;
    let tree = parser
        .parse_with_options(
            &mut |offset, _| &source.as_bytes()[offset..],
            None,
            Some(ParseOptions::new().progress_callback(&mut progress)),
        )
        .ok_or_else(|| ShellRejection::new("parse_limit", "Shell parsing exceeded its budget"))?;
    if tree.root_node().has_error() {
        return Err(ShellRejection::new(
            "parse_error",
            "Shell input could not be parsed completely",
        ));
    }
    Ok(tree)
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
fn word(node: Node<'_>, source: &str, depth: usize) -> Result<String> {
    if depth > MAX_DEPTH {
        return Err(ShellRejection::new(
            "syntax_limit",
            "Shell word nesting exceeds its budget",
        ));
    }
    match node.kind() {
        "command_name" | "concatenation" => {
            let mut result = String::new();
            for child in node.named_children(&mut node.walk()) {
                result.push_str(&word(child, source, depth + 1)?);
            }
            Ok(result)
        }
        "word" | "number" => unescape(text(node, source), false),
        "raw_string" => Ok(text(node, source)[1..text(node, source).len() - 1].into()),
        "string" => {
            for child in node.named_children(&mut node.walk()) {
                if child.kind() != "string_content" {
                    return Err(ShellRejection::new(
                        "expansion",
                        "Shell expansions are not allowed in read-only commands",
                    ));
                }
            }
            let value = text(node, source);
            unescape(&value[1..value.len() - 1], true)
        }
        _ => Err(ShellRejection::new(
            "expansion",
            "Shell substitutions, expansions and executable strings are not read-only",
        )),
    }
}
fn unescape(value: &str, double_quoted: bool) -> Result<String> {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        let next = chars
            .next()
            .ok_or_else(|| ShellRejection::new("parse_error", "Incomplete shell escape"))?;
        if next == '\n' {
            continue;
        }
        if double_quoted && !matches!(next, '$' | '`' | '"' | '\\') {
            result.push('\\');
        }
        result.push(next);
    }
    Ok(result)
}
fn command_words(node: Node<'_>, source: &str, plain: bool) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for child in node.named_children(&mut node.walk()) {
        match child.kind() {
            "variable_assignment" => {
                return Err(ShellRejection::new(
                    "env_assignment",
                    "Environment assignments can change executable lookup or inject libraries",
                ))
            }
            "file_redirect" if !plain => redirect(child, source)?,
            "file_redirect" | "herestring_redirect" => {
                return Err(ShellRejection::new(
                    "redirect",
                    "Redirects are not supported for this command",
                ))
            }
            _ => result.push(word(child, source, 0)?),
        }
    }
    if result.is_empty() {
        return Err(ShellRejection::new(
            "missing_command",
            "No command to classify",
        ));
    }
    Ok(result)
}
fn redirect(node: Node<'_>, source: &str) -> Result<()> {
    let target = node
        .child_by_field_name("destination")
        .ok_or_else(|| ShellRejection::new("redirect", "Missing redirect destination"))?;
    // Validate expansions even in input redirects; the shell evaluates them.
    let target = word(target, source, 0)?;
    let op = node
        .children(&mut node.walk())
        .find(|n| !n.is_named())
        .map(|n| n.kind())
        .unwrap_or("");
    if !matches!(op, "<" | "<&" | ">" | ">>" | ">&" | ">|" | "<>") {
        return Err(ShellRejection::new(
            "redirect",
            "Unsupported redirect operator",
        ));
    }
    if matches!(op, "<" | "<&")
        || target == "/dev/null"
        || (op == ">&" && !target.is_empty() && target.bytes().all(|c| c.is_ascii_digit()))
    {
        Ok(())
    } else {
        Err(ShellRejection::new(
            "redirect",
            "File-writing redirects are not read-only",
        ))
    }
}
/// Used by remembered approval rules; compounds/expansions must not inherit a
/// grant for a single executable. Returns literal argv, never invokes a shell.
pub fn plain_command(source: &str) -> Option<Vec<String>> {
    let source = continuations(source).ok()?;
    let tree = parse(&source).ok()?;
    let root = tree.root_node();
    let mut cursor = root.walk();
    let children: Vec<_> = root
        .children(&mut cursor)
        .filter(|n| !matches!(n.kind(), ";" | "comment"))
        .collect();
    if children.len() != 1 {
        return None;
    }
    let mut command = children[0];
    if command.kind() == "negated_command" {
        command = command.named_child(0)?;
    }
    if command.kind() != "command" {
        return None;
    }
    command_words(command, &source, true).ok()
}
