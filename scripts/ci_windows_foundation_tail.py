"""Reuse admitted Windows build outputs; copies are upload packaging only."""

import base64
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import stat
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
RECEIPT = "windows-foundation-tail.json"
UPLOAD = "windows-native-upload"
RECEIPT_CAP = 64 * 1024
MANIFEST_CAP = 1024 * 1024
DATA_CAP = 8 * 1024 * 1024
MAX_FILES = 128
TOTAL_CAP = 4 * 1024 * 1024 * 1024
PROFILE = {"debug_assertions": True, "debuginfo": 2, "opt_level": "0", "overflow_checks": True, "test": True}
ROLES = (
    ("console", "debug/locron.exe"),
    ("gui", "debug/locron-service-launcher.exe"),
    ("store", "debug/deps/locron_store-*.exe"),
    ("engine", "debug/deps/locron_engine-*.exe"),
    ("server", "debug/deps/locron_server-*.exe"),
    ("lifecycle", "debug/deps/windows_lifecycle-*.exe"),
    ("gui_tests", "debug/deps/windows_service_launcher-*.exe"),
    ("mcp", "debug/deps/mcp-*.exe"),
    ("http", "debug/deps/contract-*.exe"),
)


def require(condition, phase):
    if not condition:
        raise ValueError("fixed Windows tail refusal: " + phase)


def load_reader():
    source = ROOT / "scripts/ci_prune_artifact_facts.py"
    spec = importlib.util.spec_from_file_location("locron_tail_artifact_reader", source)
    require(spec is not None and spec.origin == str(source), "fixed-reader")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ARTIFACTS = load_reader()


def existing(path, directory=False):
    path = pathlib.Path(path)
    require(path.is_absolute() and not any(ord(c) < 32 for c in str(path)), "absolute-path")
    for component in (*reversed(path.parents), path):
        info = component.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400, "no-reparse")
    info = path.lstat()
    require(stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode), "path-type")
    return path


def read_json(path, cap=RECEIPT_CAP):
    with existing(path).open("rb") as stream:
        encoded = stream.read(cap + 1)
    require(len(encoded) <= cap, "json-cap")
    return json.loads(encoded.decode("utf-8"), object_pairs_hook=ARTIFACTS.binding_object)


def data_digest(path):
    path = existing(path)
    with path.open("rb") as stream:
        before = os.fstat(stream.fileno())
        digest, total = hashlib.sha256(), 0
        while chunk := stream.read(ARTIFACTS.ARTIFACT_CHUNK):
            total += len(chunk)
            require(total <= DATA_CAP, "data-hash-cap")
            digest.update(chunk)
        after = os.fstat(stream.fileno())
    fields = lambda info: (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)
    require(fields(before) == fields(after) == fields(path.stat()) and total == after.st_size, "data-hash-conserved")
    return digest.hexdigest(), total


def write_json(path, value, cap):
    encoded = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    require(len(encoded) <= cap, "json-write-cap")
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())


def command(arguments):
    # Fixed Cargo/Git queries share the existing 35-minute job owner. Captures are
    # temporary files, never an unbounded in-memory communicate() buffer or output.
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        result = subprocess.run(arguments, cwd=ROOT, stdout=out, stderr=err, check=False)
        require(result.returncode == 0, "command-status")
        out.seek(0)
        value = out.read(DATA_CAP + 1)
        require(len(value) <= DATA_CAP, "command-capture-cap")
        return value


def metadata():
    return json.loads(command(["cargo", "metadata", "--locked", "--no-deps", "--format-version=1"]))


def producer(root):
    return read_json(root / "prune-qualification/producer.json")


def compiler_version(root):
    compiler = command(["rustc", "-vV"]).decode("ascii")
    with existing(root / "prune-qualification/toolchain-prune-producer.stdout").open("rb") as stream:
        previous = stream.read(RECEIPT_CAP + 1)
    require(len(previous) <= RECEIPT_CAP and previous.decode("ascii") == compiler, "original-compiler")
    return compiler


