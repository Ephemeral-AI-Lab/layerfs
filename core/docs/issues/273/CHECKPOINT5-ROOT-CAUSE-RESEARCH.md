# #273 checkpoint 5 root-cause research

> **Status:** Research; informative and not a product contract. This is a
> source-pinned, read-only audit of retained checkpoint-5 observations, not a
> new speed sample or release admission. Product source remains unchanged.

Source inspected: `f305a3c711bce1e77e44be503ca6dba95f6d2c38`; campaign producer
`1c2b2c1d0274a8267dbf410122a58d11574f1b47`; candidate product unchanged since
`a2359620a7966314fbb2a96c98e8da958df72c6c`; frozen control
`48b51e874a41b3e1e6c6661e145316df8b408f07`. The owner requested research with
subagents after reviewing the checkpoint-5 slowness report.

Three independent audits covered ordinary WRITE/publication, Commit/source
locality/reconcile, and harness/evidence validity. The primary agent combined
those findings, checked source anchors, derived the exact schedule/cache counts
and inspected existing process CPU telemetry. No Cargo, Docker, product test,
performance arm or fault injection ran. Raw receipts and historical reports
remain unchanged. [Derived arithmetic and input hashes](evidence/checkpoint5-root-cause/DERIVED.json)
make the calculations reproducible.

## 1. Findings and their confidence

The qualified hot WRITE structural improvement is real in these observations.
It does not make the whole operation cheap: each accepted byte still causes
multiple authenticated 4 KiB file versions, readback verification and retirement.
Dispersed writes barely use that specialization, and their Commit source loses
pack locality. Reconcile additionally contains an overlooked quadratic patch
scan. The cold-input claim has a concrete harness defect.

| Finding | Evidence | Confidence / limit |
| --- | --- | --- |
| Eligible append/frontier structural work scales with W; seeks/fetches plateau | Actual timed-command WRITE counters, not just functional tests | High for these schedules; not full CPU or elapsed-time proof |
| Publication is the dominant remaining Exec region | 5.075 s of 5.964 s append Exec; 9.739 s of 10.610 s dispersed Exec | Measured region; no inner split of I/O vs allocation/codec CPU |
| New page/file lifecycle creates a large per-byte floor | Successful write counts plus source create/write/readback/release chain | High on operation/byte volume; exact syscall wall shares unmeasured |
| Dispersed Commit repeatedly reloads packs and seeks P | One-page reader, exact schedule permutation, SaveFile attribution | High mechanism evidence; predicted miss counts need direct counter validation |
| Reconcile scans the full update map once per touched pack | `live_refs` loop, exact deletion-patch arithmetic | Confirmed superlinear term; not the principal 1.8 s SaveFile cost |
| Hashing occurs after eviction/residency verification | Actual campaign-producing harness and digest helper | Confirmed cold-contract defect affecting both arms |
| Quick controls/coverage do not establish retained private-journal bounds | Driver representation, progress parser and fixture lifecycle | Confirmed observer/coverage defects |

This narrows the prior readiness assessment: the focused hot-WRITE proof passed,
but full-path complexity and measurement qualification were not established.
The discovered C5 patch scan was outside the reported hot structural counters.

## 2. The hot route is exercised, but only on eligible workloads

The following counters come from campaign-3 candidate `backing_samples` at the
last sampled WRITE **4,096**, one before the 4,097-write command finishes.
Directory writes are already included in index writes.

| Candidate schedule | Hot writes | Admissions | Normalizations | Carries | Root seeks | Index fetches | Index writes | Pack writes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Append | 4,094 | 1 | 0 | 224 | 50 | 15 | 16,672 | 4,096 |
| Original #248 separated | 4,094 | 1 | 0 | 385 | 54 | 16 | 16,988 | 4,096 |
| Dispersed | 5 | 3,936 | 4,330 | 0 | 61,389 | 25,972 | 26,392 | 4,096 |
| Repeated | 0 | 0 | 0 | 0 | 65,637 | 41,052 | 4,096 | 4,096 |

Append quartiles W=1,024/2,048/3,072/4,096 have seeks 45/50/50/50,
fetches 15/15/15/15, visits 23,705/47,590/71,408/95,150, index writes
4,044/8,254/12,464/16,672 and retirement inspections
4,998/10,161/15,325/20,488. No repeated-root-seek or old-retired-prefix-sweep
failure is visible in that eligible sequence. Binding validation still counts
node visits even when it reads a cached node.

