//! Ask-mode remember selectors adapted from Craft v0.14.0; never executes commands.
use super::*;
use loom_core::{RememberKey, RememberKind, RememberOffer};
const DANGEROUS:&str="rm|rmdir|sudo|su|chmod|chown|chgrp|mv|cp|dd|mkfs|fdisk|parted|kill|killall|pkill|reboot|shutdown|halt|poweroff|curl|wget|ssh|scp|rsync|git push|git reset|git rebase|git checkout|git clean|git restore|git switch|git stash|git branch|git tag|git rm|git worktree|git gc|git reflog|kubectl apply|kubectl delete|kubectl drain|kubectl replace|helm install|helm upgrade|helm uninstall|helm rollback|terraform apply|terraform destroy|tofu apply|tofu destroy|pulumi up|pulumi destroy|npm publish|npm unpublish|pnpm publish|yarn publish|bun publish|cargo publish|gem push|docker push|podman push|netlify deploy|fly deploy|flyctl deploy|wrangler deploy|wrangler publish|firebase deploy";
const WRAPPERS:&str="sudo su doas pkexec gosu runuser run0 sg newgrp setpriv chpst env xargs nohup time timeout watch exec eval command builtin nice ionice stdbuf script parallel chroot flock caffeinate arch unbuffer strace ltrace dtruss sandbox-exec firejail bwrap nsenter unshare";
const LEADING:&str="git gh glab hg svn npm npx pnpm pnpx yarn bun bunx deno pip pip3 pipx uv uvx poetry conda gem bundle cargo go composer brew port apt apt-get dnf yum pacman mise asdf nix make just rake gradle gradlew mvn dotnet swift xcodebuild flutter dart cmake ninja bazel docker podman kubectl helm terraform tofu pulumi aws gcloud az doctl fly flyctl vercel netlify wrangler heroku supabase firebase systemctl launchctl service";
const NAME_ONLY: &str =
    "mkdir touch open xdg-open code cursor subl zed pbcopy say afplay notify-send sleep";
