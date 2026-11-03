"""K1 item 7 (binding parity): one #516 cell on `native` through the Python
package or the `layerfs` CLI binary, mirroring the Node runner's steps
(api/node/test/verify/cells/cell.mjs) in a fresh private home with a sqlite
Store: import-env, store.create, project.init (empty folder), branch.fork,
sandbox.create, [setup mount/exec/commit/unmount], warm-up
mount/exec/unmount --discard, timed mount/exec/commit/unmount, verify
mount/identity/verifier/unmount --discard, sandbox.close, home removed.

    python3 parity_cell.py <C01..C12> <python|cli> <workloads.json> <receipt.json>

workloads.json is workloads-dump.mjs's output (the runner's exact bytes).
Env: LFS533_DAEMON (macOS daemon), LFS533_LAYERFS (CLI), LFS533_PRODUCT;
PYTHONPATH must hold the Python package for the python binding.
One attempt per call; nothing is retried.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone

cell, binding, workloads_path, receipt = sys.argv[1:5]
W = json.load(open(workloads_path))
spec = W[cell]
LFS = os.environ["LFS533_LAYERFS"]
home = tempfile.mkdtemp(prefix="lfs535k1-par-")
os.chmod(home, 0o700)
empty = tempfile.mkdtemp(prefix="lfs535k1-src-")


class Refused(Exception):
    def __init__(self, code, message):
        super().__init__(f"{code} {message}")
        self.code = code


if binding == "python":
    import layerfs

    lfs = layerfs.client(home=home)

    def call(name, *args, **opts):
        family, op = name.split(".")
        try:
            return getattr(getattr(lfs, family), op)(*args, **opts)
        except layerfs.LayerFSError as error:
            raise Refused(getattr(error, "code", None), str(error)) from None

elif binding == "cli":

    def call(name, *args, **opts):
        family, op = name.split(".")
        argv = [LFS, family, op, *[str(a) for a in args]]
        for key, value in opts.items():
            flag = "--" + key.replace("_", "-")
            if value is True:
                argv.append(flag)
            elif value is False or value is None:
                continue
            else:
                argv += [flag, str(value)]
        env = dict(os.environ, LAYERFS_HOME=home)
        done = subprocess.run(argv, env=env, capture_output=True, text=True)
        if done.returncode != 0:
            text = (done.stderr or done.stdout).strip()
            try:
                body = json.loads(text)
                body = body.get("refused", body)
                raise Refused(body.get("code"), body.get("message", text))
            except ValueError:
                raise Refused(f"exit:{done.returncode}", text[:2000]) from None
        return json.loads(done.stdout) if done.stdout.strip() else None

else:
    sys.exit("binding is python or cli")


def parse_timed(stdout):
    found = re.search(r"^LFS527 (\S+) (\S+) (\S+)$", stdout or "", re.M)
    if not found:
        return {"rc": None, "ms": None}
    rc = int(found.group(1)) if found.group(1).isdigit() else None
    ns = lambda t: re.fullmatch(r"\d{16,}", t) is not None  # noqa: E731
    ms = (int(found.group(3)) - int(found.group(2))) / 1e6 if ns(found.group(2)) and ns(found.group(3)) else None
    return {"rc": rc, "ms": ms}


out = {
    "format": "lfs533-cell/1",
    "binding": binding,
    "cell": cell,
    "machine": "native",
    "product": os.environ.get("LFS533_PRODUCT"),
    "startedAt": datetime.now(timezone.utc).isoformat(),
    "load1": os.getloadavg()[0],
    "body": {"sha256": spec["bodySha256"]},
    "store": {"kind": "sqlite"},
    "ok": False,
    "cause": None,
    "phases": {},
    "steps": [],
}


class StepFailed(Exception):
    pass


def step(name, fn):
    at = time.perf_counter()
    try:
        value = fn()
    except Refused as error:
        out["steps"].append({"name": name, "ms": round((time.perf_counter() - at) * 1000, 3), "ok": False, "error": {"code": error.code, "message": str(error)[:2000]}})
        raise StepFailed(f"{name}: {error}") from None
    ms = round((time.perf_counter() - at) * 1000, 3)
    out["steps"].append({"name": name, "ms": ms, "ok": True})
    return value, ms


def summary(value, ms):
    return {"wallMs": ms, "exitCode": value.get("exit_code"), "stdout": (value.get("stdout") or "")[:2000], "stderr": (value.get("stderr") or "")[:2000], **parse_timed(value.get("stdout"))}


mounted = set()
sandbox = None
project = "cells/p"
msg = lambda label: f"lfs533 {cell} {label}"  # noqa: E731
try:

    def import_env():
        env = {"PATH": os.environ["PATH"], "HOME": os.environ["HOME"], "LAYERFS_HOME": home, "LAYERFS_DAEMON": os.environ["LFS533_DAEMON"]}
        done = subprocess.run([LFS, "import-env"], env=env, capture_output=True, text=True)
        if done.returncode != 0:
            raise Refused(f"exit:{done.returncode}", done.stderr)

    step("import-env", import_env)
    step("store.create", lambda: call("store.create", "cells"))
    step("project.init", lambda: call("project.init", project, empty))
    step("branch.fork", lambda: call("branch.fork", project, "lane", "main"))
    out["sandbox"], _ = step("sandbox.create", lambda: call("sandbox.create", project, "c" + cell[1:], "native"))
    sandbox = "c" + cell[1:]

    def mount(label):
        value, ms = step(f"{label}.mount", lambda: call("workspace.mount", sandbox, "lane"))
        mounted.add(value["ws"])
        return value, ms

    def unmount(label, ws, discard):
        _, ms = step(f"{label}.unmount", lambda: call("workspace.unmount", ws, discard=discard))
        mounted.discard(ws)
        return ms

    def ex(ws, text, label):
        return step(label, lambda: call("workspace.exec", ws, text, message=msg(label)))

    if spec["setup"]:
        value, _ = mount("setup")
        ran = summary(*ex(value["ws"], spec["setup"], "setup.exec"))
        if ran["exitCode"] != 0:
            raise StepFailed(f"setup body exited {ran['exitCode']}: {ran['stderr']}")
        committed, _ = step("setup.commit", lambda: call("workspace.commit", value["ws"], message=f"lfs533 {cell} setup"))
        unmount("setup", value["ws"], False)
        out["phases"]["setup"] = {"exec": ran, "commit": committed}
    value, mms = mount("warmup")
    ran = summary(*ex(value["ws"], spec["line"], "warmup.exec"))
    ums = unmount("warmup", value["ws"], True)
    out["phases"]["warmup"] = {"mountMs": mms, "exec": ran, "unmountMs": ums}
    value, mms = mount("timed")
    ws = value["ws"]
    ran = summary(*ex(ws, spec["line"], "timed.exec"))
    committed, cms = step("timed.commit", lambda: call("workspace.commit", ws, message=f"lfs533 {cell}"))
    ums = unmount("timed", ws, False)
    out["phases"]["timed"] = {"mountMs": mms, "workspace": value, "exec": ran, "commitMs": cms, "commit": committed, "unmountMs": ums}
    value, _ = mount("verify")
    identity, _ = ex(value["ws"], W["IDENTITY"], "verify.identity")
    out["where"] = {line.split("=", 1)[0]: (line.split("=", 1)[1] or None) for line in (identity.get("stdout") or "").split("\n") if "=" in line}
    checked, _ = ex(value["ws"], spec["verifier"], "verify.exec")
    unmount("verify", value["ws"], True)
    out["phases"]["verify"] = {"exitCode": checked.get("exit_code"), "stdout": (checked.get("stdout") or "")[:2000], "stderr": (checked.get("stderr") or "")[:2000]}
    timed = out["phases"]["timed"]
    problems = []
    if timed["exec"]["exitCode"] != 0:
        problems.append(f"exec exit {timed['exec']['exitCode']}")
    if timed["exec"]["rc"] not in (None, 0):
        problems.append(f"body exit {timed['exec']['rc']}")
    c = timed["commit"] or {}
    if not c.get("commit") and (c.get("kind") or c.get("outcome")) != "UpToDate":
        problems.append(f"Commit answered {json.dumps(c)}")
    v = out["phases"]["verify"]
    if v["exitCode"] != 0 or not re.search(r"^VERIFY OK$", v["stdout"], re.M):
        problems.append(f"verifier: {v['stdout'].strip()} {v['stderr'].strip()}".strip())
    out["ok"] = not problems
    out["cause"] = "; ".join(problems) or None
except StepFailed as error:
    out["ok"] = False
    out["cause"] = str(error)
finally:
    for ws in list(mounted):
        try:
            call("workspace.unmount", ws, discard=True, force=True)
            out["steps"].append({"name": "cleanup.unmount", "ok": True})
        except Refused as error:
            out["steps"].append({"name": "cleanup.unmount", "ok": False, "error": str(error)})
    if sandbox:
        try:
            step("sandbox.close", lambda: call("sandbox.close", sandbox))
        except StepFailed as error:
            out["closeError"] = str(error)
    out["endedAt"] = datetime.now(timezone.utc).isoformat()
    with open(receipt, "w") as f:
        f.write(json.dumps(out, indent=2) + "\n")
    shutil.rmtree(home, ignore_errors=True)
    shutil.rmtree(empty, ignore_errors=True)
t = out["phases"].get("timed", {}).get("exec", {}).get("wallMs", "-")
print(f"{cell} native {binding} {'PASS' if out['ok'] else 'FAIL'} {out['cause'] or ''} timed exec {t} ms")
sys.exit(0 if out["ok"] else 1)
