"""Actual ordinary-runtime stream delivery through the real Engine.

One daemon and one mounted Workspace over an already prepared owned volume,
driven through the R7 runtime's SDK/Sandbox control. Each case is one ordinary
command whose stdout and stderr the runtime copies to files; the controller
derives the expected bytes from the stream definition alone and compares
length, SHA-256 and the runtime's own delivered counts. Functional only, no
timing claim, one attempt per operation.

Judged cases, in a mounted Workspace: both streams at sizes around the
8,192-byte copy window and around 64 KiB and 1 MiB; 64 MiB on each stream,
interleaved. Then the Workspace is unmounted and observed Gone, and two cases
run outside any mount: a descendant keeps stdout open and writes one line
after the command has exited 0 (the command's own bytes and status are
judged; whether the late line arrives is recorded and not judged, because the
written contract is disputed on it); and a command writes 1 MiB to each
stream and exits 7 (complete bytes and the actual status are judged). The
last case ends the runtime's protocol loop by design, so the container is
then stopped explicitly here and that is recorded.

Not constructible through this protocol, and reported as such: a stalled or
failing sink, a delayed consumer, and a connection cut between frames.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run_full_oracle as full
from run_full_oracle import EventProcess, OriginalFailure, checked, lifecycle, write

OVERALL_SECONDS = 60
BLOCK = 4096
MEBIBYTE = 1 << 20
SIZES = ((0, 0), (1, 0), (0, 1), (8191, 8193), (8192, 8192), (8193, 8191),
         (65535, 65537), (MEBIBYTE, MEBIBYTE + 1))
LARGE = (64 * MEBIBYTE, 64 * MEBIBYTE)
HELD_SECONDS = 3
RETAINED_BYTES = 65537
LATE = b"late line from the descendant\n"
EXIT_CODE = 7
FILLS = dict(stdout=0x6F, stderr=0x65)
DELIVERED = re.compile(r"delivered=OutputProgress \{ stdout: (\d+), stderr: (\d+), "
                       r"wire_payload: (\d+), saturated: false \}")
EXITED = "exit_code: Some(%d)" % EXIT_CODE
NOT_CONSTRUCTIBLE = ["stalled or failing sink (backpressure)", "fast exit with a delayed consumer",
                     "connection cut between frames"]


def expected(name, total, suffix=b""):
    """Length and digest from the definition; no produced byte is consulted."""
    digest = hashlib.sha256()
    for index in range((total + BLOCK - 1) // BLOCK):
        block = index.to_bytes(8, "big") + bytes([FILLS[name]]) * (BLOCK - 8)
        digest.update(block[:min(total - index * BLOCK, BLOCK)])
    digest.update(suffix)
    return total + len(suffix), digest.hexdigest()


def observed(path):
    digest = hashlib.sha256()
    total = 0
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(MEBIBYTE), b""):
            digest.update(block)
            total += len(block)
    return total, digest.hexdigest()


def compare(case, paths, totals, suffix=b""):
    """Differences between one case's two output files and the definition."""
    failures = []
    for name, total, extra in (("stdout", totals[0], suffix), ("stderr", totals[1], b"")):
        want, got = expected(name, total, extra), observed(paths[name])
        case[name] = dict(expected_bytes=want[0], observed_bytes=got[0], expected_sha256=want[1],
                          observed_sha256=got[1])
        if want != got:
            failures.append("%s %s differs: %d bytes expected, %d observed"
                            % (case["label"], name, want[0], got[0]))
        elif got[0] > RETAINED_BYTES:
            # A verified large stream is regenerable from its definition; its
            # length and digest stay in the result. A differing one is kept.
            Path(paths[name]).unlink()
            case[name]["bytes_retained"] = False
    return failures


def delivered(rows, start):
    """The runtime's own counts for the one command since `start`, or None."""
    found = [row for row in rows[start:] if row.get("event") == "streams"]
    match = DELIVERED.search(found[0].get("value", "")) if len(found) == 1 else None
    if match is None or int(match.group(3)) != int(match.group(1)) + int(match.group(2)):
        return None
    return [int(match.group(1)), int(match.group(2))]


