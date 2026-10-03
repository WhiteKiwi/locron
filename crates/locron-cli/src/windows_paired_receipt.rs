//! Pure qualification of the unpublished Windows paired receipt.
//! This test-only metadata never substitutes for retained live pair guards.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{Result, ensure};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use super::windows_protocol::maintenance_path;
use super::windows_receipt::{UserPath, required_nullable, same_path, stable_version, valid_hash};

pub(super) const RECEIPT: &str = ".locron-install-receipt-v2";
pub(super) const LAUNCHER_ABI: &str = "native-gui-v1";
pub(super) const EXECUTABLES: [&str; 2] = ["locron.exe", "locron-service-launcher.exe"];
pub(super) const PAYLOADS: [&str; 7] = [
    "locron.exe",
    "locron-service-launcher.exe",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "uninstall.ps1",
    ".locron-installer.ps1",
];

/// Position zero is the console and position one the fixed sibling GUI launcher.
/// Native identities stay in actual live guards/journals, outside this wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Executable {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: String,
    pub sid: String,
    pub channel: String,
    pub directory: String,
    pub executables: [Executable; 2],
    pub target: String,
    pub version: String,
    pub launcher_abi: String,
    pub archive_url: String,
    pub archive_sha256: String,
    #[serde(deserialize_with = "strict_files")]
    pub files: BTreeMap<String, String>,
    #[serde(deserialize_with = "required_nullable")]
    pub user_path: Option<UserPath>,
}

fn strict_files<'de, D>(deserializer: D) -> std::result::Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Files;
    impl<'de> Visitor<'de> for Files {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("the seven unique exact paired Windows payload names")
        }

        fn visit_map<M>(self, mut map: M) -> std::result::Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut files = BTreeMap::new();
            while let Some((name, hash)) = map.next_entry::<String, String>()? {
                if !PAYLOADS.contains(&name.as_str()) || files.insert(name, hash).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate or unknown paired receipt payload name",
                    ));
                }
            }
            if files.len() != PAYLOADS.len() {
                return Err(serde::de::Error::custom(
                    "missing paired receipt payload name",
                ));
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Files)
}

impl Receipt {
    pub(super) fn parse(bytes: &[u8], sid: &str, directory: &str, target: &str) -> Result<Self> {
        ensure!(
            bytes.len() <= 128 * 1024,
            "paired receipt exceeds its size limit"
        );
        let receipt: Self = serde_json::from_slice(bytes)?;
        receipt.validate(sid, directory, target)?;
        Ok(receipt)
    }

