"""Bounded capture for CI-built Windows image probes, not a process-tree sandbox.

No communicate() buffer, unbounded wait or closing a pipe underneath its reader.
An uncertain cleanup retains its owner and prevents another probe in this process.
Synchronous platform spawn/kill calls themselves are not interruptible deadlines.
"""
import math
import os
import subprocess
import threading
import time

MAX_PROBE_BYTES = 4 * 1024
_CLEANUP_SECONDS = 3.0
_ADMISSION = threading.Lock()
_QUARANTINED = None


class ProbeOutputLimit(ValueError):
    def __init__(self, stream, captured):
        super().__init__(f"package probe {stream} exceeds {MAX_PROBE_BYTES} bytes")
        self.stream = stream
        self.captured = captured


class ProbeCleanupPending(RuntimeError):
    pass


class _Capture:
    def __init__(self, stream):
        self.stream = stream
        self.data = bytearray()
        self.error = None
        self.done = threading.Event()
        self.start_attempted = False
        self.thread = threading.Thread(target=self.read, name="locron-probe-reader", daemon=True)

    def start(self):
        # Thread.start can be interrupted after native creation but before it
        # publishes ident. Never infer that an attempted start created no reader.
        self.start_attempted = True
        self.thread.start()

    def read(self):
        try:
            while len(self.data) <= MAX_PROBE_BYTES:
                size = min(1024, MAX_PROBE_BYTES + 1 - len(self.data))
                chunk = self.stream.read(size)
                if not chunk:
                    return
                self.data.extend(chunk)
        except BaseException as error:
            self.error = error
        finally:
            self.done.set()

    def check(self, name):
        # The event publishes the complete terminal buffer/error. Do not inspect
        # the buffer concurrently with a running reader, including on timeout.
        if self.done.is_set():
            if self.error is not None:
                raise RuntimeError(f"package probe {name} read failed") from self.error
            if len(self.data) > MAX_PROBE_BYTES:
                raise ProbeOutputLimit(name, bytes(self.data))


class _Owner:
    def __init__(self):
        self.process = None
        self.captures = []

    def finish(self, deadline, terminate=False):
        """Return True only after the exact child and every attempted reader finish."""
        try:
            if self.process is not None:
                if terminate and self.process.poll() is None:
                    self.process.kill()
                while self.process.poll() is None:
                    left = deadline - time.monotonic()
                    if left <= 0:
                        return False
                    time.sleep(min(0.01, left))
            for capture in self.captures:
                if capture.start_attempted:
                    # An unpublished start cannot be joined yet. That exception
                    # means cleanup is uncertain, so the owner must be retained.
                    capture.thread.join(max(0, deadline - time.monotonic()))
                    if capture.thread.is_alive():
                        return False
            # These are unbuffered read ends, and no reader can still touch them.
            if self.process is not None:
                self.process.stdout.close()
                self.process.stderr.close()
            return True
        except BaseException:
            return False


def run_probe(command, *, cwd=None, stdin=subprocess.DEVNULL, capture_output=True,
              text=False, check=False, timeout=30, env=None):
    """The small subprocess.run-compatible subset used by package version probes.

    Retain at most limit+1 bytes per stream; normal return requires root exit and
    both EOFs. Only the owned root is terminated, never arbitrary processes by PID.
    Descendants retaining a pipe cause refusal/quarantine, not a tree-exit claim.
    """
    global _QUARANTINED
    if not isinstance(command, (list, tuple)) or not command:
        raise ValueError("package probe requires an explicit executable/argument sequence")
    if not capture_output or stdin != subprocess.DEVNULL:
        raise ValueError("package probes require captured streams and closed stdin")
    if isinstance(timeout, bool) or not isinstance(timeout, (int, float)) or not math.isfinite(timeout):
        raise ValueError("package probe timeout must be finite")
    if timeout <= 0:
        raise subprocess.TimeoutExpired(command, timeout)
    if timeout > 30:
        raise ValueError("package probe cannot extend the thirty-second operation limit")
    deadline = time.monotonic() + timeout
    if not _ADMISSION.acquire(blocking=False):
        raise ProbeCleanupPending("a package probe is active or its cleanup remains unconfirmed")
    owner = _Owner()
    retained = False
    try:
        if time.monotonic() >= deadline:
            raise subprocess.TimeoutExpired(command, timeout)
        owner.process = subprocess.Popen(
            command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, shell=False,
            bufsize=0, close_fds=True,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
        owner.captures = [_Capture(owner.process.stdout), _Capture(owner.process.stderr)]
        for capture in owner.captures:
            capture.start()
        while True:
            for name, capture in zip(("stdout", "stderr"), owner.captures):
                capture.check(name)
            left = deadline - time.monotonic()
            if left <= 0:
                raise subprocess.TimeoutExpired(command, timeout)
            if owner.process.poll() is not None and all(c.done.is_set() for c in owner.captures):
                break
            time.sleep(min(0.01, left))
        if not owner.finish(deadline) or time.monotonic() >= deadline:
            raise subprocess.TimeoutExpired(command, timeout)
        for name, capture in zip(("stdout", "stderr"), owner.captures):
            capture.check(name)
        stdout, stderr = (bytes(c.data) for c in owner.captures)
        if text:
            stdout = stdout.decode("utf-8").replace("\r\n", "\n").replace("\r", "\n")
            stderr = stderr.decode("utf-8").replace("\r\n", "\n").replace("\r", "\n")
        result = subprocess.CompletedProcess(command, owner.process.returncode, stdout, stderr)
        if check:
            result.check_returncode()
        return result
    except BaseException as error:
        if not owner.finish(time.monotonic() + _CLEANUP_SECONDS, terminate=True):
            # Exactly one quarantine is possible because its admission stays held.
            _QUARANTINED = owner
            retained = True
            raise ProbeCleanupPending("package probe cleanup is unconfirmed; no later probe is admitted") from error
        raise
    finally:
        if not retained:
            _ADMISSION.release()
