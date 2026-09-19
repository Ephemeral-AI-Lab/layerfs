#!/usr/bin/env python3
"""E2 part 3 -- does a CONTENT-KEYED index reach a base without any path key?

Reimplements, byte-for-byte, the product's own candidate index
(core/crates/layerfs-storage/src/encoding/delta/candidates.rs):

  * signature(raw)  : 16-byte window, 257 rolling hash, mix(), the 8 smallest
                      distinct values, EMPTY = u64::MAX for unused slots
  * find()          : a candidate is accepted iff it shares >= 2 of the 8 hashes;
                      highest overlap wins, ties break on the smaller id

The only thing changed is the LIFETIME: the product drops the index at the end of
every save (cas/lifecycle.rs:90, cas/store.rs:392).  Here it persists, and it is
keyed by the content signature alone -- no path, no branch, no commit, no history.

Two capacity models are reported:
  * unbounded  : every previously stored whole-file object is indexed;
  * ring1024   : the product's own SLOTS = 1024, most recent wins.

    python3 e2_content_index.py
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
OUT = HERE / "e2_content_index.json"

SMALL_FILE_THRESHOLD = 131_072
SELECTION = tuple(sorted(set(range(1, 158, 10)) | {157}))

WINDOW = 16
SLOTS = 1024
EMPTY = np.uint64(0xFFFF_FFFF_FFFF_FFFF)

# ---------------------------------------------------------------- signature ---

_INV257 = pow(257, -1, 1 << 64)


def _powers(base: int, size: int) -> np.ndarray:
    """base**i mod 2**64 for i in 0..size, built with modular wraparound."""
    table = np.empty(size, dtype=np.uint64)
    value = 1
    for i in range(size):
        table[i] = np.uint64(value)
        value = (value * base) & 0xFFFF_FFFF_FFFF_FFFF
    return table


_POW257 = _powers(257, SMALL_FILE_THRESHOLD + 32)
_POW_INV = _powers(_INV257, SMALL_FILE_THRESHOLD + 32)


def _grow(need: int) -> None:
    """Extends both power tables so any payload length is representable."""
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
    """The product's own 8-hash signature, for one payload."""
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


def signature_reference(raw: bytes) -> tuple[int, ...]:
    """The literal Rust algorithm, for cross-checking the vectorised one."""
    if len(raw) < WINDOW:
        return ()
    high = pow(257, WINDOW - 1, 1 << 64)
    mask = (1 << 64) - 1
    rolling = 0
    for byte in raw[:WINDOW]:
        rolling = (rolling * 257 + byte) & mask
    # The Rust code keeps a sorted 8-entry array and inserts only a hash strictly
    # below the current maximum; this mirrors it exactly, without the quadratic
    # membership scan.
    found = [(1 << 64) - 1] * 8
    for start in range(0, len(raw) - WINDOW + 1):
        if start != 0:
            rolling = (
                (rolling - raw[start - 1] * high) * 257 + raw[start + WINDOW - 1]
            ) & mask
        h = _mix_scalar(rolling)
        if h < found[7] and h not in found:
            index = 0
            while index < 8 and found[index] < h:
                index += 1
            found[index + 1 :] = found[index:7]
            found[index] = h
    # The Rust array is padded to eight with EMPTY, and `find()` filters EMPTY out
    # of both the lookup and the overlap count, so the real hash set is what is
    # compared here.
    return tuple(value for value in found if value != (1 << 64) - 1)


def _mix_scalar(value: int) -> int:
    mask = (1 << 64) - 1
    value = ((value ^ (value >> 30)) * 0xBF58_476D_1CE4_E5B9) & mask
    value = ((value ^ (value >> 27)) * 0x94D0_49BB_1331_11EB) & mask
    return value ^ (value >> 31)


# ------------------------------------------------------------------ corpus ---

def load_manifest(sha: str) -> dict[str, tuple[str, str, int]]:
    tree: dict[str, tuple[str, str, int]] = {}
    raw = (CORPUS / "inputs" / sha / "manifest.tsv").read_text()
    for line in raw.splitlines():
        if not line:
            continue
        mode, oid, size, hexpath = line.split("\t")
        tree[bytes.fromhex(hexpath).decode("utf-8", "surrogateescape")] = (
            mode, oid, int(size),
        )
    return tree


