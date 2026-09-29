"""Compound retained-history registry and thin adapter to the existing Rust driver."""
from dataclasses import dataclass
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parents[1]
ROOT = HERE.parents[2]
BACKEND = HERE.parent / "fs-bench-pro-storage-content"
sys.path.insert(0, str(BACKEND / "shared"))
import history_corpus as corpus  # noqa: E402
import trace as trace_module  # noqa: E402
from shared import history_storage as storage  # noqa: E402

ENV = {"LAYERFS_CONSTRUCTION_WORKERS": "1",
       "LAYERFS_HISTORY_ADVISORY": "1", "LAYERFS_HISTORY_CHUNK_PREDECESSORS": "1",
       "LAYERFS_HISTORY_FULL_PRODUCER": "0", "LAYERFS_HISTORY_ORDERED_PREDECESSORS": "0",
       "LAYERFS_HISTORY_SIMILARITY_CANDIDATES": "0", "LAYERFS_HISTORY_DEPTH_LIMIT": "255",
       "LAYERFS_HISTORY_PHASES": "0"}


@dataclass(frozen=True)
class Case:
    id: str
    backend_id: str
    states: int
    ceiling_bytes: int
    command_budget_ns: int
    verification_budget_ns: int

    @property
    def version(self):
        return self.id.rsplit("-", 1)[1]

    @property
    def profile(self):
        return f"c1-c2-c5-retained-history-{self.version}"

    @property
    def pin_root(self):
        return ROOT / f"core/docs/issues/286/oracles/history-reference-{self.version}"


# Limits frozen from the independent stride10 baseline, with the owner's
# separate <3-minute command constraint: min(170s, ceil(1.25 * 46.799667833s
# * states/17 / 5s) * 5s). Time remains diagnostic; strict storage is the gate.
CASES = {case.id: case for case in (
    Case("history-retention-stride-10-total-storage-v1", "history-stride10", 17,
         49_344_512, 60_000_000_000, 10_000_000_000),
    Case("history-retention-stride-3-total-storage-v1", "history-stride3", 53,
         64_024_576, 170_000_000_000, 20_000_000_000),
    Case("history-retention-stride-1-total-storage-v1", "history-stride1", 157,
         83_947_520, 170_000_000_000, 30_000_000_000),
    Case("history-retention-stride-10-total-storage-v2", "history-stride10", 17,
         49_344_512, 60_000_000_000, 10_000_000_000),
    Case("history-retention-stride-3-total-storage-v2", "history-stride3", 53,
         64_024_576, 170_000_000_000, 20_000_000_000),
    Case("history-retention-stride-1-total-storage-v2", "history-stride1", 157,
         83_947_520, 170_000_000_000, 30_000_000_000),
)}
SELECTED = tuple(name for name in CASES if name.endswith("-v2"))[:2]


def identity(common):
    value = common.identities()
    native = list(BACKEND.glob("src/**/*.rs"))
    oracle_counts = BACKEND / "src/workload/history-oracle-counts-v1.tsv"
    native += [BACKEND / "Cargo.toml", BACKEND / "Cargo.lock", oracle_counts]
    native += list((ROOT / "core/crates").glob("**/src/**/*.rs"))
    native += list((ROOT / "core/crates").glob("**/sql/**/*.sql"))
    native += list((ROOT / "core/crates").glob("**/Cargo.toml"))
    native += [ROOT / "core/Cargo.toml", ROOT / "core/Cargo.lock", ROOT / ".cargo/config.toml"]
    value["compilation_seal"] = common.seal(native)
    value["harness_seal"] = common.seal(list(HERE.glob("**/*.py")) + list(BACKEND.glob("src/**/*.rs")) + [oracle_counts])
    value["dependency_seal"] = common.seal([ROOT / "core/Cargo.lock", BACKEND / "Cargo.lock"])
    value["root_cargo_config_sha256"] = common.digest(ROOT / ".cargo/config.toml")
    return value


def build(out, common, identities):
    cache = common.RESULTS / "history-build-release.json"
    prior = json.loads(cache.read_text()) if cache.exists() else {}
    binary = Path(prior.get("binary", {}).get("path", "/absent"))
    if prior.get("compilation_seal") == identities["compilation_seal"] and binary.is_file() and common.digest(binary) == prior["binary"]["sha256"]:
        return {**prior, "mode": "exact-binary-reuse", "wall_ns": 0, "command": None}
    command = ["cargo", "+1.85.1", "build", "--release", "--locked", "--manifest-path",
               str(BACKEND / "Cargo.toml"), "--bin", "fs-bench-storage-content"]
    start = time.monotonic_ns()
    with (out / "build.log").open("xb") as log:
        result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            env={**os.environ, "CARGO_TARGET_DIR": str(common.target_path())}, timeout=170)
    record = {"command": command, "wall_ns": time.monotonic_ns() - start,
              "status": "PASS" if result.returncode == 0 else "FAIL", "exit_code": result.returncode,
              "compilation_seal": identities["compilation_seal"], "mode": "worktree-local-incremental",
              "build_profile": "release", "dependency_reuse": "same worktree's locked Cargo target"}
    if result.returncode == 0:
        source = common.target_path() / "release/fs-bench-storage-content"
        sha = common.digest(source)
        binary = common.RESULTS / "binary-archive" / sha / source.name
        binary.parent.mkdir(parents=True, exist_ok=True)
        if not binary.exists():
            shutil.copy2(source, binary)
            binary.chmod(0o555)
        record["binary"] = {"path": str(binary), "sha256": sha}
        common.write_json(cache, record)
    return record


