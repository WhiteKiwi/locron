#!/usr/bin/env python3
"""Direct WinGet writer recovery controls; native filesystem proof is opt-in."""
import argparse
from contextlib import ExitStack, contextmanager, redirect_stdout
import errno
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import platform
import stat
import struct
import subprocess
import sys
import sysconfig
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "winget_output", Path(__file__).with_name("render-winget-manifest.py"))
winget = importlib.util.module_from_spec(spec)
spec.loader.exec_module(winget)

REAL_OPEN = Path.open
REAL_MKDIR = Path.mkdir
REAL_LSTAT = Path.lstat
REAL_UNLINK = Path.unlink
REAL_RMDIR = Path.rmdir
REAL_FSTAT = os.fstat
windows = winget.windows_release
TAG = "v0.10.0"
DOCUMENTS = {
    "first.yaml": "first: café\nsecond line\n",
    "second.yaml": "second: 한글\n",
    "third.yaml": "third: complete\n",
}
EXPECTED = {
    "first.yaml": b"first: caf\xc3\xa9\nsecond line\n",
    "second.yaml": b"second: \xed\x95\x9c\xea\xb8\x80\n",
    "third.yaml": b"third: complete\n",
}
SENTINEL = b"parent sentinel\n"
FOREIGN = b"interposed fixture bytes\n"


def identity(facts):
    return facts.st_dev, facts.st_ino, stat.S_IFMT(facts.st_mode)


class NoNotesError(OSError):
    """Exercise the writer's existing optional exception-note fallback."""
    add_note = None


def pe(machine, subsystem):
    """Static, nonexecuted PE fixture with the actual ZIP inspector's required headers."""
    binary = bytearray(512)
    binary[:2] = b"MZ"
    struct.pack_into("<I", binary, 60, 128)
    binary[128:132] = b"PE\0\0"
    for offset, value in ((132, machine), (134, 1), (148, 240), (150, 0x22), (152, 0x20B), (220, subsystem)):
        struct.pack_into("<H", binary, offset, value)
    struct.pack_into("<I", binary, 260, 16)
    return bytes(binary)


def frozen_manifests(digests):
    """Independent complete byte oracle frozen from the pre-recovery renderer templates."""
    base = 'PackageIdentifier: WhiteKiwi.locron\nPackageVersion: "0.10.0"\n'
    version = base + "DefaultLocale: en-US\nManifestType: version\nManifestVersion: 1.12.0\n"
    locale = base + (
        "PackageLocale: en-US\nPublisher: WhiteKiwi\n"
        "PublisherUrl: https://github.com/WhiteKiwi\n"
        "PublisherSupportUrl: https://github.com/WhiteKiwi/locron/issues\n"
        "PackageName: locron\nPackageUrl: https://github.com/WhiteKiwi/locron\n"
        "License: MIT OR Apache-2.0\n"
        "LicenseUrl: https://github.com/WhiteKiwi/locron/blob/main/LICENSE-MIT\n"
        "ShortDescription: A local-first job scheduler with a command line interface.\n"
        "Moniker: locron\nManifestType: defaultLocale\nManifestVersion: 1.12.0\n"
    )
    installer = base + (
        "MinimumOSVersion: 10.0.22000.0\nInstallerType: zip\nNestedInstallerType: portable\n"
        "UpgradeBehavior: install\nCommands:\n- locron\nInstallers:\n"
    )
    for architecture, target in (("x64", "x86_64-pc-windows-msvc"), ("arm64", "aarch64-pc-windows-msvc")):
        name = f"locron-v0.10.0-{target}.zip"
        installer += (
            f"- Architecture: {architecture}\n"
            f"  InstallerUrl: https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/{name}\n"
            f"  InstallerSha256: {digests[name].upper()}\n"
            "  NestedInstallerFiles:\n"
            f"  - RelativeFilePath: locron-v0.10.0-{target}/locron.exe\n"
            "    PortableCommandAlias: locron\n"
        )
    installer += "ManifestType: installer\nManifestVersion: 1.12.0\n"
    return {name: (
        f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.{kind}.1.12.0.schema.json\n" + body
    ).encode("utf-8") for name, kind, body in (
        ("WhiteKiwi.locron.yaml", "version", version),
        ("WhiteKiwi.locron.locale.en-US.yaml", "defaultLocale", locale),
        ("WhiteKiwi.locron.installer.yaml", "installer", installer),
    )}


def release_inputs(owner, paired):
    inputs = owner.mkdir(owner.root / "final inputs 한글")
    digests = {}
    for name in sorted(winget.assets.expected_assets(TAG)):
        if not name.endswith(".zip"):
            owner.seed(inputs / name, b"unrelated release payload fixture\n")
    files = windows.PAIRED_FILES if paired else windows.FILES
    for target, machine in windows.TARGETS.items():
        contents = {name: pe(machine, windows.SUBSYSTEMS[name]) if name in windows.SUBSYSTEMS
                    else b"fixture documentation\n" for name in files}
        archive = inputs / f"locron-{TAG}-{target}.zip"
        windows.write_archive(archive, TAG, target, contents, files)
        owner.register(archive)
    for name in sorted(winget.assets.expected_assets(TAG)):
        digests[name] = hashlib.sha256((inputs / name).read_bytes()).hexdigest()
    owner.seed(inputs / "SHA256SUMS.txt", "".join(
        f"{digest}  {name}\n" for name, digest in digests.items()).encode("utf-8"))
    return inputs, frozen_manifests(digests)


class Receipt:
    def __init__(self, expected):
        self.expected = set(expected)
        if len(self.expected) != len(expected):
            raise AssertionError("duplicate expected control keys")
        self.completed = []
        self.observations = []

    def complete(self, case, control, owner):
        key = f"{case.id()}:{control}"
        case.assertIn(key, self.expected, "unknown completed control")
        case.assertNotIn(key, self.completed)
        case.assertTrue(owner.cleaned, "checked fixture cleanup precedes completion")
        case.assertTrue(all(stream.closed for stream in owner.streams))
        case.assertTrue(all(stream.closed for stream in owner.seed_streams))
        case.assertFalse(owner.readonly)
        self.observations.append({
            "control": key, "operations": owner.operations,
            "writer_streams": len(owner.streams),
            "closed_streams": sum(stream.closed for stream in owner.streams),
            "identities": owner.identities, "returned_errors": owner.errors,
        })
        self.completed.append(key)


