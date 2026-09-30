#!/usr/bin/env python3
"""Validate publication inputs and refuse replacement of an existing release."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import urllib.request

REPO = "WhiteKiwi/locron"


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
    return names


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
        if name in parsed or "/" in name or name in (".", ".."):
            raise ValueError("unsafe or duplicate checksum filename")
        parsed[name] = match[1].lower()
    return parsed


def validate_inputs(tag, directory, installer, with_checksums=True):
    names = expected_assets(tag)
    allowed = names | ({"SHA256SUMS.txt"} if with_checksums else set())
    if {p.name for p in directory.iterdir()} != allowed or not all((directory / name).is_file() and not (directory / name).is_symlink() for name in allowed):
        raise ValueError("release inventory differs from four archives and four Linux packages")
    if not installer.is_file():
        raise ValueError("missing installer")
    hashes = {name: sha(directory / name) for name in names}
    if with_checksums:
        if checksums((directory / "SHA256SUMS.txt").read_text()) != hashes:
            raise ValueError("checksums differ from final publication bytes")
        hashes["SHA256SUMS.txt"] = sha(directory / "SHA256SUMS.txt")
    hashes["install.sh"] = sha(installer)
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


def publish(tag, directory, installer, notes):
    hashes = validate_inputs(tag, directory, installer)
    result = subprocess.run(["gh", "api", f"repos/{REPO}/releases/tags/{tag}"], capture_output=True, text=True, check=False)
    if result.returncode == 0:
        release = json.loads(result.stdout)
        if release.get("tag_name") != tag:
            raise ValueError("existing release tag differs")
        verify_existing(release, hashes)
        print("Existing immutable release inventory and all ten digests match; no write needed.")
        return
    if "HTTP 404" not in result.stderr:
        raise RuntimeError("cannot inventory existing release; refusing publication")
    args = ["gh", "release", "create", tag, "--repo", REPO, "--verify-tag", "--title", "locron " + tag]
    args += [str(directory / name) for name in sorted(expected_assets(tag) | {"SHA256SUMS.txt"})]
    args.append(str(installer))
    if notes.is_file() and notes.stat().st_size:
        args += ["--notes-file", str(notes)]
    else:
        raise ValueError("missing curated changelog section")
    subprocess.run(args, check=True, timeout=600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("inventory", "publish"))
    parser.add_argument("tag")
    parser.add_argument("directory", type=Path)
    parser.add_argument("--installer", type=Path, default=Path("install.sh"))
    parser.add_argument("--notes", type=Path, default=Path("release-notes.md"))
    args = parser.parse_args()
    if args.mode == "inventory":
        validate_inputs(args.tag, args.directory, args.installer, with_checksums=False)
    else:
        publish(args.tag, args.directory, args.installer, args.notes)


if __name__ == "__main__":
    main()
