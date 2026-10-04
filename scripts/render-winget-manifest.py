#!/usr/bin/env python3
"""Render proposed WinGet manifests from verified final Windows release ZIPs."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess

import windows_release

spec = importlib.util.spec_from_file_location("release_assets", Path(__file__).with_name("release-assets.py"))
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)
IDENTIFIER = "WhiteKiwi.locron"
SCHEMA = "1.12.0"
SOURCE = "https://github.com/WhiteKiwi/locron"
MAX_CHECKSUM_BYTES = 128 * 1024


def _same_object(path, original):
    current = path.lstat()
    return (current.st_dev, current.st_ino, stat.S_IFMT(current.st_mode)) == (
        original.st_dev, original.st_ino, stat.S_IFMT(original.st_mode)
    )


def _write_documents(output, documents):
    # Prepare every byte before creating output; encoding failures leave no files.
    encoded = []
    for name, text in documents.items():
        if (not isinstance(name, str) or not name or name in (".", "..")
                or any(character in name for character in ("/", "\\", ":", "\0"))):
            raise ValueError("manifest output name must be a single file leaf")
        encoded.append((name, text.encode("utf-8")))
    if not encoded:
        raise ValueError("manifest output cannot be empty")

    output.mkdir(parents=True, exist_ok=False)  # Never clean up a pre-existing output.
    directory, created = None, []
    try:
        directory = output.lstat()
        if not stat.S_ISDIR(directory.st_mode):
            raise OSError("new manifest output is not an ordinary directory")
        for name, data in encoded:
            if not _same_object(output, directory):
                raise OSError("manifest output directory changed during generation")
            path = output / name
            with path.open("xb") as stream:
                created.append((path, os.fstat(stream.fileno())))
                if stream.write(data) != len(data):
                    raise OSError("short manifest output write")
                stream.flush()
    except BaseException as error:
        # Remove only objects created by this attempt. Never recurse into an
        # output directory or delete an unexpected/replaced file to make it empty.
        problems = []
        for path, identity in reversed(created):
            try:
                if directory is None or not _same_object(output, directory):
                    problems.append("output directory changed; retained remaining files")
                    break
                if not _same_object(path, identity):
                    problems.append(f"retained replaced manifest: {path}")
                    continue
                path.unlink()
            except FileNotFoundError:
                continue
            except OSError as cleanup_error:
                problems.append(f"could not remove {path}: {cleanup_error}")
        try:
            if directory is not None and _same_object(output, directory):
                output.rmdir()  # Fails rather than removing unrelated entries.
            else:
                problems.append("output directory identity is unavailable; retained output")
        except FileNotFoundError:
            pass
        except OSError as cleanup_error:
            problems.append(f"retained output directory {output}: {cleanup_error}")
        add_note = getattr(error, "add_note", None)
        if add_note is not None:
            for problem in problems:
                add_note(problem)
        raise
    return output


def render(tag, directory, output, paired=False):
    release = assets.version(tag)
    if not assets.includes_windows(tag):
        raise ValueError("WinGet generation requires a Windows feature release tag")
    checksum_path = directory / "SHA256SUMS.txt"
    if not checksum_path.is_file() or checksum_path.is_symlink():
        raise ValueError("missing or unsafe final release checksum file")
    with checksum_path.open("rb") as stream:
        checksum_bytes = stream.read(MAX_CHECKSUM_BYTES + 1)
    if len(checksum_bytes) > MAX_CHECKSUM_BYTES:
        raise ValueError("final release checksum file exceeds its byte limit")
    sums = assets.checksums(checksum_bytes.decode("utf-8"))
    if set(sums) != assets.expected_assets(tag):
        raise ValueError("checksum inventory differs from final release payloads")
    entries = []
    for architecture, target in (("x64", "x86_64-pc-windows-msvc"), ("arm64", "aarch64-pc-windows-msvc")):
        name = f"locron-{tag}-{target}.zip"
        archive = directory / name
        # A manifest host must not execute either architecture. Native version/ABI
        # evidence still comes from the architecture-specific package CI gates.
        facts = windows_release.inspect_archive(archive, tag, target, paired=paired,
                                                expected_sha256=sums[name])
        digest = facts["archive_sha256"]
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
    return _write_documents(output, documents)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("directory", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--validate", action="store_true", help="run installed winget validate; no package installation")
    parser.add_argument("--paired", action="store_true",
                        help="inspect draft five-file ZIPs; keep the sole console alias and do not execute images")
    args = parser.parse_args()
    directory = render(args.tag, args.directory, args.output, paired=args.paired)
    if args.validate:
        subprocess.run(["winget", "validate", "--manifest", str(directory.resolve()), "--disable-interactivity"],
                       check=True, timeout=120)
    print(directory)


if __name__ == "__main__":
    main()
