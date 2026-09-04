#!/usr/bin/env python3
"""Phase 1 adapter: sealed sources, qualified inputs, isolated samples and retained outcomes."""
import argparse
import contextlib
import fcntl
import gzip
import hashlib
import importlib.util
import json
import os
import platform
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import uuid

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
spec = importlib.util.spec_from_file_location("custody", HERE / "sdk-edit-custody.py")
custody = importlib.util.module_from_spec(spec)
spec.loader.exec_module(custody)
EXTENDED = {"admission-batch-failure-retry", "final-publication-failure-retry", "corrupt-descendant", "missing-descendant", "exec-500", "sustained-600s"}
FIXED_LARGE = {"tiny-create", "tiny-stat", "tiny-unlink", "directory-construct", "workspace-distributed-sdk-edit", "workspace-dense-rewrite", "namespace-subtree-relocate-delete"}
LOG_LIMIT = 64 * 1024 * 1024


PHASE1_PRODUCT_LIMIT_NS = 15_000_000_000
SUPPRESSION_POLICY_PATH = REPO / "docs/roadmap/0.1/0.1.3/phase-1-runtime-suppressions.md"
INITIAL_SUPPRESSED_CASES = frozenset({
    "dedup-history-unrelated-500", "workspace-dense-rewrite-500", "tiny-bulk-create-500",
    "dedup-history-unrelated-100", "directory-content-scan-500", "workspace-dense-rewrite-100",
    "namespace-subtree-relocate-delete-500", "tiny-bulk-delete-500", "tiny-bulk-create-100",
    "git-tool-500", "git-tool-1", "git-tool-100", "git-tool-10", "dedup-history-distributed-500",
})
SUPPRESSION_STATUS = "suppressed_phase1_time_budget"


def load_suppressions(campaign):
    path = Path(campaign) / "phase1-runtime-suppressions.json"
    value = read_json(path) if path.exists() else {"schema": "phase1-runtime-suppressions-v1", "limit_ns": PHASE1_PRODUCT_LIMIT_NS, "cases": {}}
    if value.get("schema") != "phase1-runtime-suppressions-v1" or value.get("limit_ns") != PHASE1_PRODUCT_LIMIT_NS or not isinstance(value.get("cases"), dict):
        raise ValueError("invalid persistent Phase1 suppression ledger")
    for case, record in value["cases"].items():
        if record.get("scenario_id") != case or record.get("status") != SUPPRESSION_STATUS:raise ValueError("invalid suppression identity/status")
    changed = False
    for case in sorted(INITIAL_SUPPRESSED_CASES - value["cases"].keys()):
        value["cases"][case] = {"scenario_id": case, "status": SUPPRESSION_STATUS, "origin": "user-initial",
            "reason": "Explicit user-authorized initial14 Phase1 runtime exclusion; no confirmation run required", "at_unix_ns": time.time_ns(),
            "policy_sha256": custody.sha(SUPPRESSION_POLICY_PATH), "limit_ns": PHASE1_PRODUCT_LIMIT_NS}
        changed = True
    if changed or not path.exists():atomic_json(path, value)
    return value


def is_suppressed(case, ledger):
    return not case.get("proof_only") and case["scenario_id"] in ledger["cases"]


def product_budget_observation(event):
    value = event.get("cumulative_ns")
    if event.get("kind") != "product-time-budget-exceeded" or event.get("limit_ns") != PHASE1_PRODUCT_LIMIT_NS or type(value) is not int or value <= PHASE1_PRODUCT_LIMIT_NS or event.get("measurement") not in {"active-pure-call-sum", "completed-pure-call-sum"} or not isinstance(event.get("phase"), str):
        raise ValueError("invalid authoritative product-budget event")
    if event["measurement"] == "active-pure-call-sum":
        completed, active = event.get("completed_product_ns"), event.get("active_phase_ns")
        if type(completed) is not int or type(active) is not int or min(completed, active) < 0 or completed + active != value:
            raise ValueError("product-budget clock equation mismatch")
    return value


def record_suppression(campaign, case, source, seed, evidence, event):
    if case.get("proof_only"):raise ValueError("standalone proof is exempt from performance suppression")
    if event.get("scenario_id") != case["scenario_id"] or event.get("limit_ns") != PHASE1_PRODUCT_LIMIT_NS or type(event.get("observed_product_ns")) is not int or event["observed_product_ns"] <= PHASE1_PRODUCT_LIMIT_NS:
        raise ValueError("invalid measured product-budget trigger")
    ledger = load_suppressions(campaign)
    if case["scenario_id"] not in ledger["cases"]:
        ledger["cases"][case["scenario_id"]] = {"scenario_id": case["scenario_id"], "status": SUPPRESSION_STATUS,
            "origin": "measured-product-budget", "reason": "One measured performance sample exceeded the cumulative15-second product budget",
            "at_unix_ns": time.time_ns(), "policy_sha256": custody.sha(SUPPRESSION_POLICY_PATH), "limit_ns": PHASE1_PRODUCT_LIMIT_NS,
            "source_revision": source, "seed": seed, "mode": "performance", "evidence_path": str(evidence),
            "observed_product_ns": event["observed_product_ns"], "event": event}
        atomic_json(Path(campaign) / "phase1-runtime-suppressions.json", ledger)
    return ledger["cases"][case["scenario_id"]]


def completed_product_time(case, outcome):
    if outcome.get("mode") != "performance" or case.get("proof_only"):return None
    complete = outcome.get("sample_complete")
    if not isinstance(complete, dict):return None
    if case.get("input_mode") == "directory":
        phases = [row for row in parse_records(Path(outcome["evidence_path"]) / "raw.jsonl") if row.get("kind") == "phase" and row.get("phase") == "initialize"]
        if len(phases) != 1:raise ValueError("initialization product-time boundary unavailable")
        value = phases[0].get("elapsed_ns")
    else:
        value = complete.get("pure_call_sum_ns")
    if type(value) is not int or value < 0:raise ValueError("completed product-time sum unavailable")
    return value


def budget_suppression_can_continue(outcome):
    sound = outcome.get("supervisor_cleanup_status") == "pass" and not outcome.get("container_oom_killed") and not outcome.get("observer_errors") and not outcome.get("other_resource_failure") and not outcome.get("other_product_failure") and outcome.get("harness_status") != "fail"
    return sound and (successful(outcome) or outcome.get("phase1_status") == SUPPRESSION_STATUS and outcome.get("supervisor_failure") in {None, "product-time-budget"})


def kill_group(child):
    try: os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError: pass
    child.wait()


def command(argv, timeout=60, **kw):
    child = subprocess.Popen([str(x) for x in argv], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True, **kw)
    try:
        stdout, stderr = child.communicate(timeout=timeout)
    except BaseException:
        kill_group(child)
        raise
    if child.returncode:
        raise subprocess.CalledProcessError(child.returncode, argv, stdout, stderr)
    return stdout.strip()


@contextlib.contextmanager
def phase_deadline(seconds, label):
    """Bound Python hashing/copying and child startup as well as the product."""
    prior = signal.getsignal(signal.SIGALRM)
    def expire(*_): raise TimeoutError(f"{label} deadline expired ({seconds}s)")
    signal.signal(signal.SIGALRM, expire)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try: yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, prior)


def read_json(path): return json.loads(Path(path).read_text())


def atomic_json(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".pending")
    custody.write_json(temporary, value)
    temporary.replace(path)


def deadline(case, mode):
    if mode == "fast-verify":mode = "verify"
    if case.get("inherited"): return 30 if mode == "verify" else 10
    if case["family_id"] == "workspace_reliability":
        return 1500 if case["operation"] == "sustained-600s" else 3600 if case["operation"] in EXTENDED else 600
    if case["family_id"] == "dedup_branch_history" and case["tier"] >= 100:
        return 7200 if mode == "verify" else 3600
    large = case["tier"] >= 100 or case["operation"] in FIXED_LARGE
    return (1800 if mode == "verify" else 600) if large else (600 if mode == "verify" else 120)


def preparation_deadline(case):
    if case.get("inherited"): return 30
    if case["family_id"] in {"dedup_branch_history", "workspace_reliability"}: return 600
    return 1800 if case["tier"] >= 100 or case["operation"] in FIXED_LARGE else 600


def bounded_run(argv, out, err, seconds, env, resource_files=(), mutable=None, observer_errors=None, observer_process=None, on_budget=None):
    started = time.monotonic_ns()
    reason = None
    with Path(out).open("xb") as stdout, Path(err).open("xb") as stderr:
        child = subprocess.Popen(argv, stdout=stdout, stderr=stderr, env=env, start_new_session=True)
        budget_reader = Path(out).open("rb") if on_budget is not None else None
        pending = b""
        def poll_budget():
            nonlocal pending
            if budget_reader is None:return False
            pending += budget_reader.read()
            lines = pending.split(b"\n");pending = lines.pop()
            tripped = False
            for line in lines:
                if b"product-time-budget-exceeded" not in line:continue
                event = json.loads(line.removeprefix(b"RELIABILITY\t"))
                if event.get("kind") == "product-time-budget-exceeded":
                    on_budget(event);tripped = True
            return tripped
        try:
            while child.poll() is None:
                if poll_budget():reason = "product-time-budget"
                if (time.monotonic_ns()-started)/1e9 >= seconds:
                    reason = "case-timeout"
                if sum(p.stat().st_size for p in (Path(out), Path(err), *resource_files) if p.exists()) > LOG_LIMIT:
                    reason = "retained-log-limit"
                if mutable:
                    files = list(Path(mutable).glob("store.sqlite*"))
                    if sum(p.stat().st_size for p in files) > 4*1024**3 or sum(p.stat().st_blocks*512 for p in files) > 4*1024**3:
                        reason = "store-disk-limit"
                if observer_errors or (observer_process is not None and observer_process.poll() is not None):
                    reason = "resource-observer-failure"
                if reason:
                    kill_group(child)
                    stderr.write(f"phase1 supervisor: {reason}\n".encode())
                    break
                time.sleep(.05)
            if poll_budget() and reason is None:reason = "product-time-budget"
        except BaseException as error:
            kill_group(child)
            error.phase1_result = {"exit_code": child.returncode, "timeout": False, "supervisor_failure": "interrupted" if isinstance(error, KeyboardInterrupt) else "supervisor-error",
                "external_process_wall_ns": time.monotonic_ns()-started, "hard_deadline_seconds": seconds}
            raise
        finally:
            if budget_reader is not None:budget_reader.close()
        if sum(p.stat().st_size for p in (Path(out), Path(err), *resource_files) if p.exists()) > LOG_LIMIT:
            reason = "retained-log-limit"
        code = 124 if reason in {"case-timeout", "product-time-budget"} else 125 if reason else child.returncode
    return {"exit_code": code, "child_exit_code": child.returncode, "timeout": reason in {"case-timeout", "product-time-budget"}, "supervisor_failure": reason,
            "external_process_wall_ns": time.monotonic_ns()-started, "hard_deadline_seconds": seconds}


