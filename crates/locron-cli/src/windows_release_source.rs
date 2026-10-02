//! Canonical immutable release identity for the native Windows frontend.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};
use serde::Deserialize;
use url::Url;

use super::windows_receipt::{stable_version, valid_hash};

const RELEASE_BASE: &str = "https://github.com/WhiteKiwi/locron/releases/download";

#[derive(Debug, Deserialize)]
struct Metadata {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: String,
}

pub(super) struct Release {
    pub version: String,
    pub digests: BTreeMap<String, String>,
}

/// The version-aware binary inventory used in SHA256SUMS, without installer scripts.
pub(super) fn payload_inventory(version: &str) -> Result<BTreeSet<String>> {
    stable_version(version)?;
    let mut names = BTreeSet::new();
    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
    ] {
        names.insert(format!("locron-v{version}-{target}.tar.gz"));
    }
    for arch in ["amd64", "arm64"] {
        names.insert(format!("locron_{version}-1_{arch}.deb"));
    }
    for arch in ["aarch64", "x86_64"] {
        names.insert(format!("locron-{version}-1.{arch}.rpm"));
    }
    for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
        names.insert(format!("locron-v{version}-{target}.zip"));
    }
    Ok(names)
}

impl Release {
    /// Ignore unrelated GitHub metadata, while requiring its exact final asset identity.
    pub(super) fn parse(bytes: &[u8], selected: Option<&str>) -> Result<Self> {
        ensure!(
            bytes.len() <= 1024 * 1024,
            "release metadata exceeds its size limit"
        );
        let metadata: Metadata = serde_json::from_slice(bytes)?;
        ensure!(
            !metadata.draft && !metadata.prerelease,
            "release is not published stable"
        );
        let version = metadata
            .tag_name
            .strip_prefix('v')
            .ok_or_else(|| anyhow::anyhow!("release tag is not canonical"))?;
        stable_version(version)?;
        if let Some(selected) = selected {
            ensure!(
                version == selected,
                "release differs from the selected version"
            );
        }
        let mut expected = payload_inventory(version)?;
        expected.extend(
            [
                "SHA256SUMS.txt",
                "install.sh",
                "install.ps1",
                "uninstall.ps1",
            ]
            .map(str::to_owned),
        );
        ensure!(
            metadata.assets.len() == expected.len(),
            "release has unexpected assets"
        );
        let mut digests = BTreeMap::new();
        for asset in metadata.assets {
            ensure!(
                expected.remove(&asset.name),
                "duplicate or unexpected release asset"
            );
            let hash = asset
                .digest
                .strip_prefix("sha256:")
                .ok_or_else(|| anyhow::anyhow!("release asset lacks its final SHA-256"))?;
            let hash = hash.to_ascii_lowercase();
            ensure!(valid_hash(&hash), "malformed final release digest");
            ensure!(
                asset.browser_download_url == format!("{RELEASE_BASE}/v{version}/{}", asset.name),
                "release asset is outside its canonical repository/tag/name"
            );
            digests.insert(asset.name, hash);
        }
        ensure!(expected.is_empty(), "release has missing assets");
        Ok(Self {
            version: version.to_owned(),
            digests,
        })
    }

    pub(super) fn asset_url(&self, name: &str) -> Result<String> {
        ensure!(
            self.digests.contains_key(name),
            "asset is not in the exact release inventory"
        );
        Ok(format!("{RELEASE_BASE}/v{}/{name}", self.version))
    }

    /// Require the entire checksum inventory to agree with final API digests.
    pub(super) fn checksums(&self, bytes: &[u8]) -> Result<BTreeMap<String, String>> {
        ensure!(
            bytes.len() <= 128 * 1024,
            "release checksums exceed their size limit"
        );
        let expected = payload_inventory(&self.version)?;
        let mut sums = BTreeMap::new();
        for line in std::str::from_utf8(bytes)?.lines() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            let hash = line
                .get(..64)
                .ok_or_else(|| anyhow::anyhow!("short checksum entry"))?;
            let separator = line
                .get(64..66)
                .ok_or_else(|| anyhow::anyhow!("short checksum separator"))?;
            let name = line
                .get(66..)
                .ok_or_else(|| anyhow::anyhow!("short checksum name"))?;
            let hash = hash.to_ascii_lowercase();
            ensure!(
                valid_hash(&hash)
                    && matches!(separator, "  " | " *")
                    && expected.contains(name)
                    && sums.insert(name.to_owned(), hash.clone()).is_none(),
                "unsafe, duplicate or unexpected checksum entry"
            );
            ensure!(
                self.digests.get(name) == Some(&hash),
                "checksum differs from final API digest"
            );
        }
        ensure!(
            sums.len() == expected.len(),
            "release checksum inventory is incomplete"
        );
        Ok(sums)
    }
}