class TrackedStream:
    """Only delegates to an actual owned binary file, including fileno and close."""
    def __init__(self, owner, raw, name, stage=None, origin=None):
        self.owner, self.raw, self.name = owner, raw, name
        self.stage, self.origin = stage, origin
        self.opened_identity = None

    @property
    def closed(self):
        return self.raw.closed

    def fileno(self):
        return self.raw.fileno()

    def write(self, data):
        self.owner.operations["write"] += 1
        if self.stage in ("write", "short-write"):
            written = self.raw.write(data[:max(1, len(data) // 2)])
            if self.stage == "write":
                raise self.origin
            return written
        return self.raw.write(data)

    def flush(self):
        self.owner.operations["flush"] += 1
        self.raw.flush()
        if self.stage == "flush":
            raise self.origin

    def close(self):
        self.owner.operations["close"] += 1
        self.raw.close()

    def __enter__(self):
        self.raw.__enter__()
        return self

    def __exit__(self, kind, value, traceback):
        self.close()
        if self.stage == "close" and kind is None:
            raise self.origin
        return False


class CaseOwner:
    """One disposable parent owns every stream, replacement and cleanup callback."""
    def __init__(self, case):
        self.case = case
        self.root = Path(tempfile.mkdtemp(prefix="locron winget 한글 "))
        self.parent = self.root / "existing parent 폴더"
        self.generated_parent = self.parent / "created parent"
        self.output = self.generated_parent / "output 한글"
        self.sentinel = self.parent / "sentinel"
        self.streams, self.seed_streams = [], []
        self.readonly = []
        self.identities, self.errors = [], []
        self.operations = dict.fromkeys(
            ("open", "mkdir", "write", "flush", "close", "lstat", "fstat", "unlink", "rmdir"), 0)
        self.unlink_order = []
        self.rmdir_errors = []
        self.cleaned = False
        self.cleanup_error = None
        self.root_identity = None
        self.owned = {}
        # Register the owner before any fallible seed I/O. Later attribute
        # cleanups are registered after this and therefore run first in unittest.
        case.addCleanup(self.cleanup)

    def __enter__(self):
        self.root_identity = self.capture(self.root)
        self.owned[self.root] = self.root_identity
        self.mkdir(self.parent)
        self.seed(self.sentinel, SENTINEL)
        self.parent_identity = self.capture(self.parent)
        return self

    def __exit__(self, kind, value, traceback):
        self.cleanup()
        return False

    def capture(self, path, facts=None):
        if facts is None:
            facts = REAL_LSTAT(path)
        result = identity(facts)
        if self.case.native:
            self.case.assertGreater(result[1], 0, "native fixture identity is mandatory")
        self.identities.append(list(result))
        return result

    def present(self, path):
        try:
            REAL_LSTAT(path)
        except FileNotFoundError:
            return False
        return True

    def guard_root(self):
        self.case.assertIsNotNone(self.root_identity, "unknown fixture root cannot be mutated")
        self.case.assertEqual(self.capture(self.root), self.root_identity,
                              "never mutate through a substituted fixture root")

    def guard(self, path):
        self.guard_root()
        path.relative_to(self.root)
        for parent in reversed(path.parents):
            if parent == self.root or self.root in parent.parents:
                self.case.assertEqual(self.capture(parent), self.owned.get(parent),
                                      "never mutate through a substituted fixture parent")
        facts = REAL_LSTAT(path)
        self.case.assertEqual(self.capture(path, facts), self.owned.get(path),
                              "never mutate a substituted fixture leaf")
        return facts

    def register(self, path, facts=None):
        self.guard_root()
        self.guard(path.parent)
        current = REAL_LSTAT(path)
        if facts is not None:
            self.case.assertEqual(identity(current), identity(facts), "created fd/path identity mismatch")
        self.owned[path] = self.capture(path, current)
        return self.owned[path]

    def mkdir(self, path):
        self.guard(path.parent)
        path.mkdir()
        self.register(path)
        return path

    def move(self, source, destination):
        self.guard(source)
        self.guard(destination.parent)
        self.case.assertFalse(self.present(destination), "owned rename requires an absent destination")
        moved = {path: captured for path, captured in self.owned.items()
                 if path == source or source in path.parents}
        for path in moved:
            if self.present(path):
                self.guard(path)
        source.rename(destination)
        for path, captured in moved.items():
            del self.owned[path]
            self.owned[destination / path.relative_to(source)] = captured
        self.guard(destination)

    def delete(self, path):
        facts = self.guard(path)
        if stat.S_ISDIR(facts.st_mode):
            REAL_RMDIR(path)
        else:
            REAL_UNLINK(path)
        del self.owned[path]

    def symlink(self, path, target):
        self.guard(path.parent)
        self.guard(target)
        path.symlink_to(target, target_is_directory=True)  # Unsupported setup fails, never skips.
        self.register(path)

    def adopt_generation(self, names=EXPECTED):
        # Only a known, unpatched generation into a proved-absent output may register its new objects.
        if self.generated_parent not in self.owned:
            self.register(self.generated_parent)
        else:
            self.guard(self.generated_parent)
        self.register(self.output)
        self.case.assertEqual({leaf.name for leaf in self.output.iterdir()}, set(names))
        for name in names:
            self.register(self.output / name)

    def seed(self, path, data):
        self.case.assertLessEqual(len(data), 4096)
        self.guard(path.parent)
        raw = REAL_OPEN(path, "xb")
        self.seed_streams.append(raw)
        with raw:
            self.register(path, REAL_FSTAT(raw.fileno()))
            self.case.assertEqual(raw.write(data), len(data))
            raw.flush()
            return self.capture(path, REAL_FSTAT(raw.fileno()))

    def assert_closed(self):
        self.case.assertTrue(all(stream.closed for stream in self.streams))
        self.case.assertTrue(all(stream.closed for stream in self.seed_streams))

    def assert_parent(self):
        self.case.assertEqual(self.capture(self.parent), self.parent_identity)
        self.case.assertEqual(self.sentinel.read_bytes(), SENTINEL)

    def readonly_file(self, path):
        original = self.capture(path)
        original_mode = self.guard(path).st_mode
        self.case.assertTrue(original_mode & stat.S_IWRITE, "owned fixture starts writable before readonly mutation")
        self.readonly.append((path, original, original_mode))
        self.case.addCleanup(self.restore_readonly)
        # Both cleanup registrations exist before the actual attribute change.
        self.guard(path)
        os.chmod(path, stat.S_IREAD)
        return original

    def restore_readonly(self):
        for path, original, original_mode in list(self.readonly):
            self.guard(path)
            self.case.assertEqual(self.capture(path), original,
                                  "never clear an attribute on a replacement")
            os.chmod(path, original_mode)
            self.case.assertEqual(REAL_LSTAT(path).st_mode, original_mode, "restore the saved original mode")
            self.readonly.remove((path, original, original_mode))

    def cleanup(self):
        if self.cleanup_error is not None:
            raise self.cleanup_error  # Report again; do not retry unknown teardown.
        if self.cleaned:
            return
        try:
            for stream in [*self.streams, *self.seed_streams]:
                if not stream.closed:
                    stream.close()
            self.assert_closed()
            self.guard_root()
            self.restore_readonly()
            self.case.assertEqual(self.capture(self.root), self.root_identity,
                                  "cleanup only the original disposable case root")
            # Preflight every extant recorded object before any nonrecursive deletion.
            for path in self.owned:
                if self.present(path):
                    self.guard(path)
            for path in sorted(self.owned, key=lambda entry: len(entry.parts), reverse=True):
                if not self.present(path):
                    del self.owned[path]  # A recorded object already removed by production.
                else:
                    self.delete(path)
        except BaseException as error:
            self.cleanup_error = error
            raise
        self.cleaned = True

    @contextmanager
    def intercept(self, stage=None, position=None, origin=None, before_open=None,
                  lstat_fault=None, fstat_fault=None, unlink_fault=None, rmdir_fault=None):
        def open_file(path, mode="r", *args, **kwargs):
            if path.parent != self.output or mode != "xb":
                return REAL_OPEN(path, mode, *args, **kwargs)
            self.operations["open"] += 1
            index = self.operations["open"]
            if before_open is not None:
                before_open(path, index)
            if stage == "open" and index == position:
                raise origin
            raw = REAL_OPEN(path, mode, *args, **kwargs)
            selected_stage = stage if index == position else None
            stream = TrackedStream(self, raw, path.name, selected_stage, origin)
            self.streams.append(stream)
            self.register(path, REAL_FSTAT(raw.fileno()))
            # This is a real fixture observation; the writer still performs its
            # own unmodified fstat, which may be refused in the unknown-ID case.
            stream.opened_identity = self.capture(path, REAL_FSTAT(stream.fileno()))
            return stream

        def make_directory(path, *args, **kwargs):
            if path in (self.output, self.generated_parent):
                self.operations["mkdir"] += 1
            result = REAL_MKDIR(path, *args, **kwargs)
            if path in (self.output, self.generated_parent):
                self.register(path)
            return result

        def path_stat(path):
            if path == self.output or path.parent == self.output:
                self.operations["lstat"] += 1
                if lstat_fault is not None:
                    lstat_fault(path)
            return REAL_LSTAT(path)

        def file_stat(fd):
            if any(not stream.closed and stream.fileno() == fd for stream in self.streams):
                self.operations["fstat"] += 1
                if fstat_fault is not None:
                    fstat_fault(fd)
            return REAL_FSTAT(fd)

        def unlink_file(path, *args, **kwargs):
            if path.parent != self.output:
                return REAL_UNLINK(path, *args, **kwargs)
            self.operations["unlink"] += 1
            self.unlink_order.append(path.name)
            if unlink_fault is not None:
                unlink_fault(path)
            try:
                return REAL_UNLINK(path, *args, **kwargs)
            except OSError as error:
                self.errors.append(error_facts("unlink", error))
                raise

        def remove_directory(path):
            if path != self.output:
                return REAL_RMDIR(path)
            self.operations["rmdir"] += 1
            if rmdir_fault is not None:
                rmdir_fault(path)
            try:
                return REAL_RMDIR(path)
            except OSError as error:
                self.rmdir_errors.append(error)
                self.errors.append(error_facts("rmdir", error))
                raise

        with ExitStack() as stack:
            for owner, name, replacement in (
                    (Path, "open", open_file), (Path, "mkdir", make_directory),
                    (Path, "lstat", path_stat), (os, "fstat", file_stat),
                    (Path, "unlink", unlink_file), (Path, "rmdir", remove_directory)):
                stack.enter_context(patch.object(owner, name, replacement))
            yield


def error_facts(operation, error):
    return {"operation": operation, "type": type(error).__name__,
            "errno": error.errno, "winerror": getattr(error, "winerror", None)}


class RecoveryAssertions:
    def complete(self, control, owner):
        self.receipt.complete(self, control, owner)

    def assert_documents(self, owner):
        self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, set(EXPECTED))
        for name, expected in EXPECTED.items():
            actual = (owner.output / name).read_bytes()
            self.assertEqual(actual, expected)
            self.assertNotIn(b"\r\n", actual)
        owner.assert_closed()
        owner.assert_parent()

    def snapshot(self, owner, path):
        facts = REAL_LSTAT(path)
        result = [(".", owner.capture(path, facts), facts.st_mode,
                   path.read_bytes() if stat.S_ISREG(facts.st_mode) else None)]
        if stat.S_ISDIR(facts.st_mode):
            for leaf in sorted(path.rglob("*")):
                facts = REAL_LSTAT(leaf)
                result.append((leaf.relative_to(path).as_posix(), owner.capture(leaf, facts), facts.st_mode,
                               leaf.read_bytes() if stat.S_ISREG(facts.st_mode) else None))
        return result

    def assert_origin(self, returned, origin, args):
        self.assertIs(returned, origin)
        self.assertEqual(returned.args, args)
        self.assertEqual(type(returned), type(origin))

    def assert_notes(self, error, *fragments):
        notes = getattr(error, "__notes__", ())
        for fragment in fragments:
            self.assertTrue(any(fragment in note for note in notes), fragment)

    def repeat_success(self, owner):
        # This is one explicit, unpatched new call, never a failed-test retry.
        self.assertIs(winget._write_documents(owner.output, dict(DOCUMENTS)), owner.output)
        owner.adopt_generation()
        self.assert_documents(owner)

    def repeat_refusal(self, owner):
        before = self.snapshot(owner, owner.output)
        with self.assertRaises(FileExistsError):
            winget._write_documents(owner.output, dict(DOCUMENTS))
        self.assertEqual(self.snapshot(owner, owner.output), before)
        owner.assert_parent()

    def replacement_control(self, owner, directory):
        origin = OSError(errno.EIO, "injected third-open origin")
        original_args = origin.args
        recorded = {}

        def replace(path, index):
            if index != 3:
                return
            owner.assert_closed()
            if directory:
                parked = owner.generated_parent / "parked original directory"
                recorded["original"] = owner.capture(owner.output)
                owner.move(owner.output, parked)
                owner.mkdir(owner.output)
                owner.seed(owner.output / "foreign.yaml", FOREIGN)
                recorded["replacement"] = owner.capture(owner.output)
                recorded["parked"] = self.snapshot(owner, parked)
                recorded["foreign"] = self.snapshot(owner, owner.output)
                recorded["parked_path"] = parked
            else:
                first = owner.output / "first.yaml"
                parked = owner.generated_parent / "parked original file.yaml"
                recorded["original"] = owner.capture(first)
                owner.move(first, parked)  # Keep the old ID live; do not assume no ID reuse.
                recorded["replacement"] = owner.seed(first, FOREIGN)
                recorded["foreign"] = self.snapshot(owner, first)
                recorded["parked"] = self.snapshot(owner, parked)
                recorded["parked_path"] = parked
            raise origin

        with owner.intercept(before_open=replace):
            with self.assertRaises(OSError) as raised:
                winget._write_documents(owner.output, dict(DOCUMENTS))
        self.assert_origin(raised.exception, origin, original_args)
        self.assertEqual(recorded["original"][0], recorded["replacement"][0])
        self.assertEqual(recorded["original"][2], recorded["replacement"][2])
        self.assertNotEqual(recorded["original"][1], recorded["replacement"][1])
        self.assertEqual(self.snapshot(owner, recorded["parked_path"]), recorded["parked"])
        if directory:
            self.assertEqual(self.snapshot(owner, owner.output), recorded["foreign"])
            self.assertEqual(owner.operations["unlink"], 0)
            self.assertEqual(owner.operations["rmdir"], 0)
            self.assert_notes(origin, "output directory changed", "identity is unavailable")
        else:
            self.assertEqual(self.snapshot(owner, owner.output / "first.yaml"), recorded["foreign"])
            self.assertFalse((owner.output / "second.yaml").exists())
            self.assertEqual(owner.unlink_order, ["second.yaml"])
            self.assert_notes(origin, "retained replaced manifest", "retained output directory")
        owner.assert_closed()
        self.repeat_refusal(owner)


class PortableRecovery(RecoveryAssertions, unittest.TestCase):
    def test_success_writes_exact_utf8_lf_and_returns_output(self):
        with self.subTest(control="success"):
            with CaseOwner(self) as owner:
                with owner.intercept():
                    self.assertIs(winget._write_documents(owner.output, dict(DOCUMENTS)), owner.output)
                self.assert_documents(owner)
                self.assertEqual(len(owner.streams), 3)
                self.assertEqual(owner.operations["write"], 3)
                self.assertEqual(owner.operations["flush"], 3)
                self.assertEqual(owner.operations["close"], 3)
            self.complete("success", owner)

    def test_preflight_refusals_have_no_output_or_parent_effect(self):
        invalid = ("", ".", "..", "a/b", "a\\b", "a:b", "a\0b", None, 17)
        controls = [(f"name-{index}", {name: "text\n"}, ValueError)
                    for index, name in enumerate(invalid)]
        controls += [("empty", {}, ValueError),
                     ("second-encoding", {"first.yaml": "valid\n", "second.yaml": "\ud800"},
                      UnicodeEncodeError)]
        for label, documents, error_type in controls:
            with self.subTest(control=label):
                with CaseOwner(self) as owner:
                    before = self.snapshot(owner, owner.parent)
                    with owner.intercept():
                        with self.assertRaises(error_type):
                            winget._write_documents(owner.output, documents)
                    self.assertFalse(owner.output.exists())
                    self.assertFalse(owner.generated_parent.exists())
                    self.assertEqual(owner.operations["open"], 0)
                    self.assertEqual(owner.operations["mkdir"], 0)
                    self.assertEqual(self.snapshot(owner, owner.parent), before)
                    owner.assert_parent()
                self.complete(label, owner)

    def test_existing_output_and_ancestors_are_preserved(self):
        for kind in ("directory", "file"):
            with self.subTest(control=kind):
                with CaseOwner(self) as owner:
                    owner.mkdir(owner.generated_parent)
                    if kind == "directory":
                        owner.mkdir(owner.output)
                        owner.seed(owner.output / "keep", FOREIGN)
                    else:
                        owner.seed(owner.output, FOREIGN)
                    before = self.snapshot(owner, owner.parent)
                    with owner.intercept():
                        with self.assertRaises(FileExistsError):
                            winget._write_documents(owner.output, dict(DOCUMENTS))
                    self.assertEqual(self.snapshot(owner, owner.parent), before)
                    self.assertEqual(owner.operations["open"], 0)
                    self.assertEqual(owner.operations["unlink"], 0)
                    self.assertEqual(owner.operations["rmdir"], 0)
                    owner.assert_parent()
                self.complete(kind, owner)

    def test_second_and_third_stage_failures_cleanup_and_allow_one_new_attempt(self):
        for position in (2, 3):
            for stage in ("open", "write", "short-write", "flush", "close"):
                label = f"{position}-{stage}"
                with self.subTest(control=label):
                    with CaseOwner(self) as owner:
                        origin = OSError(errno.EIO, "injected generation boundary")
                        original_args = origin.args
                        with owner.intercept(stage, position, origin):
                            with self.assertRaises(OSError) as raised:
                                winget._write_documents(owner.output, dict(DOCUMENTS))
                        if stage == "short-write":
                            self.assertIsNot(raised.exception, origin)
                            self.assertEqual(raised.exception.args, ("short manifest output write",))
                        else:
                            self.assert_origin(raised.exception, origin, original_args)
                        owner.assert_closed()
                        self.assertEqual(owner.operations["open"], position)
                        acquired = position - 1 if stage == "open" else position
                        self.assertEqual(len(owner.streams), acquired)
                        self.assertEqual(owner.operations["close"], acquired)
                        self.assertFalse(owner.output.exists())
                        self.assertTrue(owner.generated_parent.is_dir())
                        self.assertEqual(owner.operations["unlink"], acquired)
                        self.assertEqual(owner.operations["rmdir"], 1)
                        owner.assert_parent()
                        self.repeat_success(owner)
                    self.complete(label, owner)

    def test_cleanup_continues_and_preserves_origin_notes(self):
        for error_type in (OSError, NoNotesError):
            label = error_type.__name__
            with self.subTest(control=label):
                with CaseOwner(self) as owner:
                    origin = error_type(errno.EIO, "injected original error")
                    original_args = origin.args
                    cleanup_error = OSError(errno.EACCES, "injected cleanup error")

                    def refuse_second(path):
                        if path.name == "second.yaml":
                            raise cleanup_error

                    with owner.intercept("open", 3, origin, unlink_fault=refuse_second):
                        with self.assertRaises(OSError) as raised:
                            winget._write_documents(owner.output, dict(DOCUMENTS))
                    self.assert_origin(raised.exception, origin, original_args)
                    self.assertEqual(owner.unlink_order, ["second.yaml", "first.yaml"])
                    self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {"second.yaml"})
                    self.assertEqual((owner.output / "second.yaml").read_bytes(), EXPECTED["second.yaml"])
                    self.assertEqual(len(owner.rmdir_errors), 1)
                    if error_type is OSError:
                        self.assert_notes(origin, "injected cleanup error", "retained output directory")
                    else:
                        self.assertFalse(getattr(origin, "__notes__", ()))
                    owner.assert_closed()
                    self.repeat_refusal(owner)
                self.complete(label, owner)

    def test_unavailable_created_identities_leave_output_for_inspection(self):
        for boundary in ("directory-lstat", "first-fstat", "second-fstat"):
            with self.subTest(control=boundary):
                with CaseOwner(self) as owner:
                    origin = OSError(errno.EIO, "injected unavailable identity")
                    original_args = origin.args

                    def refuse_directory(path):
                        if path == owner.output:
                            raise origin

                    def refuse_file(fd):
                        if boundary == "first-fstat" or owner.operations["fstat"] == 2:
                            raise origin

                    options = {"lstat_fault": refuse_directory} if boundary == "directory-lstat" else {
                        "fstat_fault": refuse_file}
                    with owner.intercept(**options):
                        with self.assertRaises(OSError) as raised:
                            winget._write_documents(owner.output, dict(DOCUMENTS))
                    self.assert_origin(raised.exception, origin, original_args)
                    self.assertTrue(owner.output.is_dir())
                    if boundary != "second-fstat":
                        self.assertEqual(owner.operations["unlink"], 0)
                    if boundary == "directory-lstat":
                        self.assertEqual(list(owner.output.iterdir()), [])
                        self.assertEqual(owner.operations["open"], 0)
                        self.assertEqual(owner.operations["rmdir"], 0)
                    elif boundary == "first-fstat":
                        self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {"first.yaml"})
                        self.assertEqual((owner.output / "first.yaml").read_bytes(), b"")
                        self.assertEqual(owner.operations["rmdir"], 1)
                    else:
                        self.assertEqual(owner.operations["fstat"], 2)
                        self.assertEqual(owner.unlink_order, ["first.yaml"])
                        self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {"second.yaml"})
                        self.assertEqual((owner.output / "second.yaml").read_bytes(), b"")
                        self.assertEqual(owner.capture(owner.output / "second.yaml"), owner.owned[owner.output / "second.yaml"])
                        self.assert_notes(origin, "retained output directory")
                    owner.assert_closed()
                    self.repeat_refusal(owner)
                self.complete("A07" if boundary == "second-fstat" else boundary, owner)

    def test_disappeared_created_file_is_not_a_cleanup_error(self):
        with self.subTest(control="disappeared"):
            with CaseOwner(self) as owner:
                origin = OSError(errno.EIO, "injected third-open error")
                original_args = origin.args

                def disappear(path, index):
                    if index == 3:
                        owner.assert_closed()
                        owner.delete(owner.output / "first.yaml")
                        raise origin

                with owner.intercept(before_open=disappear):
                    with self.assertRaises(OSError) as raised:
                        winget._write_documents(owner.output, dict(DOCUMENTS))
                self.assert_origin(raised.exception, origin, original_args)
                self.assertEqual(owner.unlink_order, ["second.yaml"])
                self.assertFalse(owner.output.exists())
                self.assertFalse(getattr(origin, "__notes__", ()))
                owner.assert_closed()
                self.repeat_success(owner)
            self.complete("disappeared", owner)

    def test_unrelated_entries_survive_nonrecursive_cleanup(self):
        with self.subTest(control="unrelated"):
            with CaseOwner(self) as owner:
                origin = OSError(errno.EIO, "injected third-open error")
                original_args = origin.args
                saved = {}

                def add_foreign(path, index):
                    if index == 3:
                        owner.assert_closed()
                        owner.seed(owner.output / "unrelated", FOREIGN)
                        nested = owner.output / "nested"
                        owner.mkdir(nested)
                        owner.seed(nested / "keep", FOREIGN)
                        saved["file"] = self.snapshot(owner, owner.output / "unrelated")
                        saved["tree"] = self.snapshot(owner, nested)
                        raise origin

                with owner.intercept(before_open=add_foreign):
                    with self.assertRaises(OSError) as raised:
                        winget._write_documents(owner.output, dict(DOCUMENTS))
                self.assert_origin(raised.exception, origin, original_args)
                self.assertEqual(owner.unlink_order, ["second.yaml", "first.yaml"])
                self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {"unrelated", "nested"})
                self.assertEqual(self.snapshot(owner, owner.output / "unrelated"), saved["file"])
                self.assertEqual(self.snapshot(owner, owner.output / "nested"), saved["tree"])
                self.assertEqual(len(owner.rmdir_errors), 1)
                self.assert_notes(origin, "retained output directory")
                owner.assert_closed()
                owner.assert_parent()
            self.complete("unrelated", owner)

    def test_replaced_created_file_is_retained_while_other_owned_leaf_is_removed(self):
        with self.subTest(control="file-replacement"):
            with CaseOwner(self) as owner:
                self.replacement_control(owner, directory=False)
            self.complete("file-replacement", owner)

    def test_replaced_output_directory_retains_new_tree_and_parked_original(self):
        with self.subTest(control="directory-replacement"):
            with CaseOwner(self) as owner:
                self.replacement_control(owner, directory=True)
            self.complete("directory-replacement", owner)

    def test_rendered_manifests_match_frozen_full_bytes(self):
        for control, paired in (("A01", True), ("A02", False)):
            with self.subTest(control=control):
                with CaseOwner(self) as owner:
                    inputs, expected = release_inputs(owner, paired)
                    self.assertFalse(owner.present(owner.output))
                    self.assertIs(winget.render(TAG, inputs, owner.output, paired=paired), owner.output)
                    owner.adopt_generation(expected)
                    self.assertEqual({path.name: path.read_bytes() for path in owner.output.iterdir()}, expected)
                    owner.assert_parent()
                self.complete(control, owner)

    def test_existing_directory_symlink_preserves_owned_target(self):
        with self.subTest(control="A03"):
            with CaseOwner(self) as owner:
                owner.mkdir(owner.generated_parent)
                target = owner.mkdir(owner.root / "owned symlink target")
                owner.seed(target / "marker", FOREIGN)
                owner.symlink(owner.output, target)
                original_readlink = owner.output.readlink()
                self.assertTrue(owner.output.samefile(target))
                original = owner.capture(owner.output)
                before = self.snapshot(owner, target)
                with owner.intercept():
                    with self.assertRaises(FileExistsError):
                        winget._write_documents(owner.output, dict(DOCUMENTS))
                self.assertEqual(owner.capture(owner.output), original)
                self.assertEqual(owner.output.readlink(), original_readlink)
                self.assertTrue(owner.output.samefile(target))
                self.assertEqual(self.snapshot(owner, target), before)
                self.assertEqual(owner.operations["open"], 0)
                self.assertEqual(owner.operations["unlink"], 0)
                self.assertEqual(owner.operations["rmdir"], 0)
                owner.assert_parent()
            self.complete("A03", owner)

    def test_portable_exclusive_collisions_preserve_interposed_leaf(self):
        for control, position in (("A04", 2), ("A05", 3)):
            with self.subTest(control=control):
                with CaseOwner(self) as owner:
                    saved = {}

                    def interpose(path, index):
                        if index == position:
                            owner.assert_closed()
                            saved["path"] = path
                            saved["identity"] = owner.seed(path, FOREIGN)

                    with owner.intercept(before_open=interpose):
                        with self.assertRaises(FileExistsError) as raised:
                            winget._write_documents(owner.output, dict(DOCUMENTS))
                    owner.errors.append(error_facts("actual-exclusive-open", raised.exception))
                    self.assertEqual(raised.exception.errno, errno.EEXIST)
                    self.assertEqual(owner.capture(saved["path"]), saved["identity"])
                    self.assertEqual(saved["path"].read_bytes(), FOREIGN)
                    self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {saved["path"].name})
                    self.assertEqual(len(owner.streams), position - 1)
                    self.assertEqual(owner.operations["unlink"], position - 1)
                    owner.assert_closed()
                    self.repeat_refusal(owner)
                self.complete(control, owner)

    def test_renamed_away_directory_has_cleanup_not_confirmed_notes(self):
        with self.subTest(control="A06"):
            with CaseOwner(self) as owner:
                origin = OSError(errno.EIO, "injected third-open origin")
                original_args = origin.args
                saved = {}

                def move_away(path, index):
                    if index == 3:
                        owner.assert_closed()
                        parked = owner.generated_parent / "parked without substitute"
                        owner.move(owner.output, parked)
                        saved["path"] = parked
                        saved["tree"] = self.snapshot(owner, parked)
                        raise origin

                with owner.intercept(before_open=move_away):
                    with self.assertRaises(OSError) as raised:
                        winget._write_documents(owner.output, dict(DOCUMENTS))
                self.assert_origin(raised.exception, origin, original_args)
                self.assertFalse(owner.present(owner.output))
                self.assertEqual(self.snapshot(owner, saved["path"]), saved["tree"])
                self.assertEqual(owner.operations["unlink"], 0)
                self.assertEqual(owner.operations["rmdir"], 0)
                self.assert_notes(origin, "output directory identity is unavailable; cleanup not confirmed",
                                  "output directory is missing; cleanup not confirmed")
                owner.assert_closed()
                owner.assert_parent()
            self.complete("A06", owner)

    def test_empty_output_rmdir_failure_preserves_primary(self):
        with self.subTest(control="A08"):
            with CaseOwner(self) as owner:
                origin = OSError(errno.EIO, "injected third-open origin")
                original_args = origin.args
                cleanup_error = OSError(errno.EACCES, "injected empty-output rmdir refusal")
                calls = []

                def refuse_rmdir(path):
                    calls.append(path)
                    self.assertEqual(calls, [owner.output], "rmdir injection is single-use")
                    raise cleanup_error

                with owner.intercept("open", 3, origin, rmdir_fault=refuse_rmdir):
                    with self.assertRaises(OSError) as raised:
                        winget._write_documents(owner.output, dict(DOCUMENTS))
                self.assert_origin(raised.exception, origin, original_args)
                self.assertIsNot(raised.exception, cleanup_error)
                self.assertEqual(calls, [owner.output])
                self.assertEqual(owner.unlink_order, ["second.yaml", "first.yaml"])
                self.assertEqual(list(owner.output.iterdir()), [])
                self.assertEqual(owner.capture(owner.output), owner.owned[owner.output])
                self.assert_notes(origin, "injected empty-output rmdir refusal", "retained output directory")
                owner.assert_closed()
                self.repeat_refusal(owner)
            self.complete("A08", owner)

    def test_validator_preserves_frozen_complete_manifests(self):
        for control, fail in (("A09", True), ("A10", False)):
            with self.subTest(control=control):
                with CaseOwner(self) as owner:
                    inputs, expected = release_inputs(owner, paired=True)
                    origin = subprocess.CalledProcessError(7, "injected structural validator")
                    original_args = origin.args
                    arguments = [winget.__file__, TAG, str(inputs), str(owner.output), "--validate"]
                    self.assertFalse(owner.present(owner.output))
                    # Actual generation/ZIP inspection; only the external validator is controlled.
                    with patch.object(sys, "argv", arguments), \
                            patch.object(winget.subprocess, "run", side_effect=origin if fail else None) as validator:
                        with redirect_stdout(io.StringIO()) as printed:
                            if fail:
                                with self.assertRaises(subprocess.CalledProcessError) as raised:
                                    winget.main()
                                self.assert_origin(raised.exception, origin, original_args)
                            else:
                                winget.main()
                    owner.adopt_generation(expected)
                    self.assertEqual({path.name: path.read_bytes() for path in owner.output.iterdir()}, expected)
                    validator.assert_called_once_with(
                        ["winget", "validate", "--manifest", str(owner.output.resolve()), "--disable-interactivity"],
                        check=True, timeout=120)
                    self.assertEqual(printed.getvalue(), "" if fail else str(owner.output) + "\n")
                    owner.assert_parent()
                self.complete(control, owner)