def context(original, root=ROOT):
    expected = os.environ["PRUNE_QUALIFICATION_PLATFORM"]
    require(os.name == "nt" and expected in ("x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"), "native-windows")
    compiler = compiler_version(root)
    require(re.findall(r"^host: ([^\r\n]+)\r?$", compiler, re.M) == [expected], "native-host")
    require(original["native_host"] == expected, "original-native-host")
    checkout = command(["git", "rev-parse", "HEAD"]).decode("ascii").strip()
    tree = command(["git", "rev-parse", "HEAD^{tree}"]).decode("ascii").strip()
    command(["git", "diff", "--quiet", "HEAD", "--"])
    require(checkout == os.environ["GITHUB_SHA"] == original["checkout"]["checkout"], "actual-checkout")
    require(tree == original["revision"]["tree"] and original["revision"]["head"] == os.environ["PRUNE_QUALIFICATION_HEAD"], "actual-revision")
    require(all(re.fullmatch(r"[0-9a-f]{40}", value) for value in (checkout, tree, original["revision"]["head"])), "revision-shape")
    return {"compiler": compiler, "host": expected, "checkout": checkout, "revision": original["revision"]}


def executable(path, host):
    path = existing(path)
    facts, identity = ARTIFACTS.facts(path, host)
    return {"facts": facts, "identity": identity}


def bindings(original, host):
    prod = pathlib.Path(original["production_artifact"]["executable"])
    rec = pathlib.Path(original["recovery_artifact"]["executable"])
    production = executable(prod, host)
    recovery = executable(rec, host)
    require(production["identity"] == original["production_identity"] and recovery["identity"] == original["recovery_identity"], "original-executable-ids")
    data, encoded, identity, digest = ARTIFACTS.binding_json_facts(prod, rec, host, original["revision"])
    require(identity == original["binding_identity"] and digest == original["binding_sha256"], "original-binding")
    for name, observed in (("production", production), ("recovery", recovery)):
        require(all(observed["facts"][key] == data[name][key] for key in ("path", "sha256", "size")), "original-executable-hash")
    return {"production": production, "recovery": recovery, "binding": {"identity": identity, "sha256": digest, "bytes_base64": base64.b64encode(encoded).decode("ascii")}}


def core_artifact(messages, data, global_target):
    packages = [p for p in data["packages"] if p["name"] == "locron-core"]
    require(len(packages) == 1, "one-core-package")
    matches = [m for m in messages if m.get("reason") == "compiler-artifact" and m.get("package_id") == packages[0]["id"] and m.get("target", {}).get("name") == "locron_core" and m["target"]["kind"] == ["lib"] and m.get("executable")]
    require(len(matches) == 1, "one-core-executable")
    chosen = matches[0]
    require(chosen.get("fresh") is True and chosen.get("features") == [] and chosen.get("profile") == PROFILE, "fresh-default-core-profile")
    path = existing(chosen["executable"])
    require(path.parent == global_target / "debug/deps" and path.name.startswith("locron_core-") and path.suffix == ".exe", "original-core-placement")
    return {key: chosen[key] for key in ("package_id", "target", "features", "profile", "executable", "fresh")}


def bounded_glob(base, pattern):
    paths = []
    for path in base.glob(pattern):
        require(len(paths) < MAX_FILES, "glob-cap")
        paths.append(path)
    return sorted(paths)


