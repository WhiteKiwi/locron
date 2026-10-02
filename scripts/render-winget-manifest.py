#!/usr/bin/env python3
"""Render proposed WinGet manifests from verified final Windows release ZIPs."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

import windows_release

spec = importlib.util.spec_from_file_location("release_assets", Path(__file__).with_name("release-assets.py"))
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)
IDENTIFIER = "WhiteKiwi.locron"
SCHEMA = "1.12.0"
SOURCE = "https://github.com/WhiteKiwi/locron"


def render(tag, directory, output):
    release = assets.version(tag)
    if not assets.includes_windows(tag):
        raise ValueError("WinGet generation requires a Windows feature release tag")
    sums = assets.checksums((directory / "SHA256SUMS.txt").read_text(encoding="utf-8"))
    if set(sums) != assets.expected_assets(tag):
        raise ValueError("checksum inventory differs from final release payloads")
    entries = []
    for architecture, target in (("x64", "x86_64-pc-windows-msvc"), ("arm64", "aarch64-pc-windows-msvc")):
        name = f"locron-{tag}-{target}.zip"
        archive = directory / name
        windows_release.validate_archive(archive, tag, target)
        digest = assets.sha(archive)
        if sums[name] != digest:
            raise ValueError("WinGet archive differs from the final release checksum")
        entries.extend((f"- Architecture: {architecture}", f"  InstallerUrl: {SOURCE}/releases/download/{tag}/{name}",
                        f"  InstallerSha256: {digest.upper()}", "  NestedInstallerFiles:",
                        f"  - RelativeFilePath: locron-{tag}-{target}/locron.exe",
                        "    PortableCommandAlias: locron"))
    base = f"PackageIdentifier: {IDENTIFIER}\nPackageVersion: {json.dumps(release)}\n"
    documents = {
        f"{IDENTIFIER}.yaml": base + "DefaultLocale: en-US\nManifestType: version\n" + f"ManifestVersion: {SCHEMA}\n",
        f"{IDENTIFIER}.locale.en-US.yaml": base +
            "PackageLocale: en-US\nPublisher: WhiteKiwi\n" +
            f"PublisherUrl: https://github.com/WhiteKiwi\nPublisherSupportUrl: {SOURCE}/issues\n" +
            f"PackageName: locron\nPackageUrl: {SOURCE}\n" +
            f"License: MIT OR Apache-2.0\nLicenseUrl: {SOURCE}/blob/main/LICENSE-MIT\n" +
            "ShortDescription: A local-first job scheduler with a command line interface.\n" +
            "Moniker: locron\nManifestType: defaultLocale\n" + f"ManifestVersion: {SCHEMA}\n",
        f"{IDENTIFIER}.installer.yaml": base +
            "MinimumOSVersion: 10.0.22000.0\nInstallerType: zip\nNestedInstallerType: portable\n" +
            "UpgradeBehavior: install\nCommands:\n- locron\nInstallers:\n" +
            "\n".join(entries) + "\nManifestType: installer\n" + f"ManifestVersion: {SCHEMA}\n",
    }
    for name in documents:
        kind = "installer" if name.endswith(".installer.yaml") else "defaultLocale" if ".locale." in name else "version"
        documents[name] = f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.{kind}.{SCHEMA}.schema.json\n" + documents[name]
    output.mkdir(parents=True, exist_ok=False)
    for name, text in documents.items():
        (output / name).write_text(text, encoding="utf-8", newline="\n")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("directory", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--validate", action="store_true", help="run installed winget validate; no package installation")
    args = parser.parse_args()
    directory = render(args.tag, args.directory, args.output)
    if args.validate:
        subprocess.run(["winget", "validate", "--manifest", str(directory.resolve()), "--disable-interactivity"],
                       check=True, timeout=120)
    print(directory)


if __name__ == "__main__":
    main()
