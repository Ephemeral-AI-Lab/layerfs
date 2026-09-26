#!/usr/bin/env python3
"""#243 ordinary Workspace shell: freeze inputs, then one append-only attempt per case."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

BENCH = Path(__file__).resolve().parent
ROOT = BENCH.parents[2]
CORE = ROOT / "core"
RESULTS = ROOT / "benchmark-results/fs-bench-pro"
REGISTRY = BENCH / "registry/workspace-shell-package-v1.json"
BASE = "alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8"
SEED_COMMAND = "printf '1' | dd of=node_modules/@fixture/core/BUILD_ID bs=1 seek=0 conv=notrunc 2>/dev/null"
sys.path.insert(0, str(BENCH))
from runner import identities  # noqa: E402


def sha(data):
    return hashlib.sha256(data).hexdigest()


def digest(path):
    with Path(path).open("rb") as source:
        h = hashlib.sha256()
        for block in iter(lambda: source.read(1 << 20), b""):
            h.update(block)
        return h.hexdigest()


def json_file(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def pad(header, size):
    assert len(header) <= size
    return header + b" " * (size - len(header))


def recipe():
    base = "node_modules/@fixture/"
    lock = lambda v: ('{"name":"fixture-app","lockfileVersion":3,"packages":{'
        '"node_modules/@fixture/core":{"version":"' + v + '"},'
        '"node_modules/@fixture/parser":{"version":"' + v + '"},'
        '"node_modules/@fixture/ui":{"version":"' + v + '"}}}\n').encode()
    v1 = {
        base + "core/package.json": b'{"name":"@fixture/core","version":"1.0.0"}\n',
        base + "core/index.js": b'export const core = 1;\n',
        base + "core/BUILD_ID": b'1.0.0\n',
        base + "parser/package.json": b'{"name":"@fixture/parser","version":"1.0.0"}\n',
        base + "parser/lib/parse.js": pad(b'export const parse = () => 1;\n', 65536),
        base + "parser/lib/legacy.js": b'export const legacy = true;\n',
        base + "ui/package.json": b'{"name":"@fixture/ui","version":"1.0.0"}\n',
        base + "ui/dist/ui.js": pad(b'export const ui = 1;\n', 1048576),
        "package-lock.json": lock("1.0.0"),
    }
    v2 = {
        "core/package.json": b'{"name":"@fixture/core","version":"2.0.0"}\n',
        "parser/package.json": b'{"name":"@fixture/parser","version":"2.0.0"}\n',
        "parser/lib/parse.js": pad(b'export const parse = () => 2;\n', 65536),
        "parser/generated/tokenize.js": b"export const tokenize = s => s.split(' ');\n",
        "ui/package.json": b'{"name":"@fixture/ui","version":"2.0.0"}\n',
        "ui/dist/ui.js": pad(b'export const ui = 2;\n', 1048576),
        "package-lock.json": lock("2.0.0"),
        "patch-4k.bin": b"P" * 4096,
    }
    shapes = {
        "package": v1,
        "large": {**v1, "large.bin": b"A" * (10 << 20)},
        "repeated": {**v1, "repeated.bin": b"." * (64 << 10)},
    }
    return shapes, v2


def expected_files(shape, case_id, shapes, v2):
    files = dict(shapes[shape])
    base = "node_modules/@fixture/"
    if case_id == "mixed-refresh-v1":
        files[base + "core/BUILD_ID"] = b"2.0.0\n"
        files.pop(base + "parser/lib/legacy.js")
        for path, data in v2.items():
            if path == "patch-4k.bin":
                continue
            files["package-lock.json" if path == "package-lock.json" else base + path] = data
    elif case_id == "overwrite-4k-v1":
        data = bytearray(files["large.bin"])
        data[5 << 20:(5 << 20) + 4096] = v2["patch-4k.bin"]
        files["large.bin"] = bytes(data)
    elif case_id == "repeated-one-byte-v1":
        files["repeated.bin"] = b"X" * 16 + files["repeated.bin"][16:]
    elif case_id != "failed-command-no-commit-v1":
        raise ValueError(case_id)
    return files


def manifest(files):
    directories = {"."}
    for name in files:
        parts = Path(name).parts
        directories.update(str(Path(*parts[:i])) for i in range(1, len(parts)))
    rows = [(name, "d", 0o755, 0, "-") for name in directories]
    rows += [(name, "f", 0o644, len(data), sha(data)) for name, data in files.items()]
    return "".join("\t".join(map(str, row)) + "\n" for row in sorted(rows))


def write_tree(root, files):
    root.mkdir(parents=True, exist_ok=False)
    root.chmod(0o755)
    for name, data in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.parent.chmod(0o755)
        path.write_bytes(data)
        path.chmod(0o644)


def registry():
    data = json.loads(REGISTRY.read_text())
    assert data["schema"] == "issue243-workspace-shell-package-v1"
    assert len(data["cases"]) == 4
    assert [row["scenario_id"] for row in data["cases"]] == data["case_order"]
    assert len(set(data["case_order"])) == 4
    assert {row["shape"] for row in data["cases"]} == {"package", "large", "repeated"}
    assert data["construction_workers"] == 1 and data["one_attempt_per_case"]
    for row in data["cases"]:
        assert row["complete_command_limit_s"] <= 25
        assert row["command"] and "layerfs-edit-tool" not in row["command"]
    return data


def self_check():
    data = registry()
    shapes, v2 = recipe()
    assert len(shapes["package"]) == 9 and len(v2) == 8
    assert len(expected_files("package", "mixed-refresh-v1", shapes, v2)) == 9
    assert len(expected_files("large", "overwrite-4k-v1", shapes, v2)["large.bin"]) == 10 << 20
    assert expected_files("repeated", "repeated-one-byte-v1", shapes, v2)["repeated.bin"][:17] == b"X" * 16 + b"."
    assert data["cases"][-1]["expected_failure"] and not data["cases"][-1]["commit"]
    return {"registry_sha256": digest(REGISTRY), "v1_files": 9, "v2_files": 8, "cases": data["case_order"]}


def run_command(command, log, *, timeout=None, env=None):
    started = time.monotonic_ns()
    result = subprocess.run(command, cwd=ROOT, capture_output=True, env=env, timeout=timeout)
    Path(str(log) + ".stdout").write_bytes(result.stdout)
    Path(str(log) + ".stderr").write_bytes(result.stderr)
    return result, time.monotonic_ns() - started


def one_receipt(stdout):
    lines = [line[8:] for line in stdout.splitlines() if line.startswith(b"RECEIPT\t")]
    if len(lines) != 1:
        raise ValueError(f"expected one driver receipt, got {len(lines)}")
    return json.loads(lines[0])


def case_spec(path, values):
    Path(path).write_text("".join(f"{key}={value}\n" for key, value in sorted(values.items())))


def prepare(output):
    output.mkdir(parents=True, exist_ok=False)
    identity = identities()
    if identity["source_dirty"]:
        raise ValueError(f"prepare needs committed source: {identity['dirty_paths']}")
    data = registry()
    shapes, v2 = recipe()
    fixture = output / "fixture"
    fixture.mkdir()
    for shape, files in shapes.items():
        write_tree(fixture / shape, files)
        (fixture / f"{shape}.tsv").write_text(manifest(files))
    context = output / "image-context"
    context.mkdir()
    write_tree(context / "v2", v2)
    (context / "Dockerfile").write_text(
        f"FROM {BASE}\nCOPY layerfs-daemon /layerfs-daemon\n"
        "COPY v2 /fixtures/v2\nENTRYPOINT [\"/layerfs-daemon\"]\n")
    builds = [
        ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml", "--locked", "-p", "layerfs-sdk", "--example", "benchmark_init", "--example", "benchmark_shell"],
        ["cargo", "+1.85.1", "build", "--release", "--manifest-path", "core/Cargo.toml", "--locked", "-p", "layerfs-server", "--example", "verify_shell"],
        ["cargo", "+1.85.1", "zigbuild", "--release", "--manifest-path", "core/Cargo.toml", "--locked", "--offline", "--target", "aarch64-unknown-linux-musl", "-p", "layerfs-daemon"],
    ]
    for index, command in enumerate(builds):
        result, _ = run_command(command, output / f"build-{index}")
        if result.returncode:
            raise RuntimeError(f"build-{index} failed; retained output")
    binary_sources = {
        "benchmark_init": CORE / "target/release/examples/benchmark_init",
        "benchmark_shell": CORE / "target/release/examples/benchmark_shell",
        "verify_shell": CORE / "target/release/examples/verify_shell",
    }
    archive = output / "binary-archive"
    archive.mkdir()
    binaries = {}
    for name, source in binary_sources.items():
        digest_ = digest(source)
        target = archive / digest_ / name
        target.parent.mkdir()
        shutil.copyfile(source, target)
        target.chmod(0o555)
        binaries[name] = {"path": str(target), "sha256": digest_}
    daemon = CORE / "target/aarch64-unknown-linux-musl/release/layerfs-daemon"
    shutil.copyfile(daemon, context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o755)
    tag = "layerfs-shell-package-v1:issue243"
    result, _ = run_command(["docker", "build", "-q", "-t", tag, str(context)], output / "image-build")
    if result.returncode:
        raise RuntimeError("image build failed; retained output")
    image_id = subprocess.check_output(["docker", "image", "inspect", tag, "--format", "{{.Id}}"], text=True).strip()
    key = os.urandom(32).hex()
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": key, "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    masters = {}
    for shape in shapes:
        root = output / "masters" / shape
        root.mkdir(parents=True)
        init, init_wall = run_command([binaries["benchmark_init"]["path"], str(fixture / shape),
                                     str(root / "store.sqlite"), str(root / "history.sqlite"),
                                     f"shell-package-{shape}"], root / "init", timeout=60, env=env)
        if init.returncode:
            raise RuntimeError(f"Init failed for {shape}; retained output")
        created = json.loads(init.stdout.splitlines()[-1])
        if created["status"] != "COMPLETE":
            raise RuntimeError(f"Init incomplete for {shape}")
        fields = {"scenario_id": f"seed-{shape}", "project_id": created["project_id"],
                  "genesis_layer": created["genesis_layer"], "genesis_root": created["root"],
                  "genesis_root_serial": created["root_serial"],
                  "branch_body": hashlib.sha256(shape.encode()).digest()[:16].hex(),
                  "command_hex": SEED_COMMAND.encode().hex(), "expected_failure": "0",
                  "telemetry_run": int.from_bytes(os.urandom(16), "big") or 1}
        case_spec(root / "seed.case", fields)
        seeded, seed_wall = run_command([binaries["benchmark_shell"]["path"], "seed", str(root / "seed.case"),
                                        str(root / "store.sqlite"), str(root / "history.sqlite"), image_id],
                                       root / "seed", timeout=25, env=env)
        if seeded.returncode:
            raise RuntimeError(f"seed failed for {shape}; retained output")
        receipt = one_receipt(seeded.stdout)
        if receipt["status"] != "COMPLETE" or not receipt["head_commit"] or not receipt["unmount_ok"] or not receipt["sandbox_delete_ok"]:
            raise RuntimeError(f"seed incomplete for {shape}")
        fields.update(branch_id=receipt["branch_id"], old_commit=receipt["head_commit"], expected_failure="1")
        case_spec(root / "verify.case", fields)
        verified, verify_wall = run_command([binaries["verify_shell"]["path"], str(root / "verify.case"),
                                             str(root / "store.sqlite"), str(root / "history.sqlite"),
                                             str(fixture / f"{shape}.tsv"), str(fixture / f"{shape}.tsv")],
                                            root / "verify", timeout=9, env=env)
        if verified.returncode:
            raise RuntimeError(f"master verifier failed for {shape}; retained output")
        masters[shape] = {**{key_: fields[key_] for key_ in ("project_id", "genesis_layer", "genesis_root", "genesis_root_serial", "branch_id", "old_commit")},
                          "store_sha256": digest(root / "store.sqlite"), "history_sha256": digest(root / "history.sqlite"),
                          "old_manifest_sha256": digest(fixture / f"{shape}.tsv"),
                          "init_wall_ns": init_wall, "seed_wall_ns": seed_wall, "verify_wall_ns": verify_wall,
                          "path": str(root)}
        (root / "store.sqlite").chmod(0o444)
        (root / "history.sqlite").chmod(0o444)
    for row in data["cases"]:
        (fixture / f"{row['scenario_id']}.tsv").write_text(manifest(expected_files(row["shape"], row["scenario_id"], shapes, v2)))
    prepared = {"schema": "issue243-shell-prepared-v1", "source": identity,
                "registry_sha256": digest(REGISTRY), "cursor_key": key,
                "binaries": binaries, "image_id": image_id, "image_tag": tag,
                "daemon_sha256": digest(context / "layerfs-daemon"),
                "dockerfile_sha256": digest(context / "Dockerfile"),
                "v2_files": {name: {"size": len(value), "sha256": sha(value)} for name, value in v2.items()},
                "masters": masters,
                "manifests": {path.name: digest(path) for path in fixture.glob("*.tsv")},
                "cache_contract": data["cache_contract"], "clone_method": "shutil.copyfile independent writable byte copy"}
    json_file(output / "prepared.json", prepared)
    print(json.dumps({"prepared": str(output / "prepared.json"), "image_id": image_id, "master_shapes": list(masters)}))


def lft1(stderr):
    events = []
    for line in stderr.splitlines():
        if line.startswith(b"LFT1 "):
            try: events.append(json.loads(line[5:]))
            except ValueError: pass
    selected = [event for event in events if event.get("kind") == "operation" and event.get("key") == 243_000]
    return {"event_count": len(events), "operation": selected[0] if len(selected) == 1 else None,
            "operation_event_count": len(selected)}


def run_case(row, prepared, output):
    folder = output / row["scenario_id"]
    folder.mkdir()
    master = prepared["masters"][row["shape"]]
    master_path = Path(master["path"])
    clone = {}
    for name in ("store", "history"):
        source = master_path / f"{name}.sqlite"
        if digest(source) != master[f"{name}_sha256"]:
            raise ValueError(f"master {name} seal mismatch")
        target = folder / f"{name}.sqlite"
        shutil.copyfile(source, target)
        target.chmod(0o644)
        if digest(target) != master[f"{name}_sha256"]:
            raise ValueError(f"clone {name} seal mismatch")
        clone[name] = str(target)
    fields = {key: master[key] for key in ("project_id", "genesis_layer", "genesis_root", "genesis_root_serial", "branch_id", "old_commit")}
    fields.update(scenario_id=row["scenario_id"], command_hex=row["command"].encode().hex(),
                  expected_failure=int(row["expected_failure"]),
                  telemetry_run=int.from_bytes(os.urandom(16), "big") or 1)
    case_spec(folder / "case.before", fields)
    command = [prepared["binaries"]["benchmark_shell"]["path"], "run", str(folder / "case.before"),
               clone["store"], clone["history"], prepared["image_id"]]
    env = {**os.environ, "LAYERFS_HISTORY_CURSOR_KEY": prepared["cursor_key"], "LAYERFS_CONSTRUCTION_WORKERS": "1"}
    start = time.monotonic_ns()
    try:
        process = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=row["complete_command_limit_s"], env=env)
        stdout, stderr, code, timeout = process.stdout, process.stderr, process.returncode, False
    except subprocess.TimeoutExpired as error:
        stdout, stderr, code, timeout = error.stdout or b"", error.stderr or b"", None, True
    complete_ns = time.monotonic_ns() - start
    (folder / "driver.stdout").write_bytes(stdout)
    (folder / "driver.stderr").write_bytes(stderr)
    try: driver = one_receipt(stdout)
    except (ValueError, json.JSONDecodeError): driver = None
    counts = dict(item.split("=", 1) for item in driver["projection_counts"].split(",") if "=" in item) if driver else {}
    route = bool(driver and int(counts.get("write", -1)) >= row["expected_write_min"]
                 and driver.get("commit_called") == row["commit"])
    verify = {"status": "NOT_RUN"}
    if driver and driver.get("branch_id") == master["branch_id"]:
        fields["expected_head_commit"] = driver["head_commit"]
        case_spec(folder / "case.verify", fields)
        vcommand = [prepared["binaries"]["verify_shell"]["path"], str(folder / "case.verify"),
                    clone["store"], clone["history"],
                    str(Path(master["path"]).parents[1] / "fixture" / f"{row['shape']}.tsv"),
                    str(Path(master["path"]).parents[1] / "fixture" / f"{row['scenario_id']}.tsv")]
        try:
            result, wall = run_command(vcommand, folder / "verifier", timeout=9, env=env)
            verify = {"status": "PASS" if result.returncode == 0 else "FAIL", "exit_code": result.returncode,
                      "wall_ns": wall, "command": vcommand}
        except subprocess.TimeoutExpired:
            verify = {"status": "FAIL", "timeout": True, "command": vcommand}
    lft = lft1(stderr)
    functional = bool(code == 0 and not timeout and driver and driver.get("status") == "COMPLETE"
                      and route and driver.get("unmount_ok") and driver.get("sandbox_delete_ok")
                      and verify["status"] == "PASS")
    receipt = {"schema": "issue243-shell-attempt-v1", "family_id": "workspace_shell_package",
               "scenario_id": row["scenario_id"], "scenario_version": 1,
               "source_commit": prepared["source"]["source_commit"], "source_tree": prepared["source"]["source_tree"],
               "product_seal": prepared["source"]["product_seal"], "harness_seal": prepared["source"]["harness_seal"],
               "registry_sha256": prepared["registry_sha256"], "image_id": prepared["image_id"],
               "binary_sha256": prepared["binaries"]["benchmark_shell"]["sha256"],
               "verifier_sha256": prepared["binaries"]["verify_shell"]["sha256"],
               "master": master, "clone_method": prepared["clone_method"], "clone": clone,
               "command": command, "shell_command": row["command"], "complete_command_wall_ns": complete_ns,
               "complete_command_limit_s": row["complete_command_limit_s"], "driver_exit_code": code,
               "driver_timeout": timeout, "driver": driver, "callback_counts": counts, "route_ok": route,
               "lft1": lft, "verifier": verify,
               "cache_contract": prepared["cache_contract"], "cache_status": "INELIGIBLE",
               "performance_status": "INELIGIBLE", "functional_status": "PASS" if functional else "FAIL",
               "cleanup_status": "PASS" if driver and driver.get("unmount_ok") and driver.get("sandbox_delete_ok") else "FAIL",
               "row_status": "INELIGIBLE" if functional else "FAIL", "sample_count": 1,
               "resource_scope": "LFT1 process-shared sampled driver process only; no phase-isolated or container RSS",
               "count_scope": "Status write aggregates namespace mutations; not exact FUSE WRITE count"}
    json_file(folder / "receipt.json", receipt)
    sums = [f"{digest(path)}  {path.name}" for path in sorted(folder.iterdir()) if path.is_file()]
    (folder / "SHA256SUMS").write_text("\n".join(sums) + "\n")
    return receipt


def run(prepared_path, output):
    prepared = json.loads(prepared_path.read_text())
    current = identities()
    if current["source_dirty"] or current["source_commit"] != prepared["source"]["source_commit"] or current["product_seal"] != prepared["source"]["product_seal"] or current["harness_seal"] != prepared["source"]["harness_seal"]:
        raise ValueError("source or harness changed since preparation")
    if digest(REGISTRY) != prepared["registry_sha256"]:
        raise ValueError("registry changed")
    image = subprocess.check_output(["docker", "image", "inspect", prepared["image_id"], "--format", "{{.Id}}"], text=True).strip()
    if image != prepared["image_id"]:
        raise ValueError("image identity changed")
    for binary in prepared["binaries"].values():
        if digest(binary["path"]) != binary["sha256"]:
            raise ValueError("binary seal mismatch")
    data = registry()
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    stop = None
    for row in data["cases"]:
        if stop:
            folder = output / row["scenario_id"]
            folder.mkdir()
            receipt = {"scenario_id": row["scenario_id"], "sample_count": 0,
                       "row_status": "NOT_RUN", "reason": stop}
            json_file(folder / "receipt.json", receipt)
        else:
            try:
                receipt = run_case(row, prepared, output)
                if receipt["cleanup_status"] != "PASS":
                    stop = "prior case cleanup failed or unknown; further runs unsafe"
            except Exception as error:
                stop = f"case error: {error!r}"
                folder = output / row["scenario_id"]
                folder.mkdir(exist_ok=True)
                receipt = {"scenario_id": row["scenario_id"], "sample_count": 0,
                           "row_status": "NOT_RUN", "reason": stop}
                json_file(folder / "receipt.json", receipt)
        rows.append({"scenario_id": row["scenario_id"], "row_status": receipt["row_status"]})
    json_file(output / "campaign.json", {"schema": "issue243-shell-campaign-v1", "prepared": str(prepared_path),
                                         "rows": rows, "source": prepared["source"], "registry_sha256": prepared["registry_sha256"]})
    print(json.dumps({"output": str(output), "rows": rows}))


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("self-check")
    p = sub.add_parser("prepare"); p.add_argument("--output", required=True, type=Path)
    p = sub.add_parser("run"); p.add_argument("--prepared", required=True, type=Path); p.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.command == "self-check": print(json.dumps(self_check(), sort_keys=True))
    elif args.command == "prepare": prepare(args.output)
    else: run(args.prepared, args.output)


if __name__ == "__main__":
    main()