    pub(super) fn validate(&self, sid: &str, directory: &str, target: &str) -> Result<()> {
        ensure!(
            self.schema == "locron.install/windows-v2"
                && self.channel == "standalone"
                && self.sid == sid,
            "paired receipt does not bind this user and channel"
        );
        ensure!(
            matches!(target, "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc")
                && self.target == target,
            "paired receipt does not bind this native architecture"
        );
        stable_version(&self.version)?;
        ensure!(
            self.launcher_abi == LAUNCHER_ABI,
            "unknown paired launcher ABI"
        );
        let directory = maintenance_path(directory)?;
        maintenance_path(&self.directory)?;
        ensure!(
            same_path(&self.directory, &directory)?,
            "paired receipt does not bind this exact directory"
        );
        ensure!(
            self.files.len() == PAYLOADS.len()
                && PAYLOADS
                    .iter()
                    .all(|name| { self.files.get(*name).is_some_and(|hash| valid_hash(hash)) }),
            "paired receipt does not contain the exact final seven-file map"
        );
        for (name, executable) in EXECUTABLES.iter().zip(&self.executables) {
            maintenance_path(&executable.path)?;
            ensure!(
                same_path(&executable.path, &format!("{directory}\\{name}"))?
                    && valid_hash(&executable.sha256)
                    && self.files.get(*name) == Some(&executable.sha256),
                "ordered executable path/digest differs from the fixed sibling inventory"
            );
        }
        let asset = format!("locron-v{}-{}.zip", self.version, self.target);
        ensure!(
            self.archive_url
                == format!(
                    "https://github.com/WhiteKiwi/locron/releases/download/v{}/{asset}",
                    self.version
                )
                && valid_hash(&self.archive_sha256),
            "paired receipt has invalid canonical release metadata"
        );
        if let Some(path) = &self.user_path {
            path.validate(&directory)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::windows_receipt::appended_path;
    use super::*;

    const SID: &str = "S-1-5-21-1-2-3-1001";
    const DIRECTORY: &str = r"C:\test-owned\paired 한글";
    const TARGET: &str = "x86_64-pc-windows-msvc";

    fn receipt() -> Value {
        json!({
            "schema": "locron.install/windows-v2", "sid": SID, "channel": "standalone",
            "directory": DIRECTORY,
            "executables": [
                {"path": format!(r"{DIRECTORY}\locron.exe"), "sha256": "cd".repeat(32)},
                {"path": format!(r"{DIRECTORY}\locron-service-launcher.exe"), "sha256": "ef".repeat(32)},
            ],
            "target": TARGET, "version": "0.10.0", "launcher_abi": LAUNCHER_ABI,
            "archive_url": format!("https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-{TARGET}.zip"),
            "archive_sha256": "ab".repeat(32),
            "files": PAYLOADS.iter().map(|name| ((*name).to_owned(), json!(match *name {
                "locron.exe" => "cd".repeat(32),
                "locron-service-launcher.exe" => "ef".repeat(32),
                _ => "12".repeat(32),
            }))).collect::<BTreeMap<_, _>>(),
            "user_path": null,
        })
    }

    fn parse(value: &Value) -> Result<Receipt> {
        Receipt::parse(&serde_json::to_vec(value)?, SID, DIRECTORY, TARGET)
    }

    #[test]
    fn ordered_pair_and_seven_file_map_roundtrip_without_persistent_native_ids() {
        let original = receipt();
        let decoded = parse(&original).unwrap();
        assert_eq!(RECEIPT, ".locron-install-receipt-v2");
        assert_eq!(decoded.files.len(), 7);
        assert_eq!(decoded.executables[0].sha256, decoded.files[EXECUTABLES[0]]);
        assert_eq!(decoded.executables[1].sha256, decoded.files[EXECUTABLES[1]]);
        let encoded = serde_json::to_value(&decoded).unwrap();
        assert_eq!(encoded, original);
        assert_eq!(encoded.as_object().unwrap().len(), 12);
        for executable in encoded["executables"].as_array().unwrap() {
            assert_eq!(executable.as_object().unwrap().len(), 2);
            assert!(executable.get("path").is_some());
            assert!(executable.get("sha256").is_some());
        }
    }

    #[test]
    fn pair_refuses_swapped_missing_extra_duplicate_and_wrong_sibling_bindings() {
        let original = receipt();
        let mut swapped = original.clone();
        swapped["executables"].as_array_mut().unwrap().swap(0, 1);
        assert!(parse(&swapped).is_err());
        let mut missing = original.clone();
        missing["executables"].as_array_mut().unwrap().pop();
        assert!(parse(&missing).is_err());
        let mut extra = original.clone();
        let console = extra["executables"][0].clone();
        extra["executables"].as_array_mut().unwrap().push(console);
        assert!(parse(&extra).is_err());
        let mut repeated = original.clone();
        repeated["executables"][1] = repeated["executables"][0].clone();
        assert!(parse(&repeated).is_err());
        for index in 0..2 {
            for changed in [
                r"C:\other\locron.exe",
                r"C:\test-owned\paired 한글\foreign.exe",
                r"C:\test-owned\paired 한글\locron-service-launcher.exe:stream",
            ] {
                let mut value = original.clone();
                value["executables"][index]["path"] = json!(changed);
                assert!(parse(&value).is_err());
            }
            let mut value = original.clone();
            value["executables"][index]["sha256"] = json!("00".repeat(32));
            assert!(parse(&value).is_err());
        }
    }

    #[test]
    fn v1_fields_unknown_abi_native_ids_and_malformed_pair_types_refuse() {
        for (field, changed) in [
            ("schema", json!("locron.install/windows-v1")),
            ("launcher_abi", json!("native-gui-v2")),
            ("executable", json!(format!(r"{DIRECTORY}\locron.exe"))),
            ("binary_sha256", json!("cd".repeat(32))),
            ("executables", Value::Null),
            ("executables", json!({"path": "foreign"})),
        ] {
            let mut value = receipt();
            value[field] = changed;
            assert!(parse(&value).is_err(), "{field}");
        }
        for field in ["path", "sha256"] {
            let mut value = receipt();
            value["executables"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(parse(&value).is_err());
        }
        for field in ["role", "name", "volume", "file", "identity"] {
            let mut value = receipt();
            value["executables"][1][field] = json!("unaccepted");
            assert!(parse(&value).is_err(), "{field}");
        }
    }

    #[test]
    fn map_and_wire_duplicates_missing_names_aliases_and_trailing_json_refuse() {
        let original = receipt();
        for name in PAYLOADS {
            let mut value = original.clone();
            value["files"].as_object_mut().unwrap().remove(name);
            assert!(parse(&value).is_err(), "{name}");
        }
        let mut foreign = original.clone();
        foreign["files"]["extra.exe"] = json!("00".repeat(32));
        assert!(parse(&foreign).is_err());
        let text = serde_json::to_string(&original).unwrap();
        for altered in [
            text.replacen("{", r#"{"schema":"foreign","#, 1),
            text.replace(
                r#""README.md":"#,
                &format!(r#""README.md":"{}","README.md":"#, "12".repeat(32)),
            ),
            text.replacen(r#""path":"#, r#""path":"C:\\other","path":"#, 1),
            text.replace("README.md", "README.MD"),
            text.replace("README.md", r"README\u002emd").replacen(
                r#""files":{"#,
                &format!(r#""files":{{"README.md":"{}","#, "12".repeat(32)),
                1,
            ),
            format!("{text}{{}}"),
        ] {
            assert!(Receipt::parse(altered.as_bytes(), SID, DIRECTORY, TARGET).is_err());
        }
    }

    #[test]
    fn account_directory_target_stable_version_and_canonical_release_are_bound() {
        let original = receipt();
        let bytes = serde_json::to_vec(&original).unwrap();
        assert!(Receipt::parse(&bytes, "another", DIRECTORY, TARGET).is_err());
        assert!(Receipt::parse(&bytes, SID, r"C:\another", TARGET).is_err());
        assert!(Receipt::parse(&bytes, SID, DIRECTORY, "aarch64-pc-windows-msvc").is_err());
        for (field, changed) in [
            ("channel", "winget"),
            ("version", "0.9.6"),
            ("version", "0.10.0-beta"),
            ("target", "x86_64-pc-windows-gnu"),
            ("archive_url", "http://github.com/WhiteKiwi/locron"),
            ("archive_sha256", "AB"),
        ] {
            let mut value = original.clone();
            value[field] = json!(changed);
            assert!(parse(&value).is_err(), "{field}");
        }
        let mut arm = original;
        arm["target"] = json!("aarch64-pc-windows-msvc");
        arm["archive_url"] = json!(
            "https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-aarch64-pc-windows-msvc.zip"
        );
        Receipt::parse(
            &serde_json::to_vec(&arm).unwrap(),
            SID,
            DIRECTORY,
            "aarch64-pc-windows-msvc",
        )
        .unwrap();
    }

    #[test]
    fn user_path_preserves_missing_empty_raw_value_and_kind_without_expansion() {
        for (before, before_kind) in [
            (Value::Null, Value::Null),
            (json!(""), json!("String")),
            (json!(r"%USERPROFILE%\bin"), json!("ExpandString")),
        ] {
            let mut value = receipt();
            let kind = if before_kind.is_null() {
                json!("String")
            } else {
                before_kind.clone()
            };
            value["user_path"] = json!({"before":before,"before_kind":before_kind,
                "after":appended_path(before.as_str(), DIRECTORY),"after_kind":kind});
            assert_eq!(serde_json::to_value(parse(&value).unwrap()).unwrap(), value);
        }
        let mut missing = receipt();
        missing.as_object_mut().unwrap().remove("user_path");
        assert!(parse(&missing).is_err());
        let mut inconsistent = receipt();
        inconsistent["user_path"] = json!({"before":null,"before_kind":"String",
            "after":DIRECTORY,"after_kind":"String"});
        assert!(parse(&inconsistent).is_err());
    }

    #[test]
    fn oversized_wire_and_over_cap_executable_path_refuse_before_any_guard() {
        let mut value = receipt();
        value["archive_url"] = json!("x".repeat(128 * 1024));
        assert!(parse(&value).is_err());
        let directory = format!("C:\\{}", "a".repeat(4090));
        let mut value = receipt();
        value["directory"] = json!(directory);
        for (index, name) in EXECUTABLES.iter().enumerate() {
            value["executables"][index]["path"] = json!(format!("{directory}\\{name}"));
        }
        assert!(
            Receipt::parse(
                &serde_json::to_vec(&value).unwrap(),
                SID,
                &directory,
                TARGET
            )
            .is_err()
        );
    }
}
