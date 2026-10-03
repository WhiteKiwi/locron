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
        #[cfg(not(windows))]
        let content =
            std::fs::read_to_string(file).map_err(|error| format!("environment file: {error}"))?;
        #[cfg(windows)]
        let content = String::from_utf8(
            read_input_file(file).map_err(|error| format!("environment file: {error}"))?,
        )
        .map_err(|error| format!("environment file: {error}"))?;
        apply_environment_layer(&mut environment, &parse_environment_file(&content)?)?;
    }
    apply_environment_layer(&mut environment, &job.values)?;
    Ok(environment)
}

/// Reads one user-selected input while Windows no-follow guards remain retained.
pub fn read_input_file(path: &Path) -> std::io::Result<Vec<u8>> {
    #[cfg(not(windows))]
    {
        std::fs::read(path)
    }
    #[cfg(windows)]
    {
        use std::io::Read;
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut file = crate::filesystem::open_read_no_follow(&absolute)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
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

#[cfg(windows)]
fn is_ambiguous_windows_path(path: &Path) -> bool {
    !path.is_absolute()
        && (path.has_root()
            || matches!(path.components().next(), Some(std::path::Component::Prefix(_))))
}

/// Resolves only the effective PATH/PATHEXT, relative entries being based on job cwd.
/// Ambiguous Windows drive-relative/root-relative PATH entries are skipped.
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
    if is_ambiguous_windows_path(requested) {
        // These forms do not resolve relative to the complete job cwd.
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
        let search_path = environment_value(environment, "PATH").map_or("", String::as_str);
        let directories = std::env::split_paths(search_path);
        #[cfg(windows)]
        // Joining C:bin would discard cwd and consult the ambient drive directory.
        let directories = directories.filter(|directory| !is_ambiguous_windows_path(directory));
        directories
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

    #[cfg(windows)]
    #[test]
    fn windows_path_admission_preserves_relative_absolute_and_unc_forms() {
        for path in [r"C:bin", "C:", r"\bin", "/bin"] {
            assert!(is_ambiguous_windows_path(Path::new(path)), "{path}");
        }
        // UNC cases exercise path admission without requiring a live network share.
        for path in [
            "",
            ".",
            r"工具 bin",
            r"..\bin",
            r"C:\tools",
            "C:/tools",
            r"\\server\share\bin",
            r"\\?\C:\tools",
            r"\\?\UNC\server\share\bin",
        ] {
            assert!(!is_ambiguous_windows_path(Path::new(path)), "{path}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_search_ignores_ambient_directories() {
        const CHILD_ROOT: &str = "LOCRON_TEST_PATH_SEARCH_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            assert_windows_path_search_ignores_ambient_directories(Path::new(&root));
            std::fs::write(Path::new(&root).join("verified"), b"job cwd preserved").unwrap();
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let ambient = root.path().join("ambient");
        let job = root.path().join("job");
        for directory in [
            ambient.clone(),
            ambient.join("bin"),
            job.clone(),
            job.join("工具 bin"),
        ] {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("tool.ExE"), b"resolution fixture").unwrap();
        }
        // Only this disposable child changes its cwd/environment, not the parallel test host.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "execution::tests::windows_path_search_ignores_ambient_directories",
                "--nocapture",
            ])
            .env(CHILD_ROOT, std::fs::canonicalize(root.path()).unwrap())
            .current_dir(&ambient)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read(root.path().join("verified")).unwrap(),
            b"job cwd preserved"
        );
    }

    #[cfg(windows)]
    fn assert_windows_path_search_ignores_ambient_directories(root: &Path) {
        use std::path::{Component, Prefix};

        let cwd = root.join("job");
        let ambient = std::env::current_dir().unwrap();
        assert_ne!(
            std::fs::canonicalize(&cwd).unwrap(),
            std::fs::canonicalize(&ambient).unwrap()
        );
        let drive = match ambient.components().next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => char::from(drive),
                _ => panic!("isolated fixture must use a local drive"),
            },
            _ => panic!("isolated fixture must have a drive prefix"),
        };
        let rooted_bin: PathBuf = ambient.join("bin").components().skip(1).collect();
        let job_bin = cwd.join("工具 bin");
        let selected = std::fs::canonicalize(job_bin.join("tool.ExE")).unwrap();
        let entries = [
            (format!("{drive}:bin"), ambient.join("bin")),
            (format!("{drive}:"), ambient.clone()),
            (rooted_bin.to_str().unwrap().to_owned(), ambient.join("bin")),
        ];
        for (entry, ambient_directory) in entries {
            // Establish the actual filesystem candidate admitted by the former cwd.join path.
            assert_eq!(
                std::fs::canonicalize(cwd.join(&entry).join("tool.ExE")).unwrap(),
                std::fs::canonicalize(ambient_directory.join("tool.ExE")).unwrap()
            );
            let mut environment = BTreeMap::from([
                ("PATH".into(), entry.clone()),
                ("PATHEXT".into(), ".EXE".into()),
            ]);
            assert!(
                resolve_executable("tool", &cwd, &environment).is_none(),
                "{entry}"
            );
            for directory in [Path::new("工具 bin"), job_bin.as_path()] {
                let path = std::env::join_paths([Path::new(&entry), directory])
                    .unwrap()
                    .into_string()
                    .unwrap();
                environment.insert("PATH".into(), path);
                assert_eq!(
                    resolve_executable("tool", &cwd, &environment),
                    Some(selected.clone()),
                    "ambiguous entry {entry} displaced {directory:?}"
                );
            }
        }
        let environment = BTreeMap::from([
            ("PATH".into(), String::new()),
            ("PATHEXT".into(), ".EXE".into()),
        ]);
        assert_eq!(
            resolve_executable("tool", &cwd, &environment),
            Some(std::fs::canonicalize(cwd.join("tool.ExE")).unwrap())
        );
    }
}
