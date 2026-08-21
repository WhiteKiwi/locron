//! Normalized target and execution-environment values.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ValidationError;

/// Runnable target snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// Direct executable with preserved argument boundaries.
    Process {
        executable: String,
        args: Vec<String>,
    },
    /// Explicit command interpreted by the selected shell.
    Shell { command: String, shell: PathBuf },
    /// Absolute HTTP request.
    Http(HttpTarget),
}

/// HTTP request target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HttpTarget {
    pub method: HttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub body_file: Option<PathBuf>,
    pub success_statuses: Vec<u16>,
    pub follow_redirects: bool,
}

/// Supported HTTP methods.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl HttpMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
        }
    }
}

/// Environment sources referenced by a job.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub file: Option<PathBuf>,
    pub values: BTreeMap<String, String>,
    pub path: Option<String>,
}

impl Target {
    /// Validates values that can be proven before persistence.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Process { executable, .. } if executable.trim().is_empty() => Err(
                ValidationError::new("executable", "required", "executable cannot be empty"),
            ),
            Self::Shell { command, shell } if command.trim().is_empty() || !shell.is_absolute() => {
                Err(ValidationError::new(
                    "shell",
                    "invalid_shell",
                    "command must be non-empty and shell must be absolute",
                ))
            }
            Self::Http(http) => http.validate(),
            _ => Ok(()),
        }
    }
}

impl HttpTarget {
    fn validate(&self) -> Result<(), ValidationError> {
        if !(self.url.starts_with("http://") || self.url.starts_with("https://")) {
            return Err(ValidationError::new(
                "url",
                "absolute_url_required",
                "URL must begin with http:// or https://",
            ));
        }
        if self.body.is_some() && self.body_file.is_some() {
            return Err(ValidationError::new(
                "body",
                "conflicting_body_sources",
                "inline body and body file are mutually exclusive",
            ));
        }
        if self
            .success_statuses
            .iter()
            .any(|status| !(100..=599).contains(status))
        {
            return Err(ValidationError::new(
                "success_status",
                "out_of_range",
                "status must be from 100 through 599",
            ));
        }
        Ok(())
    }
}

impl Environment {
    /// Rejects reserved runtime metadata names.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if let Some(name) = self.values.keys().find(|name| name.starts_with("LOCRON_")) {
            return Err(ValidationError::new(
                "environment",
                "reserved_name",
                format!("{name} is reserved"),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_reserved_environment() {
        let env = Environment {
            values: BTreeMap::from([("LOCRON_RUN_ID".into(), "x".into())]),
            ..Environment::default()
        };
        assert!(env.validate().is_err());
    }

    #[test]
    fn rejects_relative_http_url_and_two_bodies() {
        let mut target = HttpTarget {
            method: HttpMethod::Get,
            url: "/health".into(),
            headers: BTreeMap::new(),
            body: None,
            body_file: None,
            success_statuses: vec![],
            follow_redirects: false,
        };
        assert!(target.validate().is_err());
        target.url = "https://example.test".into();
        target.body = Some(vec![]);
        target.body_file = Some("body".into());
        assert!(target.validate().is_err());
    }
}
