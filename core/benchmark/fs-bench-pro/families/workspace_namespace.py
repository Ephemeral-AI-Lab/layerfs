"""Family 5 namespace controls through the existing native and public SDK drivers."""
from dataclasses import dataclass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil

from families import workspace_commit as commit
from families import workspace_commit_native as native
from families import workspace_write as shared

SCHEMA = "core-workspace-namespace-run-v1"
PROOF_SCHEMA = "core-workspace-namespace-proof-v1"
NATIVE_SCHEMA = "core-workspace-namespace-native-run-v1"
PROFILE = "namespace-component-and-sdk-clone-functional-v1"
LAYOUTS = {"small": (0, 0, False, 0), "wide": (0, 64, False, 0),
           "deep": (0, 0, True, 0), "unrelated": (0, 0, False, 256)}
CURSOR = "35" * 32


@dataclass(frozen=True)
class NativeCase(native.Case):
    environment: tuple = ()


def move_case(name, extra=0, resident=0, unrelated=0):
    return NativeCase(f"workspace-namespace-{name}-native-v1", "phase_b_namespace::move_scaling",
                      "wide" if extra else "unrelated" if unrelated else "small",
                      budget_ns=15_000_000_000, ignored=True,
                      environment=(("LAYERFS_PHASE_B_EXTRA", extra), ("LAYERFS_PHASE_B_RESIDENT", resident),
                                   ("LAYERFS_PHASE_B_UNRELATED", unrelated)))


NATIVE = {case.id: case for case in (
    move_case("uncached-descendants-3"), move_case("uncached-descendants-67", extra=64),
    move_case("resident-descendants-3", resident=1), move_case("resident-descendants-67", extra=64, resident=1),
    move_case("unrelated-resident-256", unrelated=256),
    NativeCase("workspace-namespace-deep-4097-uncached-native-v1", "phase_b_namespace::deep_move_back", "deep",
               budget_ns=15_000_000_000, ignored=True, environment=(("LAYERFS_PHASE_B_RESIDENT", 0),)),
    NativeCase("workspace-namespace-deep-4097-resident-native-v1", "phase_b_namespace::deep_move_back", "deep",
               budget_ns=15_000_000_000, ignored=True, environment=(("LAYERFS_PHASE_B_RESIDENT", 1),)),
    NativeCase("workspace-namespace-retained-live-g1-g2-native-v1", "phase_b_namespace::retained_live_namespace",
               budget_ns=15_000_000_000, ignored=True),
    NativeCase("workspace-namespace-rename-refund-native-v1", "a_successful_directory_rename_seals_once_and_refunds_exactly",
               budget_ns=15_000_000_000),
    NativeCase("workspace-namespace-rename-refusal-native-v1", "growing_rename_refuses_private_budget_before_publication",
               clones=2, budget_ns=15_000_000_000),
)}


@dataclass(frozen=True)
class SdkCase:
    id: str
    layout: str
    command: str
    budget_ns: int = 15_000_000_000
    prelude: str = ""


MOVE = ("umask 022 && mv packages/old/subtree packages/new/subtree && "
        "mv packages/new/subtree/child packages/new/subtree/renamed && "
        "mv packages/new/subtree/renamed packages/new/subtree/child && "
        "mv packages/new/subtree packages/old/subtree && mv packages/old/subtree packages/new/subtree && "
        "printf grand-new > packages/new/subtree/child/grand.txt.next && "
        "mv -f packages/new/subtree/child/grand.txt.next packages/new/subtree/child/grand.txt && "
        'test "$(cat packages/new/subtree/sibling.txt)" = sibling-base && '
        'test "$(ls packages/new/subtree | wc -l)" -eq 2')
DEEP = (f"set -e; umask 022 && a={'a' * 255} && b={'b' * 63} && d={'d' * 250} && "
        'exec 5<. && mkdir "$a" && cd "$a" && mkdir "$b" && cd "$b" && '
        'mv /proc/self/fd/5/src mmmmmmm && cd mmmmmmm && '
        'i=0; while [ $i -lt 15 ]; do exec 3<. && cd "/proc/self/fd/3/$d" || exit; i=$((i+1)); done; '
        'test "$(cat leaf)" = deep-base && printf deep-new > leaf.next && mv -f leaf.next leaf')
COMPONENTS = ('umask 022; i=0; while [ $i -lt 270 ]; do mkdir d || exit; exec 3<. || exit; '
              'cd /proc/self/fd/3/d || exit; i=$((i+1)); done; printf leaf > file && test "$(cat file)" = leaf')
