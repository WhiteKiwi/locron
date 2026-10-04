"""Task-owned publication copies; not a sandbox against malicious same-account code."""
from contextlib import contextmanager
import os
from pathlib import Path
import stat
import tempfile

CHUNK_BYTES = 1024 * 1024


def _signature(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
            info.st_mtime_ns, info.st_ctime_ns)


def copy_regular(source, destination, limit=None):
    """Copy one observed regular file, without following a substituted final link.

    The initial length bounds I/O; growth, truncation and changed metadata refuse.
    Callers validate the resulting bytes and keep the owned destination private.
    """
    before = source.lstat()
    if not stat.S_ISREG(before.st_mode) or (limit is not None and before.st_size > limit):
        raise ValueError(f"unsafe or oversized publication input: {source.name}")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
    flags |= getattr(os, "O_BINARY", 0)
    descriptor = os.open(source, flags)
    try:
        opened = os.fstat(descriptor)
        if _signature(opened) != _signature(before):
            raise ValueError(f"publication input changed before copy: {source.name}")
        with os.fdopen(descriptor, "rb", closefd=False) as incoming:
            with destination.open("xb") as outgoing:
                remaining = opened.st_size
                while remaining:
                    chunk = incoming.read(min(CHUNK_BYTES, remaining))
                    if not chunk:
                        raise ValueError(f"publication input shrank during copy: {source.name}")
                    if outgoing.write(chunk) != len(chunk):
                        raise OSError("short publication snapshot write")
                    remaining -= len(chunk)
                if incoming.read(1):
                    raise ValueError(f"publication input grew during copy: {source.name}")
                if (_signature(os.fstat(descriptor)) != _signature(opened)
                        or _signature(source.lstat()) != _signature(opened)):
                    raise ValueError(f"publication input changed during copy: {source.name}")
                outgoing.flush()
                os.fsync(outgoing.fileno())
    finally:
        os.close(descriptor)


@contextmanager
def snapshot(sources, limits=None):
    """Retain a private temporary tree until the caller's synchronous upload ends."""
    limits = limits or {}
    seen = set()
    for name in sources:
        if (not isinstance(name, str) or any(part in ("", ".", "..") for part in name.split("/"))
                or any(character in name for character in "\\:\0") or name.casefold() in seen):
            raise ValueError("unsafe or duplicate publication snapshot name")
        seen.add(name.casefold())
    with tempfile.TemporaryDirectory(prefix="locron-publication-") as temporary:
        root = Path(temporary)
        for name, source in sources.items():
            destination = root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            copy_regular(Path(source), destination, limits.get(name))
        yield root