def invoke(command, folder, label, budget_ns, environment):
    started = time.monotonic_ns()
    with (folder / f"{label}.stdout").open("xb") as out, (folder / f"{label}.stderr").open("xb") as err:
        try:
            child = subprocess.run(command, cwd=ROOT, env=environment, stdout=out, stderr=err,
                                   timeout=budget_ns / 1e9)
            record = {"exit_code": child.returncode, "timed_out": False}
        except subprocess.TimeoutExpired:
            record = {"exit_code": None, "timed_out": True}
    return {**record, "command": command, "wall_ns": time.monotonic_ns() - started,
            "budget_ns": budget_ns}


def semantic_gates_pass(gates, resource_gate):
    semantic = [gate for gate in gates if gate.identifier != resource_gate]
    return bool(semantic) and all(gate.status == "PASS" for gate in semantic)


def case_run(out, case, common, identities, binary):
    folder = out / case.id
    folder.mkdir()
    pin_path = case.pin_root / f"{case.backend_id}.tsv"
    pins = json.loads((case.pin_root / "manifest.json").read_text())
    expected = pins["cases"][case.backend_id]
    if common.digest(pin_path) != expected["roots_sha256"]:
        raise ValueError("independent root ledger seal mismatch")
    method_env = {**ENV, "LAYERFS_HISTORY_RETAINED_CATALOG": case.version[-1]}
    environment = {**os.environ, **method_env, "LAYERFS_HISTORY_ROOT_PINS": str(pin_path),
                   "LAYERFS_HISTORY_ROOT_PINS_SHA256": expected["roots_sha256"]}
    native = folder / "native"  # The existing driver creates this fresh directory.
    command = [binary["path"], "--case", case.backend_id, "--corpus", str(corpus.DEFAULT_ROOT),
               "--out", str(native)]
    record = {"schema": f"core-history-retention-receipt-{case.version}", "family": "history_retention",
              "case": case.id, "historical_backend_id": case.backend_id, "profile": case.profile,
              "verifier_method": "complete-listed-tree+c2-stored-lengths+selected-public-digests+8MiB-verified-page-identity-v2d",
              "verification_identity_reuse": "verifier-only, empty-start, at-most-8MiB authenticated C1 page ObjectId memo; trace hit/read/peak counters",
              "identity": identities, "binary": binary, "corpus": corpus.identity(),
              "root_ledger": expected, "env": method_env, "sample_count": 0,
              "construction_workers": 1, "setup": "InProcess", "clone_method": None,
              "cache_contract": "fresh-growing-store; untimed corpus reads; no cold time claim",
              "corpus_oracle_count_reuse": "committed 157-row SHA/count ledger; actual selected oracle bytes hashed each invocation",
              "reused_proof_identities": [], "numeric_time_eligibility": "INELIGIBLE",
              "storage_gate": (storage.GATE if case.version == "v1" else "g1.o6-total-retained-below-v016-v2"),
              "admission_eligible": False,
              "competing_work": common.competing_work()}
    common.write_json(folder / "declaration.json", record)
    performance = invoke(command, folder, "perf", case.command_budget_ns, environment)
    record["performance"] = performance
    record["attempted_invocation_count"] = 1
    record["sample_count"] = 1 if (native / "timing.json").is_file() else None
    if (native / "phases-perf.json").is_file():
        record["phases"] = json.loads((native / "phases-perf.json").read_text())
    record["storage"] = storage.collect(native, case.ceiling_bytes, case.states, version=case.version)
    canonical = record["storage"]["owners"].get("C2", {})
    fields = ("canonical_bytes", "canonical_objects")
    required = (expected["canonical_bytes"], expected["canonical_objects"]) if case.version == "v1" else expected["canonical_required"]
    record["canonical_status"] = ("INCOMPLETE" if any(type(canonical.get(field)) is not int for field in fields)
        else "PASS" if all(canonical[field] == pin for field, pin in zip(fields, required)) else "FAIL")
    roots = trace_module.read(native / "trace.jsonl").values("counter") if (native / "trace.jsonl").is_file() else {}
    complete = sum(key.startswith("history.state.") and key.endswith(".root") for key in roots) == case.states
    if not complete:
        record["canonical_status"] = "INCOMPLETE"
    if complete and not performance["timed_out"]:
        verification = invoke(command + ["--phase", "verify"], folder, "verify",
                              case.verification_budget_ns, environment)
        parsed = trace_module.read(native / "trace.jsonl")
        gates = parsed.gates()
        record["native_gates"] = [{"id": gate.identifier, "status": gate.status,
                                   "measured": gate.measured, "limit": gate.limit,
                                   "class": gate.gate_class} for gate in gates]
        record["observed_counters"] = parsed.counters()
        record["correctness_status"] = "PASS" if (
            verification["exit_code"] == 0 and not verification["timed_out"]
            and verification["wall_ns"] <= case.verification_budget_ns and not parsed.defects
            and semantic_gates_pass(gates, record["storage_gate"])
            and record["canonical_status"] == "PASS"
            and record["storage"].get("retained_counts_status") == "PASS") else "FAIL"
        record["verification"] = verification
    else:
        record["correctness_status"] = "INCOMPLETE"
        record["verification"] = {"status": "NOT_RUN", "reason": "no complete driver outcome"}
    record["command_budget_status"] = "PASS" if performance["wall_ns"] <= case.command_budget_ns and not performance["timed_out"] else "TARGET_MISS"
    record["cleanup_status"] = "PASS" if performance["exit_code"] == 0 and not performance["timed_out"] and not record["storage"]["issues"] else "INCOMPLETE"
    record["status"] = "PASS" if all(record[key] == "PASS" for key in (
        "correctness_status", "command_budget_status", "cleanup_status")) and record["storage"]["status"] == "PASS" else "FAIL"
    common.write_json(folder / "receipt.json", record)
    return record


