"""Family 7 package/POSIX workloads using the existing public SDK and full proof."""
from dataclasses import dataclass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess

from families import workspace_namespace as namespace
from families import workspace_commit_native as native

SCHEMA = "core-workspace-shell-package-run-v2"
PROOF_SCHEMA = "core-workspace-shell-package-proof-v2"
NATIVE_SCHEMA = "core-workspace-shell-package-native-run-v2"
NATIVE = {"workspace-shell-package-progress-custody-native-v2": native.Case(
    "workspace-shell-package-progress-custody-native-v2", "phase_b_mutations::progress_custody",
    clones=2, retained=True, budget_ns=15_000_000_000, ignored=True)}
PROFILE = "package-many-file-functional-clone-v2"
DEFERRED = "failed-command-no-commit-v1"
DEFER_REASON = "OWNER-DEFERRED #276: unmount does not discard dirty changes; original method remains historical, no repeated dirty-close attempt"
CURSOR = "35" * 32


@dataclass(frozen=True)
class Case(namespace.SdkCase):
    original: str = ""
    count: int = 0
    spread: int = 1
    retained: bool = False


def many_command(count, spread=1, value="new"):
    mkdir = "mkdir many" if spread == 1 else f"mkdir many; j=0; while test $j -lt {spread}; do mkdir many/d$j; j=$((j+1)); done"
    name = 'many/f$i' if spread == 1 else 'many/d$((i%17))/f$i'
    return (f"set -eu; umask 022; {mkdir}; i=0; while test $i -lt {count}; do "
            f"printf '{value}-%s' \"$i\" > {name}; i=$((i+1)); done; "
            f"test $(find many -type f | wc -l) -eq {count}")


def cases():
    # Lazy import preserves the shared shell helper's import-first route.
    from shell_package import registry
    result = {}
    for old in registry()["cases"]:
        if old["scenario_id"] == DEFERRED:
            continue
        item = Case("workspace-shell-package-" + old["scenario_id"].removesuffix("-v1") + "-sdk-v2",
                    old["shape"], old["command"], old["complete_command_limit_s"] * 1_000_000_000,
                    original=old["scenario_id"])
        result[item.id] = item
    for count, spread, bound in ((128, 1, 15), (129, 1, 15), (257, 17, 15), (1025, 17, 25)):
        item = Case(f"workspace-shell-package-many-{count}-sdk-v2", "package", many_command(count, spread),
                    bound * 1_000_000_000, count=count, spread=spread)
        result[item.id] = item
    item = Case("workspace-shell-package-many-129-live-g2-sdk-v2", "package",
                "set -eu; i=0; while test $i -lt 129; do printf 'new-%s' \"$i\" > many/f$i; i=$((i+1)); done; "
                "rm many/f128; printf replacement > many/f127.next; mv -f many/f127.next many/f127; "
                "test $(find many -type f | wc -l) -eq 128",
                prelude=many_command(129, value="old"), count=129, retained=True)
    result[item.id] = item
    return result


def files(case):
    from shell_package import recipe, expected_files
    shapes, v2 = recipe()
    old = dict(shapes[case.layout])
    new = expected_files(case.layout, case.original, shapes, v2) if case.original else dict(old)
    if case.count:
        for i in range(case.count):
            path = f"many/f{i}" if case.spread == 1 else f"many/d{i % 17}/f{i}"
            if case.retained:
                old[path] = f"old-{i}".encode()
            new[path] = f"new-{i}".encode()
        if case.retained:
            del new["many/f128"]
            new["many/f127"] = b"replacement"
    return old, new


def oracle(case):
    from shell_package import manifest
    return tuple(manifest(value) for value in files(case))


def attempt(out, case, master, prepared):
    def route(driver, counts, control):
        return bool(driver and driver.get("commit_called") and driver.get("exec_exit_status") == 0
                    and int(counts.get("write", 0)) >= (case.count or 1))
    row = namespace.sdk_attempt(out, case, master, prepared, profile=PROFILE, oracle_builder=oracle,
        properties={"identity_old_prefix": "node_modules/@fixture/core/index.js",
                    "identity_new_prefix": "node_modules/@fixture/core/index.js"},
        route_validator=route, pin_path=b"many/f0", pin_expected=b"old-0")
    row.update(family_id="workspace_shell_package", original_case_id=case.original or None,
               changed_file_count=case.count or None, workload_source_sha256=hashlib.sha256(case.command.encode()).hexdigest())
    namespace.shared.save(out / case.id / "receipt.json", row)
    return row


