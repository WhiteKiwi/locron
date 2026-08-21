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
    pub headers: BTreeMap<String, HttpHeaderSource>,
    pub body: Option<Vec<u8>>,
    pub body_file: Option<PathBuf>,
    pub success_statuses: Vec<u16>,
    pub follow_redirects: bool,
}

/// Source for one HTTP request header value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", content = "value", rename_all = "snake_case")]
pub enum HttpHeaderSource {
    /// Plaintext inline configuration.
    Inline(String),
    /// Name read from the effective attempt environment.
    Environment(String),
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
            Self::Process { executable, args }
                if executable.trim().is_empty()
                    || executable.contains('\0')
                    || args.iter().any(|argument| argument.contains('\0')) =>
            {
                Err(ValidationError::new(
                    "process",
                    "invalid_argument_vector",
                    "executable must be non-empty and argv cannot contain NUL",
                ))
            }
            Self::Process { executable, .. }
                if executable.contains('/') && !PathBuf::from(executable).is_absolute() =>
            {
                Err(ValidationError::new(
                    "executable",
                    "absolute_path_required",
                    "an executable containing a path separator must be absolute",
                ))
            }
            Self::Shell { command, shell }
                if command.trim().is_empty() || command.contains('\0') || !shell.is_absolute() =>
            {
                Err(ValidationError::new(
                    "shell",
                    "invalid_shell",
                    "command must be non-empty without NUL and shell must be absolute",
                ))
            }
            Self::Http(http) => http.validate(),
            _ => Ok(()),
        }
    }
}

impl HttpTarget {
    fn validate(&self) -> Result<(), ValidationError> {
        let url = url::Url::parse(&self.url).map_err(|error| {
            ValidationError::new("url", "absolute_url_required", error.to_string())
        })?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ValidationError::new(
                "url",
                "absolute_url_required",
                "URL must be absolute HTTP or HTTPS",
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
            .body_file
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        {
            return Err(ValidationError::new(
                "body_file",
                "absolute_path_required",
                "HTTP body file must be absolute",
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
        let mut normalized_names = std::collections::BTreeSet::new();
        for (name, source) in &self.headers {
            if !is_valid_http_header_name(name) {
                return Err(ValidationError::new(
                    "header",
                    "invalid_name",
                    format!("invalid HTTP header name: {name}"),
                ));
            }
            if !normalized_names.insert(name.to_ascii_lowercase()) {
                return Err(ValidationError::new(
                    "header",
                    "duplicate_name",
                    format!("duplicate case-insensitive HTTP header: {name}"),
                ));
            }
            match source {
                HttpHeaderSource::Inline(value) if value.contains(['\r', '\n', '\0']) => {
                    return Err(ValidationError::new(
                        "header",
                        "invalid_value",
                        format!("header {name} contains a prohibited character"),
                    ));
                }
                HttpHeaderSource::Environment(environment)
                    if !is_valid_environment_name(environment) =>
                {
                    return Err(ValidationError::new(
                        "header_env",
                        "invalid_environment_name",
                        format!("invalid environment name: {environment}"),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl Environment {
    /// Rejects reserved runtime metadata names.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.file.as_ref().is_some_and(|path| !path.is_absolute()) {
            return Err(ValidationError::new(
                "environment_file",
                "absolute_path_required",
                "environment file must be absolute",
            ));
        }
        for (name, value) in &self.values {
            if !is_valid_environment_name(name) {
                return Err(ValidationError::new(
                    "environment",
                    "invalid_name",
                    format!("invalid environment name: {name}"),
                ));
            }
            if name.starts_with("LOCRON_") {
                return Err(ValidationError::new(
                    "environment",
                    "reserved_name",
                    format!("{name} is reserved"),
                ));
            }
            if value.contains('\0') {
                return Err(ValidationError::new(
                    "environment",
                    "invalid_value",
                    format!("{name} contains NUL"),
                ));
            }
        }
        if self.path.as_deref().is_some_and(|path| path.contains('\0')) {
            return Err(ValidationError::new(
                "path",
                "invalid_value",
                "PATH contains NUL",
            ));
        }
        Ok(())
    }
}

/// Returns whether a name is valid in the normalized attempt environment.
#[must_use]
pub fn is_valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first == b'_' || first.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

/// Returns whether a name follows the HTTP field-name token grammar.
#[must_use]
pub fn is_valid_http_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
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

    #[test]
    fn rejects_invalid_environment_and_header_sources() {
        let env = Environment {
            values: BTreeMap::from([("1INVALID".into(), "value".into())]),
            ..Environment::default()
        };
        assert!(env.validate().is_err());

        let mut target = HttpTarget {
            method: HttpMethod::Get,
            url: "https://example.test".into(),
            headers: BTreeMap::from([(
                "X-Token".into(),
                HttpHeaderSource::Environment("INVALID-NAME".into()),
            )]),
            body: None,
            body_file: None,
            success_statuses: vec![],
            follow_redirects: false,
        };
        assert!(target.validate().is_err());
        target.headers = BTreeMap::from([(
            "Bad Header".into(),
            HttpHeaderSource::Inline("value".into()),
        )]);
        assert!(target.validate().is_err());
        target.headers = BTreeMap::from([(
            "X-Token".into(),
            HttpHeaderSource::Inline("line\r\nbreak".into()),
        )]);
        assert!(target.validate().is_err());
    }

    #[test]
    fn accepts_typed_inline_and_environment_http_headers() {
        let target = HttpTarget {
            method: HttpMethod::Post,
            url: "https://example.test/hook".into(),
            headers: BTreeMap::from([
                (
                    "Content-Type".into(),
                    HttpHeaderSource::Inline("application/json".into()),
                ),
                (
                    "X-Token".into(),
                    HttpHeaderSource::Environment("TOKEN".into()),
                ),
            ]),
            body: Some(br#"{"ok":true}"#.to_vec()),
            body_file: None,
            success_statuses: vec![200, 204],
            follow_redirects: true,
        };
        assert!(target.validate().is_ok());
    }

    #[test]
    fn normalized_domain_rejects_relative_runtime_paths() {
        let environment = Environment {
            file: Some("relative.env".into()),
            ..Environment::default()
        };
        assert!(environment.validate().is_err());

        let process = Target::Process {
            executable: "./relative-command".into(),
            args: vec![],
        };
        assert!(process.validate().is_err());

        let http = Target::Http(HttpTarget {
            method: HttpMethod::Post,
            url: "https://example.test".into(),
            headers: BTreeMap::new(),
            body: None,
            body_file: Some("relative.body".into()),
            success_statuses: vec![],
            follow_redirects: false,
        });
        assert!(http.validate().is_err());
    }

    #[test]
    fn process_and_shell_reject_nul_before_persistence() {
        assert!(
            Target::Process {
                executable: "/usr/bin/printf".into(),
                args: vec!["bad\0argument".into()],
            }
            .validate()
            .is_err()
        );
        assert!(
            Target::Shell {
                command: "bad\0command".into(),
                shell: "/bin/sh".into(),
            }
            .validate()
            .is_err()
        );
    }
}
