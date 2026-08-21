# CP-0010 — FastCDC v2 and current Phase-4 grind checkpoint

Status: `PASS / RETAIN-FASTCDC-V2 / PHASE-4-ACTIVE`
Date: 2026-08-21
Parent checkpoint: CP-0009
Experiment mode: accepted successor plus independent confirmation
Primary operation: 100-MiB durable full create
Fresh independent campaign wall through terminal verification: `9.953490458 s`
Fresh independent rows: `20/20 complete; 16 measured`
Transient databases retained: `yes, only inside the sealed versioned evidence root`

## 1. Executive checkpoint

The project remains in **Phase 4: core storage-algorithm optimization**. It has
not started WP5, Phase 5, production integration, or application cutover.

The current position is:

```text
Phase 4 full grind
  -> WP4-M/WP4-P compatibility profile complete
  -> M4.5/F0/F1/F2 durability and publication controls complete
  -> F3 grouped-write candidate rejected and reverted
  -> Canonical-v2 complete validation PASS / frozen
  -> FastCDC contiguous-region kernel v2 PASS / retained
  -> independent FastCDC rerun PASS / retained
  -> NEXT: explicit SQLite writer-memory policy
  -> THEN: materialization/read-path qualification and bounded concurrency
  -> OPEN: H09 count-change locality, reopen authority, native/cold materialization
```

FastCDC v2 closes the serial safe-Rust exact-boundary CDC sublane. The current
full-create lane has demonstrated `301.179778 MiB/s` for the exact private
100-MiB durable capture/publication boundary. Phase 4 remains open because the
writer still peaks near 89 MiB RSS, true cold/native materialization is
unmeasured, the hot read path lacks a cache-trust equivalent, reopen authority
remains linear, and 500-MiB count-changing mapping remains suffix-sensitive.

## 2. Checkpoint identity

| Field | Value |
|---|---|
| Repository | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` |
| Branch | `codex/empty-worktree` |
| Starting committed HEAD | `daf4cefc1fd7861681de3f94bf042b556cc21ccb` |
| Candidate state | dirty working tree; not committed |
| FastCDC candidate diff SHA-256 | `72ed9fee8e6a203a15d88df8e1c555f13a52a8ed4f0ef5eabdad742e0b8a3d76` |
| FastCDC candidate source SHA-256 | `bc0346eec113914943d046a4ab4742420acfff570d6b00115082c40bdf8e58b6` |
| Historical Canonical-v2 control executable | `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280` |
| FastCDC v2 executable / next control | `454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8` |
| Profile | `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b` |
| Fixture SHA-256 | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| FastCDC successor manifest SHA-256 | `f64a484c7966d17f7e1af2ebc8a91c58248605e28d29c9d0d750ded93f951e38` |
| Independent methodology SHA-256 | `fa6c22fc8db15fa04aad0a386fe54d5cf6395635a65190b566156c36db56332c` |
| Independent direct raw / analysis | `12493a20...6b51` / `84387881...393b` |
| Independent durable raw / analysis | `cb4b0a18...9bc3` / `d0d85c98...9c53` |
| Independent payload manifest | `9d6953bcdc3d8b476452b0e3646a04151d7ebc3a345dcb5cb7ccdfa9a481b713` |
| Independent terminal / verification | `58eec75a...e5a4` / `0120f98a...2b26` |
| Host | Apple M3 Max; macOS 26.4.1 (25E253) |
| Rust / SQLite | Rust 1.96.0 / SQLite 3.51.0 |

The current dirty state is identified by the committed HEAD, exact candidate
source/diff, executable, profile, baseline manifest, and sealed evidence. It
must not be attributed to `HEAD` alone.

## 3. What FastCDC v2 changed

The accepted scanner preserves the exact Gear table, recurrence, small/large
masks, first-byte and second-byte boundary tests, 8/16/32-KiB profile,
fragmentation semantics, callback errors, and 32-KiB bound.

It changes only the execution shape:

```text
control:
  inspect pending/region/fields and extend Vec for essentially every pair