class NativeWindowsRecovery(RecoveryAssertions, unittest.TestCase):
    def test_windows_actual_exclusive_collision_preserves_the_interposed_leaf(self):
        for position in (2, 3):
            with self.subTest(control=f"collision-{position}"):
                with CaseOwner(self) as owner:
                    saved = {}

                    def interpose(path, index):
                        if index == position:
                            owner.assert_closed()
                            saved["path"] = path
                            saved["identity"] = owner.seed(path, FOREIGN)

                    with owner.intercept(before_open=interpose):
                        with self.assertRaises(FileExistsError) as raised:
                            winget._write_documents(owner.output, dict(DOCUMENTS))
                    owner.errors.append(error_facts("actual-exclusive-open", raised.exception))
                    self.assertEqual(raised.exception.errno, errno.EEXIST)
                    self.assertEqual(owner.capture(saved["path"]), saved["identity"])
                    self.assertEqual(saved["path"].read_bytes(), FOREIGN)
                    self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {saved["path"].name})
                    self.assertEqual(len(owner.streams), position - 1)
                    self.assertEqual(owner.operations["unlink"], position - 1)
                    owner.assert_closed()
                    self.repeat_refusal(owner)
                self.complete(f"collision-{position}", owner)

    def test_windows_actual_replacements_are_detected_by_real_file_identities(self):
        for kind in ("file", "directory"):
            with self.subTest(control=kind):
                with CaseOwner(self) as owner:
                    self.replacement_control(owner, directory=kind == "directory")
                self.complete(kind, owner)

    def test_windows_readonly_cleanup_error_preserves_origin_and_partial_output(self):
        with self.subTest(control="readonly-cleanup"):
            with CaseOwner(self) as owner:
                origin = OSError(errno.EIO, "injected third-open origin")
                original_args = origin.args
                recorded = {}

                def readonly(path, index):
                    if index == 3:
                        owner.assert_closed()
                        first = owner.output / "first.yaml"
                        recorded["identity"] = owner.readonly_file(first)
                        self.assertFalse(REAL_LSTAT(first).st_mode & stat.S_IWRITE)
                        raise origin

                with owner.intercept(before_open=readonly):
                    with self.assertRaises(OSError) as raised:
                        winget._write_documents(owner.output, dict(DOCUMENTS))
                self.assert_origin(raised.exception, origin, original_args)
                self.assertEqual(owner.unlink_order, ["second.yaml", "first.yaml"])
                self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {"first.yaml"})
                first = owner.output / "first.yaml"
                self.assertEqual(owner.capture(first), recorded["identity"])
                self.assertEqual(first.read_bytes(), EXPECTED["first.yaml"])
                denied = [item for item in owner.errors if item["operation"] == "unlink"]
                self.assertEqual(len(denied), 1)
                self.assertEqual(denied[0]["type"], "PermissionError")
                self.assertEqual(denied[0]["winerror"], 5)
                self.assertEqual(len(owner.rmdir_errors), 1)
                self.assert_notes(origin, "could not remove", "retained output directory")
                owner.assert_closed()
                self.repeat_refusal(owner)
                self.assertFalse(REAL_LSTAT(first).st_mode & stat.S_IWRITE)
            self.complete("readonly-cleanup", owner)

    def test_windows_real_closed_streams_allow_namespace_reuse_and_a_second_run(self):
        with self.subTest(control="closed-reuse"):
            with CaseOwner(self) as owner:
                with owner.intercept():
                    self.assertIs(winget._write_documents(owner.output, dict(DOCUMENTS)), owner.output)
                self.assert_documents(owner)
                self.assertEqual(len(owner.streams), 3)
                for stream in owner.streams:
                    self.assertIsNotNone(stream.opened_identity)
                    self.assertEqual(owner.capture(owner.output / stream.name), stream.opened_identity)
                first = owner.output / "first.yaml"
                original = owner.capture(first)
                parked = owner.generated_parent / "reuse first.yaml"
                owner.move(first, parked)
                self.assertEqual(owner.capture(parked), original)
                owner.move(parked, first)
                self.assertEqual(owner.capture(first), original)
                for name in EXPECTED:
                    owner.delete(owner.output / name)
                owner.delete(owner.output)
                self.assertFalse(owner.output.exists())
                self.assertTrue(owner.generated_parent.is_dir())
                owner.assert_parent()
                self.repeat_success(owner)
                for name in EXPECTED:
                    owner.capture(owner.output / name)
            self.complete("closed-reuse", owner)

    def test_windows_late_close_error_allows_real_cleanup_and_namespace_reuse(self):
        with self.subTest(control="N07"):
            with CaseOwner(self) as owner:
                origin = OSError(errno.EIO, "injected second-file close after real close")
                original_args = origin.args
                with owner.intercept("close", 2, origin):
                    with self.assertRaises(OSError) as raised:
                        winget._write_documents(owner.output, dict(DOCUMENTS))
                self.assert_origin(raised.exception, origin, original_args)
                self.assertEqual(len(owner.streams), 2)
                self.assertEqual(owner.operations["close"], 2)
                self.assertEqual(owner.unlink_order, ["second.yaml", "first.yaml"])
                self.assertFalse(owner.present(owner.output))
                self.assertFalse(getattr(origin, "__notes__", ()))
                owner.assert_closed()
                self.repeat_success(owner)  # One explicit unpatched same-path call.
                for name in EXPECTED:
                    path = owner.output / name
                    original = owner.capture(path)
                    parked = owner.generated_parent / ("late-close-reuse " + name)
                    owner.move(path, parked)
                    self.assertEqual(owner.capture(parked), original)
                    owner.delete(parked)
                owner.delete(owner.output)
                self.assertFalse(owner.present(owner.output))
                owner.assert_parent()
            self.complete("N07", owner)


