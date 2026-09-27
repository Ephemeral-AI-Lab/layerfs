# Issue 261: three 100-write patterns on one 10 MiB old head

> **Status:** Dated exploratory diagnosis. All latency rows are cache-INELIGIBLE;
> no numerical speed ranking or release admission follows from them.

The [prospective specification](../../../../docs/roadmap/0.1/0.1.7/issue261-three-pattern-100-spec.md)
was committed at `af0f10ef1` before changing the writer or sampling. The
runner/verifier were committed at `f47f4525b`; source identity for all three
public attempts was `7361312e6936136930584b314717067bcee1d879`.
The [append-only evidence](evidence/patterns-100-v1/) retains the raw receipts,
driver and verifier logs, old/expected manifests, release build and master
proof, seals, and ignored Store/history hashes. One initial preparation failed
at `f47f4525b` because the broad product seal includes verifier examples. The
exact changed Core file was only `verify_shell.rs`; `7361312e6` made release
binary reuse fail-closed on all other Core inputs. That failure produced **no
sample** and remains recorded as `preparation-fail.json`.

All cases used the same independently verified old Commit and 10,485,760-byte
`data.bin` of `A`. Each cloned its closed Store/history by independent writable
byte copy, mounted once, called one public SDK Exec that launched one shell and
one writer process with one fd, then called one public Commit. Append used 100
one-byte `write` calls with `O_APPEND`; dispersed used 100 unique interior
`pwrite` offsets from the fixed permutation; repeated used 100 `pwrite` calls
at byte 5,242,880 with a changed value each time. The independent oracle
checked both full old/new file images, exact head parent, bytes, mode, size,
changed runs, and path inventory. No direct Workspace mutation method was
called by the driver.

## One public attempt per case

Times below are raw, instrumented and cache-uncontrolled. They are useful for
locating work in each receipt but are not a cache-qualified comparison.

| Case | Exec ms | Commit ms | Complete command ms | Writer through #100 ms | Verifier ms | Functional / cleanup |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| True append | 433.382 | 34.674 | 1,406.260 | 420.161 | 93.066 | PASS / PASS |
| Dispersed edits | 518.925 | 75.974 | 1,524.494 | 505.467 | 94.954 | PASS / PASS |
| Repeated one-place edits | 467.413 | 23.324 | 1,345.645 | 451.469 | 93.154 | PASS / PASS |

Each row is `INELIGIBLE` for numerical performance admission. All completed
within the declared 15 s diagnostic command limit; the verifier ran separately
within 9 s. Each recorded exactly 100 actual FUSE WRITE callbacks, one LOOKUP,
three GETATTRs, one OPEN, zero READs, four Workspace upstream Service calls,
and no FUSE namespace mutation. FLUSH and RELEASE are implemented but not
counted individually. One successful projected write invokes one inode
invalidation; notification ACK and exact transport frame counts are unavailable.
The four upstream calls fit one Exec Inspect and Commit's SaveFile, portable
metadata and composite History operations. There is no per-write SDK/control
or Service round trip.

## Work and final-state counts

| Case | Final extents / changed runs / replacement bytes | Ledger 4 KiB reads / writes | Other metadata page reads | Live metadata pages | Retained payloads / private allocated bytes | Routine record scans |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Append | 101 / 1 / 100 | 2,941 / 1,470 | 591 | 4 | 100 / 843,776 | 99 |
| Dispersed | 201 / 100 / 100 | 3,696 / 1,929 | 703 | 37 | 100 / 983,040 | 99 |
| Repeated | 3 / 1 / 1 | 3,451 / 1,489 | 689 | 4 | 2 / 36,864 | 197 |

The ledger read/write totals amount to 18,067,456, 23,040,000 and 20,234,240
bytes of 4 KiB API direct-I/O transfer respectively; they are **not** measured
physical-device bytes. Every accepted one-byte callback still creates one
8,192-byte private segment with two aligned 4 KiB payload writes. Thus all
three generated 100 segments and 819,200 payload-write bytes during Exec.

| Case | Writer ms per consecutive 25 writes | Ledger reads per consecutive 25 writes |
| --- | --- | --- |
| Append | 101.723 / 87.492 / 114.086 / 116.861 | 688 / 725 / 753 / 775 |
| Dispersed | 96.194 / 99.198 / 125.537 / 184.537 | 688 / 725 / 1,006 / 1,277 |
| Repeated | 105.666 / 121.836 / 117.532 / 106.435 | 826 / 875 / 875 / 875 |

These are four checkpoints inside each one attempt, not four samples. The
snapshot itself scans retained records, and cache residency is uncontrolled.
The count divergence after write 50 is the durable evidence; quartile wall
differences alone cannot establish a speed trend.

Append and dispersed retain all 100 payloads. Repeated overwrites keep only
two at the final snapshot; the preceding versions become reclaimable as old
roots retire. Its 197 routine record scans, compared with 99 for the other
cases, are consistent with that extra reclamation. The repeated case's
`own_payload` span was 237.630 ms versus 227.007 ms dispersed and 196.056 ms
append; that span includes routine maintenance, so it does not isolate device
allocation time. Its publication span was 208.279 ms, versus 273.075 ms for
dispersed and 217.863 ms for append.

