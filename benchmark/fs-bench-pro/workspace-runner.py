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


def bounded_run(argv, out, err, seconds, env, resource_files=(), mutable=None, observer_errors=None, observer_process=None):
    started = time.monotonic_ns()
    reason = None
    with Path(out).open("xb") as stdout, Path(err).open("xb") as stderr:
        child = subprocess.Popen(argv, stdout=stdout, stderr=stderr, env=env, start_new_session=True)
        try:
            while child.poll() is None:
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
        except BaseException:
            kill_group(child)
            raise
        if sum(p.stat().st_size for p in (Path(out), Path(err), *resource_files) if p.exists()) > LOG_LIMIT:
            reason = "retained-log-limit"
        code = 124 if reason == "case-timeout" else 125 if reason else child.returncode
    return {"exit_code": code, "timeout": reason == "case-timeout", "supervisor_failure": reason,
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


def source_validation(build):
    custody.verify_manifest(build / "evidence")
    assets = read_json(build / "evidence/build.json")
    if assets["schema"] != "fs-bench-pro-workspace-build-v1" or assets["status"] != "pass": raise ValueError("unqualified Workspace build")
    if custody.sha(build / "fs-benchmark-pro") != assets["binary_sha256"]: raise ValueError("host binary seal")
    for name in ("workspace-runner.py", "sdk-edit-custody.py", "lib-runtime.sh"):
        expected = subprocess.check_output(["git", "show", f"{assets['revision']}:benchmark/fs-bench-pro/{name}"], cwd=REPO)
        if (HERE / name).read_bytes() != expected: raise ValueError(f"running {name} differs from sealed build revision")
    assets["workspace_preparation_compatibility"] = custody.workspace_preparation_digest(assets)
    docker = json.loads(command(["docker", "info", "--format", "{{json .}}"] ))
    environment = {"host": platform.uname()._asdict(), "host_cpu_count": os.cpu_count(),
                   "docker": {key: docker.get(key) for key in ("ID", "ServerVersion", "KernelVersion", "OperatingSystem", "OSType", "Architecture", "NCPU", "MemTotal")},
                   "resource_profile": "v013-macos-docker-linux-fuse-ack-window-v1"}
    assets["runtime_environment"] = environment
    assets["environment_identity"] = hashlib.sha256(json.dumps(environment, sort_keys=True).encode()).hexdigest()
    return assets


def acquire(case, seed, binary, cache, run, build, acquisitions, assets, reference=False):
    started = time.monotonic_ns()
    info = json.loads(command([binary, "workspace-reference-info" if reference else "workspace-fixture-info", case["scenario_id"], seed]))
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


def stream_samples(process, target, ready, errors):
    try:
        with gzip.open(target, "xb") as stream:
            for line in iter(process.stdout.readline, b""):
                stream.write(line)
                if not ready.is_set(): stream.flush(); ready.set()
    except BaseException as error:
        errors.append(str(error)); ready.set()


def sample(case, seed, args, assets, campaign, acquisitions):
    started = time.monotonic_ns()
    attempt = campaign / "attempts" / f"{case['scenario_id']}-s{seed}-{args.mode}-{uuid.uuid4().hex[:12]}"
    attempt.mkdir(parents=True)
    prepared_dir = attempt / "preparation"; prepared_dir.mkdir()
    mutable = campaign / "scratch" / attempt.name; mutable.mkdir(parents=True)
    name = "layerfs-v013-" + uuid.uuid4().hex[:16]
    binary = str(Path(args.assets).resolve() / "fs-benchmark-pro")
    evidence = Path(args.assets).resolve() / "evidence"
    env = dict(os.environ, LAYERFS_V013_IMAGE=assets["image_id"], LAYERFS_V013_RESOURCE_PROFILE="1")
    for key in ("LAYERFS_V013_GIT_REFERENCE_HOST", "LAYERFS_V013_VERIFIER_EXCHANGE", "LAYERFS_V013_VERIFIER_EXCHANGE_HOST"):
        env.pop(key, None)
    outcome = {"command_wall_scope":"one sample preparation/runtime/product/cleanup; CLI validation is in invocation receipt", "schema": "fs-bench-pro-v013-sample-v1", "scenario_id": case["scenario_id"], "family_id": case["family_id"],
               "seed": seed, "seed_label": f"repetition-{seed}" if case.get("inherited") else f"layerfs-v0.1.3-seed-{seed}",
               "proof_only": bool(case.get("proof_only")), "inherited": bool(case.get("inherited")), "mode": args.mode,
               "source_revision": assets["revision"], "product_identity": assets["product_seal"], "harness_identity": assets["harness_seal"],
               "contract_commit": assets["phase1_contract_commit"], "image_id": assets["image_id"], "source_arm": "baseline", "admission_eligible": False,
               "environment_identity": assets["environment_identity"], "report_generator_identity": assets["report_generator_sha256"],
               "mutable_diagnostic_path": str(mutable), "coverage_status": "unexecuted", "harness_status": "in-progress", "product_status": "not-run", "evidence_path": str(attempt),
               "invalidation_reason": args.invalidate_reason}
    sampler = None; sampler_thread = None; sampler_stderr = None
    observer_errors = []; runtime_started = False
    cgroup_path = attempt / "cgroup-samples.tsv.gz"
    custody.write_json(attempt / "environment.json", assets["runtime_environment"])
    try:
        with phase_deadline(preparation_deadline(case), "selected input/runtime preparation"):
            old_image = os.environ.get("LAYERFS_V013_IMAGE")
            os.environ["LAYERFS_V013_IMAGE"] = assets["image_id"]
            try:
                acquired = acquire(case, seed, binary, Path(args.cache).resolve(), prepared_dir, evidence, acquisitions, assets)
                reference = None
                if case["operation"] == "git-tool":
                    reference_dir=prepared_dir / "reference"; reference_dir.mkdir()
                    reference=acquire(case, seed, binary, Path(args.cache).resolve(), reference_dir, evidence, acquisitions, assets, reference=True)
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
            if case["operation"] == "git-tool" and args.mode == "verify":
                exchange=attempt / "verifier-exchange"; exchange.mkdir()
                env["LAYERFS_V013_VERIFIER_EXCHANGE"] = str(exchange)
                env["LAYERFS_V013_VERIFIER_EXCHANGE_HOST"] = str(exchange)
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
        result = bounded_run(argv, attempt / "raw.jsonl", attempt / "stderr.txt", deadline(case,args.mode), env, (cgroup_path, attempt / "sampler-stderr.txt"), mutable, observer_errors, sampler)
        outcome.update(result)
        records = parse_records(attempt / "raw.jsonl")
        internal_deadlines=[r for r in records if r.get("kind")=="deadline-failure"]
        if internal_deadlines:
            outcome.update(timeout=True,internal_deadlines=internal_deadlines)
        outcome["coverage_status"] = "executed" if any(r.get("kind") in {"sample-start", "proof-start"} for r in records) else "unexecuted"
        complete_kind = "proof-complete" if case["family_id"] == "workspace_reliability" else "sample-complete"
        complete = [r for r in records if r.get("kind") == complete_kind]
        outcome["product_status"] = "pass" if result["exit_code"] == 0 and len(complete) == 1 else "fail"
        outcome["harness_status"] = "needs-review" if outcome["product_status"] != "pass" else "pending-validation"
        outcome["sample_complete"] = complete[0] if len(complete) == 1 else None
        if outcome["coverage_status"] != "executed": outcome.update(harness_status="fail", product_status="not-run")
        if observer_errors or any(r.get("kind") in {"host-rss-failure","host-resource-failure"} for r in records): outcome["harness_status"] = "fail"
    except BaseException as error:
        outcome.update(error=f"{type(error).__name__}: {error}", harness_status="fail")
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


def main():
    invocation_started=time.monotonic_ns()
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--family");p.add_argument("--case")
    repeat=p.add_mutually_exclusive_group();repeat.add_argument("--seed",type=int,choices=(1,2,3));repeat.add_argument("--repetition",type=int,choices=(1,2,3,4,5))
    p.add_argument("--mode",choices=("performance","verify"),default="performance")
    p.add_argument("--all",action="store_true");p.add_argument("--extended",action="store_true")
    p.add_argument("--invalidate-reason",help="Explicitly recollect selected prior slots, preserving their raw outcomes and reason")
    p.add_argument("--self-check",action="store_true");p.add_argument("--build")
    p.add_argument("--assets",default=os.environ.get("LAYERFS_V013_ASSETS"))
    p.add_argument("--output",default=str(REPO / "benchmark-results/fs-bench-pro/phase1-v013"));p.add_argument("--cache",default=str(REPO / "target/phase1-prepared"))
    args=p.parse_args()
    if args.self_check: self_check();return 0
    if args.build: build_assets(args);return 0
    if not args.assets:p.error("--assets must select a sealed build")
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
    if any((r["family_id"]=="dedup_branch_history" and r["tier"]>=100) or (r["family_id"]=="workspace_reliability" and r["operation"] in EXTENDED) for r in selected) and not args.extended:p.error("required extended members need explicit --extended")
    try: planned=[(case,seed) for case in selected for seed in schedule(case,args)]
    except ValueError as error:p.error(str(error))
    if args.all and args.family=="edit_length_changing_capped" and args.mode=="performance":
        if len(selected)!=5:raise ValueError("five inherited definitions required")
        rotations=((0,1,2,3,4),(2,3,4,0,1),(4,0,1,2,3),(1,2,3,4,0),(4,0,1,2,3))
        planned=[(selected[index],rep) for rep,order in enumerate(rotations,1) for index in order]
    if any(seed is None for _,seed in planned):p.error("selected row requires its matching seed or repetition selector")
    campaign=Path(args.output).resolve();campaign.mkdir(parents=True,exist_ok=True)
    invocations=campaign/"invocations";invocations.mkdir(exist_ok=True)
    invocation_path=invocations/(uuid.uuid4().hex+".json")
    invocation={"source_revision":assets["revision"],"image_id":assets["image_id"],"source_validation_ns":validation_ns,"registry_query_ns":registry_ns,"planned_slots":[[case["scenario_id"],seed,args.mode] for case,seed in planned],"status":"running","invocation_wall_ns":None}
    atomic_json(invocation_path,invocation)
    failures=False
    with (campaign/"measurement.lock").open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        ledger_path=campaign/"slots.json";ledger=read_json(ledger_path) if ledger_path.exists() else {};acquisitions={}
        for case,seed in planned:
            key=f"{assets['harness_seal']}:{assets['product_seal']}:{assets['image_id']}:{assets['environment_identity']}:{case['scenario_id']}:{seed}:{args.mode}"
            previous=ledger.get(key)
            action=ledger_action(previous,args.invalidate_reason)
            if action in {"reuse-recorded-outcome","retained-failure-needs-investigation"}:
                print(json.dumps({"action":action,"case":case["scenario_id"],"seed":seed,"evidence":previous["evidence_path"]}),flush=True)
                failures |= not successful(previous)
                continue
            if previous:
                change={"slot":key,"previous_evidence":previous["evidence_path"],"reason":args.invalidate_reason,"at_unix_ns":time.time_ns()}
                with (campaign/"invalidations.jsonl").open("a") as stream:stream.write(json.dumps(change,sort_keys=True)+"\n")
            result=sample(case,seed,args,assets,campaign,acquisitions)
            if previous:result["previous_evidence_path"]=previous["evidence_path"]
            ledger[key]=result;atomic_json(ledger_path,ledger)
            print(json.dumps(result,sort_keys=True),flush=True)
            failures |= not successful(result)
            if result.get("interrupted"):break
    invocation.update(status="failed-outcomes" if failures else "pass",invocation_wall_ns=time.monotonic_ns()-invocation_started)
    atomic_json(invocation_path,invocation)
    print(json.dumps({"invocation_receipt":str(invocation_path),"invocation_wall_ns":invocation["invocation_wall_ns"],"status":invocation["status"]}),flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
