//! Shared, deterministic execution configuration for every application surface.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::target::is_valid_environment_name;

/// Materializes the same layered environment for execution and advisory probes.
pub fn effective_environment(
    execution_path: &str,
    global: &BTreeMap<String, String>,
    job: &crate::target::Environment,
) -> Result<BTreeMap<String, String>, String> {
    let mut environment = minimal_environment();
    environment.insert("PATH".into(), execution_path.into());
    apply_environment_layer(&mut environment, global)?;
    if let Some(path) = &job.path {
        environment.insert("PATH".into(), path.clone());
    }
    if let Some(file) = &job.file {
        let content =
            std::fs::read_to_string(file).map_err(|error| format!("environment file: {error}"))?;
        apply_environment_layer(&mut environment, &parse_environment_file(&content)?)?;
    }
    apply_environment_layer(&mut environment, &job.values)?;
    Ok(environment)
}

/// Canonical key used to compare and merge environment names on this platform.
#[must_use]
pub fn environment_name(name: &str) -> String {
    if cfg!(windows) {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    }
}

/// Returns a value using the platform's environment-name comparison rule.
#[must_use]
pub fn environment_value<'a>(
    environment: &'a BTreeMap<String, String>,
    name: &str,
) -> Option<&'a String> {
    let name = environment_name(name);
    environment
        .iter()
        .find_map(|(key, value)| (environment_name(key) == name).then_some(value))
}

/// Runtime metadata is reserved even when Windows callers vary its casing.
#[must_use]
pub fn is_reserved_environment_name(name: &str) -> bool {
    environment_name(name).starts_with("LOCRON_")
}

/// Validates one configuration layer before its names are normalized or merged.
pub fn validate_environment_layer(environment: &BTreeMap<String, String>) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for (name, value) in environment {
        if !is_valid_environment_name(name) || is_reserved_environment_name(name) {
            return Err(format!("invalid or reserved environment name {name}"));
        }
        if value.contains('\0') {
            return Err(format!("environment value for {name} contains NUL"));
        }
        if !names.insert(environment_name(name)) {
            return Err(format!(
                "environment names collide in one configuration layer: {name}"
            ));
        }
    }
    Ok(())
}

/// Applies a validated layer, overriding earlier values by canonical key.
pub fn apply_environment_layer(
    environment: &mut BTreeMap<String, String>,
    layer: &BTreeMap<String, String>,
) -> Result<(), String> {
    validate_environment_layer(layer)?;
    for (name, value) in layer {
        let key = environment_name(name);
        environment.retain(|existing, _| environment_name(existing) != key);
        environment.insert(key, value.clone());
    }
    Ok(())
}

/// Parses a plain KEY=VALUE file without shell expansion or startup scripts.
pub fn parse_environment_file(content: &str) -> Result<BTreeMap<String, String>, String> {
    let mut layer = BTreeMap::new();
    let mut names = BTreeSet::new();
    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| format!("environment file line {} is malformed", index + 1))?;
        if cfg!(windows) && !names.insert(environment_name(name)) {
            return Err(format!(
                "environment file repeats a name in one layer: {name}"
            ));
        }
        layer.insert(name.to_owned(), value.to_owned());
    }
    validate_environment_layer(&layer)?;
    Ok(layer)
}

/// Captures only the platform's small runtime environment whitelist.
#[must_use]
pub fn minimal_environment() -> BTreeMap<String, String> {
    let names: &[&str] = if cfg!(windows) {
        &[
            "SYSTEMROOT",
            "COMSPEC",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "LOCALAPPDATA",
            "APPDATA",
            "USERNAME",
            "PATH",
            "PATHEXT",
        ]
    } else {
        &[
            "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TMPDIR", "PATH",
        ]
    };
    std::env::vars_os()
        .filter_map(|(name, value)| {
            let key = environment_name(&name.into_string().ok()?);
            names
                .contains(&key.as_str())
                .then(|| value.into_string().ok().map(|value| (key, value)))
                .flatten()
        })
        .collect()
}

/// Stock non-interactive shell used when a job does not choose one explicitly.
pub fn default_shell() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is unavailable")?;
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err("SystemRoot must be absolute".into());
        }
        Ok(root.join("System32").join("cmd.exe"))
    }
    #[cfg(not(windows))]
    {
        Ok(PathBuf::from("/bin/sh"))
    }
}

/// Initial owned execution PATH; existing persisted settings remain authoritative.
#[must_use]
pub fn default_execution_path() -> String {
    if cfg!(windows) {
        std::env::var("PATH").unwrap_or_default()
    } else {
        "/usr/local/bin:/usr/bin:/bin".into()
    }
}

/// Constructs explicit shell-family arguments with interactive profiles disabled.
pub fn shell_arguments(shell: &Path, command: &str) -> Result<Vec<String>, String> {
    if command.contains('\0') {
        return Err("shell command contains NUL".into());
    }
    #[cfg(windows)]
    {
        let name = shell
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or("shell executable name is not valid UTF-8")?
            .to_ascii_lowercase();
        let prefix: &[&str] = match name.as_str() {
            "cmd" => &["/D", "/S", "/C"],
            "powershell" | "pwsh" => &["-NoProfile", "-NonInteractive", "-Command"],
            "sh" | "bash" | "dash" | "zsh" | "ksh" | "ash" => &["-c"],
            _ => {
                return Err(format!(
                    "unsupported Windows shell family: {}",
                    shell.display()
                ));
            }
        };
        Ok(prefix
            .iter()
            .map(|value| (*value).to_owned())
            .chain([command.to_owned()])
            .collect())
    }
    #[cfg(not(windows))]
    {
        let _ = shell;
        Ok(vec!["-c".into(), command.into()])
    }
}

