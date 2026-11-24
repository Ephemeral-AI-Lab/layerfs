"""Read-only helpers for the qv2 similarity experiment (schema 42 Store).

Every connection uses SQLite's immutable URI, so the Store file is never
written and no sidecar files are created next to it.
"""

import sqlite3
import struct
from collections import OrderedDict
from compression import zstd

import numpy as np

u32 = lambda b, o: struct.unpack_from("<I", b, o)[0]
WF_OVER = 23
CHUNK_OVER = 21
M64 = (1 << 64) - 1


def connect(db):
    return sqlite3.connect(f"file:{db}?immutable=1", uri=True)


class Lru(OrderedDict):
    def __init__(self, cap):
        super().__init__()
        self.cap = cap

    def get_(self, k):
        v = self.get(k)
        if v is not None:
            self.move_to_end(k)
        return v

    def put(self, k, v):
        self[k] = v
        self.move_to_end(k)
        if len(self) > self.cap:
            self.popitem(last=False)


def pack_groups(body):
    """Group bodies of a payload pack (whole-file v23 or chunk v25), raw codec only."""
    ver, n, used = u32(body, 8), u32(body, 12), u32(body, 16)
    if ver == 23:
        starts = [u32(body, 24 + 4 * i) for i in range(n)]
        return [body[s:(starts[i + 1] if i + 1 < n else used)] for i, s in enumerate(starts)]
    out = []
    for i in range(n):
        o = 24 + 16 * i
        s, enc, codec = u32(body, o), u32(body, o + 4), body[o + 12]
        raw = body[s:s + enc]
        out.append(zstd.decompress(raw) if codec == 1 else raw)
    return out


def framed(group, k):
    n = u32(group, 0)
    st = 4 + 4 * n + (u32(group, 4 + 4 * (k - 1)) if k else 0)
    return group[st:4 + 4 * n + u32(group, 4 + 4 * k)]


class Payloads:
    """Decodes chunk and whole-file payloads, following PREFIX chains."""

    def __init__(self, con, loc, base, pack_cap=3000, raw_cap=60000):
        self.con, self.loc, self.base = con, loc, base
        self.packs = Lru(pack_cap)
        self.raw = Lru(raw_cap)
        self.decodes = 0

    def groups(self, pid):
        g = self.packs.get_(pid)
        if g is None:
            body = self.con.execute("SELECT body FROM pack WHERE pack_id=?", (pid,)).fetchone()[0]
            g = pack_groups(body)
            self.packs.put(pid, g)
        return g

    def record(self, oid):
        pid, g, r, role, canon = self.loc[oid]
        return framed(self.groups(pid)[g], r)

    def parts(self, oid):
        rec = self.record(oid)
        role = self.loc[oid][3]
        tag = rec[0]
        if role == 2:
            frame = rec[5 + (32 if tag == 1 else 0):]
        else:
            frame = rec[1 + (32 if tag == 1 else 0):]
        return tag, frame, rec

    def payload(self, oid):
        v = self.raw.get_(oid)
        if v is not None:
            return v
        chain = []
        o = oid
        while True:
            v = self.raw.get_(o)
            if v is not None:
                break
            tag, frame, _ = self.parts(o)
            if tag == 1:
                chain.append((o, frame))
                o = self.base[o]
                continue
            v = frame if tag == 2 else zstd.decompress(frame)
            self.decodes += 1
            self.raw.put(o, v)
            break
        for o, frame in reversed(chain):
            v = zstd.decompress(frame, zstd_dict=zstd.ZstdDict(v, is_raw=True))
            self.decodes += 1
            self.raw.put(o, v)
        return v


def load_payload_locations(con, roles=(1, 2)):
    q = ("SELECT object_id, pack_id, group_number, record_number, role, canonical_length "
         f"FROM object_location WHERE role IN ({','.join(map(str, roles))})")
    return {o: (p, g, r, role, c) for o, p, g, r, role, c in con.execute(q)}


