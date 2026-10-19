//! Program-specific inspection corresponding to Craft v0.14.0 bash-validator.
use super::{regex, Result, ShellRejection};
fn reject(reason: &str) -> ShellRejection {
    ShellRejection::new("unsafe_arguments", reason)
}
fn test(pattern: &str, value: &str) -> Result<bool> {
    regex(pattern)?
        .is_match(value)
        .map_err(|_| reject("Argument inspection exceeded its pattern budget"))
}
pub(super) fn inspect(argv: &[String]) -> Result<()> {
    let Some(name) = argv.first() else {
        return Err(reject("Missing executable"));
    };
    let args = &argv[1..];
    match name.to_ascii_lowercase().as_str() {
        "find"
            if args.iter().any(|a| {
                [
                    "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fprint", "-fprint0",
                    "-fprintf", "-fls",
                ]
                .contains(&a.as_str())
            }) =>
        {
            return Err(reject("find arguments execute commands or write files"));
        }
        "awk" | "gawk" | "mawk" | "nawk" => {
            let script = args.join(" ");
            if test(
                r"(?i)\bsystem\s*\(|\|\s*getline\b|\bprint\b[^\n]*\|\s*[\x22'`]",
                &script,
            )? {
                return Err(reject("awk arguments can execute an external command"));
            }
            // An additional safety correction: awk print/printf redirects write
            // files even though they are inside shell quotes and absent in AST.
            if test(r"\b(?:print|printf)\b[^;\n}]*>", &script)? {
                return Err(reject("awk output redirection writes files"));
            }
        }
        "sed" | "gsed" => sed(args)?,
        "sort" | "gsort" => sort(args)?,
        "gh" if args.first().is_some_and(|a| a == "api") => gh(&args[1..])?,
        "rg" if args.iter().any(|a| a == "--pre" || a.starts_with("--pre=")) => {
            return Err(reject("rg --pre executes a preprocessing command"))
        }
        _ => {}
    }
    Ok(())
}
fn sed(args: &[String]) -> Result<()> {
    let mut scripts = Vec::new();
    let mut script_option = false;
    let mut operands = 0;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            if !script_option && operands == 0 {
                if let Some(script) = args.get(i + 1) {
                    scripts.push(script.as_str());
                }
            }
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = long
                .split_once('=')
                .map(|(n, v)| (n, Some(v)))
                .unwrap_or((long, None));
            match name {
                "in-place" | "file" => {
                    return Err(reject("sed modifies files or loads an uninspected script"))
                }
                "expression" => {
                    script_option = true;
                    scripts.push(if let Some(value) = inline {
                        value
                    } else {
                        i += 1;
                        args.get(i).map(String::as_str).unwrap_or("")
                    });
                }
                "line-length"
                    if inline.is_none()
                        && args.get(i + 1).is_some_and(|s| {
                            !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
                        }) =>
                {
                    i += 1
                }
                _ => {}
            }
        } else if let Some(flags) = arg.strip_prefix('-').filter(|_| arg.len() > 1) {
            for (offset, flag) in flags.char_indices() {
                match flag {
                    'i' | 'f' => {
                        return Err(reject("sed modifies files or loads an uninspected script"))
                    }
                    'e' => {
                        script_option = true;
                        let rest = &flags[offset + 1..];
                        scripts.push(if !rest.is_empty() {
                            rest
                        } else {
                            i += 1;
                            args.get(i).map(String::as_str).unwrap_or("")
                        });
                        break;
                    }
                    'l' => {
                        if offset + 1 == flags.len()
                            && args.get(i + 1).is_some_and(|s| {
                                !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
                            })
                        {
                            i += 1;
                        }
                        break;
                    }
                    _ => {}
                }
            }
        } else {
            if !script_option && operands == 0 {
                scripts.push(arg);
            }
            operands += 1;
        }
        i += 1;
    }
    for script in scripts {
        sed_script(script)?;
    }
    Ok(())
}
fn replace_matches(pattern: &str, text: &str, keep_prefix: bool) -> Result<String> {
    let re = regex(pattern)?;
    let mut result = String::new();
    let mut offset = 0;
    for found in re.captures_iter(text) {
        let found = found.map_err(|_| reject("sed inspection pattern exceeded its budget"))?;
        let whole = found.get(0).expect("matched expression");
        result.push_str(&text[offset..whole.start()]);
        if keep_prefix {
            result.push_str(found.get(1).map(|m| m.as_str()).unwrap_or(""));
        }
        result.push(' ');
        offset = whole.end();
    }
    result.push_str(&text[offset..]);
    Ok(result)
}
fn sed_script(script: &str) -> Result<()> {
    const SUB: &str = r"s([^\\\n])((?:\\.|(?!\1)[^\\])*)\1((?:\\.|(?!\1)[^\\])*)\1([a-zA-Z0-9]*)";
    for caps in regex(SUB)?.captures_iter(script) {
        let caps = caps.map_err(|_| reject("sed substitution inspection exceeded its budget"))?;
        if caps.get(4).is_some_and(|m| m.as_str().contains(['w', 'e'])) {
            return Err(reject(
                "sed substitution can write files or execute programs",
            ));
        }
    }
    let value = replace_matches(SUB, script, false)?;
    let value = replace_matches(
        r"y([^\\\n])((?:\\.|(?!\1)[^\\])*)\1((?:\\.|(?!\1)[^\\])*)\1",
        &value,
        false,
    )?;
    let value = replace_matches(r"/(?:\\.|[^/\\])*/[IM]*", &value, false)?;
    let value = replace_matches(r"(^|[;{}\s!\d$,])(?::\w+|[btT]\w*)", &value, true)?;
    if !test(r"^[\s\d,$~+!;{}pPl=nNdDgGhHxqQzFv]*$", &value)? {
        return Err(reject(
            "sed script is not limited to print-style operations",
        ));
    }
    Ok(())
}
fn sort(args: &[String]) -> Result<()> {
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            if ["output", "compress-program"].contains(&long.split('=').next().unwrap_or("")) {
                return Err(reject(
                    "sort arguments write files or execute an external program",
                ));
            }
        } else if let Some(flags) = arg.strip_prefix('-') {
            for (offset, flag) in flags.char_indices() {
                if flag == 'o' {
                    return Err(reject("sort -o writes files"));
                }
                if "ktST".contains(flag) {
                    if offset + 1 == flags.len() {
                        i += 1;
                    }
                    break;
                }
            }
        }
        i += 1;
    }
    Ok(())
}
fn gh(args: &[String]) -> Result<()> {
    let mut methods = Vec::new();
    let mut fields = Vec::new();
    let mut uses_input = false;
    let mut endpoint = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with('-') || arg == "-" {
            if endpoint.is_none() {
                endpoint = Some(arg.as_str());
            }
            i += 1;
            continue;
        }
        let option = if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = long
                .split_once('=')
                .map(|(n, v)| (n, Some(v)))
                .unwrap_or((long, None));
            [
                "cache",
                "field",
                "header",
                "hostname",
                "input",
                "jq",
                "method",
                "preview",
                "raw-field",
                "template",
            ]
            .contains(&name)
            .then_some((name, value))
        } else {
            arg.char_indices().skip(1).find_map(|(offset, c)| {
                let name = match c {
                    'F' => "field",
                    'H' => "header",
                    'X' => "method",
                    'f' => "raw-field",
                    'p' => "preview",
                    'q' => "jq",
                    't' => "template",
                    _ => return None,
                };
                Some((name, (offset + 1 < arg.len()).then(|| &arg[offset + 1..])))
            })
        };
        if let Some((name, inline)) = option {
            let value = if let Some(v) = inline {
                v
            } else {
                i += 1;
                args.get(i).map(String::as_str).unwrap_or("")
            };
            match name {
                "method" => methods.push(value.to_uppercase()),
                "field" | "raw-field" => fields.push(value),
                "input" => uses_input = true,
                "header" if test(r"(?i)x-http-method|method-override", value)? => {
                    return Err(reject("gh API method-override headers can mutate state"))
                }
                _ => {}
            }
        }
        i += 1;
    }
    if endpoint == Some("graphql") {
        if uses_input || fields.iter().any(|f| f.starts_with("query=@")) {
            return Err(reject("GraphQL query is loaded from an uninspected file"));
        }
        for field in fields {
            if test(r"(?i)\bmutation\b", field)? {
                return Err(reject("GraphQL mutation changes remote state"));
            }
        }
    } else if methods.iter().any(|m| m != "GET" && m != "HEAD")
        || (methods.is_empty() && (!fields.is_empty() || uses_input))
    {
        return Err(reject("gh API arguments select a mutating HTTP request"));
    }
    Ok(())
}
