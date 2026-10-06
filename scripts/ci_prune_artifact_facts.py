"""Fixed runner-side artifact and binding facts for qualification CI only."""

def require(condition, phase):
    if not condition:
        raise ValueError("fixed artifact binding refusal phase=" + phase)

import json
import re

import hashlib
import os
import pathlib
import stat
import struct

ARTIFACT_CAP = 512 * 1024 * 1024
ARTIFACT_CHUNK = 65536


class WindowsReadApi:
    def __init__(self):
        import ctypes
        import msvcrt

        self.ct = ctypes
        self.crt = msvcrt
        require(os.name == "nt" and ctypes.sizeof(ctypes.c_void_p) == 8, "native-handle-width")
        require(ctypes.sizeof(ctypes.c_wchar) == 2 and ctypes.sizeof(ctypes.c_int32) == 4, "native-widths")

        class FileIdInfo(ctypes.Structure):
            _fields_ = [("volume", ctypes.c_uint64), ("identifier", ctypes.c_ubyte * 16)]

        class AttributeTagInfo(ctypes.Structure):
            _fields_ = [("attributes", ctypes.c_uint32), ("tag", ctypes.c_uint32)]

        class BasicInfo(ctypes.Structure):
            _fields_ = [
                ("creation", ctypes.c_int64),
                ("access", ctypes.c_int64),
                ("write", ctypes.c_int64),
                ("change", ctypes.c_int64),
                ("attributes", ctypes.c_uint32),
            ]

        require(ctypes.sizeof(FileIdInfo) == 24 and FileIdInfo.identifier.offset == 8, "file-id-layout")
        require(ctypes.alignment(FileIdInfo) == 8, "file-id-alignment")
        require(ctypes.sizeof(AttributeTagInfo) == 8 and AttributeTagInfo.tag.offset == 4, "attribute-layout")
        require(ctypes.sizeof(BasicInfo) == 40 and BasicInfo.attributes.offset == 32, "basic-layout")
        self.id_type = FileIdInfo
        self.tag_type = AttributeTagInfo
        self.basic_type = BasicInfo
        self.dll = ctypes.WinDLL("kernel32.dll", use_last_error=True)
        handle = ctypes.c_void_p
        dword = ctypes.c_uint32
        boolean = ctypes.c_int32
        self.create = self.dll.CreateFileW
        self.create.argtypes = [ctypes.c_wchar_p, dword, dword, handle, dword, dword, handle]
        self.create.restype = handle
        self.info = self.dll.GetFileInformationByHandleEx
        self.info.argtypes = [handle, ctypes.c_int32, handle, dword]
        self.info.restype = boolean
        self.file_type = self.dll.GetFileType
        self.file_type.argtypes = [handle]
        self.file_type.restype = dword
        self.size = self.dll.GetFileSizeEx
        self.size.argtypes = [handle, ctypes.POINTER(ctypes.c_int64)]
        self.size.restype = boolean
        self.close = self.dll.CloseHandle
        self.close.argtypes = [handle]
        self.close.restype = boolean
        self.invalid = ctypes.c_void_p(-1).value

    def snapshot(self, fd):
        # Only actual successful full FileIdInfo is authoritative. No stat/fallback ID.
        handle = self.crt.get_osfhandle(fd)
        require(handle not in (0, -1) and self.file_type(handle) == 1, "native-disk-file")
        identifier = self.id_type()
        tag = self.tag_type()
        basic = self.basic_type()
        size = self.ct.c_int64()
        require(self.info(handle, 18, self.ct.byref(identifier), self.ct.sizeof(identifier)) != 0, "full-file-id-success")
        require(self.info(handle, 9, self.ct.byref(tag), self.ct.sizeof(tag)) != 0, "attribute-tag-success")
        require(tag.attributes & (0x10 | 0x400) == 0 and tag.tag == 0, "native-regular-no-reparse")
        require(self.info(handle, 0, self.ct.byref(basic), self.ct.sizeof(basic)) != 0, "basic-info-success")
        require(basic.attributes == tag.attributes, "native-attributes-stable")
        require(self.size(handle, self.ct.byref(size)) != 0 and 0 < size.value <= ARTIFACT_CAP, "native-bounded-size")
        raw_id = bytes(identifier.identifier)
        identity = (identifier.volume, raw_id)
        # Core file-id 0.2.3 uses u128::from_le_bytes then {:032x}; keep that spelling.
        public_identity = {
            "platform": "windows",
            "volume_serial_number": identifier.volume,
            "file_id": format(int.from_bytes(raw_id, "little"), "032x"),
        }
        return identity, size.value, (basic.write, basic.change, tag.attributes), public_identity