candidate:
  fill to minimum
  -> resolve pending once per fragment
  -> scan one fixed small-mask region
  -> scan one fixed large-mask region
  -> append one accepted span at cut/maximum/fragment exit
```

The machine-code preflight confirms masks remain in registers, the region loop
contains no target comparison or recurring mutable-mask load, and the exact
Gear updates and cut tests remain.

An independent semantic audit modeled 53 input shapes under eight
fragmentation schedules. All `424/424` old/new transcript and byte comparisons
matched, including empty, one-byte, `MIN/TARGET/MAX +/- 1`, first-byte cuts,
second-byte cuts, forced maximums, and callback failures.

## 4. 100-MiB durable full-create history

Historical centers are context only. Candidate effects are established by
their own adjacent A/B campaigns.

| Checkpoint | Durable wall | Throughput | Status |
|---|---:|---:|---|
| CP-0009 current-product v1 | 640.109209 ms | 156.223 MiB/s | historical v1 control |
| Canonical-v2 frozen | 512.214000 ms | 195.231 MiB/s | accepted identity/profile baseline |
| FastCDC v2 original retained campaign | 339.748094 ms | 294.336 MiB/s | accepted successor |
| FastCDC v2 independent confirmation | **332.027604 ms** | **301.179778 MiB/s** | fresh sealed confirmation |

The fresh independent campaign used one warmup `AB` and measured
`AB / BA / AB / BA` for both direct and durable stages. It completed all 20
planned rows, retained 16 measured rows, performed no build, retry, resume,
row deletion, or selective repair, and sealed the result within 9.954 seconds.

### Fresh independent durable pairs

| Pair | Order | Control | FastCDC v2 | Saving |
|---:|:---:|---:|---:|---:|
| 1 | AB | 399.208666 ms | 332.202041 ms | 67.006625 ms |
| 2 | BA | 398.026083 ms | 330.972541 ms | 67.053542 ms |
| 3 | AB | 394.818083 ms | 332.331375 ms | 62.486708 ms |
| 4 | BA | 402.168458 ms | 332.604458 ms | 69.564000 ms |
| **Position-balanced center** | — | **398.555323 ms** | **332.027604 ms** | **66.527719 ms / 16.692217%** |

All four pairs and both positions favored the candidate. All four measured
candidate rows individually exceeded 300 MiB/s. The exact 300-MiB/s boundary
is 333.333333 ms; the independent center crossed it by 1.305729 ms.

### Fresh phase breakdown

| Phase | Adjacent control | FastCDC v2 | Candidate change |
|---|---:|---:|---:|
| Canonical CAS + mapping | 287.271760 ms | 214.836823 ms | **-72.434938 ms** |
| Proof | 0.043271 ms | 0.043979 ms | +0.000709 ms |
| SQLite durable COMMIT | 111.240292 ms | 117.146802 ms | **+5.906510 ms slower** |
| Durable total | 398.555323 ms | 332.027604 ms | **-66.527719 ms** |

The candidate won despite a slower sampled COMMIT. The effect is localized in
the only phase containing CDC, and user CPU fell from 0.350 to 0.280 seconds.
This is evidence of a construction-CPU improvement, not weakened durability.

### Exact full-create work

Every durable row retained:

```text
source / CDC bytes                 104,857,600
CDC references                           5,284
objects created / reused              5,372 / 0
canonical bytes written             105,122,466
mapping bytes                            196,174
SQL calls                                  5,381
BLOB writes                               10,748
transactions / COMMITs                     1 / 1
Q high-water / terminal                86,181 / 0
post logical DB bytes                109,199,360
post logical store bytes             109,199,392
post allocated store bytes           117,510,144
```

All rows retained rollback journal `DELETE`, `synchronous=FULL`,
`temp_store=FILE`, `mmap_size=0`, exact timer equations, zero publication graph
rescan, and no journal/WAL/SHM residue.

## 5. Current lifecycle scoreboard

Only full create was freshly rerun after FastCDC v2. Other values are the last
validated Canonical-v2 lifecycle evidence and must not be relabeled as new
FastCDC measurements.

| Operation | Wall | Rate where meaningful | Evidence scope |
|---|---:|---:|---|
| Durable 100-MiB full create | **332.027604 ms** | **301.179778 MiB/s** | fresh independent FastCDC rerun |
| Same-open same-count middle edit | 6.960791 ms | latency | Canonical-v2 predecessor |
| Same-open `+1` early / middle | 5.108458 / 4.576000 ms | latency | Canonical-v2 predecessor |
| One-byte early / middle / late | 6.410375 / 6.414750 / 6.725166 ms | latency | Canonical-v2 candidate guards |
| First edit after reopen | 154.019083 ms | lifecycle latency | Canonical-v2 candidate guard |
| Warm authenticated logical reconstruction | 338.775916 ms | 295.180369 MiB/s | timed second pass after priming |
| Fresh-process logical reconstruction | 366.356667 ms | 272.958046 MiB/s | OS cache warm-or-unknown |
| Full scrub/authentication | 176.882750 ms | 565.346253 MiB/s equivalent | no output or write |
| Reopen / visible head | 2.088334 ms | latency | process launch excluded |
| Authenticated returned 1-MiB range | 2.279209 ms | 438.748706 MiB/s | exact returned bytes |

The older CP-0008 diagnostic remains the scale warning: 500-MiB `+1` early
and middle edits measured 27.140916 and 15.102042 ms same-open, while the first
edit after reopen was approximately 1.23-1.26 seconds. Those numbers are not
the current binary, but they establish suffix-sensitive mapping and linear
reopen authentication as still-open structural problems.

## 6. Materialization vocabulary and current limitation

The project has no proven physically cold materialization result.

```text
warm logical reconstruction:         338.775916 ms / 295.180 MiB/s
fresh-process logical reconstruction: 366.356667 ms / 272.958 MiB/s
proven cold native materialization:   Unavailable
```

`materialize-warm` performs one untimed priming reconstruction and then times
the second reconstruction. `materialize-fresh` opens a fresh process and
SQLite connection, but the OS/filesystem cache remains `warm-or-unknown`.

The timed warm pass is validation-grade work, not a cache-trust read. It
performs approximately:

```text
170 SQL queries
5,371 returned object rows and BLOB reads
5,284 chunk BLOB reads
104,926,292 borrowed chunk bytes
105,122,401 canonical bytes authenticated
5,371 canonical identity hashes
complete mapping and summary validation
complete closure digest
complete ordered occurrence commitment
complete reconstructed-source fingerprint
```

It streams raw bytes into hashers and counters. It does not create, write,
fsync, or atomically publish a native destination file. Therefore the current
name means authenticated logical reconstruction, not user-visible native
checkout.

## 7. Comparison with the other `layerfs` implementation

The public repository's current README reports:

| Public README metric | Baseline | M2 | M3 |
|---|---:|---:|---:|
| Cold 100-MiB write | 17.9 MiB/s | 44.2 MiB/s | 60.0 MiB/s |
| Cold 100-MiB read | 43.8 MiB/s | 118.1 MiB/s | 259.6 MiB/s |
| Warm 100-MiB read | 44.4 MiB/s | 118.6 MiB/s | 2,921.5 MiB/s |
| **100-MiB materialization** | 33.8 MiB/s | 67.9 MiB/s | **108.5 MiB/s** |

Sources:

- <https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/README.md>
- <https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/benchmarks/m2-minibench.md>
- <https://github.com/Ephemeral-AI-Lab/layerfs/blob/main/docs/evidence/m3/exit.md>

The 2,921.5-MiB/s number is **not materialization**. The public benchmark
defines A3 as verification-bound first read, A4 as a `cache-trust path`, and A7
as reopen-then-read-all materialization. Its M3 profile explicitly allows a
128-MiB engine content cache that can hold the complete 100-MiB fixture, a
128-MiB SQLite page cache for the approximately 115-MiB database page set, and
a 192-MiB managed-resident envelope.

The public benchmark also states that `cold` means cold **engine** cache, not
cold operating-system cache; OS and SQLite caches are warm after untimed setup,
and operating-system cache dropping is unavailable. Its M3 exit record reports
a different exact-candidate checkpoint (306.3 MiB/s first read and 840.5 MiB/s
warm), so README headline and exit-record values must not be merged into one
campaign.

### Apples-to-apples conclusions

1. Our 295.180-MiB/s authenticated logical reconstruction is approximately
   `2.72x` the public README's 108.5-MiB/s M3 materialization result. Our
   materialization is not generally slower than that implementation's
   materialization.
2. Our path is approximately `9.90x` slower than the public 2,921.5-MiB/s
   **cache-trust read**, because we do not have an equivalent whole-file hot
   content cache and we reauthenticate/recompute validation digests on every
   reconstruction.
3. The public hot result is purchased with large explicit cache budgets. It is
   not evidence that a bounded low-memory materializer should reach 2.9 GiB/s
   without a different trust/cache policy.
4. Our current full-create writer RSS is approximately 89 MiB even without a
   payload cache, so copying the public 128+128-MiB cache profile would conflict
   with the present memory objective unless separately authorized and bounded.

## 8. Benchmark and product gaps now exposed

The read/materialization lane needs three explicitly different operations:

```text
authenticated logical reconstruction
  existing path; reauthenticate and recompute complete validation