SDK = {case.id: case for case in (
    SdkCase("workspace-namespace-move-replace-descendants-3-sdk-v1", "small", MOVE),
    SdkCase("workspace-namespace-move-replace-descendants-67-sdk-v1", "wide", MOVE),
    SdkCase("workspace-namespace-deep-4097-sdk-v1", "deep", DEEP, 25_000_000_000),
    SdkCase("workspace-namespace-components-270-sdk-v1", "small", COMPONENTS, 25_000_000_000),
    SdkCase("workspace-namespace-retained-g1-replace-sdk-v1", "small",
            "printf grand-new > packages/new/subtree/child/grand.txt.next && mv -f packages/new/subtree/child/grand.txt.next packages/new/subtree/child/grand.txt",
            prelude="mv packages/old/subtree packages/new/subtree"),
)}


def prepare(out, container, common, identity, binary, needed):
    return native.masters(out, container, common, identity, binary, needed, layouts=LAYOUTS)


def tree(layout):
    _, extra, deep, unrelated = LAYOUTS[layout]
    result = {name: None for name in ("", "packages", "packages/old", "packages/new",
                                     "packages/old/subtree", "packages/old/subtree/child")}
    result.update({"packages/old/subtree/child/grand.txt": b"grand-base", "packages/old/subtree/sibling.txt": b"sibling-base"})
    result.update({f"packages/old/subtree/child/extra{i:03}.txt": b"x" for i in range(extra)})
    if deep:
        name = "src"
        result[name] = None
        for _ in range(15):
            name += "/" + "d" * 250
            result[name] = None
        result[name + "/leaf"] = b"deep-base"
    if unrelated:
        result["unrelated"] = None
        result.update({f"unrelated/f{i:03}": b"u" for i in range(unrelated)})
    return result


def manifest(values):
    return "".join(f"{path or '.'}\t{'d' if data is None else 'f'}\t{493 if data is None else 420}\t"
                   f"{0 if data is None else len(data)}\t{'-' if data is None else hashlib.sha256(data).hexdigest()}\n"
                   for path, data in sorted(values.items()))


def oracle(case):
    old = tree(case.layout)
    old[".marker"] = b"baseline"
    new = dict(old)
    if case.command == COMPONENTS:
        path = ""
        for _ in range(270):
            path = path + "/d" if path else "d"
            new[path] = None
        new[path + "/file"] = b"leaf"
    elif case.command == DEEP:
        prefix = "a" * 255 + "/" + "b" * 63
        new["a" * 255] = None
        new[prefix] = None
        new = {prefix + "/mmmmmmm" + path[3:] if path == "src" or path.startswith("src/") else path: data for path, data in new.items()}
        leaf = prefix + "/mmmmmmm" + ("/" + "d" * 250) * 15 + "/leaf"
        assert len(leaf) == 4097
        new[leaf] = b"deep-new"
    else:
        new = {"packages/new/subtree" + path[len("packages/old/subtree"):] if path.startswith("packages/old/subtree") else path: data for path, data in new.items()}
        if case.prelude:
            old = dict(new)
        new["packages/new/subtree/child/grand.txt"] = b"grand-new"
    return manifest(old), manifest(new)


def copy_master(master, destination):
    source = Path(master["path"])
    for name, sha in master["files"].items():
        if shared.sha256(source / name) != sha:
            raise ValueError("namespace master seal changed")
        shutil.copyfile(source / name, destination / name)
        (destination / name).chmod(0o644)
        if shared.sha256(destination / name) != sha:
            raise ValueError("namespace independent copy mismatch")


def fields(master, scenario, command):
    fixture = dict(line.split("=", 1) for line in (Path(master["path"]) / "fixture.before").read_text().splitlines())
    return {"project_id": fixture["project_id"], "genesis_layer": fixture["genesis_layer"],
            "genesis_root": fixture["root"], "genesis_root_serial": fixture["root_serial"],
            "branch_id": fixture["branch_id"], "binding_key_hex": b"inherited-workspace".hex(),
            "component_oracle": "1", "scenario_id": scenario, "command_hex": command.encode().hex(),
            "expected_failure": "0", "telemetry_run": str(int.from_bytes(os.urandom(16), "big") or 1)}


def env():
    return {**os.environ, "LAYERFS_CONSTRUCTION_WORKERS": "1", "LAYERFS_HISTORY_CURSOR_KEY": CURSOR}


