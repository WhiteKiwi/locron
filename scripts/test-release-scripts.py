#!/usr/bin/env python3
"""Hermetic release verification: never sign, notarize, publish, or update live state."""
import fcntl
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), ROOT / "scripts" / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


sign = load("sign-macos-release")
assets = load("release-assets")
smoke = load("smoke-published-updater")
TAG = "v0.9.6"


def archive_bytes(tag, target, extra=None):
    data = io.BytesIO()
    root = f"locron-{tag}-{target}"
    with tarfile.open(fileobj=data, mode="w:gz") as stream:
        for name in sign.FILES:
            content = b"fixture executable " + tag.encode() if name == "locron" else b"license/readme"
            member = tarfile.TarInfo(root + "/" + name)
            member.size = len(content)
            member.mode = 0o755 if name == "locron" else 0o644
            stream.addfile(member, io.BytesIO(content))
        if extra:
            stream.addfile(extra, io.BytesIO(b"x") if extra.isfile() else None)
    return data.getvalue()


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.inputs = self.root / "inputs"
        self.inputs.mkdir()
        for target in sign.TARGETS:
            (self.inputs / f"locron-{TAG}-{target}.tar.gz").write_bytes(archive_bytes(TAG, target))
        self.failure = None
        self.calls = []

    def run_tool(self, args):
        self.calls.append(args)
        if self.failure == "command":
            raise RuntimeError("fixture tool failure")
        if "lipo" in args[0]:
            return "arm64" if "aarch64" in args[-1] else "x86_64"
        if args[-1] == "--version":
            return "locron 0.9.6\n"
        if "codesign" in args[0] and "-d" in args:
            return (f"Identifier={sign.IDENTIFIER}\nTeamIdentifier={sign.TEAM}\n"
                    "Authority=Developer ID Application: Fixture\nTimestamp=Sep 30, 2026\n"
                    "CodeDirectory v=20500 flags=0x10000(runtime)\n") if self.failure != "signature" else "Signature=adhoc\n"
        if "submit" in args:
            return json.dumps({"id": "12345678-1234-1234-1234-123456789abc",
                               "status": "Invalid" if self.failure == "notary" else "Accepted"})
        if "log" in args:
            Path(args[-1]).write_text(json.dumps({"status": "Accepted", "issues": ["issue"] if self.failure == "log" else None}))
        if "-R=notarized" in args:
            if self.failure == "assessment":
                raise sign.ToolFailure("codesign", 3, "explicit requirement failed")
            return "explicit requirement satisfied"
        return ""

    def test_signing_preserves_layout_bytes_and_receipt(self):
        output = self.root / "output"
        receipt = sign.sign_release(self.inputs, output, TAG, self.run_tool, self.root / "shared.lock")
        self.assertEqual(receipt["team"], sign.TEAM)
        self.assertEqual(len(receipt["targets"]), 2)
        self.assertTrue((self.root / "shared.lock").exists())
        for result in receipt["targets"].values():
            self.assertEqual(result["notarized_requirement"]["exit_code"], 0)
            self.assertEqual(result["notarized_requirement"]["requirement"], "notarized")
        submits = [call for call in self.calls if "submit" in call]
        self.assertEqual(len(submits), 1)
        for target in sign.TARGETS:
            self.assertEqual(smoke.extracted_binary(archive_bytes(TAG, "x86_64-unknown-linux-gnu"), TAG), b"fixture executable " + TAG.encode())
            self.assertEqual(set(sign.unpack(output / f"locron-{TAG}-{target}.tar.gz", self.root, target, TAG).iterdir()),
                             {self.root / f"locron-{TAG}-{target}" / name for name in sign.FILES})

    def test_signing_failures_supply_no_output(self):
        for failure in ("command", "signature", "notary", "log", "assessment"):
            with self.subTest(failure=failure):
                self.failure = failure
                output = self.root / failure
                with self.assertRaises((ValueError, RuntimeError)):
                    sign.sign_release(self.inputs, output, TAG, self.run_tool, self.root / "shared.lock")
                self.assertFalse(output.exists())
                if failure == "assessment":
                    diagnosis = output.with_name(output.name + ".diagnostics") / "aarch64-apple-darwin-notarized.json"
                    self.assertEqual(json.loads(diagnosis.read_text())["exit_code"], 3)

    def test_archive_rejects_traversal_links_and_extra_files(self):
        target = next(iter(sign.TARGETS))
        for name, kind in (("../outside", tarfile.REGTYPE), ("/absolute", tarfile.REGTYPE),
                           (f"locron-{TAG}-{target}/extra", tarfile.REGTYPE),
                           (f"locron-{TAG}-{target}/link", tarfile.SYMTYPE),
                           (f"locron-{TAG}-{target}/hardlink", tarfile.LNKTYPE)):
            with self.subTest(name=name), tempfile.TemporaryDirectory(dir=self.root) as temporary:
                extra = tarfile.TarInfo(name)
                extra.type = kind
                extra.size = 1 if kind == tarfile.REGTYPE else 0
                extra.linkname = "/outside"
                fixture = Path(temporary) / "fixture.tar.gz"
                fixture.write_bytes(archive_bytes(TAG, target, extra))
                with self.assertRaises(ValueError):
                    sign.unpack(fixture, Path(temporary), target, TAG)

    def test_archive_missing_duplicate_and_inventory_refused(self):
        target = next(iter(sign.TARGETS))
        extra = tarfile.TarInfo(f"locron-{TAG}-{target}/locron")
        extra.size = 1
        fixture = self.root / "duplicate.tar.gz"
        fixture.write_bytes(archive_bytes(TAG, target, extra))
        with self.assertRaises(ValueError):
            sign.unpack(fixture, self.root, target, TAG)
        (self.inputs / "unexpected").write_text("x")
        with self.assertRaises(ValueError):
            sign.sign_release(self.inputs, self.root / "output", TAG, self.run_tool)

    def test_architecture_version_and_signature_refusals(self):
        for output in ("arm64 x86_64", "wrong"):
            with self.assertRaises(ValueError):
                sign.verify_binary(Path("fixture"), "arm64", "0.9.6", lambda args: output)
        self.failure = "signature"
        with self.assertRaises(ValueError):
            sign.verify_signature(Path("fixture"), self.run_tool)

    def test_exact_tag_and_environment_guards(self):
        for tag in ("main", "v01.2.3", "v1.2.3-beta.1", "v1.2.3/../bad"):
            with self.assertRaises(ValueError):
                sign.version_tag(tag)
        env = {"GITHUB_EVENT_NAME": "push", "GITHUB_REPOSITORY": assets.REPO, "GITHUB_REF": "refs/tags/" + TAG}
        with patch.dict(os.environ, env, clear=True):
            sign.require_release_environment(TAG)
        for key in env:
            invalid = dict(env, **{key: "untrusted"})
            with patch.dict(os.environ, invalid, clear=True), self.assertRaises(ValueError):
                sign.require_release_environment(TAG)

    def test_publication_inventory_and_immutable_digests(self):
        directory = self.root / "release"
        directory.mkdir()
        installer = self.root / "install.sh"
        installer.write_text("installer")
        for name in assets.expected_assets(TAG):
            (directory / name).write_text(name)
        sums = "".join(f"{assets.sha(directory / name)}  {name}\n" for name in sorted(assets.expected_assets(TAG)))
        (directory / "SHA256SUMS.txt").write_text(sums)
        hashes = assets.validate_inputs(TAG, directory, installer)
        release = {"assets": [{"name": name, "digest": "sha256:" + digest} for name, digest in hashes.items()]}
        assets.verify_existing(release, hashes)
        release["assets"][0]["digest"] = "sha256:" + "0" * 64
        with self.assertRaises(ValueError):
            assets.verify_existing(release, hashes)
        (directory / "unexpected").write_text("x")
        with self.assertRaises(ValueError):
            assets.validate_inputs(TAG, directory, installer)

    def test_checksum_parser_historical_and_duplicate_refusal(self):
        digest = "a" * 64
        self.assertEqual(assets.checksums(digest + "  ./archive.tar.gz\n", historical=True), {"archive.tar.gz": digest})
        for text in (digest + "  ./archive.tar.gz", digest + "  ../archive.tar.gz", "invalid", (digest + "  archive\n") * 2):
            with self.assertRaises(ValueError):
                assets.checksums(text)

    def test_smoke_preserves_home_and_removes_manager_overrides(self):
        original = {"HOME": "/unchanged", "XDG_RUNTIME_DIR": "/live", "DBUS_SESSION_BUS_ADDRESS": "live",
                    "LOCRON_UPDATE_API_BASE": "untrusted", "LOCRON_SERVICE_BACKEND": "real", "PATH": "/bin"}
        env = smoke.isolated_environment(self.root, original)
        self.assertEqual(env["HOME"], original["HOME"])
        self.assertEqual(env["XDG_CONFIG_HOME"], str(self.root / "config"))
        self.assertNotIn("LOCRON_UPDATE_API_BASE", env)
        self.assertNotIn("XDG_RUNTIME_DIR", env)
        self.assertNotIn("DBUS_SESSION_BUS_ADDRESS", env)
        self.assertEqual(smoke.RECEIPT, b"locron.install/v1\nstandalone\n")

    def test_smoke_refuses_wrong_latest_and_checksum(self):
        with self.assertRaises(ValueError):
            smoke.smoke(TAG, self.root, fetch=lambda url: b'{"tag_name":"v0.9.5"}')
        data = archive_bytes("v0.9.2", "x86_64-unknown-linux-gnu")
        def fetch(url):
            return (("0" * 64 + "  ./locron-v0.9.2-x86_64-unknown-linux-gnu.tar.gz\n").encode()
                    if url.endswith("SHA256SUMS.txt") else data)
        with self.assertRaises(ValueError):
            smoke.released_binary("v0.9.2", fetch)
        def correct(url):
            return ((hashlib.sha256(data).hexdigest() + "  ./locron-v0.9.2-x86_64-unknown-linux-gnu.tar.gz\n").encode()
                    if url.endswith("SHA256SUMS.txt") else data)
        self.assertEqual(smoke.released_binary("v0.9.2", correct), b"fixture executable v0.9.2")

    def test_shared_signing_lock_times_out_without_unlinking(self):
        lock_path = self.root / "apple-signing.lock"
        with lock_path.open("a") as existing:
            fcntl.flock(existing, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(TimeoutError):
                with sign.signing_lock(lock_path, timeout=0):
                    self.fail("unexpected lock acquisition")
        self.assertTrue(lock_path.exists())

    def test_signature_requires_each_identity_property(self):
        valid = self.run_tool(["codesign", "-d"])
        for line in valid.splitlines():
            altered = valid.replace(line, "")
            def run(args):
                return altered if "-d" in args else ""
            with self.subTest(line=line), self.assertRaises(ValueError):
                sign.verify_signature(Path("binary"), run)

    def test_invalid_notary_uuid_is_refused(self):
        def run(args):
            if "submit" in args:
                return json.dumps({"id": "-" * 36, "status": "Accepted"})
            return self.run_tool(args)
        with self.assertRaises(ValueError):
            sign.sign_release(self.inputs, self.root / "output", TAG, run, self.root / "shared.lock")
        self.assertFalse((self.root / "output").exists())

    def test_smoke_full_envelope_digest_and_job_preservation(self):
        data = {tag: archive_bytes(tag, "x86_64-unknown-linux-gnu") for tag in ("v0.9.2", TAG)}
        def fetch(url):
            if url.endswith("/latest"):
                return json.dumps({"tag_name": TAG}).encode()
            tag = "v0.9.2" if "/v0.9.2/" in url else TAG
            if url.endswith("SHA256SUMS.txt"):
                prefix = "./" if tag == "v0.9.2" else ""
                return (hashlib.sha256(data[tag]).hexdigest() + "  " + prefix + f"locron-{tag}-x86_64-unknown-linux-gnu.tar.gz\n").encode()
            return data[tag]
        updated = False
        def execute(args):
            nonlocal updated
            if args == ["--version"]:
                return "locron " + ("0.9.6" if updated else "0.9.2")
            command = args[1]
            if command == "list":
                result = [{"name": "updater-smoke", "enabled": True}]
                if updated:
                    result[0]["latest_run"] = None
            elif command == "self-update":
                updated = True
                (self.root / "bin/locron").write_bytes(smoke.extracted_binary(data[TAG], TAG))
                result = {"current_version": "0.9.2", "new_version": "0.9.6", "updated": True}
            else:
                result = {"name": "updater-smoke"}
            return json.dumps({"schema": "locron.cli/v1", "ok": True, "command": command, "data": result})
        smoke.smoke(TAG, self.root, fetch, execute)
        self.assertTrue(updated)

    def test_workflow_signed_only_and_guarded(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        self.assertIn("runs-on: locron-signing-${{ github.run_id }}", workflow)
        self.assertIn("github.event_name == 'push' && github.repository == 'WhiteKiwi/locron' && startsWith(github.ref, 'refs/tags/')", workflow)
        self.assertIn("needs: [build, sign-macos]", workflow)
        publisher = workflow.split("  publish:\n", 1)[1].split("  smoke-updater:\n", 1)[0]
        self.assertIn("pattern: '*-unknown-linux-gnu'", publisher)
        self.assertIn("name: signed-macos", publisher)
        self.assertNotIn("--clobber", workflow)
        self.assertIn("scripts/release-assets.py publish", workflow)
        self.assertIn("needs: publish", workflow)
        self.assertIn("scripts/smoke-published-updater.py", workflow)
        self.assertIn("scripts/test-release-scripts.py", (ROOT / ".github/workflows/ci.yml").read_text())


if __name__ == "__main__":
    unittest.main()
