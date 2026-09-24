# #237: matched public-call memory on the exact 100k source

> **Status: Research; informative and not a product contract.** The old
> release operation was not repeated. This compares its retained memory
> reading with one new count-driven release diagnostic of promoted C3.
> It is not a timing or release-admission pair.

## The same process-memory instrument and window

The [prospective plan](c3-v016-memory-diagnostic-plan-20260924.md) fixed
`getrusage(RUSAGE_SELF).ru_maxrss` at a post-setup **t0** and **t1,
immediately after the public result**. Source review after the run found
one plan imprecision: the old driver takes its t0 resource snapshot
*before* its READY/GO cold-preflight barrier; Core takes t0 immediately
before the call. Both drivers take t1 immediately after their call and
use the same Darwin `proc_pid_rusage` API for current RSS and physical
footprint. The exact matched measure here is therefore **lifetime
high-water RSS read at public-call return**, not a strictly identical
incremental call-only peak. v0.1.6's original
[release receipt](evidence/c3-v016-side-by-side-20260924/v016-receipt.json)
already had these readings; no second old operation was run. A temporary
[Core release driver](evidence/c3-v016-memory-20260924/benchmark_init_memory.rs)
used the same C layouts and APIs around one real `Client::init_project`
call. The [C3 diagnostic receipt](evidence/c3-v016-memory-20260924/receipt.json)
and [raw probe line](evidence/c3-v016-memory-20260924/driver.stderr)
retain the new readings. Both t1 peaks exceeded their post-setup t0
peaks. The old READY/GO step is inside that interval; it did no source
import, but its small input handling is not separately charged.

