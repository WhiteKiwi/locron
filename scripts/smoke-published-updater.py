#!/usr/bin/env python3
"""Run a real v0.9.2 updater only on ephemeral hosted Linux, preserving HOME."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import subprocess
import tarfile
import tempfile
import urllib.request

spec = importlib.util.spec_from_file_location("release_assets", Path(__file__).with_name("release-assets.py"))
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)
BASE = "https://github.com/WhiteKiwi/locron/releases/download/"
RECEIPT = b"locron.install/v1\nstandalone\n"


def isolated_environment(root, original):
    env = original.copy()
    for key in list(env):
        if key.startswith("LOCRON_") or key in ("XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"):
            del env[key]
    env["LOCRON_STATE_DIR"] = str(root / "state")
    env["XDG_CONFIG_HOME"] = str(root / "config")
    return env


def require_environment(tag, env):
    assets.version(tag)
    if (platform.system() != "Linux" or platform.machine() != "x86_64"
            or env.get("GITHUB_ACTIONS") != "true" or env.get("RUNNER_ENVIRONMENT") != "github-hosted"
            or env.get("GITHUB_EVENT_NAME") != "push" or env.get("GITHUB_REPOSITORY") != "WhiteKiwi/locron"
            or env.get("GITHUB_REF") != "refs/tags/" + tag):
        raise ValueError("smoke requires ephemeral hosted Linux on the exact release tag")


def download(url):
    with urllib.request.urlopen(url, timeout=120) as response:
        return response.read()


def extracted_binary(data, tag):
    import io
    name = f"locron-{tag}-x86_64-unknown-linux-gnu/locron"
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        members = [member for member in archive if member.name == name]
        if len(members) != 1 or not members[0].isfile():
            raise ValueError("missing regular Linux executable")
        return archive.extractfile(members[0]).read()


def released_binary(tag, fetch=download):
    name = f"locron-{tag}-x86_64-unknown-linux-gnu.tar.gz"
    sums = assets.checksums(fetch(BASE + tag + "/SHA256SUMS.txt").decode(), historical=True)
    data = fetch(BASE + tag + "/" + name)
    if hashlib.sha256(data).hexdigest() != sums.get(name):
        raise ValueError("published archive checksum mismatch")
    return extracted_binary(data, tag)


def envelope(output, command):
    result = json.loads(output)
    if result.get("schema") != "locron.cli/v1" or result.get("ok") is not True or result.get("command") != command:
        raise ValueError("unexpected CLI envelope")
    return result["data"]


def smoke(tag, root, fetch=download, execute=None):
    latest = json.loads(fetch("https://api.github.com/repos/WhiteKiwi/locron/releases/latest"))
    if latest.get("tag_name") != tag or latest.get("draft") or latest.get("prerelease"):
        raise ValueError("expected release is not the published latest stable tag")
    binary = root / "bin/locron"
    binary.parent.mkdir()
    binary.write_bytes(released_binary("v0.9.2", fetch))
    binary.chmod(0o755)
    receipt = binary.parent / ".locron-install-receipt-v1"
    receipt.write_bytes(RECEIPT)
    expected_binary = released_binary(tag, fetch)
    env = isolated_environment(root, os.environ)
    if execute is None:
        def execute(args):
            return subprocess.run([str(binary), *args], env=env, capture_output=True, text=True,
                                  check=True, timeout=600).stdout
    if execute(["--version"]).strip() != "locron 0.9.2":
        raise ValueError("baseline version differs")
    # A real durable fixture job, never executed or registered with a manager.
    envelope(execute(["--json", "add", "updater-smoke", "--every", "1d", "--", "/bin/true"]), "add")
    before = envelope(execute(["--json", "list"]), "list")
    updated = envelope(execute(["--json", "self-update"]), "self-update")
    if updated.get("current_version") != "0.9.2" or updated.get("new_version") != assets.version(tag) or updated.get("updated") is not True:
        raise ValueError("updater outcome differs")
    if execute(["--version"]).strip() != "locron " + assets.version(tag):
        raise ValueError("updated version differs")
    if binary.read_bytes() != expected_binary or receipt.read_bytes() != RECEIPT:
        raise ValueError("updated executable or ownership receipt differs")
    after = envelope(execute(["--json", "list"]), "list")
    # Newer clients may add projections to list rows; durable job data must match.
    def durable(rows):
        return [{key: value for key, value in row.items() if key != "latest_run"} for row in rows]
    if durable(before) != durable(after):
        raise ValueError("fixture job state changed")
    if list(root.rglob("*.service")):
        raise ValueError("smoke unexpectedly created a manager registration")
    print(json.dumps({"schema": "locron.published-updater-smoke/v1", "from": "0.9.2",
                      "to": assets.version(tag), "binary_sha256": hashlib.sha256(expected_binary).hexdigest(),
                      "job_state_preserved": True, "manager_units": 0}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    args = parser.parse_args()
    require_environment(args.tag, os.environ)
    with tempfile.TemporaryDirectory(prefix="locron-old-updater-", dir=os.environ["RUNNER_TEMP"]) as temporary:
        smoke(args.tag, Path(temporary))


if __name__ == "__main__":
    main()
