"""Independent storage accounting from the smoke run's fresh output directories."""
import os, sqlite3, sys, glob

for root in sys.argv[1:]:
    store = os.path.join(root, "store.sqlite")
    if not os.path.exists(store):
        print(f"{root}: no store.sqlite"); continue
    size = os.path.getsize(store)
    c = sqlite3.connect(f"file:{store}?mode=ro", uri=True)
    packs = c.execute("SELECT COUNT(*), COALESCE(SUM(length(data)),0) FROM object_packs").fetchone()
    objs = c.execute("SELECT COUNT(*), COALESCE(SUM(canonical_length),0) FROM objects").fetchone()
    groups = c.execute("SELECT COUNT(*), COALESCE(SUM(count),0) FROM metadata_value_groups").fetchone()
    bases = c.execute("SELECT COUNT(*) FROM objects WHERE base_object_id IS NOT NULL").fetchone()[0]
    roles = c.execute("SELECT object_role, COUNT(*) FROM objects GROUP BY object_role ORDER BY object_role").fetchall()
    c.close()
    print(f"== {root}")
    print(f"   sqlite_file_bytes={size}")
    print(f"   packs={packs[0]} pack_payload_bytes={packs[1]}")
    print(f"   object_rows={objs[0]} canonical_bytes_sum={objs[1]} delta_based_rows={bases}")
    print(f"   value_groups={groups[0]} pooled_values={groups[1]}")
    print(f"   roles={roles}")
    if packs[1]:
        print(f"   pack_payload/canonical = {packs[1]/max(objs[1],1):.4f}; "
              f"sqlite_file/canonical = {size/max(objs[1],1):.4f}; "
              f"framing_overhead_bytes = {packs[1]-objs[1]}")
