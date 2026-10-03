#!/usr/bin/env python3
"""Offline tests for paired Windows package evidence coherence."""
import importlib.util
import json
import os
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

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


class BoundedReader:
    def __init__(self, stream, limit, reads):
        self.stream, self.limit, self.reads = stream, limit, reads

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return self.stream.__exit__(*args)

    def read(self, size=-1):
        if not isinstance(size, int) or not 0 <= size <= self.limit:
            raise AssertionError(f"unbounded artifact read: {size}")
        value = self.stream.read(size)
        self.reads.append((size, len(value)))
        return value


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

    def audit_reads(self, limits, reads):
        real_open = Path.open
        def opened(path, *args, **kwargs):
            stream = real_open(path, *args, **kwargs)
            if path in limits:
                return BoundedReader(stream, limits[path], reads.setdefault(path, []))
            return stream
        return opened

    def stale_size(self, path):
        real_stat = Path.stat
        def observed(item, *args, **kwargs):
            facts = real_stat(item, *args, **kwargs)
            if item == path:
                values = list(facts)
                values[6] = 0
                return os.stat_result(values)
            return facts
        return observed

    def test_exact_x64_arm64_pair_matches_current_context(self):
        result = paired.verify(self.root, self.expected())
        self.assertEqual(set(result), set(paired.TARGETS))

    def test_oversized_json_refuses_with_bounded_read_before_decode_or_parse(self):
        # The first sorted architecture record must refuse before any JSON parse.
        path, _ = self.read(paired.TARGETS[1])
        limit = 128 * 1024
        path.write_bytes(b"\xff" * (limit + 1))
        reads = {}
        with patch.object(Path, "stat", new=self.stale_size(path)), \
                patch.object(Path, "open", new=self.audit_reads({path: limit + 1}, reads)), \
                patch.object(Path, "read_bytes", side_effect=AssertionError("whole-file JSON read")), \
                patch.object(paired.json, "loads", side_effect=AssertionError("oversized JSON parsed")):
            with self.assertRaisesRegex(ValueError, "exceeds 128 KiB"):
                paired.verify(self.root, self.expected())
        self.assertEqual(reads, {path: [(limit + 1, limit + 1)]})

    def test_exact_limit_json_is_accepted(self):
        target = paired.TARGETS[0]
        path, value = self.read(target)
        raw = json.dumps(value).encode("utf-8")
        path.write_bytes(raw + b" " * (128 * 1024 - len(raw)))
        result = paired.verify(self.root, self.expected())
        self.assertEqual(set(result), set(paired.TARGETS))

    def test_oversized_zip_refuses_before_whole_file_read(self):
        target = paired.TARGETS[0]
        archive = self.directory(target) / f"locron-v{VERSION}-{target}.zip"
        with archive.open("r+b") as stream:
            stream.truncate(64 * 1024 * 1024 + 1)
        real_read = Path.read_bytes
        def reject_zip_read(path):
            if path == archive:
                raise AssertionError("whole-file ZIP read")
            return real_read(path)
        with patch.object(Path, "read_bytes", new=reject_zip_read):
            with self.assertRaisesRegex(ValueError, "oversized Windows archive"):
                paired._record(self.directory(target))

    def test_oversized_zip_with_stale_size_refuses_after_bounded_read_before_hash(self):
        target = paired.TARGETS[0]
        archive = self.directory(target) / f"locron-v{VERSION}-{target}.zip"
        limit = 64 * 1024 * 1024
        # Leave bytes beyond the refusal byte unread, as a growing input could.
        with archive.open("r+b") as stream:
            stream.truncate(limit + 8192)
        reads = {}
        with patch.object(Path, "stat", new=self.stale_size(archive)), \
                patch.object(Path, "open", new=self.audit_reads({archive: limit + 1}, reads)), \
                patch.object(Path, "read_bytes", side_effect=AssertionError("whole-file artifact read")), \
                patch.object(windows_release.hashlib, "sha256", side_effect=AssertionError("oversized ZIP hashed")):
            with self.assertRaisesRegex(ValueError, "archive exceeds its byte limit"):
                paired._record(self.directory(target))
        self.assertEqual(reads, {archive: [(limit + 1, limit + 1)]})

    def test_valid_archives_use_one_bounded_snapshot_each(self):
        archives = {
            self.directory(target) / f"locron-v{VERSION}-{target}.zip"
            for target in paired.TARGETS
        }
        limit = 64 * 1024 * 1024
        reads = {}
        with patch.object(Path, "open", new=self.audit_reads({path: limit + 1 for path in archives}, reads)), \
                patch.object(Path, "read_bytes", side_effect=AssertionError("whole-file artifact read")):
            result = paired.verify(self.root, self.expected())
        self.assertEqual(set(result), set(paired.TARGETS))
        self.assertEqual(set(reads), archives)
        for path, calls in reads.items():
            self.assertEqual(calls, [(limit + 1, path.stat().st_size)])

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
