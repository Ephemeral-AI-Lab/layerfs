#!/usr/bin/env python3
"""w2sum.py <log>...: write-phase summary of write2 arms."""
import json
import sys


def load(path):
    out = {"_sha": "", "_exit": None}
    for line in open(path):
        if line.startswith("PROBE "):
            d = json.loads(line[6:])
            out[d["phase"]] = d
        elif line.startswith("EXIT="):
            out["_exit"] = line.strip()
        elif not out["_sha"] and "fuse_profile_probe" in line:
            out["_sha"] = line.split()[0][:12]
    return out


def st(d, name):
    v = d["stages"].get(name, [0, 0])
    return v


def kern(d, op):
    if "callbacks" in d:
        return d["callbacks"].get(op, [0, 0])
    k = d["kernel"]
    if isinstance(k, list):
        c = sum(x["opcodes"].get(op, [0, 0])[0] for x in k)
        n = sum(x["opcodes"].get(op, [0, 0])[1] for x in k)
        return c, n
    return k["opcodes"].get(op, [0, 0])


for path in sys.argv[1:]:
    r = load(path)
    arm = path.rsplit("/", 1)[-1][:-4]
    sw = r.get("switches", {})
    setup = r.get("setup", {})
    cr = r.get("cold_read", {})
    b3 = cr.get("stages", {}).get("object_hash", [1, 0])
    print(f"== {arm} sha {r['_sha']} {r['_exit']} b3_us {b3[1]/max(b3[0],1)/1e3:.1f} "
          f"max_write {cr.get('profile',{}).get('max_write')} bg {cr.get('profile',{}).get('max_background')} "
          f"cold {cr.get('mb_s')} MB/s cpu/MB {cr.get('cpu',{}).get('cpu_us_per_mb')} load {setup.get('loadavg','')[:14]}")
    for ph in ["seq_write", "random_write_4k", "small_create", "contend_seq", "contend_create",
               "contend_random_4k", "shared_random_4k"]:
        d = r.get(ph)
        if not d:
            print(f"  {ph}: MISSING")
            continue
        c = d["cpu"]
        unit = "file" if "create" in ph else "mb"
        if "create" in ph:
            n = d.get("files", 2000) if ph == "small_create" else 2000
            wall = d["seconds"] * 1e6 / n
            head = f"wall_us/file {wall:7.1f}"
        else:
            head = f"MB/s {d.get('mb_s', 0):7.1f}"
        g = kern(d, "getattr")
        rel = kern(d, "release")
        w = kern(d, "write")
        wl = st(d, "owner_wait_long")
        hold = st(d, "owner_hold")
        ver = d.get("verified")
        if isinstance(ver, dict):
            ver = ver["ok"]
        extra = ""
        if "create" in ph:
            extra = (f" getattr_cb {g[1]/max(g[0],1)/1e3:5.1f} release_cb {rel[1]/max(rel[0],1)/1e3:5.1f}"
                     f" reljob {st(d,'release_job')[1]/max(st(d,'release_job')[0],1)/1e3:4.1f}"
                     f" flush {st(d,'digest_flush')[0]}/{st(d,'digest_flush')[1]/1e6:.1f}ms"
                     f" begin {st(d,'sql_begin')[0]}")
        else:
            extra = f" write_cb {w[1]/max(w[0],1)/1e3:6.1f} n {w[0]}"
        print(f"  {ph:18s} {head} cpu/{unit} {c['cpu_us_per_'+unit]:8.2f} cpu/wall {c['cpu_per_wall']:.2f} "
              f"helper_ms {c['helper_us']/1e3:6.1f} waits>5us {wl[0]}/{wl[1]/1e6:.1f}ms jobs {hold[0]} "
              f"hold_ms {hold[1]/1e6:.1f} ok {ver}{extra}")
