#!/usr/bin/env python3
"""W2 -- the ring-size coverage sweep the census never ran.

The E2 census (squad-e/e2_content_index.py) measured exactly two points of the
product's own index: ring 1024 (41.8 %) and unbounded (84.3 %).  Nothing in
between was ever swept, and the product's SLOTS is a *declared bound* whose
value therefore had no evidence behind it.

This script reuses E2's own vectorised signature (checked against a literal
transcription of the Rust loop) and E2's own index model, and sweeps:

  * ring size      SLOTS      in {1024 .. 131072, unbounded}
  * reference bits REFERENCES in {13 (8192, E2's), 16 (65536, the product's), exact}
  * signature width           64-bit (the product's) vs low-32 (a proposed saving)

persist=True throughout: the whole question is what a *cross-save* index buys,
and the lifetime question is already settled (S1 - P0 = 11.7 % of the gain).

    python3 w2_index_sweep.py            # full sweep
    python3 w2_index_sweep.py --quick    # ring sweep at 16 reference bits only
"""
from __future__ import annotations

import json
import sys
import time
from collections import defaultdict
from pathlib import Path

import numpy as np

CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
HERE = Path(__file__).resolve().parent
OUT = HERE / "w2_index_sweep.json"

SMALL_FILE_THRESHOLD = 131_072
SELECTION = tuple(sorted(set(range(1, 158, 10)) | {157}))

WINDOW = 16
EMPTY = np.uint64(0xFFFF_FFFF_FFFF_FFFF)
MASK64 = (1 << 64) - 1

# ------------------------------------------------------------------ signature --


def _powers(base: int, size: int) -> np.ndarray:
    table = np.empty(size, dtype=np.uint64)
    value = 1
    for i in range(size):
        table[i] = np.uint64(value)
        value = (value * base) & MASK64
    return table


_INV257 = pow(257, -1, 1 << 64)
_POW257 = _powers(257, SMALL_FILE_THRESHOLD + 32)
_POW_INV = _powers(_INV257, SMALL_FILE_THRESHOLD + 32)


def _grow(need: int) -> None:
    global _POW257, _POW_INV
    if need <= _POW257.size:
        return
    size = max(need, _POW257.size * 2)
    _POW257 = _powers(257, size)
    _POW_INV = _powers(_INV257, size)


def _mix(value: np.ndarray) -> np.ndarray:
    value = (value ^ (value >> np.uint64(30))) * np.uint64(0xBF58_476D_1CE4_E5B9)
    value = (value ^ (value >> np.uint64(27))) * np.uint64(0x94D0_49BB_1331_11EB)
    return value ^ (value >> np.uint64(31))


def signature(raw: bytes) -> tuple[int, ...]:
    """The product's own 8-hash signature (E2's vectorised form)."""
    n = len(raw)
    if n < WINDOW:
        return ()
    _grow(n + WINDOW)
    data = np.frombuffer(raw, dtype=np.uint8).astype(np.uint64)
    c = data * _POW_INV[:n]
    prefix = np.zeros(n + 1, dtype=np.uint64)
    np.cumsum(c, out=prefix[1:])
    starts = np.arange(n - WINDOW + 1, dtype=np.int64)
    rolling = _POW257[starts + WINDOW - 1] * (prefix[starts + WINDOW] - prefix[starts])
    hashed = _mix(rolling)
    hashed = hashed[hashed != EMPTY]
    if hashed.size == 0:
        return ()
    return tuple(int(v) for v in np.unique(hashed)[:8])


def _mix_scalar(value: int) -> int:
    value = ((value ^ (value >> 30)) * 0xBF58_476D_1CE4_E5B9) & MASK64
    value = ((value ^ (value >> 27)) * 0x94D0_49BB_1331_11EB) & MASK64
    return value ^ (value >> 31)


def signature_reference(raw: bytes) -> tuple[int, ...]:
    """The literal Rust loop, for cross-checking the vectorised one."""
    if len(raw) < WINDOW:
        return ()
    high = pow(257, WINDOW - 1, 1 << 64)
    rolling = 0
    for byte in raw[:WINDOW]:
        rolling = (rolling * 257 + byte) & MASK64
    found = [MASK64] * 8
    for start in range(0, len(raw) - WINDOW + 1):
        if start != 0:
            rolling = ((rolling - raw[start - 1] * high) * 257 + raw[start + WINDOW - 1]) & MASK64
        h = _mix_scalar(rolling)
        if h < found[7] and h not in found:
            index = 0
            while index < 8 and found[index] < h:
                index += 1
            found[index + 1 :] = found[index:7]
            found[index] = h
    return tuple(value for value in found if value != MASK64)


