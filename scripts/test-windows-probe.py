#!/usr/bin/env python3
"""Real child-process tests for bounded package probe output and lifecycle."""
import json
import math
import os
import struct
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import windows_probe as probe


def command(source):
    return [sys.executable, "-c", source]


class ProbeTests(unittest.TestCase):
    def run_child(self, source, **options):
        return probe.run_probe(command(source), timeout=options.pop("timeout", 5), **options)

    def assert_admission_free(self):
        self.assertTrue(probe._ADMISSION.acquire(blocking=False))
        probe._ADMISSION.release()

    def run_isolated(self, source, timeout=15):
        result = subprocess.run([sys.executable, "-B", "-c", source],
                                cwd=Path(__file__).parent, capture_output=True,
                                text=True, timeout=timeout)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_admission_free()
        return result.stdout.strip()

    def test_normal_binary_and_text_outputs_preserve_the_expected_result(self):
        result = self.run_child("import os; os.write(1,b'locron 0.10.0\\n'); os.write(2,b'note')")
        self.assertEqual(result.stdout, b"locron 0.10.0\n")
        self.assertEqual(result.stderr, b"note")
        self.assertEqual(result.returncode, 0)
        result = self.run_child("import os; os.write(1,b'a\\r\\nb\\rc')", text=True)
        self.assertEqual(result.stdout, "a\nb\nc")
        self.assert_admission_free()

    def test_exact_limit_is_accepted_on_both_streams(self):
        result = self.run_child("import os; os.write(1,b'x'*4096); os.write(2,b'y'*4096)")
        self.assertEqual(len(result.stdout), 4096)
        self.assertEqual(len(result.stderr), 4096)

    def assert_capped(self, descriptor, stream):
        children = []
        real_spawn = subprocess.Popen
        def spawn(*args, **kwargs):
            child = real_spawn(*args, **kwargs)
            children.append(child)
            return child
        started = time.monotonic()
        with patch.object(probe.subprocess, "Popen", side_effect=spawn):
            with self.assertRaises(probe.ProbeOutputLimit) as raised:
                self.run_child(f"import os,time; os.write({descriptor},b'x'*1000000); time.sleep(20)", timeout=10)
        self.assertEqual(raised.exception.stream, stream)
        self.assertEqual(len(raised.exception.captured), 4097)
        self.assertLess(time.monotonic() - started, 8)
        self.assertIsNotNone(children[0].poll())
        self.assert_admission_free()

    def test_stdout_limit_refuses_and_reaps_before_normal_exit(self):
        self.assert_capped(1, "stdout")

    def test_stderr_limit_refuses_and_reaps_before_normal_exit(self):
        self.assert_capped(2, "stderr")

    def test_concurrent_streams_do_not_deadlock(self):
        result = self.run_child("import os,threading; t=threading.Thread(target=lambda:os.write(2,b'e'*4000)); t.start(); os.write(1,b'o'*4000); t.join()")
        self.assertEqual(result.stdout, b"o" * 4000)
        self.assertEqual(result.stderr, b"e" * 4000)

    def test_terminal_output_cap_is_rechecked_after_reader_join(self):
        finished = False
        real_check = probe._Capture.check
        real_finish = probe._Owner.finish
        def check_after_finish(capture, name):
            if finished:
                real_check(capture, name)
        def finish(owner, deadline, terminate=False):
            nonlocal finished
            result = real_finish(owner, deadline, terminate)
            finished = True
            return result
        with patch.object(probe._Capture, "check", new=check_after_finish), \
                patch.object(probe._Owner, "finish", new=finish):
            with self.assertRaises(probe.ProbeOutputLimit):
                self.run_child("import os; os.write(1,b'x'*4097)")
        self.assert_admission_free()

    def test_spawn_time_consumes_the_original_operation_budget(self):
        real_spawn = subprocess.Popen
        def delayed_spawn(*args, **kwargs):
            time.sleep(0.2)
            return real_spawn(*args, **kwargs)
        with patch.object(probe.subprocess, "Popen", side_effect=delayed_spawn):
            with self.assertRaises(subprocess.TimeoutExpired):
                self.run_child("pass", timeout=0.1)
        self.assert_admission_free()

    def test_nonzero_exit_retains_bounded_captured_output(self):
        source = "import os,sys; os.write(1,b'out'); os.write(2,b'err'); sys.exit(7)"
        self.assertEqual(self.run_child(source).returncode, 7)
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            self.run_child(source, check=True)
        self.assertEqual((raised.exception.returncode, raised.exception.output, raised.exception.stderr),
                         (7, b"out", b"err"))
        self.assert_admission_free()

    def test_timeout_reaps_the_actual_owned_child(self):
        children = []
        real_spawn = subprocess.Popen
        def spawn(*args, **kwargs):
            child = real_spawn(*args, **kwargs)
            children.append(child)
            return child
        with patch.object(probe.subprocess, "Popen", side_effect=spawn):
            with self.assertRaises(subprocess.TimeoutExpired):
                self.run_child("import time; time.sleep(20)", timeout=0.3)
        self.assertIsNotNone(children[0].poll())
        self.assert_admission_free()

    def test_eof_without_child_exit_is_not_success(self):
        with self.assertRaises(subprocess.TimeoutExpired):
            self.run_child("import os,time; os.close(1); os.close(2); time.sleep(20)", timeout=0.3)
        self.assert_admission_free()

    def test_child_exit_without_pipe_eof_is_not_success(self):
        # The short-lived descendant deliberately inherits both output pipes.
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "root-exited"
            source = ("import subprocess,sys; from pathlib import Path; "
                      "subprocess.Popen([sys.executable,'-c','import time; time.sleep(2)'], stdout=sys.stdout, stderr=sys.stderr); "
                      f"Path({str(marker)!r}).write_text('started'); sys.exit(0)")
            with self.assertRaises(subprocess.TimeoutExpired):
                self.run_child(source, timeout=1)
            self.assertEqual(marker.read_text(), "started")
        self.assert_admission_free()

    def test_invalid_utf8_text_fails_after_confirmed_cleanup(self):
        with self.assertRaises(UnicodeDecodeError):
            self.run_child("import os; os.write(1,b'\\xff')", text=True)
        self.assert_admission_free()

    def test_bad_admission_inputs_never_spawn(self):
        with patch.object(probe.subprocess, "Popen", side_effect=AssertionError("unexpected spawn")):
            for timeout in (0, -1):
                with self.assertRaises(subprocess.TimeoutExpired):
                    self.run_child("pass", timeout=timeout)
            for timeout in (math.inf, math.nan, True, 31, "30"):
                with self.assertRaises(ValueError):
                    self.run_child("pass", timeout=timeout)
            for options in ({"stdin": None}, {"capture_output": False}):
                with self.assertRaises(ValueError):
                    self.run_child("pass", **options)
            with self.assertRaises(ValueError):
                probe.run_probe("not a shell command")
        self.assert_admission_free()

    def test_spawn_failure_and_reader_creation_failure_release_admission(self):
        with patch.object(probe.subprocess, "Popen", side_effect=OSError("fixture spawn failure")):
            with self.assertRaisesRegex(OSError, "fixture spawn failure"):
                self.run_child("pass")
        self.assert_admission_free()
        children = []
        real_spawn = subprocess.Popen
        real_capture = probe._Capture
        calls = 0
        def spawn(*args, **kwargs):
            child = real_spawn(*args, **kwargs)
            children.append(child)
            return child
        def second_capture_fails(stream):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise RuntimeError("fixture reader creation failure")
            return real_capture(stream)
        with patch.object(probe.subprocess, "Popen", side_effect=spawn), \
                patch.object(probe, "_Capture", side_effect=second_capture_fails), \
                patch.object(threading.Thread, "start", side_effect=AssertionError("unexpected reader start")) as start:
            with self.assertRaisesRegex(RuntimeError, "fixture reader creation failure"):
                self.run_child("import time; time.sleep(20)")
        start.assert_not_called()
        self.assertIsNotNone(children[0].poll())
        self.assert_admission_free()

    def test_uncertain_reader_start_failure_retains_owner_and_refuses_next_spawn(self):
        source = """
import subprocess
import sys
import threading
from unittest.mock import patch
import windows_probe as p
real_start = threading.Thread.start
calls = 0
def second_start_fails(thread):
    global calls
    calls += 1
    if calls == 2:
        raise RuntimeError('fixture reader start failure')
    return real_start(thread)
with patch.object(threading.Thread, 'start', new=second_start_fails):
    try:
        p.run_probe([sys.executable, '-c', 'import time; time.sleep(20)'])
    except p.ProbeCleanupPending as error:
        assert isinstance(error.__cause__, RuntimeError)
    else:
        raise AssertionError('uncertain reader start accepted')
owner = p._QUARANTINED
assert owner is not None and owner.process.poll() is not None
assert not owner.process.stdout.closed and not owner.process.stderr.closed
with patch.object(p.subprocess, 'Popen', side_effect=AssertionError('second spawn')):
    try:
        p.run_probe(['missing'])
    except p.ProbeCleanupPending:
        pass
    else:
        raise AssertionError('uncertain reader start reopened admission')
print('uncertain-start-confirmed')
"""
        self.assertEqual(self.run_isolated(source), "uncertain-start-confirmed")

    def test_interrupted_native_reader_start_retains_owner_and_admission(self):
        # Hold an actual native thread before bootstrap publishes ident/_started.
        # Its intentional quarantine must remain in this separate Python process.
        source = """
import json
import sys
import threading
from unittest.mock import patch
import windows_probe as p
entered = threading.Event()
release = threading.Event()
captures = []
owners = []
saved_wait = None
real_capture_init = p._Capture.__init__
real_bootstrap = threading.Thread._bootstrap_inner
real_finish = p._Owner.finish
def capture_init(capture, stream):
    global saved_wait
    real_capture_init(capture, stream)
    captures.append(capture)
    if len(captures) == 1:
        saved_wait = capture.thread._started.wait
        def interrupted_wait(timeout=None):
            assert entered.wait(5), 'native reader did not enter bootstrap'
            raise KeyboardInterrupt('controlled interruption in Thread.start')
        capture.thread._started.wait = interrupted_wait
def delayed_bootstrap(thread):
    if captures and thread is captures[0].thread:
        entered.set()
        assert release.wait(15), 'native reader release was not signalled'
    real_bootstrap(thread)
def finish(owner, deadline, terminate=False):
    result = real_finish(owner, deadline, terminate)
    owners.append((owner, result))
    return result
try:
    with patch.object(p._Capture, '__init__', new=capture_init), \\
            patch.object(threading.Thread, '_bootstrap_inner', new=delayed_bootstrap), \\
            patch.object(p._Owner, 'finish', new=finish):
        try:
            p.run_probe([sys.executable, '-c', 'import time; time.sleep(20)'], timeout=3)
        except (KeyboardInterrupt, p.ProbeCleanupPending) as error:
            error_type = type(error).__name__
        else:
            raise AssertionError('interrupted start returned success')
    assert entered.is_set() and captures[0].thread.ident is None
    owner, completed = owners[-1]
    acquired = p._ADMISSION.acquire(blocking=False)
    if acquired:
        p._ADMISSION.release()
    facts = {
        'exception': error_type,
        'native_reader_entered_without_ident': True,
        'cleanup_claimed_complete': completed,
        'reader_stream_closed_before_bootstrap_completed': captures[0].stream.closed,
        'admission_reopened_before_reader_completion': acquired,
        'owned_root_reaped': owner.process.poll() is not None,
        'owner_retained': p._QUARANTINED is owner,
    }
    print(json.dumps(facts))
    assert error_type == 'ProbeCleanupPending'
    assert not completed and p._QUARANTINED is owner
    assert not captures[0].stream.closed and not owner.process.stderr.closed
    assert not acquired and owner.process.poll() is not None
    with patch.object(p.subprocess, 'Popen', side_effect=AssertionError('second spawn')):
        try:
            p.run_probe(['missing'])
        except p.ProbeCleanupPending:
            pass
        else:
            raise AssertionError('live unpublished reader reopened admission')
finally:
    release.set()
    if saved_wait is not None:
        assert saved_wait(5), 'native reader did not publish startup after release'
        captures[0].thread.join(5)
        assert not captures[0].thread.is_alive(), 'fixture reader did not exit'
assert captures[0].done.is_set() and not captures[0].stream.closed
assert p._QUARANTINED is owner
assert not p._ADMISSION.acquire(blocking=False)
"""
        facts = json.loads(self.run_isolated(source))
        self.assertEqual(facts, {
            "exception": "ProbeCleanupPending",
            "native_reader_entered_without_ident": True,
            "cleanup_claimed_complete": False,
            "reader_stream_closed_before_bootstrap_completed": False,
            "admission_reopened_before_reader_completion": False,
            "owned_root_reaped": True,
            "owner_retained": True,
        })

    def test_uncertain_cleanup_retains_owner_and_refuses_the_next_spawn(self):
        # Controlled cleanup uncertainty lives in a separate Python process so
        # its intentionally quarantined admission cannot contaminate other tests.
        source = """
from unittest.mock import patch
import subprocess
import windows_probe as p
with patch.object(p.subprocess, 'Popen', side_effect=OSError('fixture failure')), patch.object(p._Owner, 'finish', return_value=False):
    try:
        p.run_probe(['missing'])
    except p.ProbeCleanupPending:
        pass
    else:
        raise AssertionError('unconfirmed cleanup accepted')
assert p._QUARANTINED is not None
with patch.object(p.subprocess, 'Popen', side_effect=AssertionError('second spawn')):
    try:
        p.run_probe(['missing'])
    except p.ProbeCleanupPending:
        pass
    else:
        raise AssertionError('quarantine reopened')
print('quarantine-confirmed')
"""
        self.assertEqual(self.run_isolated(source, timeout=5), "quarantine-confirmed")


