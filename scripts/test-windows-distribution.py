#!/usr/bin/env python3
"""Hermetic Windows distribution boundaries; no installation, tasks or publication."""
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

import windows_release as windows

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("release_assets", ROOT / "scripts/release-assets.py")
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)
spec = importlib.util.spec_from_file_location("winget_manifest", ROOT / "scripts/render-winget-manifest.py")
winget_manifest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(winget_manifest)
WINGET_VALIDATE = "--winget-validate" in sys.argv
if WINGET_VALIDATE:
    sys.argv.remove("--winget-validate")
TAG = "v0.10.0"
TARGET = "x86_64-pc-windows-msvc"


def executable(machine=0x8664, signed=False, subsystem=3):
    binary = bytearray(512)
    binary[:2] = b"MZ"
    struct.pack_into("<I", binary, 60, 128)
    binary[128:132] = b"PE\0\0"
    struct.pack_into("<H", binary, 132, machine)
    struct.pack_into("<H", binary, 134, 1)
    struct.pack_into("<H", binary, 148, 240)
    struct.pack_into("<H", binary, 150, 0x22)
    struct.pack_into("<H", binary, 152, 0x20B)
    struct.pack_into("<H", binary, 220, subsystem)
    struct.pack_into("<I", binary, 260, 16)
    if signed:
        struct.pack_into("<II", binary, 296, 480, 32)
    return bytes(binary)


def imported_executable(dll, delayed=False, subsystem=3):
    binary = bytearray(executable(subsystem=subsystem)) + bytearray(512)
    # One raw-backed section, with an import descriptor and a terminated DLL name.
    struct.pack_into("<IIII", binary, 400, 512, 0x1000, 512, 512)
    if delayed:
        struct.pack_into("<II", binary, 368, 0x1000, 64)
        struct.pack_into("<II", binary, 512, 1, 0x1080)
    else:
        struct.pack_into("<II", binary, 272, 0x1000, 40)
        struct.pack_into("<I", binary, 524, 0x1080)
    name = dll.encode("ascii") + b"\0"
    binary[640:640 + len(name)] = name
    return bytes(binary)


class WindowsDistributionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binary = self.root / "locron.exe"
        self.binary.write_bytes(executable())
        self.launcher = self.root / "locron-service-launcher.exe"
        self.launcher.write_bytes(executable(subsystem=2))
        self.probes = []
        for name in windows.FILES[1:]:
            (self.root / name).write_text("fixture documentation", encoding="utf-8")

    def package(self, target=TARGET, output="output", reported="locron 0.10.0\n"):
        return windows.package(TAG, target, self.binary, self.root / output, source=self.root,
                               execute=lambda *args, **kwargs: SimpleNamespace(stdout=reported))

    def pair_execute(self, argv, **options):
        image = Path(argv[0])
        self.assertNotEqual(image.parent, self.root)
        self.assertEqual(image.parent, options["cwd"])
        self.assertEqual({path.name for path in image.parent.iterdir()}, set(windows.PAIRED_FILES))
        self.assertEqual(options["env"]["PATH"], windows.system_environment()["PATH"])
        self.assertEqual(options["timeout"], 30)
        self.assertEqual(options["stdin"], subprocess.DEVNULL)
        self.assertIs(options["capture_output"], True)
        self.assertIs(options["check"], True)
        self.probes.append((image.name, argv[1], image.read_bytes()))
        target = next(target for target, machine in windows.TARGETS.items()
                      if machine == windows.pe_machine(image.read_bytes()))
        identity = {"schema": "locron.windows-launcher-probe/v1", "version": "0.10.0",
                    "target": target, "launcher_abi": "native-gui-v1",
                    "initial_conout_opened": False, "initial_conout_error": 2}
        output = (json.dumps(identity) if argv[1] == "--identity-probe"
                  else image.stem + " 0.10.0\n")
        return SimpleNamespace(stdout=output.encode(), stderr=b"")

    def paired_package(self, target=TARGET, output="paired", execute=None):
        return windows.package(TAG, target, self.binary, self.root / output, source=self.root,
                               execute=execute or self.pair_execute, launcher=self.launcher)

    def test_paired_archive_probes_actual_bytes_for_both_native_targets(self):
        for target, machine in windows.TARGETS.items():
            with self.subTest(target=target):
                self.binary.write_bytes(executable(machine))
                self.launcher.write_bytes(executable(machine, subsystem=2))
                archive = self.paired_package(target, output=target)
                # Validation must use final ZIP bytes even after the build outputs change.
                self.binary.write_bytes(b"replaced source")
                self.launcher.write_bytes(b"replaced source")
                self.probes.clear()
                facts = windows.validate_archive(archive, TAG, target, paired=True, execute=self.pair_execute)
                self.assertEqual([(name, arg) for name, arg, _ in self.probes],
                                 [("locron.exe", "--version"), ("locron-service-launcher.exe", "--version"),
                                  ("locron-service-launcher.exe", "--identity-probe")])
                self.assertEqual(facts["mode"], "paired-draft")
                self.assertEqual(facts["launcher_identity"]["target"], target)
                self.assertEqual(facts["launcher_abi"], "native-gui-v1")
                self.assertEqual(facts["version_probes"], {"locron.exe": "locron 0.10.0\n",
                                                        "locron-service-launcher.exe": "locron-service-launcher 0.10.0\n"})
                with zipfile.ZipFile(archive) as stream:
                    self.assertEqual(len(stream.infolist()), 5)
                    for name, subsystem in windows.SUBSYSTEMS.items():
                        data = stream.read(f"locron-{TAG}-{target}/{name}")
                        self.assertEqual(facts["binaries"][name],
                                         {"sha256": hashlib.sha256(data).hexdigest(),
                                          "subsystem": subsystem, "imports": []})
                        self.assertIn(data, [content for _, _, content in self.probes])
                with self.assertRaises(ValueError):
                    windows.validate_archive(archive, TAG, target)  # Legacy consumers cannot adopt pairs.

    def test_paired_invalid_binary_refuses_before_any_probe_or_artifact(self):
        for name, subsystem in windows.SUBSYSTEMS.items():
            path = self.root / name
            valid = path.read_bytes()
            invalid = [b"corrupt", executable(0xAA64, subsystem=subsystem),
                       executable(signed=True, subsystem=subsystem), executable(subsystem=5 - subsystem)]
            invalid += [imported_executable("VCRUNTIME140.dll", delayed, subsystem) for delayed in (False, True)]
            for data in invalid:
                with self.subTest(name=name, data=data[:4]):
                    path.write_bytes(data)
                    with self.assertRaises(ValueError):
                        self.paired_package()
                    self.assertFalse(self.probes)
                    self.assertFalse((self.root / "paired").exists())
            path.write_bytes(valid)
        self.launcher.unlink()
        with self.assertRaises(ValueError):
            self.paired_package()
        self.assertFalse(self.probes)
        self.assertFalse((self.root / "paired").exists())

    def test_paired_inventory_and_aggregate_limits_refuse_before_probes(self):
        legacy = self.package()
        with self.assertRaises(ValueError):
            windows.validate_archive(legacy, TAG, TARGET, paired=True, execute=self.pair_execute)
        archive = self.paired_package()
        with zipfile.ZipFile(archive) as stream:
            members = [(member, stream.read(member)) for member in stream.infolist()]
        variants = [members[:-1], members + [members[0]],
                    members + [(f"locron-{TAG}-{TARGET}/extra", b"extra")]]
        for index, members_changed in enumerate(variants):
            candidate = self.root / f"invalid-{index}.zip"
            with zipfile.ZipFile(candidate, "w") as stream:
                for member, data in members_changed:
                    stream.writestr(member, data)
            self.probes.clear()
            with self.assertRaises(ValueError):
                windows.validate_archive(candidate, TAG, TARGET, paired=True, execute=self.pair_execute)
            self.assertFalse(self.probes)
        with patch.object(windows, "MAX_ARCHIVE_BYTES", 1023):
            with self.assertRaises(ValueError):
                self.paired_package(output="oversized")
        self.assertFalse((self.root / "oversized").exists())
        self.assertFalse(self.probes)
        # Compressible documentation isolates expanded-size refusal from the archive-byte limit.
        (self.root / "README.md").write_bytes(b"x" * 4096)
        large = self.paired_package(output="large")
        self.probes.clear()
        with patch.object(windows, "MAX_ARCHIVE_BYTES", large.stat().st_size):
            with self.assertRaises(ValueError):
                windows.validate_archive(large, TAG, TARGET, paired=True, execute=self.pair_execute)
        self.assertFalse(self.probes)

    def test_paired_strict_identity_and_versions_leave_no_failed_artifact(self):
        identity = {"schema": "locron.windows-launcher-probe/v1", "version": "0.10.0",
                    "target": TARGET, "launcher_abi": "native-gui-v1",
                    "initial_conout_opened": False, "initial_conout_error": 2}
        invalid = [{key: value for key, value in identity.items() if key != missing} for missing in identity]
        invalid += [dict(identity, **{key: value}) for key, value in (
            ("schema", "unknown"), ("version", "0.9.6"), ("target", "aarch64-pc-windows-msvc"),
            ("launcher_abi", "unknown"), ("extra", 0), ("initial_conout_opened", 0),
            ("initial_conout_opened", True), ("initial_conout_error", None), ("initial_conout_error", True),
            ("initial_conout_error", 0), ("initial_conout_error", 2.0),
            ("initial_conout_error", 2**31), ("initial_conout_error", -(2**31) - 1))]
        encoded = json.dumps(identity).encode()
        bad_outputs = [json.dumps(value).encode() for value in invalid]
        bad_outputs += [b" " + encoded, encoded + b"\n", encoded + b"{}", b"[{}]", b"\xff",
                        encoded.replace(b'"schema":', b'"schema":"duplicate","schema":')]
        for output in bad_outputs:
            def execute(argv, **options):
                result = self.pair_execute(argv, **options)
                return SimpleNamespace(stdout=output, stderr=b"") if argv[1] == "--identity-probe" else result
            with self.subTest(output=output[:60]), self.assertRaises(ValueError):
                self.paired_package(execute=execute)
            self.assertFalse((self.root / "paired").exists())
        for name in windows.SUBSYSTEMS:
            def wrong_version(argv, **options):
                result = self.pair_execute(argv, **options)
                wrong = (Path(argv[0]).stem + " 0.9.6\n").encode()
                return SimpleNamespace(stdout=wrong, stderr=b"") if Path(argv[0]).name == name else result
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.paired_package(execute=wrong_version)
            self.assertFalse((self.root / "paired").exists())
        # Raw error codes remain facts; only the separate native controls qualify headless behavior.
        for opened, error in ((True, None), (False, -1), (False, 3)):
            facts = dict(identity, initial_conout_opened=opened, initial_conout_error=error)
            self.assertEqual(windows.launcher_identity(json.dumps(facts), "0.10.0", TARGET), facts)

    def test_paired_raw_nul_name_refuses_before_probes(self):
        archive = self.paired_package()
        candidate = self.root / "raw-name.zip"
        with zipfile.ZipFile(archive) as original, zipfile.ZipFile(candidate, "w") as stream:
            for member in original.infolist():
                name = member.filename + "Xsuffix" if member.filename.endswith("README.md") else member.filename
                stream.writestr(name, original.read(member))
        data = candidate.read_bytes()
        self.assertEqual(data.count(b"README.mdXsuffix"), 2)  # Local and central directory names.
        candidate.write_bytes(data.replace(b"README.mdXsuffix", b"README.md\0suffix"))
        self.probes.clear()
        with self.assertRaises(ValueError):
            windows.validate_archive(candidate, TAG, TARGET, paired=True, execute=self.pair_execute)
        self.assertFalse(self.probes)

    def test_paired_final_copy_failures_remove_only_the_new_artifact(self):
        read_bytes, open_file = Path.read_bytes, Path.open

        def failed_read(path):
            if path.parent.name.startswith("locron-paired-package-"):
                raise OSError("copy fixture read failure")
            return read_bytes(path)

        with patch.object(Path, "read_bytes", failed_read), self.assertRaises(OSError):
            self.paired_package(output="failed-read")
        self.assertFalse((self.root / "failed-read").exists())

        class BrokenOutput:
            def __init__(self, stream, stage):
                self.stream, self.stage = stream, stage

            def __enter__(self):
                return self

            def write(self, data):
                if self.stage == "write":
                    self.stream.write(data[:16])
                    raise OSError("copy fixture write failure")
                return self.stream.write(data)

            def __exit__(self, *args):
                self.stream.__exit__(*args)
                if self.stage == "close":
                    raise OSError("copy fixture close failure")

        for stage in ("write", "close"):
            directory = self.root / stage

            def failed_open(path, *args, **kwargs):
                stream = open_file(path, *args, **kwargs)
                return BrokenOutput(stream, stage) if path.parent == directory and args == ("xb",) else stream

            with patch.object(Path, "open", failed_open), self.assertRaisesRegex(OSError, "copy fixture"):
                self.paired_package(output=stage)
            self.assertEqual(list(directory.iterdir()), [])

    def test_paired_failed_probes_and_existing_output_are_not_published(self):
        failures = [ValueError("fixture failure"), subprocess.TimeoutExpired("probe", 30),
                    subprocess.CalledProcessError(70, "probe"),
                    SimpleNamespace(stdout=b"x" * (windows.MAX_PROBE_BYTES + 1), stderr=b""),
                    SimpleNamespace(stdout=b"locron 0.10.0\n", stderr=b"warning")]
        for failure in failures:
            def execute(argv, **options):
                self.pair_execute(argv, **options)
                if isinstance(failure, Exception):
                    raise failure
                return failure
            with self.subTest(failure=str(failure)[:50]), self.assertRaises((ValueError, subprocess.SubprocessError)):
                self.paired_package(execute=execute)
            self.assertFalse((self.root / "paired").exists())
        archive = self.paired_package()
        original = archive.read_bytes()
        with self.assertRaises(FileExistsError):
            self.paired_package()
        self.assertEqual(archive.read_bytes(), original)

    def test_paired_cli_requires_explicit_correct_mode(self):
        for mode, flags in (("package", ["--paired"]), ("validate", ["--launcher", str(self.launcher)])):
            result = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/windows_release.py"),
                                     mode, TAG, TARGET, str(self.binary), *flags],
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 2)
            self.assertIn("--launcher is package-only; --paired is validate-only", result.stderr)
            self.assertFalse(result.stdout)

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
        self.binary.write_bytes(executable())
        self.launcher.write_bytes(executable(subsystem=2))
        self.paired_package(output="release")
        self.binary.write_bytes(executable(0xAA64))
        self.launcher.write_bytes(executable(0xAA64, subsystem=2))
        self.paired_package("aarch64-pc-windows-msvc", output="release")
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
        # The public names stay unchanged, but v0.10+ publication refuses the
        # historical console-only ZIP even when it is otherwise well-formed.
        x64_name = f"locron-{TAG}-{TARGET}.zip"
        paired_x64 = (directory / x64_name).read_bytes()
        self.binary.write_bytes(executable())
        legacy = self.package(output="legacy-release")
        (directory / x64_name).write_bytes(legacy.read_bytes())
        with self.assertRaises(ValueError):
            assets.validate_inputs(TAG, directory, self.root / "install.sh",
                                  windows_installer=self.root / "install.ps1",
                                  windows_uninstaller=self.root / "uninstall.ps1")
        (directory / x64_name).write_bytes(paired_x64)
        release = {"assets": [{"name": name, "digest": "sha256:" + digest} for name, digest in hashes.items()]}
        assets.verify_existing(release, hashes)
        release["assets"][0]["digest"] = "sha256:" + "0" * 64
        with self.assertRaises(ValueError):
            assets.verify_existing(release, hashes)
        (directory / "unexpected").write_text("x")
        with self.assertRaises(ValueError):
            assets.validate_inputs(TAG, directory, self.root / "install.sh")

    def test_normal_and_delayed_dependencies_refuse_non_stock_runtime(self):
        for delayed in (False, True):
            self.assertEqual(windows.pe_imports(imported_executable("KERNEL32.dll", delayed)), ["kernel32.dll"])
            synchronization = "api-ms-win-core-synch-l1-2-0.dll"
            self.assertEqual(windows.pe_imports(imported_executable(synchronization, delayed)), [synchronization])
            for dll in ("VCRUNTIME140.dll", "VCRUNTIME140_1.dll", "MSVCP140.dll", "ucrtbased.dll",
                        "libssl-3.dll", "private.dll", "../kernel32.dll", "api-ms-win-core-synch-l1-3-0.dll",
                        "api-ms-win-private-unknown-l1-1-0.dll", "api-ms-win-crt-runtime-l1-1-0.dll"):
                with self.subTest(delayed=delayed, dll=dll), self.assertRaises(ValueError):
                    windows.pe_imports(imported_executable(dll, delayed))
        binary = bytearray(imported_executable("kernel32.dll"))
        struct.pack_into("<I", binary, 524, 0x9000)
        with self.assertRaises(ValueError):
            windows.pe_imports(binary)
        binary = bytearray(imported_executable("kernel32.dll", True))
        struct.pack_into("<I", binary, 512, 0)
        with self.assertRaises(ValueError):
            windows.pe_imports(binary)
        self.binary.write_bytes(imported_executable("MSVCP140.dll"))
        with self.assertRaises(ValueError):
            self.package(output="redistributable")

    def test_child_version_check_removes_developer_path(self):
        with patch.dict(windows.os.environ, {"SystemRoot": r"C:\Windows", "PATH": "compiler-and-sdk-path"}):
            environment = windows.system_environment()
            self.assertNotIn("compiler-and-sdk-path", environment["PATH"])
            self.assertEqual(windows.os.environ["PATH"], "compiler-and-sdk-path")

    def test_validation_cli_produces_machine_readable_package_facts(self):
        archive = self.package(output="cli-json")
        result = subprocess.run([sys.executable, "-B", str(ROOT / "scripts/windows_release.py"),
                                 "validate", TAG, TARGET, str(archive)], capture_output=True,
                                text=True, check=True, timeout=30)
        facts = json.loads(result.stdout)
        self.assertEqual(facts["target"], TARGET)
        self.assertIs(facts["unsigned"], True)
        self.assertEqual(facts["version"], "0.10.0")
        self.assertEqual(facts["binary_sha256"], hashlib.sha256(self.binary.read_bytes()).hexdigest())

    def test_winget_manifests_bind_final_architecture_paths_and_zip_bytes(self):
        self.package(output="winget-inputs")
        self.binary.write_bytes(executable(0xAA64))
        self.package("aarch64-pc-windows-msvc", output="winget-inputs")
        directory = self.root / "winget-inputs"
        sums = {name: assets.sha(directory / name) if name.endswith(".zip") else "ab" * 32
                for name in assets.expected_assets(TAG)}
        checksum_file = directory / "SHA256SUMS.txt"
        checksum_file.write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(sums.items())), encoding="utf-8")
        output = winget_manifest.render(TAG, directory, self.root / "manifests")
        self.assertEqual(len(list(output.glob("*.yaml"))), 3)
        installer = (output / "WhiteKiwi.locron.installer.yaml").read_text(encoding="utf-8")
        self.assertNotIn("Scope:", installer)  # Portable scope comes from the documented CLI flag.
        self.assertIn("MinimumOSVersion: 10.0.22000.0\n", installer)
        for target in windows.TARGETS:
            name = f"locron-{TAG}-{target}.zip"
            self.assertIn(f"InstallerSha256: {sums[name].upper()}", installer)
            self.assertIn(f"InstallerUrl: https://github.com/WhiteKiwi/locron/releases/download/{TAG}/{name}", installer)
            self.assertIn(f"RelativeFilePath: locron-{TAG}-{target}/locron.exe", installer)
        if WINGET_VALIDATE:
            subprocess.run(["winget", "validate", "--manifest", str(output), "--disable-interactivity"],
                           check=True, timeout=120)
        with self.assertRaises(FileExistsError):
            winget_manifest.render(TAG, directory, output)
        with self.assertRaises(ValueError):
            winget_manifest.render("v0.9.6", directory, self.root / "old-manifests")
        sums[f"locron-{TAG}-{TARGET}.zip"] = "00" * 32
        checksum_file.write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(sums.items())), encoding="utf-8")
        with self.assertRaises(ValueError):
            winget_manifest.render(TAG, directory, self.root / "bad-manifests")
        self.assertFalse((self.root / "bad-manifests").exists())


if __name__ == "__main__":
    unittest.main()
