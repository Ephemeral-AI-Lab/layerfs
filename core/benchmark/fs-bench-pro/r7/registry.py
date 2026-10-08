"""One canonical exploratory R7 matrix; registration is product-free.

Emit this registry before any measurement. Unrun and inapplicable arms remain
visible; no numerical performance target or release-admission claim is defined.
"""
import argparse
import hashlib
import json
from pathlib import Path

from .workloads import COMMANDS, FIXTURE_COMMIT, workload, workload_identity

SCHEMA = "r7-optimization-registry-v1"
IMAGE = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6"
DIAGNOSTIC_STOP_NS = 120_000_000_000
PERFORMANCE_STOP_NS = 15_000_000_000
VERIFIER_STOP_NS = 9_500_000_000
DIAGNOSTIC = {"E03", "E12", "E13", "E14"}
READS = {"E02", "E04", "E05", "E06", "E07", "E18", "E19"}
WRITES = {"E10", "E11", "E15", "E16", "E17"}
PHASES = {
    "setup": {"start": "clone/reuse begins", "end": "closed setup ready", "product_time": False},
    "mount": {"start": "SDK Mount invocation", "end": "Attach acknowledged Ready", "product_time": True},
    "command": {"start": "external bash launch", "end": "actual exit/status", "product_time": True},
    "streams": {"start": "external bash launch", "end": "stdout/stderr EOF, failure or disposal", "product_time": True, "overlaps": "command"},
    "commit": {"start": "SDK Commit invocation", "end": "original known success/failure/unknown reply", "product_time": True},
    "unmount": {"start": "explicit SDK Unmount invocation", "end": "original terminal reply", "product_time": True},
    "cleanup": {"start": "Unmounted reply", "end": "Gone observed or exact failure", "product_time": False, "reported_separately": True},
    "verifier": {"start": "independent verifier launch", "end": "verifier exit", "product_time": False},
}
CACHE = {
    "A": {
        "definition": "fresh mount; empty daemon immutable cache; new Store connections and overlay; fresh kernel connection",
        "arms": ["L", "N", "P"],
        "requirements": ["per-file eviction hint then mincore residency observation of all Store files and sidecars before attempt",
                         "nonzero resident pages: INELIGIBLE, attempted_operation_count=0",
                         "native/passthrough fixture file data residency also enforced; no VM-wide drop"],
    },
    "B": {
        "definition": "fresh mount, same daemon after untimed identical earlier-mount call and terminal unmount; fresh kernel connection",
        "arms": ["L"],
        "requirements": ["measured-phase object demands are zero", "Store residency for paid history read declared",
                         "earlier warm-up mount and terminal outcome recorded; no N/P analogue"],
    },
    "C": {
        "definition": "same mount after declared untimed identical call; warm native caches",
        "arms": ["L", "N", "P"],
        "requirements": ["warm-up end to measured start < 60000000000 ns", "opcode receipts",
                         "natural own-write effects and all file/SQL/reader/immutable/kernel cache scopes declared"],
    },
}
COUNTERS = ["opcode_work", "native_work", "dispatch_work", "mount_work", "owner_work",
            "statement_work", "store_work", "reader_work", "construction_work", "storage_diagnostics",
            "global_sql_work", "stored_counts", "scheduler_bytes", "peak_credited_bytes",
            "immutable_cache_allowance_bytes", "daemon_vmhwm_bytes", "store_logical_bytes",
            "store_allocated_bytes", "overlay_logical_bytes", "overlay_allocated_bytes", "cleanup_debt"]
SCALING = {
    "factors": [1, 2, 4], "mode": "counts-only; never timing resampling",
    "dimensions": ["base size with fixed change", "changed rows", "changed files", "changed bytes",
                   "huge directory", "deep tree", "hard-link aliases", "file fragmentation",
                   "tiny appends", "truncate and regrow", "incremental Commit history",
                   "mount/unmount cycles", "concurrent Workspaces", "concurrent processes",
                   "lookups outstanding at unmount", "cleanup debt"],
    "required_evidence": "per-unit counts and growth operands at 1x/2x/4x; each measured/changed path has bounded count test at two sizes",
}


