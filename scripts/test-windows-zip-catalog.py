#!/usr/bin/env python3
"""Offline raw-record regressions for the Windows release/WinGet boundary."""
import hashlib
import io
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import windows_release as windows
from windows_zip import validate_catalog

TAG = "v0.10.0"
TARGET = "x86_64-pc-windows-msvc"


def pe(machine, subsystem):
    data = bytearray(512)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 128)
    data[128:132] = b"PE\0\0"
    for at, value in ((132, machine), (134, 1), (148, 240), (150, 0x22), (152, 0x20B), (220, subsystem)):
        struct.pack_into("<H", data, at, value)
    struct.pack_into("<I", data, 260, 16)
    return bytes(data)


def fixture(paired=True, target=TARGET, method=zipfile.ZIP_DEFLATED):
    files = windows.PAIRED_FILES if paired else windows.FILES
    prefix = f"locron-{TAG}-{target}/"
    contents = {prefix + name: pe(windows.TARGETS[target], windows.SUBSYSTEMS[name])
                if name in windows.SUBSYSTEMS else b"catalog fixture documentation" for name in files}
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", compression=method) as archive:
        for name, content in contents.items():
            archive.writestr(name, content)
    return stream.getvalue(), contents


def number(data, at, width=4):
    return struct.unpack_from("<I" if width == 4 else "<H", data, at)[0]


def change(data, at, value, width=4):
    result = bytearray(data)
    struct.pack_into("<I" if width == 4 else "<H", result, at, value)
    return bytes(result)


def catalog_rows(data):
    at = number(data, len(data) - 6)
    rows = []
    for _ in range(number(data, len(data) - 12, 2)):
        rows.append(at)
        at += 46 + number(data, at + 28, 2)
    return rows


