#!/usr/bin/env python3
"""Public WinGet CLI defaults; real ZIP inspection, never native image execution."""
import contextlib
import importlib.util
import io
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import windows_release as windows

spec = importlib.util.spec_from_file_location("winget_entrypoint", Path(__file__).with_name("render-winget-manifest.py"))
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


class Entrypoint(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="locron-winget-entrypoint-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.inputs = self.root / "final inputs 한글"
        self.inputs.mkdir()
        self.write_inputs()

    def write_inputs(self, paired=True):
        for name in winget.assets.expected_assets(TAG):
            path = self.inputs / name
            if path.exists():
                path.unlink()
            if not name.endswith(".zip"):
                path.write_bytes(b"other publication fixture")
        files = windows.PAIRED_FILES if paired else windows.FILES
        for target, machine in windows.TARGETS.items():
            contents = {name: pe(machine, windows.SUBSYSTEMS[name]) if name in windows.SUBSYSTEMS
                        else b"fixture documentation" for name in files}
            windows.write_archive(self.inputs / f"locron-{TAG}-{target}.zip", TAG, target, contents, files)
        self.write_sums()

    def write_sums(self):
        (self.inputs / "SHA256SUMS.txt").write_text("".join(
            f"{winget.assets.sha(self.inputs / name)}  {name}\n"
            for name in sorted(winget.assets.expected_assets(TAG))), encoding="utf-8")

    def cli(self, output, *flags):
        return subprocess.run([sys.executable, "-B", str(Path(winget.__file__)), TAG,
                               str(self.inputs), str(output), *flags],
                              capture_output=True, text=True, timeout=10)

    def main(self, output, *flags):
        arguments = [winget.__file__, TAG, str(self.inputs), str(output), *flags]
        with patch.object(sys, "argv", arguments), contextlib.redirect_stdout(io.StringIO()) as printed:
            winget.main()
        return printed.getvalue()

    def test_documented_default_and_compatibility_flag_generate_identical_manifests(self):
        results = []
        for index, flags in enumerate(((), ("--paired",))):
            output = self.root / f"output {index}"
            result = self.cli(output, *flags)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), str(output))
            self.assertEqual(result.stderr, "")
            results.append({path.name: path.read_bytes() for path in output.iterdir()})
        self.assertEqual(results[0], results[1])
        self.assertEqual(len(results[0]), 3)
        installer = results[0]["WhiteKiwi.locron.installer.yaml"].decode()
        self.assertEqual(installer.count("PortableCommandAlias: locron\n"), 2)
        self.assertNotIn("locron-service-launcher", installer)
        self.assertNotIn("Scope:", installer)
        self.assertNotIn("RequireExplicitUpgrade", installer)

    def test_both_cli_spellings_refuse_legacy_zip_before_output(self):
        self.write_inputs(paired=False)
        for index, flags in enumerate(((), ("--paired",))):
            output = self.root / f"refused-{index}"
            result = self.cli(output, *flags)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())
            self.assertEqual(result.stdout, "")

    def test_validate_runs_only_structural_validator_after_paired_generation(self):
        for index, flags in enumerate((("--validate",), ("--validate", "--paired"))):
            output = self.root / f"validated-{index}"
            with patch.object(winget.subprocess, "run") as execute, \
                    patch.object(windows, "probe_pair", side_effect=AssertionError("Windows image executed")):
                self.assertEqual(self.main(output, *flags).strip(), str(output))
                execute.assert_called_once_with(
                    ["winget", "validate", "--manifest", str(output.resolve()), "--disable-interactivity"],
                    check=True, timeout=120)
            self.assertEqual(len(list(output.iterdir())), 3)

    def test_default_generation_does_not_invoke_validator_or_images(self):
        with patch.object(winget.subprocess, "run") as execute, \
                patch.object(windows, "probe_pair", side_effect=AssertionError("Windows image executed")):
            self.main(self.root / "static-only")
            execute.assert_not_called()

    def test_invalid_input_never_reaches_validator(self):
        self.write_inputs(paired=False)
        with patch.object(winget.subprocess, "run") as execute:
            with self.assertRaises(ValueError):
                self.main(self.root / "invalid", "--validate")
            execute.assert_not_called()
        self.assertFalse((self.root / "invalid").exists())

    def test_failed_validator_propagates_without_printing_success(self):
        output = self.root / "validator-fails"
        argv = [winget.__file__, TAG, str(self.inputs), str(output), "--validate"]
        with patch.object(sys, "argv", argv), contextlib.redirect_stdout(io.StringIO()) as printed, \
                patch.object(winget.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "winget")):
            with self.assertRaises(subprocess.CalledProcessError):
                winget.main()
        self.assertEqual(printed.getvalue(), "")
        self.assertEqual(len(list(output.iterdir())), 3)  # Keep reviewable files on validation failure.

    def test_existing_output_is_preserved(self):
        output = self.root / "existing"
        output.mkdir()
        marker = output / "keep"
        marker.write_bytes(b"existing bytes")
        result = self.cli(output)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual({path.name: path.read_bytes() for path in output.iterdir()}, {"keep": b"existing bytes"})

    def test_internal_explicit_legacy_fixture_route_is_unchanged(self):
        self.write_inputs(paired=False)
        output = self.root / "legacy-internal"
        winget.render(TAG, self.inputs, output, paired=False)
        self.assertEqual(len(list(output.iterdir())), 3)


if __name__ == "__main__":
    unittest.main()
