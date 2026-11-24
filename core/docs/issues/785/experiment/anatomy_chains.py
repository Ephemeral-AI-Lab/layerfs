import sqlite3, sys, signal, collections
signal.alarm(110)
db, edges = sys.argv[1], sys.argv[2]
con = sqlite3.connect(f"file:{db}?immutable=1", uri=True)
role_of = {oid: role for oid, role in con.execute("SELECT object_id, role FROM object_location")}
data = open(edges, "rb").read()
base = {}
erole = {}
for i in range(0, len(data), 65):
    o, b, r = data[i:i+32], data[i+32:i+64], data[i+64]
    base[o] = b; erole[o] = r
missing = sum(1 for b in base.values() if b not in role_of)
cross = sum(1 for o, b in base.items() if role_of.get(b) != erole[o])
depth = {}
def d(o):
    stack = []
    while o in base and o not in depth:
        stack.append(o); o = base[o]
    k = depth.get(o, 0)
    for x in reversed(stack):
        k += 1; depth[x] = k
    return depth.get(stack[0], 0) if stack else k
for o in base: d(o)
names = {1:"WholeFile",2:"Chunk",3:"ExtentLeaf",4:"ExtentBranch",6:"InodeLeaf",7:"DirectoryLeaf",8:"DirectoryBranch",9:"InodeBranch"}
by = collections.defaultdict(collections.Counter)
for o, k in depth.items(): by[names.get(erole[o], erole[o])][k] += 1
fan = collections.Counter(base.values())
print("edges", len(base), "missing_base", missing, "cross_role", cross)
for n, c in by.items():
    tot = sum(c.values()); mean = sum(k*v for k, v in c.items())/tot
    print(n, "n", tot, "mean", round(mean, 2), "max", max(c), "hist", sorted(c.items())[:20])
print("bases", len(fan), "max fan-in", fan.most_common(1)[0][1], "bases with >=10 dependants", sum(1 for v in fan.values() if v >= 10))