def parse_records(path):
    rows = []
    for number, line in enumerate(Path(path).read_text().splitlines(), 1):
        if line.startswith("RELIABILITY\t"): line = line.partition("\t")[2]
        if line:
            try: rows.append(json.loads(line))
            except ValueError as error: raise ValueError(f"raw record {number}: {error}") from error
    return rows


def build_assets(args):
    destination = Path(args.build).resolve()
    revision = command(["git", "rev-parse", "HEAD"], cwd=REPO)
    checkout = REPO / "target" / f"phase1-source-{revision}"
    if not checkout.exists():
        command(["git", "worktree", "add", "--detach", str(checkout), revision], cwd=REPO)
    if not (checkout / "target").exists(): (checkout / "target").symlink_to(REPO / "target")
    helper = checkout / "benchmark/fs-bench-pro/sdk-edit-custody.py"
    subprocess.run([sys.executable, str(helper), "build-workspace", str(destination), f"layerfs-v013:{revision[:12]}"], cwd=checkout, check=True)


def sealed_build(build):
    custody.verify_manifest(build / "evidence")
    assets = read_json(build / "evidence/build.json")
    if assets["schema"] != "fs-bench-pro-workspace-build-v1" or assets["status"] != "pass": raise ValueError("unqualified Workspace build")
    if custody.sha(build / "fs-benchmark-pro") != assets["binary_sha256"]: raise ValueError("host binary seal")
    assets["workspace_preparation_compatibility"] = custody.workspace_preparation_digest(assets)
    image = read_json(build / "evidence/image.json")
    custody.validate_image(image, assets)
    if image["Id"] != assets["image_id"]: raise ValueError("sealed image identity mismatch")
    custody.validate_image_binaries(build / "evidence", assets)
    return assets


def source_validation(build):
    assets = sealed_build(build)
    for name in ("workspace-runner.py", "sdk-edit-custody.py", "lib-runtime.sh"):
        expected = subprocess.check_output(["git", "show", f"{assets['revision']}:benchmark/fs-bench-pro/{name}"], cwd=REPO)
        if (HERE / name).read_bytes() != expected: raise ValueError(f"running {name} differs from sealed build revision")
    docker = json.loads(command(["docker", "info", "--format", "{{json .}}"] ))
    environment = {"host": platform.uname()._asdict(), "host_cpu_count": os.cpu_count(),
                   "docker": {key: docker.get(key) for key in ("ID", "ServerVersion", "KernelVersion", "OperatingSystem", "OSType", "Architecture", "NCPU", "MemTotal")},
                   "resource_profile": "v013-macos-docker-linux-fuse-ack-window-v1"}
    assets["runtime_environment"] = environment
    assets["environment_identity"] = hashlib.sha256(json.dumps(environment, sort_keys=True).encode()).hexdigest()
    return assets


# This frozen recipe has four generated-file COPY layers and one chmod/self-check
# layer after its immutable system prefix. None replaces Git, libraries or config.
GIT_SYSTEM_RECIPE_SHA256 = "7271d9f0437152402d556d3a0d7804f4a3e0fb4a3fdf5f59d2c1f87ac8166023"


def git_system_identity(build, assets):
    recipe = subprocess.check_output(["git", "show", f"{assets['revision']}:benchmark/fs-bench-pro/Dockerfile.layerfs"], cwd=REPO)
    if hashlib.sha256(recipe).hexdigest() != GIT_SYSTEM_RECIPE_SHA256:
        raise ValueError("preparation producer requires the reviewed immutable Git system recipe")
    image = read_json(build / "evidence/image.json")
    layers = image["RootFS"]["Layers"]
    if image["Id"] != assets["image_id"] or image["Os"] != "linux" or len(layers) <= 5:
        raise ValueError("preparation image system identity missing")
    source_env = {"LAYERFS_SOURCE_COMMIT", "LAYERFS_SOURCE_TREE", "LAYERFS_SOURCE_SEAL"}
    return {"recipe_sha256": GIT_SYSTEM_RECIPE_SHA256,
            "platform": [image.get(key) for key in ("Os", "Architecture", "Variant")],
            "rootfs_type": image["RootFS"]["Type"], "system_layers": layers[:-5],
            "config": {key: value for key, value in image["Config"].items() if key not in {"Labels", "Env"}},
            "environment": [item for item in image["Config"]["Env"] if item.partition("=")[0] not in source_env]}


SQL_CAPTURE_SCHEMA = "crates/layerfs-layerstack-store/src/schema.rs"
SQL_CAPTURE_SCHEMA_PAIR = ("bf21e1b0f4d20f0752c3baa180e83b2cf842a0b0f4e97244d7fbf80c141b1daf",
                           "827c18cc63eeb5c1df0decb9b2e35291c0de18e2222eed0f07c776382c0af950")
PREPARATION_SPILL_OBJECTS = "crates/layerfs-layerstack-store/src/objects.rs"
PREPARATION_SPILL_PAIR = ("1e88000d97560d5d9d8afdaaf379144cfd859133897650f357c8299a19b3aa32",
                          "4b07eb03a2e6ddfe926a2c5fa621db462c659ff1ee164e41ec3b90cb871df9c8")


def preparation_inputs(revision, full_helpers=False):
    # The timed host helper uses the existing custody preparation slice;
    # all other helper/dependency inputs still match in full.
    exact = {"Cargo.toml", "Cargo.lock", *("benchmark/fs-bench-pro/" + path for path in
        ("families/sdk_edit_common.rs", "src/main.rs", "workload.rs", "src/workspace_bench.rs", "workspace_common.rs"))}
    prefixes = ("crates/layerfs-content/", "crates/layerfs-layerstack-store/", "crates/layerfs-sdk/", "crates/layerfs-monitor/")
    entries = {}
    for row in subprocess.check_output(["git", "ls-tree", "-rz", revision], cwd=REPO).split(b"\0"):
        if not row:continue
        metadata, path = row.split(b"\t", 1);path = path.decode();mode, kind, oid = metadata.split()
        if path in exact or path.startswith(prefixes):
            if kind != b"blob":raise ValueError("non-file preparation input")
            entries[path] = (mode.decode(), oid)
    batch = subprocess.check_output(["git", "cat-file", "--batch"], cwd=REPO, input=b"\n".join(value[1] for value in entries.values()) + b"\n")
    files, cursor = {}, 0
    for path, (mode, _) in entries.items():
        end = batch.index(b"\n", cursor);_, kind, length = batch[cursor:end].split();cursor = end + 1
        if kind != b"blob":raise ValueError("missing preparation blob")
        data = batch[cursor:cursor + int(length)];cursor += int(length) + 1
        ranges = {"benchmark/fs-bench-pro/src/workspace_bench.rs": (b"fn fixture_info(", b"\nfn output_text(")}
        if not full_helpers and path in ranges:
            start, end = ranges[path];left = data.index(start) if start else 0;right = data.index(end, left) if end else len(data);data = data[left:right]
        files[path] = (mode, data)
    return files


PREPARATION_TIMED_HOST_PAIR = ("0a0df8c560928ac916aae8ce984b683f65085c344cd832fc86f9e3ffee51fcb3",
                               "df074a4160010b328db3ecb92d99f82a87f4acacd0bf347e5490d435e9f85771")


FAST_VERIFIER_SOURCE = "f5f8a69859bd9c0a2e7dc7780de55578fb05eec3"
FAST_VERIFIER_HASHES = {
    "src/workspace_bench.rs": "4a8a57746d028a3ec0b9cfdd12cacabe26705744bfa60c025bf6e73ffd89646b",
    "src/workspace_verify.rs": "84e7be4ec4fd09fca243dbff2049873c1d5d0166577bc3a027b1c1f383e66e42",
    "workspace_common.rs": "434a48b380ffdd457e8faa02fd7afa2c20bd0ee83545793022b75d698d1bd85c",
    "ordinary_workloads.rs": "92a1e08a3ede32ef016f8cb3985e3744839a97d604041f666c7db32d5424e4df",
    "workload.rs": "50f1ddc77b11909f27cf6ee7fe8e3c6f93d969d095ee6d9e11b5dfe0c0d21635",
}


FAST_V2_HASHES = {
    "src/workspace_bench.rs": "30c716c289b72855c377c778756d5cdf28145126e0da00be4b31060b21d54f62",
    "src/workspace_verify.rs": "e8475f29123dd44f969be8a9cd0664e0501320e8b8304e441df782f8d86be5ed",
    "src/dedup_verify.rs": "bcb8a7effbd4d318f91e2e9e6c98ecc53e5d33c712711c501d99cdf33331f815",
    "workspace_common.rs": "7d4011cd531af8fca7fce3a36897efbc163cdb4e26170b74ce6ebdc568a3a448",
    "workspace_registry.rs": "2c4b4230f30f1668931aea6b77a877ddaeb191b4e916c2872313c1a9811a055d",
    "ordinary_workloads.rs": "3a4b013e67c469f4295be89b6e8330198814e23c695ca4da5fcb029ae262b56b",
    "workload.rs": "50f1ddc77b11909f27cf6ee7fe8e3c6f93d969d095ee6d9e11b5dfe0c0d21635",
}


HISTORICAL_FULL_VERIFIER_REVISION = "7948df2de269e5ffd47a232ffd8091ff83f8869f"
HISTORICAL_FULL_VERIFIER_SHA256 = "c0bdb1d9e2faef6efe7f542f2a7a1cd35fe1c1ba1c21991c16ec22f34b9bd4e4"
HISTORICAL_FULL_VERIFIER_HASHES = {
    HISTORICAL_FULL_VERIFIER_REVISION: HISTORICAL_FULL_VERIFIER_SHA256,
    "e32469e975e8e185ca525b02bb71d70bafa4e865": "346bcc35e0db2df1975193563aaa46669daddd4da882f9bca360776b2322b320",
}


def fast_verifier_source_proof(revision):
    pairs = {}
    host = subprocess.check_output(["git", "show", f"{revision}:benchmark/fs-bench-pro/src/workspace_bench.rs"], cwd=REPO)
    v2 = hashlib.sha256(host).hexdigest() == FAST_V2_HASHES["src/workspace_bench.rs"]
    for relative, expected in (FAST_V2_HASHES if v2 else FAST_VERIFIER_HASHES).items():
        if relative == "src/workspace_verify.rs" and not v2:
            expected = HISTORICAL_FULL_VERIFIER_HASHES.get(revision, expected)
        path = "benchmark/fs-bench-pro/" + relative
        old, new = [subprocess.check_output(["git", "show", f"{rev}:{path}"], cwd=REPO) for rev in (FAST_VERIFIER_SOURCE, revision)]
        if hashlib.sha256(new).hexdigest() != expected:raise ValueError("unreviewed fast verifier source: " + path)
        pairs[path] = {"old_sha256": hashlib.sha256(old).hexdigest(), "new_sha256": expected}
    return pairs


