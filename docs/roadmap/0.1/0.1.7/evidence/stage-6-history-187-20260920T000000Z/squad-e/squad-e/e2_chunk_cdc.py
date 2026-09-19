#!/usr/bin/env python3
"""E2 -- the chunk lane: is CDC a pure function of the byte stream, and what
does a chunk DELTA base need?

Ports layerfs-content/src/file/cdc/gear.rs (two-byte rolling GEAR, frozen
profile) line for line, then runs it over the stride10 lane's chunked file
versions and asks two separate questions:

  Q1  does the chunk partition depend on anything but the bytes?  (pure function)
  Q2  how many chunks are byte-identical to a chunk of an EARLIER version of the
      same file -- i.e. how much of the chunk lane's win is exact dedup, which
      needs no correspondence at all?

    python3 e2_chunk_cdc.py
"""
from __future__ import annotations

import json
import re
import sqlite3
from collections import Counter, defaultdict
from pathlib import Path

CORPUS = Path("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data")
GEAR_RS = Path(
    "/Users/yifanxu/Ephemeral-AI-Lab/layerfs/core/crates/layerfs-content/src/file/cdc/gear.rs"
)
STORE = Path("/tmp/base187/sample.sqlite")
HERE = Path(__file__).resolve().parent
OUT = HERE / "e2_chunk_cdc.json"

SELECTION = tuple(sorted(set(range(1, 158, 10)) | {157}))
SMALL_FILE_THRESHOLD = 131_072

MASK64 = (1 << 64) - 1
NORMALIZATION_SHIFT = 2
PROFILE_SEED = 0
MINIMUM = 8_192
TARGET = 16_384
MAXIMUM = 32_768
SMALL_MASK = 0x0000_D903_0353_7000
LARGE_MASK = 0x0000_D901_0353_0000
SHIFTED_SMALL_MASK = 0x0001_B206_06A6_E000
SHIFTED_LARGE_MASK = 0x0001_B202_06A6_0000

# canonical_length - payload, derived from the Store's own Chunk rows below
CHUNK_CANONICAL_OVERHEAD = 21


def load_gear() -> list[int]:
    text = GEAR_RS.read_text()
    body = text.split("pub const GEAR: [u64; 256] = [", 1)[1].split("];", 1)[0]
    values = [int(token, 16) for token in re.findall(r"0x[0-9a-fA-F_]+", body)]
    assert len(values) == 256, f"GEAR has {len(values)} entries"
    return values


GEAR = load_gear()


def scan_region(data: bytes, hash_value: int, shifted_mask: int, mask: int):
    cursor = 0
    size = len(data)
    while cursor < size:
        first = data[cursor]
        second = data[cursor + 1]
        hash_value = (hash_value << NORMALIZATION_SHIFT) & MASK64
        hash_value = (hash_value + (GEAR[first] << 1)) & MASK64
        if hash_value & shifted_mask == 0:
            return hash_value, cursor
        hash_value = (hash_value + GEAR[second]) & MASK64
        if hash_value & mask == 0:
            return hash_value, cursor + 1
        cursor += 2
    return hash_value, None


