"""L1 (from K1) item 10: the #536 §10b contract on a fresh macOS home with `native` and
`docker` (and a PostgreSQL Store), through the Python package; helper
processes (a lease holder, a paused push) are this same file run with a
`--holder` / `--pauser` mode.

    python3 contract_10b.py <home> <data folder> <pg url> <receipt.json>

L1 changes for 35ed671b6c (the fixes since fe6f85013b): project.init
progress is checked per route (check, [send], install, record: send byte
counts only where Init goes through a worker); the worker keeps no folder
for a pulled Store; `backup.restore` InUse fills detail.related (a lane
MinIO bucket, env LFS535_L1_BUCKET_URL and LAYERFS_BUCKET_L1B_*); the
unresolved row (a push SIGKILLed in the record window by intent_kill.py,
then the refusals naming the push that clears it, and that push); the
busy-row file.list and unresolved-row file/branch reads are recorded spec
differences (#536 §10b vs the 2.0.0 docs), kept as checks and labelled.

Every call is recorded as {call, args, ok, value | code, message, detail};
every check as {check, pass, why}. One attempt per call; nothing retried.
"""

import atexit
import json
import os
import subprocess
import sys
import time

import layerfs

if sys.argv[1] == "--holder":
    # keep Store `store`'s lease: one Store call, then stay alive
    _, _, home, project, seconds, marker = sys.argv
    lfs = layerfs.client(home=home)
    lfs.file.list(project, "lane", "/")
    open(marker, "w").write(str(os.getpid()))
    time.sleep(float(seconds))
    sys.exit(0)
if sys.argv[1] == "--pauser":
    # push Store `store` to `machine`, pausing `seconds` in the first send event
    _, _, home, store, machine, seconds, marker = sys.argv
    lfs = layerfs.client(home=home)
    paused = {"done": False}

    def on_progress(p):
        with open(marker + ".events", "a") as f:
            f.write(json.dumps(p) + "\n")
        if p.get("phase") == "send" and not paused["done"]:
            paused["done"] = True
            open(marker, "w").write(str(os.getpid()))
            time.sleep(float(seconds))

    try:
        print(json.dumps(lfs.store.push(store, machine, on_progress=on_progress)))
    except layerfs.LayerFSError as e:
        print(json.dumps({"refused": e.code, "message": e.message}))
    sys.exit(0)

