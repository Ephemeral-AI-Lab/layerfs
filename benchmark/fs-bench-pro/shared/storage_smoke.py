#!/usr/bin/env python3
"""Approved storage v3 development smokes; current host SDK + managed real FUSE."""
from __future__ import annotations
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import stat
import subprocess
import threading
import time
import uuid
import runner
import runtime
import isolation
from deepseek_ten import PROFILES as SELECTED_DEEPSEEK

CONTRACT = "docs/roadmap/0.1/0.1.4/implementation-smoke-contract-v1.md"
MANIFEST_SHA = "03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271"
SOURCE_TIP = "b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed"
GIB = 1024**3
CASES = {"deepseek-stride10": ["deepseek-stride10"], "deepseek-stride3": ["deepseek-stride3"], "deepseek-ten": ["deepseek-ten"], "small-file-delta-10x30-v1": ["small-file-delta-10x30-v1"], "deepseek-full": ["deepseek-full"], "deepseek-five": ["deepseek-five"], "small-files": ["small-files"],
         "frequent-edits": ["sdk-text-32k", "sdk-binary-8m", "fuse-text-32k", "fuse-binary-8m"]}
LIMITS = {"deepseek-stride10": (14400, 300, 14400), "deepseek-stride3": (14400, 300, 14400), "deepseek-ten": (600, 120, 600), "small-file-delta-10x30-v1": (600, 30, 600), "deepseek-full": (14400, 300, 14400), "deepseek-five": (600, 120, 600), "frequent-edits": (300, 30, 300), "small-files": (120, 30, 120)}


