use super::*;
const WRITE_WORDS:&str="create update upsert delete del remove rm destroy drop truncate purge wipe erase clear send put set add append insert write edit modify replace rename move copy duplicate clone rebase revert unpublish unarchive reopen cancel approve reject dismiss assign unassign kick ban unban unblock mute unmute follow unfollow react upvote downvote enable disable activate deactivate restart kill terminate run execute exec trigger invoke pay buy subscribe unsubscribe grant revoke authorize login logout reply forward mark submit save apply uninstall upgrade downgrade rollback reboot shutdown provision allocate accept decline attach detach notify compose reset archive publish deploy restore toggle rebuild refresh flush optimize install sync ack confirm expire reindex enqueue dequeue retry";
fn words(name: &str) -> Vec<String> {
    let mut text = String::new();
    let mut previous = None;
    for c in name.chars() {
        if c.is_ascii_uppercase()
            && previous.is_some_and(|p: char| p.is_ascii_lowercase() || p.is_ascii_digit())
        {
            text.push(' ');
        }
        text.push(c);
        previous = Some(c);
    }
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}
pub(super) fn read_only(source: &str, action: &str, verbs: &BTreeSet<String>) -> bool {
    let words = words(action);
    if words.is_empty()
        || words
            .iter()
            .any(|w| WRITE_WORDS.split_whitespace().any(|v| v == w))
    {
        return false;
    }
    if verbs.contains(&words[0]) {
        return true;
    }
    if words.len() < 2
        || !verbs.contains(&words[1])
        || ["status", "info", "count", "exists"].contains(&words[1].as_str())
    {
        return false;
    }
    self::words(source).contains(&words[0])
        || (words.len() == 2
            && ["read", "get", "list", "search", "fetch", "find", "query"]
                .contains(&words[1].as_str()))
}
