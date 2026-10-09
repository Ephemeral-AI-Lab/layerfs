"""One real release-daemon lifecycle proof over an already installed owned clone.

Run under the lead's checkout lock and prospective 60-second functional stop.
No Init, fixture copy, retry, performance qualification or container deletion.
On failure only the host controller is fenced; the lead owns container stop.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import signal
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "fs-bench-pro"))
from r7.runner import EventProcess, OriginalFailure

OVERALL_SECONDS = 60.0
VERIFIER_SECONDS = 9.5
CLEANUP_SECONDS = 5.0
IMAGE = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6"


def write_new(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def scripts(output, uid, gid):
    mutation = output / "mutation.sh"
    verifier = output / "independent-oracle.sh"
    # Mutation and oracle specify their own expected payloads; no mutation
    # output or saved mounted bytes supplies the oracle's expected contents.
    mutation.write_text("""set -euo pipefail
umask 027
python3 - <<'PY'
import os
from pathlib import Path
root = Path('r7-survival')
if os.path.lexists(root):
    raise RuntimeError('proof scope already exists; original input retained')
root.mkdir(mode=0o750)
for index in range(24):
    path = root / ('f%02d.bin' % index)
    with path.open('xb') as stream:
        stream.write(('r7-survival:%02d\\n' % index).encode() + bytes(range(256)) * 4)
    path.chmod(0o640)
os.link(root / 'f00.bin', root / 'hardlink.bin')
os.symlink('f00.bin', root / 'symlink')
print('R7 mutation complete: 24 files, one hardlink, one symlink')
PY
""")
    verifier.write_text("""set -euo pipefail