# Exact old method:label identities plus the eleven selected additions; counts alone are insufficient.
CONTROL_KEYS = {
    "PortableRecovery": {
        "test_success_writes_exact_utf8_lf_and_returns_output": ("success",),
        "test_preflight_refusals_have_no_output_or_parent_effect": (
            "name-0", "name-1", "name-2", "name-3", "name-4", "name-5", "name-6", "name-7", "name-8",
            "empty", "second-encoding"),
        "test_existing_output_and_ancestors_are_preserved": ("directory", "file"),
        "test_second_and_third_stage_failures_cleanup_and_allow_one_new_attempt": (
            "2-open", "2-write", "2-short-write", "2-flush", "2-close",
            "3-open", "3-write", "3-short-write", "3-flush", "3-close"),
        "test_cleanup_continues_and_preserves_origin_notes": ("OSError", "NoNotesError"),
        "test_unavailable_created_identities_leave_output_for_inspection": ("directory-lstat", "first-fstat", "A07"),
        "test_disappeared_created_file_is_not_a_cleanup_error": ("disappeared",),
        "test_unrelated_entries_survive_nonrecursive_cleanup": ("unrelated",),
        "test_replaced_created_file_is_retained_while_other_owned_leaf_is_removed": ("file-replacement",),
        "test_replaced_output_directory_retains_new_tree_and_parked_original": ("directory-replacement",),
        "test_rendered_manifests_match_frozen_full_bytes": ("A01", "A02"),
        "test_existing_directory_symlink_preserves_owned_target": ("A03",),
        "test_portable_exclusive_collisions_preserve_interposed_leaf": ("A04", "A05"),
        "test_renamed_away_directory_has_cleanup_not_confirmed_notes": ("A06",),
        "test_empty_output_rmdir_failure_preserves_primary": ("A08",),
        "test_validator_preserves_frozen_complete_manifests": ("A09", "A10"),
    },
    "NativeWindowsRecovery": {
        "test_windows_actual_exclusive_collision_preserves_the_interposed_leaf": ("collision-2", "collision-3"),
        "test_windows_actual_replacements_are_detected_by_real_file_identities": ("file", "directory"),
        "test_windows_readonly_cleanup_error_preserves_origin_and_partial_output": ("readonly-cleanup",),
        "test_windows_real_closed_streams_allow_namespace_reuse_and_a_second_run": ("closed-reuse",),
        "test_windows_late_close_error_allows_real_cleanup_and_namespace_reuse": ("N07",),
    },
}


