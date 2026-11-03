"""K1 item 8 (#536 §12 item 4): locks.

    python3 locks.py <home> <data folder> <receipt.json>

1. Two processes create Sandboxes of different projects (Stores l1, l2 on
   `native`; then l3, l4 pushed to `docker`) at the same instant (both wait
   for one start file); each records its own start/end. Solo creates of the
   same kind are timed first. They do not wait on each other when the two
   intervals overlap and neither concurrent create takes about a solo
   create plus the other's.
2. Two processes push the same Store (l5, 24 MB) to `docker` at the same
   instant: one is refused `Busy`, the other pushes.
One attempt per call.
"""

import json
import os
import subprocess
import sys
import time

if sys.argv[1] == "--one":
    # child: wait for the go file, then one call; print {start, end, ok, ...}
    _, _, home, go, what, a, b, c = sys.argv
    import layerfs

    lfs = layerfs.client(home=home)
    while not os.path.exists(go):
        time.sleep(0.001)
    start = time.time()
    try:
        if what == "create":
            value = lfs.sandbox.create(a, b, c)
        else:
            value = lfs.store.push(a, b)
        res = {"ok": True, "value": value}
    except layerfs.LayerFSError as e:
        res = {"ok": False, "code": e.code, "message": e.message, "detail": e.detail}
    res.update(start=start, end=time.time(), pid=os.getpid())
    print(json.dumps(res, default=str))
    sys.exit(0)

home, data, receipt = sys.argv[1:4]
import layerfs  # noqa: E402

lfs = layerfs.client(home=home)
out = {"format": "k1-locks/1", "home": home, "startedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "runs": {}, "checks": []}


def check(name, ok, why):
    out["checks"].append({"check": name, "pass": bool(ok), "why": why})
    print(("PASS " if ok else "FAIL ") + name + " — " + why, flush=True)


def pair(tag, calls):
    go = os.path.join(os.path.dirname(receipt), f".go-{tag}")
    if os.path.exists(go):
        os.remove(go)
    procs = [subprocess.Popen([sys.executable, __file__, "--one", home, go, *c], stdout=subprocess.PIPE, text=True) for c in calls]
    time.sleep(1.0)  # both children loaded and polling
    open(go, "w").close()
    res = [json.loads(p.communicate(timeout=110)[0]) for p in procs]
    os.remove(go)
    out["runs"][tag] = res
    return res


def solo(tag, call):
    return pair(tag, [call])[0]


for s in ("l1", "l2", "l3", "l4", "l5"):
    lfs.store.create(s)
    lfs.project.init(f"{s}/p", data)
for s in ("l3", "l4"):
    lfs.store.push(s, "docker")

for machine, (s1, s2) in (("native", ("l1", "l2")), ("docker", ("l3", "l4"))):
    a = solo(f"{machine}-solo", ["create", f"{s1}/p", f"solo-{s1}", machine])
    lfs.sandbox.close(f"solo-{s1}")
    one = a["end"] - a["start"]
    r = pair(f"{machine}-pair", [["create", f"{s1}/p", f"x-{s1}", machine], ["create", f"{s2}/p", f"x-{s2}", machine]])
    for x in r:
        x["dur"] = round(x["end"] - x["start"], 3)
    overlap = min(r[0]["end"], r[1]["end"]) - max(r[0]["start"], r[1]["start"])
    longest = max(x["dur"] for x in r)
    ok = all(x["ok"] for x in r) and overlap > 0 and longest < one + min(x["dur"] for x in r) * 0.9
    check(
        f"{machine}: two processes creating Sandboxes of different projects do not wait on each other",
        ok,
        f"solo {one:.3f}s; concurrent durations {[x['dur'] for x in r]}s; starts differ {abs(r[0]['start'] - r[1]['start']) * 1000:.1f} ms; overlap {overlap:.3f}s",
    )
    for x in r:
        if x["ok"]:
            lfs.sandbox.close(x["value"]["name"])

r = pair("push-same", [["push", "l5", "docker", "-"], ["push", "l5", "docker", "-"]])
codes = sorted("ok" if x["ok"] else x["code"] for x in r)
busy = [x for x in r if not x["ok"]]
check(
    "two processes pushing the same Store: one pushes, the other is refused Busy",
    codes == ["Busy", "ok"],
    f"answers {codes}; refusal {busy[0]['message'][:300] if busy else None}; starts differ {abs(r[0]['start'] - r[1]['start']) * 1000:.1f} ms",
)
for s in ("l3", "l4", "l5"):
    lfs.store.pull(s)
for s in ("l1", "l2", "l3", "l4", "l5"):
    lfs.store.remove(s)
out["doctor_end"] = layerfs.doctor(home)
out["endedAt"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
open(receipt, "w").write(json.dumps(out, indent=2, default=str) + "\n")