def sdk_master(out, layout, common, artifacts):
    from shell_package import case_spec

    source = common.RESULTS / "workspace-commit-native-master-v1" / layout
    sealed = json.loads((source / "prepared.json").read_text())
    key = common.seal([source / "fixture.before", source / "store.sqlite", source / "history.sqlite"])
    path = common.RESULTS / "workspace-namespace-sdk-master-v1" / key
    if (path / "prepared.json").exists():
        result = json.loads((path / "prepared.json").read_text())
        for name, sha in result["files"].items():
            if shared.sha256(path / name) != sha:
                raise ValueError("sealed SDK namespace master changed")
        return {**result, "reuse": True}
    path.mkdir(parents=True)
    copy_master(sealed, path)
    values = fields(sealed, "prepare-namespace-" + layout, "umask 022 && printf baseline > .marker")
    case_spec(path / "seed.case", values)
    record, stdout, stderr = shared.execute([artifacts["benchmark_shell"]["path"], "seed-existing", str(path / "seed.case"),
        str(path / "store.sqlite"), str(path / "history.sqlite"), artifacts["image_id"]], out / ("seed-" + layout), timeout=15, env=env())
    driver = shared.receipt_line(stdout)
    if record["exit_code"] != 0 or not driver or driver["status"] != "COMPLETE" or not commit.cleanup_complete(driver, commit.control_line(stdout), stderr):
        raise RuntimeError("SDK namespace seed failed; retained setup evidence")
    result = {"path": str(path), "old_commit": driver["head_commit"], "preparation": record,
              "producer_identity": artifacts["identity"], "producer_binary": artifacts["benchmark_shell"],
              "layout": layout, "source_master": sealed,
              "files": {name: shared.sha256(path / name) for name in ("store.sqlite", "history.sqlite", "fixture.before")}}
    for name in result["files"]:
        (path / name).chmod(0o444)
    shared.save(path / "prepared.json", result)
    return {**result, "reuse": False}


def sdk_attempt(out, case, master, prepared):
    from shell_package import case_spec

    folder = out / case.id
    folder.mkdir()
    copy_master(master, folder)
    values = fields(master, case.id, case.command)
    values["old_commit"] = master["old_commit"]
    if case.command == COMPONENTS:
        values.update(identity_old_prefix="", identity_new_prefix="")
    elif case.command == DEEP:
        values.update(identity_old_prefix="src", identity_new_prefix="a" * 255 + "/" + "b" * 63 + "/mmmmmmm",
                      identity_replaced="a" * 255 + "/" + "b" * 63 + "/mmmmmmm" + ("/" + "d" * 250) * 15 + "/leaf")
    else:
        values.update(identity_old_prefix="packages/new/subtree" if case.prelude else "packages/old/subtree",
                      identity_new_prefix="packages/new/subtree", identity_replaced="packages/new/subtree/child/grand.txt")
    if case.prelude:
        values.update(prelude_command_hex=case.prelude.encode().hex(), pin_path_hex=b"packages/new/subtree/child/grand.txt".hex(), pin_read_bytes="16384")
    case_spec(folder / "case.before", values)
    old, new = oracle(case)
    (folder / "old.tsv").write_text(old)
    (folder / "new.tsv").write_text(new)
    record, stdout, stderr = shared.execute([prepared["artifacts"]["benchmark_shell"]["path"], "run", str(folder / "case.before"),
        str(folder / "store.sqlite"), str(folder / "history.sqlite"), prepared["image_id"]], folder / "driver", timeout=case.budget_ns / 1e9, env=env())
    driver, control = shared.receipt_line(stdout), commit.control_line(stdout)
    cleanup = commit.cleanup_complete(driver, control, stderr)
    counts = dict(item.split("=", 1) for item in (driver or {}).get("projection_counts", "").split(",") if "=" in item)
    route = bool(driver and driver.get("commit_called") and int(counts.get("rename", 0)) >= (1 if case.command != COMPONENTS else 0))
    pin = not case.prelude or bool(control and control.get("pin_observation_ok") and control.get("pinned_bytes") == 10
        and control.get("pinned_sha256") == hashlib.sha256(b"grand-base").hexdigest() and control.get("pin_release_ok"))
    complete = bool(record["exit_code"] == 0 and not record["timeout"] and record["wall_ns"] <= case.budget_ns and driver and driver.get("status") == "COMPLETE" and route and cleanup and pin)
    if driver and driver.get("head_commit"):
        if case.prelude:
            values.update(old_commit=control["prelude_head_commit"], expected_old_parent=master["old_commit"])
        case_spec(folder / "case.verify", {**values, "expected_head_commit": driver["head_commit"]})
    row = {"case": case.id, "source": prepared["identity"], "profile": PROFILE, "sample_count": 1,
           "route": "public WorkspaceApi Mount/Exec/Commit/Status/unmount/delete; POSIX-FUSE",
           "master": master, "artifacts": prepared["artifacts"], "image_id": prepared["image_id"],
           "performance": record, "command_budget_ns": case.budget_ns,
           "command_status": "PASS" if not record["timeout"] and record["wall_ns"] <= case.budget_ns else "FAIL",
           "driver": driver, "control": control, "route_status": "PASS" if route else "FAIL", "pin_status": "PASS" if pin else "FAIL",
           "cleanup_status": "PASS" if cleanup else "UNKNOWN" if record["timeout"] else "FAIL",
           "clone_method": "closed independent writable byte copy; SHA256 identity checks outside timer",
           "cache_contract": "host and daemon cache uncontrolled; clone does not establish cold state",
           "numeric_latency_status": "INELIGIBLE", "performance_claim": False,
           "verification": {"status": "SKIPPED", "reason": "separate prove, without performance replay"},
           "status": "COMPLETE_DIAGNOSTIC" if complete else "FAIL"}
    shared.save(folder / "receipt.json", row)
    return row


