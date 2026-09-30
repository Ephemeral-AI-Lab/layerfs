#!/usr/bin/env python3
"""Generate/check spec-derived golden vectors and independently decode their closure."""
import argparse
import hashlib
import json
import struct
from pathlib import Path

from reference import (HERE, SPEC, Hashes, Reference, MapNode, SortedNode, envelope,
                       name_bytes, object_manifest, ordered, sorted_root,
                       sorted_update, u16, u64, unwrap)


def number(raw):
    return int.from_bytes(raw, "big")


def decode_sorted(ref, oid, root=True):
    raw = ref.objects[oid][0]
    value = unwrap(raw)
    if len(value) < 31 or number(value[8:10]) != 1 or value[12] != 0:
        raise ValueError("sorted header")
    forms = {b"LFS7PAR\0": ("parent", (14, 15)), b"LFS6INT\0": ("inode", (7, 8)),
             b"LFS6NSP\0": ("directory", (1, 2))}
    if value[:8] not in forms:
        raise ValueError("sorted magic")
    form, tags = forms[value[:8]]
    tag, level, count = value[10], value[11], number(value[13:15])
    if tag != tags[bool(level)]:
        raise ValueError("sorted role")
    cursor, entries, keys = 31, [], []

    def take(size):
        nonlocal cursor
        result = value[cursor:cursor + size]
        if len(result) != size:
            raise ValueError("sorted row EOF")
        cursor += size
        return result

    def name():
        size = number(take(2))
        if not 1 <= size <= 255:
            raise ValueError("sorted name length")
        return name_bytes(take(size))

    for _ in range(count):
        key = name() if form == "directory" else number(take(8))
        if form != "directory" and key == 0:
            raise ValueError("sorted serial")
        keys.append(key)
        if level:
            child = decode_sorted(ref, take(32), False)
            if child.form != form or child.level + 1 != level or child.upper != key:
                raise ValueError("sorted child correspondence")
            entries.append(child)
        elif form == "parent":
            kind, parent, component = number(take(1)), number(take(8)), name()
            if kind not in (2, 3) or parent == 0 or parent == key:
                raise ValueError("parent row")
            entries.append((key, kind, parent, component))
        elif form == "inode":
            entries.append((key, number(take(1)), number(take(8)), take(32), take(32)))
        else:
            serial = number(take(8))
            if serial == 0:
                raise ValueError("directory serial")
            entries.append((key, serial))
    if cursor != len(value) or any(a >= b for a, b in zip(keys, keys[1:])):
        raise ValueError("sorted EOF/order")
    node = SortedNode(form, level, tuple(entries))
    if not node.fits() or level > 31 or (not root and not node.filled()):
        raise ValueError("sorted fill/depth")
    if level and count < 2 or form == "inode" and count == 0:
        raise ValueError("sorted root collapse/empty inode")
    if number(value[15:23]) != node.count or number(value[23:31]) != node.row_bytes:
        raise ValueError("sorted summaries")
    if ref.encode_sorted(node, root) != oid:
        raise ValueError("sorted reference round-trip")
    return node


def decode_map(ref, oid, root=True):
    value = unwrap(ref.objects[oid][0])
    if len(value) < 31 or value[:10] != b"LFS4MAP\0\x00\x03" or value[12]:
        raise ValueError("mapping header")
    tag, level, count = value[10], value[11], number(value[13:15])
    width = 40 if tag == 8 else 48 if tag == 9 else 0
    if not width or len(value) != 31 + count * width or (tag == 8) != (level == 0):
        raise ValueError("mapping grammar")
    entries, size, extents = [], 0, 0
    for pos in range(31, len(value), width):
        row = value[pos:pos + width]
        if level == 0:
            child, offset, length = row[:32], number(row[32:36]), number(row[36:40])
            payload = unwrap(ref.objects[child][0])
            if not payload.startswith(b"LFS4CHK\0") or not length or offset + length > len(payload) - 8:
                raise ValueError("mapping payload coverage")
            entries.append((child, offset, length))
        else:
            child = decode_map(ref, row[16:48], False)
            if child.level + 1 != level:
                raise ValueError("mapping child level")
            size, extents = size + child.length, extents + child.count
            if (number(row[:8]), number(row[8:16])) != (size, extents):
                raise ValueError("mapping cumulative summaries")
            entries.append(child)
    node = MapNode(level, tuple(entries))
    if (number(value[15:23]), number(value[23:31])) != (node.length, node.count):
        raise ValueError("mapping summaries")
    if ref.encode_map(node, root) != oid:
        raise ValueError("mapping round-trip")
    return node