def package_image(out, common, prepared):
    from shell_package import recipe, write_tree, BASE
    shared = namespace.shared
    context = out / "package-image-context"
    context.mkdir()
    write_tree(context / "v2", recipe()[1])
    # Reuse the sealed daemon and its Docker layer, adding only frozen input files.
    import shutil
    shutil.copyfile(prepared["artifacts"]["layerfs-daemon"]["path"], context / "layerfs-daemon")
    (context / "layerfs-daemon").chmod(0o555)
    (context / "Dockerfile").write_text(f"FROM {BASE}\nCOPY layerfs-daemon /layerfs-daemon\nCOPY v2 /fixtures/v2\nENTRYPOINT [\"/layerfs-daemon\"]\n")
    seal = hashlib.sha256(json.dumps({str(path.relative_to(context)): shared.sha256(path)
        for path in context.rglob("*") if path.is_file()}, sort_keys=True).encode()).hexdigest()
    cache = common.RESULTS / "workspace-shell-package-image-v2.json"
    prior = json.loads(cache.read_text()) if cache.exists() else None
    if prior and prior["seal"] == seal:
        result = {**prior, "mode": "exact-image-reuse", "wall_ns": 0}
    else:
        record, stdout, _ = shared.execute(["docker", "build", "-q", str(context)], out / "package-image-build")
        shared.save(out / "package-image-build.json", record)
        if record["exit_code"] != 0:
            raise RuntimeError("package image build failed")
        result = {"image_id": stdout.decode().strip(), "seal": seal, "mode": "build", "wall_ns": record["wall_ns"]}
        shared.save(cache, result)
    if subprocess.check_output(["docker", "image", "inspect", result["image_id"], "--format", "{{.Id}}"], text=True).strip() != result["image_id"]:
        raise ValueError("package image identity changed")
    return result


def master(out, layout, common, prepared):
    from shell_package import recipe, write_tree, manifest, case_spec, SEED_COMMAND
    shared = namespace.shared
    content = recipe()[0][layout]
    manifest_text = manifest(content)
    key = hashlib.sha256(manifest_text.encode()).hexdigest()
    path = common.RESULTS / "workspace-shell-package-master-v2" / key
    if (path / "prepared.json").exists():
        result = json.loads((path / "prepared.json").read_text())
        for name, sha in result["files"].items():
            if shared.sha256(path / name) != sha:
                raise ValueError("sealed package master changed")
        return {**result, "reuse": True}
    path.mkdir(parents=True)
    write_tree(path / "source", content)
    (path / "old.tsv").write_text(manifest_text)
    created_record, stdout, _ = shared.execute([prepared["artifacts"]["benchmark_init"]["path"], str(path / "source"),
        str(path / "store.sqlite"), str(path / "history.sqlite"), "family7-" + layout], out / ("init-" + layout), timeout=15, env=namespace.env(CURSOR))
    shared.save(out / ("init-" + layout + ".json"), created_record)
    if created_record["exit_code"] != 0:
        raise RuntimeError("untimed package fixture SDK Init failed")
    created = json.loads(stdout.splitlines()[-1])
    if created["status"] != "COMPLETE":
        raise RuntimeError("package fixture Init incomplete")
    values = {"scenario_id": "prepare-package-" + layout, "project_id": created["project_id"],
              "genesis_layer": created["genesis_layer"], "genesis_root": created["root"],
              "genesis_root_serial": str(created["root_serial"]), "branch_body": hashlib.sha256(layout.encode()).digest()[:16].hex(),
              "command_hex": SEED_COMMAND.encode().hex(), "expected_failure": "0", "component_oracle": "1",
              "telemetry_run": str(int.from_bytes(os.urandom(16), "big") or 1)}
    case_spec(path / "seed.case", values)
    record, stdout, stderr = shared.execute([prepared["artifacts"]["benchmark_shell"]["path"], "seed", str(path / "seed.case"),
        str(path / "store.sqlite"), str(path / "history.sqlite"), prepared["image_id"]], out / ("seed-" + layout), timeout=15, env=namespace.env(CURSOR))
    shared.save(out / ("seed-" + layout + ".json"), record)
    driver = shared.receipt_line(stdout)
    if record["exit_code"] != 0 or not driver or driver["status"] != "COMPLETE" or not namespace.commit.cleanup_complete(driver, namespace.commit.control_line(stdout), stderr):
        raise RuntimeError("package seed failed; retained setup evidence")
    fixture = {"project_id": created["project_id"], "genesis_layer": created["genesis_layer"],
               "root": created["root"], "root_serial": str(created["root_serial"]), "branch_id": driver["branch_id"]}
    case_spec(path / "fixture.before", fixture)
    case_spec(path / "verify.case", {**values, "branch_id": driver["branch_id"], "old_commit": driver["head_commit"], "expected_failure": "1"})
    proof, stdout, _ = shared.execute([prepared["artifacts"]["verify_checkpoint5"]["path"], str(path / "verify.case"),
        str(path / "store.sqlite"), str(path / "history.sqlite"), str(path / "old.tsv"), str(path / "old.tsv")], out / ("master-proof-" + layout), timeout=9, env=namespace.env(CURSOR))
    shared.save(out / ("master-proof-" + layout + ".json"), proof)
    if proof["exit_code"] != 0 or json.loads(stdout).get("status") != "PASS":
        raise RuntimeError("package master full oracle failed")
    result = {"path": str(path), "layout": layout, "old_commit": driver["head_commit"], "cursor_key": CURSOR,
              "binding_key_hex": b"layerfs-bench-pro".hex(), "producer_identity": prepared["identity"],
              "preparation": {"init": created_record, "seed": record, "full_proof": proof},
              "files": {name: shared.sha256(path / name) for name in ("store.sqlite", "history.sqlite", "fixture.before")}}
    for name in result["files"]:
        (path / name).chmod(0o444)
    shared.save(path / "prepared.json", result)
    return {**result, "reuse": False}