def body(output, label, arguments):
    path = output / (label + ".sh")
    path.write_text("set -euo pipefail\nexec python3 -B /code/stream_source.py " + arguments + "\n")
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--daemon", type=Path, required=True)
    parser.add_argument("--socket", default="/Users/yifanxu/.docker/run/docker.sock")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    config = json.loads(args.inputs.read_text())
    source = Path(__file__).with_name("stream_source.py")
    result = dict(schema="r8b-runtime-streams-controller-v1", status="INCOMPLETE",
                  timing_claim="NOT_MEASURED", global_profile="Disposable/WAL/OFF",
                  overlay_profile="MEMORY/OFF/EXCLUSIVE", construction_workers=1,
                  overall_wall_stop_seconds=OVERALL_SECONDS, operation_attempts=1,
                  not_constructible=NOT_CONSTRUCTIBLE, volume=config["volume"], image=config["image"],
                  seals={name: lifecycle.sha(path) for name, path in
                         (("runtime", args.runtime), ("daemon", args.daemon), ("source", source),
                          ("controller", Path(__file__)), ("inputs", args.inputs))},
                  cases=[], phases=[])
    failures = []
    runtime = container = None
    deadline = time.monotonic() + OVERALL_SECONDS - 2
    try:
        argv = [str(args.runtime), "serve", "--socket", args.socket, "--volume", config["volume"],
                "--daemon", str(args.daemon), "--manifest", config["manifest"],
                "--receipt", str(args.output / "runtime.events.jsonl"),
                "--uid", str(config["uid"]), "--gid", str(config["gid"])]
        runtime = EventProcess(argv, args.output, "runtime")
        runtime.event("protocol_ready", min(deadline, time.monotonic() + 6))
        container = runtime.container
        if not container:
            raise OriginalFailure("no original acknowledged container")
        checked(["docker", "exec", container, "mkdir", "-m", "755", "/code"])
        checked(["docker", "cp", str(source), container + ":/code/stream_source.py"])
        key = secrets.token_hex(32)
        lifecycle.attempt(runtime, result, "mount", key, None, 5, deadline, controls=2)
        for totals in (*SIZES, LARGE):
            case = dict(label="sizes-%d-%d" % totals, stdout_bytes=totals[0], stderr_bytes=totals[1])
            start = len(runtime.rows)
            event = lifecycle.attempt(runtime, result, "command", key,
                                      body(args.output, case["label"], "%d %d 0" % totals), 20, deadline)
            failures += compare(case, event["fields"], totals)
            case["runtime_delivered"] = delivered(runtime.rows, start)
            if case["runtime_delivered"] != list(totals):
                failures.append(case["label"] + " runtime delivered counts differ from the definition")
            result["cases"].append(case)
        lifecycle.attempt(runtime, result, "unmount", key, None, 5, deadline, controls=1)
        lifecycle.cleanup(runtime, result, key, deadline)
        result["workspace"] = "Unmounted, Gone observed before the cases outside a mount"
        held = dict(label="descendant-held", stdout_bytes=6, stderr_bytes=0, held_seconds=HELD_SECONDS,
                    late_line="RECORDED, not judged: the stream contract for bytes written by a "
                              "descendant after the command exited is disputed")
        began = time.monotonic()
        start = len(runtime.rows)
        event = lifecycle.attempt(runtime, result, "command", "native:/tmp",
                                  body(args.output, held["label"], "6 0 0 held %d" % HELD_SECONDS),
                                  HELD_SECONDS + 6, deadline)
        held["host_seconds_not_a_measurement"] = round(time.monotonic() - began, 3)
        held["exit_code"] = event["fields"].get("exit_code")
        held["runtime_delivered"] = delivered(runtime.rows, start)
        data = Path(event["fields"]["stdout"]).read_bytes()
        early = expected("stdout", 6)
        held["stdout"] = dict(observed_bytes=len(data), observed_sha256=hashlib.sha256(data).hexdigest(),
                              command_bytes=early[0], command_sha256=early[1])
        held["late_line_delivered"] = data[6:] == LATE
        if (len(data[:6]), hashlib.sha256(data[:6]).hexdigest()) != early or data[6:] not in (b"", LATE):
            failures.append("descendant-held: the command's own bytes are not an exact prefix")
        if held["runtime_delivered"] != [len(data), 0]:
            failures.append("descendant-held: runtime delivered counts differ from the bytes on file")
        result["cases"].append(held)
        exiting = dict(label="exit-%d-with-output" % EXIT_CODE, stdout_bytes=MEBIBYTE, stderr_bytes=MEBIBYTE)
        start = len(runtime.rows)
        try:
            lifecycle.attempt(runtime, result, "command", "native:/tmp",
                              body(args.output, exiting["label"],
                                   "%d %d %d" % (MEBIBYTE, MEBIBYTE, EXIT_CODE)), 10, deadline)
            failures.append("a nonzero exit was reported as a completed command")
        except OriginalFailure as reported:
            exiting["original_failure"] = str(reported)
        rows = [row for row in runtime.rows[start:] if row.get("event") == "command"]
        fields = rows[0].get("fields", {}) if len(rows) == 1 else {}
        exiting["exit_code"] = fields.get("exit_code")
        exiting["typed_status_in_event"] = EXITED in rows[0].get("value", "") if rows else False
        if exiting["exit_code"] != str(EXIT_CODE) or not exiting["typed_status_in_event"]:
            failures.append("the runtime did not report the actual exit status")
        if not {"stdout", "stderr"} <= set(fields):
            failures.append("the nonzero-exit case left no pair of output files")
        else:
            failures += compare(exiting, fields, (MEBIBYTE, MEBIBYTE))
            exiting["runtime_delivered"] = delivered(runtime.rows, start)
            if exiting["runtime_delivered"] != [MEBIBYTE, MEBIBYTE]:
                failures.append("nonzero-exit delivered counts differ from the definition")
        result["cases"].append(exiting)
    except Exception as original:
        result.update(original_failure_type=type(original).__name__, original_failure=str(original))
        failures.append("controller failure: " + str(original))
    finally:
        if runtime is not None:
            result["custody"] = runtime.retain(RuntimeError("protocol loop ended by the nonzero-exit case"
                                                            if "original_failure" not in result
                                                            else result["original_failure"]))
            if container:
                stopped = subprocess.run(["docker", "stop", "-t", "1", container],
                                         capture_output=True, timeout=15)
                result["container_stop"] = ("EXPLICIT docker stop by this controller, exit %d"
                                            % stopped.returncode)
            process = getattr(runtime, "process", None)
            for stream in (runtime.selector, runtime.raw, runtime.stderr,
                           getattr(process, "stdin", None), getattr(process, "stdout", None)):
                if stream is not None:
                    try:
                        stream.close()
                    except Exception as closing:
                        result.setdefault("independent_close_failures", []).append(str(closing))
        result.update(judged_failures=failures, status="FAIL" if failures else "PASS")
        write(args.output / "result.json", result)
    print(json.dumps({key: value for key, value in result.items() if key != "phases"}, sort_keys=True))
    if result["status"] != "PASS":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