def leaves(node):
    return list(node.entries) if not node.level else [row for child in node.entries for row in leaves(child)]


def decode_state(ref, oid):
    value = unwrap(ref.objects[oid][0])
    if value.startswith(b"LFS5SML\0"):
        if value[8:10] != b"\0\x01" or not 0 < len(value) - 10 < 131072:
            raise ValueError("whole file")
        return value[10:]
    v2 = value[:10] == b"LFS7FST\0\0\x02"
    if not v2 and value[:10] != b"LFS4MAP\0\0\x03":
        raise ValueError("state magic/version")
    if len(value) != (125 if v2 else 93) or value[10:12] != b"\x0a\0":
        raise ValueError("state width/role/flags")
    if value[29:61] != ref.mapping_profile or (v2 and value[61:93] != ref.edit_policy):
        raise ValueError("state profile/policy")
    tree = decode_map(ref, value[-32:])
    if (number(value[12:20]), number(value[20:28]), value[28]) != (tree.length, tree.count, tree.level):
        raise ValueError("state summaries")
    return ref.read_map(tree)


def certify_filesystem(ref, oid):
    value = unwrap(ref.objects[oid][0])
    version = number(value[8:10])
    if value[:8] != b"LFS6FSR\0" or version not in (1, 2):
        raise ValueError("filesystem magic/version")
    if len(value) != (116 if version == 1 else 148) or value[10:12] != b"\x06\0":
        raise ValueError("filesystem width/role/flags")
    if value[12:44] != (ref.namespace_v1 if version == 1 else ref.namespace_v2):
        raise ValueError("filesystem profile")
    root = number(value[76:84])
    inodes = {row[0]: row for row in leaves(decode_sorted(ref, value[84:116]))}
    parents = {} if version == 1 else {row[0]: row for row in leaves(decode_sorted(ref, value[116:148]))}
    actual, counts = {}, {serial: 0 for serial in inodes}
    for serial, (_, kind, _, content, _) in inodes.items():
        if kind == 1:
            decode_state(ref, content)
        elif kind == 3:
            target = unwrap(ref.objects[content][0])
            if target[:12] != b"LFS4LNK\0\0\x01\x05\0" or len(target) < 14:
                raise ValueError("symlink grammar")
            length = number(target[12:14])
            if length > 4096 or len(target) != 14 + length or b"\0" in target[14:]:
                raise ValueError("symlink length/target")
        if kind != 2:
            continue
        for name, target in leaves(decode_sorted(ref, content)):
            if target not in inodes or target == root:
                raise ValueError("namespace missing/root binding")
            counts[target] += 1
            if inodes[target][1] != 1:
                if target in actual:
                    raise ValueError("namespace alias")
                actual[target] = (target, inodes[target][1], serial, name)
    if version == 2 and parents != actual:
        raise ValueError("namespace parent correspondence")
    for serial, (_, kind, count, _, _) in inodes.items():
        if count != counts[serial] or (serial == root and (kind != 2 or count != 0)):
            raise ValueError("namespace counts")
        if serial != root and (count < 1 or kind != 1 and count != 1):
            raise ValueError("namespace orphan/alias")
        if kind != 1 and serial != root:
            at, seen = serial, set()
            while at != root:
                if at in seen or at not in actual:
                    raise ValueError("namespace cycle/reachability")
                seen.add(at)
                at = actual[at][2]