def preparation_source_compatibility(producer, runtime, legacy_full_helpers=False):
    if producer["revision"] not in {"3422433020a678a77f88e8a110492ca293c05e30", "a40b17e05486e5b747b689e7710475d739556a69"}:
        raise ValueError("unreviewed preparation producer revision")
    old, new = preparation_inputs(producer["revision"], legacy_full_helpers), preparation_inputs(runtime["revision"], legacy_full_helpers)
    if set(old) != set(new):raise ValueError("preparation source inventory changed")
    changed = {}
    timed_host = None
    fast_pairs = None
    if not legacy_full_helpers:
        name = "benchmark/fs-bench-pro/src/workspace_bench.rs"
        hashes = tuple(hashlib.sha256(subprocess.check_output(["git", "show", f"{value['revision']}:{name}"], cwd=REPO)).hexdigest() for value in (producer, runtime))
        if hashes[0] != hashes[1]:
            if hashes != PREPARATION_TIMED_HOST_PAIR:
                if hashes[0] != PREPARATION_TIMED_HOST_PAIR[0] or hashes[1] not in {FAST_VERIFIER_HASHES["src/workspace_bench.rs"], FAST_V2_HASHES["src/workspace_bench.rs"]}:raise ValueError("unreviewed timed host helper change")
                fast_pairs = fast_verifier_source_proof(runtime["revision"])
                for relative, marker, prefix in (("workspace_common.rs", b"pub(crate) fn decode_manifest(", True), ("workload.rs", b"pub(crate) struct Sha256", False)):
                    path = "benchmark/fs-bench-pro/" + relative
                    for values in (old, new):
                        mode, data = values[path];offset = data.index(marker)
                        values[path] = (mode, data[:offset] if prefix else data[offset:])
            timed_host = {"path": name, "producer_sha256": hashes[0], "runtime_sha256": hashes[1], "unchanged_preparation_span": "fn fixture_info( through before fn output_text("}
    for path, expected in ((SQL_CAPTURE_SCHEMA, SQL_CAPTURE_SCHEMA_PAIR), (PREPARATION_SPILL_OBJECTS, PREPARATION_SPILL_PAIR)):
        if old[path] == new[path]:
            if path == SQL_CAPTURE_SCHEMA:raise ValueError("compatibility mismatch is not the reviewed SQL capture fix")
            continue
        hashes = tuple(hashlib.sha256(value[path][1]).hexdigest() for value in (old, new))
        if hashes != expected or old[path][0] != new[path][0]:raise ValueError("unreviewed preparation implementation pair")
        changed[path] = {"producer_sha256": hashes[0], "runtime_sha256": hashes[1]}
        new[path] = old[path]
    if old != new:raise ValueError("another preparation dependency changed")
    manifest = {path: {"mode": value[0], "sha256": hashlib.sha256(value[1]).hexdigest()} for path, value in old.items()}
    result = {"kind": "exact-sql-capture-and-derived-spill-preparation-v1" if legacy_full_helpers else "exact-sql-capture-and-derived-spill-preparation-v2", "producer_revision": producer["revision"],
            "runtime_revision": runtime["revision"], "producer_compatibility": producer["workspace_preparation_compatibility"],
            "runtime_compatibility": runtime["workspace_preparation_compatibility"], "changed_inputs": changed,
            "producer_input_manifest_sha256": hashlib.sha256(json.dumps(manifest, sort_keys=True).encode()).hexdigest(),
            "scope": "Qualified canonical input/reference bytes only; SQL history capture is outside persistent state, and the reviewed derived spill index preserves canonical bytes/order/collision results. Actual producer identity and cache key remain unchanged; no performance compatibility claim."}
    if not legacy_full_helpers:result["timed_host_source_pair"] = timed_host
    if fast_pairs is not None:result["fast_verification_source_pairs"] = fast_pairs
    return result


SUPPRESSION_NOTICES = {
    "phase-1-handoff": b"> **Latest Phase 1 scope:** Enforce the [15-second suppression policy](phase-1-runtime-suppressions.md)\n> before scheduling work. Its permanent Phase 1 exclusions supersede the\n> original full-inventory completion language below; never count a suppression\n> as a passing benchmark.\n\n",
    "testing-rules": b"> **Latest Phase 1 scope:** The [15-second suppression policy](phase-1-runtime-suppressions.md)\n> supersedes the original full-inventory execution requirement. Keep suppressed\n> combinations explicit; all remaining active cases require valid results and\n> independent verification.\n\n",
}


FAST_ACCEPTANCE_NOTICE = b"> **Latest verification acceptance, 2026-09-04:** Apply the explicit\n> [fast-verification amendment](phase-1-fast-verification-amendment.md).\n> Qualified fast checks now suffice for routine Phase 1 verification; remaining\n> exhaustive coverage is deferred to Phase 2, with honest assurance labels.\n> Targeted failure confirmation, expected-error, resource and cleanup gates remain.\n\n"


def preparation_contract_compatible(name, producer, runtime):
    path = f"docs/roadmap/0.1/0.1.3/{name}.md"
    left = producer["phase1_contract_files"].get(path);right = runtime["phase1_contract_files"].get(path)
    if left is not None and left == right:return True
    notice = SUPPRESSION_NOTICES.get(name)
    if left is None or right is None:return False
    old, new = [subprocess.check_output(["git", "show", f"{source['revision']}:{path}"], cwd=REPO) for source in (producer, runtime)]
    if hashlib.sha256(old).hexdigest() != left or hashlib.sha256(new).hexdigest() != right:return False
    candidates = [new]
    if name == "phase-1-handoff" and new.count(FAST_ACCEPTANCE_NOTICE) == 1:candidates.append(new.replace(FAST_ACCEPTANCE_NOTICE, b"", 1))
    return any(value == old or notice is not None and value.count(notice) == 1 and value.replace(notice, b"", 1) == old for value in candidates)


def select_preparation(build, assets, producer_build, registry, selected):
    if producer_build == build:
        return assets
    producer = sealed_build(producer_build)
    if producer["revision"] == "3422433020a678a77f88e8a110492ca293c05e30" and any(case["scenario_id"] == "namespace-subtree-relocate-delete-500" for case in selected):
        raise ValueError("namespace500 requires the qualified a40 producer; old342 preparation exceeded its deadline")
    if producer["workspace_preparation_compatibility"] != assets["workspace_preparation_compatibility"]:
        producer["preparation_source_compatibility"] = preparation_source_compatibility(producer, assets)
    contracts = ("testing-rules", "phase-1-handoff", "failure-repair-amendment", "execution-contract",
        "ordinary-execution-contract", "dedup-reliability-execution-contract", "capped-inherited-replacements",
        "payload-create-read", "tiny-file-churn", "directory-construction-traversal", "git-tool-workflow",
        "namespace-mutation", "workspace-change-locality", "mixed-load-bearing-workload", "dedup-cross-file",
        "dedup-cdc-locality", "dedup-workspace-reuse", "dedup-branch-history", "workspace-reliability")
    for name in contracts:
        path = f"docs/roadmap/0.1/0.1.3/{name}.md"
        if not preparation_contract_compatible(name, producer, assets):
            raise ValueError(f"preparation producer changes frozen contract: {path}")
    producer_registry = [json.loads(line) for line in command([producer_build / "fs-benchmark-pro", "workspace-registry"]).splitlines()]
    if producer_registry != registry:
        raise ValueError("preparation producer changes frozen registry")
    if any(case["operation"] == "git-tool" for case in selected):
        system = git_system_identity(producer_build, producer)
        if system != git_system_identity(build, assets):
            raise ValueError("preparation producer changes immutable Git system/runtime identity")
        producer["git_system_identity_sha256"] = hashlib.sha256(json.dumps(system, sort_keys=True).encode()).hexdigest()
    return producer


FAST_PROFILE = "fast-verify-v1"
CERTIFICATE_READONLY = {"tiny-stat", "payload-random-read", "directory-metadata-scan", "directory-content-scan", "workspace-clean-commit"}


def certificate_source_bindings(source_revision, runtime_revision, families):
    paths = {"benchmark/fs-bench-pro/workspace_common.rs", "benchmark/fs-bench-pro/ordinary_workloads.rs",
        "benchmark/fs-bench-pro/workload.rs", "benchmark/fs-bench-pro/workspace_registry.rs"}
    paths.update(f"benchmark/fs-bench-pro/families/{family}.rs" for family in families)
    if any(family.startswith("dedup_") for family in families):paths.add("benchmark/fs-bench-pro/dedup_workloads.rs")
    bindings = {}
    for name in sorted(paths):
        values = [subprocess.check_output(["git", "show", f"{revision}:{name}"], cwd=REPO) for revision in (source_revision, runtime_revision)]
        if name.endswith("workspace_common.rs"):
            values = [value[:value.index(b"pub(crate) fn decode_manifest(")] for value in values]
        elif name.endswith("ordinary_workloads.rs"):
            values = [value.split(b"// BEGIN NATIVE FAST VERIFICATION V1", 1)[0].rstrip() for value in values]
        elif name.endswith("/workload.rs"):
            values = [value[value.index(b"pub(crate) struct Sha256"):] for value in values]
        elif name.endswith("workspace_registry.rs"):
            # Only profile dispatch was added; cases/fixtures/expected recipes precede it.
            values = [value[:value.index(b"pub(crate) fn dispatch(")] for value in values]
        if values[0] != values[1]:raise ValueError("fixture/oracle source assumptions changed: " + name)
        bindings[name] = hashlib.sha256(values[0]).hexdigest()
    return bindings


def qualified_json(path):
    path = Path(path)
    entries = {}
    for line in (path.parent / "evidence.sha256").read_text().splitlines():
        digest, name = line.split(maxsplit=1)
        if name in entries:raise ValueError("duplicate qualified artifact seal")
        entries[name] = digest
    if entries.get(path.name) != custody.sha(path):raise ValueError("qualified artifact changed: " + str(path))
    return read_json(path)


def qualified_full_row(directory, campaign):
    for source in sorted((Path(campaign) / "qualification").glob("*/incremental-full-rows.json")):
        value = qualified_json(source)
        rows = [row for row in value.get("rows", []) if Path(row.get("evidence", "")).resolve() == directory]
        if rows:
            if len(rows) != 1:raise ValueError("duplicate qualified full row")
            return rows[0], custody.sha(source)
    source = Path(campaign) / "results/review.json"
    value = qualified_json(source)
    rows = [row for row in value.get("rows", []) if Path(row.get("evidence", "")).resolve() == directory]
    if len(rows) != 1:raise ValueError("no independently qualified full row")
    return rows[0], custody.sha(source)


def certificate_product_binding(source_revision, source_product, source_case, assets, campaign, registry):
    if source_product == assets["product_seal"]:return {"scope": "identical product seal"}
    spec = importlib.util.spec_from_file_location("certificate_product_report", HERE / "generate-workspace-report.py")
    report = importlib.util.module_from_spec(spec);spec.loader.exec_module(report)
    config = read_json(Path(campaign) / "evidence-builds.json")
    bridges = report.configured_product_bridges(config, assets, {row["scenario_id"]:row for row in registry})
    source = {"revision": source_revision, "product_seal": source_product}
    return report.matching_product_bridge(bridges, source, assets, source_case)


