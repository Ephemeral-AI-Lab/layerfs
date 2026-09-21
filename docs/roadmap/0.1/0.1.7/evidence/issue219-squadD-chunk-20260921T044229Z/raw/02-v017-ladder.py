#!/usr/bin/env python3
"""raw/02 - Derive the v0.1.6 / v0.1.7 namespace-10000 file-size ladder from source.

Both arms (workload/main.rs:424-449 and
core/benchmark/fs-bench-pro-storage-content/src/ops/namespace_content.rs:113-129)
compute the SAME weight function; both then run the SAME largest-remainder pass
(workload/main.rs:332-386 / namespace_content.rs:205-258). This script transcribes
those two functions literally and prints the resulting population.

Pure arithmetic. It runs no product code, builds nothing, benchmarks nothing.

Usage:  python3 raw/02-v017-ladder.py
"""

LOGICAL_BYTES = 300_000_000
ANCHOR_FILES = 1
ANCHOR_BYTES = 100_000_000
EMPTY_FILES = 100
CUTOFF = 131_072          # layerfs-content/src/policy.rs:14 (exclusive)
WHOLE_FILE_OVERHEAD = 23  # layerfs-storage/src/policy.rs:127
CHUNK_OVERHEAD = 21       # layerfs-storage/src/policy.rs:124-127 (comment)
CHUNK_OBJECTS = 14_466    # v0.1.7 receipt resources.space.canonical_objects.chunk
WHOLE_FILE_OBJECTS = 9_444
FILE_STATE_OBJECTS = 456

# (count, lower weight, upper weight) - v0.1.6 workload/main.rs:427-429,
# v0.1.7 namespace_content.rs:38-42
BANDS = [(7_899, 1, 8), (1_500, 32, 256), (500, 1_024, 8_192)]


def relative_weight(lower, upper, role, count):
    """workload/main.rs:434-448 == namespace_content.rs:113-129."""
    width = upper - lower + 1
    numerator = (2 * role + 1) * width
    denominator = 2 * count
    return lower + numerator // denominator


def main():
    items = []  # (band_count, weight)
    for (count, lower, upper) in BANDS:
        for role in range(count):
            items.append((count, relative_weight(lower, upper, role, count)))

    weight_sum = sum(w for _, w in items)
    positive = len(items)
    distributable = LOGICAL_BYTES - ANCHOR_FILES * ANCHOR_BYTES - positive

    print("weight_sum          = %d" % weight_sum)
    print("positive (non-anchor, non-empty) files = %d" % positive)
    print("distributable       = %d - %d - %d = %d"
          % (LOGICAL_BYTES, ANCHOR_FILES * ANCHOR_BYTES, positive, distributable))
    print("scaling ratio       = %.6f" % (distributable / weight_sum))
    print()

    for (count, lower, upper) in BANDS:
        ws = [w for (c, w) in items if c == count]
        print("band count=%5d declared weight band (%d,%d): realized weights %d..%d sum=%d"
              % (count, lower, upper, min(ws), max(ws), sum(ws)))
    print()

    floors = [distributable * w // weight_sum for _, w in items]
    remainders = [(distributable * w) % weight_sum for _, w in items]
    extra = distributable - sum(floors)
    sizes = [1 + f for f in floors]
    # largest remainder first, ties by slot index (namespace_content.rs:253-255)
    for i in sorted(range(positive), key=lambda i: (-remainders[i], i))[:extra]:
        sizes[i] += 1

    print("largest-remainder top-ups (extra) = %d" % extra)
    print("sum of positive sizes             = %d" % sum(sizes))
    print("  + 100 empty + 100,000,000 anchor = %d" % (sum(sizes) + ANCHOR_BYTES))
    print()

    for (count, lower, upper) in BANDS:
        band = sorted(s for (c, _), s in zip(items, sizes) if c == count)
        print("band count=%5d -> byte sizes %7d .. %-7d total %d"
              % (count, band[0], band[-1], sum(band)))
    print("overall positive sizes %d .. %d" % (min(sizes), max(sizes)))
    print()

    # ---- cutoff partition -------------------------------------------------
    chunked = [s for s in sizes if s >= CUTOFF]
    whole = [s for s in sizes if s < CUTOFF]
    print("files with size == %d exactly      : %d" % (CUTOFF, sum(1 for s in sizes if s == CUTOFF)))
    print("non-anchor chunked (>= %d)         : %d" % (CUTOFF, len(chunked)))
    print("  + anchor                         : 1")
    print("chunked FILES total                : %d" % (len(chunked) + 1))
    print("whole-file objects                 : %d   (expected %d)"
          % (len(whole), WHOLE_FILE_OBJECTS))
    print()

    # ---- canonical byte closure -------------------------------------------
    whole_raw = sum(whole)
    chunk_raw = sum(chunked) + ANCHOR_BYTES
    print("whole-file raw bytes               : %d" % whole_raw)
    print("  %.0f objects x %d B overhead      : %d"
          % (len(whole), WHOLE_FILE_OVERHEAD, len(whole) * WHOLE_FILE_OVERHEAD))
    print("  canonical total                  : %d   (receipt 24652248)"
          % (whole_raw + len(whole) * WHOLE_FILE_OVERHEAD))
    print("chunk raw bytes                    : %d" % chunk_raw)
    print("  %d chunks x %d B overhead        : %d"
          % (CHUNK_OBJECTS, CHUNK_OVERHEAD, CHUNK_OBJECTS * CHUNK_OVERHEAD))
    print("  canonical total                  : %d   (receipt 275868750)"
          % (chunk_raw + CHUNK_OBJECTS * CHUNK_OVERHEAD))
    print("implied mean chunk payload         : %.2f B  (target 16384, max 32768)"
          % (chunk_raw / CHUNK_OBJECTS))
    print("file-state objects (chunked files) : %d   (receipt %d)"
          % (len(chunked) + 1, FILE_STATE_OBJECTS))
    print()
    print("NOTE: docstring at namespace_content.rs:26 cites weight sum 102,555,546;")
    print("      the formula at namespace_content.rs:113-129 yields %d." % weight_sum)


if __name__ == "__main__":
    main()