def must_refuse(body):
    try:
        body()
    except (ValueError, UnicodeError, KeyError, struct.error):
        return "REFUSE"
    raise AssertionError("reference accepted a declared rejection vector")


def patterned(size):
    return bytes((i * 37 + (i >> 7) * 11) % 251 for i in range(size))


def vector_set(hashes):
    ref = Reference(hashes)
    pins = SPEC["v1_pins"]
    assert ref.cdc_profile.hex() == pins["cdc_profile"]
    assert ref.mapping_profile.hex() == pins["mapping_profile"]
    assert hashes.object(b"layerfs-v2-object-domain-probe").hex() == "2465a95cc821c538feddb27829458e640c9d7114cf4ffce6ee43000030006606"
    empty = MapNode(0, ())
    assert ref.encode_map(empty).hex() == pins["empty_leaf"]
    assert ref.state(empty, 1).hex() == pins["empty_state"]
    assert ref.complete(b"layerfs-stage02-fixture", 1)[0].hex() == pins["small_fixture"]
    assert ref.save(envelope(b"LFS4CHK\0" + b"\x5a" * 8192), 2).hex() == pins["chunk_fixture"]
    repo = HERE.parents[4]
    for fixture in SPEC["v1_retained_codec_fixtures"]:
        raw = (repo / fixture["path"]).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == fixture["sha256"]
        assert hashes.object(raw).hex() == fixture["expected_object_id"]
    result = {"schema": "layerfs-issue287-independent-vectors/v1",
              "contract_sha256": hashlib.sha256((HERE / "contract.json").read_bytes()).hexdigest(),
              "method_sha256": {p: hashlib.sha256((HERE / p).read_bytes()).hexdigest()
                                for p in ("reference.py", "verify.py", "hash/src/main.rs", "hash/Cargo.toml", "hash/Cargo.lock")},
              "profiles": {"cdc": ref.cdc_profile.hex(), "mapping_v3": ref.mapping_profile.hex(),
                           "edit_policy_v2": ref.edit_policy.hex(), "namespace_v1": ref.namespace_v1.hex(),
                           "namespace_v2": ref.namespace_v2.hex()},
              "v1_fixed_pins": pins, "v1_retained_codec_fixtures": SPEC["v1_retained_codec_fixtures"],
              "files": [], "namespaces": [], "parent_transitions": [],
              "rejections": []}
    for size in (0, 1, 131071, 131072, 131073):
        raw = patterned(size)
        root, tree = ref.complete(raw)
        assert decode_state(ref, root) == raw
        result["files"].append({"name": f"complete-{size}", "input": {"pattern": "(i*37+(i>>7)*11)%251", "bytes": size},
                                "root": root.hex(), "bytes_sha256": hashlib.sha256(raw).hexdigest(),
                                "extent_count": tree.count if tree else None,
                                "chunk_lengths": [len(x) for x in ref.cdc(raw)] if tree else []})
    base = patterned(262144)
    base_root, base_tree = ref.complete(base, 1)
    first = [(12345, 12349, b"R0-EDIT")]
    second = [(80000, 80000, b"successor"), (100009, 100013, b"\0" * 4)]
    for name, parent_root, parent_tree, parent_bytes, edits in (
        ("v2-from-exact-v1-parent", base_root, base_tree, base, first),
        ("v2-noop-preserves-v1-root", base_root, base_tree, base, [(17, 21, base[17:21])]),
    ):
        root, tree, raw = ref.edited(parent_root, parent_tree, parent_bytes, edits)
        assert decode_state(ref, root) == raw
        result["files"].append({"name": name, "parent_root": parent_root.hex(), "root": root.hex(),
                                "edits": [[a, b, c.hex()] for a, b, c in edits],
                                "bytes_sha256": hashlib.sha256(raw).hexdigest(), "extent_count": tree.count})
    parent, tree, raw = ref.edited(base_root, base_tree, base, first)
    root, tree, raw = ref.edited(parent, tree, raw, second)
    assert decode_state(ref, root) == raw
    result["files"].append({"name": "v2-successive-exact-parent", "parent_root": parent.hex(), "root": root.hex(),
                            "edits": [[a, b, c.hex()] for a, b, c in second],
                            "bytes_sha256": hashlib.sha256(raw).hexdigest(), "extent_count": tree.count})
    metadata = ref.save(envelope(b"LFS4MET\0" + u16(1) + bytes([9, 0, 0]) + u16(0) + u64(0) + u64(0)), 11)
    file_content = ref.complete(b"regular bytes")[0]
    symlink = ref.save(envelope(b"LFS4LNK\0" + u16(1) + bytes([5, 0]) + u16(6) + b"target"), 13)
    for count in (0, 1, 29, 30):
        records = {1: (2, None, metadata), **{i + 2: (2, None, metadata) for i in range(count)}}
        bindings = [(1, f"d{i:04}" + "x" * 250, i + 2) for i in range(count)]
        fs = ref.filesystem(records, bindings)
        certify_filesystem(ref, bytes.fromhex(fs["root"]))
        result["namespaces"].append({"name": f"max-name-dirs-{count}", "input": {"child_directories": count, "name_recipe": "d{index:04}+x*250", "seed": "00..1f"}, **fs})
        if count == 0:
            old = ref.filesystem(records, bindings, version=1)
            certify_filesystem(ref, bytes.fromhex(old["root"]))
            result["namespaces"].append({"name": "v1-empty-independent-reference", **old})
    records = {1: (2, None, metadata), 2: (2, None, metadata), 3: (1, file_content, metadata), 4: (3, symlink, metadata)}
    fs = ref.filesystem(records, [(1, "d", 2), (1, "f", 3), (2, "alias", 3), (2, "s", 4)])
    certify_filesystem(ref, bytes.fromhex(fs["root"]))
    result["namespaces"].append({"name": "directory-symlink-regular-alias", **fs})
    valid_root = bytes.fromhex(fs["root"])
    original = unwrap(ref.objects[valid_root][0])
    empty_parent = ref.encode_sorted(sorted_root("parent", []))
    forged_pair = ref.save(envelope(original[:116] + empty_parent), 10)
    result["rejections"].append({"name": "authenticated-mismatched-inode-parent-pair",
                                  "root": forged_pair.hex(),
                                  "expected": must_refuse(lambda: certify_filesystem(ref, forged_pair))})
    rows = [(i + 2, 2, 1, name_bytes(f"d{i:04}" + "x" * 250)) for i in range(30)]
    before = sorted_root("parent", rows)
    after = sorted_update(before, [(i + 2, None) for i in range(19)])
    before_id, after_id = ref.encode_sorted(before), ref.encode_sorted(after)
    assert decode_sorted(ref, after_id) == after and after.level == 0 and after.count == 11
    result["parent_transitions"].append({"name": "30-to-11-delete-root-collapse", "before": before_id.hex(),
                                         "after": after_id.hex(), "removed_serials": list(range(2, 21)),
                                         "before_level": before.level, "after_level": after.level})
    for count in (11, 12, 29, 30):
        node = SortedNode("parent", 0, tuple(rows[:count]))
        verdict = "ACCEPT" if 3277 <= node.size <= 8192 else "REFUSE"
        if verdict == "REFUSE":
            must_refuse(lambda: ref.encode_sorted(node, False))
        else:
            oid = ref.encode_sorted(node, False)
            assert decode_sorted(ref, oid, False) == node
        result["parent_transitions"].append({"name": f"nonroot-max-name-leaf-{count}", "canonical_bytes": node.size,
                                             "expected": verdict})
    children = []
    for i in range(204):
        leaf = [(2 + i * 12 + j, 2, 1, name_bytes(f"n{i:04}-{j:02}" + "x" * 247)) for j in range(12)]
        children.append(SortedNode("parent", 0, tuple(leaf)))
    for count in (80, 81, 203, 204):
        node = SortedNode("parent", 1, tuple(children[:count]))
        verdict = "ACCEPT" if 3277 <= node.size <= 8192 else "REFUSE"
        root_id = None
        if verdict == "REFUSE":
            must_refuse(lambda: ref.encode_sorted(node, False))
        else:
            root_id = ref.encode_sorted(node, False)
            assert decode_sorted(ref, root_id, False) == node
        result["parent_transitions"].append({"name": f"nonroot-parent-branch-{count}", "canonical_bytes": node.size,
                                             "root": root_id.hex() if root_id else None, "expected": verdict,
                                             "child_leaf_recipe": "12 rows: serial=2+i*12+j; kind=2; parent=1; name=n{i:04}-{j:02}+x*247"})
    bad_cases = [
        ("envelope-trailing", lambda: unwrap(envelope(b"x") + b"\0")),
        ("envelope-short", lambda: unwrap(envelope(b"x")[:-1])),
        ("parent-name-256", lambda: ref.encode_sorted(SortedNode("parent", 0, ((2, 2, 1, b"x" * 256),)))),
        ("parent-regular-kind", lambda: ref.encode_sorted(SortedNode("parent", 0, ((2, 1, 1, b"x"),)))),
        ("parent-self", lambda: ref.encode_sorted(SortedNode("parent", 0, ((2, 2, 2, b"x"),)))),
        ("parent-duplicate", lambda: ref.encode_sorted(SortedNode("parent", 0, ((2, 2, 1, b"a"), (2, 2, 1, b"b"))))),
        ("namespace-nonfile-alias", lambda: ref.filesystem({1: (2, None, metadata), 2: (2, None, metadata)}, [(1, "a", 2), (1, "b", 2)])),
        ("namespace-disconnected-cycle", lambda: ref.filesystem({1: (2, None, metadata), 2: (2, None, metadata), 3: (2, None, metadata)}, [(2, "a", 3), (3, "b", 2)])),
        ("namespace-orphan", lambda: ref.filesystem({1: (2, None, metadata), 2: (1, file_content, metadata)}, [])),
    ]
    for name, body in bad_cases:
        result["rejections"].append({"name": name, "expected": must_refuse(body)})
    result["objects"] = object_manifest(ref)
    result["coverage"] = {"canonical_codec_and_finite_reference": "PASS",
                          "candidate_v2_product_comparison": "NOT_RUN: R3 product does not exist at R0",
                          "resource_liveness_provider_or_speed_claim": "NOT_RUN: separate owning gates",
                          "large_edit_partition_matrix": "NOT_RUN: method implemented; wider prospective R3 vectors remain required"}
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", type=Path, help="write a new golden ledger; refuses an existing path")
    parser.add_argument("--check", action="store_true", help="compare immutable vectors.json")
    args = parser.parse_args()
    if bool(args.write) == bool(args.check):
        parser.error("choose exactly one of --write or --check")
    hashes = Hashes()
    try:
        result = vector_set(hashes)
    finally:
        hashes.close()
    text = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.write:
        with args.write.open("x") as dest:
            dest.write(text)
    else:
        assert json.loads((HERE / "vectors.json").read_text()) == result, "frozen vector ledger changed"
    print(json.dumps({"status": "PASS", "source": SPEC["baseline_commit"], "files": len(result["files"]),
                      "namespaces": len(result["namespaces"]), "parent_transitions": len(result["parent_transitions"]),
                      "rejections": len(result["rejections"]), "objects": len(result["objects"]),
                      "claim": "independent reference identities and internal decode/certification only"}, sort_keys=True))


if __name__ == "__main__":
    main()