def verification_certificate(path, case, seed, assets, campaign, registry, components=False):
    """Qualify existing full evidence only; never run or silently fall back to verification."""
    try:
        directory = Path(path).resolve()
        custody.verify_manifest(directory)
        if (directory / "input-qualification.json").exists():
            marker = qualified_json(directory / "input-qualification.json")
            if marker.get("status") != "canonical_input_qualified" or marker.get("fully_verified") is not False:raise ValueError("input reference is not qualified canonical state")
            if marker["seed"] != seed:raise ValueError("input reference seed differs")
            binding = certificate_product_binding(marker["source_revision"], marker["product_identity"], marker["scenario_id"], assets, campaign, registry)
            certificate_source_bindings(marker["source_revision"], assets["revision"], {case["family_id"], marker["family_id"]})
            return {**marker, "schema": "fast-verification-certificate-v2", "profile": "fast-verify-v2", "assurance": "canonical_input_qualified", "reference_assurance": "qualified_content_components" if components else "canonical_input_qualified", "reference_native_readback": False,
                "source_attempt": str(directory), "source_manifest_sha256": custody.sha(directory / "evidence.sha256"), "source_scenario_id": marker["scenario_id"], "source_seed": marker["seed"], "source_step": 0, "product_seal": marker["product_identity"], "product_compatibility": binding}
        outcome = read_json(directory / "outcome.json")
        if outcome.get("mode") != "verify" or not successful(outcome) or outcome.get("timeout") or outcome.get("observer_errors"):
            raise ValueError("certificate is not a completed full verification")
        if outcome.get("seed") != seed:raise ValueError("certificate seed differs")
        product_binding = certificate_product_binding(outcome["source_revision"], outcome["product_identity"], outcome["scenario_id"], assets, campaign, registry)
        source_case = next((row for row in registry if row["scenario_id"] == outcome.get("scenario_id")), None)
        if source_case is None or not components and source_case["operation"] not in CERTIFICATE_READONLY:
            raise ValueError("initial profile requires a fully verified read-only pristine-input certificate")
        bindings = certificate_source_bindings(outcome["source_revision"], assets["revision"], {case["family_id"], source_case["family_id"]})
        records = parse_records(directory / "raw.jsonl")
        if any(row.get("kind") == "fast-verification-complete" for row in records) or sum(row.get("kind") == "verification-complete" and row.get("status") == "pass" for row in records) != 1:
            raise ValueError("certificate lacks exhaustive completion")
        report_spec = importlib.util.spec_from_file_location("certificate_report", HERE / "generate-workspace-report.py")
        report = importlib.util.module_from_spec(report_spec);report_spec.loader.exec_module(report)
        qualified_row, qualified_row_sha256 = qualified_full_row(directory, campaign)
        rows = [qualified_row]
        if len(rows) != 1 or rows[0].get("mode") != "verify" or rows[0].get("evidence_status") != "PASS" or rows[0].get("product_status") != "pass" or rows[0].get("issues") or rows[0].get("violations"):
            raise ValueError("certificate has no admitted full-verification row in the retained report")
        row = rows[0]
        if row.get("input_identity") != outcome.get("input_identity") or any(outcome.get(key) != row.get("source_identity", {}).get(key) for key in report.IDENTITY_FIELDS):
            raise ValueError("report certificate identity differs from its sealed outcome")
        package = directory / "verification/canonical-verification"
        canonical = dict(line.split("=", 1) for line in (package / "canonical-receipt.txt").read_text().splitlines())
        natives = [report.receipt(item["receipt"]) for item in records if item.get("kind") == "native-verification"]
        if len(natives) != 1 or natives[0].get("verification_status") != "pass" or canonical.get("verification_status") != "pass" or canonical.get("canonical_role_status") != "pass":
            raise ValueError("full canonical/native checks are not both passing")
        for key in ("oracle_identity", "verified_paths", "verified_regular_paths", "logical_bytes"):
            if str(natives[0].get(key)) != canonical.get(key):raise ValueError("canonical/native certificate coverage disagrees: " + key)
        if canonical.get("oracle_scope") != "independent-source" or canonical.get("persistence_custody_paths") != "0":
            raise ValueError("certificate must use independent expected data, not output-derived custody")
        if not report.digest(canonical.get("canonical_root")) or not report.digest(canonical.get("oracle_identity")):
            raise ValueError("certificate root/oracle identity missing")
        artifacts = {name: str(package / name) for name in ("independent-manifest.tsv.gz", "file-roots.tsv.gz", "payload-extents.tsv.gz", "canonical-receipt.txt")}
        return {"schema": "fast-verification-certificate-v2", "profile": "fast-verify-v2", "assurance": "fully_verified", "reference_assurance": "qualified_content_components" if components else "fully_verified", "reference_native_readback": True, "source_seed": outcome["seed"], "source_step": 1, "product_compatibility": product_binding,
            "source_attempt": str(directory), "source_manifest_sha256": custody.sha(directory / "evidence.sha256"),
            "source_revision": outcome["source_revision"], "product_seal": outcome["product_identity"], "seed": seed,
            "source_scenario_id": outcome["scenario_id"], "input_plan_sha256": outcome["input_identity"],
            "oracle_identity": canonical["oracle_identity"], "root": canonical["canonical_root"],
            "source_environment_identity": outcome["environment_identity"], "runtime_environment_identity": assets["environment_identity"],
            "source_bindings": bindings, "canonical_role_census": {key:value for key,value in canonical.items() if key.startswith("canonical_")},
            "artifacts": artifacts, "artifact_sha256": {name:custody.sha(Path(value)) for name,value in artifacts.items()},
            "report_sha256": qualified_row_sha256, "scope": "Certified logical immutable reference only; not current Store byte availability or exhaustive fresh-FUSE readback. No current timing/resource equivalence is claimed."}
    except (OSError, ValueError, KeyError, TypeError, StopIteration, AssertionError) as error:
        raise ValueError(f"fast-verify requires a compatible fully verified certificate; full --mode verify is required: {error}") from error


def recipe_paths(binary, case, seed, step):
    lines = command([binary, "workspace-content-recipes", case, seed, step]).splitlines()
    if not lines or lines[0] != "path\tcontent_recipe_sha256":raise ValueError("independent recipe header")
    result = {}
    for line in lines[1:]:
        path, digest = line.split("\t")
        if path in result or len(digest) != 64:raise ValueError("independent recipe duplicate/hash")
        result[path] = digest
    return result


def prepare_fast_certificate(attempt, certificate, case=None, seed=None, binary=None, target_manifest=None, input_plan=None):
    target = attempt / "verifier-exchange" / "fast-certificate";target.mkdir(parents=True)
    copy = dict(certificate);copy["artifacts"] = {};copy["artifact_sha256"] = {}
    source_artifacts = {}
    for name, source in certificate["artifacts"].items():
        data = Path(source).read_bytes()
        if hashlib.sha256(data).hexdigest() != certificate["artifact_sha256"][name]:raise ValueError("certificate artifact changed during acquisition")
        source_artifacts[name] = data
    components = certificate.get("reference_assurance") == "qualified_content_components"
    if components:
        source_roots = dict(line.split("\t") for line in gzip.decompress(source_artifacts["file-roots.tsv.gz"]).decode().splitlines()[1:])
        source_recipes = recipe_paths(binary, certificate["source_scenario_id"], certificate["source_seed"], certificate["source_step"])
        target_recipes = recipe_paths(binary, case["scenario_id"], seed, 0)
        by_recipe = {}
        for path in sorted(source_recipes):
            if path in source_roots:by_recipe.setdefault(source_recipes[path], path)
        mapping = {path: by_recipe[digest] for path, digest in sorted(target_recipes.items()) if digest in by_recipe}
        text = Path(target_manifest).read_text();lines = text.splitlines()
        if not lines or lines[0] != "workspace-independent-manifest-v1":raise ValueError("target independent manifest header")
        metadata = {line.split("\t")[0]:line for line in lines[1:]}
        selected = {"."}
        for path in mapping:
            selected.add(path)
            while "/" in path:
                path = path.rsplit("/", 1)[0];selected.add(path)
        if not selected.issubset(metadata):raise ValueError("component target ancestor outside independent fixture")
        manifest = "workspace-independent-manifest-v1\n" + "".join(metadata[path] + "\n" for path in sorted(selected))
        roots = "path\tcontent_root\n" + "".join(path + "\t" + source_roots[source] + "\n" for path, source in mapping.items())
        remapped = {"independent-manifest.tsv.gz":gzip.compress(manifest.encode(),mtime=0), "file-roots.tsv.gz":gzip.compress(roots.encode(),mtime=0)}
        if "payload-extents.tsv.gz" in source_artifacts:
            extent_lines = gzip.decompress(source_artifacts["payload-extents.tsv.gz"]).decode().splitlines();by_source = {}
            if not extent_lines or extent_lines[0] != "path\tordinal\tpayload_id\tsource_offset\tlogical_length\tpayload_length":raise ValueError("source extent header")
            for line in extent_lines[1:]:
                path, rest = line.split("\t", 1);by_source.setdefault(path, []).append(rest)
            extents = extent_lines[0] + "\n" + "".join(path + "\t" + rest + "\n" for path, source in mapping.items() for rest in by_source.get(source, []))
            remapped["payload-extents.tsv.gz"] = gzip.compress(extents.encode(),mtime=0)
        mapping_text = "target_path\tsource_path\tcontent_recipe_sha256\n" + "".join(path + "\t" + source + "\t" + target_recipes[path] + "\n" for path, source in mapping.items())
        remapped["component-mapping.tsv"] = mapping_text.encode()
        for filename, recipes in (("source-content-recipes.tsv", source_recipes), ("target-content-recipes.tsv", target_recipes)):
            remapped[filename] = ("path\tcontent_recipe_sha256\n" + "".join(path + "\t" + value + "\n" for path,value in sorted(recipes.items()))).encode()
        copy.update(input_plan_sha256=input_plan, oracle_identity=hashlib.sha256(manifest.encode()).hexdigest(), source_artifacts=certificate["artifacts"], source_artifact_sha256=certificate["artifact_sha256"], component_count=len(mapping), reference_root_scope="content components only; target pristine root not certified")
        source_artifacts = remapped
    for name, data in source_artifacts.items():
        destination = target / name;destination.write_bytes(data)
        copy["artifacts"][name] = str(destination);copy["artifact_sha256"][name] = hashlib.sha256(data).hexdigest()
    copy["profile"] = "fast-verify-v2";copy["seed"] = seed if seed is not None else copy["seed"]
    json_path = target / "certificate.json";custody.write_json(json_path, copy);binding = custody.sha(json_path)
    values = {"profile": "fast-verify-v2", "root": copy["root"], "certificate_sha256": binding, "certificate_json": str(json_path),
        "seed": str(copy["seed"]), "input_plan_sha256": copy["input_plan_sha256"], "oracle_identity": copy["oracle_identity"],
        "certificate_file_roots": copy["artifacts"]["file-roots.tsv.gz"], "certificate_manifest": copy["artifacts"]["independent-manifest.tsv.gz"],
        "source_attempt": copy["source_attempt"], "source_revision": copy["source_revision"], "product_seal": copy["product_seal"],
        "reference_assurance": copy.get("reference_assurance", "fully_verified"), "reference_native_readback": str(copy.get("reference_native_readback", True)).lower(),
        "certificate_manifest_sha256": copy["source_manifest_sha256"], "certificate_file_roots_sha256": copy["artifact_sha256"]["file-roots.tsv.gz"],
        "certificate_manifest_file_sha256": copy["artifact_sha256"]["independent-manifest.tsv.gz"]}
    for name, field in (("payload-extents.tsv.gz", "certificate_extents"), ("component-mapping.tsv", "certificate_mapping")):
        if name in copy["artifacts"]:values[field] = copy["artifacts"][name];values[field + "_sha256"] = copy["artifact_sha256"][name]
    if any("\n" in value or "\t" in value for value in values.values()):raise ValueError("certificate projection contains an unsafe path/value")
    tsv = target / "certificate.tsv";tsv.write_text("".join(key + "\t" + value + "\n" for key,value in values.items()))
    return tsv, binding, custody.sha(tsv)