The specialized [frontier eligibility](../../../crates/layerfs-workspace/src/backing/active/hot_path.rs#L240)
requires advancing within retained Base/Zero coverage. The dispersed offset
permutation violates locality repeatedly; only five of 4,096 sampled writes
are hot. Its thousands of admissions/normalizations are real extra work on the
generic route. Repeated same-position replacement remains generic with bounded
final extent count. Neither schedule inherits the monotone-frontier bound.

The old control was already path-local, approximately O(W*log_B E), with finite
height bounds; it was not sustained O(W²). The new conditional O(W) structural
bound removes a small number of tree levels and ownership operations, not all
work. [The implementation spec's full CPU terms](PHASE4.5-IMPLEMENTATION-SPEC.md#L611)
retain encoding, registry/Node lookup, physical I/O and lifecycle cost.

## 3. What still costs six seconds for 4,097 one-byte writes

| Candidate command | Full Exec | Acquisition through sampled W=4,096 | Publication through sampled W=4,096 |
| --- | ---: | ---: | ---: |
| Append 4,097 | 5,964.176 ms | 723.286 ms | 5,074.657 ms |
| Original #248 gate | 6,562.717 ms | 779.112 ms | 5,609.905 ms |
| Dispersed 4,097 | 10,610.247 ms | 707.375 ms | 9,738.656 ms |
| Repeated 4,097 | 5,938.620 ms | 708.874 ms | 5,067.452 ms |

The wrapper timers enclose [FUSE acquisition/publication](../../../crates/layerfs-fuse/src/adapter.rs#L546).
Publication accounts for about 85% of append Exec. Candidate legacy
`publication_core_ns=0` means that old timer is not entered; it does not mean
publication is free.

Each accepted tiny callback performs the following work:

```text
one-byte mounted WRITE
  -> temporary p-* input: create, allocate, write, close
  -> open input and read its framed block
  -> merge/encode changed leaves; construct new immutable versions
  -> each active page: create, identify, allocate, write 4 KiB,
                      read back 4 KiB, verify, record readiness, close
  -> publish pack + selected directory + matching revision
  -> release replaced owners: open/check/close/unlink/account
  -> notification and FUSE reply
```

[PageStore creation](../../../crates/layerfs-workspace/src/backing/active/pages.rs#L361)
preallocates/checks the file, [writes and immediately reads it back](../../../crates/layerfs-workspace/src/backing/active/pages.rs#L448),
then authenticates and compares it. This readback does **not** increment
`active_index_fetches`/`active_pack_fetches`; those counters increment in the
separate [resolver read route](../../../crates/layerfs-workspace/src/backing/active/pages.rs#L514).
Thus 15 index fetches coexist with 20,768 full-page readbacks.

Source-derived traffic, from recorded successful page writes:

| Sampled schedule | Active page creations: index + pack | Active bytes written | Immediate readback bytes |
| --- | ---: | ---: | ---: |
| Append | 20,768 | 81.125 MiB | 81.125 MiB |
| #248 separated | 21,084 | 82.359 MiB | 82.359 MiB |
| Dispersed | 30,488 | 119.094 MiB | 119.094 MiB |
| Repeated | 8,192 | 32 MiB | 32 MiB |

These are requested file I/O bytes deduced from code and counters, not measured
physical-device traffic. They exclude ordinary fetches, temporary input and
cleanup reads. Append also creates 4,096 temporary 4 KiB input owners, reads
them in publication, and ordinarily reopens/reads/unlinks 4,095 released inputs.
It performs 20,488 active retirement inspections. See
[tiny input reading](../../../crates/layerfs-workspace/src/filesystem/active_file.rs#L142)
and [physical release](../../../crates/layerfs-workspace/src/backing/active/pages.rs#L584).

Retained allocation shrinks because old versions are released, but cumulative
I/O does not shrink to retained allocation. Packing 80 records per page still
writes the current whole page once per acknowledged byte; it does not batch
80 user syscalls into one physical publication.

There is also repeated fit merging, actual merging, encoding and decode-back
into the cache. [Fit check](../../../crates/layerfs-workspace/src/backing/active/hot_path.rs#L326)
and [publication merge](../../../crates/layerfs-workspace/src/backing/active/splice.rs#L613)
repeat related work; [cached decode](../../../crates/layerfs-workspace/src/backing/active/splice.rs#L405)
allocates record vectors and repeats verification. Their independent time is
not measured, so physical I/O cannot yet be assigned all 5.075 seconds.

Existing daemon process-shared CPU windows show append Exec 1.71 s user +
1.26 s system CPU; repeated 2.06 + 0.96 s; dispersed 3.53 + 1.90 s. These are
sampled process totals, not per-function profiles or cgroup peaks. They show
substantial CPU/kernel work too. Wall minus process CPU is not an isolated
I/O-wait measurement with concurrent threads and sampled boundaries.

Removing the acquisition wrapper alone has an observed upper-bound model of
5.964 / (5.964 - 0.723) = **1.138x**, assuming all other costs unchanged. It
cannot produce a 10x overall improvement. Its publication-side read is extra
and currently unsplit. The main diagnostic target is publication itself.

## 4. Dispersed Commit loses pack locality

At 512 dispersed writes, SDK Commit grows from 91.256 to 193.840 ms;
SaveFile grows from 74.016 to 164.748 ms, explaining 90.732 ms of the
102.584 ms increase. At 4,097 candidate writes, daemon WorkspaceCommit is
1,801.887 ms and SaveFile is **1,638.335 ms**, about 91%. Current records do
not split source reading from unchanged canonical construction within SaveFile.

The [upload reader](../../../crates/layerfs-workspace/src/commit/active.rs#L139)
emits final extents in file-offset order. Packs were populated in chronological
write order. [ActivePackReader](../../../crates/layerfs-workspace/src/backing/active/reader.rs#L40)
keeps one page; a different logical page requires a fresh P-key lookup and full
pack read/decode. [Snapshot get](../../../crates/layerfs-workspace/src/backing/active/index.rs#L657)
constructs an uncached resolver, so every miss reopens and decodes the locator
path. [Pack decoding](../../../crates/layerfs-workspace/src/backing/active/pack.rs#L125)
allocates all records, including records not needed by that pull.

Exact schedule arithmetic predicts:

| Writes | Distinct pack pages | One-page misses / P lookups | Pack records decoded | Whole pack bytes read | Append misses |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 2 | 41 | 2,080 | 167,936 | 2 |
| 512 | 7 | 512 | 39,424 | 2,097,152 | 7 |
| 4,097 | 52 | 4,097 | 326,689 | 16,781,312 | 52 |

These are source-derived predictions, not measured post-Commit counter deltas.
They have a falsifiable oracle in DERIVED.json. At 4,097, packing still reads
one full page per byte, adds P-root lookups, and repeatedly decodes the same
52 pages. An eight-page LRU would reduce 512-write misses to seven, but leaves
**all 4,097 misses** on the larger exact schedule. Enlarging the cache to fit
one test is not a general locality solution or permission to widen a bound.

Both arms' C1 work is identical at paired dispersed counts: 2,028/10,532
nodes read at 100/512, 7/18 nodes created, 100/512 payloads, and 303/1,539
input record lookups. Candidate 4,097 has 84,982 C1 visits and 4,097 payloads.
The unchanged canonical work is substantial; it does not explain the paired
regression through a different construction count. The native client
[coalesces short source pulls](../../../crates/layerfs-bridge/src/adapters/native/client.rs#L94);
this is not one network frame per one-byte extent.

## 5. A real quadratic term remains in reconcile

[`live_refs`](../../../crates/layerfs-workspace/src/backing/active/reclaim.rs#L20)
uses `updates.iter().any(...)` over the entire patch for each logical page.
[`prune_dead`](../../../crates/layerfs-workspace/src/backing/active/reclaim.rs#L90)
calls it for every touched pack. For these unchanged-G2 dispersed Commit rows,
all old R entries are deletions, so the first search cannot stop early.

With W writes: E deletions=2W+1, R deletions=W, I/D updates=2,
initial M=3W+3, P=ceil(W/80). Each P deletion enlarges the patch. Exact checks:

```text
P*M + P*(P-1)/2
  W=100:       607
  W=512:    10,794
  W=4,097: 640,614
```

The cost is Θ(P*M), or Θ(W²/80) for this shape. This is outside the hot-WRITE
bound and violates a claim of universally linear Commit bookkeeping. It occurs
after SaveFile, so it cannot be the main explanation for SaveFile's 1.638 s.
The minimal algorithm correction is to use the BTreeMap's existing prefix
range instead of scanning unrelated patch keys, retaining the subsequent
selected-index/pin/custody checks. No implementation was changed in this audit.

Other residual work includes two captured extent scans, descriptor/extent
materialization, O(M log M) patch insertion, generic bulk deletion and capture
release. Candidate append has about 126.7 ms of daemon Commit outside SaveFile;
#248 has about 194.8 ms. These are residual buckets, not measured reconcile
times. Mixed-page compaction is not the cause for these no-G2-edit single-file
rows: prune removes P and [compaction skips deleted locators](../../../crates/layerfs-workspace/src/backing/active/compaction.rs#L364).

## 6. Measurement and coverage corrections

The evidence audit verified 303 checksummed attempt files without mismatch;
source/artifact/workload/oracle/worker/fixture identities match the declared
pairing. That validates custody, not the following cache or coverage claims.

[cache_files](../../../benchmark/fs-bench-pro/checkpoint5_273.py#L341) evicts and
checks residency, then [digest](../../../benchmark/fs-bench-pro/shell_package.py#L28)
reads every Store/history byte afterward, before launch. Recorded zero pages
therefore do not establish zero pages at launch. Hash first, invalidate last,
and prohibit payload reads after the final residency check. Preserve all old
receipts; the earlier claim of demonstrably cold Exec input is unsupported.

The report's separate private Linux page-cache explanation is also too broad:
[private file create/open](../../../crates/layerfs-workspace/src/backing/segments.rs#L222)
requests O_DIRECT in both arms. Linux defines direct I/O as bypassing its
file-data page cache ([kernel documentation](https://docs.kernel.org/filesystems/iomap/operations.html#direct-i-o)).
This is source evidence, not proof of runtime handling, metadata cache state,
Docker VM/backend/device state or host SQLite state. Those domains must be
separated; no current row is promoted to eligible.

Other confirmed gaps:

- Clean Commit correctly returns UpToDate, but
  [the driver accepts only Committed](../../../crates/layerfs-api/sdk/examples/benchmark_shell.rs#L219).
- A one-write command emits one reachable progress checkpoint;
  [the parser requires four](../../../benchmark/fs-bench-pro/separated_writes.py#L467).
- Control dispersed-512 has an interleaved, unparseable final WRITE sample;
  the candidate #248 shape emits no mandatory C1 edit-load record.
- Quick controls start a fresh Workspace from committed Store/history. They
  do not retain a live/pinned private 4,097-record journal in that Workspace;
  preparation was also moved outside complete lifecycle wall. This does not
  prove [the original retained-journal contract](ACTIVE-FORMAT-AND-EVALUATION-v1.md#L265).
- Three sequential Execs before one Commit remain unexecuted at the registered
  shape; mapped process-continuity proof is partial coverage.
- No cgroup memory domains or phase-local RSS bound were recorded.
- Diagnostic overhead is asymmetric: repeated-4,097 baseline emits 4,097
  extent-splice lines and 1,068,995 captured stderr bytes; candidate emits no
  old-splice lines and 42,868 bytes. Equal diagnostic switches do not imply
  equal observer cost. Its timing contribution is unmeasured.

The summary also has arithmetic errors: eight of nine completed wall pairs
are faster for candidate, not all nine; clean's complete wall is faster even
though its attributed subtotal is slower; actual statuses are 15 INELIGIBLE,
four INCOMPLETE and five FAIL. Small n=1 differences cannot be declared noise
or repeatability findings. These corrections qualify interpretation without
rewriting any archived row or historical result.

## 7. Next work, in causal order

1. Correct harness hash/eviction ordering, outcome/progress/zero-counter
   representation, retained-journal fixture lifecycle and observer accounting.
   Freeze a prospective corrected identity/version before any new comparison;
   it cannot replace or relabel campaign-3.
2. Repair the full-patch prefix scan using bounded relevant-key traversal.
   Prove exact liveness/deletion, quota refusal and selected-view custody.
3. Add one labelled causal diagnostic for the active upload: actual pack misses,
   P lookups, records decoded, index reads and source time, with equivalent
   baseline accounting. Check the 41/512/4,097 prediction. Optimize locator
   reuse and record/window access within charged bounds; an eight-page cache
   alone does not solve the 4,097 schedule.
4. Split publication time into encode/fit/merge/cache decode, create/stat/
   allocation, direct write, readback/verification and release. Count native
   file syscalls where available. This determines whether the next change is
   CPU reuse or the physical page/file lifecycle. Preserve validation, atomic
   acknowledgement, pin/refund and unknown-outcome rules.
5. Treat borrowed tiny input as a secondary opportunity, and generic-path
   admission/normalization turnover as a separate locality problem. Do not
   widen hot/pack limits, add workers, relax cache rules or defer charged work
   beyond acknowledgement to produce a better timer.

The expected benefit of future changes must be derived from those measured
regions, not a claimed factor from Big O. A small reclaim fix cannot remove
SaveFile's cost; acquisition-only removal cannot remove publication's cost.
The optimized hot seeking is a useful foundation, but the current physical
publication unit and scattered-read path still determine end-to-end latency.
