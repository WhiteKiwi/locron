#!/usr/bin/env python3
"""Hermetic publication byte-binding tests: gh is controlled, files/ZIPs are real."""
import contextlib
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import release_snapshot
import windows_release as windows

spec = importlib.util.spec_from_file_location("snapshot_assets", Path(__file__).with_name("release-assets.py"))
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)


def pe(machine, subsystem):
    binary = bytearray(512)
    binary[:2] = b"MZ"
    struct.pack_into("<I", binary, 60, 128)
    binary[128:132] = b"PE\0\0"
    for offset, value in ((132, machine), (134, 1), (148, 240), (150, 0x22), (152, 0x20B), (220, subsystem)):
        struct.pack_into("<H", binary, offset, value)
    struct.pack_into("<I", binary, 260, 16)
    return bytes(binary)


class PublicationSnapshot(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="locron-release-snapshot-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def inputs(self, tag="v0.10.0"):
        directory = self.root / tag / "release inputs 한글"
        directory.mkdir(parents=True)
        for name in assets.expected_assets(tag):
            if not name.endswith(".zip"):
                (directory / name).write_bytes(f"fixture {name}".encode())
        if assets.includes_windows(tag):
            for target, machine in windows.TARGETS.items():
                contents = {name: pe(machine, windows.SUBSYSTEMS[name]) if name in windows.SUBSYSTEMS
                            else b"fixture documentation" for name in windows.PAIRED_FILES}
                windows.write_archive(directory / f"locron-{tag}-{target}.zip", tag, target, contents, windows.PAIRED_FILES)
        (directory / "SHA256SUMS.txt").write_text("".join(
            f"{assets.sha(directory / name)}  {name}\n" for name in sorted(assets.expected_assets(tag))), encoding="utf-8")
        installers = directory.parent / "installers"
        installers.mkdir()
        for name in ("install.sh", "install.ps1", "uninstall.ps1"):
            (installers / name).write_bytes(f"unexecuted fixture {name}".encode())
        notes = directory.parent / "notes.md"
        notes.write_text("Curated fixture notes", encoding="utf-8")
        return directory, installers, notes

    def publish(self, tag, directory, installers, notes):
        with contextlib.redirect_stdout(io.StringIO()):
            assets.publish(tag, directory, installers / "install.sh", notes,
                           installers / "install.ps1", installers / "uninstall.ps1")

    def validate(self, tag, directory, installers, **kwargs):
        return assets.validate_inputs(tag, directory, installers / "install.sh",
                                      windows_installer=installers / "install.ps1",
                                      windows_uninstaller=installers / "uninstall.ps1", **kwargs)

    def payload_paths(self, tag, directory, installers):
        result = {path.name: path for path in directory.iterdir()}
        result.update(assets.expected_installers(tag, installers / "install.sh", installers / "install.ps1",
                                                installers / "uninstall.ps1"))
        return result

    def test_source_mutation_during_api_cannot_change_any_uploaded_byte(self):
        tag = "v0.10.0"
        directory, installers, notes = self.inputs(tag)
        originals = self.payload_paths(tag, directory, installers)
        expected = {name: path.read_bytes() for name, path in originals.items()}
        expected_notes = notes.read_bytes()
        uploaded = []
        def gh(command, **kwargs):
            if command[1] == "api":
                for path in (*originals.values(), notes):
                    path.write_bytes(b"changed after snapshot validation")
                return subprocess.CompletedProcess(command, 1, "", "HTTP 404")
            self.assertEqual(command[:4], ["gh", "release", "create", tag])
            self.assertIn("--verify-tag", command)
            self.assertTrue(kwargs["check"])
            self.assertEqual(kwargs["timeout"], 600)
            end = command.index("--notes-file")
            uploaded.extend(Path(value) for value in command[9:end])
            self.assertEqual({path.name: path.read_bytes() for path in uploaded}, expected)
            self.assertEqual(Path(command[end + 1]).read_bytes(), expected_notes)
            self.assertTrue(all(path not in originals.values() for path in uploaded))
            return subprocess.CompletedProcess(command, 0)
        with patch.object(assets.subprocess, "run", side_effect=gh) as execute:
            self.publish(tag, directory, installers, notes)
        self.assertEqual(execute.call_count, 2)
        self.assertEqual(len(uploaded), 14)
        self.assertTrue(all(not path.exists() for path in uploaded))
        self.assertTrue(all(path.read_bytes() == b"changed after snapshot validation" for path in originals.values()))

    def test_legacy_publication_inventory_and_basenames_are_preserved(self):
        for tag, count in (("v0.2.0", 9), ("v0.9.6", 10)):
            with self.subTest(tag=tag):
                directory, installers, notes = self.inputs(tag)
                expected = {name: path.read_bytes() for name, path in self.payload_paths(tag, directory, installers).items()}
                def gh(command, **kwargs):
                    if command[1] == "api":
                        return subprocess.CompletedProcess(command, 1, "", "HTTP 404")
                    paths = [Path(value) for value in command[9:command.index("--notes-file")]]
                    self.assertEqual(len(paths), count)
                    self.assertEqual({path.name: path.read_bytes() for path in paths}, expected)
                    return subprocess.CompletedProcess(command, 0)
                with patch.object(assets.subprocess, "run", side_effect=gh):
                    self.publish(tag, directory, installers, notes)

    def test_existing_release_noop_still_allows_absent_notes(self):
        import json
        tag = "v0.10.0"
        directory, installers, notes = self.inputs(tag)
        hashes = self.validate(tag, directory, installers)
        release = {"tag_name": tag, "assets": [{"name": name, "digest": "sha256:" + digest} for name, digest in hashes.items()]}
        notes.unlink()
        with patch.object(assets.subprocess, "run", return_value=subprocess.CompletedProcess("gh", 0, json.dumps(release), "")) as execute:
            self.publish(tag, directory, installers, notes)
            self.assertEqual(execute.call_count, 1)
            self.assertEqual(execute.call_args.args[0][1], "api")

    def test_existing_release_drift_refuses_without_create(self):
        import json
        tag = "v0.10.0"
        directory, installers, notes = self.inputs(tag)
        hashes = self.validate(tag, directory, installers)
        release = {"tag_name": tag, "assets": [{"name": name, "digest": "sha256:" + digest} for name, digest in hashes.items()]}
        release["assets"][0]["digest"] = "sha256:" + "0" * 64
        with patch.object(assets.subprocess, "run", return_value=subprocess.CompletedProcess("gh", 0, json.dumps(release), "")) as execute:
            with self.assertRaises(ValueError):
                self.publish(tag, directory, installers, notes)
            self.assertEqual(execute.call_count, 1)

    def test_checksum_refusal_occurs_before_any_gh_call(self):
        tag = "v0.10.0"
        directory, installers, notes = self.inputs(tag)
        path = directory / "SHA256SUMS.txt"
        path.write_bytes(b"malformed checksums")
        with patch.object(assets.subprocess, "run") as execute, self.assertRaises(ValueError):
            try:
                self.publish(tag, directory, installers, notes)
            finally:
                execute.assert_not_called()
        self.assertEqual(path.read_bytes(), b"malformed checksums")

    def test_missing_notes_for_new_release_never_reaches_create(self):
        tag = "v0.9.6"
        directory, installers, notes = self.inputs(tag)
        notes.unlink()
        with patch.object(assets.subprocess, "run", return_value=subprocess.CompletedProcess("gh", 1, "", "HTTP 404")) as execute:
            with self.assertRaisesRegex(ValueError, "curated changelog"):
                self.publish(tag, directory, installers, notes)
            self.assertEqual(execute.call_count, 1)

    def test_upload_error_cleans_scratch_and_preserves_sources(self):
        tag = "v0.10.0"
        directory, installers, notes = self.inputs(tag)
        original = {path: path.read_bytes() for path in self.payload_paths(tag, directory, installers).values()}
        captured = []
        def gh(command, **kwargs):
            if command[1] == "api":
                return subprocess.CompletedProcess(command, 1, "", "HTTP 404")
            captured.extend(Path(value) for value in command[9:command.index("--notes-file")])
            raise subprocess.CalledProcessError(1, command)
        with patch.object(assets.subprocess, "run", side_effect=gh), self.assertRaises(subprocess.CalledProcessError):
            self.publish(tag, directory, installers, notes)
        self.assertTrue(captured)
        self.assertTrue(all(not path.exists() for path in captured))
        self.assertEqual({path: path.read_bytes() for path in original}, original)

    def test_unsafe_source_types_refuse_before_network(self):
        tag = "v0.10.0"
        directory, installers, notes = self.inputs(tag)
        path = installers / "install.ps1"
        saved = path.read_bytes()
        for kind in ("directory", "symlink", "fifo"):
            with self.subTest(kind=kind):
                path.unlink()
                if kind == "directory":
                    path.mkdir()
                elif kind == "symlink":
                    path.symlink_to(notes)
                else:
                    os.mkfifo(path)
                with patch.object(assets.subprocess, "run") as execute, self.assertRaises((ValueError, OSError)):
                    try:
                        self.publish(tag, directory, installers, notes)
                    finally:
                        execute.assert_not_called()
                if kind == "directory":
                    path.rmdir()
                else:
                    path.unlink()
                path.write_bytes(saved)

    def test_extra_inventory_refuses_before_copy(self):
        tag = "v0.10.0"
        directory, installers, notes = self.inputs(tag)
        (directory / "unowned").write_bytes(b"keep")
        with patch.object(assets, "snapshot", side_effect=AssertionError("unexpected copy")), self.assertRaises(ValueError):
            self.publish(tag, directory, installers, notes)
        self.assertEqual((directory / "unowned").read_bytes(), b"keep")

    def test_windows_digest_is_the_digest_of_the_inspected_snapshot(self):
        tag = "v0.10.0"
        directory, installers, _ = self.inputs(tag)
        expected = self.validate(tag, directory, installers)
        inspect = windows.inspect_archive
        def replaced_after_inspection(path, *args, **kwargs):
            facts = inspect(path, *args, **kwargs)
            path.write_bytes(b"different path bytes after inspection")
            return facts
        with patch.object(windows, "inspect_archive", side_effect=replaced_after_inspection):
            result = self.validate(tag, directory, installers)
        for name, digest in expected.items():
            self.assertEqual(result[name], digest)
        self.assertTrue(all(assets.sha(directory / name) != expected[name] for name in expected if name.endswith(".zip")))

    def test_checksum_parse_and_returned_digest_share_one_read(self):
        tag = "v0.9.6"
        directory, installers, _ = self.inputs(tag)
        path = directory / "SHA256SUMS.txt"
        expected = hashlib.sha256(path.read_bytes()).hexdigest()
        parse = assets.checksums
        def changed_after_parse(text):
            result = parse(text)
            path.write_bytes(b"later path contents")
            return result
        with patch.object(assets, "checksums", side_effect=changed_after_parse):
            hashes = self.validate(tag, directory, installers)
        self.assertEqual(hashes["SHA256SUMS.txt"], expected)
        self.assertNotEqual(assets.sha(path), expected)

    def test_size_bound_and_copy_error_remove_only_temporary_files(self):
        source = self.root / "source"
        source.write_bytes(b"source")
        with self.assertRaises(ValueError), release_snapshot.snapshot({"payload": source}, {"payload": 2}):
            self.fail("oversized source admitted")
        roots = []
        copy = release_snapshot.copy_regular
        def failed_second(path, destination, limit):
            roots.append(destination.parent)
            if destination.name == "second":
                raise OSError("copy fixture failure")
            return copy(path, destination, limit)
        with patch.object(release_snapshot, "copy_regular", side_effect=failed_second), self.assertRaises(OSError):
            with release_snapshot.snapshot({"first": source, "second": source}):
                self.fail("failed snapshot admitted")
        self.assertTrue(roots)
        self.assertTrue(all(not path.exists() for path in roots))
        self.assertEqual(source.read_bytes(), b"source")

    def test_copy_refuses_substitution_between_lstat_and_open(self):
        source, destination = self.root / "source", self.root / "destination"
        source.write_bytes(b"original")
        real_open = os.open
        def replaced(path, flags, *args, **kwargs):
            if Path(path) == source:
                source.rename(self.root / "original")
                source.write_bytes(b"original")
            return real_open(path, flags, *args, **kwargs)
        with patch.object(release_snapshot.os, "open", side_effect=replaced), self.assertRaises(ValueError):
            release_snapshot.copy_regular(source, destination)
        self.assertFalse(destination.exists())

    def test_copy_refuses_growth_truncation_and_same_length_change(self):
        source = self.root / "source"
        fdopen = os.fdopen
        for change in ("grow", "truncate", "same-length"):
            with self.subTest(change=change):
                source.write_bytes(b"abcdefgh" * 8)
                class ChangingReader:
                    def __init__(self, stream):
                        self.stream, self.first = stream, True
                    def __enter__(self):
                        return self
                    def __exit__(self, *args):
                        return self.stream.__exit__(*args)
                    def read(self, size):
                        result = self.stream.read(size)
                        if self.first:
                            self.first = False
                            value = b"x" * (65 if change == "grow" else 8 if change == "truncate" else 64)
                            source.write_bytes(value)
                            os.utime(source, ns=(1, 1))
                        return result
                def reader(*args, **kwargs):
                    return ChangingReader(fdopen(*args, **kwargs))
                with patch.object(release_snapshot, "CHUNK_BYTES", 8), \
                        patch.object(release_snapshot.os, "fdopen", side_effect=reader), self.assertRaises(ValueError):
                    with release_snapshot.snapshot({"payload": source}):
                        self.fail("changed file admitted")

    def test_snapshot_names_cannot_escape_or_collide(self):
        source = self.root / "source"
        source.write_bytes(b"keep")
        for sources in ({"../outside": source}, {"/absolute": source}, {"C:\\outside": source},
                        {"a//b": source}, {"A": source, "a": source}):
            with self.subTest(sources=sources), self.assertRaises(ValueError), release_snapshot.snapshot(sources):
                self.fail("unsafe snapshot name admitted")

    def test_copy_never_overwrites_existing_destination(self):
        source, destination = self.root / "source", self.root / "destination"
        source.write_bytes(b"source")
        destination.write_bytes(b"keep")
        with self.assertRaises(FileExistsError):
            release_snapshot.copy_regular(source, destination)
        self.assertEqual(destination.read_bytes(), b"keep")


if __name__ == "__main__":
    unittest.main()
