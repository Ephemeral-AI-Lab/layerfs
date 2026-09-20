#!/usr/bin/env python3
"""Record one explicit resource-sensitive command under shared measurement locks."""
import contextlib, fcntl, importlib.util, json, os, subprocess, sys, time
from pathlib import Path
p = Path(__file__).resolve().parent
repo = p.parents[5]
spec = importlib.util.spec_from_file_location("receipt", repo / "core/benchmark/fs-bench-pro-storage-content/shared/receipt.py")
receipt = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = receipt
spec.loader.exec_module(receipt)
name, *command = sys.argv[1:]
log = p / "checks" / (name + ".log")
result = p / "checks" / (name + ".json")
assert not log.exists() and not result.exists(), "append-only check receipt"
with contextlib.ExitStack() as stack:
    for lock in dict.fromkeys([(Path(os.environ.get("TMPDIR", "/tmp")) / "layerfs-infra-measurement.lock").resolve(), Path("/tmp/layerfs-infra-measurement.lock").resolve()]):
        handle = stack.enter_context(lock.open("a"))
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    stack.enter_context(receipt.measurement_lock(repo / "core/benchmark/fs-bench-pro-storage-content/.measurement.lock"))
    env = os.environ.copy()
    manifest = command[command.index("--manifest-path") + 1] if "--manifest-path" in command else ""
    target = "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/" + ("core/benchmark/fs-bench-pro-storage-content/target" if "benchmark" in manifest else "core/target")
    if command[0] == "cargo": env["CARGO_TARGET_DIR"] = target
    start = time.monotonic_ns()
    with log.open("x") as output:
        completed = subprocess.run(command, cwd=repo, env=env, stdout=output, stderr=subprocess.STDOUT)
    document = {"command":command,"cwd":str(repo),"exit_code":completed.returncode,"wall_ns":time.monotonic_ns()-start,"cargo_target_dir":env.get("CARGO_TARGET_DIR"),"dependency_reuse":"existing incremental pinned-toolchain Cargo target"}
    with result.open("x") as output: json.dump(document, output, indent=2)
    print(json.dumps(document))
    print(log.read_text()[-5000:])
    sys.exit(completed.returncode)
