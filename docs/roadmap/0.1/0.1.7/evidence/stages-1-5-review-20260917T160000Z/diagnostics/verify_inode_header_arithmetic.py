"""Independent byte-level check of the Stage 5 inode page header arithmetic.

Reads the SEALED reference fixtures (never written by this script) and verifies
the recorded subtree byte total against the row width the source declares.
Header value layout (sorted/format.rs:658-665, 681-693):
  magic[0:8] version[8:10] role[10] level[11] flags[12] count u16[13:15]
  subtree_count u64[15:23] subtree_bytes u64[23:31]
"""
import struct, sys, pathlib

FIXTURES = pathlib.Path(sys.argv[1])
LEAF_ROW_BYTES = 81   # object/inode_leaf.rs:32 (8-byte serial + 73-byte value)
INODE_VALUE_BYTES = 73  # object/inode_leaf.rs:24

def header(raw):
    start = raw.find(b"LFS6INT\x00")
    assert start >= 0, "inode magic missing"
    value = raw[start:]
    version = struct.unpack(">H", value[8:10])[0]
    role, level, flags = value[10], value[11], value[12]
    count = struct.unpack(">H", value[13:15])[0]
    subtree_count = struct.unpack(">Q", value[15:23])[0]
    subtree_bytes = struct.unpack(">Q", value[23:31])[0]
    return dict(envelope=start, version=version, role=role, level=level,
                flags=flags, count=count, subtree_count=subtree_count,
                subtree_bytes=subtree_bytes, total=len(raw))

print(f"LEAF_ROW_BYTES={LEAF_ROW_BYTES} INODE_VALUE_BYTES={INODE_VALUE_BYTES} "
      f"row=serial8+value{INODE_VALUE_BYTES}")
for name in ("codec-inode-leaf", "codec-inode-branch"):
    raw = (FIXTURES / f"{name}.bin").read_bytes()
    h = header(raw)
    if h["level"] == 0:
        expected_bytes = h["count"] * LEAF_ROW_BYTES
        expected_count = h["count"]
    else:
        expected_bytes = h["subtree_count"] * LEAF_ROW_BYTES
        expected_count = h["subtree_count"]
    ok_b = h["subtree_bytes"] == expected_bytes
    ok_c = h["subtree_count"] == expected_count
    print(f"{name}: {h}")
    print(f"  subtree_bytes={h['subtree_bytes']} expected=rows*{LEAF_ROW_BYTES}={expected_bytes} -> {'MATCH' if ok_b else 'MISMATCH'}")
    print(f"  rows*73 (the WRONG width) would be {expected_count * INODE_VALUE_BYTES} -> "
          f"{'DIFFERENT, so the two widths are distinguishable' if expected_count * INODE_VALUE_BYTES != expected_bytes else 'INDISTINGUISHABLE'}")
    assert ok_b and ok_c, name
print("ALL MATCH")