def run(selection, out, common):
    out = common.owned(out)
    identities = identity(common)
    if identities["source_dirty"]:
        raise ValueError("commit product/harness/profile/independent pins before collection")
    # No silent substitute for an unapproved or incomplete oracle ledger.
    for case in CASES.values():
        json.loads((case.pin_root / "manifest.json").read_text())
    out.mkdir(parents=True)
    common.RESULTS.mkdir(parents=True, exist_ok=True)
    with (common.RESULTS / ".run.lock").open("a+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        built = build(out, common, identities)
        common.write_json(out / "build.json", built)
        selected = [selection] if selection in CASES else list(SELECTED)
        for name in selected:
            if built["status"] == "PASS":
                try:
                    case_run(out, CASES[name], common, identities, built["binary"])
                except Exception as error:
                    folder = out / name
                    folder.mkdir(exist_ok=True)
                    common.write_json(folder / "receipt.json", {"case": name, "status": "INCOMPLETE",
                        "sample_count": 1 if (folder / "native/timing.json").is_file() else None,
                        "error": repr(error), "identity": identities})
        for name in CASES:
            if not (out / name).exists():
                (out / name).mkdir()
                common.write_json(out / name / "receipt.json", {"case": name,
                    "status": "NOT_RUN", "reason": "explicit run-only tier or unselected case", "sample_count": 0})
    common.write_json(out / "run.json", {"schema": "core-history-retention-run-v2",
        "selection": selected, "identity": identities,
        "profiles": sorted({CASES[name].profile for name in selected})})
    (out / "report.txt").write_text(report(out))
    common.manifest_run(out)
    return out


def report(run):
    rows = ["case\tstatus\tcommand_ns\tverifier_ns\tallocated_bytes\tstorage_gate"]
    for name in CASES:
        value = json.loads((run / name / "receipt.json").read_text())
        rows.append("\t".join(map(str, (name, value["status"],
            value.get("performance", {}).get("wall_ns"), value.get("verification", {}).get("wall_ns"),
            value.get("storage", {}).get("total_retained_allocated_bytes"), value.get("storage", {}).get("status")))))
    return "\n".join(rows) + "\n"


def verify(run, common):
    # Retained-evidence rederivation only; the separately bounded native verifier
    # has already reopened public C5 and traversed the complete C1 namespace.
    manifest = json.loads((run / "manifest.json").read_text())
    files = {str(p.relative_to(run)): p for p in run.rglob("*") if p.is_file() and p.name != "manifest.json"}
    if set(files) != set(manifest["files"]):
        raise ValueError("history evidence inventory mismatch")
    for name, path in files.items():
        item = manifest["files"][name]
        if path.stat().st_size != item["bytes"] or common.digest(path) != item["sha256"]:
            raise ValueError(f"retained history evidence changed: {name}")
    for name, case in CASES.items():
        receipt = json.loads((run / name / "receipt.json").read_text())
        if receipt["status"] != "NOT_RUN":
            actual = storage.collect(run / name / "native", case.ceiling_bytes, case.states,
                                     version=case.version)
            if actual != receipt["storage"]:
                raise ValueError("compound storage gate rederivation mismatch")
            if receipt["status"] != "PASS":
                raise ValueError(f"nonpassing history case: {name}")
    return "PASS"