/// Permit only GitHub's known HTTPS transport hosts through bounded redirects.
/// The original API and asset URLs are independently constructed, never supplied by a record.
pub(super) fn transport_allowed(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && matches!(
            url.host_str(),
            Some(
                "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
                    | "github-releases.githubusercontent.com"
            )
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn metadata(version: &str) -> Value {
        let mut names = payload_inventory(version).unwrap();
        names.extend(
            [
                "SHA256SUMS.txt",
                "install.sh",
                "install.ps1",
                "uninstall.ps1",
            ]
            .map(str::to_owned),
        );
        json!({ "tag_name": format!("v{version}"), "draft": false, "prerelease": false,
            "assets": names.into_iter().map(|name| json!({
                "browser_download_url": format!("{RELEASE_BASE}/v{version}/{name}"),
                "name": name, "digest": format!("sha256:{}", "ab".repeat(32))
            })).collect::<Vec<_>>() })
    }

    fn parse(value: &Value) -> Result<Release> {
        Release::parse(&serde_json::to_vec(value)?, Some("0.10.0"))
    }

    #[test]
    fn source_requires_published_exact_version_inventory_and_final_asset_identity() {
        let value = metadata("0.10.0");
        let release = parse(&value).unwrap();
        assert_eq!(release.digests.len(), 14);
        assert_eq!(
            release.asset_url("install.ps1").unwrap(),
            format!("{RELEASE_BASE}/v0.10.0/install.ps1")
        );
        assert!(release.asset_url("../locron.exe").is_err());
        for field in ["draft", "prerelease"] {
            let mut changed = value.clone();
            changed[field] = json!(true);
            assert!(parse(&changed).is_err());
        }
        for (field, bad) in [
            ("name", json!("INSTALL.PS1")),
            ("digest", json!("sha1:abcd")),
            ("digest", json!("sha256:../malformed")),
            (
                "browser_download_url",
                json!("http://github.com/WhiteKiwi/locron/install.ps1"),
            ),
            (
                "browser_download_url",
                json!(format!("{RELEASE_BASE}/v0.10.1/install.ps1")),
            ),
        ] {
            let mut changed = value.clone();
            changed["assets"][0][field] = bad;
            assert!(parse(&changed).is_err(), "{field}");
        }
        let mut changed = value.clone();
        changed["assets"][0] = changed["assets"][1].clone();
        assert!(parse(&changed).is_err());
        let mut changed = value;
        changed["tag_name"] = json!("v0.9.6");
        assert!(parse(&changed).is_err());
        assert!(parse(&metadata("0.10.1")).is_err());
    }

    #[test]
    fn checksums_refuse_partial_casefolded_escaped_duplicate_and_disagreeing_inventory() {
        let release = parse(&metadata("0.10.0")).unwrap();
        let mut text = String::new();
        for name in payload_inventory("0.10.0").unwrap() {
            text.push_str(&format!("{}  {name}\n", "ab".repeat(32)));
        }
        assert_eq!(release.checksums(text.as_bytes()).unwrap().len(), 10);
        let first = text.lines().next().unwrap();
        for changed in [
            text.replacen(first, "", 1),
            format!("{text}{first}\n"),
            text.replacen("locron-", "LOCRON-", 1),
            text.replacen("locron-", "./locron-", 1),
            text.replacen("ab", "cd", 1),
            text.replacen("  ", " *../", 1),
            format!("{text}{}  install.ps1\n", "ab".repeat(32)),
        ] {
            assert!(release.checksums(changed.as_bytes()).is_err());
        }
        assert!(release.checksums(b"\xff").is_err());
    }

    #[test]
    fn transport_refuses_downgrade_userinfo_foreign_ports_and_lookalike_hosts() {
        for url in [
            "https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/install.ps1",
            "https://release-assets.githubusercontent.com/github-production-release-asset/1?signature=abc",
        ] {
            assert!(transport_allowed(&Url::parse(url).unwrap()));
        }
        for url in [
            "http://github.com/WhiteKiwi/locron",
            "https://github.com:444/WhiteKiwi/locron",
            "https://github.com.evil.example/WhiteKiwi/locron",
            "https://user@github.com/file",
            "https://github.com/file#fragment",
            "file:///C:/locron.exe",
            "https://evil.example/WhiteKiwi/locron",
        ] {
            assert!(!transport_allowed(&Url::parse(url).unwrap()), "{url}");
        }
    }
}
