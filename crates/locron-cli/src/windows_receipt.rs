//! Strict ownership metadata for the Windows standalone distribution.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{Result, ensure};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

pub(super) const RECEIPT: &str = ".locron-install-receipt-v1";
pub(super) const PAYLOADS: [&str; 6] = [
    "locron.exe",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "uninstall.ps1",
    ".locron-installer.ps1",
];

/// Registry kinds whose raw string values can be preserved without expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum PathKind {
    String,
    ExpandString,
}

/// A conditional rollback record, including the distinction between absent and empty PATH.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UserPath {
    #[serde(deserialize_with = "required_nullable")]
    pub before: Option<String>,
    pub after: String,
    #[serde(deserialize_with = "required_nullable")]
    pub before_kind: Option<PathKind>,
    pub after_kind: PathKind,
}

impl UserPath {
    fn validate(&self, directory: &str) -> Result<()> {
        ensure!(
            self.before.is_some() == self.before_kind.is_some(),
            "PATH record confuses an absent and present value"
        );
        ensure!(
            !directory.contains(';') && !self.after.contains('\0'),
            "PATH record contains an ambiguous directory/value"
        );
        ensure!(
            self.before
                .as_ref()
                .is_none_or(|value| !value.contains('\0')),
            "PATH record contains a NUL"
        );
        Ok(())
    }
}

/// Exact installer-owned files and their independently checked final digests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: String,
    pub sid: String,
    pub channel: String,
    pub directory: String,
    pub executable: String,
    pub target: String,
    pub version: String,
    pub archive_url: String,
    pub archive_sha256: String,
    pub binary_sha256: String,
    #[serde(deserialize_with = "strict_files")]
    pub files: BTreeMap<String, String>,
    #[serde(deserialize_with = "required_nullable")]
    pub user_path: Option<UserPath>,
}

pub(super) fn required_nullable<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn strict_files<'de, D>(deserializer: D) -> std::result::Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Files;
    impl<'de> Visitor<'de> for Files {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("the six unique exact Windows installer payload names")
        }

        fn visit_map<M>(self, mut map: M) -> std::result::Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut files = BTreeMap::new();
            while let Some((name, hash)) = map.next_entry::<String, String>()? {
                if !PAYLOADS.contains(&name.as_str()) || files.insert(name, hash).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or unknown receipt payload name",
                    ));
                }
            }
            if files.len() != PAYLOADS.len() {
                return Err(serde::de::Error::custom("missing receipt payload name"));
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Files)
}

pub(super) fn stable_version(version: &str) -> Result<[u64; 3]> {
    let parts = version.split('.').collect::<Vec<_>>();
    ensure!(parts.len() == 3, "expected a stable release version");
    let mut numbers = [0; 3];
    for (index, part) in parts.iter().enumerate() {
        ensure!(
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0')),
            "expected a canonical stable release version"
        );
        numbers[index] = part.parse()?;
    }
    ensure!(numbers >= [0, 10, 0], "release predates Windows support");
    Ok(numbers)
}

pub(super) fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Canonical local-drive spelling, without opening or granting ownership to a path.
pub(super) fn local_path(path: &str) -> Result<String> {
    let ordinary = path.strip_prefix(r"\\?\").unwrap_or(path);
    let bytes = ordinary.as_bytes();
    ensure!(
        bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/')
            && !ordinary[2..].contains(':')
            && !ordinary.chars().any(char::is_control),
        "expected an absolute local-drive path without alternate streams"
    );
    let path = ordinary.replace('/', "\\");
    for component in path[3..].split('\\') {
        ensure!(
            component.is_empty()
                || (!matches!(component, "." | "..")
                    && !component.ends_with(['.', ' '])
                    && !component.contains(['?', '*', '"', '<', '>', '|'])),
            "ambiguous Windows path component"
        );
        let stem = component.split('.').next().unwrap_or_default();
        let stem = stem.to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "reserved Windows path component"
        );
    }
    if path.len() == 3 {
        Ok(path)
    } else {
        Ok(path.trim_end_matches('\\').to_owned())
    }
}

