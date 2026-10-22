"""#420 bundle study, exploration: encoded and compressed bundle bytes for
the grouping made by group.py (client-side byte counts, no Postgres).

Usage: python3 -I encode.py GROUPS.pickle ENCODING LEVELS [DICT_KB] [SAMPLE_MOD]
  ENCODING rows | col | colref
    rows    each row's columns as stored (ints 8 bytes BE, blobs with a
            varint length), tables in a fixed order
    col     columnar per table: sorted keys, delta/varint integers, fixed
            32-byte ids in their own columns, in-bundle id references
            (an inode value's content root / a digest's root / a
            signature's object that is a new object of the same bundle is
            its index), names as one length column + one byte column
    colref  col, plus a global object number for ids introduced by an
            earlier bundle (numbered in bundle order), run ids that name an
            earlier Commit by its bundle number, and (NAMES=1, default)
            binding names numbered globally on first appearance
  Env: OUT=file writes each body as [u32 LE length][body] in bundle order;
       COMMITS=file the bundle -> commit id list (33 bytes each, Init zero);
       DICT_OUT=file the trained dictionary; BREAKDOWN=1 per-section bytes.
  LEVELS  comma list of zstd levels; prefix d to use a dictionary trained
          on every 16th bundle's encoding (DICT_KB, default 112)
  SAMPLE_MOD k: encode only bundles with index % k == 0 (estimate, scaled)
"""
import pickle
import sys
import time
from compression import zstd


def uv(n):
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def zz(n):
    return uv((n << 1) ^ (n >> 63))


groups_file, encoding, levels = sys.argv[1], sys.argv[2], sys.argv[3].split(",")
dict_kb = int(sys.argv[4]) if len(sys.argv) > 4 else 112
mod = int(sys.argv[5]) if len(sys.argv) > 5 else 1
data = pickle.load(open(groups_file, "rb"))
G = data["groups"]
commits = data["meta"]["commits"]
nb = len(G["ol"])

# Global object numbers (bundle order, then the bundle's sorted order) and
# Commit numbers, for colref.
obj_num = {}
name_num = {}
import os as _os
PARENT_MAJOR = bool(_os.environ.get('PARENT_MAJOR'))
NAMES = _os.environ.get('NAMES', '1') == '1'
commit_num = {c: k + 1 for k, c in enumerate(commits)}


def sorted_ol(rows):
    return sorted(rows, key=lambda r: (r[3], r[4], r[5], r[0]))


def enc_rows(k):
    out = bytearray()
    for t in ["ol", "iv", "bh", "fd", "cs", "cl"]:
        rows = G[t][k]
        out += uv(len(rows))
        for r in rows:
            for v in r:
                if v is None:
                    out += b"\x00"
                elif isinstance(v, int):
                    out += v.to_bytes(8, "big", signed=True)
                else:
                    b = v.encode() if isinstance(v, str) else v
                    out += uv(len(b) + 1) + b
    return bytes(out)


SECT = {}


def mark(name, out, start):
    SECT.setdefault(name, []).append(bytes(out[start:]))


