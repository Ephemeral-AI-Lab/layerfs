# Phase 4 optimization decision map

> **Historical routing map — not current control.** This document records the
> research direction after CP-0009. It is superseded by the
> [Phase-4 full grind roadmap](../../implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md)
> and the sealed
> [G3-v13 report](../../implementation-detail/phase-4/experiments/g3-incremental-materialization/G3-REPORT.md).
> Current status is **G3 PASS / G4 READY — v13 STATICALLY CLOSED AND
> TERMINALLY SEALED**; G4 remains planning-only and UNSTARTED, Phase 4 remains
> incomplete, and G5/G6 remain pending.

Historical status: research direction after CP-0009. This roadmap does not itself
authorize a format, implementation, migration, or benchmark campaign. WP4-P is
complete and remains closed; K64/F64 + DIR256K is the current product profile.

## Executive answer

Phase 4 should iterate in this order:

```text
1. retain CP-0009; preserve the terminal H05/H05b/H05c evidence
2. restore the private benchmark source to CP-0009 exactly
3. run canonical-v2 authority/migration research and a nonpersistent shadow
4. explore one explanatory v2 variant at a time with <=120-second screens
5. keep reopen authority and count-change requirements as separate research
6. study exact CDC/hash execution, then residual SQLite physical tuning
```

H05 is closed as `MEASURED NO-GO / REVERT`: it passed semantic/work gates and
won `3/3` measured pairs with a `16.655343%` paired-median durable improvement,
but failed its frozen exact allocated-storage equality gate. H05b did not
justify changing that rule, and H05c established exact 100-MiB A/A allocation
equality in `6/6` pairs. CP-0009 therefore remains the accepted control.

Compact v2 is now the strongest full-create research direction because it can
remove the separate raw `ChunkId` pass and shrink mapping references. It is
not yet a speed forecast: the mandatory ordered commitment still costs work,
and v2 authority, profile dispatch, errors, receipts and migration remain open.

## Controlling evidence

| Item | Result | Interpretation |
|---|---:|---|
| CP-0009 durable full submit | `640.109209 ms` / `156.223 MiB/s` | Exact current-product control; candidate evidence requires adjacent A/B |
| CP-0009 construction / proof / COMMIT | `504.215417 / 0.038542 / 135.855250 ms` | Current full-create decomposition |
| Primary target | `500.000 ms` / `200 MiB/s` | Current contextual gap is `140.109209 ms` |
| Exact control executable | `9cda87ee...49d7` | Must be copied and hash-frozen before any candidate build |
| CP-0009 protected boundaries | edit `9.737 ms`; warm/fresh materialize `425.801/433.513 ms`; 1-MiB returned range `3.171 ms`; reopen `3.008 ms` | Guard neighboring product behavior |
| CP-0008 500-MiB `+1` early / middle | `27.141 / 15.102 ms` same-open | Fixed radix remains accepted under the current `<50 ms` policy despite `O(suffix)` work |
| CP-0008 first-after-reopen | `1.263 / 1.229 s` at 500 MiB | Separate linear authority problem; no safe bypass is authorized |
| F4-A diagnostic durable | `636.836792 ms` | Attribution only, not a new baseline |
| Raw `ChunkId` hash | `95.185147 ms` | Removable only under a new identity/profile |
| Construction source + sequence hash | `89.067215 ms` | Combined gross lane; source-only share unavailable |
| Canonical `ObjectId` hash | `96.068155 ms` | Required current CAS identity |
| Exact hash subtotal | `280.146626 ms` | `53.45%` of diagnostic mapping |
| CDC-exclusive | `128.723024 ms` | Required scan; exact-loop constants remain researchable |
| Encode / proof bookkeeping | `3.161540 / 1.306847 ms` | Too small to lead |
| F4-A2 scanner-copy removal | `3.701583 ms`, `0/5` at gate | Closed direction |
| Same-count edit | approximately `440.023 -> 9.134 ms` | Protect; do not redesign for full-create work |
| Logical reconstruction | `438.069792 ms` | Warm-or-unknown hashing sink, not cold/native materialization |

Row-by-row optimistic subtraction of the raw-ID and entire combined
construction lanes gives `427.084–454.849 ms`, median `452.873 ms`
(approximately `220.8 MiB/s`). That is a ceiling only: it credits removal of
the whole combined construction interval even though a replacement ordered
commitment is necessary.

## Direction tree

### A. Canonical authority — first

#### A1. Whole-source authority — research question closed

The adversary review concludes that the private whole-source digest supplies no
independently required publication authority once a fixed, domain-separated,
ordered canonical tuple commitment is tied one-for-one to authenticated
transaction-local `PutEvidence`, exact topology/root/transition checks, and
one-shot proof consumption before COMMIT.

