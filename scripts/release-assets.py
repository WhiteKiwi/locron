#!/usr/bin/env python3
"""Validate publication inputs and refuse replacement of an existing release."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request

REPO = "WhiteKiwi/locron"
WINDOWS_FIRST_RELEASE = (0, 10, 0)
INSTALLER_FIRST_RELEASE = (0, 3, 0)


def version(tag):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
        raise ValueError("invalid stable release tag")
    return tag[1:]


def expected_assets(tag):
    release = version(tag)
    names = {f"locron-{tag}-{target}.tar.gz" for target in (
        "aarch64-apple-darwin", "x86_64-apple-darwin", "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu")}
    names.update(f"locron_{release}-1_{arch}.deb" for arch in ("amd64", "arm64"))
    names.update(f"locron-{release}-1.{arch}.rpm" for arch in ("aarch64", "x86_64"))
    if includes_windows(tag):
        names.update(f"locron-{tag}-{target}.zip" for target in (
            "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"))
    return names


def includes_windows(tag):
    return tuple(map(int, version(tag).split("."))) >= WINDOWS_FIRST_RELEASE


def expected_installers(tag, installer, windows_installer, windows_uninstaller):
    result = {}
    if tuple(map(int, version(tag).split("."))) >= INSTALLER_FIRST_RELEASE:
        result["install.sh"] = installer
    if includes_windows(tag):
        result.update({"install.ps1": windows_installer, "uninstall.ps1": windows_uninstaller})
    return result


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checksums(text, historical=False):
    parsed = {}
    for line in text.splitlines():
        match = re.fullmatch(r"([a-fA-F0-9]{64}) [ *](\S+)", line)
        if not match:
            raise ValueError("invalid checksum entry")
        name = match[2]
        if historical and name.startswith("./"):
            name = name[2:]
        if name in parsed or "/" in name or "\\" in name or ":" in name or name in (".", ".."):
            raise ValueError("unsafe or duplicate checksum filename")
        parsed[name] = match[1].lower()
    return parsed


def validate_inputs(tag, directory, installer, with_checksums=True,
                    windows_installer=Path("install.ps1"), windows_uninstaller=Path("uninstall.ps1")):
    names = expected_assets(tag)
    allowed = names | ({"SHA256SUMS.txt"} if with_checksums else set())
    if {p.name for p in directory.iterdir()} != allowed or not all((directory / name).is_file() and not (directory / name).is_symlink() for name in allowed):
        raise ValueError("release inventory differs from the exact version/platform inventory")
    installers = expected_installers(tag, installer, windows_installer, windows_uninstaller)
    if not all(path.is_file() and not path.is_symlink() for path in installers.values()):
        raise ValueError("missing or unsafe installer")
    if includes_windows(tag):
        from windows_release import inspect_archive
        for target in ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"):
            # Publication runs on Ubuntu: native version/ABI probes already ran in
            # each Windows build leg, while final bytes are re-inspected statically.
            inspect_archive(directory / f"locron-{tag}-{target}.zip", tag, target, paired=True)
    hashes = {name: sha(directory / name) for name in names}
    if with_checksums:
        if checksums((directory / "SHA256SUMS.txt").read_text()) != hashes:
            raise ValueError("checksums differ from final publication bytes")
        hashes["SHA256SUMS.txt"] = sha(directory / "SHA256SUMS.txt")
    hashes.update((name, sha(path)) for name, path in installers.items())
    return hashes


def fetch_json(url):
    with urllib.request.urlopen(url, timeout=120) as response:
        return json.load(response)


def verify_existing(release, hashes):
    assets = release.get("assets", [])
    if release.get("draft") or release.get("prerelease") or {asset.get("name") for asset in assets} != set(hashes) or len(assets) != len(hashes):
        raise ValueError("existing release inventory differs; refusing replacement")
    for asset in assets:
        if asset.get("digest") != "sha256:" + hashes[asset["name"]]:
            raise ValueError("existing release bytes differ; refusing replacement")


def publish(tag, directory, installer, notes, windows_installer=Path("install.ps1"),
            windows_uninstaller=Path("uninstall.ps1")):
    hashes = validate_inputs(tag, directory, installer, windows_installer=windows_installer,
                            windows_uninstaller=windows_uninstaller)
    result = subprocess.run(["gh", "api", f"repos/{REPO}/releases/tags/{tag}"], capture_output=True, text=True, check=False)
    if result.returncode == 0:
        release = json.loads(result.stdout)
        if release.get("tag_name") != tag:
            raise ValueError("existing release tag differs")
        verify_existing(release, hashes)
        print(f"Existing immutable release inventory and all {len(hashes)} digests match; no write needed.")
        return
    if "HTTP 404" not in result.stderr:
        raise RuntimeError("cannot inventory existing release; refusing publication")
    args = ["gh", "release", "create", tag, "--repo", REPO, "--verify-tag", "--title", "locron " + tag]
    args += [str(directory / name) for name in sorted(expected_assets(tag) | {"SHA256SUMS.txt"})]
    args.extend(str(path) for path in expected_installers(
        tag, installer, windows_installer, windows_uninstaller).values())
    if notes.is_file() and notes.stat().st_size:
        args += ["--notes-file", str(notes)]
    else:
        raise ValueError("missing curated changelog section")
    subprocess.run(args, check=True, timeout=600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("inventory", "publish", "plan"))
    parser.add_argument("tag", nargs="?", default=os.environ.get("GITHUB_REF_NAME"))
    parser.add_argument("directory", type=Path, nargs="?")
    parser.add_argument("--installer", type=Path, default=Path("install.sh"))
    parser.add_argument("--notes", type=Path, default=Path("release-notes.md"))
    parser.add_argument("--windows-installer", type=Path, default=Path("install.ps1"))
    parser.add_argument("--windows-uninstaller", type=Path, default=Path("uninstall.ps1"))
    args = parser.parse_args()
    if args.mode == "plan":
        value = "true" if includes_windows(args.tag) else "false"
        if "GITHUB_OUTPUT" in os.environ:
            with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
                output.write(f"windows={value}\n")
        print(f"Windows publication inputs: {value}")
    elif args.directory is None:
        parser.error("inventory/publish require an artifact directory")
    elif args.mode == "inventory":
        validate_inputs(args.tag, args.directory, args.installer, with_checksums=False,
                        windows_installer=args.windows_installer, windows_uninstaller=args.windows_uninstaller)
    else:
        publish(args.tag, args.directory, args.installer, args.notes,
                args.windows_installer, args.windows_uninstaller)


if __name__ == "__main__":
    main()