class Scanner:
    """The frozen scanner, ported."""

    def __init__(self) -> None:
        self.chunk = bytearray()
        self.pending = None
        self.hash = PROFILE_SEED

    def emit(self, sink) -> None:
        if not self.chunk:
            return
        sink(bytes(self.chunk))
        self.chunk = bytearray()
        self.hash = PROFILE_SEED

    def process_pending_pair(self, first: int, second: int, sink) -> None:
        small = len(self.chunk) < TARGET
        self.hash = (self.hash << NORMALIZATION_SHIFT) & MASK64
        self.hash = (self.hash + (GEAR[first] << 1)) & MASK64
        if self.hash & (SHIFTED_SMALL_MASK if small else SHIFTED_LARGE_MASK) == 0:
            self.emit(sink)
            self.chunk += bytes((first, second))
            return
        self.hash = (self.hash + GEAR[second]) & MASK64
        if self.hash & (SMALL_MASK if small else LARGE_MASK) == 0:
            self.chunk.append(first)
            self.emit(sink)
            self.chunk.append(second)
            return
        self.chunk += bytes((first, second))
        if len(self.chunk) == MAXIMUM:
            self.emit(sink)

    def consume(self, data: bytes, sink) -> None:
        if self.pending is not None:
            first = self.pending
            self.pending = None
            second = data[0]
            data = data[1:]
            self.process_pending_pair(first, second, sink)
        while data:
            if len(self.chunk) < MINIMUM:
                needed = MINIMUM - len(self.chunk)
                take = min(needed, len(data))
                self.chunk += data[:take]
                data = data[take:]
                if not data:
                    return
            position = len(self.chunk)
            hash_value = self.hash
            available_pairs = len(data) // 2
            if position < TARGET:
                small_pairs = min((TARGET - position) // 2, available_pairs)
            else:
                small_pairs = 0
            small_end = small_pairs * 2
            hash_value, cut = scan_region(
                data[:small_end], hash_value, SHIFTED_SMALL_MASK, SMALL_MASK
            )
            cursor = small_end
            if cut is None:
                position += small_end
                available_pairs = (len(data) - cursor) // 2
                large_pairs = min((MAXIMUM - position) // 2, available_pairs)
                large_end = cursor + large_pairs * 2
                hash_value, found = scan_region(
                    data[cursor:large_end], hash_value, SHIFTED_LARGE_MASK, LARGE_MASK
                )
                position += large_end - small_end
                start = cursor
                cursor = large_end
                cut = None if found is None else start + found
            self.hash = hash_value
            if cut is not None:
                self.chunk += data[:cut]
                self.emit(sink)
                data = data[cut:]
                continue
            self.chunk += data[:cursor]
            data = data[cursor:]
            if position == MAXIMUM:
                self.emit(sink)
                continue
            if data:
                self.pending = data[0]
            return

    def finish(self, sink) -> None:
        if self.pending is not None:
            self.chunk.append(self.pending)
            self.pending = None
        self.emit(sink)


def chunkify(payload: bytes, block: int = MAXIMUM) -> list:
    scanner = Scanner()
    chunks = []
    for start in range(0, len(payload), block):
        scanner.consume(payload[start : start + block], chunks.append)
    scanner.finish(chunks.append)
    return chunks


def load_manifest(sha: str) -> dict:
    tree = {}
    for line in (CORPUS / "inputs" / sha / "manifest.tsv").read_text().splitlines():
        if not line:
            continue
        mode, oid, size, hexpath = line.split("\t")
        tree[bytes.fromhex(hexpath).decode("utf-8", "surrogateescape")] = (
            mode, oid, int(size),
        )
    return tree


def main() -> int:
    manifest = json.loads((CORPUS / "checkpoint-manifest.json").read_bytes())
    cps = manifest["checkpoints"]
    shas = [cps[i - 1]["sha"] for i in SELECTION]
    states = [load_manifest(sha) for sha in shas]

    where = {}
    for position in range(len(SELECTION)):
        first = 1 if position == 0 else SELECTION[position - 1] + 1
        for index in range(first, SELECTION[position] + 1):
            directory = CORPUS / "inputs" / cps[index - 1]["sha"] / "blobs"
            for entry in directory.iterdir():
                where.setdefault(entry.name, entry)

    versions = []
    for position, tree in enumerate(states):
        previous = states[position - 1] if position else {}
        for path, (mode, oid, size) in sorted(tree.items()):
            old = previous.get(path)
            if (old is None or old[1] != oid) and size >= SMALL_FILE_THRESHOLD:
                versions.append((position, path, oid, size))
    print(f"chunked versions {len(versions)} bytes {sum(v[3] for v in versions)}")

    probe = where[versions[0][2]].read_bytes()
    one = chunkify(probe, block=MAXIMUM)
    two = chunkify(probe, block=4096)
    three = chunkify(probe, block=64 * 1024)
    print(
        f"Q1 purity probe ({len(probe)} B): block 32768 -> {len(one)} chunks,"
        f" block 4096 -> {len(two)}, block 65536 -> {len(three)},"
        f" identical partitions: {one == two == three}"
    )

    emitted = Counter()
    per_version = {}
    for position, path, oid, size in versions:
        chunks = chunkify(where[oid].read_bytes())
        per_version[(path, oid)] = chunks
        for chunk in chunks:
            emitted[len(chunk)] += 1

    store = sqlite3.connect(f"file:{STORE}?mode=ro", uri=True)
    stored = Counter(
        row[0] - CHUNK_CANONICAL_OVERHEAD
        for row in store.execute(
            "select canonical_length from objects where object_role = 2"
        )
    )
    print(
        f"port validation: emitted {sum(emitted.values())} chunks,"
        f" Store holds {sum(stored.values())} distinct Chunk objects"
    )
    print(f"  emitted length multiset == stored length multiset: {emitted == stored}")
    if emitted != stored:
        extra = emitted - stored
        missing = stored - emitted
        print(
            f"  emitted-only {sum(extra.values())}, stored-only {sum(missing.values())}"
        )

    total = 0
    reused = 0
    reused_bytes = 0
    within = 0
    seen = defaultdict(set)
    first_position = {}
    for position, path, oid, size in versions:
        chunks = per_version[(path, oid)]
        total += len(chunks)
        for chunk in chunks:
            if chunk in seen[path] and first_position[path] < position:
                reused += 1
                reused_bytes += len(chunk)
        local = Counter(chunks)
        within += sum(count - 1 for count in local.values())
        seen[path].update(chunks)
        first_position[path] = position

    # The same accounting globally: a duplicate emission is a chunk the Store
    # already holds, and the split says whether the match crossed a file.
    global_counts = Counter()
    owner = {}
    same_path_dupes = 0
    cross_path_dupes = 0
    for position, path, oid, size in versions:
        for chunk in per_version[(path, oid)]:
            global_counts[chunk] += 1
            if chunk in owner:
                if owner[chunk] == path:
                    same_path_dupes += 1
                else:
                    cross_path_dupes += 1
            else:
                owner[chunk] = path
    distinct_chunks = len(global_counts)
    duplicates = sum(global_counts.values()) - distinct_chunks
    print(
        f"  distinct chunk contents {distinct_chunks} vs Store Chunk rows"
        f" {sum(stored.values())}  -> residual {distinct_chunks - sum(stored.values())}"
    )
    print(
        f"  duplicate emissions {duplicates}: same-path {same_path_dupes},"
        f" cross-path {cross_path_dupes}"
    )

    share = 100.0 * reused / total if total else 0.0
    print(
        f"Q2 same-file exact chunk reuse: {reused} of {total} chunks"
        f" ({share:.1f} %), {reused_bytes} B"
    )
    print(
        "  a chunk identical to an earlier version's chunk has the SAME ObjectId,"
        " so the Store already dedups it with no correspondence at all"
    )

    result = {
        "chunked_versions": len(versions),
        "chunked_version_bytes": sum(v[3] for v in versions),
        "purity_probe_chunks": [len(one), len(two), len(three)],
        "purity_probe_identical": one == two == three,
        "emitted_chunks": sum(emitted.values()),
        "store_chunk_objects": sum(stored.values()),
        "port_matches_store": emitted == stored,
        "chunks": total,
        "chunks_reused_within_file": reused,
        "bytes_reused_within_file": reused_bytes,
        "chunks_duplicated_within_one_version": within,
        "distinct_chunk_contents": distinct_chunks,
        "distinct_minus_store_residual": distinct_chunks - sum(stored.values()),
        "duplicate_emissions": duplicates,
        "duplicate_emissions_same_path": same_path_dupes,
        "duplicate_emissions_cross_path": cross_path_dupes,
    }
    OUT.write_text(json.dumps(result, indent=2, sort_keys=True))
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