The external fixture fingerprint remains custody evidence. Current v1 also
retains its existing ordered `(length, raw_id)` sequence because a canonical
transcript cannot authenticate v1's separate raw-ID field. See the completed
[authority report](while-waiting-phase-4-to-finish/task-b-witness-authority/report.md).

#### A2. H05 canonical-witness substitution — terminal no-go

Change only the private proof commitment:

```text
control:   whole-source digest + ordered raw-ID sequence
candidate: ordered canonical-ID commitment + ordered raw-ID sequence
```

For the 100-MiB control, candidate tuple input is
`5,284 * (4 + 32) = 190,224` bytes in place of a 104,857,600-byte whole-source
digest, a predicted net witness-input reduction of 104,667,376 bytes. Hash
input is a direct-counter prediction, not speed evidence. The complete paired
durable wall decides the mechanism.

The v7 screen completed with `3/3` favorable measured pairs and a
`16.655343%` paired-median durable improvement. Semantic, exact-work, Q,
transaction, one-COMMIT and logical/apparent storage gates passed, but the
prospectively frozen exact allocated-storage equality gate failed. H05b and
H05c closed the allocation follow-up without authorizing an amended H05
campaign. Preserve H05 as a strong performance prior and terminal rejected
mechanism; do not rerun, relabel or promote it.

#### A3. Compact v2 — next nonpersistent shadow

```text
v1 occurrence = raw_id[32] + length[4] + canonical_id[32] = 68 bytes
v2 occurrence = length[4] + canonical_id[32]              = 36 bytes
```

Exact retained-fixture effects before topology changes:

```text
32 * 5,284 references        = 169,088 fewer mapping bytes
mapping bytes                = 365,262 -> 196,174
full K64 leaf                = 4,380 -> 2,332 bytes
```

V1 already stores the canonical ID, so a dual reader could normalize v1/v2
references without fetching old payloads. The unresolved work is authority:
profile dispatch, v1-parent/v2-child deltas, downgrade rejection, receipts,
error identity, retained history, and migration.

H05 has now measured the ordered canonical-commitment mechanism, although its
candidate is not promotable. Begin compact v2 as a nonpersistent shadow model.
Use short graded exploratory screens to learn attainable speed; keep semantic,
authority, durability and bounded-resource requirements hard. Do not create a
general migration framework, reopen WP4-P, or rewrite history merely to
benchmark it.

### B. CAS + CDC + COW core — second

The accepted full-create path is already one source pass with bounded windows
and K/F frontiers. Another buffer abstraction is not a strategic change.

Ranked remaining directions:

1. Run the A3 authority/migration shadow and short explanatory v2 variants.
2. Test an exact-boundary FastCDC hot loop. F4-A2 killed buffer-copy removal,
   not the `128.723 ms` Gear/boundary loop itself.
3. Consider a larger CDC profile only as a versioned tradeoff among scan time,
   object count, dedup, edit amplification, and small-range authentication.
4. Keep multicore hashing as a separately authorized execution profile; the
   current contract forbids workers/queues.

Do not replace the K64/F64 COW radix for full-create speed. Mapping encode and
proof wall are too small, while the same-count edit result is already strong.
Content-defined/prolly mapping remains research only for measured early/middle
count-changing suffix churn.

### C. Hot/cold materialization — third

There is no accepted native/cold result yet. Reopening SQLite is not evidence
of a cold cache, and `438.070 ms` is logical reconstruction into a hash sink.

The high-value materialization directions are workload-specific:

1. **Authenticated parent-to-child delta materialization.** If the destination
   is proven to equal parent root `P`, apply only the authenticated `P -> C`
   delta. Unchanged payload work can disappear.
2. **Verified native file seeds plus APFS clonefile.** Reconstruct and verify a
   private seed once, then clone it for repeated same-volume destinations and
   apply target metadata separately.
3. **Canonical-only one-pass reads.** V2 can remove the second raw-payload hash
   after canonical authentication; the exact read-side wall remains unmeasured.

The blocker is destination authority. A publication receipt, inode, mtime,
size, or FSEvents notification is not by itself proof that a user-editable
directory still equals a root. Event gaps require a safe fallback. Native path
collision, symlink race, `u32 mode`, atomic replacement, and durability rules
must be specified before a fast path can be trusted.

### D. Compression and Git packing — stop for the retained path

The exact retained fixture rejects this direction:

```text
adaptive independent zstd-1 saving    4,529,478 bytes
ideal finalized-DB saving             4.1453%
chunks that became smaller            563 / 5,284
exploratory per-object encode wall    about 147.8 ms
generous byte-related wall ceiling    about 3.0 ms
Git delta result                      8 depth-1 deltas; pack +844 bytes
```

