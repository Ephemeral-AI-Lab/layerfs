"""One bounded Linux P lifecycle proof in an already owned container.

The lead supplies the native binary, oracle, empty backing and mountpoint, and
stops the owned container after failure. No retry, lazy detach, implicit mount
cleanup, third-party change or timing qualification is performed here.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import select
import signal
import subprocess
import time

READY_SECONDS = 5.0
ORACLE_SECONDS = 9.5
JOIN_SECONDS = 5.0
OVERALL_SECONDS = 58.0


def write_new(path, row):
    with Path(path).open("x") as stream:
        json.dump(row, stream, indent=2, sort_keys=True)
        stream.write("\n")


def mounted(target):
    """Read our mount namespace; do not stat a possibly blocked FUSE inode."""
    target = str(Path(target).absolute())
    with Path("/proc/self/mountinfo").open() as stream:
        for line in stream:
            fields = line.split()
            point = fields[4]
            for escaped, actual in (("\\040", " "), ("\\011", "\t"), ("\\012", "\n"), ("\\134", "\\")):
                point = point.replace(escaped, actual)
            if point == target:
                return True
    return False


def wait_original(process, deadline):
    """Pidfd readiness observes actual exit; no timeout-polling wall grid."""
    descriptor = os.pidfd_open(process.pid)
    try:
        remaining = max(0, deadline - time.monotonic())
        if not select.select([descriptor], [], [], remaining)[0]:
            raise TimeoutError("original process did not exit within declared wall stop")
        return process.wait()
    finally:
        os.close(descriptor)


class MountOwner:
    def __init__(self, binary, backing, target, output, label):
        self.label = label
        self.stdout_path = output / (label + ".stdout.jsonl")
        self.stderr_path = output / (label + ".stderr")
        self.stdout = self.stdout_path.open("xb")
        self.stderr = self.stderr_path.open("xb")
        self.argv = [str(binary), str(backing), str(target)]
        self.process = subprocess.Popen(self.argv, cwd="/", stdin=subprocess.DEVNULL,
                                        stdout=self.stdout, stderr=self.stderr, start_new_session=True)
        self.reader = self.stdout_path.open("rb")
        self.pending = b""
        self.events = []
        self.unmount_attempted = False
        self.joined = False
        write_new(output / (label + ".custody.json"), {"argv": self.argv, "pid": self.process.pid,
                  "target": str(target), "backing": str(backing), "unmount_attempts": 0,
                  "timeout_disposition": "retain original mount/process; lead stops only this owned container"})

    def collect(self):
        self.pending += self.reader.read(65536)
        while b"\n" in self.pending:
            line, self.pending = self.pending.split(b"\n", 1)
            self.events.append(json.loads(line))
        if len(self.pending) > 65536:
            raise ValueError("bounded original P diagnostic line exceeded; raw file retained")

    def ready(self, target, deadline):
        while True:
            self.collect()
            event = next((row for row in self.events if row.get("event") == "ready"), None)
            failures = [row for row in self.events if row.get("event") in {"startup_not_ready", "startup_failure", "handshake_failure"}]
            if failures:
                raise RuntimeError("original P startup failure: " + json.dumps(failures))
            if event:
                if event.get("phase") != "Serving" or [event.get(key) for key in ("configured", "created", "entered", "exited", "joined")] != [2,2,2,0,0]:
                    raise RuntimeError("original P readiness does not establish two serving loops")
                if not mounted(target):
                    raise RuntimeError("ready loops without the selected kernel mount")
                profile = next((row for row in self.events if row.get("event") == "negotiated"), None)
                if profile is None:
                    raise RuntimeError("original negotiated profile missing")
                for key, expected in {"max_write": 131072, "max_readahead": 131072,
                                      "max_background": 1, "congestion_threshold": 1,
                                      "configured_receive_loops": 2, "ttl_seconds": 60,
                                      "writeback": False, "default_permissions": True}.items():
                    if profile.get(key) != expected:
                        raise RuntimeError("original negotiated P profile mismatch: " + key)
                return event
            if self.process.poll() is not None:
                raise RuntimeError("original P process exited before actual readiness")
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError("actual two-loop P readiness exceeded five seconds")
            time.sleep(min(0.001, left))

    def terminal(self, target, output, deadline):
        if self.unmount_attempted:
            raise RuntimeError("original plain unmount already attempted; no replay")
        self.unmount_attempted = True
        with (output / (self.label + ".umount.stdout")).open("xb") as out, (output / (self.label + ".umount.stderr")).open("xb") as err:
            argv = ["umount", str(target)]
            process = subprocess.Popen(argv, cwd="/", stdin=subprocess.DEVNULL,
                                       stdout=out, stderr=err, start_new_session=True)
            write_new(output / (self.label + ".umount-attempt.json"),
                      {"argv": argv, "pid": process.pid, "attempts": 1, "lazy": False,
                       "timeout_disposition": "original detach outcome unknown; never a second attempt"})
            status = wait_original(process, deadline)
            if status:
                raise RuntimeError("original plain unmount exit " + str(status))
        status = wait_original(self.process, deadline)
        self.collect()
        if status:
            raise RuntimeError("original P session exit " + str(status))
        joined = next((row for row in self.events if row.get("event") == "joined"), None)
        if not joined or joined.get("clean") is not True:
            raise RuntimeError("original P loop-join outcome not clean")
        snapshot = joined["snapshot"]
        if [snapshot.get(key) for key in ("configured", "created", "entered", "exited", "joined")] != [2,2,2,2,2]:
            raise RuntimeError("original P receiver joins incomplete")
        if mounted(target):
            raise RuntimeError("selected mount remains after original plain unmount")
        drain = next((row for row in self.events if row.get("event") == "drain"), None)
        if drain is None or drain.get("held_handles") != 0:
            raise RuntimeError("original P terminal handle accounting incomplete")
        self.joined = True
        return {"joined": joined, "drain": drain,
                "opcode_completeness": "UNAVAILABLE: implemented callback domain; batch FORGET and default callbacks are not kernel request totals"}


def oracle(args, stage, output, overall_deadline, original=None):
    stdout_path = output / (stage + ".json")
    stderr_path = output / (stage + ".stderr")
    argv = ["python3", str(args.oracle), "--mount", str(args.mount), "--stage", stage]
    if original is not None:
        argv += ["--original", str(original)]
    def nonroot():
        os.setgroups([])
        os.setgid(args.gid)
        os.setuid(args.uid)
    with stdout_path.open("xb") as out, stderr_path.open("xb") as err:
        begun = time.monotonic_ns()
        process = subprocess.Popen(argv, cwd="/", stdin=subprocess.DEVNULL, stdout=out,
                                   stderr=err, start_new_session=True, preexec_fn=nonroot)
        write_new(output / (stage + ".attempt.json"), {"argv": argv, "pid": process.pid,
                  "uid": args.uid, "gid": args.gid, "wall_stop_ns": 9_500_000_000, "attempts": 1})
        try:
            status = wait_original(process, min(overall_deadline, time.monotonic() + ORACLE_SECONDS))
        except TimeoutError:
            # Cancel only the exact original verifier group. A cancellation is
            # a failed proof and establishes no mount drain or mutation rollback.
            cancellation = {"pid": process.pid, "signal": "SIGKILL", "attempts": 1,
                            "proof_status": "FAIL", "mount_cleanup": "NOT_ATTEMPTED"}
            try:
                os.killpg(process.pid, signal.SIGKILL)
                cancellation["signal_result"] = "sent"
            except ProcessLookupError:
                cancellation["signal_result"] = "already absent; original exit not inferred"
            try:
                cancellation["original_exit_after_cancel"] = wait_original(process, time.monotonic() + 1)
            except (TimeoutError, OSError) as error:
                cancellation["original_exit_after_cancel"] = "UNAVAILABLE: " + str(error)
            write_new(output / (stage + ".timeout-custody.json"), cancellation)
            raise
    stdout_path.chmod(0o644)
    row = json.loads(stdout_path.read_text())
    if status or row.get("status") != "PASS" or row.get("stage") != stage:
        raise RuntimeError("original independent " + stage + " oracle failed; stdout/stderr retained")
    return {"argv": argv, "status": status, "wall_ns": time.monotonic_ns() - begun,
            "wall_stop_ns": 9_500_000_000, "scope": "scoped independent functional proof", "original": row}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("/r7-passthrough"))
    parser.add_argument("--oracle", type=Path, default=Path("/r7-proof.py"))
    parser.add_argument("--backing", type=Path, default=Path("/native"))
    parser.add_argument("--mount", type=Path, default=Path("/p"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--uid", type=int, default=501)
    parser.add_argument("--gid", type=int, default=20)
    args = parser.parse_args()
    if platform.system() != "Linux" or os.geteuid() != 0 or args.uid == 0:
        parser.error("Linux root orchestrator and nonroot proof identity required")
    args.backing, args.mount = args.backing.resolve(), args.mount.resolve()
    if args.backing.is_relative_to(args.mount) or args.mount.is_relative_to(args.backing):
        parser.error("backing and mountpoint must be disjoint")
    if mounted(args.mount) or next(args.mount.iterdir(), None) or next(args.backing.iterdir(), None):
        parser.error("owned backing and unmounted mountpoint must be empty")
    info = args.backing.stat()
    if (info.st_uid, info.st_gid) != (args.uid, args.gid):
        parser.error("owned empty backing must preserve the declared nonroot owner")
    if args.output.resolve().is_relative_to(args.backing) or args.output.resolve().is_relative_to(args.mount):
        parser.error("evidence must be outside both filesystem roots")
    args.output.mkdir(parents=True, exist_ok=False)
    args.output.chmod(0o755)
    begun = time.monotonic_ns()
    deadline = time.monotonic() + OVERALL_SECONDS
    result = {"schema": "r7-passthrough-lifecycle-proof-v1", "status": "INCOMPLETE",
              "exploratory": True, "admission_eligible": False, "timing_claim": "NOT_MEASURED",
              "overall_wall_stop_ns": 58_000_000_000, "global_store": "NOT_APPLICABLE; no Store executed",
              "durable": "NOT_RUN — disabled by owner until explicit reauthorization",
              "binary": str(args.binary), "oracle": str(args.oracle), "uid": args.uid, "gid": args.gid,
              "phases": [], "preservation": "only supplied owned empty backing/mount/output; no container or volume deletion"}
    owners = []
    try:
        first = MountOwner(args.binary, args.backing, args.mount, args.output, "first")
        owners.append(first)
        result["phases"].append({"first_ready": first.ready(args.mount, min(deadline, time.monotonic() + READY_SECONDS))})
        result["phases"].append({"mutation": oracle(args, "mutation", args.output, deadline)})
        result["phases"].append({"first_terminal": first.terminal(args.mount, args.output, min(deadline, time.monotonic() + JOIN_SECONDS))})
        second = MountOwner(args.binary, args.backing, args.mount, args.output, "second")
        owners.append(second)
        result["phases"].append({"second_ready": second.ready(args.mount, min(deadline, time.monotonic() + READY_SECONDS))})
        result["phases"].append({"remount": oracle(args, "remount", args.output, deadline, args.output / "mutation.json")})
        result["phases"].append({"wide": oracle(args, "wide", args.output, deadline)})
        result["phases"].append({"second_terminal": second.terminal(args.mount, args.output, min(deadline, time.monotonic() + JOIN_SECONDS))})
        result["status"] = "PASS"
    except (OSError, ValueError, RuntimeError, TimeoutError) as error:
        result.update(status="FAIL", original_failure=str(error),
                      custody="original mount/receiver state retained; lead explicitly stops only this owned container; no automatic detach/replay")
    result["owners"] = [{"pid": owner.process.pid, "unmount_attempts": int(owner.unmount_attempted),
                         "joined": owner.joined, "observed_host_exit": owner.process.poll()} for owner in owners]
    result["mount_present"] = mounted(args.mount)
    result["complete_proof_ns"] = time.monotonic_ns() - begun
    write_new(args.output / "result.json", result)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["status"] == "PASS" else 1)


if __name__ == "__main__":
    main()
