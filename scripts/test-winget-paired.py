#!/usr/bin/env python3
"""Offline coverage for static paired WinGet generation and retained native gates."""
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import windows_release as windows

spec = importlib.util.spec_from_file_location("winget", Path(__file__).with_name("render-winget-manifest.py"))
winget = importlib.util.module_from_spec(spec)
spec.loader.exec_module(winget)
TAG = "v0.10.0"


def pe(machine, subsystem):
    binary = bytearray(512)
    binary[:2] = b"MZ"
    struct.pack_into("<I", binary, 60, 128)
    binary[128:132] = b"PE\0\0"
    for offset, value in ((132, machine), (134, 1), (148, 240), (150, 0x22), (152, 0x20B), (220, subsystem)):
        struct.pack_into("<H", binary, offset, value)
    struct.pack_into("<I", binary, 260, 16)
    return bytes(binary)


class PairedManifests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="locron-winget-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.inputs = self.root / "inputs"
        self.inputs.mkdir()
        self.output = self.root / "manifests"
        self.write_inputs()

    def write_inputs(self, paired=True):
        for name in winget.assets.expected_assets(TAG):
            (self.inputs / name).write_bytes(b"unrelated release fixture")
        for target, machine in windows.TARGETS.items():
            files = windows.PAIRED_FILES if paired else windows.FILES
            contents = {name: pe(machine, windows.SUBSYSTEMS[name]) if name in windows.SUBSYSTEMS
                        else b"fixture documentation" for name in files}
            archive = self.inputs / f"locron-{TAG}-{target}.zip"
            archive.unlink()
            windows.write_archive(archive, TAG, target, contents, files)
        self.write_sums()

    def write_sums(self):
        (self.inputs / "SHA256SUMS.txt").write_text("".join(
            f"{winget.assets.sha(self.inputs / name)}  {name}\n"
            for name in sorted(winget.assets.expected_assets(TAG))), encoding="utf-8")

    def archive(self, target="x86_64-pc-windows-msvc"):
        return self.inputs / f"locron-{TAG}-{target}.zip"

    def rewrite(self, change):
        archive = self.archive()
        with zipfile.ZipFile(archive) as source:
            contents = {info.filename: source.read(info) for info in source.infolist()}
        change(contents)
        with zipfile.ZipFile(archive, "w") as destination:
            for name, content in contents.items():
                destination.writestr(name, content)
        self.write_sums()

    def test_both_architectures_render_without_execution_or_extraction(self):
        with patch.object(windows, "probe_pair", side_effect=AssertionError("native execution")), \
                patch.object(windows.tempfile, "TemporaryDirectory", side_effect=AssertionError("extraction")):
            winget.render(TAG, self.inputs, self.output, paired=True)
        installer = (self.output / "WhiteKiwi.locron.installer.yaml").read_text()
        self.assertEqual(installer.count("PortableCommandAlias: locron\n"), 2)
        self.assertEqual(installer.count("RelativeFilePath:"), 2)
        self.assertNotIn("locron-service-launcher", installer)
        self.assertNotIn("Scope:", installer)
        self.assertNotIn("RequireExplicitUpgrade", installer)
        for target in windows.TARGETS:
            name = self.archive(target).name
            self.assertIn(f"{winget.SOURCE}/releases/download/{TAG}/{name}", installer)
            self.assertIn(winget.assets.sha(self.archive(target)).upper(), installer)
            self.assertIn(f"locron-{TAG}-{target}/locron.exe", installer)
        self.assertIn("- Architecture: x64\n", installer)
        self.assertIn("- Architecture: arm64\n", installer)
        self.assertEqual(len(list(self.output.iterdir())), 3)

    def test_static_facts_do_not_claim_runtime_abi_or_version_probes(self):
        facts = windows.inspect_archive(self.archive(), TAG, "x86_64-pc-windows-msvc", paired=True)
        self.assertEqual(facts["mode"], "paired-static")
        self.assertEqual(facts["archive_sha256"], winget.assets.sha(self.archive()))
        self.assertEqual(set(facts["binaries"]), set(windows.SUBSYSTEMS))
        self.assertTrue({"launcher_abi", "launcher_identity", "version_probes"}.isdisjoint(facts))

    def test_native_validation_still_requires_all_three_runtime_probes(self):
        calls = []
        def execute(command, **kwargs):
            name, argument = Path(command[0]).name, command[1]
            self.assertEqual(kwargs["timeout"], 30)
            self.assertEqual(Path(command[0]).read_bytes(), pe(0x8664, windows.SUBSYSTEMS[name]))
            calls.append((name, argument))
            if argument == "--version":
                output = f"{name.removesuffix('.exe')} 0.10.0\n".encode()
            else:
                output = json.dumps({"schema": "locron.windows-launcher-probe/v1", "version": "0.10.0",
                                     "target": "x86_64-pc-windows-msvc", "launcher_abi": "native-gui-v1",
                                     "initial_conout_opened": False, "initial_conout_error": 2}).encode()
            return subprocess.CompletedProcess(command, 0, output, b"")
        facts = windows.validate_archive(self.archive(), TAG, "x86_64-pc-windows-msvc",
                                         paired=True, execute=execute)
        self.assertEqual(calls, [("locron.exe", "--version"), ("locron-service-launcher.exe", "--version"),
                                 ("locron-service-launcher.exe", "--identity-probe")])
        self.assertEqual(facts["mode"], "paired-draft")
        self.assertEqual(facts["launcher_abi"], "native-gui-v1")

    def test_native_probe_failure_is_not_downgraded_to_static_success(self):
        def execute(*args, **kwargs):
            raise subprocess.TimeoutExpired(args[0], kwargs["timeout"])
        with self.assertRaises(subprocess.TimeoutExpired):
            windows.validate_archive(self.archive(), TAG, "x86_64-pc-windows-msvc", paired=True, execute=execute)

    def test_legacy_default_and_explicit_paired_inventory_remain_distinct(self):
        with self.assertRaises(ValueError):
            winget.render(TAG, self.inputs, self.output)
        self.assertFalse(self.output.exists())
        self.write_inputs(paired=False)
        with self.assertRaises(ValueError):
            winget.render(TAG, self.inputs, self.output, paired=True)
        self.assertFalse(self.output.exists())
        winget.render(TAG, self.inputs, self.output)
        self.assertEqual(len(list(self.output.iterdir())), 3)
        facts = windows.validate_archive(self.archive(), TAG, "x86_64-pc-windows-msvc")
        self.assertEqual(set(facts), {"version", "target", "unsigned", "binary_sha256", "imports"})

    def test_checksum_mismatch_refuses_before_any_zip_interpretation(self):
        self.archive().write_bytes(b"tampered, not a ZIP")
        with patch.object(windows.zipfile, "ZipFile", side_effect=AssertionError("ZIP interpreted before digest")):
            with self.assertRaisesRegex(ValueError, "final release checksum"):
                winget.render(TAG, self.inputs, self.output, paired=True)
        self.assertFalse(self.output.exists())

    def test_snapshot_digest_and_inspected_contents_are_from_the_same_read(self):
        path = self.archive()
        original = path.read_bytes()
        real_zip = windows.zipfile.ZipFile
        def open_zip(source, *args, **kwargs):
            path.write_bytes(b"changed after bounded read")
            return real_zip(source, *args, **kwargs)
        with patch.object(windows.zipfile, "ZipFile", side_effect=open_zip):
            facts = windows.inspect_archive(path, TAG, "x86_64-pc-windows-msvc", paired=True,
                                             expected_sha256=hashlib.sha256(original).hexdigest())
        self.assertEqual(facts["archive_sha256"], hashlib.sha256(original).hexdigest())
        self.assertNotEqual(facts["archive_sha256"], winget.assets.sha(path))

    def test_missing_or_extra_member_is_rejected_before_output(self):
        for extra in (False, True):
            with self.subTest(extra=extra):
                self.write_inputs()
                def change(contents):
                    root = f"locron-{TAG}-x86_64-pc-windows-msvc/"
                    if extra:
                        contents[root + "extra.dll"] = b"extra"
                    else:
                        del contents[root + "locron-service-launcher.exe"]
                self.rewrite(change)
                with self.assertRaises(ValueError):
                    winget.render(TAG, self.inputs, self.output, paired=True)
                self.assertFalse(self.output.exists())

    def test_wrong_architecture_and_subsystem_are_rejected(self):
        for machine, subsystem in ((0xAA64, 2), (0x8664, 3)):
            with self.subTest(machine=machine, subsystem=subsystem):
                self.write_inputs()
                self.rewrite(lambda contents: contents.update({
                    f"locron-{TAG}-x86_64-pc-windows-msvc/locron-service-launcher.exe": pe(machine, subsystem)}))
                with self.assertRaises(ValueError):
                    winget.render(TAG, self.inputs, self.output, paired=True)
                self.assertFalse(self.output.exists())

    def test_checksum_inventory_rejects_missing_extra_duplicate_and_oversize(self):
        path = self.inputs / "SHA256SUMS.txt"
        original = path.read_text()
        for text in ("", original.splitlines()[0] + "\n", original + original.splitlines()[0] + "\n",
                     original + "00" * 32 + "  extra.zip\n", "x" * (winget.MAX_CHECKSUM_BYTES + 1)):
            with self.subTest(length=len(text)):
                path.write_text(text)
                with self.assertRaises(ValueError):
                    winget.render(TAG, self.inputs, self.output, paired=True)
                self.assertFalse(self.output.exists())

    def test_missing_or_symlinked_checksum_file_is_rejected(self):
        path = self.inputs / "SHA256SUMS.txt"
        target = self.inputs / "saved-sums"
        path.rename(target)
        with self.assertRaises(ValueError):
            winget.render(TAG, self.inputs, self.output, paired=True)
        try:
            path.symlink_to(target)
        except OSError:
            self.skipTest("host does not allow fixture symlinks")
        with self.assertRaises(ValueError):
            winget.render(TAG, self.inputs, self.output, paired=True)
        self.assertFalse(self.output.exists())

    def test_existing_output_is_never_overwritten(self):
        self.output.mkdir()
        marker = self.output / "preserved"
        marker.write_bytes(b"keep me")
        with self.assertRaises(FileExistsError):
            winget.render(TAG, self.inputs, self.output, paired=True)
        self.assertEqual(list(self.output.iterdir()), [marker])
        self.assertEqual(marker.read_bytes(), b"keep me")

    def test_unsupported_and_malformed_versions_are_rejected_before_output(self):
        for tag in ("v0.9.6", "v0.10.0-preview", "../v0.10.0", "0.10.0", "v00.10.0"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                winget.render(tag, self.inputs, self.output, paired=True)
        self.assertFalse(self.output.exists())

    def test_cli_paired_mode_generates_documents_on_a_non_windows_host(self):
        result = subprocess.run([sys.executable, "-B", str(Path(winget.__file__)), TAG,
                                 str(self.inputs), str(self.output), "--paired"],
                                capture_output=True, text=True, timeout=10, check=True)
        self.assertEqual(result.stdout.strip(), str(self.output))
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()