trusted hot read
  repeated read under a versioned receipt/cache authority

cold native materialization
  open/authenticate, write destination bytes and metadata, fsync/publish
```

They must not share one label or one performance claim.

The first fast experiment should be a benchmark-private phase decomposition,
not a production cache:

```text
SQLite mapping/BLOB acquisition
canonical ObjectId authentication
closure digest
raw reconstructed-source fingerprint
ordered occurrence commitment
output/destination work
```

The diagnostic may compute omitted validation after the timer for exact parity.
It must not weaken the production authority path. A `<20 s` screen is enough
to identify whether the approximately 339-ms warm wall is dominated by
SQLite/BLOB acquisition or redundant validation hashes.

A later trusted-hot design must bind any receipt/cache entry to at least the
store instance, profile, integrity epoch, visible root/head generation, object
or file commitment, and downgrade/fresh-reopen behavior. A cache is an
authority mechanism, not merely a performance map. Because a full 100-MiB
payload cache is expensive, prefer qualification of compact verified receipts
and a native/APFS seed or OS cache before defaulting to a 128-MiB application
payload cache.

## 9. Incremental materialization is a first-class product workload

Files in the intended workspace are expected to change frequently. Repeatedly
performing a full authenticated reconstruction after every small edit would
discard the locality that CAS, CDC, COW mappings, and authenticated deltas
already establish.

The materialization lane therefore has a mandatory base-relative operation:

```text
previously materialized native file at authenticated root R0
  + authenticated transition/delta R0 -> R1
  + exact changed chunk/range set
  -> native file at R1