Foreground compression and Git-style delta packing should not be implemented.
Reopen only an adaptive, independent, below-identity zstd question if a real
post-dedup corpus first proves at least 20% physical-byte savings within 5%
protected create/read overhead. Offline archive/cold compaction is a separate
product problem.

### E. SQLite residual — after the core

SQLite currently writes approximately one final database image, not several:
`26,676 * 4,096 = 109,264,896` dirty-page bytes versus a
`109,268,992`-byte final database. The residual question is operation
granularity and overflow/B-tree work, not giant logical byte amplification.

The first residual direction is a new-database `4K / 8K / 16K` page-profile
study with a byte-fixed cache and protected full-create, edit, scrub,
reconstruction, range, storage, Q, and RSS results. The observed
`24.282 ms` mapping direct-VFS and `48.194 ms` COMMIT main-write lanes form only
a mixed gross ceiling; they are not additive physical-I/O evidence, and actual
page-size savings are unavailable.

Only if canonical/core work and page profiling still miss the target should a
hybrid immutable value log plus SQLite catalog/head receive a lower-bound
study. The old carrier's `10.3` index pages per lookup and `4.02x` reopen reads
must not be rebuilt.

## Fast research-to-experiment loop

Every optimization uses the same two-tier cadence:

```text
static authority and removable-budget research
  -> prospective one-variable contract
  -> <=120-second kill screen
  -> RETAIN-FOR-FULL-CAMPAIGN or REVERT
  -> full adjacent balanced A/B only for a retained screen
  -> independent audit
  -> accepted checkpoint or terminal revert
```

The screen should be the smallest affected boundary plus one correctness smoke
for protected neighbors. It may use three balanced pairs to reject weak work
quickly. It may not claim PASS, portability, or production authority. A full
candidate still needs the preregistered repetition count, complete protected
operations, independent recomputation, and custody manifest.

Static read-only research can run in parallel on disjoint reports. Builds,
repository-changing implementations, and all performance campaigns are
serialized. This prevents host interference and keeps each wall change causal.

Stop rules are intentionally cheap:

- stop before code when optimistic removable wall cannot reach the gate;
- revert immediately on any identity, authority, durability, error, Q,
  storage, or one-COMMIT mismatch;
- stop after the screen when paired wall is below the prospective gate;
- never run the complete 42-boundary package for a screen that already failed.

## Future specialist-agent roster

These are separate assignments, not one bundled implementation:

| Priority | Specialist question | Output expected |
|---:|---|---|
| 1 | V1/V2 canonical shadow model | Nonpersistent equivalence, authority, errors and migration model |
| 2 | Canonical-v2 execution variants | Direct counters and short graded screens for one explanatory variant at a time |
| 3 | Reopen authority | Threat model and invalidation proof; no scrub bypass until authority exists |
| 4 | Count-change requirement | Decide whether the product requires near-constant multi-GiB latency before any prolly simulator/code |
| 5 | Exact FastCDC loop | Boundary-identical mechanism analysis and isolated ceiling; no profile change |
| 6 | Destination authority / APFS seeds | Safe proof, invalidation, path/mode, clone, and fallback model |
| 7 | SQLite residual | 4/8/16-KiB physical-profile direction after the core decision |
| Deferred | Representative-corpus compression | Ratio-only screen before any codec implementation |

Each future agent must reread the code and sealed evidence independently.
This decision map is routing context, not evidence.

## What not to do

- Do not stack identity, CDC, mapping, compression, and SQLite changes.
- Do not reopen the already-complete WP4-P for a benchmark-private proof change.
- Do not run old pre-CP-0009 prompts unchanged or treat F2 as the current
  product control.
- Do not claim the `452.873 ms` ceiling as expected performance.
- Do not optimize encode/proof/copy lanes that are already below a few
  milliseconds.
- Do not replace the proven same-count edit path to chase full-create speed.
- Do not call reopened/warm-or-unknown reconstruction a cold checkout.
- Do not use compression to attack a fixture where it saves only 4.15%.
- Do not resurrect a carrier before a current same-work lower bound.
- Do not weaken caller-thread, one-transaction/COMMIT, identity, authority,
  exact errors, bounded memory, or fresh verification inside a comparable
  current-profile experiment.

## Recommended next action

Preserve terminal H05/H05b/H05c evidence, restore only the private benchmark
source to exact CP-0009, and start canonical-v2 with parallel code/evidence
research plus a nonpersistent authority/migration shadow. Explore one
explanatory implementation at a time with focused tests and `<=120`-second
graded screens. Promotion-grade A/B, integration and migration remain later
decisions. SQLite page profiling remains residual work, not the first move.