def prepare(root=ROOT):
    original = producer(root)
    observed = context(original, root)
    target = existing(original["test_target"], directory=True)
    require(target == pathlib.Path(os.environ["LOCRON_PRUNE_RECOVERY_CARGO_TARGET_DIR"]), "producer-target")
    rec = pathlib.Path(original["recovery_artifact"]["executable"])
    rec.relative_to(target)
    require(original["recovery_artifact"]["features"] == [] and original["recovery_artifact"]["profile"] == PROFILE, "original-recovery-profile")
    require(str(original["production_artifact"]["executable"]) == os.environ["LOCRON_PRUNE_TEST_BINARY"] and str(rec) == os.environ["LOCRON_PRUNE_RECOVERY_TEST_BINARY"], "forwarded-original-artifacts")
    before = bindings(original, observed["host"])
    data = metadata()
    global_target = existing(data["target_directory"], directory=True)
    require(global_target == root / "target" and global_target != target, "original-global-target")
    messages = [json.loads(line) for line in command(["cargo", "test", "-p", "locron-core", "--lib", "--locked", "--no-run", "--message-format=json"]).splitlines()]
    selected = core_artifact(messages, data, global_target)
    core_paths = bounded_glob(global_target, "debug/deps/locron_core-*.exe")
    require(0 < len(core_paths) < MAX_FILES and pathlib.Path(selected["executable"]) in core_paths, "cold-core-members")
    cores = [executable(path, observed["host"]) for path in core_paths]
    require(bindings(original, observed["host"]) == before, "prepare-bindings-conserved")
    record = {"version": 1, "context": observed, "target": str(target), "bindings": before, "core_selection": selected, "global_core": cores}
    write_json(root / RECEIPT, record, RECEIPT_CAP)
    with existing(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as stream:
        stream.write("CARGO_TARGET_DIR=" + str(target) + "\n")
    print("windows-foundation-tail prepare PASS")


def inventory(target, cores, selected, host):
    require(sum(c["facts"]["path"] == selected for c in cores) == 1, "one-tested-cold-core")
    members = [("cold_core" if c["facts"]["path"] == selected else "untested_global_core", pathlib.Path(c["facts"]["path"]), pathlib.Path("target/debug/deps") / pathlib.Path(c["facts"]["path"]).name) for c in cores]
    for role, pattern in ROLES:
        matches = bounded_glob(target, pattern)
        require(matches, "required-upload-role-" + role)
        require(len(members) + len(matches) < MAX_FILES, "inventory-cap")
        members.extend((role, path, pathlib.Path("target") / path.relative_to(target)) for path in matches)
    records, seen_ids, seen_names = [], set(), set()
    for role, path, relative in members:
        observed = executable(path, host)
        identity = json.dumps(observed["identity"], sort_keys=True)
        require(identity not in seen_ids and str(relative).casefold() not in seen_names, "duplicate-upload-member")
        seen_ids.add(identity)
        seen_names.add(str(relative).casefold())
        records.append({"role": role, "relative": relative.as_posix(), **observed})
    return records


def evidence(root):
    paths = [root / "windows-native-evidence.json", root / "windows-native-sha256.txt"]
    base = existing(root / "engine-output-qualification", directory=True)
    pending, directories = [base], 1
    while pending:
        directory = pending.pop()
        require(len(directory.relative_to(base).parts) <= 16, "evidence-depth")
        with os.scandir(directory) as entries:
            for entry in entries:
                path = pathlib.Path(entry.path)
                if entry.is_dir(follow_symlinks=False):
                    existing(path, directory=True)
                    directories += 1
                    require(directories <= MAX_FILES, "evidence-directory-cap")
                    pending.append(path)
                else:
                    paths.append(existing(path))
                    require(len(paths) < MAX_FILES, "evidence-count")
    return paths


def copy_member(path, destination, host=None, expected=None):
    path = existing(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    existing(destination.parent, directory=True)
    reader = ARTIFACTS.OwnedRead(ARTIFACTS.WindowsReadApi() if os.name == "nt" else None) if host else None
    stream = None
    try:
        if reader:
            reader.open(path)
            before, digest, size = ARTIFACTS.owned_digest(reader, host)
            require(expected == {"facts": {"path": str(path), "sha256": digest, "size": size}, "identity": before[3]}, "copy-original")
            stream = reader.stream
            stream.seek(0)
        else:
            digest, size = data_digest(path)
            stream = path.open("rb")
            before = path.stat()
        copied = hashlib.sha256()
        total = 0
        with destination.open("xb") as output:
            while chunk := stream.read(ARTIFACTS.ARTIFACT_CHUNK):
                total += len(chunk)
                require(total <= (ARTIFACTS.ARTIFACT_CAP if host else DATA_CAP), "copy-cap")
                output.write(chunk)
                copied.update(chunk)
            output.flush()
            os.fsync(output.fileno())
        copied_digest = copied.hexdigest()
        if reader:
            require(reader.snapshot() == before and executable(path, host) == expected, "copy-source-conserved")
            copied_facts = executable(destination, host)
            require(copied_facts["facts"]["sha256"] == digest == copied_digest and copied_facts["facts"]["size"] == size == total and copied_facts["identity"] != before[3], "copy-verified")
        else:
            after = path.stat()
            require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns) and after.st_size == total, "data-source-conserved")
            copied_digest, copied_size = data_digest(destination)
            require(data_digest(path) == (digest, size) == (copied_digest, copied_size) and size == total, "data-copy-verified")
        return {"source_path": str(path), "source_identity": expected["identity"] if expected else None, "sha256": digest, "size": total, "copied_sha256": copied_digest}
    finally:
        if reader:
            require(reader.close_known(), "copy-owned-close")
        elif stream:
            stream.close()


