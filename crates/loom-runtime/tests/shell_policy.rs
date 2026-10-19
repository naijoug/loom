use loom_runtime::shell::{plain_command, ShellPolicy};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cases {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    command: String,
    upstream_allowed: bool,
    loom_allowed: Option<bool>,
    upstream_plain: Option<Vec<String>>,
    loom_plain: Option<Vec<String>>,
}
fn cases() -> Cases {
    serde_json::from_str(include_str!("fixtures/craft-shell-cases.json")).unwrap()
}
#[test]
fn rust_shell_policy_matches_executed_craft_v014_cases_with_documented_safety_corrections() {
    let policy = ShellPolicy::bundled(false).unwrap();
    let mut differences = vec![];
    for case in cases().cases {
        let result = policy.validate(&case.command);
        let expected = case.loom_allowed.unwrap_or(case.upstream_allowed);
        if result.is_ok() != expected {
            differences.push(format!(
                "{:?}: expected {expected}, got {result:?}",
                case.command
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
#[test]
fn remembered_command_keys_have_the_same_plain_argv_boundary_as_craft() {
    let mut differences = vec![];
    for case in cases().cases {
        let actual = plain_command(&case.command);
        let expected = case.loom_plain.or(case.upstream_plain);
        if actual != expected {
            differences.push(format!(
                "{:?}: upstream {:?}, Rust {actual:?}",
                case.command, expected
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
#[test]
fn custom_allowlists_cannot_override_syntax_or_program_safety_checks() {
    let policy = ShellPolicy::compile([".*".into()]).unwrap();
    for command in [
        "touch ordinary",
        "custom-program arg",
        "(custom-a && custom-b) | custom-c",
    ] {
        policy.validate(command).unwrap();
    }
    for command in [
        "PATH=/evil ls",
        "ls $(touch f)",
        "ls &",
        "ls > file",
        "cat <$(touch f)",
        "find . -delete",
        "sort -oout file",
        "sed -n 's/x/y/e' file",
        "gh api -XDELETE /repos",
        "while true; do ls; done",
    ] {
        assert!(policy.validate(command).is_err(), "{command}");
    }
}
#[test]
fn invalid_configuration_and_large_or_deep_shell_input_are_bounded() {
    assert!(ShellPolicy::compile(["[".into()]).is_err());
    assert!(ShellPolicy::compile(["x".repeat(4097)]).is_err());
    let policy = ShellPolicy::bundled(false).unwrap();
    assert!(policy.validate(&"a".repeat(65 * 1024)).is_err());
    assert!(policy.validate("ls\0; touch f").is_err());
    assert!(policy
        .validate(&format!("{}ls{}", "(".repeat(300), ")".repeat(300)))
        .is_err());
}
