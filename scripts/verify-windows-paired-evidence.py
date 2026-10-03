#!/usr/bin/env python3
"""Verify that the two paired Windows package artifacts form one coherent CI build."""
import argparse
import json
from pathlib import Path
import re

import windows_release

TARGETS = ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc")
EXECUTABLES = ("locron.exe", "locron-service-launcher.exe")
SCHEMA = "locron.windows-paired-package-verification/v1"
KIND = "draft-paired-windows-package"
HASH = re.compile(r"[0-9a-f]{64}")
GIT_SHA = re.compile(r"[0-9a-f]{40}")
VERSION = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")
FIELDS = {
    "schema", "artifact_kind", "source_repository", "source_revision",
    "source_head_revision", "source_ref", "source_workflow", "run_id",
    "run_attempt", "version", "target", "unsigned", "binaries",
    "version_probes", "launcher_abi", "launcher_identity", "archive_sha256",
    "rustc", "rustflags", "probe_path", "probe_bytes",
}
LAUNCHER_FIELDS = {
    "schema", "version", "target", "launcher_abi",
    "initial_conout_opened", "initial_conout_error",
}
BINARY_FIELDS = {"sha256", "subsystem", "imports"}


def _unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON field: {key}")
        value[key] = item
    return value


def _load(path):
    with path.open("rb") as stream:
        raw = stream.read(128 * 1024 + 1)
    if len(raw) > 128 * 1024:
        raise ValueError("paired verification document exceeds 128 KiB")
    try:
        value = json.loads(raw.decode("utf-8-sig"), object_pairs_hook=_unique_object)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("invalid paired verification JSON") from error
    if not isinstance(value, dict) or set(value) != FIELDS:
        raise ValueError("paired verification has missing or unknown fields")
    return value


def _sha(value, label):
    if not isinstance(value, str) or not HASH.fullmatch(value):
        raise ValueError(f"{label} is not canonical lowercase SHA-256")
    return value


def _git_sha(value, label):
    if not isinstance(value, str) or not GIT_SHA.fullmatch(value):
        raise ValueError(f"{label} is not a canonical 40-hex Git commit")
    return value


def _positive_decimal(value, label):
    if not isinstance(value, str) or not value.isascii() or not value.isdigit():
        raise ValueError(f"{label} is not a decimal string")
    number = int(value)
    if number <= 0 or str(number) != value:
        raise ValueError(f"{label} is not canonical positive decimal")
    return value


def _rustc(value, target):
    if not isinstance(value, str):
        raise ValueError("rustc evidence is not text")
    lines = value.splitlines()
    if not lines or not re.fullmatch(r"rustc [0-9]+\.[0-9]+\.[0-9]+ \([0-9a-f]+ [0-9]{4}-[0-9]{2}-[0-9]{2}\)", lines[0]):
        raise ValueError("rustc version line is not canonical")
    facts = {"version_line": lines[0]}
    for line in lines[1:]:
        if ": " in line:
            key, item = line.split(": ", 1)
            if key in facts:
                raise ValueError("duplicate rustc fact")
            facts[key] = item
    required = {"binary", "commit-hash", "commit-date", "host", "release", "LLVM version"}
    if not required.issubset(facts) or facts["host"] != target:
        raise ValueError("rustc host/facts do not match package target")
    return {key: facts[key] for key in sorted(facts) if key != "host"}


def _imports(value):
    if not isinstance(value, list) or any(not isinstance(item, str) or item != item.lower() for item in value):
        raise ValueError("binary import evidence is invalid")
    if value != sorted(set(value)):
        raise ValueError("binary import evidence is not sorted and unique")
    return value


def _launcher(value, version, target):
    if not isinstance(value, dict) or set(value) != LAUNCHER_FIELDS:
        raise ValueError("launcher identity has missing or unknown fields")
    expected = {
        "schema": "locron.windows-launcher-probe/v1",
        "version": version,
        "target": target,
        "launcher_abi": "native-gui-v1",
    }
    if any(value[key] != item for key, item in expected.items()):
        raise ValueError("launcher identity does not match package identity")
    opened = value["initial_conout_opened"]
    error = value["initial_conout_error"]
    if type(opened) is not bool:
        raise ValueError("launcher console-open fact is not boolean")
    if opened:
        if error is not None:
            raise ValueError("successful launcher console-open has an error")
    elif type(error) is not int or error == 0 or not -(2**31) <= error < 2**31:
        raise ValueError("failed launcher console-open lacks a bounded nonzero error")