fn member(list: &str, value: &str) -> bool {
    list.split_whitespace().any(|word| word == value)
}
fn command_name(argv: &[String]) -> String {
    argv.first()
        .map(|s| s.rsplit('/').next().unwrap_or("").to_ascii_lowercase())
        .unwrap_or_default()
}
pub fn bash_key(command: &str) -> Option<String> {
    let argv = crate::shell::plain_command(command)?;
    let name = command_name(&argv);
    let sub = argv.get(1).map(|s| s.to_ascii_lowercase());
    if member(WRAPPERS, &name)
        || DANGEROUS
            .split('|')
            .any(|v| v == name || sub.as_ref().is_some_and(|s| v == format!("{name} {s}")))
    {
        return None;
    }
    if member(LEADING, &name) {
        let leading: Vec<_> = argv
            .iter()
            .skip(1)
            .take(3)
            .take_while(|s| !s.starts_with('-'))
            .collect();
        if argv.len() > 1 && leading.is_empty() {
            return None;
        }
        if name == "gh" && sub.as_deref() == Some("api") {
            return None;
        }
        if leading.iter().any(|s|s.split(['-',':','_']).any(|w|member("rm rb rmi del delete remove destroy terminate purge drop prune kill wipe reset revoke uninstall unpublish deregister",&w.to_ascii_lowercase()))){return None;}
        if member("npx pnpx bunx uvx", &name)
            || leading
                .iter()
                .any(|s| member("run exec x dlx run-script", &s.to_ascii_lowercase()))
        {
            return Some(format!("exact:{}", serde_json::to_string(&argv).ok()?));
        }
        let words: Vec<_> = std::iter::once(&argv[0]).chain(leading).collect();
        if words.iter().any(|s| s.chars().any(char::is_whitespace)) {
            return None;
        }
        return Some(
            words
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    if member(NAME_ONLY, &name) {
        Some(argv[0].clone())
    } else {
        Some(format!("exact:{}", serde_json::to_string(&argv).ok()?))
    }
}
pub fn network_hosts(command: &str) -> Option<Vec<String>> {
    let argv = crate::shell::plain_command(command)?;
    let name = command_name(&argv);
    if name != "curl" && name != "wget" {
        return None;
    }
    let scheme=crate::shell::regex(r"(?i)^[a-z][a-z0-9+.-]*://(?:[^/\\\s@?#]+@)?(\[[0-9a-f:.]+\]|[a-z0-9-]+(?:\.[a-z0-9-]+)*)(?::[0-9]+)?(?:[/?#]\S*)?$").ok()?;
    let bare=crate::shell::regex(r"(?i)^(?:[^/\\\s@?#]+@)?(\[[0-9a-f:.]+\]|[a-z0-9-]+(?:\.[a-z0-9-]+)*)(?::[0-9]+)?(?:[/?#]\S*)?$").ok()?;
    let mut hosts = BTreeSet::new();
    for arg in argv.iter().skip(1) {
        let option = arg.split('=').next().unwrap_or("");
        if ["-K", "--config", "-i", "--input-file", "-e", "--execute"].contains(&option) {
            return None;
        }
        // Attached short-option values must not hide a second configuration/input file.
        if (name == "curl" && arg.starts_with("-K"))
            || (name == "wget" && (arg.starts_with("-i") || arg.starts_with("-e")))
        {
            return None;
        }
        if arg.starts_with('-') {
            if arg.contains("://") {
                return None;
            }
            continue;
        }
        let host = scheme
            .captures(arg)
            .ok()?
            .and_then(|c| c.get(1).map(|m| m.as_str().to_ascii_lowercase()))
            .or_else(|| {
                bare.captures(arg)
                    .ok()
                    .flatten()
                    .and_then(|c| c.get(1).map(|m| m.as_str().to_ascii_lowercase()))
            });
        if (arg.contains("://") || arg.starts_with('[')) && host.is_none() {
            return None;
        }
        if let Some(host) = host {
            hosts.insert(host);
        }
    }
    Some(hosts.into_iter().collect())
}
fn key(kind: RememberKind, selector: &str, label: String) -> Result<RememberKey> {
    RememberKey::new(kind, selector, label.chars().take(400).collect())
        .map_err(|e| error(e.to_string()))
}
pub(super) fn offer(
    policy: &PermissionPolicy,
    mode: PermissionMode,
    name: &str,
    input: &Value,
) -> Result<Option<RememberOffer>> {
    let ask = match mode {
        PermissionMode::Ask => true,
        PermissionMode::Guarded => !policy.guarded.as_ref().is_some_and(|c| c.active()),
        _ => false,
    };
    if !ask {
        return Ok(None);
    }
    let keys = match name {
        "write" | "Write" | "edit" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            let raw = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .or_else(|| input.get("notebook_path"))
                .and_then(Value::as_str)
                .ok_or_else(|| error("Missing write path"))?;
            let target = paths::absolute(raw, &policy.scope.working_directory, &policy.scope.home)?;
            let parent = target
                .parent()
                .ok_or_else(|| error("Write target has no parent"))?;
            let Some(real) = paths::nearest_real(parent) else {
                return Ok(None);
            };
            let selector = serde_json::to_string(&(parent, &real))
                .map_err(|_| error("Cannot identify write folder"))?;
            vec![key(
                RememberKind::Directory,
                &selector,
                format!("同一目录内的写入：{}", parent.display()),
            )?]
        }
        "bash" | "Bash" => {
            let Some(command) = input.get("command").and_then(Value::as_str) else {
                return Ok(None);
            };
            let Some(argv) = crate::shell::plain_command(command) else {
                return Ok(None);
            };
            let name = command_name(&argv);
            if (name == "brew"
                && argv.iter().any(|s| s == "--cask")
                && argv
                    .get(1)
                    .is_some_and(|s| s == "install" || s == "upgrade"))
                || name == "installer"
            {
                return Ok(None);
            }
            if let Some(hosts) = network_hosts(command).filter(|h| !h.is_empty()) {
                hosts
                    .iter()
                    .map(|host| key(RememberKind::Domain, host, format!("网络目标：{host}")))
                    .collect::<Result<Vec<_>>>()?
            } else if let Some(selector) = bash_key(command) {
                let label = if selector.starts_with("exact:") {
                    format!("完全相同的 {name} 命令参数")
                } else {
                    format!("命令前缀：{selector}")
                };
                vec![key(RememberKind::Command, &selector, label)?]
            } else {
                return Ok(None);
            }
        }
        _ if name.starts_with("mcp__") => {
            vec![key(RememberKind::Tool, name, format!("同一工具：{name}"))?]
        }
        _ if name.starts_with("api_") => {
            let method = input
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_ascii_uppercase();
            let path = input.get("path").and_then(Value::as_str).unwrap_or("");
            // Include the source, preventing one endpoint's grant from authorizing another service.
            vec![key(
                RememberKind::Api,
                &serde_json::to_string(&(name, &method, path))
                    .map_err(|_| error("Cannot identify API request"))?,
                format!("同一 {name} {method} 请求路径"),
            )?]
        }
        _ => return Ok(None),
    };
    if keys.len() > 32 {
        return Ok(None);
    }
    Ok(Some(RememberOffer { keys }))
}