def fast_profile_self_check():
    """Small product-free check; coordinator runs once alongside host/native negatives."""
    with tempfile.TemporaryDirectory(prefix="phase1-fast-certificate-") as temporary:
        root = Path(temporary);source = root / "source";source.mkdir()
        artifacts = {}
        for name in ("file-roots.tsv.gz", "independent-manifest.tsv.gz", "canonical-receipt.txt"):
            path = source / name;path.write_bytes(b"bounded certificate model\n");artifacts[name] = str(path)
        certificate = {"root": "1" * 64, "seed": 1, "input_plan_sha256": "2" * 64, "oracle_identity": "3" * 64,
            "source_attempt": str(source), "source_revision": "4" * 40, "product_seal": "5" * 64, "source_manifest_sha256": "6" * 64,
            "artifacts": artifacts, "artifact_sha256": {name: custody.sha(Path(path)) for name, path in artifacts.items()}}
        tsv, binding, projection = prepare_fast_certificate(root / "attempt", certificate)
        fields = dict(line.split("\t", 1) for line in tsv.read_text().splitlines())
        assert fields["certificate_sha256"] == binding and custody.sha(tsv) == projection
        assert fields["certificate_file_roots_sha256"] == custody.sha(Path(fields["certificate_file_roots"]))
        tsv.write_text(tsv.read_text().replace("profile\tfast-verify-v2", "profile\tchanged-profile"))
        assert custody.sha(tsv) != projection, "projection tamper not detected"
        Path(artifacts["file-roots.tsv.gz"]).write_bytes(b"changed")
        try:prepare_fast_certificate(root / "bad-copy", certificate)
        except ValueError:pass
        else:raise AssertionError("altered certificate source accepted")
        try:verification_certificate(root / "missing", {}, 1, {}, root, [])
        except ValueError as error:assert "full --mode verify is required" in str(error)
        else:raise AssertionError("missing full certificate accepted")
        common = {"harness_identity": "h", "product_identity": "p", "image_id": "i", "environment_identity": "e", "scenario_id": "tiny-stat-1", "seed": 1}
        assert slot_key({**common, "mode": "verify"}) != slot_key({**common, "mode": "fast-verify"})
    print("fast_profile_certificate_projection_scope_self_check=pass")


def acquire(case, seed, binary, cache, run, build, acquisitions, assets, reference=False, runtime_binary=None, certificate=None):
    started = time.monotonic_ns()
    info = json.loads(command([binary, "workspace-reference-info" if reference else "workspace-fixture-info", case["scenario_id"], seed]))
    if runtime_binary is not None and str(runtime_binary) != str(binary):
        runtime_info = json.loads(command([runtime_binary, "workspace-reference-info" if reference else "workspace-fixture-info", case["scenario_id"], seed]))
        if runtime_info != info:
            raise ValueError("runtime and preparation producer disagree on selected fixture/reference identity")
    if certificate is not None and certificate.get("reference_assurance") != "qualified_content_components" and info.get("input_plan_sha256") != certificate["input_plan_sha256"]:
        raise ValueError("fast-verify certificate input differs; full --mode verify is required")
    identity = (assets["workspace_preparation_compatibility"], reference, json.dumps(info, sort_keys=True))
    if identity in acquisitions:
        receipt = dict(acquisitions[identity], run_acquisition_reused=True, run_acquisition_ns=time.monotonic_ns()-started)
        custody.write_json(run / "acquisition.json", receipt)
        return receipt
    with contextlib.redirect_stdout(sys.stderr):
        custody.acquire_prepared(cache, binary, case["scenario_id"] + "-" + str(seed), run / "acquisition.json", str(build),
            workspace=(case["scenario_id"], str(seed)), workspace_timeout=preparation_deadline(case),
            workspace_expected=info, workspace_compatibility=assets["workspace_preparation_compatibility"], workspace_reference=reference)
    receipt = read_json(run / "acquisition.json")
    acquisitions[identity] = receipt
    return receipt


def qualify_input(case, seed, args, assets, campaign, producer):
    if case["input_mode"] != "store" or case.get("proof_only"):raise ValueError("canonical input qualification requires one routine Store input")
    binary = Path(args.assets).resolve() / "fs-benchmark-pro";producer_build = Path(args.preparation_assets or args.assets).resolve()
    identity = json.loads(command([binary, "workspace-fixture-info", case["scenario_id"], seed]))
    recipe = command([binary, "workspace-input-recipe-identity", case["scenario_id"], seed]).strip().split("=", 1)[1]
    key = hashlib.sha256(json.dumps({"input_plan":identity["input_plan_sha256"],"recipe":recipe,"profile":"canonical-input-v1"},sort_keys=True).encode()).hexdigest()
    folder = campaign / "input-qualifications" / key
    if (folder / "input-qualification.json").exists():
        marker = qualified_json(folder / "input-qualification.json")
        if marker.get("status") != "canonical_input_qualified" or marker.get("input_plan_sha256") != identity["input_plan_sha256"] or marker.get("input_recipe_identity") != recipe:raise ValueError("input qualification cache mismatch")
        certificate_source_bindings(marker["source_revision"], assets["revision"], {case["family_id"],marker["family_id"]})
        print(json.dumps({"action":"reuse-qualified-canonical-input","reference":str(folder),"assurance":"canonical_input_qualified"}),flush=True);return
    if folder.exists():raise ValueError("failed/incomplete input qualification retained; investigate before explicit retry")
    folder.mkdir(parents=True);prepared=folder/"preparation";prepared.mkdir();clone=campaign/"scratch"/("input-qualification-"+key);clone.mkdir(parents=True)
    old_image=os.environ.get("LAYERFS_V013_IMAGE");os.environ["LAYERFS_V013_IMAGE"]=producer["image_id"]
    try:
        acquired=acquire(case,seed,producer_build/"fs-benchmark-pro",Path(args.cache).resolve(),prepared,producer_build/"evidence",{},producer,runtime_binary=binary)
        master=Path(acquired["prepared_path"])
        custody.write_json(prepared/"clone.json",custody.clone_prepared(master/"store.sqlite",clone/"store.sqlite",acquired["store_sha256"]))
        shutil.copyfile(master/"branch-id",clone/"branch-id")
        for name in ("input-manifest.tsv","cache.json","evidence.sha256"):shutil.copyfile(Path(acquired["cache_path"])/name,prepared/("master-"+name))
        argv=[str(binary),"workspace-qualify-input",str(clone),case["scenario_id"],str(seed),str(folder)]
        custody.write_json(folder/"command.json",{"argv":argv});result=bounded_run(argv,folder/"raw.jsonl",folder/"stderr.txt",preparation_deadline(case),dict(os.environ,LAYERFS_V013_RESOURCE_PROFILE="1"),mutable=clone)
        records=parse_records(folder/"raw.jsonl");complete=[r for r in records if r.get("kind")=="input-qualification-complete"]
        if result["exit_code"]!=0 or result["supervisor_failure"] or len(complete)!=1 or complete[0].get("status")!="canonical_input_qualified" or complete[0].get("case")!=case["scenario_id"] or complete[0].get("seed")!=seed or any(r.get("kind") in {"resource-failure","host-rss-failure","host-resource-failure"} for r in records):raise ValueError("canonical input qualification failed; original evidence retained")
        package=folder/"canonical-verification";receipt=dict(line.split("=",1) for line in (package/"canonical-receipt.txt").read_text().splitlines())
        if receipt.get("verification_status")!="pass" or receipt.get("canonical_role_status")!="pass" or receipt.get("canonical_root")!=complete[0]["root"] or receipt.get("oracle_identity")!=complete[0]["oracle_identity"]:raise ValueError("canonical input receipt/root differs")
        artifacts={name:str(package/name) for name in ("independent-manifest.tsv.gz","file-roots.tsv.gz","payload-extents.tsv.gz","canonical-receipt.txt")}
        marker={"schema":"canonical-input-qualification-v1","status":"canonical_input_qualified","fully_verified":False,"reference_native_readback":False,"scenario_id":case["scenario_id"],"family_id":case["family_id"],"seed":seed,"source_revision":assets["revision"],"product_identity":assets["product_seal"],"harness_identity":assets["harness_seal"],"image_id":assets["image_id"],"input_plan_sha256":identity["input_plan_sha256"],"input_recipe_identity":recipe,"root":complete[0]["root"],"oracle_identity":complete[0]["oracle_identity"],"store_sha256":acquired["store_sha256"],"environment_identity":assets["environment_identity"],"artifacts":artifacts,"artifact_sha256":{name:custody.sha(Path(path)) for name,path in artifacts.items()},"resource_result":result}
        shutil.rmtree(clone)
        if sum(p.stat().st_size for p in folder.rglob("*") if p.is_file())>LOG_LIMIT:raise ValueError("input qualification exceeds64MiB artifact cap")
        custody.write_json(folder/"input-qualification.json",marker);custody.seal(folder)
        print(json.dumps({"action":"qualified-canonical-input","reference":str(folder),"assurance":"canonical_input_qualified"}),flush=True)
    except BaseException as error:
        custody.write_json(folder/"failure.json",{"status":"fail","error":str(error),"mutable_clone":str(clone)});custody.seal(folder);raise
    finally:
        if old_image is None:os.environ.pop("LAYERFS_V013_IMAGE",None)
        else:os.environ["LAYERFS_V013_IMAGE"]=old_image


def stream_samples(process, target, ready, errors):
    try:
        with gzip.open(target, "xb") as stream:
            for line in iter(process.stdout.readline, b""):
                stream.write(line)
                if not ready.is_set(): stream.flush(); ready.set()
    except BaseException as error:
        errors.append(str(error)); ready.set()