```

This is **incremental materialization**. It is different from both the current
full logical reconstruction and a cached read.

### Required fast paths

```text
target root == receipt root:
  validate authority and return no-op

same-size/same-count edit:
  clone or stage the verified base
  -> patch changed ranges only
  -> fsync and atomically publish

count-changing edit:
  reuse the verified prefix and any filesystem-supported unchanged extents
  -> rebuild the shifted suffix honestly
  -> fsync and atomically publish

invalid/missing/gapped authority:
  full authenticated reconstruction fallback
```

For a flat native file, a byte insertion or deletion changes subsequent byte
offsets. Without a proven filesystem range-clone/insert primitive or a
segmented mounted view, early count-changing edits retain an unavoidable
shifted-suffix cost. Do not claim file-size-insensitive native updates from CDC
rejoin alone.

### Authority contract

An incremental update is eligible only when the destination has a protected
receipt binding at least:

```text
store instance and profile
integrity epoch
base visible root/head generation
file commitment and logical length
destination identity and metadata policy
last materialized transition
external-mutation continuity
```

A receipt proves what LayerFS last wrote; by itself it does not prove that
another process has not since modified the destination. A LayerFS-owned mount
can mediate all mutations. An arbitrary native path needs operating-system
mutation continuity, with any event gap or identity mismatch forcing full
fallback.

### Preferred local implementation hypothesis

For APFS, the first bounded prototype should investigate a verified native
seed rather than a 100-MiB application payload cache:

```text
verified native base
  -> clone to private temporary destination
  -> patch authenticated changed ranges
  -> apply metadata
  -> fsync file
  -> atomic rename/publication
  -> persist new receipt
