"""Raw Windows ZIP admission, kept in sync with windows_package.rs::catalog.

This module never extracts files or executes images. Check the release digest
before calling it; structural validity is not release provenance or ownership.
"""
import struct
import zipfile
import zlib


def validate_catalog(data: bytes, expected: set[str], limit: int) -> dict[str, int]:
    """Validate exact local/central records and return bounded decoded sizes."""
    if len(expected) not in (4, 5) or not 22 <= len(data) <= limit:
        raise ValueError("invalid Windows ZIP size or inventory")

    def at(offset, size):
        if offset < 0 or offset + size > len(data):
            raise ValueError("truncated Windows ZIP header")
        return memoryview(data)[offset:offset + size]

    def u16(offset):
        return struct.unpack("<H", at(offset, 2))[0]

    def u32(offset):
        return struct.unpack("<I", at(offset, 4))[0]

    end = len(data) - 22
    if u32(end) != 0x06054B50 or u16(end + 20) != 0:
        raise ValueError("Windows ZIP must end at its uncommented catalog")
    if (u16(end + 4) != 0 or u16(end + 6) != 0
            or u16(end + 8) != len(expected) or u16(end + 10) != len(expected)):
        raise ValueError("Windows ZIP must have the exact single-disk member inventory")
    start = u32(end + 16)
    if start + u32(end + 12) != end:
        raise ValueError("invalid Windows ZIP catalog span")

    offset, total = start, 0
    sizes, names, ranges = {}, set(), []
    for _ in range(len(expected)):
        if offset + 46 > end or u32(offset) != 0x02014B50:
            raise ValueError("invalid Windows ZIP central header")
        flags, method = u16(offset + 8), u16(offset + 10)
        if flags & ~0x0800 or method not in (0, 8):
            raise ValueError("encrypted or unsupported Windows ZIP member")
        compressed, size = u32(offset + 20), u32(offset + 24)
        name_length = u16(offset + 28)
        if u16(offset + 30) or u16(offset + 32) or u16(offset + 34):
            raise ValueError("unexpected Windows ZIP metadata")
        if offset + 46 + name_length > end:
            raise ValueError("Windows ZIP filename exceeds its catalog")
        raw_name = bytes(at(offset + 46, name_length))
        name = raw_name.decode("utf-8")
        if name not in expected or name.lower() in names:
            raise ValueError("unexpected or duplicate Windows ZIP filename")
        names.add(name.lower())
        attributes = u32(offset + 38)
        if ((attributes >> 16) & 0xF000) not in (0, 0x8000) or attributes & 0x0410:
            raise ValueError("Windows ZIP has linked, reparse or directory members")
        local = u32(offset + 42)
        if (local + 30 > start or u32(local) != 0x04034B50
                or u16(local + 6) != flags or u16(local + 8) != method
                or u32(local + 14) != u32(offset + 16)
                or u32(local + 18) != compressed or u32(local + 22) != size
                or u16(local + 26) != name_length or u16(local + 28) != 0):
            raise ValueError("Windows ZIP local and central records disagree")
        if bytes(at(local + 30, name_length)) != raw_name:
            raise ValueError("Windows ZIP local filename differs")
        local_end = local + 30 + name_length + compressed
        if local_end > start:
            raise ValueError("Windows ZIP payload overlaps its catalog")
        total += size
        if total > limit:
            raise ValueError("Windows ZIP expands beyond its size limit")
        sizes[name] = size
        ranges.append((local, local_end))
        offset += 46 + name_length
    if offset != end or len(names) != len(expected):
        raise ValueError("Windows ZIP has extra catalog data")
    previous = 0
    for begin, finish in sorted(ranges):
        if begin != previous:
            raise ValueError("Windows ZIP has overlapping or extra payload data")
        previous = finish
    if previous != start:
        raise ValueError("Windows ZIP payload inventory is incomplete")
    return sizes


def read_member(data: bytes, local: int, size: int) -> bytes:
    """Decode one admitted raw member without trusting ZipExtFile's size cap.

    Call only after validate_catalog. Its aggregate byte limit and matching
    headers bound this local span and size; neither is taken from user paths.
    """
    method = struct.unpack_from("<H", data, local + 8)[0]
    crc, compressed = struct.unpack_from("<II", data, local + 14)
    name_length = struct.unpack_from("<H", data, local + 26)[0]
    start = local + 30 + name_length
    payload = memoryview(data)[start:start + compressed]
    if method == 0:
        if compressed != size:
            raise zipfile.BadZipFile("Windows ZIP stored member size differs")
        content = bytes(payload)
    else:
        inflater = zlib.decompressobj(-15)
        try:
            content = inflater.decompress(payload, size + 1)
        except zlib.error as error:
            raise zipfile.BadZipFile("invalid Windows ZIP deflate stream") from error
        if (len(content) != size or not inflater.eof
                or inflater.unused_data or inflater.unconsumed_tail):
            raise zipfile.BadZipFile("Windows ZIP deflate size or stream boundary differs")
    if zlib.crc32(content) != crc:
        raise zipfile.BadZipFile("Windows ZIP member CRC differs")
    return content
