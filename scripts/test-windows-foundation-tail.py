#!/usr/bin/env python3
"""Portable controls for Windows tail provenance and upload-only packaging."""

import copy
import hashlib
import json
import os
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

import ci_windows_foundation_tail as tail


HOST = "x86_64-unknown-linux-gnu"


def binary(path, label="fixture"):
    value = bytearray(128)
    value[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<H", value, 18, 62)
    value.extend(label.encode("ascii"))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(value)
    return path


class TailTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="locron-native-tail-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.target = self.root / "recovery"
        self.target.mkdir()
        self.global_target = self.root / "target"
        self.core = binary(self.global_target / "debug/deps/locron_core-cold.exe")
        self.revision = {"head": "a" * 40, "tree": "b" * 40}
        self.original = {
            "revision": self.revision,
            "checkout": {"checkout": "c" * 40},
            "test_target": str(self.target),
        }
        self.production = binary(self.root / "production/debug/locron.exe", "production")
        self.recovery = binary(self.target / "debug/deps/locron-recovery.exe", "recovery")
        prod, rec = tail.executable(self.production, HOST), tail.executable(self.recovery, HOST)
        binding = {"version": 1, "revision": self.revision,
                   "production": dict(prod["facts"], profile_test=False),
                   "recovery": dict(rec["facts"], profile_test=True)}
        self.locator = self.production.parent.parent / "prune-artifact-binding.json"
        self.locator.write_text(json.dumps(binding), encoding="utf-8")
        _, _, binding_id, binding_sha = tail.ARTIFACTS.binding_json_facts(self.production, self.recovery, HOST, self.revision)
        self.original.update({"production_artifact": {"executable": str(self.production)},
                              "recovery_artifact": {"executable": str(self.recovery), "features": [], "profile": tail.PROFILE},
                              "production_identity": prod["identity"], "recovery_identity": rec["identity"],
                              "binding_identity": binding_id, "binding_sha256": binding_sha})
        self.message = {"reason": "compiler-artifact", "package_id": "core-package",
                        "target": {"name": "locron_core", "kind": ["lib"]},
                        "features": [], "profile": tail.PROFILE, "fresh": True,
                        "executable": str(self.core)}
        self.metadata = {"packages": [{"name": "locron-core", "id": "core-package"}],
                         "target_directory": str(self.global_target)}
        self.context = {"host": HOST, "compiler": "fixture compiler", "checkout": "c" * 40, "revision": self.revision}
        self.prepared = {"version": 1, "context": self.context, "target": str(self.target),
                         "bindings": tail.bindings(self.original, HOST),
                         "core_selection": {key: self.message[key] for key in ("package_id", "target", "features", "profile", "executable", "fresh")},
                         "global_core": [tail.executable(self.core, HOST)]}
        for role, pattern in tail.ROLES:
            binary(self.target / pattern.replace("*", "default"), role)
        (self.root / "windows-native-evidence.json").write_text("{}\n", encoding="utf-8")
        (self.root / "windows-native-sha256.txt").write_bytes(b"")
        evidence = self.root / "engine-output-qualification"
        evidence.mkdir()
        (evidence / "empty.stderr").write_bytes(b"")
        self.environment = {"CARGO_TARGET_DIR": str(self.target),
                            "LOCRON_PRUNE_RECOVERY_CARGO_TARGET_DIR": str(self.target),
                            "LOCRON_PRUNE_TEST_BINARY": str(self.production),
                            "LOCRON_PRUNE_RECOVERY_TEST_BINARY": str(self.recovery)}

    def save(self):
        (self.root / tail.RECEIPT).write_text(json.dumps(self.prepared), encoding="utf-8")

    def stage(self):
        self.save()
        with patch.dict(os.environ, self.environment), \
                patch.object(tail, "producer", return_value=self.original), \
                patch.object(tail, "context", return_value=self.context):
            tail.stage(self.root)

    def test_positive_inventory_copies_only_and_conserves_original_bindings(self):
        original = tail.bindings(self.original, HOST)
        self.stage()
        upload = self.root / tail.UPLOAD
        manifest = json.loads((upload / "windows-native-upload-manifest.json").read_text())
        self.assertTrue(manifest["upload_only"])
        self.assertEqual(tail.bindings(self.original, HOST), original)
        self.assertEqual({m["role"] for m in manifest["members"]}, {"cold_core", "evidence", *(r[0] for r in tail.ROLES)})
        for member in manifest["members"]:
            source, staged = Path(member["source_path"]), upload / member["relative"]
            self.assertEqual(staged.read_bytes(), source.read_bytes())
            self.assertEqual(member["copied_sha256"], hashlib.sha256(staged.read_bytes()).hexdigest())
            self.assertEqual(member["sha256"], member["copied_sha256"])
            if member["role"] != "evidence":
                self.assertNotEqual(member["source_identity"], tail.executable(staged, HOST)["identity"])
            else:
                self.assertIsNone(member["source_identity"])
        self.assertEqual((upload / "engine-output-qualification/empty.stderr").stat().st_size, 0)
        with self.assertRaises(FileExistsError):
            self.stage()

    def test_original_artifact_hash_identity_and_binding_bytes_cannot_change(self):
        for member in ("production", "recovery", "binding"):
            with self.subTest(member=member):
                path = {"production": self.production, "recovery": self.recovery, "binding": self.locator}[member]
                content = path.read_bytes()
                path.write_bytes(content + b" ")
                with self.assertRaises(ValueError):
                    tail.bindings(self.original, HOST)
                path.write_bytes(content)
        replacement = self.recovery.with_suffix(".replacement")
        replacement.write_bytes(self.recovery.read_bytes())
        replacement.replace(self.recovery)
        with self.assertRaisesRegex(ValueError, "original-executable-ids"):
            tail.bindings(self.original, HOST)

    def test_extra_global_core_is_transported_without_claiming_tested_provenance(self):
        extra = binary(self.core.with_name("locron_core-unselected.exe"), "unselected")
        self.prepared["global_core"].append(tail.executable(extra, HOST))
        self.stage()
        manifest = tail.read_json(self.root / tail.UPLOAD / "windows-native-upload-manifest.json", tail.MANIFEST_CAP)
        cold = [member for member in manifest["members"] if member["role"] == "cold_core"]
        untested = [member for member in manifest["members"] if member["role"] == "untested_global_core"]
        self.assertEqual([m["source_path"] for m in cold], [str(self.core)])
        self.assertEqual([m["source_path"] for m in untested], [str(extra)])

    def test_core_selection_refuses_missing_duplicates_rebuilt_feature_profile_and_wrong_target(self):
        cases = [[], [self.message, self.message]]
        for key, value in (("fresh", False), ("features", ["extra"]), ("profile", dict(tail.PROFILE, test=False)),
                           ("executable", str(self.recovery)), ("package_id", "different-package")):
            changed = copy.deepcopy(self.message)
            changed[key] = value
            cases.append([changed])
        for messages in cases:
            with self.subTest(messages=messages), self.assertRaises(ValueError):
                tail.core_artifact(messages, self.metadata, self.global_target)

    def test_stage_refuses_wrong_target_core_replacement_and_outside_core(self):
        self.environment["CARGO_TARGET_DIR"] = str(self.global_target)
        with self.assertRaisesRegex(ValueError, "admitted-tail-target"):
            self.stage()
        self.environment["CARGO_TARGET_DIR"] = str(self.target)
        replacement = self.core.with_suffix(".replacement")
        replacement.write_bytes(self.core.read_bytes())
        replacement.replace(self.core)
        with self.assertRaisesRegex(ValueError, "cold-core-conserved"):
            self.stage()
        outside = binary(self.root / "elsewhere/locron_core-cold.exe")
        self.prepared["core_selection"]["executable"] = str(outside)
        self.prepared["global_core"] = [tail.executable(outside, HOST)]
        with self.assertRaisesRegex(ValueError, "prepared-core-placement"):
            self.stage()
        self.assertFalse((self.root / tail.UPLOAD).exists())

    def test_missing_role_link_escape_duplicate_identity_and_abi_refuse(self):
        required = self.target / "debug/deps/contract-default.exe"
        content = required.read_bytes()
        required.unlink()
        with self.assertRaisesRegex(ValueError, "required-upload-role-http"):
            tail.inventory(self.target, self.prepared["global_core"], str(self.core), HOST)
        required.symlink_to(self.core)
        with self.assertRaisesRegex(ValueError, "no-reparse"):
            tail.inventory(self.target, self.prepared["global_core"], str(self.core), HOST)
        required.unlink()
        os.link(self.core, required)
        with self.assertRaisesRegex(ValueError, "duplicate-upload-member"):
            tail.inventory(self.target, self.prepared["global_core"], str(self.core), HOST)
        required.unlink()
        required.write_bytes(b"not an executable")
        with self.assertRaises(ValueError):
            tail.inventory(self.target, self.prepared["global_core"], str(self.core), HOST)
        required.write_bytes(content)
        alias = self.root / "aliased-target"
        alias.symlink_to(self.target, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "no-reparse"):
            tail.existing(alias / "debug/locron.exe")

    def test_copy_refuses_overwrite_and_changed_expected_source(self):
        destination = self.root / "staged/copy.exe"
        expected = tail.executable(self.core, HOST)
        tail.copy_member(self.core, destination, HOST, expected)
        with self.assertRaises(FileExistsError):
            tail.copy_member(self.core, destination, HOST, expected)
        self.core.write_bytes(self.core.read_bytes() + b"changed")
        with self.assertRaisesRegex(ValueError, "copy-original"):
            tail.copy_member(self.core, destination.with_name("new.exe"), HOST, expected)

    def test_evidence_directory_and_data_size_are_bounded_and_duplicate_json_refuses(self):
        base = self.root / "engine-output-qualification"
        (base / "empty.stderr").unlink()
        for index in range(3):
            (base / str(index)).mkdir()
        with patch.object(tail, "MAX_FILES", 3), self.assertRaisesRegex(ValueError, "evidence-directory-cap"):
            tail.evidence(self.root)
        data = self.root / "oversized"
        data.write_bytes(b"12345")
        with patch.object(tail, "DATA_CAP", 4), self.assertRaisesRegex(ValueError, "data-hash-cap"):
            tail.copy_member(data, self.root / "too-big")
        data.write_text('{"version":1,"version":1}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "duplicate-field"):
            tail.read_json(data)
        with self.assertRaisesRegex(ValueError, "json-cap"):
            tail.read_json(data, 2)

    def test_prepare_does_not_publish_target_on_stale_core_or_changed_bindings(self):
        env_file = self.root / "github-env"
        env_file.touch()
        self.environment["GITHUB_ENV"] = str(env_file)
        stale = dict(self.message, fresh=False)
        with patch.dict(os.environ, self.environment), \
                patch.object(tail, "producer", return_value=self.original), \
                patch.object(tail, "context", return_value=self.context), \
                patch.object(tail, "metadata", return_value=self.metadata), \
                patch.object(tail, "command", return_value=json.dumps(stale).encode()):
            with self.assertRaisesRegex(ValueError, "fresh-default-core-profile"):
                tail.prepare(self.root)
        self.assertEqual(env_file.read_bytes(), b"")
        self.assertFalse((self.root / tail.RECEIPT).exists())

    def test_prepare_publishes_only_original_recovery_target_after_conservation(self):
        env_file = self.root / "github-env"
        env_file.touch()
        self.environment["GITHUB_ENV"] = str(env_file)
        with patch.dict(os.environ, self.environment), \
                patch.object(tail, "producer", return_value=self.original), \
                patch.object(tail, "context", return_value=self.context), \
                patch.object(tail, "metadata", return_value=self.metadata), \
                patch.object(tail, "command", return_value=json.dumps(self.message).encode()):
            tail.prepare(self.root)
        self.assertEqual(env_file.read_text(), "CARGO_TARGET_DIR=" + str(self.target) + "\n")
        self.assertEqual(tail.read_json(self.root / tail.RECEIPT), self.prepared)

    def test_prepare_refuses_actual_binding_change_during_no_run(self):
        env_file = self.root / "github-env"
        env_file.touch()
        self.environment["GITHUB_ENV"] = str(env_file)

        def change_binding(arguments):
            self.locator.write_bytes(self.locator.read_bytes() + b" ")
            return json.dumps(self.message).encode()

        with patch.dict(os.environ, self.environment), \
                patch.object(tail, "producer", return_value=self.original), \
                patch.object(tail, "context", return_value=self.context), \
                patch.object(tail, "metadata", return_value=self.metadata), \
                patch.object(tail, "command", side_effect=change_binding):
            with self.assertRaisesRegex(ValueError, "original-binding"):
                tail.prepare(self.root)
        self.assertEqual(env_file.read_bytes(), b"")
        self.assertFalse((self.root / tail.RECEIPT).exists())

    def test_exact_original_compiler_capture_is_required(self):
        capture = self.root / "prune-qualification/toolchain-prune-producer.stdout"
        capture.parent.mkdir()
        capture.write_bytes(b"original compiler\n")
        with patch.object(tail, "command", return_value=b"original compiler\n"):
            self.assertEqual(tail.compiler_version(self.root), "original compiler\n")
        with patch.object(tail, "command", return_value=b"other compiler\n"):
            with self.assertRaisesRegex(ValueError, "original-compiler"):
                tail.compiler_version(self.root)
        capture.write_bytes(b"too large")
        with patch.object(tail, "RECEIPT_CAP", 3), patch.object(tail, "command", return_value=b"too large"):
            with self.assertRaisesRegex(ValueError, "original-compiler"):
                tail.compiler_version(self.root)

    def test_total_and_glob_caps_refuse_before_upload_creation(self):
        with patch.object(tail, "TOTAL_CAP", tail.MANIFEST_CAP + 1):
            with self.assertRaisesRegex(ValueError, "total-size-cap"):
                self.stage()
        self.assertFalse((self.root / tail.UPLOAD).exists())
        for index in range(3):
            binary(self.target / ("extra-" + str(index) + ".exe"))
        with patch.object(tail, "MAX_FILES", 2), self.assertRaisesRegex(ValueError, "glob-cap"):
            tail.bounded_glob(self.target, "extra-*.exe")


if __name__ == "__main__":
    unittest.main()
