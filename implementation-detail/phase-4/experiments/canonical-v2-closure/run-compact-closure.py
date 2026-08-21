#!/usr/bin/env python3
"""One globally supervised compact canonical-v2 closure loop."""

import csv
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
ROOT = REPO / "target/phase4-canonical-v2-closure-20260821-v2/compact-results-v1"
LOCK = Path("/tmp/layerfs-CANONICAL_V2_COMPACT.lock")
CONTROL = REPO / "target/phase4-canonical-v2-exploration-20260821-v1/control/phase4_create_edit_benchmark-cp0009"
CONTROL_SOURCE = REPO / "target/phase4-canonical-v2-exploration-20260821-v1/control/phase4_create_edit_benchmark-cp0009.rs"
CANDIDATE_BUILD = REPO / "target/release/phase4_create_edit_benchmark"
ANALYZER = HERE / "analyze-compact-closure.py"
MANIFEST_TOOL = HERE / "manifest-bundle.py"
METHODOLOGY = HERE / "PROSPECTIVE-METHODOLOGY-CUSTODY-COMPACT-v1.tsv"
PREREG = HERE / "PROSPECTIVE-COMPACT-CLOSURE-v1.md"
ORACLE = HERE / "INDEPENDENT-FIXTURE-ORACLE-v1.tsv"
HISTORY_MANIFEST = REPO / "target/phase4-canonical-v2-closure-20260821-v1/final-candidate-v1/TERMINAL-MANIFEST-v1.tsv"
HISTORY_CLARIFICATION = REPO / "target/phase4-canonical-v2-closure-20260821-v1/final-candidate-v1-clarification-v1/TERMINAL-MANIFEST-v1.tsv"
CONTROL_SHA = "9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7"
CONTROL_SOURCE_SHA = "3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a"
FIXTURE_SHA = {
    1_048_576: "4a3acf60f044bbae8ed0d0a8aa8fabd8b4cee74216dbccc36255b9c6fbe50a2a",
    104_857_600: "63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4",
}
V1_BASE = "2d41c27f96b0332475fb8ec3c46a336c9c8a8084408bc545e5cbb24d51cb25d0,ba15fd20469414de99c135fc90a5c5ad028f99f115b8c0d138ace9ec98536412,d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a"
V1_PLUS_RESULT = {
    "plus1-early": "4648eb987df7b46844135218cdbd73cbd8480d34b74a832f123fdfb1221869eb,ac12e88bc47967043647484112ab5d1113d7f0ebbaa8c9026749b9123d8e949a,e86efa7aaeaaf8f983c8fcaf48b5c206ce6d53d2be502cfc05a33dede544c5f1",
    "plus1-middle": "41e9b48e1af960a4587027b929608d50686b59cd9dc22a625cbb5548379539b9,bfcc3537f01f17265ecef026e5fc5ccf4a4da599c4659ddd4259a8bd63ff74a9,4eb35ed21ded2bf3135d058a6a0da042db1af3c53d74d119e82c956a9c07110a",
}
PLUS_TEMPLATE = {
    "plus1-early": REPO / "target/phase4-h05-canonical-witness-screen-20260821-v1/screen-results-v6/work-v1/smoke-2-plus1-early/db-K64-F64-104857600-plus1-early-910002.sqlite.expectations",
    "plus1-middle": REPO / "target/phase4-h05-canonical-witness-screen-20260821-v1/screen-results-v6/work-v1/smoke-3-plus1-middle/db-K64-F64-104857600-plus1-middle-910003.sqlite.expectations",
}

deadline = 0.0
started = 0.0
current_child = None
lock_held = False
actual_sequence = 0


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write(path, contents):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents)


def remaining():
    return deadline - time.monotonic()