def sample(case, seed, args, assets, campaign, acquisitions, producer=None):
    started = time.monotonic_ns()
    attempt = campaign / "attempts" / f"{case['scenario_id']}-s{seed}-{args.mode}-{uuid.uuid4().hex[:12]}"
    attempt.mkdir(parents=True)
    prepared_dir = attempt / "preparation"; prepared_dir.mkdir()
    mutable = campaign / "scratch" / attempt.name; mutable.mkdir(parents=True)
    name = "layerfs-v013-" + uuid.uuid4().hex[:16]
    binary = str(Path(args.assets).resolve() / "fs-benchmark-pro")
    producer = assets if producer is None else producer
    producer_build = Path(args.preparation_assets or args.assets).resolve()
    producer_binary = str(producer_build / "fs-benchmark-pro")
    evidence = producer_build / "evidence"
    env = dict(os.environ, LAYERFS_V013_IMAGE=assets["image_id"], LAYERFS_V013_RESOURCE_PROFILE="1")
    for key in ("LAYERFS_V013_GIT_REFERENCE_HOST", "LAYERFS_V013_VERIFIER_EXCHANGE", "LAYERFS_V013_VERIFIER_EXCHANGE_HOST", "LAYERFS_V013_FAST_CERTIFICATE", "LAYERFS_V013_FAST_CERTIFICATE_SHA256", "LAYERFS_V013_FAST_NO_REUSE", "LAYERFS_V013_FAST_INPUT_PLAN_SHA256"):
        env.pop(key, None)
    outcome = {"command_wall_scope":"one sample preparation/runtime/product/cleanup; CLI validation is in invocation receipt", "schema": "fs-bench-pro-v013-sample-v1", "scenario_id": case["scenario_id"], "family_id": case["family_id"],
               "seed": seed, "seed_label": f"repetition-{seed}" if case.get("inherited") else f"layerfs-v0.1.3-seed-{seed}",
               "proof_only": bool(case.get("proof_only")), "inherited": bool(case.get("inherited")), "mode": args.mode,
               "source_revision": assets["revision"], "product_identity": assets["product_seal"], "harness_identity": assets["harness_seal"],
               "contract_commit": assets["phase1_contract_commit"], "image_id": assets["image_id"], "source_arm": args.source_arm, "admission_eligible": False,
               "environment_identity": assets["environment_identity"], "report_generator_identity": assets["report_generator_sha256"],
               "mutable_diagnostic_path": str(mutable), "coverage_status": "unexecuted", "harness_status": "in-progress", "product_status": "not-run", "evidence_path": str(attempt),
               "invalidation_reason": args.invalidate_reason}
    sampler = None; sampler_thread = None; sampler_stderr = None
    observer_errors = []; runtime_started = False
    cgroup_path = attempt / "cgroup-samples.tsv.gz"
    custody.write_json(attempt / "environment.json", assets["runtime_environment"])
    custody.write_json(prepared_dir / "producer-selection.json", {
        "assets": str(producer_build), "revision": producer["revision"], "image_id": producer["image_id"],
        "binary_sha256": producer["binary_sha256"], "build_manifest_sha256": custody.sha(evidence / "build.json"),
        "workspace_preparation_compatibility": producer["workspace_preparation_compatibility"],
        "git_system_identity_sha256": producer.get("git_system_identity_sha256"),
        "source_compatibility": producer.get("preparation_source_compatibility"),
        "runtime_revision": assets["revision"], "runtime_image_id": assets["image_id"],
        "scope": "Selected acquisition producer; cache hits retain their original master producer and input/oracle identities."})
    try:
        with phase_deadline(preparation_deadline(case), "selected input/runtime preparation"):
            old_image = os.environ.get("LAYERFS_V013_IMAGE")
            os.environ["LAYERFS_V013_IMAGE"] = producer["image_id"]
            try:
                acquired = acquire(case, seed, producer_binary, Path(args.cache).resolve(), prepared_dir, evidence, acquisitions, producer, runtime_binary=binary, certificate=getattr(args, "fast_certificate", None))
                reference = None
                if case["operation"] == "git-tool":
                    reference_dir=prepared_dir / "reference"; reference_dir.mkdir()
                    reference=acquire(case, seed, producer_binary, Path(args.cache).resolve(), reference_dir, evidence, acquisitions, producer, reference=True, runtime_binary=binary)
            finally:
                if old_image is None: os.environ.pop("LAYERFS_V013_IMAGE", None)
                else: os.environ["LAYERFS_V013_IMAGE"] = old_image
            for receipt,where in [(acquired,prepared_dir)]+([(reference,prepared_dir/"reference")] if reference else []):
                entry=Path(receipt["cache_path"])
                for filename in ("input-manifest.tsv","evidence.sha256","cache.json"):
                    if (entry/filename).exists():shutil.copyfile(entry/filename,where/("master-"+filename))
            master = Path(acquired["prepared_path"])
            outcome.update(input_identity=acquired["fixture"]["input_plan_sha256"], cache_key=acquired["key"], cache_disposition=acquired["cache_disposition"])
            sample_input = master / "input"
            if case["input_mode"] == "store":
                clone = custody.clone_prepared(master / "store.sqlite", mutable / "store.sqlite", acquired["store_sha256"])
                shutil.copyfile(master / "branch-id", mutable / "branch-id")
            else:
                sample_input = mutable / "input"
                clone = custody.clone_prepared_directory(master / "input", sample_input, acquired)
            custody.write_json(prepared_dir / "clone.json", clone)
            if reference:
                env["LAYERFS_V013_GIT_REFERENCE_HOST"] = str(Path(reference["prepared_path"]) / "input")
                outcome["oracle_identity"] = reference["fixture"]["input_plan_sha256"]
            if case["operation"] == "git-tool" and args.mode == "verify" or args.mode == "fast-verify":
                exchange=attempt / "verifier-exchange"; exchange.mkdir(exist_ok=True)
                env["LAYERFS_V013_VERIFIER_EXCHANGE"] = str(exchange)
                env["LAYERFS_V013_VERIFIER_EXCHANGE_HOST"] = str(exchange)
            if args.mode == "fast-verify":
                env["LAYERFS_V013_FAST_INPUT_PLAN_SHA256"] = outcome["input_identity"]
                if args.fast_no_reuse:
                    recipe = command([binary,"workspace-input-recipe-identity",case["scenario_id"],seed]).strip().split("=",1)[1]
                    binding = hashlib.sha256(f"fast-independent-current-content-v2\n{case['scenario_id']}\n{seed}\n{outcome['input_identity']}\n{recipe}\n".encode()).hexdigest()
                    env["LAYERFS_V013_FAST_NO_REUSE"] = "1";outcome["input_recipe_identity"] = recipe;outcome["reference_assurance"] = "independent_current_content"
                else:
                    certificate_path, binding, projection = prepare_fast_certificate(attempt, args.fast_certificate, case, seed, binary, prepared_dir/"master-input-manifest.tsv", outcome["input_identity"])
                    env["LAYERFS_V013_FAST_CERTIFICATE"] = str(certificate_path);env["LAYERFS_V013_FAST_CERTIFICATE_SHA256"] = projection
                    outcome["verification_certificate_projection_identity"] = projection;outcome["reference_assurance"] = args.fast_certificate["reference_assurance"]
                outcome["verification_certificate_identity"] = binding;outcome["verification_profile"] = "fast-verify-v2"
            runtime_start = time.monotonic_ns(); runtime_started = True
            container, port, capability = command(["bash", HERE / "lib-runtime.sh", name, assets["image_id"]], env=env).split("\t")
            outcome["runtime_preparation_ns"] = time.monotonic_ns()-runtime_start
            inspect = json.loads(command(["docker", "inspect", name]))[0]
            custody.write_json(attempt / "container-before.json", inspect)
            env.update(LAYERFS_EXEC_TRANSPORT="daemon", LAYERFS_FUSE_TRANSPORT="daemon", LAYERFS_DAEMON_TCP_ENDPOINT=f"127.0.0.1:{port}",
                       LAYERFS_DAEMON_CAPABILITY=capability, LAYERFS_DAEMON_CONTAINER_ID=container, LAYERFS_FUSE_HOST="host.docker.internal")
            sampler_stderr = (attempt / "sampler-stderr.txt").open("xb")
            sampler = subprocess.Popen(["docker", "exec", name, "/usr/local/bin/fs-benchmark-workload", "workspace-resource-sample"], stdout=subprocess.PIPE, stderr=sampler_stderr, start_new_session=True)
            ready=threading.Event()
            sampler_thread=threading.Thread(target=stream_samples,args=(sampler,cgroup_path,ready,observer_errors),daemon=True); sampler_thread.start()
            if not ready.wait(5) or observer_errors or sampler.poll() is not None: raise RuntimeError("cgroup sampler failed readiness")
            outcome["preparation_ns"] = time.monotonic_ns()-started
        argv = [binary, "workspace-run", str(mutable), str(sample_input), case["scenario_id"], str(seed), args.mode, container]
        custody.write_json(attempt / "command.json", {"argv": argv, "environment_names": sorted(k for k in env if k.startswith("LAYERFS_")), "capability": "redacted"})
        def product_budget_trip(event):
            value = product_budget_observation(event)
            decision = record_suppression(campaign, case, assets["revision"], seed, attempt,
                {**event, "scenario_id": case["scenario_id"], "observed_product_ns": value})
            outcome.update(phase1_status=SUPPRESSION_STATUS, phase1_suppression=decision)
        result = bounded_run(argv, attempt / "raw.jsonl", attempt / "stderr.txt", deadline(case,args.mode), env, (cgroup_path, attempt / "sampler-stderr.txt"), mutable, observer_errors, sampler,
            product_budget_trip if args.mode == "performance" and not case.get("proof_only") else None)
        outcome.update(result)
        records = parse_records(attempt / "raw.jsonl")
        internal_deadlines=[r for r in records if r.get("kind")=="deadline-failure"]
        if internal_deadlines:
            outcome.update(timeout=True,internal_deadlines=internal_deadlines)
        outcome["coverage_status"] = "executed" if any(r.get("kind") in {"sample-start", "proof-start"} for r in records) else "unexecuted"
        complete_kind = "fast-verification-complete" if args.mode == "fast-verify" else "proof-complete" if case["family_id"] == "workspace_reliability" else "sample-complete"
        complete = [r for r in records if r.get("kind") == complete_kind]
        outcome["product_status"] = "pass" if result["exit_code"] == 0 and len(complete) == 1 else "fail"
        outcome["harness_status"] = "needs-review" if outcome["product_status"] != "pass" else "pending-validation"
        outcome["sample_complete"] = complete[0] if len(complete) == 1 else None
        if args.mode == "fast-verify":
            folder = attempt / "verification" / "fast-verification";folder.mkdir(parents=True, exist_ok=True)
            custody.write_json(folder / "receipts.json", [row for row in records if row.get("kind") in {"fast-canonical-verification", "fast-native-verification", "fast-dedup-verification", "fast-history-complete", "fast-verification-complete"}])
        if args.mode != "performance":outcome["assurance_status"] = ("fast_iteration_verified" if args.mode == "fast-verify" else "fully_verified") if outcome["product_status"] == "pass" else "not_verified"
        measured = completed_product_time(case, outcome)
        if measured is not None and measured > PHASE1_PRODUCT_LIMIT_NS:
            decision = record_suppression(campaign, case, assets["revision"], seed, attempt,
                {"scenario_id": case["scenario_id"], "limit_ns": PHASE1_PRODUCT_LIMIT_NS, "observed_product_ns": measured,
                 "kind": "completed-performance-sum", "measurement": "phase.initialize.elapsed_ns" if case.get("input_mode") == "directory" else "sample-complete.pure_call_sum_ns"})
            outcome.update(phase1_status=SUPPRESSION_STATUS, phase1_suppression=decision)
        outcome["other_product_failure"] = any(row.get("kind") == "product-budget-phase" and row.get("state") == "end" and row.get("phase_error") is not None for row in records)
        outcome["other_resource_failure"] = any(row.get("kind") in {"resource-failure", "host-rss-failure", "host-resource-failure", "required-observation-failure", "monitor-observation-failure", "spool-observation-failure", "product-budget-observation-error"} for row in records)
        if outcome["coverage_status"] != "executed": outcome.update(harness_status="fail", product_status="not-run")
        if observer_errors or any(r.get("kind") in {"host-rss-failure","host-resource-failure","product-budget-observation-error"} for r in records): outcome["harness_status"] = "fail"
    except BaseException as error:
        outcome.update(getattr(error, "phase1_result", {}))
        outcome.update(error=f"{type(error).__name__}: {error}", harness_status="fail")
        raw_path = attempt / "raw.jsonl"
        if raw_path.exists():
            began = False
            for line in raw_path.read_text(errors="replace").splitlines():
                try:row = json.loads(line.removeprefix("RELIABILITY\t"))
                except ValueError:continue
                began |= row.get("kind") in {"sample-start", "proof-start"}
            if began:
                outcome["coverage_status"] = "executed"
                if outcome["product_status"] == "not-run":outcome["product_status"] = "fail"
        if isinstance(error, subprocess.CalledProcessError):
            (attempt / "supervisor-command-stderr.txt").write_text(error.stderr or "")
        if isinstance(error, KeyboardInterrupt): outcome["interrupted"] = True
    finally:
        cleanup_start = time.monotonic_ns()
        try:
            with phase_deadline(60, "owned cleanup"):
                if runtime_started:
                    try:
                        after=json.loads(command(["docker", "inspect", name], timeout=10))[0]
                        custody.write_json(attempt / "container-after.json", after)
                        outcome["container_oom_killed"] = after["State"]["OOMKilled"]
                        outcome["runtime_running_after_client"] = after["State"]["Running"]
                    except BaseException as error: outcome["cleanup_inspection_error"] = str(error)
                    # Attempt removal even when inspection/startup failed.
                    removed=subprocess.run(["docker", "rm", "-f", name], stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=40)
                    if removed.returncode:
                        remaining=command(["docker", "ps", "-aq", "--filter", f"name=^/{name}$"], timeout=5)
                        if remaining: raise RuntimeError(removed.stderr.decode(errors="replace"))
                    outcome["supervisor_cleanup_status"] = "pass"
                if sampler is not None:
                    try: sampler.wait(timeout=5)
                    except subprocess.TimeoutExpired: kill_group(sampler)
                if sampler_thread is not None:
                    sampler_thread.join(timeout=5)
                    if sampler_thread.is_alive(): raise RuntimeError("sampler output did not quiesce")
                if sampler_stderr is not None: sampler_stderr.close()
                if observer_errors: outcome.update(harness_status="fail", observer_errors=observer_errors)
                if args.mode in {"verify", "fast-verify"}:
                    artifact_name = "fast-verification" if args.mode == "fast-verify" else "canonical-verification"
                    for directory in sorted(mutable.rglob(artifact_name)):
                        retained=attempt/"verification"/directory.relative_to(mutable)
                        retained.parent.mkdir(parents=True,exist_ok=True)
                        shutil.move(str(directory),str(retained))
                if outcome.get("product_status") == "pass" and outcome.get("harness_status") != "fail":
                    shutil.rmtree(mutable); outcome["mutable_sample_cleanup_status"] = "pass"
                else: outcome["mutable_sample_cleanup_status"] = "retained-for-investigation"
        except BaseException as error:
            outcome.update(supervisor_cleanup_status="fail", cleanup_error=str(error))
        outcome["cleanup_ns"] = time.monotonic_ns()-cleanup_start
        outcome["command_wall_ns"] = time.monotonic_ns()-started
        if sum(p.stat().st_size for p in attempt.rglob("*") if p.is_file()) > LOG_LIMIT:
            outcome.update(harness_status="fail", retained_output_resource_status="fail-64MiB-limit")
        custody.write_json(attempt / "outcome.json", outcome)
        custody.seal(attempt)
    return outcome