class OwnedRead:
    def __init__(self, api):
        self.api = api
        self.native = None
        self.fd = None
        self.stream = None

    def open(self, path):
        if self.api is None:
            # Missing capability refuses; there is no followed-open fallback.
            self.fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
        else:
            # READ only, SHARE_READ only, NULL security (noninheritable), EXISTING,
            # OPEN_REPARSE_POINT. No CREATE/TRUNCATE/WRITE/DELETE/BACKUP flag.
            handle = self.api.create(str(path), 0x80000000, 1, None, 3, 0x00200000, None)
            require(handle not in (None, self.api.invalid), "strict-native-open")
            self.native = handle
            self.fd = self.api.crt.open_osfhandle(handle, os.O_RDONLY | os.O_NOINHERIT)
            # Successful open_osfhandle transfers ownership; never CloseHandle hereafter.
            self.native = None
            self.api.crt.setmode(self.fd, os.O_BINARY)
        require(not os.get_inheritable(self.fd), "noninherited-reader")
        # The wrapper borrows the fd. The OwnedRead closes the fd exactly once, even
        # if wrapper construction fails; no automatic fd-close ambiguity is needed.
        self.stream = os.fdopen(self.fd, "rb", buffering=0, closefd=False)
        return self

    def snapshot(self):
        if self.api is not None:
            return self.api.snapshot(self.fd)
        metadata = os.fstat(self.fd)
        require(stat.S_ISREG(metadata.st_mode), "unix-regular-file")
        require(0 <= metadata.st_dev < 2**64 and 0 < metadata.st_ino < 2**64, "unix-full-identity")
        require(0 < metadata.st_size <= ARTIFACT_CAP, "unix-bounded-size")
        identity = (metadata.st_dev, metadata.st_ino)
        public_identity = {"platform": "unix", "dev": metadata.st_dev, "ino": metadata.st_ino}
        return identity, metadata.st_size, (metadata.st_mtime_ns, metadata.st_ctime_ns, metadata.st_mode), public_identity

    def close_known(self):
        # Attempt each actual owned resource once. A failed/unknown close blocks PASS.
        ok = True
        if self.stream is not None:
            stream, self.stream = self.stream, None
            try:
                stream.close()
            except BaseException:
                ok = False
        if self.fd is not None:
            fd, self.fd = self.fd, None
            try:
                os.close(fd)
            except BaseException:
                ok = False
        if self.native is not None:
            handle, self.native = self.native, None
            try:
                ok = (self.api.close(handle) != 0) and ok
            except BaseException:
                ok = False
        return ok


def verify_abi(stream, size, host):
    # Preserve the already proposed fixed ELF/Mach-O/PE ABI check, on this stream.
    stream.seek(0)
    header = stream.read(64)
    arch = host.split("-")[0]
    if host.endswith("windows-msvc"):
        require(len(header) == 64 and header[:2] == b"MZ", "PE-header")
        offset = struct.unpack_from("<I", header, 60)[0]
        require(offset <= 1024 * 1024 and offset + 6 <= size, "PE-offset")
        stream.seek(offset)
        pe = stream.read(6)
        require(pe[:4] == b"PE\0\0" and struct.unpack_from("<H", pe, 4)[0] == {"x86_64": 0x8664, "aarch64": 0xAA64}[arch], "PE-native")
    elif host.endswith("apple-darwin"):
        require(header[:4] == b"\xcf\xfa\xed\xfe" and struct.unpack_from("<I", header, 4)[0] == {"x86_64": 0x01000007, "aarch64": 0x0100000C}[arch], "Mach-native")
    else:
        require(header[:6] == b"\x7fELF\x02\x01" and struct.unpack_from("<H", header, 18)[0] == {"x86_64": 62, "aarch64": 183}[arch], "ELF-native")


def owned_digest(reader, host):
    before = reader.snapshot()
    verify_abi(reader.stream, before[1], host)
    reader.stream.seek(0)
    digest = hashlib.sha256()
    total = 0
    while True:
        part = reader.stream.read(min(ARTIFACT_CHUNK, ARTIFACT_CAP - total + 1))
        require(isinstance(part, bytes), "actual-binary-read")
        if not part:
            break
        total += len(part)
        require(total <= ARTIFACT_CAP, "artifact-hash-cap")
        digest.update(part)
    after = reader.snapshot()
    require(before == after and total == before[1], "retained-snapshot-stable")
    return before, digest.hexdigest(), total