def enc_col(k, ref):
    # Header: format byte 0xB1, flags (bit 0 colref, bit 1 global names).
    out = bytearray([0xB1, (1 if ref else 0) | (2 if ref and NAMES else 0)])
    ol = sorted_ol(G["ol"][k])
    local = {r[0]: i for i, r in enumerate(ol)}
    base_num = len(obj_num)
    if ref:
        # Basis the references are relative to: objects and names numbered,
        # and bundles published, when the bundle was encoded.
        out += uv(base_num) + uv(len(name_num)) + uv(k)

    def idref(oid):
        # 0 = literal follows; 1+i = this bundle's object i; with ref,
        # 1+len(ol)+d = global object (base_num - 1 - d).
        if oid in local:
            return uv(1 + local[oid]), b""
        if ref and oid in obj_num:
            return uv(1 + len(ol) + (base_num - 1 - obj_num[oid])), b""
        return b"\x00", oid

    # object_location: pack dictionary, then columns.
    packs = sorted({r[3] for r in ol})
    pidx = {p: i for i, p in enumerate(packs)}
    out += uv(len(ol)) + uv(len(packs))
    prev = 0
    for p in packs:
        out += uv(p - prev)
        prev = p
    cols = [bytearray() for _ in range(5)]
    pg = pr = None
    for r in ol:
        oid, role, clen, pack, group, rec = r
        cols[0] += uv(pidx[pack])
        cols[1] += uv(group)
        # record numbers ascend within (pack, group): delta from previous
        cols[2] += uv(rec - pr if (pack, group) == pg and rec >= pr else rec)
        pg, pr = (pack, group), rec
        cols[3] += bytes([role]) + uv(clen)
        cols[4] += oid
    st = len(out)
    for c in cols[:4]:
        out += c
    mark('ol.cols', out, st)
    st = len(out)
    out += cols[4]
    mark('ol.ids', out, st)
    # inode_value: ordinal runs, then value columns.
    iv = sorted(G["iv"][k])
    out += uv(len(iv))
    runs = []
    for o, _ in iv:
        if runs and runs[-1][0] + runs[-1][1] == o:
            runs[-1][1] += 1
        else:
            runs.append([o, 1])
    out += uv(len(runs))
    prev = 0
    for s, n in runs:
        out += zz(s - prev) + uv(n)
        prev = s + n
    kinds, rcs, roots, lits, meta = bytearray(), bytearray(), bytearray(), bytearray(), bytearray()
    modes, mtimes, nanos = bytearray(), bytearray(), bytearray()
    pm = 0
    for _, v in iv:
        kinds.append(v[0])
        rcs += uv(int.from_bytes(v[1:9], "big"))
        r, lit = idref(v[9:41])
        roots += r
        lits += lit
        if v[0] & 0x80:
            mode = int.from_bytes(v[41:45], "big")
            ms = int.from_bytes(v[45:53], "big", signed=True)
            ns = int.from_bytes(v[53:57], "big")
            modes += uv(mode)
            mtimes += zz(ms - pm)
            nanos += uv(ns)
            pm = ms
            assert v[57:] == bytes(16)
        else:
            r2, lit2 = idref(v[41:73])
            meta += r2
            lits += lit2
    for name, part in [('iv.kinds', kinds), ('iv.refcount', rcs), ('iv.roots', roots), ('iv.mode', modes), ('iv.mtime', mtimes), ('iv.nanos', nanos), ('iv.mdroots', meta), ('iv.literals', lits)]:
        st = len(out); out += part; mark(name, out, st)
    # binding_hint: sorted keys, deltas; names: lengths then bytes.
    bh = G["bh"][k]
    if PARENT_MAJOR:
        bh = sorted(bh, key=lambda r: (r[1], r[0], r[2]))
    out += uv(len(bh))
    a, b_, lens, names = bytearray(), bytearray(), bytearray(), bytearray()
    pc = pp = 0
    for child, parent, name in bh:
        if PARENT_MAJOR:
            a += uv(parent - pp)
            b_ += zz(child - pc if parent == pp else child)
        else:
            a += uv(child - pc)
            b_ += zz(parent - pp)
        pc, pp = child, parent
        if ref and NAMES:
            n = name_num.get(name)
            if n is None:
                lens += b"\x00" + uv(len(name))
                names += name
                name_num[name] = len(name_num)
            else:
                lens += uv(1 + (len(name_num) - 1 - n))
        else:
            lens += uv(len(name))
            names += name
    for name, part in [('bh.child', a), ('bh.parent', b_), ('bh.lens', lens), ('bh.names', names)]:
        st = len(out); out += part; mark(name, out, st)
    # file_digest.
    fd = sorted(G["fd"][k])
    out += uv(len(fd))
    ds, ls, rs, rl = bytearray(), bytearray(), bytearray(), bytearray()
    for d, ln, root in fd:
        ds += d
        ls += uv(ln)
        r, lit = idref(root)
        rs += r
        rl += lit
    st = len(out); out += ls + rs + ds + rl; mark('fd', out, st)
    # content_signature.
    cs = sorted(G["cs"][k])
    out += uv(len(cs))
    pslot = 0
    for slot, stamp, oid, sig in cs:
        r, lit = idref(oid)
        out += zz(slot - pslot) + uv(stamp) + r + lit + sig
        pslot = slot
    # commit_lineage (the bundle's own Commit).
    out += uv(len(G["cl"][k]))
    for cid, run, depth, newest, merged in G["cl"][k]:
        if run == cid:
            out += b"\x00"
        elif ref and run in commit_num:
            out += uv(1 + (k - commit_num[run]))
        else:
            out += b"\x01" + run
        out += uv(depth) + uv(newest)
        if merged is None:
            out += b"\x00"
        elif ref and merged in commit_num:
            out += uv(1 + (k - commit_num[merged]))
        else:
            out += b"\x01" + merged
    for i, r in enumerate(ol):
        obj_num.setdefault(r[0], base_num + i)
    return bytes(out)


OUT = open(_os.environ["OUT"], "wb") if _os.environ.get("OUT") else None
if OUT and _os.environ.get("COMMITS"):
    with open(_os.environ["COMMITS"], "wb") as f:
        f.write(bytes(33))
        for c in commits:
            f.write(c)
t0 = time.time()
encoded = []
for k in range(nb):
    if encoding == "rows":
        e = enc_rows(k)
    else:
        e = enc_col(k, encoding == "colref")
    if k % mod == 0:
        encoded.append(e)
    if OUT:
        OUT.write(len(e).to_bytes(4, "little") + e)
t_enc = time.time() - t0
raw = sum(len(e) for e in encoded)
scale = nb / len(encoded)
import os
if os.environ.get("BREAKDOWN"):
    for name, parts in SECT.items():
        print(name, "raw", round(sum(map(len, parts))/1e6, 2), "z3", round(sum(len(zstd.compress(x, 3)) for x in parts if x)/1e6, 2))
print(f"encoding {encoding} bundles {nb} sampled {len(encoded)} raw {raw*scale/1e6:.1f} MB encode_s {t_enc:.1f}")
dictionary = None
for lv in levels:
    t = time.time()
    if lv.startswith("d"):
        if dictionary is None:
            samples = [e for i, e in enumerate(encoded) if i % 16 == 0 and len(e) > 8]
            dictionary = zstd.train_dict(samples, dict_kb * 1024)
            print(f"  dict trained on {len(samples)} samples, {len(dictionary.dict_content)} bytes")
            if _os.environ.get("DICT_OUT"):
                open(_os.environ["DICT_OUT"], "wb").write(dictionary.dict_content)
        level = int(lv[1:])
        cz = zstd.ZstdCompressor(level=level, zstd_dict=dictionary)
        size = sum(len(zstd.compress(e, level=level, zstd_dict=dictionary)) for e in encoded)
        size += len(dictionary.dict_content) / scale
    else:
        level = int(lv)
        size = sum(len(zstd.compress(e, level=level)) for e in encoded)
    print(f"  zstd {lv}: {size*scale/1e6:.1f} MB ({size/raw:.3f}) secs {time.time()-t:.1f}")