class PackageIntegration(unittest.TestCase):
    def test_default_pair_validation_uses_bounded_capture_for_all_three_probes(self):
        import windows_release as release
        target = "x86_64-pc-windows-msvc"
        calls = []
        def execute(argv, **kwargs):
            name, argument = Path(argv[0]).name, argv[1]
            calls.append((name, argument))
            self.assertEqual(kwargs["timeout"], 30)
            self.assertEqual(kwargs["stdin"], subprocess.DEVNULL)
            if argument == "--version":
                output = f"{name.removesuffix('.exe')} 0.10.0\n".encode()
            else:
                output = json.dumps({"schema": "locron.windows-launcher-probe/v1", "version": "0.10.0",
                                     "target": target, "launcher_abi": "native-gui-v1",
                                     "initial_conout_opened": False, "initial_conout_error": 2}).encode()
            return subprocess.CompletedProcess(argv, 0, output, b"")
        contents = {name: b"fixture" for name in release.PAIRED_FILES}
        with patch.object(release, "run_probe", side_effect=execute):
            versions, identity = release.probe_pair(contents, "0.10.0", target, None)
        self.assertEqual(len(calls), 3)
        self.assertEqual(versions["locron.exe"], "locron 0.10.0\n")
        self.assertEqual(identity["launcher_abi"], "native-gui-v1")

    def test_default_legacy_package_uses_bounded_capture_and_keeps_zip_shape(self):
        import windows_release as release
        binary = bytearray(512)
        binary[:2] = b"MZ"
        struct.pack_into("<I", binary, 60, 128)
        binary[128:132] = b"PE\0\0"
        for offset, value in ((132, 0x8664), (134, 1), (148, 240), (150, 0x22), (152, 0x20B)):
            struct.pack_into("<H", binary, offset, value)
        struct.pack_into("<I", binary, 260, 16)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / "locron.exe"
            executable.write_bytes(binary)
            for name in release.FILES[1:]:
                (root / name).write_bytes(b"fixture")
            response = subprocess.CompletedProcess([], 0, "locron 0.10.0\n", "")
            with patch.object(release, "run_probe", return_value=response) as execute:
                archive = release.package("v0.10.0", "x86_64-pc-windows-msvc", executable,
                                          root / "output", source=root)
            execute.assert_called_once()
            self.assertTrue(execute.call_args.kwargs["text"])
            with release.zipfile.ZipFile(archive) as stored:
                self.assertEqual(len(stored.infolist()), 4)

    def test_custom_executors_still_work_and_static_inspection_does_not_execute(self):
        import windows_release as release
        with patch.object(release, "run_probe", side_effect=AssertionError("default probe executed")):
            with patch.object(release, "_inspect_archive", return_value=({"version": "0.10.0"}, {}, "ab" * 32)):
                result = release.inspect_archive(Path("not opened"), "v0.10.0", "x86_64-pc-windows-msvc")
            self.assertEqual(result["archive_sha256"], "ab" * 32)
            def controlled(*args, **kwargs):
                raise ValueError("controlled executor invoked")
            with self.assertRaisesRegex(ValueError, "controlled executor invoked"):
                release.probe_pair({"locron.exe": b"fixture"}, "0.10.0", "x86_64-pc-windows-msvc", controlled)


if __name__ == "__main__":
    unittest.main()
