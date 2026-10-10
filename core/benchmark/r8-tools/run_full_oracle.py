"""One registered full mounted oracle; preparation and proof remain separate.

Reuse the authentic R7 runtime's SDK/Sandbox control and original event custody.
Only a new owned volume/container is used; historical masters are never opened.
This is a functional test, with natural caches and no latency qualification.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "core/benchmark/fs-bench-pro"))
from r7.runner import EventProcess, OriginalFailure

spec = importlib.util.spec_from_file_location(
    "r7_lifecycle", ROOT / "core/benchmark/r7-tools/lifecycle_proof.py")
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)
IMAGE = lifecycle.IMAGE
SEALED = Path("/tmp/layerfs-r7-full-sealed-20261009.sqlite")
MANIFEST = Path("/tmp/layerfs-r7-full-installed-20261009.manifest")
INVENTORY = Path("/tmp/layerfs-r7-full-inventory-20261009.jsonl")
EXPECTED = {
    str(SEALED): "0c836c88ce4eb04be8d823cbe23980696f96f9a56fe6e2da226cbb3b6bdecf05",
    str(MANIFEST): "1d3b8c9c679033dc98cf44eda6b7894fa7244fba2c963e3c7a7ef4fbac272684",
    str(INVENTORY): "d058d8c61220950b989772681cee4446575893076318d128a25884849271ba52",
}


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, sort_keys=True, indent=2)
        stream.write("\n")


def checked(args, timeout=10, **kwargs):
    value = subprocess.run(args, timeout=timeout, capture_output=True, **kwargs)
    if value.returncode:
        raise RuntimeError(f"original command failed {args}: {value.stderr!r}")
    return value


def export_oracle(runtime, output, result, deadline):
    result["oracle_export_attempted"] = True
    failures = []
    # Partial rows/scratch exist even when a timeout prevents the final JSON.
    # Each independent original copy is attempted once and recorded separately.
    for source, name in (("/tmp/r8-full-oracle.json.artifacts", "oracle.artifacts"),
                         ("/tmp/r8-full-oracle.json", "oracle.json")):
        left = deadline - time.monotonic()
        if left <= 0:
            failures.append("no oracle export after proof deadline: " + source)
            continue
        try:
            checked(["docker", "cp", runtime.container + ":" + source, str(output / name)],
                    timeout=min(5, left))
            result.setdefault("exported_oracle_members", []).append(name)
        except Exception as copying:
            failures.append(str(copying))
    if failures:
        result["oracle_export_failures"] = failures
        raise RuntimeError("; ".join(failures))
    result["oracle_artifacts"] = "oracle.json and oracle.artifacts retained before terminal teardown"


def validate_exported_rows(output, observed):
    for name, key in (("expected.jsonl", "expected_sha256"),
                      ("observed.jsonl", "observed_sha256")):
        if lifecycle.sha(output / "oracle.artifacts" / name) != observed[key]:
            raise OriginalFailure("exported forensic row hash differs: " + name)


def prepare(output):
    output.mkdir(parents=True, exist_ok=False)
    facts = {}
    for path in (SEALED, MANIFEST, INVENTORY):
        before = path.stat()
        digest = lifecycle.sha(path)
        after = path.stat()
        if before != after or digest != EXPECTED[str(path)]:
            raise RuntimeError("retained input identity differs: " + str(path))
        facts[str(path)] = dict(sha256=digest, bytes=before.st_size,
                               inode=before.st_ino, mtime_ns=before.st_mtime_ns)
    volume = "layerfs-r8-full-proof-20261010-" + secrets.token_hex(6)
    write(output / "owned-volume.json", dict(volume=volume, inputs=facts,
          cache="host hash/stream copy warm pages; functional only; no cold claim"))
    checked(["docker", "volume", "create", volume])
    # cat consumes a full byte stream: neither reflink nor mutable master mount.
    # The original sealed host file is read-only, the new volume is sole-owned.
    with SEALED.open("rb") as source:
        copied = checked(["docker", "run", "--rm", "--name", volume + "-copy",
                          "--network", "none", "-i", "-v", volume + ":/dest",
                          IMAGE, "sh", "-c",
                          "set -e; umask 077; mkdir -m 700 /dest/global; cat > /dest/global/store.sqlite; "
                          "sha256sum /dest/global/store.sqlite; "
                          "stat -c '%s %b %B %a %u %g' /dest/global/store.sqlite"],
                         timeout=45, stdin=source)
    (output / "copy.stdout").write_bytes(copied.stdout)
    (output / "copy.stderr").write_bytes(copied.stderr)
    if copied.stdout.decode().split()[0] != EXPECTED[str(SEALED)]:
        raise RuntimeError("independent volume copy digest differs")
    backing = copied.stdout.decode().splitlines()[1].split()
    if int(backing[0]) != facts[str(SEALED)]["bytes"] or backing[3:] != ["600", "0", "0"]:
        raise RuntimeError("copied Store violates protected-file ownership/mode contract")
    config = dict(schema="r8-full-proof-inputs-v1", volume=volume,
                  manifest=str(MANIFEST), manifest_sha256=EXPECTED[str(MANIFEST)],
                  inventory=str(INVENTORY), inventory_sha256=EXPECTED[str(INVENTORY)],
                  store_sha256=EXPECTED[str(SEALED)], store_bytes=facts[str(SEALED)]["bytes"],
                  backing_mode="0600", backing_uid=0, backing_gid=0,
                  image=IMAGE, uid=501, gid=20, setup="full independent streamed byte copy",
                  source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip())
    write(output / "inputs.json", config)
    print(json.dumps(config, sort_keys=True))


def proof(args):
    args.output.mkdir(parents=True, exist_ok=False)
    config = json.loads(args.inputs.read_text())
    registration = json.loads(args.registration.read_text())
    registered = registration["full_fixture_proof"]
    for label, path in (("runtime", args.runtime), ("daemon", args.daemon),
                        ("comparator", Path(__file__).with_name("full_oracle.py")),
                        ("controller", Path(__file__))):
        if lifecycle.sha(path) != registered["binaries_and_tools"][label]["sha256"]:
            raise ValueError("registered executable/tool identity differs: " + label)
    if lifecycle.sha(args.inputs) != registered["inputs_sha256"]:
        raise ValueError("registered prepared inputs differ")
    if (config["image"] != IMAGE or config["setup"] != "full independent streamed byte copy"
            or not config["volume"].startswith("layerfs-r8-full-proof-20261010-")):
        raise ValueError("wrong owned preparation")
    if lifecycle.sha(MANIFEST) != config["manifest_sha256"]:
        raise ValueError("manifest identity differs")
    result = dict(schema="r8-full-proof-controller-v1", status="INCOMPLETE",
                  source_commit=registration["identities"]["source_commit"],
                  registration_sha256=lifecycle.sha(args.registration),
                  global_profile="Disposable/WAL/OFF", construction_workers=1,
                  cache="natural, setup copy warmth declared; functional only", phases=[],
                  operation_attempts=1, performance_samples=0, full_byte_stop_seconds=100)
    runtime = None
    deadline = time.monotonic() + 98
    try:
        argv = [str(args.runtime), "serve", "--socket", args.socket,
                "--volume", config["volume"], "--daemon", str(args.daemon),
                "--manifest", config["manifest"], "--receipt", str(args.output / "runtime.events.jsonl"),
                "--uid", "501", "--gid", "20"]
        runtime = EventProcess(argv, args.output, "runtime")
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 6))
        if not runtime.container:
            raise OriginalFailure("no original acknowledged container")
        checked(["docker", "exec", runtime.container, "mkdir", "-m", "755", "/code"])
        for source, destination in ((Path(__file__).with_name("full_oracle.py"), "/code/full_oracle.py"),
                                    (INVENTORY, "/code/full-inventory.jsonl")):
            checked(["docker", "cp", str(source), runtime.container + ":" + destination])
        key = secrets.token_hex(32)
        lifecycle.attempt(runtime, result, "mount", key, None, 5, deadline, controls=2)
        body = args.output / "full-oracle.sh"
        body.write_text("set -euo pipefail\nexec timeout --kill-after=1s 85s python3 /code/full_oracle.py "
                        "--root . --inventory /code/full-inventory.jsonl --inventory-sha256 "
                        + config["inventory_sha256"] + " --uid 501 --gid 20 --walkers "
                        + str(int(registered["comparator_walkers"])) + " --output /tmp/r8-full-oracle.json\n")
        result["oracle_requested"] = True
        event = lifecycle.attempt(runtime, result, "verify", key, body, 87, deadline)
        fields = event.get("fields", {})
        if fields.get("registered_execs") != "0" or fields.get("exit_code") != "0":
            raise OriginalFailure("full verifier lacks original zero exit without registration")
        observed = json.loads(Path(fields["stdout"]).read_text())
        if observed.get("schema") != "r8-full-mounted-oracle-v2" or observed.get("status") != "PASS":
            raise OriginalFailure("full independent mounted oracle failed")
        if observed.get("walkers") != registered["comparator_walkers"]:
            raise OriginalFailure("actual comparator walker count differs from registration")
        if (observed.get("oracle_source_sha256") != registered["binaries_and_tools"]["comparator"]["sha256"]
                or observed.get("inventory_sha256_before") != config["inventory_sha256"]
                or observed.get("inventory_sha256_after") != config["inventory_sha256"]
                or (observed.get("owner_uid"), observed.get("owner_gid")) != (501, 20)):
            raise OriginalFailure("actual comparator/input/identity differs from registration")
        result["oracle"] = observed
        export_oracle(runtime, args.output, result, deadline)
        if json.loads((args.output / "oracle.json").read_text()) != observed:
            raise OriginalFailure("exported oracle differs from original stdout")
        validate_exported_rows(args.output, observed)
        lifecycle.attempt(runtime, result, "unmount", key, None, 5, deadline, controls=1)
        lifecycle.cleanup(runtime, result, key, deadline)
        lifecycle.attempt(runtime, result, "stop", "", None, 5, deadline)
        runtime.process.stdin.close()
        code = runtime.process.wait(timeout=max(0, deadline - time.monotonic()))
        if code:
            raise OriginalFailure("runtime exit differs: " + str(code))
        result.update(status="PASS", container_stop="ACKNOWLEDGED", host_exit=code)
    except Exception as original:
        result.update(status="FAIL", original_failure_type=type(original).__name__,
                      original_failure=str(original))
        if runtime is None:
            runtime = getattr(original, "event_process", None)
        if (runtime is not None and result.get("oracle_requested")
                and not result.get("oracle_export_attempted")):
            try:
                export_oracle(runtime, args.output, result, deadline)
            except Exception as exporting:
                result["independent_export_failure"] = str(exporting)
        if runtime is not None:
            result["custody"] = runtime.retain(original)
    finally:
        if runtime is not None:
            process = getattr(runtime, "process", None)
            for stream in (runtime.selector, runtime.raw, runtime.stderr,
                           getattr(process, "stdin", None), getattr(process, "stdout", None)):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception as closing:
                        result.setdefault("independent_close_failures", []).append(str(closing))
                        result["status"] = "FAIL"
        write(args.output / "proof.json", result)
    print(json.dumps(result, sort_keys=True))
    if result["status"] != "PASS":
        raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("prepare", "proof"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--inputs", type=Path)
    parser.add_argument("--registration", type=Path)
    parser.add_argument("--runtime", type=Path)
    parser.add_argument("--daemon", type=Path)
    parser.add_argument("--socket", default="/Users/yifanxu/.docker/run/docker.sock")
    args = parser.parse_args()
    if args.mode == "prepare":
        prepare(args.output)
    else:
        proof(args)


if __name__ == "__main__":
    main()
