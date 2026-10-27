use std::fmt;

/// Error shown to the user. IPC carries it as a string: a one-line summary,
/// then a blank line and the raw output behind it (the frontend `ErrorText`
/// shows the summary and keeps the details behind a toggle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError {
    pub summary: String,
    pub details: Option<String>,
}

impl AgentError {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            details: None,
        }
    }

    /// Empty details are dropped so the string never ends in a dangling blank line.
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        let details = details.into();
        self.details = (!details.trim().is_empty()).then_some(details);
        self
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary)?;
        if let Some(details) = &self.details {
            write!(f, "\n\n{details}")?;
        }
        Ok(())
    }
}

impl From<AgentError> for String {
    fn from(error: AgentError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_and_details_are_separated_by_one_blank_line() {
        assert_eq!(
            String::from(AgentError::new("Codex cannot run")),
            "Codex cannot run"
        );
        assert_eq!(
            AgentError::new("Codex cannot run")
                .with_details("raw\nstderr")
                .to_string(),
            "Codex cannot run\n\nraw\nstderr"
        );
        assert_eq!(
            AgentError::new("Codex cannot run")
                .with_details("  \n")
                .to_string(),
            "Codex cannot run"
        );
    }
}