```

This may make same-size changes proportional to changed ranges while allowing
the operating-system page cache to serve hot reads. It also preserves a safe
old destination until atomic publication. APFS clone behavior, crash ordering,
external mutation, cleanup, and actual changed-block allocation must be
measured rather than inferred.

### Complexity targets

```text
trusted hot range read:
  O(requested bytes)

no-op materialization:
  O(receipt/authority validation)

same-size incremental materialization:
  O(changed ranges + changed bytes + publication durability)

count-changing flat-file materialization:
  O(changed CDC window + shifted native suffix + durability)

full fallback:
  O(file bytes + mapping/object authentication)
```

Long edit histories must not be replayed one transition at a time. The
materialization receipt identifies the actual base root, and the candidate
must derive or bound a direct base-to-target changed-range plan. If the delta
chain, changed-range count, or affected bytes exceed a fixed limit, it falls
back to full reconstruction.

### Fast benchmark matrix

The first benchmark-private prototype should remain under the fast-iteration
budget and cover:

| Workload | 1 MiB | 10 MiB | 100 MiB | 500 MiB deferred |
|---|---:|---:|---:|---:|
| receipt-valid no-op | yes | yes | yes | — |
| one-byte same-size early/middle/late | smoke | smoke | primary | — |
| 1-MiB same-size replacement | smoke | smoke | primary | — |
| `+1/-1` early/middle/late | smoke | smoke | primary | — |
| invalid receipt / external mutation | fallback | fallback | primary | — |
| full fallback | smoke | smoke | control | — |

The 500-MiB column is explicitly deferred. CP-0008 remains historical scale
evidence only; no new 500-MiB cell is authorized by this checkpoint.

Every candidate must verify exact native bytes and metadata, root/transition,
receipt authority, changed-range equations, crash cleanup, atomic destination
publication, allocated bytes, RSS/Q, and terminal residue. Benchmark-native
read speed and incremental-update speed separately; do not call a fast hot read
an incremental update.

This requirement changes the materialization priority: after the writer-memory
policy and phase-decomposition screen, same-size incremental materialization is
the first implementation candidate. A general whole-file payload cache is not.

## 10. Resources and unresolved writer memory

Fresh FastCDC durable resource centers:

| Resource | Adjacent control | FastCDC v2 | Result |
|---|---:|---:|---|
| User CPU | 0.350 s | 0.280 s | improved 20% |
| System CPU | 0.110 s | 0.110 s | unchanged |
| Maximum RSS | 93,442,048 B | 93,454,336 B | unchanged |
| Peak footprint | 92,197,296 B | 92,209,584 B | unchanged |
| Exact Q high-water / terminal | 86,181 / 0 | 86,181 / 0 | exact |

SQLite held an observed 87,050,240-byte page-cache snapshot before COMMIT in
the Canonical-v2 full-create evidence. The runtime exposes `cache_size=2000`
pages but `cache_spill=20000`, so the one large transaction retains roughly
20,000 dirty 4-KiB pages before spilling. This is not debug memory and is not
LayerFS `Q`.

The next one-variable screen should compare the exact FastCDC control with an
explicit `PRAGMA cache_spill=2000`, protecting wall, dirty writes/spills,
COMMIT, one transaction, `FULL+DELETE`, identities, storage, and terminal Q.
Retain only if RSS/cache fall by at least 50% with no more than 5% durable-wall
regression. If that fails, a separately preregistered `8192` compromise is the
only immediate fallback; do not add a tuning framework.

## 11. Decision

Decision: `RETAIN-FASTCDC-V2 / PHASE-4-ACTIVE`

Controlling facts:

```text
semantic source audit:         ACCEPT; 424/424 model comparisons exact
original direct screen:        100.485990-ms saving; 4/4 wins
independent direct screen:     100.647646-ms saving; 4/4 wins
original durable campaign:      74.666885-ms saving; 4/4 wins
independent durable campaign:   66.527719-ms saving; 4/4 wins
fresh durable center:          332.027604 ms / 301.179778 MiB/s
identities/work/durability:    exact PASS
Q/storage/residue:             exact PASS
writer RSS:                    approximately 89 MiB; unresolved
true cold/native materialize:  Unavailable
trusted hot-read lane:         absent
```

The exact next executable control remains:

```text
454bc2f3deacd8581a3cc352c8b7495215cdc103a85580606246ea12bb25eba8
```

## 12. Next actions

Remaining work is bundled into Phase-4 grind phases:

| Grind phase | Work | Gate |
|---|---|---|
| **G0 — freeze** | checkpoint retained FastCDC source, evidence documents, CP-0010, and scoreboard | clean exact control |
| **G1 — writer memory** | screen `cache_spill=2000`; select/reject byte-equivalent memory policy | RSS/cache `>=50%` lower and wall regression `<=5%`, or retain current policy honestly |
| **G2 — materialization research** | `<20 s` phase decomposition; destination receipt and external-mutation contract | one materialization candidate selected without weakening authority |
| **G3 — incremental prototype** | verified native seed; no-op, same-size one-byte, 1-MiB replacement, invalid-receipt and fault fallbacks | exact output/atomicity and clear changed-range signal |
| **G4 — materialization acceptance** | compact 1/10/100 matrix; trusted hot read and native/cold classification | complete materialization scoreboard frozen |
| **G5 — remaining core lanes** | reopen authority, H09/count-change locality, optional bounded create concurrency | separate one-variable retain/revert/defer decisions |
| **G6 — Phase-4 closure** | final audit, limitations, manifests, WP5 handoff | Phase 4 PASS or exact blockers |

Measured candidates remain serial inside each phase. G4 is forbidden when G3
fails, 500-MiB work is deferred, and bounded create concurrency is eligible
only if a materially sub-300-ms create target is selected after the
materialization lane.

Phase 4 is not complete until the full-create resource policy,
materialization/read disposition, reopen-authority disposition, and
count-change locality policy are explicitly closed or deferred with evidence.

## 13. Evidence

Local controlling evidence:

- [CP-0009 baseline](cp-0009-dirty-b073a7e04c7a-current-product-baseline.md)
- [Canonical-v2 baseline](../baseline/canonical-v2-baseline-v1.md)
- [FastCDC v2 baseline](../baseline/fastcdc-contiguous-region-kernel-v2-baseline-v1.md)
- [Phase-4 full-grind roadmap](../2026-08-21-phase-4-full-grind.md)
- `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2/`
- `target/phase4-fastcdc-contiguous-region-kernel-20260821-v2-independent-rerun-v1/results-v1/`

The target roots are sealed evidence and are not copied into this compact
checkpoint directory. This report creates no new performance sample and makes
no commit.