# --------------------------------------------------------------------- corpus --


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


def build_population():
    manifest = json.loads((CORPUS / "checkpoint-manifest.json").read_bytes())
    cps = manifest["checkpoints"]
    shas = [cps[i - 1]["sha"] for i in SELECTION]
    states = [load_manifest(sha) for sha in shas]

    order: list[tuple[int, str, str, int]] = []
    for position, tree in enumerate(states):
        previous = states[position - 1] if position else {}
        changed = []
        for path, (mode, oid, size) in tree.items():
            old = previous.get(path)
            if old is None or old[1] != oid:
                changed.append((path, oid, size))
        changed.sort()
        for path, oid, size in changed:
            order.append((position, path, oid, size))

    whole = [row for row in order if 0 < row[3] < SMALL_FILE_THRESHOLD]
    print(
        f"whole-file occurrences {len(whole)}"
        f" distinct {len({row[2] for row in whole})}"
        f" repeat-occurrences {len(whole) - len({row[2] for row in whole})}"
    )

    started = time.monotonic()
    where: dict[str, Path] = {}
    for position in range(len(SELECTION)):
        first = 1 if position == 0 else SELECTION[position - 1] + 1
        last = SELECTION[position]
        for index in range(first, last + 1):
            directory = CORPUS / "inputs" / cps[index - 1]["sha"] / "blobs"
            for entry in directory.iterdir():
                where.setdefault(entry.name, entry)
    print(f"blob index {len(where)} in {time.monotonic() - started:.1f}s")

    started = time.monotonic()
    signatures: dict[str, tuple[int, ...]] = {}
    missing = 0
    for position, path, oid, size in whole:
        if oid in signatures:
            continue
        blob = where.get(oid)
        if blob is None:
            missing += 1
            signatures[oid] = ()
            continue
        signatures[oid] = signature(blob.read_bytes())
    print(f"signatures {len(signatures)} in {time.monotonic() - started:.1f}s, missing {missing}")
    return whole, signatures, missing


# ---------------------------------------------------------------------- model --


def run(whole, signatures, ring, reference_bits, truncate=None) -> dict:
    """One persistent index model, with candidates.rs's own admission rule.

    ring is SLOTS or None.  reference_bits is the width of the
    hash & (REFERENCES - 1) table (13 == the census value, 16 == today's
    product, None == exact).  truncate folds each hash to its low N bits before
    any comparison, which is what a narrower on-disk row would do.
    """
    def fold(h: int) -> int:
        return h if truncate is None else (h & ((1 << truncate) - 1))

    sig_of: dict[str, tuple[int, ...]] = {}
    for oid, sig in signatures.items():
        sig_of[oid] = tuple(sorted({fold(h) for h in sig}))

    by_hash: dict[int, list[int]] = defaultdict(list)
    references: dict[int, int] = {}
    entries: list[tuple[str, tuple[int, ...], int, str]] = []
    ring_ids: list[int] = []
    covered: dict[str, str] = {}
    covered_cross_save: set[str] = set()
    same_path_hits = 0
    cross_path_hits = 0
    admitted: set[str] = set()
    mask = None if reference_bits is None else (1 << reference_bits) - 1
    for position, path, oid, size in whole:
        fresh = oid not in admitted
        admitted.add(oid)
        sig = sig_of[oid]
        if len(sig) >= 2:
            reachable: list[tuple[tuple[str, tuple[int, ...], int, str], int]] = []
            if mask is None:
                seen: set[int] = set()
                for h in sig:
                    for index in by_hash.get(h, ()):
                        if index not in seen:
                            seen.add(index)
                            reachable.append((entries[index], index))
            else:
                for h in sig:
                    slot = references.get(h & mask)
                    if slot is None:
                        continue
                    if h in entries[slot][1]:
                        reachable.append((entries[slot], slot))
            best = None
            for entry, index in reachable:
                other_path, other_sig, other_position, other_oid = entry
                if other_oid == oid:
                    continue
                overlap = len(set(sig) & set(other_sig))
                if overlap >= 2 and (
                    best is None
                    or overlap > best[0]
                    or (overlap == best[0] and other_oid < best[2])
                ):
                    best = (overlap, index, other_oid)
            if best is not None and oid not in covered:
                _, index, other_oid = best
                covered[oid] = other_oid
                if entries[index][2] != position:
                    covered_cross_save.add(oid)
                if entries[index][0] == path:
                    same_path_hits += 1
                else:
                    cross_path_hits += 1
        if not fresh or oid in covered:
            continue
        if ring is not None and len(entries) >= ring:
            evicted = ring_ids.pop(0)
            for h in entries[evicted][1]:
                bucket = by_hash.get(h)
                if bucket and evicted in bucket:
                    bucket.remove(evicted)
                if mask is not None and references.get(h & mask) == evicted:
                    del references[h & mask]
        index = len(entries)
        entries.append((path, sig, position, oid))
        ring_ids.append(index)
        for h in sig:
            by_hash[h].append(index)
            if mask is not None:
                references[h & mask] = index
    return {
        "covered_objects": len(covered),
        "covered_cross_save_objects": len(covered_cross_save),
        "covered_canonical_bytes": sum(
            size + 23 for position, path, oid, size in whole if oid in covered
        ),
        "covered_same_path": same_path_hits,
        "covered_cross_path": cross_path_hits,
        "index_entries": len(entries),
    }