At writes 25/50/75/100, allocated metadata pages were 4/4/4/4 for append,
4/4/18/37 for dispersed, and 4/4/4/4 for repeated. Dispersed and append had
identical ledger counts through write 50 (1,413 reads, 691 writes), before the
124-record extent-leaf capacity is crossed. In the dispersed last quarter,
ledger reads rose to 1,277 and writes to 703, versus 688/341 in its first
quarter. The index path is localized, but the 37 live pages show substantial
retained metadata growth once interior edits spread across a branched tree.
`MetadataStatus.allocated_pages` does not identify page kinds or occupancy;
the exact number of extent leaves, branch pages, page writes and tree height
was not exported. Leaf-boundary fragmentation is a source-supported candidate,
not an observed page-kind count. The splice copies touched pages and shares
untouched children; its branch packing does not globally compact shared leaves.

For the 10 MiB chunked file, Commit lowered the **final** extent sequence,
not 100 historical writes. The host C1 save counters were:

| Case | C1 `nodes_read` | C1 nodes created | C1 payloads created | C1 payload bytes | Host SaveFile ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Append | 4 | 2 | 1 | 121 | 19.131 |
| Dispersed | 2,028 | 7 | 100 | 2,200 | 49.176 |
| Repeated | 17 | 4 | 1 | 22 | 5.002 |

`replace_chunked` applies each final edit with two splits and concatenation
over mapping summaries, so 100 dispersed final runs drive more C1 tree work
than one append or repeated run. Its `nodes_read` counter is a logical
`load_node` count: `load_node` increments it before selecting a stored page or
an in-memory draft. **2,028 is not a count of 2,028 Store reads or round trips.**
The exact Store read/write operations remain unexported. The Store file grew
286,720 / 290,816 / 286,720 bytes for append / dispersed / repeated; SQLite
file-size growth does not attribute individual Store work. History file size
was unchanged, while each oracle verified the new logical head.

## Complexity and next optimization decisions

Let `N=100`, `E` be final extents, `H` the file extent-tree height, `F` its
fixed page fanout, and `R` final changed runs. Every case necessarily has
`N` synchronous kernel WRITE requests/replies and `N` atomic Workspace root
publications. Private segment acquisition is `O(N)` with a large fixed I/O
constant. A localized splice descends a touched path, `O(H + touched pages)`,
but ownership publication and prior-root cleanup enumerate the Local/child
edges of copied pages and perform 4 KiB ledger read-modify-write batches.
The current append and repeated sequences fit one 124-record leaf; dispersed
crosses that leaf capacity and the metadata-page count then rises. Commit
walks final descriptors `O(E)` and applies `R` final C1 edits to the chunked
base. Clean close removes retained payloads and is `O(retained payloads)`.

The repeated case confirms the user's desired **Commit** semantics: only the
last byte is represented in the committed change. It does **not** yet deliver a
near-free Exec. The same-offset loop still pays 100 FUSE callbacks, private
segment acquisitions, metadata root publications, invalidations and nearly
100 old-payload reclamations. The raw repeated Exec was below dispersed, but
the counts show a shared per-write floor and the cache state prevents a speed
ranking.

The smallest source-confirmed next candidate is the duplicate authenticated
ledger read during old-root `cleanup_step` → `load_raw` in
`backing/metadata_reclaim.rs` / `backing/ownership.rs`. Reuse the verified
owner within the same cleanup step while retaining identity/checksum and
failure ordering; declare and measure its predicted read-count effect in a
new labelled 100-write diagnostic. For larger gains in repeated writes,
reduce the tiny payload's 8 KiB file/create/fallocate/two-write profile while
preserving immutable per-write custody and exact charging. For dispersed
edits, first export extent page-kind/occupancy and C1 stored-page versus draft
load counts; then consider local leaf rebalancing and a bulk final-edit walk.
Neither may defer successful write visibility until Commit.

The #248 4,097 public gate remains `NOT_RUN`; the #249 30 s product Exec timer
and the earlier 512-write `EBUSY` remain separate unresolved risks. No
workload, deadline, worker count or cache policy was changed after these rows.

## Source and checks

Relevant source: `layerfs-fuse/src/adapter.rs` (`write`),
`layerfs-workspace/src/backing/payload.rs` (`own_payload`),
`layerfs-workspace/src/backing/binary_plus_tree/extent/splice.rs` (`replace`),
`layerfs-workspace/src/backing/ownership/ledger_batch.rs` (`change_refs_run`),
`layerfs-workspace/src/commit/lower.rs` (`lower_file`), and
`layerfs-content/src/file/edit/apply.rs` (`replace_chunked`). The source and
exact logs are pinned by each receipt.

The sealed-image generic writer's three modes passed an exact-byte plain-file
smoke check outside LayerFS; the locked release verifier built; the one old
master passed its read-only proof; all three public Exec/Commit routes and
independent oracles passed. No product implementation source changed in this
three-pattern follow-up, so previously passing unaffected product checks were
not rerun. A Zig `-fsyntax-only` attempt was rejected by Zig's wrapper as an
unused `-c` argument; the subsequent static `-O3` writer build and sealed-image
smoke check passed. No CI or preflight was run.

Production LOC method for each commit: `tools/production_loc.py --root .
--detail`; first parent and staged tree, product only. Reference remained
65,417; Core remained 58,509; adapter remained 0; combined remained 123,926.
The prospective spec (`af0f10ef1`), runner/verifier (`f47f4525b`), and
fail-closed reuse correction (`7361312e6`) each changed combined and per-scope
production LOC by **0**. The evidence/report commit also changes no product
source; its exact comparison belongs in that commit message.
