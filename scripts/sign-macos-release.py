#!/usr/bin/env python3
"""Sign the two reviewed macOS archives using the existing local Keychain.

No credentials are copied or configured. Commands are injectable for hermetic tests.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import tarfile
import tempfile
import time
import zipfile
import uuid

IDENTITY = "F998EC776E413B3E4D00D5A1D63BBE6FA5C5765E"
TEAM = "4H4Z446LHS"
IDENTIFIER = "dev.locron.cli"
PROFILE = "c6s-notary"
TARGETS = {"aarch64-apple-darwin": "arm64", "x86_64-apple-darwin": "x86_64"}
FILES = {"locron", "README.md", "LICENSE-MIT", "LICENSE-APACHE"}


def version_tag(tag):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
        raise ValueError("expected an immutable stable vMAJOR.MINOR.PATCH tag")
    return tag[1:]


def require_release_environment(tag):
    version_tag(tag)
    if (os.environ.get("GITHUB_EVENT_NAME") != "push"
            or os.environ.get("GITHUB_REPOSITORY") != "WhiteKiwi/locron"
            or os.environ.get("GITHUB_REF") != "refs/tags/" + tag):
        raise ValueError("signing requires the exact repository's pushed release tag")


class ToolFailure(RuntimeError):
    def __init__(self, tool, code, output):
        super().__init__(f"{Path(tool).name} failed (exit {code})")
        self.code = code
        self.output = output


def command(args):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=1800, check=False)
    except subprocess.TimeoutExpired:
        raise ToolFailure(args[0], None, "command timed out") from None
    if result.returncode:
        raise ToolFailure(args[0], result.returncode, result.stdout + result.stderr)
    return result.stdout + result.stderr


def verify_notarization(binary, target, diagnostics, run):
    args = ["/usr/bin/codesign", "--verify", "--strict", "--verbose=4",
            "-R=notarized", "--check-notarization", str(binary)]
    result = {"target": target, "tool": "codesign", "requirement": "notarized",
              "check_notarization": True, "exit_code": 0}
    try:
        output = run(args)
    except ToolFailure as error:
        result["exit_code"] = error.code
        result["diagnostic"] = error.output[-8192:].replace(str(binary), "<signed executable>")
        (diagnostics / (target + "-notarized.json")).write_text(json.dumps(result, indent=2) + "\n")
        raise
    result["diagnostic"] = output[-8192:].replace(str(binary), "<signed executable>")
    (diagnostics / (target + "-notarized.json")).write_text(json.dumps(result, indent=2) + "\n")
    return {key: value for key, value in result.items() if key != "diagnostic"}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def unpack(archive, root, target, tag):
    name = f"locron-{tag}-{target}"
    destination = root / name
    destination.mkdir(mode=0o700)
    seen = set()
    with tarfile.open(archive, "r:gz") as stream:
        for member in stream:
            path = PurePosixPath(member.name)
            if (not path.parts or path.is_absolute() or ".." in path.parts or member.name.rstrip("/") != str(path)
                    or path.parts[0] != name or not (member.isdir() or member.isfile())):
                raise ValueError("unsafe archive member")
            if member.isdir() and path.parts == (name,):
                continue
            if len(path.parts) != 2 or path.parts[1] not in FILES or not member.isfile():
                raise ValueError("unexpected archive layout")
            if path.parts[1] in seen or member.size > 100_000_000:
                raise ValueError("duplicate or oversized archive member")
            if path.parts[1] == "locron" and not member.mode & 0o111:
                raise ValueError("archive executable lacks execute permission")
            seen.add(path.parts[1])
            source = stream.extractfile(member)
            if source is None:
                raise ValueError("missing archive member data")
            data = source.read()
            (destination / path.parts[1]).write_bytes(data)
            (destination / path.parts[1]).chmod(0o755 if path.parts[1] == "locron" else 0o644)
    if seen != FILES:
        raise ValueError("incomplete archive")
    return destination


@contextlib.contextmanager
def signing_lock(path, timeout=1800):
    path.parent.mkdir(parents=True, exist_ok=True)
    # Persistent inode shared with other Apple jobs; never unlink this file.
    with path.open("a") as lock:
        deadline = time.monotonic() + timeout
        while True:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise TimeoutError("shared Apple signing lock unavailable")
                time.sleep(1)
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def verify_binary(binary, arch, version, run):
    if run(["/usr/bin/lipo", "-archs", str(binary)]).strip() != arch:
        raise ValueError("unexpected executable architecture")
    if run([str(binary), "--version"]).strip() != "locron " + version:
        raise ValueError("unexpected executable version")


def verify_signature(binary, run):
    run(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", str(binary)])
    details = run(["/usr/bin/codesign", "-d", "--verbose=4", str(binary)])
    lines = details.splitlines()
    if (f"Identifier={IDENTIFIER}" not in lines or f"TeamIdentifier={TEAM}" not in lines
            or not any(line.startswith("Authority=Developer ID Application:") for line in lines)
            or not any(line.startswith("Timestamp=") and line != "Timestamp=none" for line in lines)
            or not any(line.startswith("CodeDirectory ") and "runtime" in line for line in lines)):
        raise ValueError("signature identity, timestamp or runtime verification failed")


def sign_release(inputs, output, tag, run=command, lock_path=None):
    version = version_tag(tag)
    expected = {f"locron-{tag}-{target}.tar.gz" for target in TARGETS}
    if ({p.name for p in inputs.iterdir()} != expected or output.exists()
            or any(p.is_symlink() or not p.is_file() for p in inputs.iterdir())):
        raise ValueError("expected exactly two Mac archives and a new output directory")
    output.parent.mkdir(parents=True, exist_ok=True)
    diagnostics = output.with_name(output.name + ".diagnostics")
    diagnostics.mkdir(mode=0o700)
    keychain = str(Path.home() / "Library/Keychains/login.keychain-db")
    lock_path = lock_path or Path.home() / "Library/Caches/home-hub/apple-signing.lock"
    with tempfile.TemporaryDirectory(prefix="locron-sign-", dir=output.parent) as temporary:
        staging = Path(temporary)
        staging.chmod(0o700)
        final = staging / "signed-macos"
        final.mkdir(mode=0o700)
        directories = {}
        for target, arch in TARGETS.items():
            directory = unpack(inputs / f"locron-{tag}-{target}.tar.gz", staging, target, tag)
            verify_binary(directory / "locron", arch, version, run)
            directories[target] = directory
        with signing_lock(lock_path):
            for directory in directories.values():
                binary = directory / "locron"
                run(["/usr/bin/codesign", "--force", "--sign", IDENTITY,
                     "--keychain", keychain, "--identifier", IDENTIFIER, "--options", "runtime", "--timestamp", str(binary)])
                verify_signature(binary, run)
            payload = staging / "notary.zip"
            with zipfile.ZipFile(payload, "w", zipfile.ZIP_DEFLATED) as archive:
                for target, directory in directories.items():
                    archive.write(directory / "locron", target + "/locron")
            result = json.loads(run(["/usr/bin/xcrun", "notarytool", "submit", str(payload),
                                     "--keychain-profile", PROFILE, "--keychain", keychain, "--wait", "--output-format", "json"]))
            submission = result.get("id", "")
            try:
                if str(uuid.UUID(submission)) != submission.lower():
                    raise ValueError("noncanonical submission ID")
            except (ValueError, AttributeError):
                raise ValueError("invalid notarization submission ID") from None
            (diagnostics / "submission.json").write_text(json.dumps({"id": submission, "status": result.get("status")}) + "\n")
            log_path = staging / "notary-log.json"
            run(["/usr/bin/xcrun", "notarytool", "log", submission, "--keychain-profile", PROFILE, "--keychain", keychain, str(log_path)])
            log = json.loads(log_path.read_text())
            # Apple's log describes submitted public executables, never Keychain values.
            (diagnostics / "notary-log.json").write_text(json.dumps(log, indent=2) + "\n")
            if result.get("status") != "Accepted" or log.get("status") != "Accepted" or log.get("issues") not in (None, []):
                raise ValueError("notary log contains issues or unexpected status")
            receipts = {}
            for target, directory in directories.items():
                binary = directory / "locron"
                verify_signature(binary, run)
                verify_binary(binary, TARGETS[target], version, run)
                requirement = verify_notarization(binary, target, diagnostics, run)
                archive_path = final / f"{directory.name}.tar.gz"
                directory.chmod(0o755)
                with tarfile.open(archive_path, "w:gz") as archive:
                    archive.add(directory, arcname=directory.name)
                # Verify repacking did not alter the signed executable.
                with tarfile.open(archive_path, "r:gz") as archive:
                    packaged = archive.extractfile(f"{directory.name}/locron").read()
                if hashlib.sha256(packaged).hexdigest() != digest(binary):
                    raise ValueError("repacked executable bytes differ")
                receipts[target] = {"binary_sha256": digest(binary), "archive_sha256": digest(archive_path), "notarized_requirement": requirement}
            receipt = {"schema": "locron.macos-signing/v1", "tag": tag, "identifier": IDENTIFIER,
                       "team": TEAM, "notarization": "Accepted", "submission_id": submission,
                       "verification": "codesign --verify --strict --verbose=4 -R=notarized --check-notarization", "targets": receipts}
            (final / "verification.json").write_text(json.dumps(receipt, indent=2) + "\n")
        os.rename(final, output)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("inputs", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    require_release_environment(args.tag)
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise ValueError("signing requires the reviewed ARM64 Mac runner")
    sign_release(args.inputs, args.output, args.tag)


if __name__ == "__main__":
    main()
