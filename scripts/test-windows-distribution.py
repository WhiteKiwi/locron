#!/usr/bin/env python3
"""Hermetic Windows distribution boundaries; no installation, tasks or publication."""
import hashlib
import importlib.util
from pathlib import Path
import struct
import tempfile
from types import SimpleNamespace
import unittest
import zipfile

import windows_release as windows

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("release_assets", ROOT / "scripts/release-assets.py")
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)
TAG = "v0.10.0"
TARGET = "x86_64-pc-windows-msvc"


def executable(machine=0x8664, signed=False):
    binary = bytearray(512)
    binary[:2] = b"MZ"
    struct.pack_into("<I", binary, 60, 128)
    binary[128:132] = b"PE\0\0"
    struct.pack_into("<H", binary, 132, machine)
    struct.pack_into("<H", binary, 134, 1)
    struct.pack_into("<H", binary, 148, 240)
    struct.pack_into("<H", binary, 150, 0x22)
    struct.pack_into("<H", binary, 152, 0x20B)
    struct.pack_into("<I", binary, 260, 16)
    if signed:
        struct.pack_into("<II", binary, 296, 480, 32)
    return bytes(binary)


class WindowsDistributionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binary = self.root / "locron.exe"
        self.binary.write_bytes(executable())
        for name in windows.FILES[1:]:
            (self.root / name).write_text("fixture documentation", encoding="utf-8")

    def package(self, target=TARGET, output="output", reported="locron 0.10.0\n"):
        return windows.package(TAG, target, self.binary, self.root / output, source=self.root,
                               execute=lambda *args, **kwargs: SimpleNamespace(stdout=reported))

    def test_native_architecture_unsigned_version_and_exact_inventory(self):
        for target, machine in windows.TARGETS.items():
            self.binary.write_bytes(executable(machine))
            archive = self.package(target, output=target)
            facts = windows.validate_archive(archive, TAG, target)
            self.assertEqual(facts["binary_sha256"], hashlib.sha256(self.binary.read_bytes()).hexdigest())
            self.assertTrue(facts["unsigned"])
            with zipfile.ZipFile(archive) as stream:
                self.assertEqual(len(stream.infolist()), 4)
        self.binary.write_bytes(executable())
        for binary in (b"corrupt", executable(0x14C), executable(signed=True)):
            self.binary.write_bytes(binary)
            with self.assertRaises(ValueError):
                self.package(output="refused")
            self.assertFalse((self.root / "refused").exists())
        self.binary.write_bytes(executable())
        with self.assertRaises(ValueError):
            self.package(output="wrong-version", reported="locron 0.9.6")
        self.assertFalse((self.root / "wrong-version").exists())

    def test_archive_links_duplicates_traversal_and_extra_files_refused(self):
        for extra in ("../outside", "/absolute", "extra", f"locron-{TAG}-{TARGET}/locron.exe",
                      f"locron-{TAG}-{TARGET}/README.MD", "C:/outside", "dir\\outside"):
            archive = self.package(output=str(len(extra)) + "-" + str(abs(hash(extra))))
            with zipfile.ZipFile(archive, "a") as stream:
                stream.writestr(extra, b"untrusted")
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                windows.validate_archive(archive, TAG, TARGET)
        archive = self.package(output="link")
        with zipfile.ZipFile(archive, "a") as stream:
            member = zipfile.ZipInfo(f"locron-{TAG}-{TARGET}/link")
            member.create_system = 3
            member.external_attr = 0o120777 << 16
            stream.writestr(member, "outside")
        with self.assertRaises(ValueError):
            windows.validate_archive(archive, TAG, TARGET)

    def test_exact_versioned_legacy_and_windows_publication_inventories(self):
        for tag in ("v0.1.0", "v0.1.1", "v0.2.0", "v0.3.0", "v0.9.6", "v0.9.7", TAG, "v1.0.0"):
            windows_tag = assets.includes_windows(tag)
            self.assertEqual(len(assets.expected_assets(tag)), 10 if windows_tag else 8)
            installers = assets.expected_installers(tag, Path("install.sh"), Path("install.ps1"), Path("uninstall.ps1"))
            self.assertEqual(set(installers), {"install.sh", "install.ps1", "uninstall.ps1"} if windows_tag
                             else {"install.sh"} if tag not in ("v0.1.0", "v0.1.1", "v0.2.0") else set())
        directory = self.root / "release"
        directory.mkdir()
        self.package(output="release")
        self.binary.write_bytes(executable(0xAA64))
        self.package("aarch64-pc-windows-msvc", output="release")
        for name in assets.expected_assets(TAG):
            if not name.endswith(".zip"):
                (directory / name).write_text("final archive/package fixture", encoding="utf-8")
        for name in ("install.sh", "install.ps1", "uninstall.ps1"):
            (self.root / name).write_text("installer fixture", encoding="utf-8")
        sums = "".join(f"{assets.sha(directory / name)}  {name}\n" for name in sorted(assets.expected_assets(TAG)))
        (directory / "SHA256SUMS.txt").write_text(sums, encoding="utf-8")
        hashes = assets.validate_inputs(TAG, directory, self.root / "install.sh",
                                       windows_installer=self.root / "install.ps1",
                                       windows_uninstaller=self.root / "uninstall.ps1")
        self.assertEqual(len(hashes), 14)
        release = {"assets": [{"name": name, "digest": "sha256:" + digest} for name, digest in hashes.items()]}
        assets.verify_existing(release, hashes)
        release["assets"][0]["digest"] = "sha256:" + "0" * 64
        with self.assertRaises(ValueError):
            assets.verify_existing(release, hashes)
        (directory / "unexpected").write_text("x")
        with self.assertRaises(ValueError):
            assets.validate_inputs(TAG, directory, self.root / "install.sh")


if __name__ == "__main__":
    unittest.main()