def run(selection, output, common):
    if selection in NATIVE or selection == "workspace-namespace-native":
        return native.run(selection, output, common, cases=NATIVE, selected=tuple(NATIVE) if selection not in NATIVE else (selection,), profile=PROFILE, schema=NATIVE_SCHEMA, prepare=prepare)
    out = common.owned(output)
    identity = common.identities()
    if identity["source_dirty"]:
        raise ValueError("commit Family5 before collection")
    out.mkdir(parents=True)
    selected = tuple(SDK) if selection in ("workspace_namespace", "workspace-namespace-sdk") else (selection,)
    summary = {"schema": SCHEMA, "profile": PROFILE, "identity": identity, "selected": list(selected), "rows": [], "status": "INCOMPLETE"}
    try:
        with (common.RESULTS / ".run.lock").open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            prior = json.loads((common.RESULTS / "issue286-workspace-commit-fast-checkpoint-r062/prepared.json").read_text())
            prepared = shared.build(out, common, identity, reuse_image=prior, artifacts_only=True)
            summary["prepared"] = prepared
            masters = {layout: sdk_master(out, layout, common, {**prepared["artifacts"], "image_id": prepared["image_id"], "identity": identity}) for layout in dict.fromkeys(SDK[name].layout for name in selected)}
            for name in selected:
                row = sdk_attempt(out, SDK[name], masters[SDK[name].layout], prepared)
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
    common.verify_run_manifest(run)
    source = json.loads((run / "run.json").read_text())
    out = common.owned(output)
    out.mkdir(parents=True)
    summary = {"schema": PROOF_SCHEMA, "run": str(run), "identity": source["identity"], "rows": []}
    with (common.RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for item in source["rows"]:
            folder = run / item["case"]
            receipt = json.loads((folder / "receipt.json").read_text()) if (folder / "receipt.json").exists() else None
            if not receipt or receipt["status"] != "COMPLETE_DIAGNOSTIC":
                summary["rows"].append({"case": item["case"], "status": "NOT_RUN", "reason": "no complete performance receipt"})
                continue
            verifier = receipt["artifacts"]["verify_checkpoint5"]
            if shared.sha256(verifier["path"]) != verifier["sha256"]:
                raise ValueError("verifier binary seal changed")
            record, stdout, _ = shared.execute([verifier["path"], str(folder / "case.verify"), str(folder / "store.sqlite"),
                str(folder / "history.sqlite"), str(folder / "old.tsv"), str(folder / "new.tsv")], out / item["case"], timeout=9, env=env())
            child = json.loads(stdout) if record["exit_code"] == 0 else None
            row = {"case": item["case"], "verification": record, "verifier_budget_ns": 9_000_000_000,
                   "child": child, "status": "PASS" if child and child.get("status") == "PASS" and child.get("advanced") and record["wall_ns"] < 9_000_000_000 else "FAIL",
                   "scope": "independent read-only full old/new tree modes and bytes, known Commit/parent; serial traversal",
                   "performance_receipt_sha256": shared.sha256(folder / "receipt.json")}
            summary["rows"].append(row)
    summary["status"] = "PASS" if all(row["status"] == "PASS" for row in summary["rows"]) else "INCOMPLETE"
    shared.save(out / "run.json", summary)
    common.manifest_run(out)
    return out


def report(out):
    data = json.loads((out / "run.json").read_text())
    lines = ["| Case | Complete command / bound ns | Exec ns | Commit ns | Correctness | Cleanup | Numeric cache |", "| --- | ---: | ---: | ---: | --- | --- | --- |"]
    for item in data["rows"]:
        row = json.loads((out / item["case"] / "receipt.json").read_text()) if (out / item["case"] / "receipt.json").exists() else item
        driver = row.get("driver") or {}
        wall = row.get("performance", row.get("functional", row.get("verification", {}))).get("wall_ns", "UNAVAILABLE")
        lines.append(f"| {item['case']} | {wall} / {row.get('command_budget_ns', row.get('functional_budget_ns', row.get('verifier_budget_ns', 'UNAVAILABLE')))} | {driver.get('exec_ns', 'N/A')} | {driver.get('commit_ns', 'N/A')} | {row['status']} | {row.get('cleanup_status', 'N/A')} | INELIGIBLE |")
    return "\n".join(lines) + "\n"