def successful(row):
    return row.get("coverage_status") == "executed" and row.get("product_status") == "pass" and row.get("harness_status") not in {"fail", "needs-review"} and row.get("supervisor_cleanup_status") == "pass"


def ledger_action(previous, reason):
    if previous is None: return "execute"
    if reason: return "invalidate"
    return "reuse-recorded-outcome" if successful(previous) else "retained-failure-needs-investigation"


def slot_key(row):
    return ":".join(str(row[key]) for key in ("harness_identity", "product_identity", "image_id", "environment_identity", "scenario_id", "seed", "mode"))


def reconcile_attempts(campaign, ledger):
    """Recover sealed outcomes, never infer success from partial output."""
    known = {row["evidence_path"] for row in ledger.values()}
    invalidations = campaign / "invalidations.jsonl"
    if invalidations.exists():
        known.update(json.loads(line)["previous_evidence"] for line in invalidations.read_text().splitlines() if line)
    recovered, incomplete = [], []
    for attempt in sorted((campaign / "attempts").glob("*")):
        if not attempt.is_dir() or str(attempt) in known:
            continue
        try:
            row = read_json(attempt / "outcome.json")
            custody.verify_manifest(attempt)
            if row.get("evidence_path") != str(attempt):
                raise ValueError("orphan path binding mismatch")
            key = slot_key(row)
        except (OSError, ValueError, KeyError, AssertionError) as error:
            incomplete.append({"evidence_path": str(attempt), "status": "interrupted-or-invalid; never reused", "reason": str(error)})
            continue
        if key in ledger:
            raise ValueError(f"multiple sealed attempts for slot {key}; explicit investigation required")
        ledger[key] = row
        recovered.append(str(attempt))
    if recovered or incomplete:
        path = campaign / "recovery"; path.mkdir(exist_ok=True)
        atomic_json(path / (uuid.uuid4().hex + ".json"), {"recovered_sealed_attempts": recovered, "retained_incomplete_attempts": incomplete})
    return recovered


@contextlib.contextmanager
def invocation_receipt(path, value, started):
    atomic_json(path, value)
    try:
        yield value
    except BaseException as error:
        value.update(status="interrupted" if isinstance(error, KeyboardInterrupt) else "failed-invocation", error=f"{type(error).__name__}: {error}")
        raise
    finally:
        value["invocation_wall_ns"] = time.monotonic_ns() - started
        atomic_json(path, value)


def schedule(case, args):
    if case.get("proof_only"):
        if args.mode != "verify": return ()
        if args.repetition is not None or args.seed not in (None,1): raise ValueError("proof recipes use aggregate seed1 exactly once")
        return (1,)
    if case.get("inherited"):
        if args.seed is not None: raise ValueError("inherited rows use --repetition, not --seed")
        if args.mode == "verify":
            if args.repetition not in (None,1): raise ValueError("inherited fixed-input verifier runs once at repetition1")
            return (1,)
        return tuple(range(1,6)) if args.all else (args.repetition,)
    if args.repetition is not None: raise ValueError("new cases use --seed 1..3, never inherited repetitions")
    return (1,2,3) if args.all else (args.seed,)


def self_check():
    with tempfile.TemporaryDirectory(prefix="layerfs-runner-check-") as directory:
        root=Path(directory)
        for name,code,limit,expected in [("pass","print('{}')",1,0),("fail","import sys;sys.exit(7)",1,7),("timeout","import time;time.sleep(10)",.05,124)]:
            result=bounded_run([sys.executable,"-c",code],root/(name+".out"),root/(name+".err"),limit,dict(os.environ))
            assert result["exit_code"] == expected, name
        source=root/"records";source.write_text('RELIABILITY\t{"kind":"proof-complete"}\n{"kind":"sample-complete"}\n')
        assert [r["kind"] for r in parse_records(source)] == ["proof-complete","sample-complete"]
        assert not successful({"coverage_status":"executed","product_status":"fail"})
        failed={"coverage_status":"executed","product_status":"fail"}
        assert ledger_action(failed,None)=="retained-failure-needs-investigation"
        assert ledger_action(failed,"repaired fixture route")=="invalidate"
        assert ledger_action(None,None)=="execute"
        base={"proof_only":False,"inherited":False,"family_id":"payload_create_read","tier":1,"operation":"payload-create"}
        args=argparse.Namespace(mode="performance",all=True,seed=None,repetition=None)
        assert schedule(base,args)==(1,2,3)
        args.mode="verify"; assert schedule(dict(base,proof_only=True),args)==(1,)
        args.mode="performance";assert schedule(dict(base,inherited=True),args)==(1,2,3,4,5)
    print("runner_self_check=pass")


def recovery_self_check():
    from unittest.mock import patch
    with tempfile.TemporaryDirectory(prefix="layerfs-runner-recovery-") as directory:
        root=Path(directory).resolve();attempt=root/"attempts"/"sealed";attempt.mkdir(parents=True)
        row=dict(zip(("harness_identity","product_identity","image_id","environment_identity","scenario_id","seed","mode"), ("h","p","i","e","case",1,"performance")), evidence_path=str(attempt), coverage_status="executed", product_status="fail")
        custody.write_json(attempt/"outcome.json",row);custody.seal(attempt)
        partial=root/"attempts"/"partial";partial.mkdir();(partial/"raw.jsonl").write_text('{"kind":"sample-start"}\n')
        ledger={};assert reconcile_attempts(root,ledger)==[str(attempt)]
        assert ledger[slot_key(row)]==row and ledger_action(row,None)=="retained-failure-needs-investigation"
        assert reconcile_attempts(root,ledger)==[] and partial.exists()
        receipt=root/"invocation.json";value={"status":"running"}
        try:
            with invocation_receipt(receipt,value,time.monotonic_ns()):raise KeyboardInterrupt()
        except KeyboardInterrupt:pass
        assert read_json(receipt)["status"]=="interrupted" and value["invocation_wall_ns"]>=0
        campaign=root/"locked";campaign.mkdir()
        registry={"scenario_id":"case","family_id":"family","tier":1,"operation":"op"}
        with (campaign/"measurement.lock").open("a") as lock:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            argv=["runner","--assets",str(root),"--output",str(campaign),"--case","case","--seed","1"]
            with patch.object(sys,"argv",argv), patch(__name__+".source_validation",return_value={}), patch(__name__+".command",return_value=json.dumps(registry)):
                try:main()
                except BlockingIOError:pass
                else:raise AssertionError("competing measurement acquired lock")
            assert not (campaign/"invocations").exists()
    print("runner_recovery_self_check=pass")