def stop_child():
    global current_child
    if current_child is not None and current_child.poll() is None:
        try:
            os.killpg(current_child.pid, signal.SIGTERM)
            current_child.wait(timeout=0.35)
        except Exception:
            try:
                os.killpg(current_child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def release_lock():
    global lock_held
    if not lock_held:
        return
    LOCK.rmdir()
    lock_held = False
    with (ROOT / "LOCK-v1.txt").open("a") as handle:
        handle.write(f"released_ns={time.time_ns()}\n")


def alarm_handler(_signum, _frame):
    signal.setitimer(signal.ITIMER_REAL, 0)
    stop_child()
    raise TimeoutError("global 119-second supervisor expired")


def run_child(label, command, env=None, check=True, timed_row=False):
    global current_child, actual_sequence
    if remaining() <= 0.6:
        raise TimeoutError("global time exhausted before child")
    timeout = min(59.0, remaining() - 0.4)
    logs = ROOT / "logs-v1"
    logs.mkdir(exist_ok=True)
    stdout_path = logs / f"{label}.stdout"
    stderr_path = logs / f"{label}.stderr"
    actual_sequence += 1
    with (ROOT / "ACTUAL-INVOCATIONS-v1.tsv").open("a") as ledger:
        ledger.write(f"{actual_sequence}\tstarted\t{time.time_ns()}\t{label}\t{' '.join(map(str, command))}\t-\n")
    before = time.monotonic()
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        current_child = subprocess.Popen(command, cwd=REPO, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            code = current_child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            stop_child()
            raise TimeoutError(f"child timeout: {label}")
        finally:
            current_child = None
    wall = time.monotonic() - before
    with (ROOT / "ACTUAL-INVOCATIONS-v1.tsv").open("a") as ledger:
        ledger.write(f"{actual_sequence}\tcompleted\t{time.time_ns()}\t{label}\t{' '.join(map(str, command))}\t{code}\n")
    if timed_row:
        text = stderr_path.read_text(errors="replace")
        match = re.search(r"([0-9.]+) real\s+([0-9.]+) user\s+([0-9.]+) sys", text)
        rss = re.search(r"(\d+)\s+maximum resident set size", text)
        peak = re.search(r"(\d+)\s+peak memory footprint", text)
        with (ROOT / "EXTERNAL-TIME-v1.tsv").open("a") as output:
            output.write(f"{label}\t{wall:.9f}\t{match.group(2) if match else 'Unavailable'}\t{match.group(3) if match else 'Unavailable'}\t{rss.group(1) if rss else 'Unavailable'}\t{peak.group(1) if peak else 'Unavailable'}\tUnavailable\tUnavailable\n")
    if check and code:
        raise RuntimeError(f"{label} exited {code}")
    return code, stdout_path, stderr_path


def verify_manifest(manifest):
    with manifest.open() as handle:
        for row in csv.DictReader(handle, delimiter="\t"):
            path = REPO / row["path"]
            if not path.is_file() or sha(path) != row["sha256"] or path.stat().st_size != int(row["size_bytes"]):
                raise RuntimeError(f"historical manifest mismatch: {row['path']}")


def verify_methodology():
    expected = os.environ.get("CANONICAL_V2_COMPACT_METHODOLOGY_SHA256")
    if not expected or sha(METHODOLOGY) != expected:
        raise RuntimeError("methodology custody anchor mismatch")
    rows = list(csv.DictReader(METHODOLOGY.open(), delimiter="\t"))
    labels = {row["label"] for row in rows}
    required = {"runner", "analyzer", "preregistration", "manifest-tool", "control", "control-source", "oracle", "historical-revise-manifest", "historical-clarification-manifest"}
    if labels != required:
        raise RuntimeError("methodology label set mismatch")
    for row in rows:
        path = REPO / row["path"]
        if not path.is_file() or sha(path) != row["sha256"] or path.stat().st_size != int(row["size_bytes"]):
            raise RuntimeError(f"methodology mismatch: {row['label']}")


def check_quiescence(label):
    output = subprocess.check_output(["/bin/ps", "-axo", "pid=,command="], text=True)
    conflicts = []
    for line in output.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        pid_text, _, command = stripped.partition(" ")
        try:
            pid = int(pid_text)
        except ValueError:
            continue
        if pid in {os.getpid(), os.getppid()} or "run-compact-closure.py" in command:
            continue
        if re.search(r"(?:^|[ /])(?:cargo|rustc|sqlite3|dtrace|fs_usage|iostat|fio|rsync|phase4_create_edit_benchmark)(?: |$)", command):
            conflicts.append(stripped)
    write(ROOT / f"QUIESCENCE-{label}-v1.txt", "status=" + ("PASS\n" if not conflicts else "FAIL\n") + "\n".join(conflicts) + ("\n" if conflicts else ""))
    if conflicts:
        raise RuntimeError(f"host quiescence failed: {label}")


def schedule():
    result = []

    def add(label, kind, size, operation, order, pair="-", comparable=True):
        for arm in list(order) if comparable else ["B"]:
            result.append({"sequence": len(result) + 1, "label": f"{label}-{arm}", "kind": kind, "size": size, "operation": operation, "arm": arm, "pair": str(pair), "order": order, "comparable": comparable})

    add("warm-full-100", "warmup", 104_857_600, "full", "AB")
    add("scale-full-1", "scaling", 1_048_576, "full", "AB")
    add("scale-full-10", "scaling", 10_485_760, "full", "BA")
    add("primary-full-100-p0", "primary", 104_857_600, "full", "AB", 0)
    add("primary-full-100-p1", "primary", 104_857_600, "full", "BA", 1)
    for label, operation, order in [("guard-same", "same-middle", "AB"), ("guard-plus1-early", "plus1-early", "BA"), ("guard-plus1-middle", "plus1-middle", "AB"), ("guard-materialize-warm", "materialize-warm", "BA"), ("guard-materialize-fresh", "materialize-fresh", "AB"), ("guard-reopen", "reopen", "BA"), ("guard-range1m", "read-range-1m", "AB")]:
        add(label, "guard", 104_857_600, operation, order)
    for label, operation in [("guard-one-byte-early", "one-byte-early"), ("guard-one-byte-middle", "one-byte-middle"), ("guard-one-byte-late", "one-byte-late"), ("guard-first-edit", "first-edit-after-reopen"), ("guard-scrub", "scrub-only")]:
        add(label, "candidate-only", 104_857_600, operation, "B", comparable=False)
    return result


def link_source(root, source):
    root.mkdir(parents=True, exist_ok=False)
    os.link(source, root / source.name)


def database(root, operation, iteration):
    return root / f"db-K64-F64-104857600-{operation}-{iteration}.sqlite"


def copy_image(source_db, target_db, expectations=None):
    subprocess.run(["/bin/cp", source_db, target_db], check=True)
    subprocess.run(["/bin/cp", Path(str(source_db) + ".authority"), Path(str(target_db) + ".authority")], check=True)
    source_expectations = expectations or Path(str(source_db) + ".expectations")
    subprocess.run(["/bin/cp", source_expectations, Path(str(target_db) + ".expectations")], check=True)


def command_for(operation, prepare=False):
    public = {
        "full": "write", "same-middle": "edit-same", "plus1-early": "edit-plus1-early", "plus1-middle": "edit-plus1-middle",
        "materialize-warm": "materialize-warm", "materialize-fresh": "materialize-fresh", "reopen": "reopen", "read-range-1m": "read-range-1m",
        "one-byte-early": "edit-one-byte-early", "one-byte-middle": "edit-one-byte-middle", "one-byte-late": "edit-one-byte-late",
        "first-edit-after-reopen": "first-edit-after-reopen", "scrub-only": "scrub-only",
    }[operation]
    if operation.startswith("plus1-"):
        return ("--count-change-scale-prepare" if prepare else "--count-change-scale-row", public)
    return ("--fast-prepare" if prepare else "--fast-row", public)


def make_v1_plus_expectation(operation, candidate):
    lines = PLUS_TEMPLATE[operation].read_text().splitlines()
    body = ["LFS-WP4M-EXPECTATIONS-3"]
    for line in lines[1:]:
        if line.startswith("canonical_commitment=") or line.startswith("manifest_blake3="):
            continue
        if line.startswith("base="):
            line = "base=" + V1_BASE
        elif line.startswith("result="):
            line = "result=" + V1_PLUS_RESULT[operation]
        body.append(line)
    body_path = ROOT / f"CONTROL-{operation}-EXPECTATION-BODY-v1.txt"
    write(body_path, "\n".join(body) + "\n")
    _, stdout, _ = run_child(f"hash-control-{operation}", [candidate, "--blake3-file", body_path])
    return body_path.read_text() + "manifest_blake3=" + stdout.read_text().strip() + "\n"


def prepare():
    fixture_root = ROOT / "work-v1/fixtures"
    fixture_root.mkdir(parents=True)
    run_child("fixtures", [CONTROL, "--fixed-radix-acceptance-fixtures", fixture_root])
    fixtures = {size: fixture_root / f"S1-{size // 1_048_576}.source" for size in (1_048_576, 10_485_760, 104_857_600)}
    for size, path in fixtures.items():
        if path.stat().st_size != size or (size in FIXTURE_SHA and sha(path) != FIXTURE_SHA[size]):
            raise RuntimeError(f"fixture custody mismatch: {size}")
    write(ROOT / "FIXTURE-MANIFEST-v1.tsv", "size_bytes\tpath\tsha256\n" + "".join(f"{size}\t{path.relative_to(REPO)}\t{sha(path)}\n" for size, path in fixtures.items()))

    masters = {}
    candidate = ROOT / "operands-v1/phase4_create_edit_benchmark-canonical-v2"
    for index, size in enumerate(fixtures):
        for arm, executable in (("A", CONTROL), ("B", candidate)):
            root = ROOT / f"work-v1/masters/full-{size}-{arm}"
            link_source(root, fixtures[size])
            iteration = 940000 + index
            run_child(f"prepare-full-{size}-{arm}", [executable, "--fast-prepare", root, str(size), "write", str(iteration)])
            masters[(arm, size, "full")] = root / f"db-K64-F64-{size}-full-{iteration}.sqlite"

    # CP-0009 prepares the one control 100-MiB published base and its exact same-edit oracle.
    control_base_root = ROOT / "work-v1/masters/published-100-A"
    link_source(control_base_root, fixtures[104_857_600])
    run_child("prepare-published-100-A", [CONTROL, "--fast-prepare", control_base_root, "104857600", "edit-same", "941000"])
    control_base = database(control_base_root, "same-middle", 941000)
    masters[("A", 104_857_600, "same-middle")] = control_base

    # Canonical-v2 derives every operation expectation from one copied published base.
    candidate_base_root = ROOT / "work-v1/masters/published-100-B"
    link_source(candidate_base_root, fixtures[104_857_600])
    run_child("prepare-published-100-B", [candidate, "--fast-prepare", candidate_base_root, "104857600", "materialize-warm", "941001"])
    candidate_base = database(candidate_base_root, "materialize-warm", 941001)
    full_v1_expectations = Path(str(masters[("A", 104_857_600, "full")]) + ".expectations")
    full_v2_expectations = Path(str(masters[("B", 104_857_600, "full")]) + ".expectations")

    for operation in ("materialize-warm", "materialize-fresh", "reopen", "read-range-1m"):
        masters[("A", 104_857_600, operation)] = (control_base, full_v1_expectations)
        masters[("B", 104_857_600, operation)] = (candidate_base, full_v2_expectations)
    masters[("B", 104_857_600, "scrub-only")] = (candidate_base, full_v2_expectations)

    plus_expectations = {}
    for operation in ("plus1-early", "plus1-middle"):
        path = ROOT / f"CONTROL-{operation}-EXPECTATIONS-v1.txt"
        write(path, make_v1_plus_expectation(operation, candidate))
        plus_expectations[operation] = path
        masters[("A", 104_857_600, operation)] = (control_base, path)

    candidate_prepare = ["same-middle", "plus1-early", "plus1-middle", "one-byte-early", "one-byte-middle", "one-byte-late", "first-edit-after-reopen"]
    for index, operation in enumerate(candidate_prepare):
        root = ROOT / f"work-v1/masters/{operation}-100-B"
        link_source(root, fixtures[104_857_600])
        iteration = 942000 + index
        cli, public = command_for(operation, prepare=True)
        env = os.environ.copy()
        env["LAYERFS_PREPARED_BASE_DATABASE"] = str(candidate_base)
        run_child(f"prepare-{operation}-100-B", [candidate, cli, root, "104857600", public, str(iteration)], env=env)
        masters[("B", 104_857_600, operation)] = database(root, operation, iteration)
    return fixtures, masters


def acquire_rows(fixtures, masters):
    candidate = ROOT / "operands-v1/phase4_create_edit_benchmark-canonical-v2"
    rows = schedule()
    fields = ["sequence", "label", "kind", "size", "operation", "arm", "pair", "order", "comparable"]
    with (ROOT / "SCHEDULE-v1.tsv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fields, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    write(ROOT / "RAW-v1.jsonl", "")
    write(ROOT / "ROW-STARTS-v1.tsv", "sequence\tevent\tmonotonic_ns\tlabel\tarm\toperation\n")
    write(ROOT / "INPUT-CUSTODY-v1.tsv", "sequence\tlabel\tarm\texecutable_sha256\tfixture_sha256\tdatabase_sha256\tauthority_sha256\texpectations_sha256\n")
    write(ROOT / "EXTERNAL-TIME-v1.tsv", "label\treal_seconds\tuser_seconds\tsystem_seconds\tmaximum_resident_set_bytes\tpeak_memory_footprint_bytes\tinstructions\tcycles\n")
    for spec in rows:
        if remaining() <= 1.0:
            raise TimeoutError("time exhausted before row")
        size, arm, operation = spec["size"], spec["arm"], spec["operation"]
        source = fixtures[size]
        root = ROOT / f"work-v1/rows/{spec['sequence']:02d}-{spec['label']}"
        link_source(root, source)
        iteration = 960000 + spec["sequence"]
        target_db = root / f"db-K64-F64-{size}-{operation}-{iteration}.sqlite"
        master = masters[(arm, size, operation)]
        if isinstance(master, tuple):
            copy_image(master[0], target_db, master[1])
        else:
            copy_image(master, target_db)
        executable = CONTROL if arm == "A" else candidate
        cli, public = command_for(operation)
        env = os.environ.copy()
        env.update({
            "LAYERFS_FAST_LANE": "1",
            "WP4M_EXECUTABLE_SHA256": sha(executable),
            "WP4M_BASE_COPY_METHOD": "physical-byte-copy-identical-database-authority-expectations",
            "WP4M_BASE_DATABASE_SHA256": sha(target_db),
            "WP4M_BASE_AUTHORITY_SHA256": sha(Path(str(target_db) + ".authority")),
            "WP4M_BASE_EXPECTATIONS_SHA256": sha(Path(str(target_db) + ".expectations")),
        })
        with (ROOT / "INPUT-CUSTODY-v1.tsv").open("a") as custody:
            custody.write(f"{spec['sequence']}\t{spec['label']}\t{arm}\t{sha(executable)}\t{sha(source)}\t{env['WP4M_BASE_DATABASE_SHA256']}\t{env['WP4M_BASE_AUTHORITY_SHA256']}\t{env['WP4M_BASE_EXPECTATIONS_SHA256']}\n")
        with (ROOT / "ROW-STARTS-v1.tsv").open("a") as starts:
            starts.write(f"{spec['sequence']}\tstarted\t{time.monotonic_ns()}\t{spec['label']}\t{arm}\t{operation}\n")
        validation = "capture-only"
        warmup = str(spec["kind"] == "warmup").lower()
        command = ["/usr/bin/time", "-l", executable, cli, root, str(size), public, str(iteration), warmup, validation]
        _, stdout, _ = run_child(f"row-{spec['sequence']:02d}-{spec['label']}", command, env=env, timed_row=True)
        row = json.loads(stdout.read_text())
        if row.get("status") != "PASS":
            raise RuntimeError(f"row failed: {spec['label']}")
        with (ROOT / "RAW-v1.jsonl").open("a") as raw:
            raw.write(json.dumps(row, separators=(",", ":")) + "\n")
        with (ROOT / "ROW-STARTS-v1.tsv").open("a") as starts:
            starts.write(f"{spec['sequence']}\tcompleted\t{time.monotonic_ns()}\t{spec['label']}\t{arm}\t{operation}\n")


def seal(status, reason):
    wall = time.monotonic() - started
    write(ROOT / "RUN-STATUS-v1.txt", f"status={status}\nreason={reason}\ntimeout={'true' if isinstance(reason, str) and 'TIME-BUDGET' in reason else 'false'}\nattempt=1\nwall_seconds_at_status={wall:.6f}\nwall_ceiling_seconds=119\n")
    manifest = ROOT / "TERMINAL-MANIFEST-v1.tsv"
    verification = ROOT / "TERMINAL-MANIFEST-VERIFICATION-v1.txt"
    if remaining() > 1.0:
        completed = subprocess.run(
            [sys.executable, MANIFEST_TOOL, "write", REPO, ROOT, manifest, verification],
            cwd=REPO,
            capture_output=True,
            text=True,
            timeout=max(0.2, remaining() - 0.2),
        )
        if completed.returncode:
            raise RuntimeError(f"terminal manifest command failed: {completed.stderr.strip()}")
        if "status=PASS" not in verification.read_text():
            raise RuntimeError("terminal manifest verification failed")
        for path in ROOT.rglob("*"):
            if path.is_file():
                path.chmod(0o444)
        for path in sorted((path for path in ROOT.rglob("*") if path.is_dir()), reverse=True):
            path.chmod(0o555)
        ROOT.chmod(0o555)


def execute():
    global deadline, started, lock_held
    if ROOT.exists():
        raise RuntimeError(f"result namespace already exists: {ROOT}")
    started = time.monotonic()
    deadline = started + 119.0
    signal.signal(signal.SIGALRM, alarm_handler)
    signal.signal(signal.SIGTERM, alarm_handler)
    signal.signal(signal.SIGINT, alarm_handler)
    signal.setitimer(signal.ITIMER_REAL, 119.0)
    ROOT.mkdir(parents=True)
    write(ROOT / "SCREEN-ATTEMPT-v1.txt", "attempt=1\nclassification=fresh compact closure\n")
    write(ROOT / "ACTUAL-INVOCATIONS-v1.tsv", "sequence\tevent\ttime_ns\tlabel\tcommand\texit\n")
    LOCK.mkdir()
    lock_held = True
    write(ROOT / "LOCK-v1.txt", f"lock={LOCK}\nacquired_ns={time.time_ns()}\nwall_ceiling_seconds=119\n")
    verify_methodology()
    if sha(CONTROL) != CONTROL_SHA or sha(CONTROL_SOURCE) != CONTROL_SOURCE_SHA:
        raise RuntimeError("CP-0009 custody mismatch")
    verify_manifest(HISTORY_MANIFEST)
    verify_manifest(HISTORY_CLARIFICATION)
    check_quiescence("PREVALIDATION")

    build_env = os.environ.copy()
    write(ROOT / "BUILD-COMMAND-v1.txt", "cargo check --locked -p layerfs-engine --bin phase4_create_edit_benchmark\ncargo test --locked -p layerfs-engine --bin phase4_create_edit_benchmark compact_v2_ -- --nocapture\ncargo build --release --locked -p layerfs-engine --bin phase4_create_edit_benchmark\n")
    run_child("cargo-check", ["cargo", "check", "--locked", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark"], env=build_env)
    run_child("focused-tests", ["cargo", "test", "--locked", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark", "compact_v2_", "--", "--nocapture"], env=build_env)
    run_child("release-build", ["cargo", "build", "--release", "--locked", "-p", "layerfs-engine", "--bin", "phase4_create_edit_benchmark"], env=build_env)

    operands = ROOT / "operands-v1"
    operands.mkdir()
    candidate = operands / "phase4_create_edit_benchmark-canonical-v2"
    shutil.copy2(CANDIDATE_BUILD, candidate)
    shutil.copy2(CONTROL, operands / CONTROL.name)
    candidate.chmod(0o555)
    (operands / CONTROL.name).chmod(0o555)
    write(ROOT / "CONTROL-SHA256-v1.txt", CONTROL_SHA + "\n")
    write(ROOT / "CANDIDATE-SHA256-v1.txt", sha(candidate) + "\n")
    source_paths = [REPO / "crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs", REPO / "crates/layerfs-core/src/canonical_v2.rs", REPO / "crates/layerfs-core/src/cas/mod.rs", REPO / "crates/layerfs-core/src/content/mod.rs", REPO / "crates/layerfs-core/src/cow/tree.rs", REPO / "crates/layerfs-core/tests/canonical_v2_fixture_oracle.rs", PREREG, ANALYZER, Path(__file__).resolve()]
    write(ROOT / "SOURCE-BUILD-CUSTODY-v1.tsv", "path\tsha256\tsize_bytes\n" + "".join(f"{path.relative_to(REPO)}\t{sha(path)}\t{path.stat().st_size}\n" for path in source_paths) + f"{candidate.relative_to(REPO)}\t{sha(candidate)}\t{candidate.stat().st_size}\n")
    write(ROOT / "ENVIRONMENT-v1.txt", f"rustc={subprocess.check_output(['rustc','--version'], text=True).strip()}\ncargo={subprocess.check_output(['cargo','--version'], text=True).strip()}\nmethodology_sha256={sha(METHODOLOGY)}\ncache_scope=warm developer environment; OS/filesystem cache warm-or-unknown\ninstructions=Unavailable\ncycles=Unavailable\nphysical_io=Unavailable\n")
    if sha(ORACLE) != "a89d13df092e9240d07cdb56a72b3a4d912db041c3d8e1f3f4cdebbf9dbb015c":
        raise RuntimeError("sealed independent oracle mismatch")
    check_quiescence("PREROW")
    fixtures, masters = prepare()
    acquire_rows(fixtures, masters)
    code, _, _ = run_child("analysis", [sys.executable, ANALYZER, ROOT], check=False)
    result = json.loads((ROOT / "ANALYSIS-v1.json").read_text())
    release_lock()
    if code or result.get("status") != "PASS":
        seal("REVISE", "ANALYSIS")
    else:
        seal("PASS", "none")
    signal.setitimer(signal.ITIMER_REAL, 0)
    return 0 if result.get("status") == "PASS" else 1


def main():
    global lock_held
    try:
        return execute()
    except TimeoutError as error:
        if ROOT.exists():
            write(ROOT / "DISPOSITION-v1.txt", "CANONICAL-V2 REVISE / TIME-BUDGET\nCP-0009 remains accepted.\n")
            write(ROOT / "ANALYSIS-v1.json", json.dumps({"status": "REVISE", "disposition": "CANONICAL-V2 REVISE", "reasons": ["TIME-BUDGET", str(error)]}, indent=2) + "\n")
            write(ROOT / "RUN-STATUS-v1.txt", f"status=REVISE\nreason=TIME-BUDGET\ntimeout=true\nattempt=1\nwall_seconds_at_status={time.monotonic()-started:.6f}\nwall_ceiling_seconds=119\n")
            try:
                release_lock()
            except Exception:
                pass
        return 124
    except Exception as error:
        if ROOT.exists():
            write(ROOT / "DISPOSITION-v1.txt", f"CANONICAL-V2 REVISE\nCP-0009 remains accepted.\nBlocker: {type(error).__name__}: {error}\n")
            write(ROOT / "ANALYSIS-v1.json", json.dumps({"status": "REVISE", "disposition": "CANONICAL-V2 REVISE", "reasons": [f"{type(error).__name__}: {error}"]}, indent=2) + "\n")
            write(ROOT / "RUN-STATUS-v1.txt", f"status=REVISE\nreason=ORCHESTRATION-OR-VALIDATION\ntimeout=false\nattempt=1\nwall_seconds_at_status={time.monotonic()-started:.6f}\nwall_ceiling_seconds=119\n")
            try:
                release_lock()
            except Exception:
                pass
            if remaining() > 1.5:
                try:
                    seal("REVISE", "ORCHESTRATION-OR-VALIDATION")
                except Exception:
                    pass
        print(f"REVISE: {type(error).__name__}: {error}", file=sys.stderr)
        return 1
    finally:
        stop_child()
        if lock_held:
            try:
                release_lock()
            except Exception:
                pass


if __name__ == "__main__":
    raise SystemExit(main())
