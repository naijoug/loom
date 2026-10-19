use loom_runtime::policy::remember::{bash_key, network_hosts};
use serde::Deserialize;
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    command: String,
    bash_key: Option<String>,
    hosts: Option<Vec<String>>,
    #[serde(default)]
    reject_hidden_target: bool,
}
#[test]
fn remember_keys_match_fixed_upstream_and_reject_attached_hidden_network_config() {
    let cases: Corpus =
        serde_json::from_str(include_str!("fixtures/craft-remember-cases.json")).unwrap();
    for case in cases.cases {
        assert_eq!(bash_key(&case.command), case.bash_key, "{}", case.command);
        assert_eq!(
            network_hosts(&case.command),
            if case.reject_hidden_target {
                None
            } else {
                case.hosts
            },
            "{}",
            case.command
        );
    }
}
