"""One #516 cell through the `layerfs` CLI binary or the Python package (#535
phase 8 item 7, lane K2). Same steps, bodies and verdict rule as the frozen
Node runner (`node/test/verify/cells/cell.mjs`), with a sqlite Store pushed to
the machine and pulled back:

    python3 cell.py <cli|python> <workloads.json> <C01..C12> <machine> <receipt.json>

The home is LAYERFS_HOME (a shared home already holding <machine>); the Store
is `<via><nn>` (cli01, py01) so it never shares a name with the Node cells.
The CLI is LFS533_LAYERFS; Python is the `layerfs` package on PYTHONPATH.
workloads.json comes from export.mjs (the Node runner's own scripts). One
attempt per call, no retry.
"""

import json
import os
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

via, workloads, cell, machine, receipt = sys.argv[1:6]
W = json.load(open(workloads))
C = W["cells"][cell]
store = ("cli" if via == "cli" else "py") + cell[1:]
project = store + "/p"
name = store
out = {
    "format": "lfs535-k2-parity/1",
    "via": via,
    "cell": cell,
    "machine": machine,
    "product": os.environ.get("LFS533_PRODUCT"),
    "startedAt": datetime.now(timezone.utc).isoformat(),
    "load1": os.getloadavg()[0],
    "body": {"sha256": C["body_sha256"]},
    "store": {"kind": "sqlite"},
    "ok": False,
    "cause": None,
    "phases": {},
    "steps": [],
}


class Refused(Exception):
    pass


if via == "cli":
    CLI = os.environ["LFS533_LAYERFS"]

    def call(family, op, *args, **opts):
        argv = [CLI, family, op] + [str(a) for a in args]
        for key, value in opts.items():
            flag = "--" + key.replace("_", "-")
            if value is True:
                argv.append(flag)
            elif value not in (None, False):
                argv += [flag, str(value)]
        done = subprocess.run(argv, capture_output=True, text=True, timeout=110)
        if done.returncode != 0:
            raise Refused(f"exit {done.returncode}: {(done.stdout + done.stderr).strip()[:2000]}")
        text = done.stdout.strip()
        return json.loads(text) if text else None

else:
    import layerfs

    lfs = layerfs.client(home=os.environ["LAYERFS_HOME"])

    def call(family, op, *args, **opts):
        try:
            return getattr(getattr(lfs, family), op)(*args, **opts)
        except layerfs.LayerFSError as error:
            raise Refused(str(error)[:2000]) from None


def step(label, family, op, *args, **opts):
    at = time.monotonic()
    try:
        value = call(family, op, *args, **opts)
    except Exception as error:  # every failure is the step's
        ms = round((time.monotonic() - at) * 1000, 3)
        out["steps"].append({"name": label, "ms": ms, "ok": False, "error": str(error)[:2000]})
        raise RuntimeError(f"{label}: {error}") from None
    ms = round((time.monotonic() - at) * 1000, 3)
    out["steps"].append({"name": label, "ms": ms, "ok": True})
    return value, ms


def timed_of(stdout):
    for line in (stdout or "").splitlines():
        parts = line.split(" ")
        if len(parts) == 4 and parts[0] == "LFS527":
            rc = int(parts[1]) if parts[1].isdigit() else None
            ns = parts[2].isdigit() and parts[3].isdigit() and len(parts[2]) >= 16
            return rc, ((int(parts[3]) - int(parts[2])) / 1e6 if ns else None)
    return None, None


def run(ws, text, label):
    value, ms = step(label, "workspace", "exec", ws, text, message=f"lfs535 {cell} {label}")
    rc, body_ms = timed_of(value.get("stdout"))
    return {
        "wallMs": ms,
        "exitCode": value.get("exit_code"),
        "stdout": (value.get("stdout") or "")[:2000],
        "stderr": (value.get("stderr") or "")[:2000],
        "rc": rc,
        "ms": body_ms,
    }


mounted = []
sandbox = None
pushed = False
empty = tempfile.mkdtemp(prefix="lfs535k2-src-")


def mount(label):
    value, ms = step(label + ".mount", "workspace", "mount", sandbox, "lane")
    mounted.append(value["ws"])
    return value, ms


def unmount(label, ws, discard):
    _, ms = step(label + ".unmount", "workspace", "unmount", ws, discard=discard)
    mounted.remove(ws)
    return ms


