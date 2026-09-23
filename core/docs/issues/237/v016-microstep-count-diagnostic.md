# #237: v0.1.6 count-only Init microsteps beside Core ImportBatch

**Status: one diagnostic operation, no speed treatment.** The original exact-release
public result remains 750.625833 ms on this source. This operation used an
instrumented build of the same v0.1.6 product and returned in 768.649083 ms;
that wall is diagnostic only and does not replace or average with the original.
The Core figures below come from separate instrumented runs and are not a
matched speed pair.

## Identity and cold source

The base product is peeled tag `v0.1.6` commit
`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`; its `crates` tree SHA is
`dcc4fb6fd01115dcbf91ba02df414e91eb5733be`. The only temporary product
edits were aggregate timers in `layerstack.rs`, `objects.rs`, and
`objects/admission.rs`, archived as [the exact diff](evidence/v016-micro/temporary-instrumentation.diff.gz)
(uncompressed SHA-256 `6d8160b482ab114cae2eb3c69f0ffc7b4b0bc279d3518954c2d7724d4ec3c834`).
The instrumented release binary SHA-256 was
`66f2e18f8e62b04bc3b0ce620550aaae682b7529cc9d35309a5fa9f874a9cd82`.
The existing benchmark-only READY/GO wrapper was unchanged, SHA-256
`5789a47ef189a465aede5b06b9f20c5f4c685fd32dece2bed4f317c07ab2f604`.

The sealed seed-1 10k/300,000,000-byte master manifest SHA-256 was
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.
The wrapper made a fresh independent byte copy outside the timer, created a
fresh Store, then used the shared cold driver to hash/invalidate and recheck
the copy before GO. [Preflight](evidence/v016-micro/raw/cold-preflight.json)
and [final recheck](evidence/v016-micro/raw/cold-recheck.json) each found
**0/27,503 resident source payload pages**; recheck-to-timer gap was
**0.358 ms**. Directory and inode metadata residency was not qualified.
The operation read **337,170,432 B** according to process disk-I/O counters.
In-timer verification was **SKIPPED**; status is `DIAGNOSTIC`, never admission
`PASS`. [Raw receipt](evidence/v016-micro/raw/receipt.json),
[stderr](evidence/v016-micro/raw/stderr.txt), and
[evidence hashes](evidence/v016-micro/evidence-manifest.json) retain the data.
The closed Store had 4,096-B SQLite pages, 24,683 object rows, 1,712 pack
rows and 301,647,077 B of pack BLOB data; its SHA-256 was
`c734a31da84ee94dd650f7e5c963b3e04411c6d57b02ef59173bbecc20fefcbb`.
The native small-content cutoff was 128 KiB. Pack payload stayed inside
SQLite BLOBs.

## Side-by-side owner and producer path

