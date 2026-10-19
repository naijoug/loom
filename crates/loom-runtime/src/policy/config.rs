use super::*;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RulePattern {
    Text(String),
    Annotated {
        pattern: String,
        comment: Option<String>,
    },
}
impl RulePattern {
    pub fn pattern(&self) -> &str {
        match self {
            Self::Text(p) | Self::Annotated { pattern: p, .. } => p,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiRule {
    pub method: String,
    pub path: String,
    pub comment: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandHint {
    pub command: String,
    pub reason: String,
    pub context: Option<String>,
    pub try_instead: Option<Vec<String>>,
    pub example: Option<String>,
    pub when_not_matching: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionConfig {
    pub version: Option<String>,
    #[serde(default)]
    pub allowed_bash_patterns: Vec<RulePattern>,
    #[serde(default)]
    pub allowed_mcp_patterns: Vec<RulePattern>,
    #[serde(default)]
    pub allowed_api_endpoints: Vec<ApiRule>,
    #[serde(default)]
    pub allowed_write_paths: Vec<RulePattern>,
    #[serde(default)]
    pub blocked_tools: Vec<RulePattern>,
    #[serde(default)]
    pub blocked_command_hints: Vec<CommandHint>,
}
impl PermissionConfig {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 256 * 1024 {
            return Err(error("Permission configuration exceeds 256 KiB"));
        }
        let config: Self =
            serde_json::from_slice(bytes).map_err(|_| error("Invalid permission configuration"))?;
        config.validate()?;
        Ok(config)
    }
    pub fn bundled() -> Result<Self> {
        Self::parse(include_bytes!(
            "../../resources/craft-v014-permissions.json"
        ))
    }
    fn validate(&self) -> Result<()> {
        if serde_json::to_vec(self)
            .map_err(|_| error("Invalid configuration"))?
            .len()
            > 256 * 1024
        {
            return Err(error("Permission configuration exceeds 256 KiB"));
        }
        for list in [
            &self.allowed_bash_patterns,
            &self.allowed_mcp_patterns,
            &self.allowed_write_paths,
            &self.blocked_tools,
        ] {
            if list.len() > 1024 || list.iter().any(|p| p.pattern().len() > 4096) {
                return Err(error("Permission rules exceed their budget"));
            }
        }
        if self.allowed_api_endpoints.len() > 1024
            || self.allowed_api_endpoints.iter().any(|r| {
                r.path.len() > 4096
                    || !["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]
                        .contains(&r.method.as_str())
            })
        {
            return Err(error("Invalid API permission rule"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolicyLayers {
    /// None selects the bundled application rules; workspace/source rules extend them.
    pub application: Option<PermissionConfig>,
    pub workspace: PermissionConfig,
    /// Only active sources belong here. Keys also constrain source MCP regexes.
    pub sources: BTreeMap<Id, PermissionConfig>,
}
pub(super) struct CompiledRules {
    pub shell: ShellPolicy,
    pub mcp_verbs: BTreeSet<String>,
    pub mcp_patterns: Vec<(Option<Id>, fancy_regex::Regex)>,
    pub api: Vec<(String, fancy_regex::Regex)>,
    pub writes: Vec<fancy_regex::Regex>,
    pub blocked: BTreeSet<String>,
    pub sources: BTreeSet<Id>,
}
impl CompiledRules {
    pub fn new(layers: &PolicyLayers, scope: &PolicyScope) -> Result<Self> {
        if layers.sources.len() > 256 {
            return Err(error("Too many active source permission layers"));
        }
        let bundled = PermissionConfig::bundled()?;
        let app = layers.application.as_ref().unwrap_or(&bundled);
        let mut shell = Vec::new();
        let mut out = Self {
            shell: ShellPolicy::compile([]).map_err(shell_error)?,
            mcp_verbs: BTreeSet::new(),
            mcp_patterns: Vec::new(),
            api: Vec::new(),
            writes: Vec::new(),
            blocked: BTreeSet::new(),
            sources: layers.sources.keys().cloned().collect(),
        };
        let all = std::iter::once((None, app, true))
            .chain(std::iter::once((None, &layers.workspace, false)))
            .chain(layers.sources.iter().map(|(id, c)| (Some(id), c, false)));
        for (source, config, is_app) in all {
            if source.is_some_and(|id| id.as_str().contains("__")) {
                return Err(error("Source slug cannot contain the MCP separator"));
            }
            config.validate()?;
            for p in &config.allowed_bash_patterns {
                if scope.craft_cli_available || !p.pattern().contains("craft-agent") {
                    shell.push(p.pattern().to_owned());
                }
            }
            for p in &config.allowed_mcp_patterns {
                let value = p.pattern();
                if is_app
                    && !value.is_empty()
                    && value.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                {
                    out.mcp_verbs.insert(value.to_owned());
                } else {
                    out.mcp_patterns.push((
                        source.cloned(),
                        crate::shell::regex(value).map_err(shell_error)?,
                    ));
                }
            }
            for r in &config.allowed_api_endpoints {
                out.api.push((
                    r.method.clone(),
                    crate::shell::regex(&r.path).map_err(shell_error)?,
                ));
            }
            for p in &config.allowed_write_paths {
                out.writes.push(paths::glob(p.pattern(), &scope.home)?);
            }
            out.blocked
                .extend(config.blocked_tools.iter().map(|p| p.pattern().to_owned()));
        }
        if out.mcp_patterns.len() + out.api.len() + out.writes.len() > 4096 {
            return Err(error("Combined permission rules exceed their budget"));
        }
        out.shell = ShellPolicy::compile(shell).map_err(shell_error)?;
        Ok(out)
    }
}
