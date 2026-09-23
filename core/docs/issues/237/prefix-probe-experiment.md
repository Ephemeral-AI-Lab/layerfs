# #237: threshold-probe initial reserve diagnostic

> Status: Research; informative and not a product contract. One standalone
> synthetic diagnostic on 2026-09-23; no product edit or public Init sample.

## Question and method

The native 10k import calls `construct_stream` 10,000 times. Its current prefix
probe reserves the complete policy cutoff in each `Vec` before reading. The
[source analysis](prefix-reserve-10k.md) proposed testing a smaller initial
reserve. I copied the exact `Vec::new` → `try_reserve_exact` →
`source.take(cutoff).read_to_end` sequence into a standalone Rust diagnostic,
using the frozen six file-size classes (100 empty; 1,074 × 325 B; 6,825 × 326 B;
1,500 × 20,782 B; 500 × 332,506 B; one × 100,000,000 B). A nonallocating
synthetic reader supplies equal `0xa5` bytes; no prepared or public benchmark
fixture was read. The script checks every prefix byte, dispatch decision and
bounded read count. A global allocator wrapper records allocation/reallocation
calls and requested bytes. Time covers prefix allocation and read, including
counter overhead, but excludes checksum validation and output. Each declared
cutoff/start arm ran **once**, in source order (cutoff, 4 KiB, 512 B, zero).

Toolchain: `rustc 1.85.1 (4eb161250 2025-03-15)`, arm64. Source identity:
`e31667f7a`; [diagnostic source](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/prefix_probe_diagnostic.rs)
SHA-256 `8b2d053f621f10977fc694e4776da9b360940c2fd768085dbfbf554124bc5553`.
Compile with `rustc +1.85.1 -O prefix_probe_diagnostic.rs -o prefix_probe_diagnostic`;
run the binary once and save stdout as
[raw TSV](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/prefix-probe-raw.tsv)
(SHA-256 `014a7e89a80aab76451aa4e19cc9306c78590bdaf2e51499f47ec80cafda34ed`).
The [process receipt](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/prefix-probe-run.time.txt)
is 2.69 s wall for all 96 class/arm rows, including validation and output.
The operation remains outside the public benchmark and supplies **no cold-file
throughput claim**. SQLite page size was not touched and remains 4 KiB.

## Results

All 96 rows passed exact byte, dispatch and source-read checks. This table
sums the six class timers and allocator counts for each arm. Requested MB is
the sum of allocation/reallocation requests, not retained memory or physical
pages; maximum capacity is per `Vec`.

| Cutoff | Initial reserve | Prefix time | Alloc calls | Realloc calls | Alloc requested | Realloc requested | Max capacity |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 128 KiB | 128 KiB | 6.560 ms | 10,000 | 0 | 1,310.72 MB | 0 MB | 128 KiB |
| 128 KiB | 4 KiB | 7.006 ms | 10,000 | 7,506 | 40.96 MB | 344.58 MB | 256 KiB |
| 128 KiB | 512 B | 6.972 ms | 10,000 | 13,509 | 5.12 MB | 358.92 MB | 256 KiB |
| 128 KiB | 0 B | 9.516 ms | 9,900 | 53,109 | 0.32 MB | 368.43 MB | 256 KiB |
| 256 KiB | 256 KiB | 7.074 ms | 10,000 | 0 | 2,621.44 MB | 0 MB | 256 KiB |
| 256 KiB | 4 KiB | 7.418 ms | 10,000 | 8,007 | 40.96 MB | 607.25 MB | 512 KiB |
| 256 KiB | 512 B | 7.721 ms | 10,000 | 14,010 | 5.12 MB | 621.59 MB | 512 KiB |
| 256 KiB | 0 B | 10.260 ms | 9,900 | 53,610 | 0.32 MB | 631.10 MB | 512 KiB |
| 512 KiB | 512 KiB | 7.847 ms | 10,000 | 0 | 5,242.88 MB | 0 MB | 512 KiB |
| 512 KiB | 4 KiB | 8.684 ms | 10,000 | 8,008 | 40.96 MB | 608.30 MB | 1 MiB |
| 512 KiB | 512 B | 8.891 ms | 10,000 | 14,011 | 5.12 MB | 622.64 MB | 1 MiB |
| 512 KiB | 0 B | 11.230 ms | 9,900 | 53,611 | 0.32 MB | 632.14 MB | 1 MiB |
| 1 MiB | 1 MiB | 8.075 ms | 10,000 | 0 | 10,485.76 MB | 0 MB | 1 MiB |
| 1 MiB | 4 KiB | 8.806 ms | 10,000 | 8,009 | 40.96 MB | 610.39 MB | 2 MiB |
| 1 MiB | 512 B | 8.900 ms | 10,000 | 14,012 | 5.12 MB | 624.74 MB | 2 MiB |
| 1 MiB | 0 B | 11.388 ms | 9,900 | 53,612 | 0.32 MB | 634.24 MB | 2 MiB |

At the frozen default cutoff, the 4 KiB start saved about 2.14 ms in the
empty/tiny classes (2.937 → 0.799 ms), then lost about 2.59 ms in the
small/medium/anchor classes (3.624 → 6.208 ms). Net synthetic prefix time
**increased 0.446 ms**, with 7,506 extra reallocations and 5,502 more source
read calls. The complete existing probe consumed only 6.560 ms in this
single-threaded synthetic model, far below the preregistered 50 ms potential
needed to pursue a public treatment. The model excludes disk latency and
worker contention, so its elapsed difference must not be presented as the
public operation's difference.

## Decision

**Reject the one-line smaller-reserve treatment.** At every accepted cutoff,
`read_to_end` grew a `Vec` to **twice** the cutoff in the medium or anchor
class, whereas the current initial reserve ended at exactly the cutoff. That
breaks the existing probe-capacity ceiling and shifts allocation failure from
the explicit pre-read fallible reserve into growth during the read. The
synthetic result also showed no time gain. A capped fallible read loop would
be a separate algorithm with additional code and no measured need here.
This experiment produced no product change and consumed no public benchmark
sample; the cold 10k throughput result remains whatever its own receipt says.
