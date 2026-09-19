#!/usr/bin/env python3
"""E2 -- what input does each storage win need?  Corpus-side correspondence census.

Reads only the corpus (manifests + oracles).  No product source, no Store, no
codec: the question "would object O reach a base under correspondence K?" is a
question about (path, state, content), not about bytes.

    python3 e2_correspondence.py

Writes e2_correspondence.json beside itself.
"""
from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path

CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
OUT = Path(__file__).resolve().parent / "e2_correspondence.json"

SMALL_FILE_THRESHOLD = 131_072  # layerfs-content/src/policy.rs DEFAULT_SMALL_FILE_THRESHOLD_BYTES

SELECTION = tuple(sorted(set(range(1, 158, 10)) | {157}))


def load_manifest(sha: str) -> dict[str, tuple[str, str, int]]:
    tree: dict[str, tuple[str, str, int]] = {}
    raw = (CORPUS / "inputs" / sha / "manifest.tsv").read_text()
    for line in raw.splitlines():
        if not line:
            continue
        mode, oid, size, hexpath = line.split("\t")
        tree[bytes.fromhex(hexpath).decode("utf-8", "surrogateescape")] = (
            mode,
            oid,
            int(size),
        )
    return tree


def main() -> int:
    manifest = json.loads((CORPUS / "checkpoint-manifest.json").read_bytes())
    cps = manifest["checkpoints"]
    shas = [cps[i - 1]["sha"] for i in SELECTION]
    states = [load_manifest(sha) for sha in shas]
    print(f"states {len(states)}: " + " ".join(str(i) for i in SELECTION))

    def representation(size: int) -> str:
        if size == 0:
            return "Empty"
        if size < SMALL_FILE_THRESHOLD:
            return "WholeFile"
        return "Chunked"

    # --- population, exactly Corpus::transition's semantics -------------------
    # occurrences in construction order: (state_position, path, oid, size, kind)
    occurrences: list[tuple[int, str, str, int, str]] = []
    for position, tree in enumerate(states):
        previous = states[position - 1] if position else {}
        changed: list[tuple[str, str, str, int, str]] = []
        for path, (mode, oid, size) in tree.items():
            old = previous.get(path)
            if old is None:
                changed.append((path, mode, oid, size, "Added"))
            elif old[1] != oid:
                changed.append((path, mode, oid, size, "Modified"))
            elif old[0] != mode:
                changed.append((path, mode, oid, size, "MetadataOnly"))
        changed.sort(key=lambda row: row[0])
        for path, mode, oid, size, kind in changed:
            occurrences.append((position, path, oid, size, kind))

    constructed = [o for o in occurrences if o[4] != "MetadataOnly"]
    union = {o[2] for o in constructed}
    union_bytes = 0
    seen: dict[str, int] = {}
    for _, _, oid, size, _ in constructed:
        if oid not in seen:
            seen[oid] = size
            union_bytes += size
    print(
        f"union oids {len(union)} bytes {union_bytes}"
        f"   [pin 44240 / 371937306]"
    )

    # --- the whole-file population -------------------------------------------
    whole_occs = [o for o in constructed if representation(o[3]) == "WholeFile"]
    wf_objects: dict[str, int] = {}
    for _, _, oid, size, _ in whole_occs:
        wf_objects.setdefault(oid, size)
    wf_canonical = sum(size + 23 for size in wf_objects.values())
    print(
        f"whole-file objects {len(wf_objects)} content {sum(wf_objects.values())}"
        f" canonical(+23) {wf_canonical}"
        f"   [B2 pin 44148 / 347445305 / 348460709]"
    )
    chunk_objects = {
        o[2] for o in constructed if representation(o[3]) == "Chunked"
    }
    print(f"chunked file roots {len(chunk_objects)}")
    print(f"empty file occurrences {sum(1 for o in constructed if o[3] == 0)}")

    # --- correspondence census, per whole-file OBJECT -------------------------
    # (a) caller-held per-path map, as the harness built it: the base offered for
    #     state k's version of p is p's content root in the PREVIOUS SELECTED
    #     state.  It exists iff p is in tree(k-1); it is ELIGIBLE only if that
    #     version is also a WholeFile object (role must match).
    # (c) per-path version chain: any earlier selected state's version of p.
    def earlier_versions(position: int, path: str, oid: str, any_earlier: bool):
        found = []
        positions = range(position - 1, -1, -1) if any_earlier else range(position - 1, position - 2, -1)
        for j in positions:
            if j < 0:
                break
            entry = states[j].get(path)
            if entry is None:
                if not any_earlier:
                    break
                continue
            if entry[1] != oid:
                found.append((j, entry[1], entry[2]))
        return found

    a_cover: dict[str, tuple[int, str]] = {}       # immediate previous selected state
    c_cover: dict[str, tuple[int, str]] = {}       # any earlier version
    a_any: dict[str, int] = {}                     # a version exists at all (any role)
    c_any: dict[str, int] = {}
    first_state: dict[str, int] = {}

    for position, path, oid, size, _ in whole_occs:
        first_state.setdefault(oid, position)
        prev = earlier_versions(position, path, oid, any_earlier=False)
        if prev:
            a_any.setdefault(oid, prev[0][0])
            j, poid, psize = prev[0]
            if representation(psize) == "WholeFile":
                a_cover.setdefault(oid, (j, poid))
        if earlier_versions(position, path, oid, any_earlier=True):
            c_any.setdefault(oid, position)
        for j, poid, psize in earlier_versions(position, path, oid, any_earlier=True):
            if representation(psize) == "WholeFile":
                c_cover.setdefault(oid, (j, poid))
                break

    def census(name: str, cover: dict, anyv: dict) -> dict:
        row = {
            "objects_with_a_version_at_all": len(anyv),
            "objects_reaching_an_ELIGIBLE_base": len(cover),
            "canonical_bytes_covered": sum(wf_objects[o] + 23 for o in cover),
        }
        print(
            f"{name:<34} version-at-all {row['objects_with_a_version_at_all']:6d}"
            f" | eligible base {row['objects_reaching_an_ELIGIBLE_base']:6d}"
            f" | canonical {row['canonical_bytes_covered']:12d}"
        )
        return row

    print()
    a_row = census("(a) per-path map (prev state)", a_cover, a_any)
    c_row = census("(c) per-path chain (any earlier)", c_cover, c_any)

    # how much the ROLE match costs: a version exists but is Chunked
    print(
        f"    role-mismatch loss (a): {len(a_any) - len(a_cover)} objects;"
        f" (c): {len(c_any) - len(c_cover)} objects"
    )

    # --- first-state distribution --------------------------------------------
    dist: dict[int, int] = defaultdict(int)
    for oid in wf_objects:
        dist[first_state[oid]] += 1
    print("\nwhole-file objects by first constructed state:")
    print("   " + " ".join(f"s{SELECTION[k]}={v}" for k, v in sorted(dist.items())))

    result = {
        "selection": list(SELECTION),
        "union_oids": len(union),
        "union_bytes": union_bytes,
        "whole_file_objects": len(wf_objects),
        "whole_file_content_bytes": sum(wf_objects.values()),
        "whole_file_canonical_bytes": wf_canonical,
        "chunk_file_roots": len(chunk_objects),
        "a_per_path_map": a_row,
        "c_per_path_chain": c_row,
        "first_state_histogram": {str(SELECTION[k]): v for k, v in sorted(dist.items())},
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True))
    print(f"\nwrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
