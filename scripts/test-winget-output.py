#!/usr/bin/env python3
"""Direct WinGet writer recovery controls; native filesystem proof is opt-in."""
import argparse
from contextlib import ExitStack, contextmanager
import errno
import importlib.util
import json
import os
from pathlib import Path
import platform
import stat
import struct
import sys
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


class Receipt:
    def __init__(self):
        self.completed = []
        self.observations = []

    def complete(self, case, control, owner):
        key = f"{case.id()}:{control}"
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
        self.temporary = tempfile.TemporaryDirectory(prefix="locron winget 한글 ")
        self.root = Path(self.temporary.name)
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
        # Register the owner before any fallible seed I/O. Later attribute
        # cleanups are registered after this and therefore run first in unittest.
        case.addCleanup(self.cleanup)

    def __enter__(self):
        self.root_identity = self.capture(self.root)
        self.parent.mkdir()
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

    def seed(self, path, data):
        self.case.assertLessEqual(len(data), 4096)
        raw = REAL_OPEN(path, "xb")
        self.seed_streams.append(raw)
        with raw:
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
        self.readonly.append((path, original))
        self.case.addCleanup(self.restore_readonly)
        # Both cleanup registrations exist before the actual attribute change.
        os.chmod(path, stat.S_IREAD)
        return original

    def restore_readonly(self):
        for path, original in list(self.readonly):
            self.case.assertEqual(self.capture(path), original,
                                  "never clear an attribute on a replacement")
            os.chmod(path, stat.S_IREAD | stat.S_IWRITE)
            self.readonly.remove((path, original))

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
            self.restore_readonly()
            self.case.assertEqual(self.capture(self.root), self.root_identity,
                                  "cleanup only the original disposable case root")
            self.temporary.cleanup()
        except BaseException as error:
            self.cleanup_error = error
            raise
        self.cleaned = True

    @contextmanager
    def intercept(self, stage=None, position=None, origin=None, before_open=None,
                  lstat_fault=None, fstat_fault=None, unlink_fault=None):
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
            # This is a real fixture observation; the writer still performs its
            # own unmodified fstat, which may be refused in the unknown-ID case.
            stream.opened_identity = self.capture(path, REAL_FSTAT(stream.fileno()))
            return stream

        def make_directory(path, *args, **kwargs):
            if path in (self.output, self.generated_parent):
                self.operations["mkdir"] += 1
            return REAL_MKDIR(path, *args, **kwargs)

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
                owner.output.rename(parked)
                owner.output.mkdir()
                owner.seed(owner.output / "foreign.yaml", FOREIGN)
                recorded["replacement"] = owner.capture(owner.output)
                recorded["parked"] = self.snapshot(owner, parked)
                recorded["foreign"] = self.snapshot(owner, owner.output)
                recorded["parked_path"] = parked
            else:
                first = owner.output / "first.yaml"
                parked = owner.generated_parent / "parked original file.yaml"
                recorded["original"] = owner.capture(first)
                first.rename(parked)  # Keep the old ID live; do not assume no ID reuse.
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
                    owner.generated_parent.mkdir()
                    if kind == "directory":
                        owner.output.mkdir()
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
        for boundary in ("directory-lstat", "first-fstat"):
            with self.subTest(control=boundary):
                with CaseOwner(self) as owner:
                    origin = OSError(errno.EIO, "injected unavailable identity")
                    original_args = origin.args

                    def refuse_directory(path):
                        if path == owner.output:
                            raise origin

                    def refuse_file(fd):
                        raise origin

                    options = {"lstat_fault": refuse_directory} if boundary == "directory-lstat" else {
                        "fstat_fault": refuse_file}
                    with owner.intercept(**options):
                        with self.assertRaises(OSError) as raised:
                            winget._write_documents(owner.output, dict(DOCUMENTS))
                    self.assert_origin(raised.exception, origin, original_args)
                    self.assertTrue(owner.output.is_dir())
                    self.assertEqual(owner.operations["unlink"], 0)
                    if boundary == "directory-lstat":
                        self.assertEqual(list(owner.output.iterdir()), [])
                        self.assertEqual(owner.operations["open"], 0)
                        self.assertEqual(owner.operations["rmdir"], 0)
                    else:
                        self.assertEqual({leaf.name for leaf in owner.output.iterdir()}, {"first.yaml"})
                        self.assertEqual((owner.output / "first.yaml").read_bytes(), b"")
                        self.assertEqual(owner.operations["rmdir"], 1)
                    owner.assert_closed()
                    self.repeat_refusal(owner)
                self.complete(boundary, owner)

    def test_disappeared_created_file_is_not_a_cleanup_error(self):
        with self.subTest(control="disappeared"):
            with CaseOwner(self) as owner:
                origin = OSError(errno.EIO, "injected third-open error")
                original_args = origin.args

                def disappear(path, index):
                    if index == 3:
                        owner.assert_closed()
                        REAL_UNLINK(owner.output / "first.yaml")
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
                        nested.mkdir()
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
                first.rename(parked)
                self.assertEqual(owner.capture(parked), original)
                parked.rename(first)
                self.assertEqual(owner.capture(first), original)
                for name in EXPECTED:
                    REAL_UNLINK(owner.output / name)
                REAL_RMDIR(owner.output)
                self.assertFalse(owner.output.exists())
                self.assertTrue(owner.generated_parent.is_dir())
                owner.assert_parent()
                self.repeat_success(owner)
                for name in EXPECTED:
                    owner.capture(owner.output / name)
            self.complete("closed-reuse", owner)


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
    receipt = Receipt()
    suite = unittest.TestSuite()
    classes = [PortableRecovery]
    if arguments.native_windows:
        classes.append(NativeWindowsRecovery)
    for selected in classes:
        for case in unittest.defaultTestLoader.loadTestsFromTestCase(selected):
            case.native = arguments.native_windows
            case.receipt = receipt
            suite.addTest(case)
    print(json.dumps(facts, sort_keys=True), flush=True)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    expected_methods, expected_controls = (14, 38) if arguments.native_windows else (10, 32)
    counts_match = result.testsRun == expected_methods and len(receipt.completed) == expected_controls
    evidence = {"schema": facts["schema"], "methods_run": result.testsRun,
                "completed_controls": len(receipt.completed), "expected_methods": expected_methods,
                "expected_controls": expected_controls, "skipped": len(result.skipped),
                "completed": receipt.completed, "observations": receipt.observations}
    print(json.dumps(evidence, sort_keys=True), flush=True)
    return 0 if (result.wasSuccessful() and counts_match and not result.skipped
                 and not result.unexpectedSuccesses) else 1


if __name__ == "__main__":
    sys.exit(main())