def save(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def seal(root):
    return {str(p.relative_to(root)): runtime.file_sha256(p)
            for p in sorted(root.rglob("*")) if p.is_file() and not p.is_symlink()}


def validate_execution_identity(args, current, host, image):
    labels=image["Config"]["Labels"]
    if host["binary_sha256"]!=runtime.file_sha256(args.host_binary):
        raise ValueError("host binary seal mismatch")
    for key,label in (("LAYERFS_SOURCE_SEAL","source-seal"),
                      ("LAYERFS_PRODUCT_SEAL","product-seal"),
                      ("LAYERFS_COMPILATION_SEAL","compilation-seal")):
        if not host.get(key) or host[key]!=labels.get("dev.layerfs."+label):
            raise ValueError("host/image identity mismatch: "+key)
    if args.source_arm!='baseline' and any(host[key]!=current[key] for key in
            ("LAYERFS_SOURCE_SEAL","LAYERFS_PRODUCT_SEAL","LAYERFS_COMPILATION_SEAL")):
        raise ValueError("stale candidate source identity")
    if host.get('WORKLOAD_SOURCE_SHA256')!=current['WORKLOAD_SOURCE_SHA256']:
        raise ValueError("historical importer workload identity mismatch")
    probe=host.get('integrated_format_probe',{})
    if probe.get('status')!='PASS' or probe.get('storage_policy')!='ordinary' or probe.get('schema_version')!=10:
        raise ValueError("ordinary schema10 format probe required")
    # Product provenance belongs to the executable, even with a current Python
    # orchestrator driving an explicitly selected archived comparator.
    return {key:host[key] for key in current if key in host}


def history_harness_identity():
    names=('runner.py','runtime.py','storage_smoke.py','integrated_storage.py',
           'repository_history.py','deepseek_ten.py')
    return {name:runtime.file_sha256(Path(__file__).parent/name) for name in names}


def disk(root):
    apparent = allocated = 0
    for base, dirs, files in os.walk(root):
        for name in files:
            m = os.lstat(Path(base) / name)
            apparent += m.st_size
            allocated += m.st_blocks * 512
    return {"apparent_bytes": apparent, "allocated_bytes": allocated}


def git(repo, *args, deadline):
    return runtime.run(["git", "--git-dir=" + str(repo), *args], deadline=deadline, output_limit=8*1024**2).stdout


def entries(raw):
    # Pinned #72 tree grammar and original construction order.
    result = {}
    for record in raw.split(b"\0"):
        if not record:
            continue
        meta, path = record.split(b"\t", 1)
        mode, kind, oid, size = meta.split()
        if kind != b"blob" or mode not in (b"100644", b"100755", b"120000"):
            raise ValueError("unsupported Git entry")
        if any(p in (b"", b".", b"..") for p in path.split(b"/")) or path.hex() in result:
            raise ValueError("unsafe/duplicate Git path")
        result[path.hex()] = (mode.decode(), oid.decode(), int(size))
    return result


def encode(tree):
    return "".join(f"{m}\t{o}\t{s}\t{p}\n" for p, (m, o, s) in sorted(tree.items()))


def deepseek_inputs(data, deadline, count=5):
    raw_manifest = (data / "checkpoint-manifest.json").read_bytes()
    if hashlib.sha256(raw_manifest).hexdigest() != MANIFEST_SHA:
        raise ValueError("frozen DeepSeek manifest identity")
    manifest = json.loads(raw_manifest)
    if manifest["tip"] != SOURCE_TIP:
        raise ValueError("source tip")
    if count not in (5, 157) or len(manifest["checkpoints"]) != 157:
        raise ValueError("frozen DeepSeek checkpoint count")
    previous = {}
    blob_digests = {}
    result = []
    for row in manifest["checkpoints"][:count]:
        raw = git(data / "source.git", "ls-tree", "-rlz", "--full-tree", row["sha"], deadline=deadline)
        if hashlib.sha256(raw).hexdigest() != row["manifest_sha256"]:
            raise ValueError("frozen tree manifest")
        tree = entries(raw)
        source = data / "inputs" / row["sha"]
        if source.is_symlink() or not source.is_dir():
            raise ValueError("required immutable #72 input absent; prepare from pinned source")
        receipt = json.loads(source.with_suffix(".receipt.json").read_text())
        key = hashlib.sha256(encode(previous).encode() + b"\0" + raw).hexdigest()
        if receipt["key"] != key or seal(source) != receipt["files"]:
            raise ValueError("cached DeepSeek input identity")
        if (source / "manifest.tsv").read_text() != encode(tree) or (source / "previous.tsv").read_text() != encode(previous):
            raise ValueError("cached importer manifests differ from Git")
        expected = {}
        for path, (mode, oid, size) in tree.items():
            deadline.require("immutable Git input validation")
            if previous.get(path) != (mode, oid, size):
                body = (source / "blobs" / oid).read_bytes()
                if len(body) != size or hashlib.sha1(b"blob " + str(size).encode() + b"\0" + body).hexdigest() != oid:
                    raise ValueError("cached blob differs from Git identity")
                blob_digests[oid] = hashlib.sha256(body).hexdigest()
            expected[path] = [mode, size, blob_digests[oid]]
            parts = bytes.fromhex(path).split(b"/")
            for n in range(1, len(parts)):
                expected[b"/".join(parts[:n]).hex()] = ["40755", 0, "-"]
        oracle = data / "oracles" / (row["sha"] + ".json")
        if json.loads(oracle.read_text()) != expected:
            raise ValueError("cached oracle differs from independently authenticated Git blobs")
        result.append({**row, "full157_index": row["index"], "input": str(source), "oracle": str(oracle),
                       "input_seal": seal(source), "oracle_sha256": runtime.file_sha256(oracle)})
        previous = tree
    return {"deepseek-full" if count == 157 else "deepseek-five": {"input": "-", "states": result}}


def pseudorandom(prefix, length):
    return b"".join(hashlib.sha256(prefix + i.to_bytes(8, "little")).digest()
                    for i in range((length+31)//32))[:length]


def text_body(index, length):
    line = f"export const file_{index:03} = 123456789;\n".encode()
    return (line * ((length+len(line)-1)//len(line)))[:length]


def oracle_tree(root):
    expected = {}
    for path in sorted(root.rglob("*")):
        m = path.lstat()
        relative = os.fsencode(path.relative_to(root)).hex()
        if stat.S_ISLNK(m.st_mode):
            body = os.fsencode(os.readlink(path)); mode = "120000"
        elif stat.S_ISDIR(m.st_mode):
            expected[relative] = ["40755", 0, "-"]; continue
        elif stat.S_ISREG(m.st_mode):
            body = path.read_bytes(); mode = f"{stat.S_IFREG | stat.S_IMODE(m.st_mode):o}"
        else:
            raise ValueError("unsupported synthetic input")
        expected[relative] = [mode, len(body), hashlib.sha256(body).hexdigest()]
    return expected


def small_file_delta_inputs(parent):
    from small_file_delta_fixture import build
    return build(parent)


def synthetic_inputs(root, smoke, deadline):
    # Native fixture/oracle preparation only. No Store or measured output is cached.
    root.mkdir(parents=True, exist_ok=True)
    key = runner.digest({"recipe": "storage-smoke-v1", "smoke": smoke,
                         "generator": runtime.file_sha256(Path(__file__)),
                         "contract": runtime.file_sha256(runner.REPO / CONTRACT)})
    final = root / key
    if not final.exists():
        temp = root / ("partial-" + uuid.uuid4().hex)
        temp.mkdir()
        descriptions = {}
        for case in CASES[smoke]:
            deadline.require("synthetic input preparation")
            folder = temp / case; initial = folder / "initial"; initial.mkdir(parents=True)
            if smoke == "frequent-edits":
                length = 32768 if case.endswith("text-32k") else 8388608
                line = b"export const storage_value = 123456789;\n"
                a = (line * ((length+len(line)-1)//len(line)))[:length] if length == 32768 else pseudorandom(b"layerfs-storage-smoke-v1/binary/", length)
                (initial / "file").write_bytes(a)
                (initial / "file").chmod(0o644)
                body = bytearray(a); states = []
                for step in range(1, 6):
                    if step < 4:
                        offset = length * step // 4; body[offset:offset+4096] = bytes([65+step])*4096
                    else:
                        body = bytearray(a)
                    if step < 5:
                        (folder / f"state-{step}").write_bytes(body)
                    states.append({os.fsencode("file").hex(): ["100644", length, hashlib.sha256(body).hexdigest()]})
            else:
                for d in [*(f"d{i}" for i in range(8)), "empty-dir"]:
                    (initial / d).mkdir()
                for i in range(128):
                    body = (b"" if i < 16 else b"x"*(i-15) if i < 32 else
                            text_body(i, 8191 if i == 32 else 4096) if i < 96 else
                            text_body(33, 4096) if i < 112 else
                            pseudorandom(f"layerfs-storage-smoke-v1/small/{i:03}/".encode(), 4096))
                    p = initial / f"d{i%8}/f{i:03}"; p.write_bytes(body); p.chmod(0o755 if i in (40, 41) else 0o644)
                (initial / "source-link").symlink_to("d1/f033")
                (initial / "dir-link").symlink_to("d0")
                expected = oracle_tree(initial); states = []
                for step in range(1, 4):
                    expected = {k: list(v) for k,v in expected.items()}
                    if step in (1,3):
                        body = text_body(32,8191) + (b"gg" if step == 1 else b"")
                        expected[b"d0/f032".hex()] = ["100644", len(body), hashlib.sha256(body).hexdigest()]
                    else:
                        expected[b"d0/f040".hex()][0] = "100644"
                    states.append(expected)
            for p in [*initial.rglob("*"), initial]:
                if not p.is_symlink():
                    if p.is_dir(): p.chmod(0o755)
                    os.utime(p, (1000000000,1000000000))
            save(folder / "oracle-initial.json", oracle_tree(initial))
            for step, expected in enumerate(states, 1):
                save(folder / f"oracle-{step}.json", expected)
            descriptions[case] = {"steps": len(states)}
        # Include complete bytes, modes and symlinks in immutable validation.
        metadata = {str(p.relative_to(temp)): {"mode": stat.S_IMODE(p.lstat().st_mode),
                    "link": os.readlink(p) if p.is_symlink() else None,
                    "mtime_ns": p.lstat().st_mtime_ns if p.is_relative_to(temp / p.relative_to(temp).parts[0] / "initial") and not p.is_symlink() else None}
                    for p in temp.rglob("*")}
        save(temp / "manifest.json", {"files": seal(temp), "metadata": metadata, "cases": descriptions})
        temp.rename(final)
    manifest = json.loads((final / "manifest.json").read_text())
    files = seal(final); files.pop("manifest.json")
    if files != manifest["files"]:
        raise ValueError("synthetic immutable input bytes changed")
    for name, expected in manifest["metadata"].items():
        p = final / name; m = p.lstat()
        if stat.S_IMODE(m.st_mode) != expected["mode"] or (os.readlink(p) if p.is_symlink() else None) != expected["link"] or (expected["mtime_ns"] is not None and m.st_mtime_ns != expected["mtime_ns"]):
            raise ValueError("synthetic immutable input metadata changed")
    result = {}
    for case, desc in manifest["cases"].items():
        folder = final / case
        result[case] = {"input": str(folder), "initial_oracle": str(folder / "oracle-initial.json"),
                       "states": [{"index": i, "oracle": str(folder / f"oracle-{i}.json")} for i in range(1,desc["steps"]+1)],
                       "input_seal": seal(folder)}
    return result


def cgroup(sample):
    end = time.monotonic()+30
    result = runner.cgroup_snapshot(sample, end)
    text = runtime.run(["docker", "exec", sample.id, "cat", "/sys/fs/cgroup/memory.stat"], deadline=runtime.Deadline(end)).stdout.decode()
    names = {"anon", "file", "kernel", "shmem", "slab", "file_dirty", "file_writeback"}
    result["memory_stat_bytes"] = {k:int(v) for k,v in (line.split() for line in text.splitlines()) if k in names}
    return result


def receive(proc, messages, log, target, end):
    records = []
    while True:
        try: line = messages.get(timeout=max(0, end-time.monotonic()))
        except queue.Empty: raise TimeoutError(f"coordinator waiting for {target}")
        if line is None: raise RuntimeError(f"coordinator exited {proc.poll()} before {target}")
        log.write(line); log.flush()
        value = json.loads(line)
        records.append(value)
        if value.get("kind") == target: return records


def run_case(args, output, case, fixture, image, mode, remaining_phase_seconds, performance=None):
    sample = proc = None
    result = {"case": case, "mode": mode, "status": "INCOMPLETE", "records": [], "cleanup_status": "NOT_RUN"}
    host = output / "host-runtime"; host.mkdir(exist_ok=True)
    tmp = host / "tmp"; tmp.mkdir(exist_ok=True)
    started = time.monotonic_ns()
    phase_end = time.monotonic()+remaining_phase_seconds
    phase_started = None
    def send(line, target, timeout=None):
        proc.stdin.write(line+"\n"); proc.stdin.flush()
        return receive(proc, messages, log, target, min(phase_end, time.monotonic()+(timeout or LIMITS[args.storage_smoke][1])))
    with (output / (mode+".jsonl")).open("x") as log, (output / (mode+".stderr")).open("x") as stderr:
        try:
            setup = time.monotonic_ns()
            setup_end = time.monotonic()+120
            sample = runtime.start_sample(image, "layerfs-storage-smoke-"+uuid.uuid4().hex[:12],
                        {"family":"small_file_delta_smoke" if case == "small-file-delta-10x30-v1" else "storage-smoke-v1", "run":output.name}, deadline=runtime.Deadline(setup_end))
            result["environment"] = sample.observation
            runtime.ensure_container_dir(sample.name, "/input", runtime.Deadline.after(30))
            if mode == "performance" and case.startswith("fuse-"):
                runtime.install_tree(sample.name, Path(fixture["input"]), "/input/fixture", runtime.Deadline.after(120))
            env = {**os.environ, "TMPDIR":str(tmp), "LAYERFS_EXEC_TRANSPORT":"daemon", "LAYERFS_FUSE_TRANSPORT":"daemon"}
            session_mode = "compatibility" if args.storage_compat_run else mode
            # The existing DeepSeek session accepts explicit step commands; selection
            # and cardinality belong to this runner, not another compiled workload.
            session_case = "deepseek-full" if case in SELECTED_DEEPSEEK else case
            command = [args.host_binary, "storage-smoke-session", str(host), sample.id, session_mode, session_case, fixture["input"]]
            if mode == "verification" and getattr(args,"storage_compact",False):
                measured = json.loads((output/"compaction-result.json").read_text())["measured_store"]
                command.append(measured["path"])
            result["command"] = command
            proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, text=True, bufsize=1, env=env, start_new_session=True)
            messages = queue.Queue()
            def read_output():
                for line in proc.stdout: messages.put(line)
                messages.put(None)
            thread = threading.Thread(target=read_output, daemon=True); thread.start()
            ready = receive(proc, messages, log, "storage-smoke-ready", setup_end)
            result["ready"] = ready
            result["setup_ns"] = time.monotonic_ns()-setup
            phase_started = time.monotonic_ns()
            phase_end = time.monotonic()+remaining_phase_seconds
            before = cgroup(sample)
            result["cgroup_before"] = before
            if mode == "performance" and case == "small-files":
                result["read_passes"] = [send("read\t"+name, "storage-smoke-read") for name in ("first", "repeat")]
            selected = fixture["states"] if mode == "performance" else performance["records"]
            if mode == "verification" and case not in ("deepseek-five", "deepseek-full", "deepseek-ten", "deepseek-stride3", "deepseek-stride10"):
                selected = [{"index":0, "identity":"initial", "oracle":fixture["initial_oracle"]}, *selected]
            for row in selected:
                if time.monotonic() >= phase_end: raise TimeoutError("smoke complete phase budget")
                if shutil.disk_usage(output).free < 50*GIB: raise RuntimeError("smoke free disk reserve")
                save(output / f"{mode}-pending-{row['index']}.json", row)
                step_start = time.monotonic_ns()
                if mode == "performance":
                    transfer = 0
                    if case in ("deepseek-five", "deepseek-full", "deepseek-ten", "deepseek-stride3", "deepseek-stride10", "small-file-delta-10x30-v1"):
                        t = time.monotonic_ns()
                        runtime.run(["docker", "exec", sample.id, "rm", "-rf", "/input/checkpoint"], deadline=runtime.Deadline.after(30))
                        runtime.install_tree(sample.name, Path(row["input"]), "/input/checkpoint", runtime.Deadline.after(300 if case in ("deepseek-full", "deepseek-stride3", "deepseek-stride10") else 120))
                        transfer = time.monotonic_ns()-t
                    values = send(f"step\t{row['index']}", "storage-smoke-step")
                    record = {**row, **values[-1], "receipts":values, "transfer_ns":transfer}
                    if case not in ("deepseek-five", "deepseek-full", "deepseek-ten", "deepseek-stride3", "deepseek-stride10", "small-files", "small-file-delta-10x30-v1") and row["index"] == 5 and record["created"]:
                        raise RuntimeError("unchanged Commit created a new state")
                    if case in ("small-file-delta-10x30-v1", "deepseek-stride3", "deepseek-stride10") and not record["created"]:
                        raise RuntimeError("selected history requires Created")
                    record["identity"] = record["commit_id"] or "initial"
                else:
                    identity = row.get("identity") or row["commit_id"]
                    values = send("verify\t"+identity, "storage-smoke-verified-read", LIMITS[args.storage_smoke][1 if case in ("deepseek-full", "deepseek-ten", "deepseek-stride3", "deepseek-stride10", "small-file-delta-10x30-v1") else 2])
                    observed = output / f"observed-{row['index']}.tsv"
                    runtime.run(["docker", "cp", sample.id+":/input/observed.tsv", str(observed)], deadline=runtime.Deadline.after(300 if case in ("deepseek-full", "deepseek-stride3", "deepseek-stride10") else 120))
                    actual = {}
                    for line in observed.read_text().splitlines():
                        kind, size, digest, path = line.split("\t")
                        if path in actual: raise ValueError("duplicate observed path")
                        actual[path] = [kind,int(size),digest]
                    expected = json.loads(Path(row["oracle"]).read_text())
                    if actual != expected:
                        save(output / f"mismatch-{row['index']}.json", [{"path_hex":p,"expected":expected.get(p),"actual":actual.get(p)} for p in sorted(set(actual)|set(expected)) if actual.get(p)!=expected.get(p)])
                        raise RuntimeError("historical oracle mismatch")
                    record = {"index":row["index"],"identity":identity,"status":"PASS","verified_entries":len(actual),"verified_bytes":sum(v[1] for v in actual.values()),"receipts":values}
                record["step_wall_ns"] = time.monotonic_ns()-step_start
                record["cgroup"] = cgroup(sample)
                if record["cgroup"].get("oom_kill",0) != before.get("oom_kill",0) or record["cgroup"].get("oom",0) != before.get("oom",0) or record["cgroup"]["swap_current"]:
                    raise RuntimeError("container OOM/swap")
                record["host_runtime_disk"] = disk(host)
                record["spool_disk"] = disk(tmp)
                staging = runtime.run(["docker","exec",sample.id,"du","-sk","/input"],deadline=runtime.Deadline.after(30)).stdout.split()[0]
                record["container_staging_allocated_bytes"] = int(staging)*1024
                if record["spool_disk"]["allocated_bytes"]+record["container_staging_allocated_bytes"]>16*GIB or disk(output)["allocated_bytes"]>32*GIB:
                    raise RuntimeError("smoke disk budget")
                result["records"].append(record)
                save(output / f"{mode}-step-{row['index']}.json", record)
                print(f"{case} {mode} step={row['index']} PASS", flush=True)
            result["work_wall_ns"] = time.monotonic_ns()-phase_started
            if time.monotonic() >= phase_end: raise TimeoutError("smoke complete phase budget")
            # End/cleanup have their own budget, not an extension of measured work.
            phase_end = time.monotonic()+120
            closed = send("close", "storage-smoke-closed", 120)
            proc.wait(timeout=120); thread.join(timeout=5)
            if proc.returncode or not closed[-1]["cleanup_ok"] or not closed[-1]["success"]:
                raise RuntimeError("coordinator cleanup/exit")
            result.update(status="PASS", closed=closed)
        except BaseException as error:
            result.update(error=str(error), error_type=type(error).__name__)
        finally:
            cleanup = time.monotonic_ns()
            if proc and proc.poll() is None:
                proc.terminate()
                try: proc.wait(timeout=10)
                except subprocess.TimeoutExpired: proc.kill(); proc.wait(timeout=10)
                result["forced_coordinator_stop"] = True
            if proc:
                proc.stdin.close(); proc.stdout.close()
            if sample:
                try:
                    logs = runtime.run(["docker","logs",sample.id],deadline=runtime.Deadline.after(30),check=False)
                    (output/(mode+"-container.log")).write_bytes(logs.stdout+logs.stderr)
                    sample.remove(runtime.Deadline.after(120))
                    result["container_removed"] = True
                    result["cleanup_status"] = "PASS" if result.get("closed",[{}])[-1].get("cleanup_ok") else "DIAGNOSTIC"
                except Exception as error:
                    result.update(cleanup_status="FAIL",cleanup_error=str(error),status="INCOMPLETE")
            result["cleanup_ns"] = time.monotonic_ns()-cleanup
            result["wall_ns"] = time.monotonic_ns()-started
            result["retained_disk"] = disk(host)
            save(output / (mode+"-result.json"), result)
    return result


def page_size(path):
    with path.open("rb") as stream:
        header = stream.read(18)
    if header[:16] != b"SQLite format 3\0":
        raise ValueError("compatibility source is not SQLite")
    value = int.from_bytes(header[16:18], "big")
    return 65536 if value == 1 else value


def prepare_compatibility(args, current, host_identity, image, deadline):
    source, output = args.storage_compat_run.resolve(), args.output.resolve()
    if output == source or output.is_relative_to(source):
        raise ValueError("compatibility output must be outside retained source")
    saved = json.loads((source / "identity.json").read_text())
    manifest = json.loads((source / "verification-manifest.json").read_text())
    before = seal(source)
    if any(before.get(name) != digest for name, digest in manifest.items()):
        raise ValueError("retained verification manifest changed")
    if saved["smoke"] != args.storage_smoke or saved["contract_sha256"] != runtime.file_sha256(runner.REPO / CONTRACT):
        raise ValueError("compatibility workload/contract mismatch")
    if json.loads((source / "verification-summary.json").read_text())["status"] != "PASS":
        raise ValueError("compatibility requires completed original verification")
    fixtures = saved["fixtures"]
    if args.storage_smoke == "deepseek-five":
        if deepseek_inputs(args.data, deadline) != fixtures:
            raise ValueError("compatibility frozen DeepSeek fixture mismatch")
    else:
        for fixture in fixtures.values():
            if seal(Path(fixture["input"])) != fixture["input_seal"]:
                raise ValueError("compatibility original fixture/oracle bytes changed")
    if set(fixtures) != set(CASES[args.storage_smoke]):
        raise ValueError("compatibility case population mismatch")
    output.mkdir(parents=True, exist_ok=False)
    copies = {}
    for case in CASES[args.storage_smoke]:
        original, copied = source / case, output / case
        host = copied / "host-runtime"
        host.mkdir(parents=True)
        performance = json.loads((original / "performance-result.json").read_text())
        if performance["status"] != "PASS" or performance["cleanup_status"] != "PASS":
            raise ValueError("compatibility requires quiescent successful producer")
        old_store, new_store = original / "host-runtime/store.sqlite", host / "store.sqlite"
        if page_size(old_store) != 65536:
            raise ValueError("compatibility source must retain 64-KiB layout")
        copies[case] = runtime.closed_store_copy(old_store, new_store, deadline=deadline)
        copies[case]["page_size_before"] = page_size(new_store)
        for name in ("branch-id", "layer-id"):
            (host / name).write_bytes((original / "host-runtime" / name).read_bytes())
        save(copied / "performance-result.json", performance)
    save(output / "compatibility-identity.json", {
        "schema": "storage-smoke-compatibility-v1", "smoke": args.storage_smoke,
        "mode": "compatibility-verification", "admission_eligible": False,
        "allocation_comparison_eligible": False, "source_run": str(source),
        "producer": saved, "verifier": {"host_identity": host_identity,
            "image_id": image["Id"], "source": current},
        "source_seal_before": before, "copies": copies, "copy_seal_before": seal(output)})
    return fixtures, before


def main(argv=None):
    from integrated_storage import PROFILES as integrated_profiles
    p = argparse.ArgumentParser()
    p.add_argument("--storage-smoke", choices=tuple(CASES), required=True)
    p.add_argument("--storage-verify-run", type=Path)
    p.set_defaults(storage_compact=False)  # Historical verification may restore this from its saved identity.
    p.add_argument("--storage-compat-run", type=Path)
    p.add_argument("--source-arm", choices=("baseline","candidate"), default="candidate")
    p.add_argument("--repetition", type=int, choices=(1,2,3), default=1)
    p.add_argument("--output", type=Path)
    p.add_argument("--image", default=os.environ.get("LAYERFS_BENCH_IMAGE"))
    p.add_argument("--host-binary", default=str(runner.REPO/"target/release/fs-benchmark-pro"))
    p.add_argument("--data", type=Path, default=Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data"))
    p.add_argument("--fixtures", type=Path, default=Path("/Users/yifanxu/Ephemeral-AI-Lab/layerfs-storage-v3-data"))
    args = p.parse_args(argv)
    if not args.image or (args.output is None) == (args.storage_verify_run is None) or (args.storage_compat_run and (not args.output or args.storage_verify_run)):
        p.error("--image and exactly one of --output / --storage-verify-run required")
    with isolation.worktree_lock_path().open("a") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        start = time.monotonic_ns()
        current = runner.source_build_args()
        host_identity = json.loads(Path(args.host_binary+".identity.json").read_text())
        image = runner.image_info(args.image,time.monotonic()+30)
        producer=validate_execution_identity(args,current,host_identity,image)
        harness=history_harness_identity()
        if shutil.disk_usage(args.output.parent if args.output and args.output.parent.exists() else runner.REPO).free < 50*GIB:
            raise RuntimeError("free disk reserve")
        deadline = runtime.Deadline.after(14400 if args.storage_smoke in ("deepseek-full", "deepseek-stride3", "deepseek-stride10") else 600 if args.storage_smoke in ("deepseek-five", "deepseek-ten") else 120)
        if args.storage_compat_run:
            fixtures, source_seal = prepare_compatibility(args, current, host_identity, image, deadline)
        elif args.storage_smoke in SELECTED_DEEPSEEK:
            from deepseek_ten import inputs
            fixtures = inputs(args.data, args.fixtures, deadline, args.storage_smoke)
        else:
            fixtures = small_file_delta_inputs(args.fixtures) if args.storage_smoke == "small-file-delta-10x30-v1" else deepseek_inputs(args.data,deadline,157 if args.storage_smoke == "deepseek-full" else 5) if args.storage_smoke in ("deepseek-five", "deepseek-full", "deepseek-ten", "deepseek-stride3", "deepseek-stride10") else synthetic_inputs(args.fixtures,args.storage_smoke,deadline)
        preparation_ns = time.monotonic_ns()-start
        output = args.storage_verify_run or args.output
        if args.storage_compat_run:
            mode = "verification"
        elif args.storage_verify_run:
            saved = json.loads((output/"identity.json").read_text())
            if saved.get('source_arm','candidate')!=args.source_arm or saved.get('harness',harness)!=harness:
                raise ValueError('verification source arm/harness mismatch')
            args.storage_compact = saved.get("storage_compact",False)
            if args.storage_compact:
                profile=integrated_profiles[args.storage_smoke]
                if saved.get("integrated_scenario")!=profile["scenario"] or saved.get("integrated_contract_sha256")!=runtime.file_sha256(runner.REPO/profile["contract"]):
                    raise ValueError("integrated verification contract mismatch")
            if saved["host_identity"]["binary_sha256"] != host_identity["binary_sha256"] or saved["image_id"] != image["Id"] or saved["fixtures"] != fixtures:
                raise ValueError("verification custody mismatch")
            if args.storage_smoke in ("deepseek-full", "deepseek-ten", "deepseek-stride3", "deepseek-stride10", "small-file-delta-10x30-v1"):
                measured = json.loads((output / "performance-manifest.json").read_text())
                for case in CASES[args.storage_smoke]:
                    name = case + ("/compacted-store/store.sqlite" if args.storage_compact else "/host-runtime/store.sqlite")
                    if runtime.file_sha256(output / name) != measured.get(name):
                        raise ValueError("measured Store changed before historical reopen")
                    if args.storage_compact:
                        from integrated_storage import freeze
                        compaction = json.loads((output/case/"compaction-result.json").read_text())
                        frozen = compaction["measured_store"]
                        current_store = freeze(output/name)
                        if compaction["status"] != "PASS" or any(current_store[k] != frozen[k] for k in ("sha256","files","allocated_bytes","apparent_bytes")):
                            raise ValueError("frozen integrated Store identity/allocation changed")
                        archive = output/case/"frozen-measured-store"; archive.mkdir()
                        copy = runtime.closed_store_copy(output/name, archive/"store.sqlite", deadline=deadline)
                        save(output/case/"verification-store-before.json",{"measured_store":current_store,"frozen_copy":copy,"frozen_path":str((archive/"store.sqlite").resolve())})
            mode = "verification"
        else:
            output.mkdir(parents=True,exist_ok=False)
            save(output/"identity.json", {"schema":"deepseek-full-issue100-v1" if args.storage_smoke == "deepseek-full" else "storage-smoke-v1","smoke":args.storage_smoke,"family":"small_file_delta_smoke" if args.storage_smoke == "small-file-delta-10x30-v1" else "storage-smoke-v1","source_arm":args.source_arm,"repetition":args.repetition,
                "host_identity":host_identity,"image_id":image["Id"],"source":producer,
                "orchestrator_source":current,"harness":harness,"fixtures":fixtures,
                "storage_compact":args.storage_compact,
                "integrated_contract_sha256":runtime.file_sha256(runner.REPO/integrated_profiles[args.storage_smoke]["contract"]) if args.storage_compact else None,
                "integrated_scenario":integrated_profiles[args.storage_smoke]["scenario"] if args.storage_compact else None,
                "contract_sha256":runtime.file_sha256(runner.REPO/(SELECTED_DEEPSEEK[args.storage_smoke][2] if args.storage_smoke in SELECTED_DEEPSEEK else "docs/roadmap/0.1/0.1.5/delta-encoding-benchmarks.md" if args.storage_smoke == "small-file-delta-10x30-v1" else CONTRACT)),"preparation_ns":preparation_ns,
                "full_run_contract_sha256":runtime.file_sha256(runner.REPO/"docs/roadmap/0.1/0.1.5/full157-execution-contract.md") if args.storage_smoke == "deepseek-full" else None,
                "phase_operation_verification_limits_seconds":LIMITS[args.storage_smoke],
                "admission_eligible":False,"cache_profile":"fresh-store-existing-os-cache-uncontrolled",
                "resource_profile":"host phase CPU and lifetime RSS; container lifetime/boundary categories; sampled disk"})
            mode = "performance"
        results = []
        remaining_phase_seconds = LIMITS[args.storage_smoke][0 if mode == "performance" else 2]
        for case in CASES[args.storage_smoke]:
            folder = output/case
            if mode == "performance": folder.mkdir()
            performance = json.loads((folder/"performance-result.json").read_text()) if mode == "verification" else None
            if performance and performance["status"] != "PASS": raise ValueError("cannot qualify incomplete performance")
            result = run_case(args,folder,case,fixtures[case],image["Id"],mode,remaining_phase_seconds,performance)
            if args.storage_compact and mode == "verification" and result["status"] == "PASS" and result["cleanup_status"] == "PASS":
                from integrated_storage import freeze
                save(folder/"verification-store-after.json", freeze(folder/"compacted-store/store.sqlite"))
            results.append(result)
            remaining_phase_seconds -= result.get("work_wall_ns",0)/1e9
            if result["status"] != "PASS": break
        success = len(results) == len(fixtures) and all(r["status"] == "PASS" and r["cleanup_status"] == "PASS" for r in results)
        if args.storage_compat_run:
            source_unchanged = seal(args.storage_compat_run.resolve()) == source_seal
            page_sizes = {case: page_size(output / case / "host-runtime/store.sqlite")
                          for case in CASES[args.storage_smoke]}
            compatibility_ok = source_unchanged and all(size == 65536 for size in page_sizes.values())
            success = success and compatibility_ok
            save(output / "compatibility-result.json", {
                "status": "PASS" if success else "INCOMPLETE",
                "source_unchanged": source_unchanged, "page_sizes_after": page_sizes,
                "copy_seal_after": seal(output), "allocation_comparison_eligible": False})
        save(output/(mode+"-summary.json"),{"status":"PASS" if success else "INCOMPLETE","cases":[r["case"] for r in results],"wall_ns":time.monotonic_ns()-start,"preparation_ns":preparation_ns})
        save(output/(mode+"-manifest.json"),seal(output))
        print(json.dumps({"output":str(output),"mode":mode,"status":"PASS" if success else "INCOMPLETE"}),flush=True)
        return 0 if success else 1
