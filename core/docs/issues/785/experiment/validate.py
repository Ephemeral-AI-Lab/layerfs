import sys, random, struct, signal
sys.path.insert(0, sys.argv[1]); signal.alarm(110)
import lfs
db = sys.argv[2]
con = lfs.connect(db)
loc = lfs.load_payload_locations(con)
base = lfs.load_bases(sys.argv[3])
P = lfs.Payloads(con, loc, base)
rows = con.execute("SELECT object_id, signature FROM content_signature").fetchall()
random.seed(7); rows = random.sample(rows, 200)
ok = okpy = 0
pays = [P.payload(o) for o, _ in rows]
sigs = lfs.signatures_np(pays)
for (o, stored), s, p in zip(rows, sigs, pays):
    folded = [struct.unpack_from("<I", stored, 4*i)[0] for i in range(8)]
    mine = [x & 0xffffffff for x in s] + [0xffffffff]*(8-len(s))
    ok += mine == folded
for p, s in list(zip(pays, sigs))[:15]:
    okpy += lfs.signature_py(p) == s
print("whole-file vs product's stored signatures:", ok, "/ 200 match")
print("numpy vs reference python:", okpy, "/ 15 match")
chunks = random.sample([o for o, v in loc.items() if v[3]==2], 10)
cp = [P.payload(o) for o in chunks]
print("chunk numpy vs python:", sum(lfs.signature_py(p)==s for p, s in zip(cp, lfs.signatures_np(cp))), "/ 10")