def cases():
    result = []
    for case in COMMANDS:
        classes = (["A", "B"] if case == "E01" else ["A", "B", "C"] if case in READS
                   else ["B", "C"] if case in WRITES or case.startswith("C")
                   else ["A", "C"] if case in DIAGNOSTIC else [None])
        base = "empty" if case.startswith("C") else "full-minus-dependencies" if case in {"E12", "E13"} else "full"
        result.append({"case_id": case, "set": "priced-diagnostic" if case in DIAGNOSTIC else
                       "not-run" if case in {"E08", "E09"} else "fixed-cost" if case == "E01" else "read-metadata" if case in READS else "write-churn",
                       "classes": classes, "fixture": base, "workload": workload(case),
                       "label": "unrefreshed index" if case == "E18" else case,
                       "complete_command_wall_stop_ns": DIAGNOSTIC_STOP_NS if case in DIAGNOSTIC else PERFORMANCE_STOP_NS,
                       "numeric_budget_verdict": False if case in DIAGNOSTIC else True,
                       "verifier_wall_stop_ns": VERIFIER_STOP_NS,
                       "oracle_scope": "scoped" if not case.startswith("C") and case != "E02" else "complete-declared-root",
                       "not_run_reason": "owner excluded TypeScript build" if case == "E08" else
                       "no authorized normalized oracle" if case == "E09" else None,
                       "prerequisites": prerequisites(case)})
    for case, scenario in [
        ("K01", "small change Commit"), ("K02", "large change Commit after E12"),
        ("K03", "large change Commit after E13"), ("K04", "five incremental Commits on one mount"),
        ("K05", "Commit then fresh-mount survival"),
        ("W01", "two Workspaces: one scanning and one writing"),
        ("W02", "several sequential and concurrent external commands on one Workspace"),
    ]:
        result.append({"case_id": case, "set": "commit" if case.startswith("K") else "concurrency",
                       "classes": [None] if case.startswith("K") else ["C"],
                       "fixture": "full-minus-dependencies" if case in {"K02", "K03"} else "full",
                       "scenario": scenario, "workload": scenario_workload(case),
                       "complete_command_wall_stop_ns": PERFORMANCE_STOP_NS,
                       "verifier_wall_stop_ns": VERIFIER_STOP_NS, "numeric_budget_verdict": True,
                       "oracle_scope": "scoped", "not_run_reason": None,
                       "prerequisites": ["independent expected manifests sealed from declared native command/schedule before selection",
                                         "Commit construction workers=1; command is never registered with daemon"]})
    return result


def scenario_workload(case):
    small = "mkdir r7-small; for i in $(seq 1 24); do printf 'r7-%s\\n' \"$i\" > r7-small/f$i; done"
    if case in {"K01", "K05"}:
        return {"commands": [small], "commit_count": 1, "fresh_mount_oracle": case == "K05",
                "oracle": "exact r7-small names, metadata and all 24 payloads; scoped, fresh mount when selected"}
    if case in {"K02", "K03"}:
        dependency = "E12" if case == "K02" else "E13"
        return {"commands": [workload(dependency)["argv"]], "commit_count": 1, "fixture": "full-minus-dependencies",
                "oracle": workload(dependency)["oracle"], "fresh_mount_oracle": True}
    if case == "K04":
        return {"commands": ["printf 'r7-increment-%s\\n' '" + str(i) + "' > r7-increment-" + str(i) for i in range(1, 6)],
                "commit_count": 5, "schedule": "same mount; command i then Commit i; no overlap",
                "oracle": "five exact files and full payloads after each known Commit; same mount and final fresh mount",
                "fresh_mount_oracle": True}
    if case == "W01":
        return {"commands": [{"workspace": "scan", "argv": workload("E02")["argv"]},
                             {"workspace": "write", "argv": workload("E16")["argv"]}],
                "schedule": "two ready Workspaces over same closed full root, external commands launched concurrently",
                "oracle": "scan full name/kind manifest and exact stdout; writer log exactly1000000 bytes; per-Workspace progress",
                "commit_count": 0}
    return {"commands": [{"workspace": "shared", "argv": workload("E01")["argv"], "stage": 0},
                         {"workspace": "shared", "argv": workload("E06")["argv"], "stage": 1},
                         {"workspace": "shared", "argv": workload("E16")["argv"], "stage": 1}],
            "schedule": "one retained mount; stage0 completes, then both stage1 external commands run concurrently",
            "oracle": "E06 exact stdout; E16 exact1000000 byte log; complete command results and per-request progress",
            "commit_count": 0}


