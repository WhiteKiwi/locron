#!/usr/bin/env python3
"""Build and inspect unsigned Windows ZIPs without changing installed software."""
import argparse
import hashlib
from pathlib import Path
import re
import stat
import struct
import subprocess
import zipfile

TARGETS = {"x86_64-pc-windows-msvc": 0x8664, "aarch64-pc-windows-msvc": 0xAA64}
FILES = ("locron.exe", "README.md", "LICENSE-MIT", "LICENSE-APACHE")
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024


def version(tag):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
        raise ValueError("expected stable vMAJOR.MINOR.PATCH tag")
    return tag[1:]


def pe_machine(binary):
    """Require a PE32+ image for a supported architecture with no certificate table."""
    if len(binary) < 64 or binary[:2] != b"MZ":
        raise ValueError("Windows executable has no DOS/PE header")
    offset = struct.unpack_from("<I", binary, 60)[0]
    if offset + 24 + 152 > len(binary) or binary[offset:offset + 4] != b"PE\0\0":
        raise ValueError("Windows executable has a truncated or invalid PE header")
    machine = struct.unpack_from("<H", binary, offset + 4)[0]
    sections = struct.unpack_from("<H", binary, offset + 6)[0]
    characteristics = struct.unpack_from("<H", binary, offset + 22)[0]
    if not sections or not characteristics & 0x2 or characteristics & 0x2000:
        raise ValueError("Windows archive must contain an executable image, not a DLL")
    optional_size = struct.unpack_from("<H", binary, offset + 20)[0]
    optional = offset + 24
    if optional_size < 152 or optional + optional_size > len(binary):
        raise ValueError("Windows executable has a truncated optional header")
    if struct.unpack_from("<H", binary, optional)[0] != 0x20B or machine not in TARGETS.values():
        raise ValueError("Windows executable is not supported x64/ARM64 PE32+")
    if struct.unpack_from("<I", binary, optional + 108)[0] < 5:
        raise ValueError("Windows executable has no complete security directory")
    if struct.unpack_from("<II", binary, optional + 144) != (0, 0):
        raise ValueError("initial Windows release must be unsigned (certificate table is present)")
    return machine


def validate_archive(path, tag, target):
    version(tag)
    if target not in TARGETS:
        raise ValueError("unsupported Windows release target")
    if not path.is_file() or path.is_symlink() or path.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ValueError("unsafe or oversized Windows archive")
    root = f"locron-{tag}-{target}"
    allowed = {root + "/" + name for name in FILES}
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(name.casefold() for name in names)) or set(names) != allowed or len(names) != 4:
            raise ValueError("Windows ZIP differs from the exact four-file inventory")
        if sum(entry.file_size for entry in entries) > MAX_ARCHIVE_BYTES:
            raise ValueError("Windows ZIP expands beyond its size limit")
        for entry in entries:
            kind = stat.S_IFMT(entry.external_attr >> 16)
            if entry.flag_bits & 1 or kind not in (0, stat.S_IFREG):
                raise ValueError("Windows ZIP contains encryption, links or special files")
            if entry.is_dir():
                raise ValueError("Windows ZIP contains an unexpected directory")
            if entry.external_attr & 0x400:
                raise ValueError("Windows ZIP contains a reparse attribute")
        binary = archive.read(root + "/locron.exe")
        if pe_machine(binary) != TARGETS[target]:
            raise ValueError("Windows archive architecture differs from its target")
    return {"version": version(tag), "target": target, "unsigned": True,
            "binary_sha256": hashlib.sha256(binary).hexdigest()}


def package(tag, target, binary, directory, source=Path("."), execute=subprocess.run):
    release = version(tag)
    if target not in TARGETS or not binary.is_file() or binary.is_symlink():
        raise ValueError("missing or unsafe supported Windows executable")
    binary_bytes = binary.read_bytes()
    if pe_machine(binary_bytes) != TARGETS[target]:
        raise ValueError("compiled executable architecture differs from target")
    result = execute([str(binary.resolve()), "--version"], capture_output=True, text=True,
                     check=True, timeout=30)
    if result.stdout.strip() != "locron " + release:
        raise ValueError("compiled executable version differs from release tag")
    contents = {"locron.exe": binary_bytes}
    for name in FILES[1:]:
        path = source / name
        if not path.is_file() or path.is_symlink():
            raise ValueError("missing or unsafe Windows archive documentation")
        contents[name] = path.read_bytes()
    directory.mkdir(parents=True, exist_ok=True)
    output = directory / f"locron-{tag}-{target}.zip"
    with output.open("xb") as stream:
        with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name in FILES:
                member = zipfile.ZipInfo(f"locron-{tag}-{target}/{name}", date_time=(1980, 1, 1, 0, 0, 0))
                member.create_system = 3
                member.external_attr = (stat.S_IFREG | (0o755 if name == "locron.exe" else 0o644)) << 16
                member.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(member, contents[name])
    validate_archive(output, tag, target)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("package", "validate"))
    parser.add_argument("tag")
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("input", type=Path)
    parser.add_argument("directory", type=Path, nargs="?", default=Path("."))
    args = parser.parse_args()
    if args.mode == "package":
        print(package(args.tag, args.target, args.input, args.directory))
    else:
        print(validate_archive(args.input, args.tag, args.target))


if __name__ == "__main__":
    main()