/// Rejects implicit batch interpretation in a direct argv target.
pub fn validate_direct_executable(executable: &Path) -> Result<(), String> {
    if cfg!(windows)
        && executable
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            })
    {
        return Err("batch targets require explicit shell execution with cmd.exe".into());
    }
    Ok(())
}

/// Resolves only the effective PATH/PATHEXT, relative entries being based on job cwd.
#[must_use]
pub fn resolve_executable(
    executable: &str,
    cwd: &Path,
    environment: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    if executable.is_empty() || executable.contains('\0') || !cwd.is_absolute() {
        return None;
    }
    let requested = Path::new(executable);
    #[cfg(windows)]
    if requested.has_root() && !requested.is_absolute()
        || (matches!(
            requested.components().next(),
            Some(std::path::Component::Prefix(_))
        ) && !requested.is_absolute())
    {
        // A drive-relative/root-relative command would consult process-global drive cwd.
        return None;
    }
    let bearing = requested.is_absolute() || requested.components().count() > 1;
    let candidates = if bearing {
        vec![if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            cwd.join(requested)
        }]
    } else {
        std::env::split_paths(environment_value(environment, "PATH").map_or("", String::as_str))
            .map(|directory| {
                if directory.is_absolute() {
                    directory
                } else {
                    cwd.join(directory)
                }
                .join(requested)
            })
            .collect()
    };
    #[cfg(windows)]
    {
        for candidate in candidates {
            let mut names = vec![candidate.clone()];
            if candidate.extension().is_none() {
                let extensions = environment_value(environment, "PATHEXT")
                    .map_or(".COM;.EXE;.BAT;.CMD", String::as_str);
                names.extend(
                    extensions
                        .split(';')
                        .filter(|extension| {
                            extension.starts_with('.')
                                && extension[1..]
                                    .bytes()
                                    .all(|byte| byte.is_ascii_alphanumeric())
                                && extension.len() > 1
                        })
                        .map(|extension| {
                            let mut name = candidate.as_os_str().to_os_string();
                            name.push(extension);
                            PathBuf::from(name)
                        }),
                );
            }
            for name in names {
                if name.is_file() {
                    return std::fs::canonicalize(name).ok();
                }
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        candidates.into_iter().find(|candidate| candidate.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_duplicate_and_reserved_names() {
        if cfg!(windows) {
            assert!(parse_environment_file("TOKEN=a\nTOKEN=b").is_err());
        } else {
            assert_eq!(
                parse_environment_file("TOKEN=a\nTOKEN=b").unwrap()["TOKEN"],
                "b"
            );
        }
        assert!(parse_environment_file("LOCRON_RUN_ID=x").is_err());
        assert!(parse_environment_file("A=x\n# ignored\nB=x=y").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn windows_layers_are_case_insensitive_and_unambiguous() {
        assert!(parse_environment_file("Path=a\nPATH=b").is_err());
        assert!(parse_environment_file("lOcRoN_RUN_ID=x").is_err());
        let mut environment = BTreeMap::from([("Path".into(), "first".into())]);
        apply_environment_layer(
            &mut environment,
            &BTreeMap::from([("path".into(), "last".into())]),
        )
        .unwrap();
        assert_eq!(environment.len(), 1);
        assert_eq!(environment_value(&environment, "Path").unwrap(), "last");
        assert!(validate_direct_executable(Path::new("script.CmD")).is_err());
        assert_eq!(
            shell_arguments(Path::new(r"C:\Windows\System32\cmd.exe"), "echo a&echo b").unwrap(),
            ["/D", "/S", "/C", "echo a&echo b"]
        );
        assert_eq!(
            shell_arguments(
                Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
                "exit 7"
            )
            .unwrap(),
            ["-NoProfile", "-NonInteractive", "-Command", "exit 7"]
        );
        assert!(shell_arguments(Path::new(r"C:\unknown.exe"), "echo a").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_resolution_uses_job_cwd_pathext_and_absolute_drive_paths() {
        let root = std::env::temp_dir().join(format!("locron-resolution-{}", uuid::Uuid::now_v7()));
        let bin = root.join("工具 bin");
        std::fs::create_dir_all(&bin).unwrap();
        let executable = bin.join("tool.ExE");
        std::fs::write(&executable, b"resolution fixture").unwrap();
        let env = BTreeMap::from([
            ("Path".into(), "工具 bin".into()),
            ("Pathext".into(), ".EXE;.CMD".into()),
        ]);
        let resolved = resolve_executable("tool", &root, &env).unwrap();
        assert_eq!(resolved, std::fs::canonicalize(&executable).unwrap());
        assert_eq!(
            resolve_executable(executable.to_str().unwrap(), &root, &BTreeMap::new()),
            Some(resolved)
        );
        assert!(resolve_executable("C:tool", &root, &env).is_none());
        assert!(resolve_executable("\\tool", &root, &env).is_none());
        assert!(resolve_executable("missing", &root, &env).is_none());
        assert!(resolve_executable("tool", &root, &BTreeMap::new()).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