def native_metadata(facts, expected):
    version = sys.getwindowsversion()
    facts.update({"os_version": list(version), "os_build": version.build,
                  "python_executable": sys.executable, "python_abi": sys.implementation.cache_tag,
                  "interpreter_platform": sysconfig.get_platform(),
                  "gil_disabled_build": sysconfig.get_config_var("Py_GIL_DISABLED"),
                  "processor_architecture": os.environ.get("PROCESSOR_ARCHITECTURE"),
                  "processor_architew6432": os.environ.get("PROCESSOR_ARCHITEW6432")})
    required_platform = {"X64": "win-amd64", "ARM64": "win-arm64"}[expected]
    machines = {"X64": {"amd64", "x86_64"}, "ARM64": {"arm64", "aarch64"}}[expected]
    if (facts["python_implementation"] != "CPython" or sys.version_info[:2] != (3, 14)
            or facts["gil_disabled_build"] or facts["pointer_bits"] != 64
            or facts["interpreter_platform"] != required_platform
            or facts["process_architecture"].lower() not in machines
            or (facts["processor_architecture"] or "").lower() not in machines
            or facts["processor_architew6432"]):
        print(json.dumps(facts, sort_keys=True), flush=True)
        raise AssertionError("actual process/interpreter facts do not establish the selected native CPython ABI")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-windows", action="store_true")
    arguments = parser.parse_args()
    facts = {"schema": "locron.winget-output-controls/v1", "os_name": os.name,
             "python_implementation": platform.python_implementation(),
             "python_version": platform.python_version(), "pointer_bits": struct.calcsize("P") * 8,
             "process_architecture": platform.machine(), "runner_architecture": os.environ.get("RUNNER_ARCH"),
             "native_windows": arguments.native_windows}
    if arguments.native_windows:
        expected = os.environ.get("EXPECTED_RUNNER_ARCH")
        if os.name != "nt" or sys.version_info < (3, 11):
            parser.error("native controls require actual Windows and Python 3.11 or later")
        if expected not in ("X64", "ARM64") or os.environ.get("RUNNER_ARCH") != expected:
            parser.error("native controls require the exact selected runner architecture")
        native_metadata(facts, expected)
    suite = unittest.TestSuite()
    classes = [PortableRecovery]
    if arguments.native_windows:
        classes.append(NativeWindowsRecovery)
    cases, expected_keys = [], []
    for selected in classes:
        selectors = set()
        for case in unittest.defaultTestLoader.loadTestsFromTestCase(selected):
            case.native = arguments.native_windows
            selector = case.id().rsplit(".", 1)[1]
            selectors.add(selector)
            expected_keys.extend(f"{case.id()}:{label}" for label in CONTROL_KEYS[selected.__name__][selector])
            cases.append(case)
        if selectors != set(CONTROL_KEYS[selected.__name__]):
            raise AssertionError("missing or unknown selected test method")
    receipt = Receipt(expected_keys)
    for case in cases:
        case.receipt = receipt
        suite.addTest(case)
    print(json.dumps(facts, sort_keys=True), flush=True)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    expected_methods, expected_controls = (21, 49) if arguments.native_windows else (16, 42)
    counts_match = result.testsRun == expected_methods and len(receipt.completed) == expected_controls
    keys_match = set(receipt.completed) == receipt.expected
    evidence = {"schema": facts["schema"], "methods_run": result.testsRun,
                "completed_controls": len(receipt.completed), "expected_methods": expected_methods,
                "expected_controls": expected_controls, "skipped": len(result.skipped),
                "completed": receipt.completed, "expected_keys": sorted(receipt.expected),
                "observations": receipt.observations}
    print(json.dumps(evidence, sort_keys=True), flush=True)
    return 0 if (result.wasSuccessful() and counts_match and keys_match and not result.skipped
                 and not result.unexpectedSuccesses) else 1


if __name__ == "__main__":
    sys.exit(main())