try:
    step("store.create", "store", "create", store)
    step("project.init", "project", "init", project, empty)
    if via == "cli":
        # A CLI process leaves its Store's write-ahead log behind (it exits
        # with the Store open), and `store push` then refuses the Store
        # (cli-wal-after-fork*.log); so through the CLI the Branch is forked
        # after the push, on the machine's worker. Python keeps Node's order.
        step("store.push", "store", "push", store, machine)
        pushed = True
        step("branch.fork", "branch", "fork", project, "lane", "main")
    else:
        step("branch.fork", "branch", "fork", project, "lane", "main")
        step("store.push", "store", "push", store, machine)
        pushed = True
    out["sandbox"], _ = step("sandbox.create", "sandbox", "create", project, name, machine)
    sandbox = name
    if C["setup"]:
        ws = mount("setup")[0]["ws"]
        ran = run(ws, C["setup"], "setup.exec")
        if ran["exitCode"] != 0:
            raise RuntimeError(f"setup.exec: setup body exited {ran['exitCode']}: {ran['stderr']}")
        committed, _ = step("setup.commit", "workspace", "commit", ws, message=f"lfs535 {cell} setup")
        unmount("setup", ws, False)
        out["phases"]["setup"] = {"exec": ran, "commit": committed}
    value, mount_ms = mount("warmup")
    ran = run(value["ws"], C["timed"], "warmup.exec")
    out["phases"]["warmup"] = {"mountMs": mount_ms, "exec": ran, "unmountMs": unmount("warmup", value["ws"], True)}
    value, mount_ms = mount("timed")
    ws = value["ws"]
    ran = run(ws, C["timed"], "timed.exec")
    committed, commit_ms = step("timed.commit", "workspace", "commit", ws, message=f"lfs535 {cell}")
    out["phases"]["timed"] = {
        "mountMs": mount_ms,
        "workspace": value,
        "exec": ran,
        "commitMs": commit_ms,
        "commit": committed,
        "unmountMs": unmount("timed", ws, False),
    }
    value, _ = mount("verify")
    identity = run(value["ws"], W["identity"], "verify.identity")
    out["where"] = dict(line.split("=", 1) for line in identity["stdout"].splitlines() if "=" in line)
    checked = run(value["ws"], C["verifier"], "verify.exec")
    unmount("verify", value["ws"], True)
    out["phases"]["verify"] = {"exitCode": checked["exitCode"], "stdout": checked["stdout"], "stderr": checked["stderr"]}
    timed = out["phases"]["timed"]
    problems = []
    if timed["exec"]["exitCode"] != 0:
        problems.append(f"exec exit {timed['exec']['exitCode']}")
    if timed["exec"]["rc"] not in (None, 0):
        problems.append(f"body exit {timed['exec']['rc']}")
    commit = timed["commit"] or {}
    if not commit.get("commit") and (commit.get("kind") or commit.get("outcome")) != "UpToDate":
        problems.append(f"Commit answered {json.dumps(commit)}")
    verify = out["phases"]["verify"]
    if verify["exitCode"] != 0 or "VERIFY OK" not in (verify["stdout"] or "").splitlines():
        problems.append(f"verifier: {verify['stdout'].strip()} {verify['stderr'].strip()}".strip())
    out["ok"] = not problems
    out["cause"] = "; ".join(problems) or None
except Exception as error:
    out["ok"] = False
    out["cause"] = str(error)
finally:
    for ws in list(mounted):
        try:
            call("workspace", "unmount", ws, discard=True, force=True)
            out["steps"].append({"name": "cleanup.unmount", "ok": True})
        except Exception as error:
            out["steps"].append({"name": "cleanup.unmount", "ok": False, "error": str(error)[:500]})
    tail = []
    if sandbox:
        try:
            step("sandbox.close", "sandbox", "close", sandbox)
        except Exception as error:
            tail.append(str(error))
    if pushed:
        try:
            step("store.pull", "store", "pull", store)
        except Exception as error:
            tail.append(str(error))
    try:
        step("store.remove", "store", "remove", store)
    except Exception as error:
        tail.append(str(error))
    if tail:
        out["ok"] = False
        out["cause"] = "; ".join([out["cause"]] + tail if out["cause"] else tail)
    os.rmdir(empty)
    out["endedAt"] = datetime.now(timezone.utc).isoformat()
    with open(receipt, "w") as f:
        f.write(json.dumps(out, indent=2) + "\n")
t = out["phases"].get("timed", {}).get("exec", {}).get("wallMs", "-")
print(f"{cell} {machine} {via} {'PASS' if out['ok'] else 'FAIL'} {out['cause'] or ''} timed exec {t} ms")
sys.exit(0 if out["ok"] else 1)