def prerequisites(case):
    values = ["release binary and pinned image", "closed independently writable setup clone or qualified shared Store"]
    if case in {"E12", "E13", "E14"}:
        values.append("sealed /replay/F and /replay/master.json, /code/node-roots.json; no network")
    if case == "E15":
        values.append("sealed /code/largest-path, matching owned fixture file")
    if case == "E19":
        values.append("282 of 14090 tracked files changed via mounted operations and Commit, or two declared host-prepared related roots; expected stdout rederived")
    if case == "E18":
        values.append("unrefreshed index only; no refreshed-index fast-path claim")
    if case in {"C09", "C10", "C11"}:
        values.append("64 MiB big precondition from empty root; survives fresh mount via setup Commit/Init; enforce phase's cache class, never own-write cold credit")
    if case not in {"E01", "E08", "E09"} and not case.startswith("C"):
        values.append("full fixture or explicitly declared reduced cut in every receipt")
    return values


def selections():
    result = []
    for case in cases():
        for cls in case["classes"]:
            for arm in ("L", "N", "P"):
                reason = case["not_run_reason"]
                if cls == "B" and arm != "L":
                    reason = "cache class B is L-only; native/passthrough has no analogue"
                if case["set"] == "commit" and arm != "L":
                    reason = reason or "native/passthrough has no canonical Store Commit; command-only control can be separately recorded"
                if cls == "C" and case["case_id"] in {"E12", "E13", "C12"}:
                    reason = reason or "strict identical-call same-mount warm-up changes non-repeatable input: existing symlinks/.experiment-store or no new Git commit; retain conflict, no guessed preconditioning"
                result.append({"selection_id": f"{case['case_id']}:{cls or 'none'}:{arm}",
                               "case_id": case["case_id"], "cache_class": cls, "arm": arm,
                               "status": "NOT_RUN", "reason": reason or "prospectively registered; not attempted",
                               "admission_eligible": False, "sample_count": 0})
    return result


def registration():
    return {"schema": SCHEMA, "family_id": "r7-optimization", "scenario_version": 1,
            "mode": "exploratory", "admission_eligible": False, "sample_count_per_identity": 1,
            "image_id": IMAGE, "store_profile": "Disposable/WAL/OFF", "overlay_profile": "MEMORY/OFF/EXCLUSIVE",
            "durable": "NOT_RUN — disabled by owner until explicit reauthorization",
            "fixture": {"commit": FIXTURE_COMMIT, "historical_files": 103108,
                        "historical_size_label": "3.237 GiB", "exact_bytes": None,
                        "policy": "owned byte copy only; never execute in or write to source checkout"},
            "phases": PHASES, "cache_classes": CACHE, "required_counters": COUNTERS,
            "scaling": SCALING, "workload_identity": workload_identity(), "cases": cases(), "selections": selections(),
            "arm_order": ["N", "P", "L"], "control_reuse": "once per exact N/P identity; no product-change resampling",
            "loop_set": "lead selects after baseline, weighted gap; at least mount/unmount, read/metadata, write, cold read, small/large Commit",
            "full_matrix_schedule": "baseline, when loop set stops improving, final",
            "priced_stop_decision": "120 s prospective bounded diagnostic stop; no pass/fail latency budget; no inherited 60/600 s scope"}


def registry_identity():
    return hashlib.sha256(json.dumps(registration(), sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    row = registration()
    row["registry_identity"] = registry_identity()
    with args.output.open("x") as stream:
        json.dump(row, stream, indent=2, sort_keys=True)
        stream.write("\n")


if __name__ == "__main__":
    main()