def run(selection, output, common):
    if selection in NATIVE:
        return native.run(selection, output, common, cases=NATIVE, selected=(selection,),
                          profile=PROFILE, schema=NATIVE_SCHEMA)
    shared = namespace.shared
    out = common.owned(output)
    identity = common.identities()
    if identity["source_dirty"]:
        raise ValueError("commit Family7 before collection")
    out.mkdir(parents=True)
    registered = cases()
    selected = (selection,) if selection in registered or selection == DEFERRED else tuple(registered)[-2:] if selection == "workspace-shell-package-tail" else tuple(registered)
    summary = {"schema": SCHEMA, "profile": PROFILE, "identity": identity, "selected": list(selected), "rows": [],
               "deferred_cases": [{"case": DEFERRED, "status": "OWNER-DEFERRED", "reason": DEFER_REASON}],
               "reused_proof_identities": {"earlier_families": "FAMILY1/2/4/5/6-CHECKPOINT; F3 r063+unchanged eight rows; product unchanged", "dirty_cleanup": "r069 FAIL, #276 comment5903530127"}}
    if selection == DEFERRED:
        summary["rows"] = [{"case": DEFERRED, "status": "NOT_RUN", "sample_count": 0, "reason": DEFER_REASON}]
    else:
        try:
            with (common.RESULTS / ".run.lock").open("a+b") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                prior = json.loads((common.RESULTS / "issue286-workspace-commit-fast-checkpoint-r062/prepared.json").read_text())
                prepared = shared.build(out, common, identity, reuse_image=prior, artifacts_only=True)
                init_build = common.build(out, common.target_path(), identity)
                shared.save(out / "init-build.json", init_build)
                if init_build["status"] != "PASS":
                    raise RuntimeError("sole existing SDK Init build failed or exceeded build budget")
                prepared["artifacts"]["benchmark_init"] = init_build["binaries"]["benchmark_init"]
                image = package_image(out, common, prepared)
                prepared.update(image_id=image["image_id"], package_image=image)
                summary["prepared"] = prepared
                shared.save(out / "prepared.json", prepared)
                masters = {layout: master(out, layout, common, prepared) for layout in dict.fromkeys(registered[name].layout for name in selected)}
                for name in selected:
                    row = attempt(out, registered[name], masters[registered[name].layout], prepared)
                    summary["rows"].append({"case": name, "status": row["status"]})
                    if row["cleanup_status"] != "PASS":
                        break
        except Exception as error:
            summary["error"] = repr(error)
        for name in selected[len(summary["rows"]):]:
            summary["rows"].append({"case": name, "status": "NOT_RUN", "reason": summary.get("error", "prior cleanup failed")})
    summary["status"] = "COMPLETE_DIAGNOSTIC" if all(row["status"] == "COMPLETE_DIAGNOSTIC" for row in summary["rows"]) else "INCOMPLETE"
    shared.save(out / "run.json", summary)
    common.manifest_run(out)
    return out


def prove(run, output, common):
    return namespace.prove(run, output, common, schema=PROOF_SCHEMA)


def report(out):
    return namespace.report(out)