def main():
    invocation_started=time.monotonic_ns()
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--family");p.add_argument("--case")
    p.add_argument("--source-arm", choices=("baseline", "corrected"), default="baseline")
    repeat=p.add_mutually_exclusive_group();repeat.add_argument("--seed",type=int,choices=(1,2,3));repeat.add_argument("--repetition",type=int,choices=(1,2,3,4,5))
    p.add_argument("--mode",choices=("performance","verify","fast-verify","qualify-input"),default="performance")
    p.add_argument("--all",action="store_true");p.add_argument("--extended",action="store_true")
    p.add_argument("--invalidate-reason",help="Explicitly recollect selected prior slots, preserving their raw outcomes and reason")
    p.add_argument("--fast-no-reuse",action="store_true",help="Explicitly check all uncertified current content; no prior input certificate")
    p.add_argument("--fast-components",action="store_true",help="Reuse only independently recipe-mapped certified content components")
    p.add_argument("--verification-certificate",help="Retained admitted full-verification attempt for the separate fast-verify profile")
    p.add_argument("--self-check",action="store_true");p.add_argument("--recovery-self-check",action="store_true");p.add_argument("--build")
    p.add_argument("--assets",default=os.environ.get("LAYERFS_V013_ASSETS"))
    p.add_argument("--preparation-assets",help="Compatible sealed producer used only for fixture/oracle acquisition; defaults to --assets")
    p.add_argument("--output",default=str(REPO / "benchmark-results/fs-bench-pro/phase1-v013"));p.add_argument("--cache",default=str(REPO / "target/phase1-prepared"))
    args=p.parse_args()
    if args.recovery_self_check: recovery_self_check();return 0
    if args.self_check: self_check();return 0
    if args.build: build_assets(args);return 0
    if not args.assets:p.error("--assets must select a sealed build")
    if args.mode == "fast-verify" and (args.all or bool(args.verification_certificate)==bool(args.fast_no_reuse) or args.seed is None):p.error("fast-verify requires one case/seed and exactly one qualified certificate or explicit --fast-no-reuse")
    if args.fast_components and not args.verification_certificate:p.error("component reuse requires an explicit qualified source certificate")
    if args.mode != "fast-verify" and (args.fast_no_reuse or args.fast_components):p.error("fast profile flags require fast-verify")
    if args.mode == "qualify-input" and (args.all or args.seed is None):p.error("qualify-input requires one case/seed")
    if args.mode != "fast-verify" and args.verification_certificate:p.error("--verification-certificate is exclusive to fast-verify")
    if args.all == bool(args.case) or (args.all and (args.seed is not None or args.repetition is not None)) or (not args.all and args.seed is None and args.repetition is None):p.error("select --case with --seed/--repetition, or explicit --all")
    if args.invalidate_reason is not None and not args.invalidate_reason.strip():p.error("invalidation reason must be nonempty")
    for key in ("LAYERFS_EXEC_INJECT_DISCONNECT","LAYERFS_WORKSPACE_INJECT_POST_ATTACH_FAILURE"):
        if os.environ.get(key)=="1":p.error(f"frozen no-injection profile rejects {key}=1")
    validation_started=time.monotonic_ns()
    build=Path(args.assets).resolve();assets=source_validation(build)
    validation_ns=time.monotonic_ns()-validation_started
    registry_started=time.monotonic_ns()
    registry=[json.loads(line) for line in command([build/"fs-benchmark-pro","workspace-registry"]).splitlines()]
    registry_ns=time.monotonic_ns()-registry_started
    selected=[r for r in registry if (not args.family or r["family_id"]==args.family) and (not args.case or r["scenario_id"]==args.case)]
    if args.mode=="performance":selected=[r for r in selected if not r.get("proof_only")]
    if not selected or (not args.all and len(selected)!=1):p.error("unknown, ambiguous or proof-only performance selection")
    if args.all and not args.family:p.error("--all requires one family")
    try: planned=[(case,seed) for case in selected for seed in schedule(case,args)]
    except ValueError as error:p.error(str(error))
    if args.all and args.family=="edit_length_changing_capped" and args.mode=="performance":
        if len(selected)!=5:raise ValueError("five inherited definitions required")
        rotations=((0,1,2,3,4),(2,3,4,0,1),(4,0,1,2,3),(1,2,3,4,0),(4,0,1,2,3))
        planned=[(selected[index],rep) for rep,order in enumerate(rotations,1) for index in order]
    if any(seed is None for _,seed in planned):p.error("selected row requires its matching seed or repetition selector")
    campaign=Path(args.output).resolve();campaign.mkdir(parents=True,exist_ok=True)
    failures=False
    with (campaign/"measurement.lock").open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        suppressions = load_suppressions(campaign)
        active = [case for case in selected if not is_suppressed(case, suppressions)]
        if any((r["family_id"]=="dedup_branch_history" and r["tier"]>=100) or (r["family_id"]=="workspace_reliability" and r["operation"] in EXTENDED) for r in active) and not args.extended:p.error("required active extended members need explicit --extended")
        producer_started = time.monotonic_ns()
        producer = select_preparation(build, assets, Path(args.preparation_assets or args.assets).resolve(), registry, active) if active else None
        producer_validation_ns = time.monotonic_ns() - producer_started
        args.fast_certificate = None
        if args.mode == "fast-verify" and active:
            if len(active) != 1 or active[0].get("proof_only") or active[0].get("inherited") or active[0].get("operation") == "git-tool":
                raise ValueError("this fast profile is unavailable; targeted/error gates still require their exact proof")
            if not args.fast_no_reuse:args.fast_certificate = verification_certificate(args.verification_certificate, active[0], args.seed, assets, campaign, registry, args.fast_components)
        if args.mode == "qualify-input":
            if active:qualify_input(active[0],args.seed,args,assets,campaign,producer)
            return 0
        invocations=campaign/"invocations";invocations.mkdir(exist_ok=True)
        # Exclusive ownership proves previous running records have no current
        # coordinator. Do not invent command duration after a hard interruption.
        for prior in invocations.glob("*.json"):
            record=read_json(prior)
            if record.get("status")=="running":
                record.update(status="interrupted-unmeasured-wall", recovery_reason="exclusive campaign lock acquired after prior coordinator ended")
                atomic_json(prior,record)
        invocation_path=invocations/(uuid.uuid4().hex+".json")
        invocation={"source_arm":args.source_arm,"source_revision":assets["revision"],"image_id":assets["image_id"],"source_validation_ns":validation_ns,"registry_query_ns":registry_ns,"planned_slots":[[case["scenario_id"],seed,args.mode] for case,seed in planned],"status":"running","invocation_wall_ns":None}
        invocation["preparation_producer"] = {"assets": str(Path(args.preparation_assets or args.assets).resolve()),
            "revision": producer["revision"], "image_id": producer["image_id"], "validation_ns": producer_validation_ns,
            "source_compatibility": producer.get("preparation_source_compatibility")} if producer else None
        invocation["suppressed_slots"] = []
        processed_active = 0
        def note_suppressed(case, seed, record):
            skipped = {"status": SUPPRESSION_STATUS, "scenario_id": case["scenario_id"], "seed": seed, "mode": args.mode,
                "suppression": record, "scope": "Phase1 exclusion, not a product pass or a newly executed sample"}
            invocation["suppressed_slots"].append(skipped)
            print(json.dumps(skipped, sort_keys=True), flush=True)
        with invocation_receipt(invocation_path,invocation,invocation_started):
            ledger_path=campaign/"slots.json";ledger=read_json(ledger_path) if ledger_path.exists() else {};acquisitions={}
            if reconcile_attempts(campaign,ledger):atomic_json(ledger_path,ledger)
            for case,seed in planned:
                suppressions = load_suppressions(campaign)
                if is_suppressed(case, suppressions):
                    note_suppressed(case, seed, suppressions["cases"][case["scenario_id"]])
                    continue
                key=slot_key({"harness_identity":assets["harness_seal"],"product_identity":assets["product_seal"],"image_id":assets["image_id"],"environment_identity":assets["environment_identity"],"scenario_id":case["scenario_id"],"seed":seed,"mode":args.mode})
                previous=ledger.get(key)
                if previous and previous.get("source_arm") != args.source_arm:
                    raise ValueError("retained outcome belongs to a different named source arm")
                action=ledger_action(previous,args.invalidate_reason)
                if action in {"reuse-recorded-outcome","retained-failure-needs-investigation"}:
                    measured = completed_product_time(case, previous)
                    if measured is not None and measured > PHASE1_PRODUCT_LIMIT_NS:
                        retained_path = Path(previous["evidence_path"])
                        custody.verify_manifest(retained_path)
                        sealed_previous = read_json(retained_path / "outcome.json")
                        if any(previous.get(key) != value for key, value in sealed_previous.items()):
                            raise ValueError("retained budget trigger differs from its sealed outcome")
                        record = record_suppression(campaign, case, previous["source_revision"], seed, previous["evidence_path"],
                            {"scenario_id": case["scenario_id"], "limit_ns": PHASE1_PRODUCT_LIMIT_NS, "observed_product_ns": measured, "kind": "completed-performance-sum", "measurement": "retained completed product sum"})
                        note_suppressed(case, seed, record)
                        if not budget_suppression_can_continue(previous):failures = True;break
                        continue
                    processed_active += 1
                    print(json.dumps({"action":action,"case":case["scenario_id"],"seed":seed,"evidence":previous["evidence_path"]}),flush=True)
                    failures |= not successful(previous)
                    if failures:break
                    continue
                if previous:
                    change={"slot":key,"previous_evidence":previous["evidence_path"],"reason":args.invalidate_reason,"at_unix_ns":time.time_ns()}
                    with (campaign/"invalidations.jsonl").open("a") as stream:stream.write(json.dumps(change,sort_keys=True)+"\n")
                processed_active += 1
                result=sample(case,seed,args,assets,campaign,acquisitions,producer)
                if previous:result["previous_evidence_path"]=previous["evidence_path"]
                ledger[key]=result;atomic_json(ledger_path,ledger)
                print(json.dumps(result,sort_keys=True),flush=True)
                if result.get("phase1_status") == SUPPRESSION_STATUS:
                    note_suppressed(case, seed, result["phase1_suppression"])
                    if budget_suppression_can_continue(result):continue
                failures |= not successful(result)
                if failures or result.get("interrupted"):break
            invocation["status"] = "failed-outcomes" if failures else ("completed_with_suppressions" if processed_active else SUPPRESSION_STATUS) if invocation["suppressed_slots"] else "pass"
    print(json.dumps({"invocation_receipt":str(invocation_path),"invocation_wall_ns":invocation["invocation_wall_ns"],"status":invocation["status"]}),flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    def terminate(signum, frame):
        raise KeyboardInterrupt(f"received signal {signum}")
    signal.signal(signal.SIGTERM, terminate)
    sys.exit(main())