def main() -> int:
    if "--selfcheck" in sys.argv:
        ok = True
        for raw in (b"a" * 20, bytes(range(256)) * 3, b"hello world, hello layerfs!" * 40):
            a, b = signature(raw), signature_reference(raw)
            print(f"selfcheck len={len(raw):6d} vectorised={len(a)} literal={len(b)} equal={a == b}")
            ok &= a == b
        print("selfcheck: " + ("PASS" if ok else "FAIL"))
        return 0 if ok else 1

    manifest = json.loads((CORPUS / "checkpoint-manifest.json").read_bytes())
    cps = manifest["checkpoints"]
    shas = [cps[i - 1]["sha"] for i in SELECTION]
    states = [load_manifest(sha) for sha in shas]

    # construction order, exactly the driver's: state, then path ascending.
    order: list[tuple[int, str, str, int]] = []   # position, path, oid, size
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

    # ------------------------------------------------------------- blobs ------
    # `blobs/` of checkpoint k holds exactly the oids k added or changed, and a
    # selected state takes a DIRECT transition from the previous selected state,
    # so a changed oid may have been introduced by any checkpoint in the span.
    # That is exactly `Corpus::locate`.  Every span is scanned once.
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
    lengths = defaultdict(int)
    for sig in signatures.values():
        lengths[len(sig)] += 1
    print(f"signature lengths: {dict(sorted(lengths.items()))}")

    # ------------------------------------------------------- index models -----
    def run(persist: bool, ring: int | None, reference_bits: int | None) -> dict:
        """One index model, with `candidates.rs`'s own admission rule applied.

        persist=False is the product today: the index is dropped at the end of
        every save.  `ring` is `SLOTS` (the product's own 1024-entry ring) or
        None.  `reference_bits` is `REFERENCES` (8192): the product's reference
        table, one entry per `hash & (REFERENCES - 1)` slot, last writer wins --
        a real recall limiter -- or None for an exact hash -> entries map.

        Only an object actually stored FULL enters the index; one that took a
        PREFIX record does not.  The choice is self-consistent here (an object
        enters exactly when no candidate was found for it), so the only modelled
        gap is the single prefix trial's full_wins/full_losses cases, which need
        the real codec (B2 measured 11 and 28 objects).
        """
        by_hash: dict[int, list[int]] = defaultdict(list)
        references: dict[int, int] = {}
        entries: list[tuple[str, tuple[int, ...], int, str]] = []
        ring_ids: list[int] = []
        covered: dict[str, tuple[str, int]] = {}
        covered_cross_save: dict[str, tuple[str, int]] = {}
        same_path_hits = 0
        cross_path_hits = 0
        admitted: set[str] = set()
        mask = None if reference_bits is None else (1 << reference_bits) - 1
        for position, path, oid, size in whole:
            # An object whose content the Store already holds is reused by
            # membership, not re-encoded, so it never enters the index.
            fresh = oid not in admitted
            admitted.add(oid)
            sig = signatures[oid]
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
                    if not persist and other_position != position:
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
                    covered[oid] = (other_oid, best[0])
                    if entries[index][2] != position:
                        covered_cross_save[oid] = (other_oid, best[0])
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

    print()
    rows = {}
    for label, persist, ring, bits in (
        ("P0 per-save, ring1024, refs8192  == the product today", False, SLOTS, 13),
        ("P1 per-save, ring1024, exact index", False, SLOTS, None),
        ("P2 per-save, unbounded, exact index", False, None, None),
        ("S1 persistent, ring1024, refs8192", True, SLOTS, 13),
        ("S2 persistent, ring1024, exact index", True, SLOTS, None),
        ("S3 persistent, unbounded, exact index", True, None, None),
    ):
        started = time.monotonic()
        row = run(persist, ring, bits)
        rows[label] = row
        print(
            f"{label:<52} covered {row['covered_objects']:6d}"
            f" (cross-save {row['covered_cross_save_objects']:6d})"
            f" canonical {row['covered_canonical_bytes']:12d}"
            f"  [{time.monotonic() - started:.1f}s]"
        )
        print(
            f"{'':<52} same-path {row['covered_same_path']:6d}"
            f" cross-path {row['covered_cross_path']:6d}"
        )

    result = {
        "whole_file_occurrences": len(whole),
        "signatures_missing": missing,
        "signature_lengths": dict(sorted(lengths.items())),
        "models": rows,
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True))
    print(f"\nwrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