pub(super) fn same_path(left: &str, right: &str) -> Result<bool> {
    // Product-generated paths preserve original spelling. File identity and
    // retained guards remain authoritative for live mutations; a spelling
    // match here cannot authorize replacing an object.
    // Limit spelling equivalence to ASCII case. Unicode lowercase expansion
    // is not Windows ordinal comparison and can merge distinct leaf names.
    Ok(local_path(left)?.eq_ignore_ascii_case(&local_path(right)?))
}

impl Receipt {
    pub(super) fn parse(bytes: &[u8], sid: &str, directory: &str, target: &str) -> Result<Self> {
        ensure!(bytes.len() <= 128 * 1024, "receipt exceeds its size limit");
        let receipt: Self = serde_json::from_slice(bytes)?;
        receipt.validate(sid, directory, target)?;
        Ok(receipt)
    }

    pub(super) fn validate(&self, sid: &str, directory: &str, target: &str) -> Result<()> {
        ensure!(
            self.schema == "locron.install/windows-v1"
                && self.channel == "standalone"
                && self.sid == sid,
            "receipt does not authorize this user and channel"
        );
        ensure!(
            matches!(target, "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc")
                && self.target == target,
            "receipt does not authorize this native architecture"
        );
        stable_version(&self.version)?;
        let directory = local_path(directory)?;
        ensure!(
            same_path(&self.directory, &directory)?
                && same_path(&self.executable, &format!("{directory}\\locron.exe"))?,
            "receipt does not authorize this exact executable"
        );
        let asset = format!("locron-v{}-{}.zip", self.version, self.target);
        ensure!(
            self.archive_url
                == format!(
                    "https://github.com/WhiteKiwi/locron/releases/download/v{}/{asset}",
                    self.version
                )
                && valid_hash(&self.archive_sha256)
                && valid_hash(&self.binary_sha256),
            "receipt has invalid canonical release digests"
        );
        ensure!(
            self.files.len() == PAYLOADS.len()
                && PAYLOADS
                    .iter()
                    .all(|name| { self.files.get(*name).is_some_and(|hash| valid_hash(hash)) })
                && self.files.get("locron.exe") == Some(&self.binary_sha256),
            "receipt file inventory or executable digest disagrees"
        );
        if let Some(path) = &self.user_path {
            path.validate(&directory)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const DIRECTORY: &str = r"C:\test-owned\Unicode 한글";
    const TARGET: &str = "x86_64-pc-windows-msvc";

    fn receipt() -> Value {
        json!({
            "schema": "locron.install/windows-v1", "sid": SID, "channel": "standalone",
            "directory": DIRECTORY, "executable": format!(r"{DIRECTORY}\locron.exe"),
            "target": TARGET, "version": "0.10.0",
            "archive_url": format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-{TARGET}.zip"),
            "archive_sha256": "ab".repeat(32), "binary_sha256": "cd".repeat(32),
            "files": PAYLOADS.iter().map(|name| ((*name).to_owned(), json!(if *name == "locron.exe" { "cd".repeat(32) } else { "ef".repeat(32) }))).collect::<BTreeMap<_, _>>(),
            "user_path": null,
        })
    }

    fn parse(value: &Value) -> Result<Receipt> {
        Receipt::parse(&serde_json::to_vec(value)?, SID, DIRECTORY, TARGET)
    }

    #[test]
    fn receipt_binds_user_channel_native_release_and_exact_directory() {
        let valid = receipt();
        let parsed = parse(&valid).unwrap();
        assert_eq!(parsed.files.len(), 6);
        assert_eq!(RECEIPT, ".locron-install-receipt-v1");
        assert!(
            Receipt::parse(
                &serde_json::to_vec(&valid).unwrap(),
                "other",
                DIRECTORY,
                TARGET
            )
            .is_err()
        );
        assert!(
            Receipt::parse(
                &serde_json::to_vec(&valid).unwrap(),
                SID,
                r"C:\other",
                TARGET
            )
            .is_err()
        );
        assert!(
            Receipt::parse(
                &serde_json::to_vec(&valid).unwrap(),
                SID,
                DIRECTORY,
                "aarch64-pc-windows-msvc"
            )
            .is_err()
        );
        for (field, changed) in [
            ("channel", "winget"),
            ("schema", "locron.install/v1"),
            ("archive_url", "http://github.com/WhiteKiwi/locron"),
            ("executable", r"C:\other\locron.exe"),
            ("version", "0.9.6"),
            ("binary_sha256", "AB"),
        ] {
            let mut value = valid.clone();
            value[field] = json!(changed);
            assert!(parse(&value).is_err(), "{field}");
        }
    }

    #[test]
    fn unknown_duplicate_missing_and_aliased_receipt_fields_refuse() {
        let valid = receipt();
        let mut value = valid.clone();
        value["extra"] = json!(1);
        assert!(parse(&value).is_err());
        let mut missing_nullable = valid.clone();
        missing_nullable
            .as_object_mut()
            .unwrap()
            .remove("user_path");
        assert!(parse(&missing_nullable).is_err());
        for name in ["README.md", "locron.exe"] {
            let mut value = valid.clone();
            value["files"].as_object_mut().unwrap().remove(name);
            assert!(parse(&value).is_err());
        }
        let mut value = valid.clone();
        value["files"]["../foreign"] = json!("ef".repeat(32));
        assert!(parse(&value).is_err());
        let text = serde_json::to_string(&valid).unwrap();
        for modified in [
            text.replacen("{", r#"{"schema":"foreign","#, 1),
            text.replace(
                r#""README.md":"#,
                &format!(r#""README.md":"{}","README.md":"#, "ef".repeat(32)),
            ),
            text.replace("README.md", "README.MD"),
            text.replace("README.md", r"README\u002emd").replacen(
                r#""files":{"#,
                &format!(r#""files":{{"README.md":"{}","#, "ef".repeat(32)),
                1,
            ),
        ] {
            assert!(Receipt::parse(modified.as_bytes(), SID, DIRECTORY, TARGET).is_err());
        }
    }

    #[test]
    fn path_record_preserves_absence_empty_and_expansion_kind_without_coercion() {
        for (before, before_kind) in [
            (Value::Null, Value::Null),
            (json!(""), json!("String")),
            (json!(r"%USERPROFILE%\bin"), json!("ExpandString")),
        ] {
            let mut value = receipt();
            value["user_path"] = json!({"before": before, "before_kind": before_kind,
                "after": DIRECTORY, "after_kind": "ExpandString"});
            let parsed = parse(&value).unwrap();
            let path = parsed.user_path.unwrap();
            assert_eq!(path.after_kind, PathKind::ExpandString);
            assert_eq!(serde_json::to_value(path).unwrap(), value["user_path"]);
        }
        for changed in [
            json!({"before": null, "before_kind": "String", "after": DIRECTORY, "after_kind": "String"}),
            json!({"before": "", "before_kind": null, "after": DIRECTORY, "after_kind": "String"}),
            json!({"before": "", "before_kind": "String", "after": DIRECTORY, "after_kind": "Binary"}),
            json!({"before": "", "before_kind": "String", "after": 1, "after_kind": "String"}),
        ] {
            let mut value = receipt();
            value["user_path"] = changed;
            assert!(parse(&value).is_err());
        }
    }

    #[test]
    fn local_spelling_refuses_streams_devices_and_ambiguous_components() {
        assert!(same_path(r"C:\test\locron.exe", r"\\?\c:\test\LOCRON.exe").unwrap());
        assert!(!same_path(r"C:\İ\locron.exe", "C:\\i\u{0307}\\locron.exe").unwrap());
        for path in [
            r"relative",
            r"\\server\share\locron.exe",
            r"C:\test:stream",
            r"C:\test.\locron.exe",
            r"C:\CON\locron.exe",
            r"C:\a\..\b",
            r"C:\a?\locron.exe",
        ] {
            assert!(local_path(path).is_err(), "{path}");
        }
        for version in [
            "",
            "1",
            "01.10.0",
            "0.10.0-beta",
            "0.10.0+build",
            "../0.10.0",
        ] {
            assert!(stable_version(version).is_err(), "{version}");
        }
    }
}