def main() -> int:
    quick = "--quick" in sys.argv
    if "--selfcheck" in sys.argv:
        ok = True
        for raw in (b"a" * 20, bytes(range(256)) * 3, b"hello world, hello layerfs!" * 40):
            a, b = signature(raw), signature_reference(raw)
            print(f"selfcheck len={len(raw):6d} vectorised={len(a)} literal={len(b)} equal={a == b}")
            ok &= a == b
        print("selfcheck: " + ("PASS" if ok else "FAIL"))
        return 0 if ok else 1

    whole, signatures, missing = build_population()
    total_bytes = sum(size + 23 for position, path, oid, size in whole)
    print(f"population canonical bytes {total_bytes}")

    rings = [1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, None]
    if quick:
        rings = [1024, 4096, 8192, 16384, 32768, 65536, None]

    rows: dict[str, dict] = {}
    print()
    print("== sweep A: ring x reference table, 64-bit signature, persistent ==")
    bit_choices = (13, 16, None) if not quick else (16,)
    for bits in bit_choices:
        for ring in rings:
            label = f"ring={ring if ring else 0:>9} refs={bits if bits else 0:>5}"
            started = time.monotonic()
            row = run(whole, signatures, ring, bits)
            row["ring"] = ring
            row["reference_bits"] = bits
            row["truncate"] = None
            rows[label] = row
            print(
                f"{label:<32} covered {row['covered_objects']:6d}"
                f" ({100.0 * row['covered_objects'] / len(whole):5.1f} %)"
                f" cross-save {row['covered_cross_save_objects']:6d}"
                f" canonical {row['covered_canonical_bytes']:12d}"
                f"  [{time.monotonic() - started:.1f}s]"
            )

    print()
    print("== sweep B: low-32 signature fold, persistent, refs=16 ==")
    for ring in rings:
        label = f"ring={ring if ring else 0:>9} refs=   16 fold=32"
        started = time.monotonic()
        row = run(whole, signatures, ring, 16, truncate=32)
        row["ring"] = ring
        row["reference_bits"] = 16
        row["truncate"] = 32
        rows[label] = row
        print(
            f"{label:<32} covered {row['covered_objects']:6d}"
            f" ({100.0 * row['covered_objects'] / len(whole):5.1f} %)"
            f" cross-save {row['covered_cross_save_objects']:6d}"
            f" canonical {row['covered_canonical_bytes']:12d}"
            f"  [{time.monotonic() - started:.1f}s]"
        )

    result = {
        "whole_file_occurrences": len(whole),
        "population_canonical_bytes": total_bytes,
        "signatures_missing": missing,
        "models": rows,
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True))
    print(f"\nwrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