def load_bases(path, roles=(1, 2)):
    data = open(path, "rb").read()
    return {data[i:i + 32]: data[i + 32:i + 64] for i in range(0, len(data), 65) if data[i + 64] in roles}


def depths(base):
    d = {}
    for o in base:
        chain = []
        x = o
        while x in base and x not in d:
            chain.append(x)
            x = base[x]
        k = d.get(x, 0)
        for y in reversed(chain):
            k += 1
            d[y] = k
    return d


# ---- product content signature (encoding/delta/candidates.rs) ----

def mix_py(v):
    v = ((v ^ (v >> 30)) * 0xbf58476d1ce4e5b9) & M64
    v = ((v ^ (v >> 27)) * 0x94d049bb133111eb) & M64
    return v ^ (v >> 31)


def signature_py(raw):
    """Reference implementation, byte for byte the product's algorithm."""
    W = 16
    res = []
    if len(raw) < W:
        return res
    high = pow(257, W - 1, 1 << 64)
    h = 0
    for b in raw[:W]:
        h = (h * 257 + b) & M64
    for s in range(0, len(raw) - W + 1):
        if s:
            h = ((h - raw[s - 1] * high) * 257 + raw[s + W - 1]) & M64
        x = mix_py(h)
        if (len(res) < 8 or x < res[-1]) and x not in res:
            res.append(x)
            res.sort()
            res = res[:8]
    return res


INV257 = pow(257, -1, 1 << 64)
_POW = _INV = None


def _tables(n):
    global _POW, _INV
    if _POW is None or len(_POW) < n:
        n2 = max(n, 1 << 22)
        k = np.arange(n2, dtype=np.uint64)
        # powers by repeated squaring are awkward in numpy; build by cumulative product
        p = np.empty(n2, dtype=np.uint64)
        q = np.empty(n2, dtype=np.uint64)
        p[0] = 1
        q[0] = 1
        b = np.uint64(257)
        ib = np.uint64(INV257)
        with np.errstate(over="ignore"):
            # doubling fill: p[m:2m] = p[0:m] * 257^m
            m = 1
            while m < n2:
                e = min(m, n2 - m)
                pm = np.uint64(pow(257, m, 1 << 64))
                qm = np.uint64(pow(INV257, m, 1 << 64))
                p[m:m + e] = p[:e] * pm
                q[m:m + e] = q[:e] * qm
                m *= 2
        _POW, _INV = p, q
        del k


def _mix_np(v):
    with np.errstate(over="ignore"):
        v = (v ^ (v >> np.uint64(30))) * np.uint64(0xbf58476d1ce4e5b9)
        v = (v ^ (v >> np.uint64(27))) * np.uint64(0x94d049bb133111eb)
        return v ^ (v >> np.uint64(31))


def signatures_np(payloads):
    """Product signatures for a batch of payloads: list of up-to-8 sorted uint64 lists."""
    lens = np.fromiter((len(p) for p in payloads), dtype=np.int64, count=len(payloads))
    a = np.frombuffer(b"".join(payloads), dtype=np.uint8).astype(np.uint64)
    L = len(a)
    _tables(L + 32)
    with np.errstate(over="ignore"):
        q = np.empty(L + 1, dtype=np.uint64)
        q[0] = 0
        np.cumsum(a * _INV[:L], out=q[1:])
        nwin = L - 15
        h = (q[16:16 + nwin] - q[:nwin]) * _POW[15:15 + nwin]
    h = _mix_np(h)
    starts = np.concatenate(([0], np.cumsum(lens)[:-1]))
    out = []
    for s, n in zip(starts.tolist(), lens.tolist()):
        if n < 16:
            out.append([])
            continue
        seg = h[s:s + n - 15]
        k = min(64, len(seg))
        part = np.partition(seg, k - 1)[:k] if len(seg) > k else seg
        u = np.unique(part)[:8]
        if len(u) < 8 and len(seg) > k:
            u = np.unique(seg)[:8]
        out.append(u.tolist())
    return out
