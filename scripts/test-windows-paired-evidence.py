#!/usr/bin/env python3
"""Offline tests for paired Windows package evidence coherence."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest

import windows_release

spec = importlib.util.spec_from_file_location(
    "paired_evidence", Path(__file__).with_name("verify-windows-paired-evidence.py")
)
paired = importlib.util.module_from_spec(spec)
spec.loader.exec_module(paired)

VERSION = "0.10.0"
REPOSITORY = "WhiteKiwi/locron"
REVISION = "a" * 40
HEAD = "b" * 40
REF = "refs/pull/130/merge"
WORKFLOW = "WhiteKiwi/locron/.github/workflows/ci.yml@refs/pull/130/merge"
RUN_ID = "37130000000"
RUN_ATTEMPT = "1"


def pe(machine, subsystem):
    binary = bytearray(512)
    binary[:2] = b"MZ"
    struct.pack_into("<I", binary, 60, 128)
    binary[128:132] = b"PE\0\0"
    for offset, value in (
        (132, machine), (134, 1), (148, 240), (150, 0x22),
        (152, 0x20B), (220, subsystem),
    ):
        struct.pack_into("<H", binary, offset, value)
    struct.pack_into("<I", binary, 260, 16)
    return bytes(binary)


def rustc(target):
    return "\n".join([
        "rustc 1.94.0 (0123456789abcdef 2026-06-01)",
        "binary: rustc",
        "commit-hash: 0123456789abcdef0123456789abcdef01234567",
        "commit-date: 2026-06-01",
        f"host: {target}",
        "release: 1.94.0",
        "LLVM version: 21.1.0",
    ])


class PairedEvidence(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="locron-paired-evidence-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for target in paired.TARGETS:
            self.write_artifact(target)

    def directory(self, target):
        return self.root / f"windows-paired-package-draft-{target}"

    def write_artifact(self, target):
        machine = windows_release.TARGETS[target]
        directory = self.directory(target)
        directory.mkdir(parents=True, exist_ok=True)
        contents = {
            name: (
                pe(machine, windows_release.SUBSYSTEMS[name])
                if name in windows_release.SUBSYSTEMS
                else f"fixture {name}".encode()
            )
            for name in windows_release.PAIRED_FILES
        }
        archive = directory / f"locron-v{VERSION}-{target}.zip"
        windows_release.write_archive(
            archive, f"v{VERSION}", target, contents, windows_release.PAIRED_FILES
        )
        facts = windows_release.inspect_archive(
            archive, f"v{VERSION}", target, paired=True
        )
        launcher = {
            "schema": "locron.windows-launcher-probe/v1",
            "version": VERSION,
            "target": target,
            "launcher_abi": "native-gui-v1",
            "initial_conout_opened": False,
            "initial_conout_error": 2,
        }
        value = {
            "schema": paired.SCHEMA,
            "artifact_kind": paired.KIND,
            "source_repository": REPOSITORY,
            "source_revision": REVISION,
            "source_head_revision": HEAD,
            "source_ref": REF,
            "source_workflow": WORKFLOW,
            "run_id": RUN_ID,
            "run_attempt": RUN_ATTEMPT,
            "version": VERSION,
            "target": target,
            "unsigned": True,
            "binaries": facts["binaries"],
            "version_probes": {
                "locron.exe": f"locron {VERSION}\n",
                "locron-service-launcher.exe": f"locron-service-launcher {VERSION}\n",
            },
            "launcher_abi": "native-gui-v1",
            "launcher_identity": launcher,
            "archive_sha256": facts["archive_sha256"],
            "rustc": rustc(target),
            "rustflags": "-C target-feature=+crt-static",
            "probe_path": "Windows system directories only",
            "probe_bytes": "staged final ZIP members",
        }
        (directory / "verification.json").write_text(
            json.dumps(value, indent=2), encoding="utf-8"
        )

    def read(self, target):
        path = self.directory(target) / "verification.json"
        return path, json.loads(path.read_text(encoding="utf-8"))

    def write(self, target, value):
        path = self.directory(target) / "verification.json"
        path.write_text(json.dumps(value, indent=2), encoding="utf-8")

    def expected(self):
        return {
            "repository": REPOSITORY,
            "revision": REVISION,
            "head": HEAD,
            "ref": REF,
            "workflow": WORKFLOW,
            "run_id": RUN_ID,
            "run_attempt": RUN_ATTEMPT,
        }

    def test_exact_x64_arm64_pair_matches_current_context(self):
        result = paired.verify(self.root, self.expected())
        self.assertEqual(set(result), set(paired.TARGETS))

    def test_every_shared_build_identity_must_match(self):
        changes = {
            "source_repository": "Other/repo",
            "source_revision": "c" * 40,
            "source_head_revision": "d" * 40,
            "source_ref": "refs/heads/other",
            "source_workflow": "Other/repo/.github/workflows/ci.yml@refs/heads/other",
            "run_id": "37130000001",
            "run_attempt": "2",
            "version": "0.10.1",
            "unsigned": False,
            "launcher_abi": "other",
            "rustflags": "-C opt-level=3",
            "probe_path": "developer PATH",
            "probe_bytes": "unbound files",
        }
        for field, changed in changes.items():
            with self.subTest(field=field):
                self.setUp()
                _, value = self.read(paired.TARGETS[1])
                value[field] = changed
                if field == "version":
                    value["version_probes"]["locron.exe"] = "locron 0.10.1\n"
                    value["version_probes"]["locron-service-launcher.exe"] = (
                        "locron-service-launcher 0.10.1\n"
                    )
                    value["launcher_identity"]["version"] = "0.10.1"
                self.write(paired.TARGETS[1], value)
                with self.assertRaises(ValueError):
                    paired.verify(self.root)

    def test_current_context_mismatch_refuses(self):
        for field, changed in {
            "repository": "Other/repo",
            "revision": "c" * 40,
            "head": "d" * 40,
            "ref": "refs/heads/other",
            "workflow": "other",
            "run_id": "9",
            "run_attempt": "2",
        }.items():
            with self.subTest(field=field):
                expected = self.expected()
                expected[field] = changed
                with self.assertRaisesRegex(ValueError, "current workflow context"):
                    paired.verify(self.root, expected)

    def test_duplicate_or_missing_architecture_refuses(self):
        second = self.directory(paired.TARGETS[1])
        for item in second.iterdir():
            item.unlink()
        second.rmdir()
        with self.assertRaises(ValueError):
            paired.verify(self.root)

    def test_archive_bytes_and_recorded_digest_are_bound(self):
        target = paired.TARGETS[0]
        archive = self.directory(target) / f"locron-v{VERSION}-{target}.zip"
        archive.write_bytes(archive.read_bytes() + b"x")
        with self.assertRaises(ValueError):
            paired.verify(self.root)

        self.tearDown()
        self.setUp()
        _, value = self.read(target)
        value["archive_sha256"] = "0" * 64
        self.write(target, value)
        with self.assertRaises(ValueError):
            paired.verify(self.root)

    def test_static_binary_facts_must_match_archive(self):
        target = paired.TARGETS[0]
        _, value = self.read(target)
        value["binaries"]["locron.exe"]["sha256"] = "0" * 64
        self.write(target, value)
        with self.assertRaises(ValueError):
            paired.verify(self.root)

    def test_runtime_version_and_launcher_identity_are_strict(self):
        target = paired.TARGETS[0]
        for mutation in ("version", "launcher_target", "console_error"):
            with self.subTest(mutation=mutation):
                self.setUp()
                _, value = self.read(target)
                if mutation == "version":
                    value["version_probes"]["locron.exe"] = "locron 9.9.9\n"
                elif mutation == "launcher_target":
                    value["launcher_identity"]["target"] = paired.TARGETS[1]
                else:
                    value["launcher_identity"]["initial_conout_error"] = 0
                self.write(target, value)
                with self.assertRaises(ValueError):
                    paired.verify(self.root)

    def test_rustc_host_and_compiler_build_must_match(self):
        target = paired.TARGETS[1]
        _, value = self.read(target)
        value["rustc"] = rustc(paired.TARGETS[0])
        self.write(target, value)
        with self.assertRaises(ValueError):
            paired.verify(self.root)

        self.tearDown()
        self.setUp()
        _, value = self.read(target)
        value["rustc"] = value["rustc"].replace("release: 1.94.0", "release: 1.94.1")
        self.write(target, value)
        with self.assertRaises(ValueError):
            paired.verify(self.root)

    def test_json_shape_rejects_unknown_missing_and_duplicate_fields(self):
        target = paired.TARGETS[0]
        path, value = self.read(target)
        value["unexpected"] = True
        self.write(target, value)
        with self.assertRaises(ValueError):
            paired.verify(self.root)

        self.tearDown()
        self.setUp()
        path, value = self.read(target)
        del value["probe_bytes"]
        self.write(target, value)
        with self.assertRaises(ValueError):
            paired.verify(self.root)

        self.tearDown()
        self.setUp()
        path, value = self.read(target)
        raw = json.dumps(value)
        path.write_text(raw[:-1] + ',"schema":"duplicate"}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "duplicate JSON field"):
            paired.verify(self.root)

    def test_symlinked_verification_or_extra_zip_refuses(self):
        target = paired.TARGETS[0]
        directory = self.directory(target)
        path = directory / "verification.json"
        saved = directory / "saved.json"
        path.rename(saved)
        try:
            path.symlink_to(saved)
        except OSError:
            self.skipTest("host does not permit symlink fixtures")
        with self.assertRaises(ValueError):
            paired.verify(self.root)

        path.unlink()
        saved.rename(path)
        (directory / "extra.zip").write_bytes(b"extra")
        with self.assertRaises(ValueError):
            paired.verify(self.root)


if __name__ == "__main__":
    unittest.main()