def facts(path, host):
    # Existing callers supply only the unique CargoJSON-selected managed artifact.
    path = pathlib.Path(path)
    text = str(path)
    require(path.is_absolute() and 0 < len(text.encode("utf-8")) <= 2048 and len(text.encode("utf-16-le")) // 2 <= 2048 and not any(ord(char) < 32 for char in text), "artifact-path")
    require((os.name == "nt") == host.endswith("windows-msvc"), "artifact-native-platform")
    api = WindowsReadApi() if os.name == "nt" else None
    first = OwnedRead(api)
    second = OwnedRead(api)
    body_failed = True
    try:
        first.open(path)
        original, digest, size = owned_digest(first, host)
        # Keep first fd/stream live while opening and checking independent path-after.
        second.open(path)
        independently_opened, second_digest, second_size = owned_digest(second, host)
        require(original == independently_opened == first.snapshot(), "retained-path-after-identity")
        require(digest == second_digest and size == second_size, "retained-path-after-hash")
        result = ({"path": text, "sha256": digest, "size": size}, original[3])
        body_failed = False
    finally:
        second_closed = second.close_known()
        first_closed = first.close_known()
        if not body_failed:
            require(second_closed and first_closed, "artifact-owned-close")
    return result


BINDING_JSON_CAP = 32768


def binding_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "binding-duplicate-field")
        result[key] = value
    return result


def owned_binding_bytes(reader):
    before = reader.snapshot()
    require(0 < before[1] <= BINDING_JSON_CAP, "binding-bounded-size")
    reader.stream.seek(0)
    parts = []
    digest = hashlib.sha256()
    total = 0
    while True:
        part = reader.stream.read(min(4096, BINDING_JSON_CAP - total + 1))
        require(isinstance(part, bytes), "binding-actual-binary-read")
        if not part:
            break
        total += len(part)
        require(total <= BINDING_JSON_CAP, "binding-read-cap")
        parts.append(part)
        digest.update(part)
    after = reader.snapshot()
    require(before == after and total == before[1], "binding-retained-snapshot-stable")
    return before, b"".join(parts), digest.hexdigest()


def binding_json_facts(production_path, recovery_path, host, expected_revision):
    # Inputs are ONLY the already CargoJSON/managed-root/closed-revision admitted
    # production/recovery artifacts. The locator is fixed; no input path/cap/mode/env.
    production_path = pathlib.Path(production_path)
    recovery_path = pathlib.Path(recovery_path)
    require(production_path.is_absolute() and recovery_path.is_absolute() and production_path.parent.name == "debug", "fixed-binding-locator")
    locator = production_path.parent.parent / "prune-artifact-binding.json"
    text = str(locator)
    require(0 < len(text.encode("utf-8")) <= 2048 and len(text.encode("utf-16-le")) // 2 <= 2048 and not any(ord(char) < 32 for char in text), "binding-locator-shape")
    require((os.name == "nt") == host.endswith("windows-msvc"), "binding-native-platform")
    api = WindowsReadApi() if os.name == "nt" else None
    first = OwnedRead(api)
    second = OwnedRead(api)
    body_failed = True
    try:
        first.open(locator)
        original, encoded, digest = owned_binding_bytes(first)
        # JSON is not an executable: it has its own fixed bound and closed schema.
        # No ABI call is disabled or made optional in the executable facts routine.
        data = json.loads(encoded.decode("utf-8"), object_pairs_hook=binding_object)
        require(type(data) is dict and set(data) == {"version", "revision", "production", "recovery"} and type(data["version"]) is int and data["version"] == 1, "binding-closed-version")
        revision = data["revision"]
        require(type(revision) is dict and set(revision) == {"head", "tree"} and all(type(value) is str and re.fullmatch(r"[0-9a-f]{40}", value) for value in revision.values()) and revision == expected_revision, "binding-closed-actual-revision")
        for name, path, test in (("production", production_path, False), ("recovery", recovery_path, True)):
            record = data[name]
            require(type(record) is dict and set(record) == {"path", "sha256", "size", "profile_test"} and record["profile_test"] is test and type(record["path"]) is str and record["path"] == str(path) and type(record["sha256"]) is str and re.fullmatch(r"[0-9a-f]{64}", record["sha256"]) and type(record["size"]) is int and 0 < record["size"] <= ARTIFACT_CAP, "binding-closed-artifact")
        require(data["production"]["path"] != data["recovery"]["path"], "binding-distinct-artifact-paths")
        second.open(locator)
        independently_opened, second_bytes, second_digest = owned_binding_bytes(second)
        require(original == independently_opened == first.snapshot(), "binding-retained-path-after-identity")
        require(encoded == second_bytes and digest == second_digest, "binding-retained-path-after-bytes-hash")
        result = (data, encoded, original[3], digest)
        body_failed = False
    finally:
        second_closed = second.close_known()
        first_closed = first.close_known()
        if not body_failed:
            require(second_closed and first_closed, "binding-owned-close")
    return result
