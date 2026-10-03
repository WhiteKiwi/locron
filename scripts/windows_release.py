#!/usr/bin/env python3
"""Build and inspect unsigned Windows ZIPs without changing installed software."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess
import tempfile
import zipfile

from windows_probe import MAX_PROBE_BYTES, run_probe
from windows_zip import read_member, validate_catalog

TARGETS = {"x86_64-pc-windows-msvc": 0x8664, "aarch64-pc-windows-msvc": 0xAA64}
FILES = ("locron.exe", "README.md", "LICENSE-MIT", "LICENSE-APACHE")
PAIRED_FILES = (FILES[0], "locron-service-launcher.exe", *FILES[1:])
SUBSYSTEMS = {"locron.exe": 3, "locron-service-launcher.exe": 2}
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024
# Deliberately finite: a new DLL requires review against the minimum Windows 11
# image. VC redistributables and application libraries never pass this gate.
SYSTEM_DLLS = frozenset((
    # WaitOnAddress/WakeByAddress*: Microsoft documents this exact API set
    # since Windows 8; it is a stock OS contract, not a redistributable.
    # https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitonaddress
    "api-ms-win-core-synch-l1-2-0.dll",
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "cfgmgr32.dll", "combase.dll",
    "crypt32.dll", "dbghelp.dll", "gdi32.dll", "iphlpapi.dll", "kernel32.dll", "kernelbase.dll",
    "netapi32.dll", "normaliz.dll", "ntdll.dll", "ole32.dll", "oleaut32.dll", "psapi.dll",
    "rpcrt4.dll", "sechost.dll", "secur32.dll", "shell32.dll", "shlwapi.dll", "ucrtbase.dll",
    "user32.dll", "userenv.dll", "version.dll", "winhttp.dll", "winmm.dll", "wintrust.dll",
    "ws2_32.dll", "wtsapi32.dll",
))


def version(tag):
    if not re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", tag):
        raise ValueError("expected stable vMAJOR.MINOR.PATCH tag")
    return tag[1:]


def pe_layout(binary):
    """Read only bounded PE32+ headers; never load an unverified executable."""
    if len(binary) < 64 or binary[:2] != b"MZ":
        raise ValueError("Windows executable has no DOS/PE header")
    offset = struct.unpack_from("<I", binary, 60)[0]
    if offset + 24 + 152 > len(binary) or binary[offset:offset + 4] != b"PE\0\0":
        raise ValueError("Windows executable has a truncated or invalid PE header")
    machine = struct.unpack_from("<H", binary, offset + 4)[0]
    sections = struct.unpack_from("<H", binary, offset + 6)[0]
    characteristics = struct.unpack_from("<H", binary, offset + 22)[0]
    if not 0 < sections <= 96 or not characteristics & 0x2 or characteristics & 0x2000:
        raise ValueError("Windows archive must contain an executable image, not a DLL")
    optional_size = struct.unpack_from("<H", binary, offset + 20)[0]
    optional = offset + 24
    if optional_size < 152 or optional + optional_size > len(binary):
        raise ValueError("Windows executable has a truncated optional header")
    if struct.unpack_from("<H", binary, optional)[0] != 0x20B or machine not in TARGETS.values():
        raise ValueError("Windows executable is not supported x64/ARM64 PE32+")
    directories = struct.unpack_from("<I", binary, optional + 108)[0]
    if not 5 <= directories <= 16 or optional_size < 112 + directories * 8:
        raise ValueError("Windows executable has no complete security directory")
    if struct.unpack_from("<II", binary, optional + 144) != (0, 0):
        raise ValueError("initial Windows release must be unsigned (certificate table is present)")
    section_table = optional + optional_size
    if section_table + sections * 40 > len(binary):
        raise ValueError("Windows executable has a truncated section table")
    return machine, optional, directories, section_table, sections


def pe_machine(binary):
    """Require a PE32+ image for a supported architecture with no certificate table."""
    return pe_layout(binary)[0]


def pe_imports(binary):
    """Validate normal and delayed imports against the stock-system allowlist."""
    _, optional, directories, table, count = pe_layout(binary)
    sections = []
    for index in range(count):
        row = table + index * 40
        virtual_size, virtual_address, raw_size, raw_address = struct.unpack_from("<IIII", binary, row + 8)
        if raw_address + raw_size > len(binary):
            raise ValueError("Windows executable section lies outside the file")
        sections.append((virtual_address, max(virtual_size, raw_size), raw_address, raw_size))
    headers = struct.unpack_from("<I", binary, optional + 60)[0]

    def at(rva, size):
        matches = [(raw + rva - start) for start, length, raw, raw_size in sections
                   if start <= rva and rva + size <= start + length and rva + size <= start + raw_size]
        if rva < headers and rva + size <= min(headers, len(binary)):
            matches.append(rva)
        if len(matches) != 1:
            raise ValueError("Windows executable has an invalid or ambiguous import RVA")
        return matches[0]

    def name(rva):
        data = bytearray()
        for index in range(256):
            byte = binary[at(rva + index, 1)]
            if byte == 0:
                break
            data.append(byte)
        else:
            raise ValueError("Windows executable has an unterminated DLL name")
        try:
            dll = data.decode("ascii").lower()
        except UnicodeDecodeError as error:
            raise ValueError("Windows executable has a non-ASCII DLL name") from error
        if dll not in SYSTEM_DLLS:
            raise ValueError(f"Windows executable needs a non-stock DLL: {dll!r}")
        return dll

    imports = set()
    for directory, stride, name_offset in ((1, 20, 12), (13, 32, 4)):
        if directory >= directories:
            continue
        rva, size = struct.unpack_from("<II", binary, optional + 112 + directory * 8)
        if (rva, size) == (0, 0):
            continue
        if not rva or size < stride or size > stride * 256:
            raise ValueError("Windows executable has an invalid import directory")
        start = at(rva, size)
        for offset in range(0, size - stride + 1, stride):
            descriptor = binary[start + offset:start + offset + stride]
            if not any(descriptor):
                break
            if directory == 13 and struct.unpack_from("<I", descriptor)[0] != 1:
                raise ValueError("Windows executable has unsupported delayed-import attributes")
            imports.add(name(struct.unpack_from("<I", descriptor, name_offset)[0]))
        else:
            raise ValueError("Windows executable has no import-directory terminator")
    return sorted(imports)


def system_environment():
    """Remove developer-tool PATH entries only from the verification child."""
    environment = os.environ.copy()
    windows = environment.get("SystemRoot", environment.get("SYSTEMROOT"))
    if os.name == "nt" and not windows:
        raise ValueError("Windows system directory is unavailable for the package check")
    for key in list(environment):
        if key.upper() == "PATH":
            del environment[key]
    environment["PATH"] = os.pathsep.join((str(Path(windows) / "System32"), windows)) if windows else ""
    return environment


def launcher_identity(output, release, target):
    def unique_object(pairs):
        result = {}
        for name, value in pairs:
            if name in result:
                raise ValueError("launcher probe has a duplicate field")
            result[name] = value
        return result

    if not output.startswith("{"):
        raise ValueError("launcher probe must start with a JSON object")
    facts, end = json.JSONDecoder(object_pairs_hook=unique_object).raw_decode(output)
    expected = {"schema": "locron.windows-launcher-probe/v1", "version": release,
                "target": target, "launcher_abi": "native-gui-v1"}
    if (end != len(output) or set(facts) != set(expected) | {"initial_conout_opened", "initial_conout_error"}
            or any(facts[name] != value for name, value in expected.items())):
        raise ValueError("launcher probe has mismatched identity, fields or trailing bytes")
    opened, error = facts["initial_conout_opened"], facts["initial_conout_error"]
    if (type(opened) is not bool or (opened and error is not None)
            or (not opened and (type(error) is not int or error == 0 or not -(2**31) <= error < 2**31))):
        raise ValueError("launcher probe has inconsistent raw console facts")
    return facts


def probe_pair(contents, release, target, execute):
    """Probe the inspected ZIP bytes, with no developer PATH or sibling DLLs."""
    execute = run_probe if execute is None else execute
    with tempfile.TemporaryDirectory(prefix="locron-paired-probe-") as temporary:
        staged = Path(temporary)
        for name, content in contents.items():
            (staged / name).write_bytes(content)

        def output(name, argument):
            result = execute([str(staged / name), argument], cwd=staged, stdin=subprocess.DEVNULL,
                             capture_output=True, check=True, timeout=30, env=system_environment())
            if result.stderr or len(result.stdout) > MAX_PROBE_BYTES:
                raise ValueError("paired executable probe has stderr or oversized output")
            return result.stdout.decode("utf-8")

        versions = {}
        for name in SUBSYSTEMS:
            versions[name] = output(name, "--version")
            if versions[name] != name.removesuffix(".exe") + " " + release + "\n":
                raise ValueError("paired executable version differs from release tag")
        identity = launcher_identity(output("locron-service-launcher.exe", "--identity-probe"), release, target)
        return versions, identity


def _inspect_archive(path, tag, target, paired=False, expected_sha256=None):
    version(tag)
    if target not in TARGETS:
        raise ValueError("unsupported Windows release target")
    if not path.is_file() or path.is_symlink() or path.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ValueError("unsafe or oversized Windows archive")
    # Hash and inspect one bounded snapshot, never two independent path reads.
    with path.open("rb") as stream:
        archive_bytes = stream.read(MAX_ARCHIVE_BYTES + 1)
    if len(archive_bytes) > MAX_ARCHIVE_BYTES:
        raise ValueError("Windows archive exceeds its byte limit")
    digest = hashlib.sha256(archive_bytes).hexdigest()
    if expected_sha256 is not None and digest != expected_sha256:
        raise ValueError("WinGet archive differs from the final release checksum")
    root = f"locron-{tag}-{target}"
    files = PAIRED_FILES if paired else FILES
    allowed = {root + "/" + name for name in files}
    sizes = validate_catalog(archive_bytes, allowed, MAX_ARCHIVE_BYTES)
    with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
        entries = archive.infolist()
        if paired and any(entry.orig_filename != entry.filename for entry in entries):
            raise ValueError("paired Windows ZIP contains a normalized raw filename")
        names = [entry.filename for entry in entries]
        if len(names) != len(set(name.casefold() for name in names)) or set(names) != allowed or len(names) != len(files):
            raise ValueError(f"Windows ZIP differs from the exact {'five' if paired else 'four'}-file inventory")
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
        # Consume every member, not only the legacy executable: CRC errors in
        # documentation must not pass release/WinGet admission either.
        contents = {}
        for name in files:
            member = root + "/" + name
            contents[name] = read_member(archive_bytes, archive.getinfo(member).header_offset, sizes[member])
        if paired:
            binaries = {}
            for name, subsystem in SUBSYSTEMS.items():
                binary = contents[name]
                machine, optional, _, _, _ = pe_layout(binary)
                if machine != TARGETS[target] or struct.unpack_from("<H", binary, optional + 68)[0] != subsystem:
                    raise ValueError("paired executable architecture or PE subsystem differs from its role")
                binaries[name] = {"sha256": hashlib.sha256(binary).hexdigest(),
                                  "subsystem": subsystem, "imports": pe_imports(binary)}
        else:
            binary = contents["locron.exe"]
    if paired:
        return ({"version": version(tag), "target": target, "unsigned": True, "mode": "paired-static",
                 "binaries": binaries}, contents, digest)
    if pe_machine(binary) != TARGETS[target]:
        raise ValueError("Windows archive architecture differs from its target")
    imports = pe_imports(binary)
    return ({"version": version(tag), "target": target, "unsigned": True,
             "binary_sha256": hashlib.sha256(binary).hexdigest(), "imports": imports}, {}, digest)


def inspect_archive(path, tag, target, paired=False, expected_sha256=None):
    """Inspect either architecture without execution, extraction or runtime claims."""
    facts, _, digest = _inspect_archive(path, tag, target, paired, expected_sha256)
    return {**facts, "archive_sha256": digest}


def validate_archive(path, tag, target, paired=False, execute=None):
    # Native package/validate gates still require every paired runtime probe.
    facts, contents, _ = _inspect_archive(path, tag, target, paired)
    if paired:
        versions, identity = probe_pair(contents, version(tag), target, execute)
        facts.update(mode="paired-draft", version_probes=versions,
                     launcher_abi=identity["launcher_abi"], launcher_identity=identity)
    return facts


def write_archive(output, tag, target, contents, files):
    with output.open("xb") as stream:
        with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name in files:
                member = zipfile.ZipInfo(f"locron-{tag}-{target}/{name}", date_time=(1980, 1, 1, 0, 0, 0))
                member.create_system = 3
                member.external_attr = (stat.S_IFREG | (0o755 if name in SUBSYSTEMS else 0o644)) << 16
                member.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(member, contents[name])


def package_pair(tag, target, binary, launcher, directory, source, execute):
    version(tag)
    if target not in TARGETS:
        raise ValueError("unsupported Windows release target")
    paths = {"locron.exe": binary, "locron-service-launcher.exe": launcher,
             **{name: source / name for name in FILES[1:]}}
    contents, remaining = {}, MAX_ARCHIVE_BYTES
    for name, path in paths.items():
        if not path.is_file() or path.is_symlink():
            raise ValueError("missing or unsafe paired Windows package input")
        with path.open("rb") as stream:
            contents[name] = stream.read(remaining + 1)
        remaining -= len(contents[name])
        if remaining < 0:
            raise ValueError("paired Windows package exceeds its aggregate size limit")
    with tempfile.TemporaryDirectory(prefix="locron-paired-package-") as temporary:
        candidate = Path(temporary) / f"locron-{tag}-{target}.zip"
        write_archive(candidate, tag, target, contents, PAIRED_FILES)
        validate_archive(candidate, tag, target, paired=True, execute=execute)
        final_bytes = candidate.read_bytes()
        directory.mkdir(parents=True, exist_ok=True)
        output = directory / candidate.name
        stream = output.open("xb")  # Failure here never authorizes removing an existing file.
        try:
            with stream:
                stream.write(final_bytes)
        except BaseException:
            output.unlink(missing_ok=True)
            raise
    return output


def package(tag, target, binary, directory, source=Path("."), execute=None, launcher=None):
    if launcher is not None:
        return package_pair(tag, target, binary, launcher, directory, source, execute)
    release = version(tag)
    execute = run_probe if execute is None else execute
    if target not in TARGETS or not binary.is_file() or binary.is_symlink():
        raise ValueError("missing or unsafe supported Windows executable")
    binary_bytes = binary.read_bytes()
    if pe_machine(binary_bytes) != TARGETS[target]:
        raise ValueError("compiled executable architecture differs from target")
    pe_imports(binary_bytes)
    result = execute([str(binary.resolve()), "--version"], capture_output=True, text=True,
                     check=True, timeout=30, env=system_environment())
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
    write_archive(output, tag, target, contents, FILES)
    validate_archive(output, tag, target)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("package", "validate"))
    parser.add_argument("tag")
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("input", type=Path)
    parser.add_argument("directory", type=Path, nargs="?", default=Path("."))
    parser.add_argument("--launcher", type=Path, metavar="PATH",
                        help="package only: create a draft five-file ZIP after staged native version/identity probes")
    parser.add_argument("--paired", action="store_true",
                        help="validate only: inspect five-file ZIP and execute staged native --version/--identity-probe")
    args = parser.parse_args()
    if (args.mode == "package" and args.paired) or (args.mode == "validate" and args.launcher is not None):
        parser.error("--launcher is package-only; --paired is validate-only")
    if args.mode == "package":
        print(package(args.tag, args.target, args.input, args.directory, launcher=args.launcher))
    else:
        print(json.dumps(validate_archive(args.input, args.tag, args.target, paired=args.paired), sort_keys=True))


if __name__ == "__main__":
    main()
