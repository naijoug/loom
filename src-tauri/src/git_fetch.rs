//! Hardened Git invocation pieces shared by Skills imports and Learn studies.
//!
//! Repositories fetched here are untrusted input: hooks, fsmonitor, external
//! transports, submodules and smudge filters must never run on their behalf.

/// `-c` overrides placed before every Git subcommand.
pub(crate) const HARDENED_CONFIG: &[&str] = &[
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "protocol.ext.allow=never",
    "-c",
    "submodule.recurse=false",
    "-c",
    "filter.lfs.required=false",
    "-c",
    "filter.lfs.smudge=",
    "-c",
    "filter.lfs.process=",
];

/// Written to `.git/info/attributes`, which takes precedence over repository
/// and global attributes, so source-selected filters never execute on checkout.
pub(crate) const NO_FILTER_ATTRIBUTES: &str = "* -filter -working-tree-encoding -text\n";

/// Whether `revision` is a full SHA-1 or SHA-256 object name.
pub(crate) fn valid_revision(revision: &str) -> bool {
    matches!(revision.len(), 40 | 64) && revision.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Maps Git stderr to a controlled diagnostic. Arbitrary SSH wrappers may print
/// secrets, so callers persist this message and never the raw stderr.
pub(crate) fn classify_failure(stderr: &[u8]) -> &'static str {
    let diagnostic = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if diagnostic.contains("host key verification failed")
        || diagnostic.contains("remote host identification has changed")
    {
        "SSH host verification failed; review known_hosts without disabling verification"
    } else if diagnostic.contains("permission denied")
        || diagnostic.contains("authentication failed")
    {
        "Git authentication failed; check the configured credential or SSH identity"
    } else if diagnostic.contains("could not resolve host")
        || diagnostic.contains("could not resolve hostname")
    {
        "Git host lookup failed"
    } else if diagnostic.contains("repository not found")
        || diagnostic.contains("remote branch") && diagnostic.contains("not found")
        || diagnostic.contains("couldn't find remote ref")
    {
        "Repository or reference not found"
    } else {
        "Git command failed; check repository access and local Git configuration"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_must_be_full_hex_object_names() {
        assert!(valid_revision(&"a".repeat(40)));
        assert!(valid_revision(&"0".repeat(64)));
        for bad in ["", "abc", &"g".repeat(40), &"a".repeat(41)] {
            assert!(!valid_revision(bad), "accepted {bad}");
        }
    }

    #[test]
    fn failures_map_to_controlled_messages_without_echoing_stderr() {
        let secret = b"fatal: Authentication failed for 'https://token@host/x'";
        let message = classify_failure(secret);
        assert!(message.starts_with("Git authentication failed"));
        assert!(!message.contains("token"));
        assert_eq!(
            classify_failure(b"fatal: repository 'https://github.com/a/b/' not found\nremote: Repository not found."),
            "Repository or reference not found"
        );
        assert_eq!(
            classify_failure(b"warning: Could not find remote branch nope to clone.\nfatal: Remote branch nope not found in upstream origin"),
            "Repository or reference not found"
        );
        assert_eq!(
            classify_failure(b"weird"),
            "Git command failed; check repository access and local Git configuration"
        );
    }
}