def stage(root=ROOT):
    prepared = read_json(root / RECEIPT)
    require(type(prepared) is dict and set(prepared) == {"version", "context", "target", "bindings", "core_selection", "global_core"} and type(prepared["version"]) is int and prepared["version"] == 1, "prepared-schema")
    original = producer(root)
    observed = context(original, root)
    require(observed == prepared["context"], "prepared-context")
    target = existing(prepared["target"], directory=True)
    require(target == pathlib.Path(os.environ["CARGO_TARGET_DIR"]) == pathlib.Path(os.environ["LOCRON_PRUNE_RECOVERY_CARGO_TARGET_DIR"]) == pathlib.Path(original["test_target"]), "admitted-tail-target")
    require(bindings(original, observed["host"]) == prepared["bindings"], "tail-original-bindings")
    require(type(prepared["global_core"]) is list and 0 < len(prepared["global_core"]) < MAX_FILES, "prepared-core-members")
    selected = prepared["core_selection"]
    require(type(selected) is dict and set(selected) == {"package_id", "target", "features", "profile", "executable", "fresh"}, "prepared-core-schema")
    require(selected["fresh"] is True and selected["features"] == [] and selected["profile"] == PROFILE and selected["target"]["name"] == "locron_core" and selected["target"]["kind"] == ["lib"], "prepared-core-selection")
    require(selected["executable"] in [core["facts"]["path"] for core in prepared["global_core"]], "selected-core-member")
    for core in prepared["global_core"]:
        path = pathlib.Path(core["facts"]["path"])
        require(path.parent == root / "target/debug/deps" and path.name.startswith("locron_core-") and path.suffix == ".exe", "prepared-core-placement")
        require(executable(core["facts"]["path"], observed["host"]) == core, "cold-core-conserved")
    records = inventory(target, prepared["global_core"], selected["executable"], observed["host"])
    data = evidence(root)
    require(len(records) + len(data) + 1 <= MAX_FILES, "total-inventory-cap")
    require(sum(r["facts"]["size"] for r in records) + sum(existing(p).stat().st_size for p in data) <= TOTAL_CAP - MANIFEST_CAP, "total-size-cap")
    upload = root / UPLOAD
    upload.mkdir()
    members = []
    for record in records:
        copied = copy_member(record["facts"]["path"], upload / record["relative"], observed["host"], {"facts": record["facts"], "identity": record["identity"]})
        members.append({"role": record["role"], "relative": record["relative"], **copied})
    for path in data:
        relative = path.relative_to(root)
        copied = copy_member(path, upload / relative)
        members.append({"role": "evidence", "relative": relative.as_posix(), **copied})
    require(bindings(original, observed["host"]) == prepared["bindings"], "packaged-bindings-conserved")
    for core in prepared["global_core"]:
        require(executable(core["facts"]["path"], observed["host"]) == core, "packaged-cold-core-conserved")
    write_json(upload / "windows-native-upload-manifest.json", {"version": 1, "upload_only": True, "context": observed, "core_selection": prepared["core_selection"], "original_bindings": prepared["bindings"], "members": members}, MANIFEST_CAP)
    print("windows-foundation-tail stage PASS files=" + str(len(members)))


if __name__ == "__main__":
    try:
        require(len(sys.argv) == 2 and sys.argv[1] in ("prepare", "stage"), "closed-phase")
        {"prepare": prepare, "stage": stage}[sys.argv[1]]()
    except Exception as error:
        print("windows-foundation-tail FAIL kind=" + type(error).__name__, file=sys.stderr)
        sys.exit(1)
