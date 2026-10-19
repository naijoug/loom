use super::*;
use std::path::Component;
pub(super) fn absolute(raw: &str, cwd: &Path, home: &Path) -> Result<PathBuf> {
    if raw.trim().is_empty() || raw.contains('\0') || raw.len() > 4096 {
        return Err(error("Invalid write path"));
    }
    let p = if raw == "~" {
        home.to_owned()
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(raw)
    };
    let p = if p.is_absolute() { p } else { cwd.join(p) };
    let mut result = PathBuf::new();
    for component in p.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}
pub(super) fn glob(pattern: &str, home: &Path) -> Result<fancy_regex::Regex> {
    let pattern = if pattern == "~" {
        home.to_string_lossy().into_owned()
    } else if let Some(rest) = pattern.strip_prefix("~/") {
        format!("{}/{rest}", home.to_string_lossy())
    } else {
        pattern.to_owned()
    };
    let mut regex = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    regex.push_str(".*");
                } else {
                    regex.push_str("[^/]*");
                }
            }
            '?' => regex.push('.'),
            c => {
                if ".+^${}()|[]\\".contains(c) {
                    regex.push('\\');
                }
                regex.push(c);
            }
        }
    }
    regex.push('$');
    crate::shell::regex(&regex).map_err(shell_error)
}
pub(super) fn nearest_real(path: &Path) -> Option<PathBuf> {
    let mut ancestor = path.to_owned();
    let mut suffix = Vec::new();
    loop {
        match std::fs::symlink_metadata(&ancestor) {
            Ok(_) => {
                let mut real = ancestor.canonicalize().ok()?;
                for part in suffix.iter().rev() {
                    real.push(part);
                }
                return Some(real);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(ancestor.file_name()?.to_os_string());
                ancestor = ancestor.parent()?.to_owned();
            }
            Err(_) => return None,
        }
    }
}
pub(super) fn within(target: &Path, base: &Path) -> bool {
    if !target.starts_with(base) {
        return false;
    }
    match (nearest_real(target), nearest_real(base)) {
        (Some(t), Some(b)) => t.starts_with(b),
        _ => false,
    }
}
pub(super) fn allowlisted(target: &Path, patterns: &[fancy_regex::Regex]) -> Result<bool> {
    let Some(lexical) = target.to_str() else {
        return Ok(false);
    };
    let Some(real) = nearest_real(target) else {
        return Ok(false);
    };
    let Some(real) = real.to_str() else {
        return Ok(false);
    };
    for rule in patterns {
        if rule
            .is_match(lexical)
            .map_err(|_| error("Write pattern exceeded its budget"))?
            && rule
                .is_match(real)
                .map_err(|_| error("Write pattern exceeded its budget"))?
        {
            return Ok(true);
        }
    }
    Ok(false)
}