def _record(directory):
    path = directory / "verification.json"
    if not path.is_file() or path.is_symlink():
        raise ValueError("paired verification.json is missing or unsafe")
    value = _load(path)
    if value["schema"] != SCHEMA or value["artifact_kind"] != KIND:
        raise ValueError("paired verification schema/kind differs")
    if not isinstance(value["source_repository"], str) or not value["source_repository"]:
        raise ValueError("source repository is missing")
    for field in ("source_revision", "source_head_revision"):
        _git_sha(value[field], field)
    for field in ("source_ref", "source_workflow"):
        if not isinstance(value[field], str) or not value[field] or any(ord(ch) < 32 for ch in value[field]):
            raise ValueError(f"{field} is invalid")
    _positive_decimal(value["run_id"], "run_id")
    _positive_decimal(value["run_attempt"], "run_attempt")
    if not isinstance(value["version"], str) or not VERSION.fullmatch(value["version"]):
        raise ValueError("package version is not canonical")
    target = value["target"]
    if target not in TARGETS:
        raise ValueError("unexpected paired package target")
    if value["unsigned"] is not True:
        raise ValueError("initial paired Windows package must be unsigned")
    if value["launcher_abi"] != "native-gui-v1":
        raise ValueError("unexpected launcher ABI")
    if value["rustflags"] != "-C target-feature=+crt-static":
        raise ValueError("paired package rustflags differ from static CRT policy")
    if value["probe_path"] != "Windows system directories only" or value["probe_bytes"] != "staged final ZIP members":
        raise ValueError("paired runtime probe policy differs")
    rustc = _rustc(value["rustc"], target)

    binaries = value["binaries"]
    if not isinstance(binaries, dict) or set(binaries) != set(EXECUTABLES):
        raise ValueError("paired binary evidence is incomplete")
    expected_subsystems = {"locron.exe": 3, "locron-service-launcher.exe": 2}
    normalized_binaries = {}
    for name in EXECUTABLES:
        item = binaries[name]
        if not isinstance(item, dict) or set(item) != BINARY_FIELDS:
            raise ValueError("paired binary evidence shape differs")
        if item["subsystem"] != expected_subsystems[name]:
            raise ValueError("paired binary subsystem differs from role")
        normalized_binaries[name] = {
            "sha256": _sha(item["sha256"], f"{name} sha256"),
            "subsystem": item["subsystem"],
            "imports": _imports(item["imports"]),
        }

    probes = value["version_probes"]
    if not isinstance(probes, dict) or set(probes) != set(EXECUTABLES):
        raise ValueError("paired version probe evidence is incomplete")
    expected_probes = {
        "locron.exe": f"locron {value['version']}\n",
        "locron-service-launcher.exe": f"locron-service-launcher {value['version']}\n",
    }
    if probes != expected_probes:
        raise ValueError("paired version probe output differs from package version")
    _launcher(value["launcher_identity"], value["version"], target)

    digest = _sha(value["archive_sha256"], "archive_sha256")
    zips = [item for item in directory.iterdir() if item.is_file() and not item.is_symlink() and item.suffix == ".zip"]
    expected_name = f"locron-v{value['version']}-{target}.zip"
    if len(zips) != 1 or zips[0].name != expected_name:
        raise ValueError("paired artifact directory must contain one exact ZIP")
    static = windows_release.inspect_archive(
        zips[0], f"v{value['version']}", target, paired=True, expected_sha256=digest
    )
    if static["archive_sha256"] != digest or static["binaries"] != normalized_binaries:
        raise ValueError("static paired ZIP facts differ from native verification")

    return value, rustc


def verify(root, expected=None):
    if not root.is_dir() or root.is_symlink():
        raise ValueError("paired evidence root is missing or unsafe")
    documents = sorted(root.rglob("verification.json"))
    if len(documents) != 2:
        raise ValueError("expected exactly two paired verification documents")
    directories = [path.parent for path in documents]
    records = [_record(directory) for directory in directories]
    values = [item[0] for item in records]
    rustc = [item[1] for item in records]
    by_target = {value["target"]: value for value in values}
    if set(by_target) != set(TARGETS) or len(by_target) != 2:
        raise ValueError("paired evidence must contain exactly x64 and ARM64")

    shared = (
        "schema", "artifact_kind", "source_repository", "source_revision",
        "source_head_revision", "source_ref", "source_workflow", "run_id",
        "run_attempt", "version", "unsigned", "launcher_abi", "rustflags",
        "probe_path", "probe_bytes",
    )
    first, second = values
    for field in shared:
        if first[field] != second[field]:
            raise ValueError(f"paired evidence disagrees on {field}")
    if rustc[0] != rustc[1]:
        raise ValueError("paired evidence used different Rust compiler builds")

    expected = expected or {}
    mapping = {
        "repository": "source_repository",
        "revision": "source_revision",
        "head": "source_head_revision",
        "ref": "source_ref",
        "workflow": "source_workflow",
        "run_id": "run_id",
        "run_attempt": "run_attempt",
    }
    for argument, field in mapping.items():
        wanted = expected.get(argument)
        if wanted is not None and first[field] != wanted:
            raise ValueError(f"paired evidence {field} does not match current workflow context")
    return by_target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--repository")
    parser.add_argument("--revision")
    parser.add_argument("--head")
    parser.add_argument("--ref")
    parser.add_argument("--workflow")
    parser.add_argument("--run-id")
    parser.add_argument("--run-attempt")
    args = parser.parse_args()
    expected = {
        key: value for key, value in vars(args).items()
        if key != "root" and value is not None
    }
    records = verify(args.root, expected)
    print("coherent paired Windows evidence: " + ", ".join(sorted(records)))


if __name__ == "__main__":
    main()
