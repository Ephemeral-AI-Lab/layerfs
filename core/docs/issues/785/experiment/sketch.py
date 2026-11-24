"""Product signatures for every chunk, in admission order (pack, group, record).

Resumable: parts already written are skipped. Usage: sketch.py TOOLS DB EDGES OUT NPARTS WORKERS
"""
import os, sys, signal, time
sys.path.insert(0, sys.argv[1])
import numpy as np
import lfs
TOOLS, DB, EDGES, OUT, NP, NW = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5]), int(sys.argv[6])

def order():
    con = lfs.connect(DB)
    rows = con.execute("SELECT object_id FROM object_location WHERE role=2 ORDER BY pack_id, group_number, record_number").fetchall()
    return [r[0] for r in rows]

SUB = 4

def work(job):
    part, sub = job
    path = f"{OUT}/sig_{part:03d}_{sub}.npz"
    if os.path.exists(path) or os.path.exists(f"{OUT}/sig_{part:03d}.npz"):
        return job, 0.0
    t = time.time()
    con = lfs.connect(DB)
    loc = lfs.load_payload_locations(con, roles=(2,))
    base = lfs.load_bases(EDGES, roles=(2,))
    ids = np.load(f"{OUT}/order.npy")
    n = len(ids)
    plo, phi = part * n // NP, (part + 1) * n // NP
    lo, hi = plo + (phi - plo) * sub // SUB, plo + (phi - plo) * (sub + 1) // SUB
    P = lfs.Payloads(con, loc, base)
    sig = np.full((hi - lo, 8), np.iinfo(np.uint64).max, dtype=np.uint64)
    lens = np.zeros(hi - lo, dtype=np.int32)
    B = 256
    for s in range(lo, hi, B):
        batch = [bytes(ids[i]) for i in range(s, min(hi, s + B))]
        pays = [P.payload(o) for o in batch]
        for j, (p, g) in enumerate(zip(pays, lfs.signatures_np(pays))):
            sig[s - lo + j, :len(g)] = g
            lens[s - lo + j] = len(p)
    np.savez(path + ".tmp.npz", lo=lo, hi=hi, sig=sig, lens=lens, decodes=P.decodes)
    os.replace(path + ".tmp.npz", path)
    return job, time.time() - t

if __name__ == "__main__":
    import multiprocessing as mp
    if not os.path.exists(f"{OUT}/order.npy"):
        ids = order()
        np.save(f"{OUT}/order.npy", np.array([np.frombuffer(i, dtype=np.uint8) for i in ids]))
        print("order", len(ids))
    todo = [(p, k) for p in range(NP) for k in range(SUB)
            if not os.path.exists(f"{OUT}/sig_{p:03d}.npz") and not os.path.exists(f"{OUT}/sig_{p:03d}_{k}.npz")]
    t0 = time.time()
    with mp.get_context("fork").Pool(NW) as pool:
        def stop(*_):
            pool.terminate(); print("deadline: stopped with parts left", flush=True); os._exit(3)
        signal.signal(signal.SIGALRM, stop); signal.alarm(108)
        for part, sec in pool.imap_unordered(work, todo):
            print(f"part {part} {sec:.1f}s at {time.time()-t0:.0f}s", flush=True)
    left = [j for j in todo if not os.path.exists(f"{OUT}/sig_{j[0]:03d}_{j[1]}.npz")]
    print("left", left)