The Core column is the separate [ImportBatch pipeline residual diagnostic](connection-slot-experiment.md#residual-diagnosis-against-v016)
on reorganized Core source; its public receipt lost daemon telemetry and was
`INCOMPLETE`. Core signature/SQL details are from yet another
[count-only diagnostic](signature-owner-diagnostic.md).
Do not combine their times into a predicted speedup.

| 10k / 300 MB measure | Instrumented v0.1.6 | Instrumented Core ImportBatch | Boundary / reading |
| --- | ---: | ---: | --- |
| Broad pipeline or file loop | **727.788 ms** | **951.323 ms** | Old starts before producer spawn and ends after channel close/join/admission resolve; Core starts before scoped workers and ends after 10k Done events/join. Raw difference 223.536 ms. |
| Owner callback / `accept` | **661.613 ms** | **752.757 ms** | Old times 1,202 slab `admit_page` callbacks; Core sums per-object C2 `accept` calls. Raw difference 91.144 ms; admission APIs differ. |
| Receiver blocking wait | **65.862 ms** | **197.262 ms** | Old times every `recv()` including terminal close; Core times successful `recv_timeout()` calls before the last Done. Raw difference 131.400 ms. |
| Receive batches | **1,202** | **1,204** | Both bounded four-slot queues. Core batch carries Object and Done; old slab carries objects, while completion is joined separately. Message count is now similar. |
| Four producer wall sum / maximum | **2,654.217 / 726.482 ms** | **3,803.686 / 951.226 ms** | Workers overlap; maxima nearly reach each pipeline endpoint. |
| Producer send time sum | **812.091 ms** | **2,055.669 ms** | Old counts only send after `try_send` reports Full; Core times all `send` calls. These are different metrics, mostly queue backpressure, and must not be subtracted as a measured improvement. |
| Producer wall minus respective send measure | **1,842.126 ms** | **1,748.017 ms** | Crude, noncomparable four-worker remainders; neither isolates construction or source I/O. |
| Source read calls / bytes | **27,951 / 300 MB** | **29,952 / 300 MB** | Old source read syscall wall unmeasured; Core summed 1,057.566 ms across four workers plus 260.079 ms for file open/stat. |
| Small-file signature | **9,399 / 33.747 MB / 96.907 ms on four producers** | **9,399 / 33.747 MB / 96.164 ms on one C2 owner** | Same algorithm and fixture. Worker time overlaps other workers and Store admission; the ~96-ms owner placement difference is a hypothesis, not an additive public saving. |

Old per-worker `(files, source bytes, wall ms)` was `(3100, 42.410 MB,
643.063)`, `(800, 141.628 MB, 640.896)`, `(2900, 73.518 MB, 726.482)`,
and `(3200, 42.443 MB, 643.776)`. Core's residual diagnostic assigned
119 files / 139.236 MB to the anchor worker and 3,157–3,377 files / roughly
53–54 MB to the others; all four Core worker walls were about 950–951 ms.
The old maximum worker wall was only 1.305 ms below pipeline completion; the
Core maximum was 0.097 ms below its file-loop completion. Both broad spans
therefore ended when their last producer finished, within these observed
boundaries. The 131.400-ms receiver-wait difference describes when batches
became available; it is not a per-message channel cost. Because the old wait
also includes terminal close and Core's does not, the timer asymmetry points
against explaining the higher Core wait by broader Core scope. The producer
task granularity and file allocation differ, while old read/open wall is
unknown; this evidence cannot isolate why Core's batches arrived later.

## SQLite admission microsteps

These old timers are nested where indicated. `admission_prepare_ns` and
`admission_insert_ns` aggregate pipeline plus final publication; the receiver
callback covers only pipeline slabs. Do not add the aggregate columns to that
callback or to the public wall.

| Old v0.1.6 aggregate | This operation | Relation to Core evidence |
| --- | ---: | --- |
| Prepare selected objects into bounded packs | **296.654 ms** | Core has different selection/placement scopes; no identical prepare timer. |
| Whole `insert` call | **132.273 ms** | Contains pack/locator SQL below, open-pack appends, pack planning and sorting. |
| New-pack multi-row INSERT SQL | **57.623 ms** | Core's separate signature diagnostic: `write_pack_total_ns` 82.282 ms, which also includes placement/append work and is not the same scope. |
| Sorted object-locator INSERT SQL | **54.690 ms** | Core separate diagnostic: `insert_objects_ns` 68.078 ms, with a different schema and Save visibility checks. |
| SQL COMMIT wall | **218.709 ms pipeline; 2.265 ms publication** | Core separate signature diagnostic: 246.451 ms for file Save, plus later prerequisite/tree Saves. Different transaction scopes. |
| Writer-lock acquisition/release | **0.050 / 0.023 ms** | Old Store owns one long-lived SQLite connection; these values are mutex-guard operations, not connection open/close. Core's file Save connection close was 104.414 ms in its pipeline residual diagnostic. |
| Exact traced BEGIN / COMMIT | **75 / 75** | 73 pipeline admission, one publication, one serial-reservation transaction. |
| Object INSERT / pack INSERT / open-pack UPDATE statements | **639 / 591 / 398** | Core separate signature diagnostic: 1,393 object INSERTs, 1,259 pack creations, 1,151 changed-span appends for file Save; SQL coverage and append mechanisms differ. |

The new-pack and locator SQL timers sum to **112.313 ms inside** the
132.273-ms old insert timer. Its **19.960-ms residual is only an upper bound**
for all open-pack UPDATE calls together, since it also includes pack planning,
queries, metadata inserts, sorting and other loop work. The old trace counts
398 open-pack UPDATE statements, but this diagnostic did not separately time
them. The old Store retained its SQLite connection across the public Init;
the later external Store/client teardown lies outside that timer. Core's
private file Save connection was released before its Init returned, and its
own diagnostic measured that release separately. The old writer-lock numbers
must not be compared directly with Core connection destruction.

## Reproduction and decision

Exactly one operation ran, using the unchanged wrapper and the instrumented
release binary, with the command below from the isolated v0.1.6 worktree.
Its output path was absent beforehand. The master was used for untimed copy
preparation only; the public operation read its own copy. No readback was run
for this temporary diagnostic; the earlier unmodified release arm passed a
separate full 10k/300-MB readback.

```sh
python3 benchmark/fs-bench-pro/issue237_v016_reference.py \
  --master /Users/yifanxu/.codex/worktrees/2776/layerfs/benchmark-results/fs-bench-pro/prepared/namespace-10000-9f0c701648528472 \
  --cold-driver /Users/yifanxu/.codex/worktrees/2776/layerfs/docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py \
  --binary target/release/fs-benchmark-pro \
  --out benchmark-results/issue237-v016-micro-count-only-a
```

The side-by-side points first to **serial owner work and producer arrival
pacing**, not a remaining channel-message-count gap: both sides now receive
about 1,200 batches. Signature placement plausibly accounts for much of the
~91-ms owner-work difference, but the producer side and receiver-wait
difference need a matched algorithm treatment before a speed claim. The old
SQL microsteps bound pack/locator publication and show that reducing COMMIT
count alone is unlikely to remove the full raw gap. No product optimization
was selected from this diagnostic.