def rebuild(data, records=None, prefix=b"", gap=b"", trailer=b""):
    """Keep offsets/counts internally consistent while changing physical coverage."""
    rows = catalog_rows(data)
    selected = range(len(rows)) if records is None else records
    local_bytes, central_bytes = bytearray(prefix), bytearray()
    for index in selected:
        row = rows[index]
        local = number(data, row + 42)
        size = 30 + number(data, row + 28, 2) + number(data, row + 20)
        central = bytearray(data[row:row + 46 + number(data, row + 28, 2)])
        struct.pack_into("<I", central, 42, len(local_bytes))
        local_bytes.extend(data[local:local + size])
        local_bytes.extend(gap)
        central_bytes.extend(central)
    footer = bytearray(data[-22:])
    struct.pack_into("<I", footer, 12, len(central_bytes))
    struct.pack_into("<I", footer, 16, len(local_bytes))
    return bytes(local_bytes + central_bytes + footer) + trailer


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="locron-catalog-")
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name) / "fixture.zip"
        self.raw, self.contents = fixture()
        self.expected = set(self.contents)
        self.rows = catalog_rows(self.raw)

    def validate(self, data, expected=None, limit=windows.MAX_ARCHIVE_BYTES):
        return validate_catalog(data, self.expected if expected is None else expected, limit)

    def inspect(self, data, paired=True, target=TARGET):
        self.path.write_bytes(data)
        return windows.inspect_archive(self.path, TAG, target, paired,
                                       expected_sha256=hashlib.sha256(data).hexdigest())

    def test_valid_four_and_five_file_archives_on_both_architectures(self):
        for paired in (False, True):
            for target in windows.TARGETS:
                for method in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
                    with self.subTest(paired=paired, target=target, method=method):
                        data, contents = fixture(paired, target, method)
                        self.assertEqual(self.validate(data, set(contents)),
                                         {name: len(value) for name, value in contents.items()})
                        facts = self.inspect(data, paired, target)
                        self.assertEqual(facts["archive_sha256"], hashlib.sha256(data).hexdigest())
                        self.assertEqual(facts["target"], target)

    def test_truncation_size_and_inventory_bounds(self):
        for size in (0, 1, 21, 22, 30, len(self.raw) - 1):
            with self.subTest(size=size), self.assertRaises(ValueError):
                self.validate(self.raw[:size])
        with self.assertRaises(ValueError):
            self.validate(self.raw, limit=len(self.raw) - 1)
        with self.assertRaises(ValueError):
            self.validate(self.raw, expected=set())

    def test_eocd_signature_comment_disks_and_member_counts(self):
        end = len(self.raw) - 22
        for at, value, width in ((0, 0, 4), (4, 1, 2), (6, 1, 2), (8, 4, 2),
                                 (10, 6, 2), (10, 65535, 2), (20, 1, 2)):
            with self.subTest(at=at, value=value), self.assertRaises(ValueError):
                self.validate(change(self.raw, end + at, value, width))

    def test_central_span_signature_and_impossible_offsets(self):
        for at, value in ((len(self.raw) - 10, 0), (len(self.raw) - 6, 0xFFFFFFFF),
                          (self.rows[0], 0), (self.rows[0] + 42, 0xFFFFFFFF)):
            with self.subTest(at=at), self.assertRaises(ValueError):
                self.validate(change(self.raw, at, value))

    def test_only_utf8_flag_and_stored_or_deflate_methods_are_admitted(self):
        row = self.rows[0]
        local = number(self.raw, row + 42)
        for flag in (1, 2, 4, 8, 16, 32, 64, 256, 1024, 4096, 0xFFFF):
            data = change(change(self.raw, row + 8, flag, 2), local + 6, flag, 2)
            with self.subTest(flag=flag), self.assertRaises(ValueError):
                self.validate(data)
        utf8 = change(change(self.raw, row + 8, 0x800, 2), local + 6, 0x800, 2)
        self.validate(utf8)
        for method in (1, 9, 12, 14, 99):
            data = change(change(self.raw, row + 10, method, 2), local + 8, method, 2)
            with self.subTest(method=method), self.assertRaises(ValueError):
                self.validate(data)

    def test_central_extra_comment_disk_and_name_spans(self):
        for field in (28, 30, 32, 34):
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.validate(change(self.raw, self.rows[0] + field, 0xFFFF, 2))

    def test_raw_names_never_normalize_nul_case_backslash_or_utf8(self):
        row = self.rows[0]
        local = number(self.raw, row + 42)
        original = self.raw[row + 46:row + 46 + number(self.raw, row + 28, 2)]
        for name in (original.upper(), original.replace(b"/", b"\\"),
                     b"\0" + original[1:], b"\xff" + original[1:], b"../" + original[3:]):
            data = bytearray(self.raw)
            data[row + 46:row + 46 + len(name)] = name
            data[local + 30:local + 30 + len(name)] = name
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.validate(bytes(data))

    def test_dos_directory_reparse_and_nonregular_unix_attributes(self):
        for value in (0x10, 0x400, 0xA0000000, 0x40000000, 0x10000000, 0x60000000):
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.validate(change(self.raw, self.rows[0] + 38, value))

    def test_each_local_record_field_must_agree_with_the_central_view(self):
        local = number(self.raw, self.rows[0] + 42)
        for field, width in ((0, 4), (6, 2), (8, 2), (14, 4), (18, 4), (22, 4), (26, 2), (28, 2)):
            value = number(self.raw, local + field, width) ^ 1
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.validate(change(self.raw, local + field, value, width))

    def test_local_filename_must_equal_the_raw_central_name(self):
        local = number(self.raw, self.rows[0] + 42)
        data = bytearray(self.raw)
        data[local + 30] ^= 1
        with self.assertRaisesRegex(ValueError, "local filename"):
            self.validate(bytes(data))

    def test_duplicate_raw_members_and_aliasing_local_offsets_refuse(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.validate(rebuild(self.raw, records=(0, 1, 2, 2, 4)))
        data = change(self.raw, self.rows[1] + 42, number(self.raw, self.rows[0] + 42))
        with self.assertRaises(ValueError):
            self.validate(data)

    def test_reordered_central_rows_and_reordered_members_remain_valid(self):
        rows = [self.raw[at:at + 46 + number(self.raw, at + 28, 2)] for at in self.rows]
        reordered = self.raw[:self.rows[0]] + b"".join(reversed(rows)) + self.raw[-22:]
        self.assertEqual(self.validate(reordered), self.validate(self.raw))
        self.assertEqual(self.validate(rebuild(self.raw, records=(4, 3, 2, 1, 0))), self.validate(self.raw))

    def test_self_consistent_prefix_gaps_and_extra_payloads_refuse(self):
        for data in (rebuild(self.raw, prefix=b"prefix"), rebuild(self.raw, gap=b"hidden")):
            with self.assertRaisesRegex(ValueError, "extra payload"):
                self.validate(data)

    def test_trailer_and_archive_comments_refuse(self):
        comment = change(self.raw, len(self.raw) - 2, 7, 2) + b"comment"
        for data in (self.raw + b"x", self.raw + self.raw, comment):
            with self.assertRaises(ValueError):
                self.validate(data)

    def test_extra_central_data_and_catalog_overlap_refuse(self):
        extra = bytearray(self.raw[:-22] + b"hidden" + self.raw[-22:])
        struct.pack_into("<I", extra, len(extra) - 10, number(self.raw, len(self.raw) - 10) + 6)
        with self.assertRaisesRegex(ValueError, "extra catalog"):
            self.validate(bytes(extra))
        row = self.rows[0]
        local = number(self.raw, row + 42)
        overlap = change(change(self.raw, row + 20, len(self.raw)), local + 18, len(self.raw))
        with self.assertRaisesRegex(ValueError, "overlaps"):
            self.validate(overlap)

    def test_aggregate_decoded_size_is_bounded_before_decompression(self):
        limit = len(self.raw) + 100
        data = self.raw
        for row in self.rows[:2]:
            local = number(data, row + 42)
            data = change(change(data, row + 24, limit // 2 + 1), local + 22, limit // 2 + 1)
        with self.assertRaisesRegex(ValueError, "expands"):
            self.validate(data, limit=limit)

    def test_invalid_catalog_cannot_reach_zip_reader_or_native_probes(self):
        for paired in (False, True):
            data, _ = fixture(paired)
            self.path.write_bytes(data + b"trailer")
            with patch.object(windows.zipfile, "ZipFile", side_effect=AssertionError("decompression")), \
                    patch.object(windows, "probe_pair", side_effect=AssertionError("execution")):
                with self.assertRaises(ValueError):
                    windows.validate_archive(self.path, TAG, TARGET, paired=paired)

    def test_checksum_comparison_still_precedes_raw_catalog_admission(self):
        self.path.write_bytes(b"not a ZIP")
        with patch.object(windows, "validate_catalog", side_effect=AssertionError("raw parse before hash")):
            with self.assertRaisesRegex(ValueError, "final release checksum"):
                windows.inspect_archive(self.path, TAG, TARGET, paired=True, expected_sha256="0" * 64)

    def test_corrupt_documentation_crc_refuses_in_both_layouts(self):
        for paired in (False, True):
            raw, contents = fixture(paired, method=zipfile.ZIP_STORED)
            for row in catalog_rows(raw):
                name = raw[row + 46:row + 46 + number(raw, row + 28, 2)].decode()
                if name.endswith(".exe"):
                    continue
                local = number(raw, row + 42)
                data = bytearray(raw)
                data[local + 30 + number(raw, local + 26, 2)] ^= 1
                with self.subTest(paired=paired, name=name):
                    self.validate(bytes(data), set(contents))
                    with self.assertRaises(zipfile.BadZipFile):
                        self.inspect(bytes(data), paired)


    def test_forged_short_size_and_prefix_crc_cannot_hide_decoded_suffix(self):
        import zlib
        for method in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
            raw, contents = fixture(method=method)
            row = catalog_rows(raw)[2]  # README, not an executable.
            local = number(raw, row + 42)
            name = raw[row + 46:row + 46 + number(raw, row + 28, 2)].decode()
            for size in (0, len(contents[name]) - 1):
                crc = zlib.crc32(contents[name][:size])
                data = change(change(raw, row + 24, size), local + 22, size)
                data = change(change(data, row + 16, crc), local + 14, crc)
                with self.subTest(method=method, size=size):
                    self.validate(data, set(contents))
                    with self.assertRaises(zipfile.BadZipFile):
                        self.inspect(data)

    def test_incomplete_or_suffixed_deflate_stream_refuses(self):
        row = self.rows[-1]
        local = number(self.raw, row + 42)
        start = local + 30 + number(self.raw, local + 26, 2)
        compressed = self.raw[start:self.rows[0]]
        for payload in (compressed[:-1], compressed + b"hidden", compressed + compressed):
            delta = len(payload) - len(compressed)
            data = bytearray(self.raw[:start] + payload + self.raw[self.rows[0]:])
            shifted_row = row + delta
            struct.pack_into("<I", data, local + 18, len(payload))
            struct.pack_into("<I", data, shifted_row + 20, len(payload))
            struct.pack_into("<I", data, len(data) - 6, self.rows[0] + delta)
            self.validate(bytes(data))
            with self.assertRaises(zipfile.BadZipFile):
                self.inspect(bytes(data))


if __name__ == "__main__":
    unittest.main()