home, data, pg_url, receipt = sys.argv[1:5]
LFS = os.environ["LFS533_LAYERFS"]
lfs = layerfs.client(home=home)
out = {"format": "l1-contract-10b/1", "home": home, "startedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "calls": [], "checks": [], "events": {}}


def call(call_name, *args, quiet=False, **opts):
    family, op = call_name.split(".")
    entry = {"call": call_name, "args": [str(a) for a in args], "opts": {k: v for k, v in opts.items() if not callable(v)}, "at": round(time.time(), 3)}
    try:
        value = getattr(getattr(lfs, family), op)(*args, **opts)
        entry.update(ok=True, value=value if not quiet else "(omitted)")
        out["calls"].append(entry)
        return value
    except layerfs.LayerFSError as e:
        entry.update(ok=False, code=e.code, message=e.message, detail=e.detail)
        out["calls"].append(entry)
        return e


def write_receipt():
    out["endedAt"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    out["passed"] = sum(c["pass"] for c in out["checks"])
    out["failed"] = sum(not c["pass"] for c in out["checks"])
    open(receipt, "w").write(json.dumps(out, indent=2, default=str) + "\n")


atexit.register(write_receipt)


def check(name, ok, why=""):
    out["checks"].append({"check": name, "pass": bool(ok), "why": why})
    print(("PASS " if ok else "FAIL ") + name + (f" — {why}" if why and not ok else ""), flush=True)


def refused(r, code):
    return isinstance(r, layerfs.LayerFSError) and r.code == code


def related(r):
    d = r.detail if isinstance(r, layerfs.LayerFSError) else None
    return (d or {}).get("related") if isinstance(d, dict) else None


def has_related(r, kind, name=None):
    rel = related(r) or []
    return any(x.get("kind") == kind and (name is None or str(x.get("name")) == str(name)) for x in rel)


def cli(*args):
    done = subprocess.run([LFS, *args], env=dict(os.environ, LAYERFS_HOME=home), capture_output=True, text=True)
    out["calls"].append({"cli": list(args), "exit": done.returncode, "stdout": done.stdout[:4000], "stderr": done.stderr[:2000]})
    return done


def worker_running():
    hid = json.load(open(os.path.join(home, "layerfs.json")))["id"][:8]
    done = subprocess.run(["docker", "inspect", "-f", "{{.State.Running}}", f"lfs-{hid}-docker"], capture_output=True, text=True)
    return done.stdout.strip() == "true", done.stdout.strip() or done.stderr.strip()


def events(tag):
    got = []
    out["events"][tag] = got
    return lambda p: got.append(p)


def wait_file(path, seconds):
    end = time.time() + seconds
    while time.time() < end and not os.path.exists(path):
        time.sleep(0.05)
    return os.path.exists(path)


PHASES = ["check", "snapshot", "send", "verify", "install", "record", "retire"]


def progress_ok(tag):
    ev = out["events"].get(tag, [])
    phases = [e.get("phase") for e in ev]
    send = [e for e in ev if e.get("phase") == "send"]
    other = [e for e in ev if e.get("phase") != "send"]
    good = (
        bool(ev)
        and all(set(e) >= {"phase", "done", "total"} for e in ev)
        and all(p in PHASES for p in phases)
        and bool(send)
        and all(isinstance(e["done"], int) and isinstance(e["total"], int) for e in send)
        and send[-1]["done"] == send[-1]["total"]
        and all(e["done"] is None and e["total"] is None for e in other)
    )
    return good, f"phases {list(dict.fromkeys(phases))}, send events {len(send)}, last {send[-1] if send else None}"


# ---- setup -----------------------------------------------------------------
subprocess.run([LFS, "import-env"], env={"PATH": os.environ["PATH"], "HOME": os.environ["HOME"], "LAYERFS_HOME": home, "LAYERFS_DAEMON": os.environ["LFS533_DAEMON"]}, check=True)
call("machine.start", "docker")
call("store.create", "a")
call("project.init", "a/p", data, on_progress=events("project.init a (native sqlite)"))
call("branch.fork", "a/p", "lane", "main")
call("store.create", "b")
call("project.init", "b/p", data)
call("branch.fork", "b/p", "lane", "main")
call("store.push", "b", "docker", on_progress=events("store.push b"))
call("store.pull", "b", on_progress=events("store.pull b"))
_hid = json.load(open(os.path.join(home, "layerfs.json")))["id"][:8]
_ls = subprocess.run(["docker", "exec", f"lfs-{_hid}-docker", "sh", "-c", "test -e /layerfs/data/store/b && find /layerfs/data/store/b || echo NO-FOLDER"], capture_output=True, text=True).stdout.strip()
check("after store.pull b the worker keeps no folder for b (retire removes the whole folder)", _ls == "NO-FOLDER", _ls[:300])
call("store.push", "b", "docker")
call("project.init", "b/q", data, on_progress=events("project.init b/q (sqlite on docker)"))
call("store.create", "c")
call("project.init", "c/p", data)
call("store.connect", "g", pg_url, password=os.environ["LFS535_PG_PASSWORD"])
call("project.init", "g/p", data, on_progress=events("project.init g (postgresql)"))
call("branch.fork", "g/p", "lane", "main")

# ---- progress --------------------------------------------------------------
for tag in ("store.push b", "store.pull b"):
    ok, why = progress_ok(tag)
    check(f"progress {tag}: {{progress:{{phase,done,total}}}}, bytes in send, null elsewhere", ok, why)
INIT = ["check", "send", "install", "record"]
for tag, worker in (("project.init a (native sqlite)", False), ("project.init b/q (sqlite on docker)", True), ("project.init g (postgresql)", False)):
    ev = out["events"][tag]
    shape = all(set(e) >= {"phase", "done", "total"} and set(e) <= {"phase", "done", "total"} for e in ev)
    phases = list(dict.fromkeys(e.get("phase") for e in ev))
    send = [e for e in ev if e.get("phase") == "send"]
    other = [e for e in ev if e.get("phase") != "send"]
    order = [p for p in INIT if p in phases] == phases and {"check", "install", "record"} <= set(phases)
    check(f"progress {tag}: events are {{phase, done, total}} only, phases in order check,[send],install,record", bool(ev) and shape and order, f"{len(ev)} events, phases {phases}, first {ev[:1]}")
    check(f"progress {tag}: done/total null outside send", all(e.get("done") is None and e.get("total") is None for e in other), f"{other[:2]}")
    if worker:
        check(f"progress {tag}: byte counts during send (Init through the worker)", bool(send) and all(isinstance(e["done"], int) and isinstance(e["total"], int) for e in send) and send[-1]["done"] == send[-1]["total"], f"send events {send[:2]} .. {send[-1:]}")
    else:
        check(f"progress {tag}: no send phase (Init in the caller's process)", not send, f"send events {send[:2]}")

# ---- Sandboxes, lists, Workspace handles -----------------------------------
call("sandbox.create", "a/p", "sa", "native")
call("sandbox.create", "b/p", "sb", "docker")
call("sandbox.create", "g/p", "sg", "native")
ml = call("machine.list")
sl = call("store.list")
xl = call("sandbox.list")
check("machine.list items carry state (and supports)", all("state" in m and "supports" in m for m in ml["machines"]), json.dumps([(m["name"], m.get("state")) for m in ml["machines"]]))
check("store.list items carry state", all("state" in s for s in sl["items"]), json.dumps([(s["name"], s.get("state"), s.get("op")) for s in sl["items"]]))
check("sandbox.list items carry state", all("state" in s for s in xl["items"]), json.dumps([(s["name"], s.get("state"), s.get("op")) for s in xl["items"]]))

w1 = call("workspace.mount", "sa", "lane", name="w1")
w2 = call("workspace.mount", "sa", "lane")
got = call("sandbox.get", "sa")
items = {w.get("name") or w.get("path"): w for w in got.get("workspaces", [])} if isinstance(got, dict) else {}
check("sandbox.get workspaces[] carry ws, name, path", len(items) == 2 and all({"ws", "name", "path"} <= set(w) for w in items.values()), json.dumps(got.get("workspaces") if isinstance(got, dict) else str(got))[:800])
named = [w for w in got["workspaces"] if w.get("name") == "w1"]
unnamed = [w for w in got["workspaces"] if w.get("name") is None]
check("named Workspace's ws is <sandbox>:<name>", bool(named) and named[0]["ws"] == "sa:w1", json.dumps(named))
check("unnamed Workspace's ws is <sandbox>:<path>", bool(unnamed) and unnamed[0]["ws"] == f"sa:{unnamed[0]['path']}", json.dumps(unnamed))
by_name, by_path_of_named = "sa:w1", f"sa:{named[0]['path']}" if named else None
r1 = call("workspace.exec", by_name, "echo n > n.txt", message="k1 name form")
r2 = call("workspace.exec", by_path_of_named, "cat n.txt", message="k1 path form")
r3 = call("workspace.status", by_name)
r4 = call("workspace.status", by_path_of_named)
check("workspace.exec/status accept <sandbox>:<name> and <sandbox>:<path> of the same Workspace", not any(isinstance(r, layerfs.LayerFSError) for r in (r1, r2, r3, r4)) and r2.get("stdout", "").strip() == "n", f"path-form read {r2 if isinstance(r2, layerfs.LayerFSError) else r2.get('stdout')}")
r5 = call("workspace.commit", by_name, message="k1 commit by name")
r6 = call("workspace.unmount", by_path_of_named)
r7 = call("workspace.exec", w2["ws"], "true", message="k1 path ws")
r8 = call("workspace.unmount", w2["ws"], discard=True)
check("workspace.commit by name, unmount by path, exec/unmount on an unnamed ws", not any(isinstance(r, layerfs.LayerFSError) for r in (r5, r6, r7, r8)), "")

# ---- InUse with related ----------------------------------------------------
r = call("store.push", "a", "docker")
check("store.push refused InUse while a Sandbox record exists, related names it", refused(r, "InUse") and has_related(r, "sandbox", "sa"), str(getattr(r, "detail", r)))
r = call("store.remove", "a")
check("store.remove refused InUse, related names the Sandbox", refused(r, "InUse") and has_related(r, "sandbox", "sa"), str(getattr(r, "detail", r)))
r = call("project.remove", "a/p")
check("project.remove refused InUse, related names the Sandbox", refused(r, "InUse") and has_related(r, "sandbox", "sa"), f"{getattr(r, 'code', r)} {getattr(r, 'detail', '')}")
r = call("store.pull", "b")
check("store.pull refused InUse while a Sandbox record exists, related names it", refused(r, "InUse") and has_related(r, "sandbox", "sb"), str(getattr(r, "detail", r)))
r = call("machine.stop", "docker")
check("machine.stop refused InUse while a Sandbox runs, related names it", refused(r, "InUse") and has_related(r, "sandbox", "sb"), f"{getattr(r, 'code', r)} {getattr(r, 'detail', '')}")
r = call("machine.remove", "docker")
check("machine.remove docker (built-in): refusal recorded", isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', r)} {getattr(r, 'message', '')} related={related(r)}")
r = call("store.disconnect", "g")
check("store.disconnect refused InUse while a Sandbox of the PostgreSQL Store exists, related names it", refused(r, "InUse") and has_related(r, "sandbox", "sg"), f"{getattr(r, 'code', r)} {getattr(r, 'message', '')} {getattr(r, 'detail', '')}")
# backup.restore InUse with related (a Sandbox of the Store's project)
bucket_url = os.environ.get("LFS535_L1_BUCKET_URL")
if bucket_url:
    call("backup.connect", "l1b", bucket_url)
    call("store.create", "k")
    call("project.init", "k/p", data)
    call("branch.fork", "k/p", "lane", "main")
    call("backup.create", "k", "l1b")
    call("sandbox.create", "k/p", "sk", "native")
    r = call("backup.restore", "k", "l1b")
    check("backup.restore refused InUse while a Sandbox of the Store runs, related names it", refused(r, "InUse") and has_related(r, "sandbox", "sk"), f"{getattr(r, 'code', r)} {getattr(r, 'message', '')} {getattr(r, 'detail', '')}")
    call("sandbox.close", "sk")
else:
    check("backup.restore InUse related: NOT_RUN (no bucket)", False, "LFS535_L1_BUCKET_URL unset")
r = call("store.remove", "g")
check("store.remove on a PostgreSQL Store refused InvalidInput naming store.disconnect", refused(r, "InvalidInput") and "store disconnect" in r.message.replace(".", " "), f"{getattr(r, 'code', r)} {getattr(r, 'message', '')}")

# a gone Sandbox's record still refuses push (running or not)
call("sandbox.create", "a/p", "sx", "native")
pids = subprocess.run(["pgrep", "-f", f"{home}.*sx"], capture_output=True, text=True).stdout.split()
out["sx_pids"] = pids
for pid in pids:
    subprocess.run(["kill", "-9", pid])
time.sleep(1)
gx = call("sandbox.get", "sx")
r = call("store.push", "a", "docker")
check("a Sandbox whose process was killed (record left)", isinstance(gx, dict), f"sandbox.get sx state={gx.get('state') if isinstance(gx, dict) else gx}, killed pids {pids}")
check("store.push refused InUse while that record exists, related names it", refused(r, "InUse") and has_related(r, "sandbox", "sx"), str(getattr(r, "detail", r)))
call("sandbox.close", "sx")
call("sandbox.close", "sa")
call("sandbox.close", "sg")
call("sandbox.close", "sb")

# ---- call-by-state: in_use (another process holds the lease) ---------------
marker = os.path.join(os.path.dirname(receipt), ".holder")
for f in (marker,):
    if os.path.exists(f):
        os.remove(f)
holder = subprocess.Popen([sys.executable, __file__, "--holder", home, "a/p", "20", marker])
wait_file(marker, 15)
hpid = open(marker).read().strip()
st = call("store.status", "a")
check("in_use: store.status state in_use", isinstance(st, dict) and st.get("state") == "in_use", f"state={st.get('state') if isinstance(st, dict) else st}")
lst = call("store.list")
check("in_use: store.list item state in_use", any(s["name"] == "a" and s.get("state") == "in_use" for s in lst["items"]), "")
for name, args in (("store.get", ("a",)), ("project.list", ("a",)), ("project.get", ("a/p",)), ("file.list", ("a/p", "lane", "/")), ("branch.list", ("a/p",))):
    r = call(name, *args, quiet=True)
    check(f"in_use: {name} answers", not isinstance(r, layerfs.LayerFSError), str(r))
r = call("sandbox.create", "a/p", "su", "native")
check("in_use: sandbox.create allowed", not isinstance(r, layerfs.LayerFSError), str(r))
call("sandbox.close", "su")
r = call("store.push", "a", "docker")
check("in_use: store.push refused InUse, related names the lease holder", refused(r, "InUse") and has_related(r, "lease", hpid), str(getattr(r, "detail", r)))
r = call("store.remove", "a")
check("in_use: store.remove refused InUse, related names the lease holder", refused(r, "InUse") and has_related(r, "lease", hpid), str(getattr(r, "detail", r)))
holder.wait(timeout=40)
os.remove(marker)
# PostgreSQL lease: store.disconnect refused InUse while another process holds it
holder = subprocess.Popen([sys.executable, __file__, "--holder", home, "g/p", "12", marker])
wait_file(marker, 15)
hpid = open(marker).read().strip()
r = call("store.disconnect", "g")
check("store.disconnect refused InUse while another process holds the PostgreSQL Store's lease, related names it", refused(r, "InUse") and has_related(r, "lease", hpid), f"{getattr(r, 'code', r)} {getattr(r, 'detail', '')}")
holder.wait(timeout=30)
os.remove(marker)
if bucket_url:
    holder = subprocess.Popen([sys.executable, __file__, "--holder", home, "k/p", "12", marker])
    wait_file(marker, 15)
    hpid = open(marker).read().strip()
    r = call("backup.restore", "k", "l1b")
    check("backup.restore refused InUse while another process holds the Store's lease, related names it", refused(r, "InUse") and has_related(r, "lease", hpid), f"{getattr(r, 'code', r)} {getattr(r, 'message', '')} {getattr(r, 'detail', '')}")
    holder.wait(timeout=30)
    os.remove(marker)

# ---- call-by-state: busy (a live push paused in send) ----------------------
# Store c: this process has made no Store call on it, so holds no lease on it
pauser = subprocess.Popen([sys.executable, __file__, "--pauser", home, "c", "docker", "14", marker], stdout=subprocess.PIPE, text=True)
wait_file(marker, 20)
check("busy: the pauser reached send (marker written)", os.path.exists(marker), "")
st = call("store.status", "c")
check("busy: store.status state busy with op", isinstance(st, dict) and st.get("state") == "busy" and isinstance(st.get("op"), dict), f"state={st.get('state') if isinstance(st, dict) else st} op={st.get('op') if isinstance(st, dict) else ''}")
lst = call("store.list")
check("busy: store.list item carries state busy and op", any(s["name"] == "c" and s.get("state") == "busy" and s.get("op") for s in lst["items"]), json.dumps([s for s in lst["items"] if s["name"] == "c"]))
for name, args in (("store.get", ("c",)), ("project.list", ("c",)), ("project.get", ("c/p",))):
    r = call(name, *args, quiet=True)
    check(f"busy: {name} answers", not isinstance(r, layerfs.LayerFSError), str(r))
r = call("file.list", "c/p", "main", "/", quiet=True)
check("busy: file.list answers (from the source before record) [recorded spec difference: 2.0.0 docs say Busy]", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')}")
for name, args, code in (
    ("sandbox.create", ("c/p", "sbusy", "native"), "Busy"),
    ("project.init", ("c/q", data), "Busy"),
    ("project.remove", ("c/p",), "Busy"),
    ("store.push", ("c", "docker"), "Busy"),
    ("store.pull", ("c",), "Busy"),
    ("store.remove", ("c",), "Busy"),
):
    r = call(name, *args)
    check(f"busy: {name} refused {code} naming the op", refused(r, code) and "store.push" in (r.message + json.dumps(r.detail or {})), f"{getattr(r, 'code', r)} {getattr(r, 'message', '')[:200]}")
pushed, _ = pauser.communicate(timeout=60)
out["pauser"] = pushed.strip()
out["pauser_events"] = open(marker + ".events").read() if os.path.exists(marker + ".events") else None
for f in (marker, marker + ".events"):
    if os.path.exists(f):
        os.remove(f)
if any(c.get("call") == "sandbox.create" and c.get("ok") and c["args"][1] == "sbusy" for c in out["calls"]):
    call("sandbox.close", "sbusy")
check("busy: the paused push then completed", '"to"' in pushed and "refused" not in pushed, pushed.strip()[:300])

# ---- no list/get/status call starts a machine; down ------------------------
r = call("machine.stop", "docker")
running, why = worker_running()
check("machine.stop docker stops its worker", not isinstance(r, layerfs.LayerFSError) and not running, why)
for name, args in (
    ("machine.list", ()), ("machine.get", ("docker",)), ("machine.status", ("docker",)), ("machine.platforms", ()),
    ("store.list", ()), ("store.get", ("a",)), ("store.status", ("a",)), ("store.get", ("b",)), ("store.status", ("b",)),
    ("project.list", ()), ("project.get", ("b/p",)), ("sandbox.list", ()),
):
    r = call(name, *args, quiet=name in ("machine.platforms",))
    check(f"stopped: {name} answers", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')}")
d = layerfs.doctor(home)
out["doctor_while_stopped"] = d
running, why = worker_running()
check("after every list/get/status call, machine.platforms and doctor the docker worker is still stopped", not running, why)
st = call("store.status", "b")
check("down: store.status state down with down_reason", isinstance(st, dict) and st.get("state") == "down", f"state={st.get('state')} reason={st.get('down_reason')}")
lst = call("store.list")
check("down: store.list item state down", any(s["name"] == "b" and s.get("state") == "down" for s in lst["items"]), "")
for name, args in (("store.get", ("b",)), ("project.list", ("b",)), ("project.get", ("b/p",))):
    r = call(name, *args, quiet=True)
    check(f"down: {name} answers", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')}")
for name, args in (
    ("file.list", ("b/p", "lane", "/")),
    ("branch.list", ("b/p",)),
    ("sandbox.create", ("b/p", "sdown", "docker")),
    ("project.init", ("b/r", data)),
    ("project.remove", ("b/q",)),
    ("store.pull", ("b",)),
    ("store.remove", ("b",)),
):
    r = call(name, *args)
    check(f"down: {name} refused Unavailable naming `layerfs machine start` (D11)", refused(r, "Unavailable") and "machine start" in r.message, f"{getattr(r, 'code', r)} {getattr(r, 'message', '')[:200]}")
running, why = worker_running()
check("down: none of those calls started the docker worker", not running, why)

# ---- back to idle --------------------------------------------------------------
call("machine.start", "docker")
for name, args in (("store.get", ("b",)), ("file.list", ("b/p", "lane", "/"))):
    r = call(name, *args, quiet=True)
    check(f"idle (after machine.start): {name} answers", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')}")
r = call("sandbox.create", "b/p", "sidle", "docker")
check("idle: sandbox.create allowed", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')}")
call("sandbox.close", "sidle")
r = call("store.pull", "b")
check("idle: store.pull allowed", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')}")
r = call("store.pull", "c")
check("idle: store.pull (the paused push's Store) allowed", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')}")
# ---- call-by-state: unresolved (a push SIGKILLed in the record window) -------
# Store u is made by the CLI, so this process holds no lease on it
cli("store", "create", "u")
cli("project", "init", "u/p", data)
kill_log = os.path.join(os.path.dirname(receipt), "item10-unresolved-kill.events.log")
done = subprocess.run([sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)), "intent_kill.py"), home, "push", "u", "docker", "record-window", kill_log], capture_output=True, text=True)
out["unresolved_kill"] = {"exit": done.returncode, "events": open(kill_log).read()[-1500:] if os.path.exists(kill_log) else None}
st = call("store.status", "u")
check("unresolved: store.status state unresolved with op (uncertain)", isinstance(st, dict) and st.get("state") == "unresolved" and (st.get("op") or {}).get("uncertain") is True, f"state={st.get('state') if isinstance(st, dict) else st} op={st.get('op') if isinstance(st, dict) else ''}")
lst = call("store.list")
check("unresolved: store.list item state unresolved with op", any(s["name"] == "u" and s.get("state") == "unresolved" and s.get("op") for s in lst["items"]), json.dumps([s for s in lst["items"] if s["name"] == "u"]))
for name, args in (("store.get", ("u",)), ("project.list", ("u",)), ("project.get", ("u/p",))):
    r = call(name, *args, quiet=True)
    check(f"unresolved: {name} answers", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')}")
for name, args in (("file.list", ("u/p", "main", "/")), ("branch.list", ("u/p",))):
    r = call(name, *args, quiet=True)
    check(f"unresolved: {name} answers where the placement is [recorded spec difference: 2.0.0 docs say Unresolved]", not isinstance(r, layerfs.LayerFSError), f"{getattr(r, 'code', '')} {getattr(r, 'message', '')[:200]}")
for name, args in (("sandbox.create", ("u/p", "sun", "native")), ("project.init", ("u/q", data)), ("project.remove", ("u/p",))):
    r = call(name, *args)
    check(f"unresolved: {name} refused Unresolved naming store status and the push that clears it", refused(r, "Unresolved") and "store status u" in r.message and "store push u docker" in r.message, f"{getattr(r, 'code', r)} {getattr(r, 'message', '')[:300]}")
d = layerfs.doctor(home)
fu = [f for f in d.get("findings", []) if f["entity"]["name"] == "u"]
check("unresolved: doctor reports an error naming the push that clears it", any(f["severity"] == "error" and "store push u docker" in f.get("fix", "") for f in fu), json.dumps(fu)[:400])
r = call("store.push", "u", "docker")
check("unresolved: a new store.push clears it and runs (Store on docker)", isinstance(r, dict) and r.get("to") == "docker", f"{getattr(r, 'code', r)} {getattr(r, 'message', '')[:300]}")
st = call("store.status", "u")
check("unresolved: afterwards store.status idle on docker, no op", isinstance(st, dict) and st.get("state") == "idle" and st.get("machine") == "docker" and not st.get("op"), f"{st}"[:300])
check("unresolved: afterwards native keeps no store.sqlite for u", not os.path.exists(os.path.join(home, "store", "u", "store.sqlite")), str(os.listdir(os.path.join(home, "store", "u"))))
r = call("store.pull", "u")
check("unresolved: then store.pull u answers", isinstance(r, dict) and r.get("to") == "native", f"{getattr(r, 'code', r)} {getattr(r, 'message', '')[:200]}")

# ---- clean up ------------------------------------------------------------------
call("store.disconnect", "g")
for st_name in ("a", "b", "c", "k", "u"):
    call("store.remove", st_name)
if bucket_url:
    call("backup.disconnect", "l1b")
out["doctor_end"] = layerfs.doctor(home)
write_receipt()
print(f"10b checks: {out['passed']} PASS, {out['failed']} FAIL; doctor end {out['doctor_end']}")