Both calls read the same seed-1 SHAKE manifest SHA-256
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`:
100,000 files, 1,001 directories and 500,000,000 logical bytes.
The new C3 run used a fresh verified byte copy and fresh Store/History,
locked Cargo release examples, and **0/126,206 resident payload pages**
at launch. Metadata cache state is still unqualified. Its separate
[full reopened oracle](evidence/c3-v016-memory-20260924/verification.json)
passed all 101,001 paths, metadata fields, sizes and SHA-256s.
The old [full oracle](evidence/c3-v016-side-by-side-20260924/v016-verification.json)
also passed. Core's target is the promoted C3 replacement tree for
v0.1.7 development; it is not a released v0.1.7 binary.

| Process measure | v0.1.6 release | Promoted C3 diagnostic | C3 minus old |
| --- | ---: | ---: | ---: |
| t0 lifetime peak RSS | 4,472,832 B (4.27 MiB) | 4,915,200 B (4.69 MiB) | +442,368 B |
| **t1 lifetime peak RSS** | **92,405,760 B (88.13 MiB)** | **154,763,264 B (147.59 MiB)** | **+62,357,504 B (59.47 MiB)** |
| New peak above post-setup t0 | 87,932,928 B (83.86 MiB) | 149,848,064 B (142.91 MiB) | +61,915,136 B; old t0 precedes GO |
| Current RSS at t1 | 92,405,760 B (88.13 MiB) | 145,981,440 B (139.22 MiB) | +53,575,680 B |
| Physical footprint at t1 | 52,560,400 B (50.13 MiB) | 116,867,648 B (111.45 MiB) | +64,307,248 B |
| Swaps at t1 | 0 | 0 | 0 |

On this exact source and matched **t1 process peak RSS** measure,
v0.1.6 used **40.29% less** than promoted C3; equivalently C3's peak
was **67.48% higher** than v0.1.6's. C3's current RSS at t1 was
8,781,824 B below its own high-water, so a snapshot only at return
would miss some of its operation peak. The
[derived arithmetic](evidence/c3-v016-memory-20260924/comparison.json)
checks these values against both raw receipts and confirms that C3's
t1 high-water equals its independent `wait4` lifecycle peak. The old
and C3 calls remain different public APIs with different formats and
Save boundaries; these two diagnostic observations do not establish
a universal version-wide memory ratio.

## Where the bytes are visible

The temporary C3 source instrument was frozen as a
[compressed diff](evidence/c3-v016-memory-20260924/instrument.diff.gz)
before the run, then removed. The following are **different accounting
domains and observation points**; they are not disjoint pieces to sum
into peak RSS.

| Component | v0.1.6 retained evidence | Promoted C3 diagnostic | Interpretation |
| --- | ---: | ---: | --- |
| SQLite live connection cache | 34,604,032 B at public t1; 33,554,432-B target | 8,767,488 B at the end of file Save and again at tree Save; 180,992 B at prerequisite Save; 2,000-page target | C3's two large readings belong to sequential Save connections, not concurrent caches. Its Save connections drop before public t1. The observed C3 excess is not explained by a larger SQLite page cache. |
| Source inventory vectors | Old task state 774,320 B | Entry vector reserves 14,680,064 B; job vector reserves 23,068,672 B at scan completion | The Core vectors coexist then. They are explicit reservations, not process RSS. |
| Inventory heap payload lengths | Old source path/name allocation unmeasured | 705,000 B entry names + 15,700,000 B job paths at scan completion | Lengths are lower bounds on those heap allocations; allocator capacity and overhead are unmeasured. |
| Other import buffers | Old explicit importer-buffer peak 5,216,394 B; slab queue peak 1,048,576 B | Core producer queues, Save batches and codec buffers lack a measured aggregate peak | Different counters and lifetimes; do not subtract one from the other. |
| Source payload page cache | 0/126,206 resident at launch; after-call residency unmeasured | 0/126,206 at launch; 126,206/126,206 after driver exit | Host file-cache pages are outside process RSS. The page count includes per-file page rounding and is not an exact byte charge. |

At Core scan completion, the two vector reservations and the stated
name/path lengths coexist and total **at least 54,153,736 B
(51.65 MiB)** of requested or logically occupied inventory storage.
The retained v0.1.6 importer uses a compact task/frontier and bounded
slab handoff rather than Core's retained `PreparedEntry` plus `Job`
inventory; their counters do not cover identical objects. The Core
structural inventory is a plausible major contributor to the higher
process peak, **not an exact attribution of the 62,357,504-B RSS gap**.
The Core file Save still opened 100,000 files and read 500,000,000 B;
it handed 111,354 objects in 2,134 batches. Its three Saves inserted
112,424 objects and acknowledged 434 transactions in this distinct
diagnostic. Counts and receipt identities are in the
[trace](evidence/c3-v016-memory-20260924/driver.stderr).

**Later phase-cause follow-up, 2026-09-24:** a separate
[C3 boundary diagnostic](c3-memory-phase-cause-result-20260924.md)
found that scan, file construction/Save and namespace construction
each set new process high-waters. It measured 54,753,736 B of entry/job
reservation and name/path lengths together at scan completion, then
18,838,440 B of serial/metadata/content-root/inode/directory vector
reservation at tree input. The result supports two materialization
contributors, not a byte-for-byte partition of this receipt's
62,357,504-B old-to-C3 difference.

The Core public-call CPU was 3.926369 s user + 5.579379 s system,
versus old 3.125596042 s user + 4.220182875 s system. That is a
matched resource window, but the APIs differ and this diagnostic is
not a speed comparison. The separate C3 driver lifecycle peak was
154,763,264 B by `wait4`; its full command took 5.664510875 s and
the oracle 9.347692042 s. These timings remain diagnostic. The
registered #236 **debug** 100k selection remains `NOT_RUN`; the
separate [#229 sparse-history gate](https://github.com/Ephemeral-AI-Lab/layerfs/issues/229)
remains open.

## Custody

The merged source commit for this diagnostic was `4caae8a273308dcb045ba05db32955af8d013f14`
(documentation-only successor to PR #239's merge). The
[freeze record](evidence/c3-v016-memory-20260924/freeze.json) pins
the temporary diff SHA-256
`34fe91a12ce4b5c995961b4d60ffc3996d2ef8d4901f04bae618a14ea91a1d15`,
driver SHA-256 `2e38208be8ce2cece723f63e664093f78c47ef9c91da03760840aa7743e6d70e`
and [runner](evidence/c3-v016-memory-20260924/issue237_memory_runner.py)
SHA-256 `5cd8fc98a6219a1532db5d8bde4e6b948a6e1de481ea9be74457d3eaefc1c48f`.
The [build receipt](evidence/c3-v016-memory-20260924/build.json)
records the release binary hash, product/harness seals and locked
command. The raw closed Store, History, source copy and hash manifest
remain in this worktree under
`benchmark-results/fs-bench-pro/issue237-c3-memory-20260924-01/`.
Curated small files are byte-identical copies checked by
[SHA256SUMS.json](evidence/c3-v016-memory-20260924/SHA256SUMS.json).