python3 - <<'PY'
import hashlib
import json
import os
from pathlib import Path
import stat
root = Path('r7-survival')
uid, gid = UID, GID
expected = {'f%02d.bin' % index for index in range(24)} | {'hardlink.bin', 'symlink'}
assert set(os.listdir(root)) == expected, 'exact scoped names differ'
info = root.lstat()
assert stat.S_ISDIR(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o750
assert (info.st_uid, info.st_gid) == (uid, gid), 'directory owner differs'
digest = hashlib.sha256()
identities = {}
for index in range(24):
    name = 'f%02d.bin' % index
    path = root / name
    info = path.lstat()
    assert stat.S_ISREG(info.st_mode) and stat.S_IMODE(info.st_mode) == 0o640
    assert (info.st_uid, info.st_gid) == (uid, gid), 'file owner differs'
    expected_bytes = ('r7-survival:%02d\\n' % index).encode() + bytes(range(256)) * 4
    with path.open('rb') as stream:
        actual = stream.read(65536)
        assert not stream.read(1), 'unexpected excess payload'
    assert actual == expected_bytes and info.st_size == len(expected_bytes), name
    assert info.st_nlink == (2 if index == 0 else 1), 'link count differs'
    identities[name] = (info.st_dev, info.st_ino)
    digest.update(name.encode() + b'\\0' + hashlib.sha256(actual).digest())
alias = (root / 'hardlink.bin').lstat()
assert stat.S_ISREG(alias.st_mode) and stat.S_IMODE(alias.st_mode) == 0o640
assert (alias.st_uid, alias.st_gid) == (uid, gid) and alias.st_nlink == 2
assert (alias.st_dev, alias.st_ino) == identities['f00.bin'], 'hardlink identity differs'
assert len(set(identities.values())) == 24, 'independent files alias'
link = (root / 'symlink').lstat()
assert stat.S_ISLNK(link.st_mode) and stat.S_IMODE(link.st_mode) == 0o777
assert (link.st_uid, link.st_gid) == (uid, gid) and link.st_nlink == 1
assert os.readlink(root / 'symlink') == 'f00.bin', 'symlink target differs'
with (root / 'hardlink.bin').open('rb') as stream:
    assert stream.read(65536) == b'r7-survival:00\\n' + bytes(range(256)) * 4
    assert not stream.read(1)
print(json.dumps({'schema': 'r7-lifecycle-scoped-oracle-v1', 'status': 'PASS',
                  'scope': 'r7-survival', 'regular_files': 24, 'names': 26,
                  'hardlink_aliases': 1, 'symlinks': 1, 'uid': uid, 'gid': gid,
                  'payload_set_sha256': digest.hexdigest()}, sort_keys=True))
PY
""".replace("UID, GID", str(uid) + ", " + str(gid)))
    return mutation, verifier


def control_bounds(row, count):
    fields = row.get("fields", {})
    before, after = (fields.get("control_send_records_" + name) for name in ("before", "after"))
    if type(before) is not int or type(after) is not int or after - before != count:
        raise OriginalFailure("original typed control-send bounds differ: " + row["event"])


def attempt(runtime, result, operation, key, path, seconds, overall_deadline, controls=None):
    start = time.monotonic_ns()
    with (runtime.folder / "controller-attempts.jsonl").open("a") as stream:
        stream.write(json.dumps({"operation": operation, "key": key,
                                 "script": str(path) if path is not None else None,
                                 "host_start_ns": start, "wall_stop_ns": int(seconds * 1e9),
                                 "container": runtime.container, "attempts": 1}) + "\n")
    row = runtime.send(operation, key, path, deadline=min(overall_deadline, time.monotonic() + seconds))
    finish = time.monotonic_ns()
    if controls is not None:
        control_bounds(row, controls)
    result["phases"].append({"operation": operation, "key": key,
                             "wall_stop_ns": int(seconds * 1e9),
                             "host_start_ns": start, "host_end_ns": finish,
                             "original_event": row})
    return row


def verify(runtime, result, key, script, label, deadline):
    row = attempt(runtime, result, "verify", key, script, VERIFIER_SECONDS, deadline)
    fields = row.get("fields", {})
    if fields.get("registered_execs") != "0" or fields.get("exit_code") != "0":
        raise OriginalFailure("verifier must have an original unregistered known-zero exit")
    with Path(fields["stdout"]).open("rb") as stream:
        data = stream.read(65537)
    if len(data) > 65536:
        raise OriginalFailure("scoped oracle exceeds its fixed 64 KiB output limit")
    oracle = json.loads(data)
    if oracle.get("schema") != "r7-lifecycle-scoped-oracle-v1" or oracle.get("status") != "PASS":
        raise OriginalFailure("independent scoped byte/metadata oracle failed")
    result[label] = oracle
    return oracle


def cleanup(runtime, result, key, overall_deadline):
    deadline = min(overall_deadline, time.monotonic() + CLEANUP_SECONDS)
    while True:
        if time.monotonic() >= deadline:
            raise TimeoutError("original closed namespace not observed Gone within five seconds")
        row = attempt(runtime, result, "cleanup", key, None,
                      max(0, deadline - time.monotonic()), deadline, controls=1)
        state = row.get("fields", {}).get("state")
        if state == "Gone":
            return
        if state not in {"Live", "Held", "Queued"}:
            raise OriginalFailure("unknown original typed cleanup observation")
        # Readiness observations are separate read-only commands, never a
        # repeated Unmount or an invocation that drives maintenance.
        time.sleep(min(0.01, max(0, deadline - time.monotonic())))


def fence_host(runtime, output, cause):
    custody = {"cause": str(cause), "container": runtime.container,
               "exec_ids": runtime.exec_ids, "host_pid": runtime.process.pid,
               "daemon_instance": runtime.daemon_instance, "scope": runtime.scope,
               "container_stop": "NOT_ATTEMPTED; lead owns explicit stop",
               "filesystem_drain": "NOT_ESTABLISHED", "operations_replayed": False,
               "host_signal": "SIGKILL only if still live"}
    write_new(output / "retained-custody.json", custody)
    if runtime.process.poll() is None:
        try:
            os.killpg(runtime.process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    try:
        custody["observed_host_exit"] = runtime.process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        custody["observed_host_exit"] = "UNAVAILABLE after bounded host fence"
    write_new(output / "host-fence.json", custody)
    return custody


def main():
    begun = time.monotonic_ns()
    # Reserve the final second for the bounded host-only failure fence. The
    # overall functional command, including setup, remains prospectively 60 s.
    deadline = time.monotonic() + OVERALL_SECONDS - 1
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-binary", type=Path, required=True)
    parser.add_argument("--daemon", type=Path, required=True)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--volume", required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--uid", type=int, default=501)
    parser.add_argument("--gid", type=int, default=20)
    args = parser.parse_args()
    if not 0 < args.uid <= 0xffffffff or not 0 <= args.gid <= 0xffffffff:
        parser.error("ordinary nonroot uint32 uid/gid required")
    for name in ("runtime_binary", "daemon", "manifest", "output"):
        value = getattr(args, name).absolute()
        if any(character in str(value) for character in "\t\r\n"):
            parser.error("protocol paths must contain no tabs or line breaks")
        setattr(args, name, value)
    args.output.mkdir(parents=True, exist_ok=False)
    with (args.output / "controller-attempts.jsonl").open("x"):
        pass
    mutation, verifier = scripts(args.output, args.uid, args.gid)
    first, fresh, scope = (secrets.token_hex(32) for _ in range(3))
    result = {"schema": "r7-real-daemon-lifecycle-proof-v1", "status": "INCOMPLETE",
              "admission_eligible": False, "timing_claim": "NOT_MEASURED",
              "global_profile": "Disposable/WAL/OFF", "overlay_profile": "MEMORY/OFF/EXCLUSIVE",
              "durable": "NOT_RUN — disabled by owner until explicit reauthorization",
              "overall_wall_stop_ns": 60_000_000_000,
              "operational_stop_ns": 59_000_000_000, "host_fence_stop_ns": 1_000_000_000,
              "setup": "already installed independent owned byte-copy volume; no Init",
              "setup_included": "serve/container creation, direct Store open and control readiness",
              "image": IMAGE, "volume": args.volume, "manifest": str(args.manifest),
              "uid": args.uid, "gid": args.gid, "workspace_keys": [first, fresh],
              "observation_scope": scope, "phases": [],
              "proof_scope": "r7-survival exact contents/names/ownership/modes/link equivalence",
              "full_fixture_oracle": "NOT_CLAIMED"}
    result["seals"] = {name: {"path": str(path), "sha256": sha(path)} for name, path in
                       (("runtime", args.runtime_binary), ("daemon", args.daemon),
                        ("manifest", args.manifest), ("mutation", mutation), ("oracle", verifier),
                        ("controller", Path(__file__).resolve()))}
    write_new(args.output / "prospective.json", result)
    argv = [str(args.runtime_binary), "serve", "--daemon", str(args.daemon),
            "--socket", args.socket, "--volume", args.volume, "--manifest", str(args.manifest),
            "--receipt", str(args.output / "runtime.events.jsonl"),
            "--uid", str(args.uid), "--gid", str(args.gid), "--observation-scope", scope]
    runtime = None
    try:
        runtime = EventProcess(argv, args.output, "runtime")
        ready = runtime.event("protocol_ready", min(deadline, time.monotonic() + 20))
        result["ready"] = ready
        origin = next((row for row in runtime.rows if row.get("event") == "control_origin"), None)
        if origin is None or origin.get("fields", {}).get("control_send_records") != 1 or not runtime.container:
            raise OriginalFailure("fresh real serve identity/one-Hello origin unavailable")
        attempt(runtime, result, "observe", "", None, 5, deadline)
        attempt(runtime, result, "mount", first, None, 10, deadline, controls=2)
        mutation_row = attempt(runtime, result, "command", first, mutation, 9.5, deadline)
        if mutation_row.get("fields", {}).get("registered_execs") != "0":
            raise OriginalFailure("mutation lacks original zero Exec-registration evidence")
        verify(runtime, result, first, verifier, "before_commit_oracle", deadline)
        commit = attempt(runtime, result, "commit", first, None, 15, deadline, controls=1)
        if commit.get("fields", {}).get("commit_kind") != "Committed":
            raise OriginalFailure("new changed frontier did not produce typed Committed")
        attempt(runtime, result, "unmount", first, None, 5, deadline, controls=1)
        cleanup(runtime, result, first, deadline)
        attempt(runtime, result, "mount", fresh, None, 10, deadline, controls=2)
        fresh_oracle = verify(runtime, result, fresh, verifier, "fresh_mount_oracle", deadline)
        if fresh_oracle != result["before_commit_oracle"]:
            raise OriginalFailure("independent pre-Commit and fresh-mount semantic oracle results differ")
        snapshot_start = len(runtime.rows)
        attempt(runtime, result, "snapshot", fresh, None, 5, deadline)
        statuses = [row for row in runtime.rows[snapshot_start:] if row.get("event") == "workspace_status"]
        if len(statuses) != 1 or statuses[0].get("fields", {}).get("key") != fresh:
            raise OriginalFailure("snapshot lacks its original fresh-Workspace Status event")
        control_bounds(statuses[0], 1)
        attempt(runtime, result, "unmount", fresh, None, 5, deadline, controls=1)
        cleanup(runtime, result, fresh, deadline)
        attempt(runtime, result, "observe", "", None, 5, deadline)
        attempt(runtime, result, "stop", "", None, 5, deadline)
        runtime.process.stdin.close()
        status = runtime.process.wait(timeout=max(0, deadline - time.monotonic()))
        if status:
            raise OriginalFailure("original runtime host exit is nonzero: " + str(status))
        result.update(status="PASS", observed_host_exit=status, container_stop="ACKNOWLEDGED")
    except Exception as cause:
        result.update(status="FAIL", original_failure_type=type(cause).__name__, original_failure=str(cause))
        if runtime is not None:
            result["retained_custody"] = fence_host(runtime, args.output, cause)
    result["complete_proof_ns"] = time.monotonic_ns() - begun
    if runtime is not None:
        result.update(container=runtime.container, exec_ids=runtime.exec_ids,
                      daemon_instance=runtime.daemon_instance, observed_scope=runtime.scope)
        runtime.raw.close()
        runtime.stderr.close()
        runtime.selector.close()
    write_new(args.output / "result.json", result)
    print(json.dumps(result, sort_keys=True))
    raise SystemExit(0 if result["status"] == "PASS" else 1)


if __name__ == "__main__":
    main()
