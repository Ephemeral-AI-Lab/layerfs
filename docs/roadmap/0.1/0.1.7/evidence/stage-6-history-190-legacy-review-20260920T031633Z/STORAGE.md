# Legacy/current storage comparison

> Status: Research; informative and not a product contract.

Read-only review for #190. No build, benchmark, product edit, or default change was made. The source proves differences in work; it does not prove their elapsed-time contribution. Existing parent batching, authenticated-record reuse, physical-group reuse, and prepared catalogue SQL remain intact.

## Identity and evidence

`L:path:line` below means historical source `7fab1027a0061e8b932345d4fcd6ac22a089b155`; `C:path:line` means current `4391f66d8f39fc4d179caf557176021fc6f2c700`. The retained raw legacy binary identifies compiled commit `ac729dfeb4ee923b0a42106a53d1c7de4cd4cfcb`; the earlier [boundary reconstruction](../stage-6-history-190-20260919T225614Z/squad-s4/V016-COMMIT-COMPOSITION.md) established equivalence for the historical harness/product paths. This review ran `git diff 7fab1027a HEAD -- crates/layerfs-layerstack-store`; its empty result establishes that the legacy Store source inspected below is still identical to that historical pin. This is not an assertion that every current `crates/` package is identical.

The [latest retained screen baseline](../stage-6-history-190-screen-20260920T024251Z/runs/baseline-history-stride10/perf-receipt.json) is an existing diagnostic, not a sample taken by this review. Its operation is 22,615,178,250 ns, filesystem interval 9,536,586,040 ns, and storage begin/accept/finish sum 11,120,818,665 ns. Provider counters report 79,784 pooled pack fetches copying 7,378,994,999 bytes, 65,337 value-group decodes, and 2,723 physical-group decodes. These are application-level copies/decodes; they are not physical disk bytes or codec-only CPU. In particular, 7.38 GB is not evidence of 7.38 GB of disk I/O.

Historical Commit includes canonical construction and Store admission. Its 11,370,679,212 ns cannot be used as a matched baseline for either proposal below: cache, execution topology, canonical population, effective workers and exclusive subphase timing differ or were not recorded.

## What the code actually does

```text
v0.1.6 / legacy Store                         current core Store
---------------------                        ------------------
locator(s)                                   locator(s)
  | physical ordering                          | one pooled leaf resolution
  v                                            v
SQLite BLOB header + directory                new PoolReader (per leaf)
  |                                            |
  v                                            v
read selected group range                    SELECT entire pack -> Vec<u8>
  |                                            |
  v                                            v
decode group                                 validate complete directory
  |                                            |
  +-- bounded pool reused within group         +-- decoded physical group cache
  |                                            |   (existing 512 KiB; retained)
  v                                            v
resolve values + authenticate                resolve values + authenticate

Group encoding: level 1                      Group encoding: level 19
Payload encoding is a separate profile       Payload level remains 3
```

Legacy `objects/read.rs:1690–1728` sorts locations by pack/group/record and drains bounded waves; `:1773–1793` shares a bounded pool reader across the targets of a demanded metadata record group. Core `encoding/delta/read.rs:139–160` constructs a new PoolReader for each resolved inode leaf while borrowing the existing physical-group cache. Their pool-value/cache lifetimes therefore differ. This is a source fact, not a recommendation to widen caches: the existing core architecture explicitly records the fresh-per-leaf lifetime (`core/docs/architecture/05-storage.md:33–36`).

The two shortlisted directions below are distinct; they must not be combined into one treatment.

## Candidate 1: measure lower group compression level first

**Recommendation: the smallest next experiment.** It is a one-constant treatment with a directly observed historical CPU/space tradeoff. The user's acceptance of a one-second improvement and a small allocation overage makes this worth testing. It is not yet a demonstrated one-second operation improvement.

Legacy `L:crates/layerfs-layerstack-store/src/objects/pack.rs:1475–1517` calls `ZSTD_getCParams(1, ...)`, caps windowLog at 16, and writes content size/checksum without dictionary ID. Current `C:core/crates/layerfs-storage/src/encoding/codec.rs:94–104,386–423` uses payload level 3, group level 19 and group window maximum 16, with the corresponding content-size/checksum flags. Changing GROUP_LEVEL alone to 1 preserves decoding grammar and canonical identities; payload level, workspace, limits and workers should remain fixed so the treatment has one cause.

Existing [S3 retained recompression evidence](../stage-6-history-190-20260919T225614Z/squad-s3/C2-COST.md) contains two separate unmatched archival logs. Level 19 minus level 1 cost **1,011,835,000 ns** and **1,425,492,000 ns** of codec process CPU, respectively, while saving **262,222 encoded bytes** in each. These are separate results, not a confidence interval or an average. They used a scratch recompressor/dynamic context and old groups, not a live history save with the current static workspace. Thus neither is a lane wall-time measurement. The 262,222 bytes are encoded-body savings, not a measured Store allocation delta; page/pack effects must be measured live.

The decisive experiment is one current-source stride10 pair, level19 versus level1, with identical harness and instrumentation, single worker, declared cache and serialized quiet-machine protocol. Record operation, exclusive group-encoder elapsed/CPU where available, save interval, actual pack-body/apparent/allocated bytes and peak RSS. A lower encoder time without a useful operation reduction is insufficient. Compare every state root and run the existing separate verifier with the exact performance identities across all selected states. Its sampled path read-back is not exhaustive verification of every path; retain that limit in the receipt and report. Expect encoded Store bytes to differ. Confirm stride3 only if stride10 makes the tradeoff worthwhile. No stride1 optimization. Allocation acceptance does not permit changing codec integrity checks, chain budgets or any worker/capacity policy.

Counterevidence and limits: archival CPU savings are close to the one-second threshold and the live group population differs. The existing 11.120818665 s save interval is only an upper bound on all Store work, not group-encoder time. A one-line parameter change can affect compression, packing and later read traffic, so the existing separate sampled read-back verification and live allocation measurements are required. This review does not approve a default change on the archival replay alone.

## Candidate 2: demanded group-range reads instead of whole-pack copies

**Source-proven amplification; time effect NOT_MEASURED.** Legacy `L:crates/layerfs-layerstack-store/src/objects/read.rs:1092–1135` opens the SQLite BLOB, reads header/directory data and only the selected encoded group range. Legacy metadata values use this extraction route at `:191–218`. Core `C:core/crates/layerfs-storage/src/encoding/pool/read.rs:180–219` calls `lookup::pack_bytes`; `C:core/crates/layerfs-storage/src/sqlite/lookup.rs:148–158` selects the entire BLOB into a Vec. Core then selects the group from the copied pack.

There is no stored byte offset/length in the current value catalogue: `C:core/crates/layerfs-storage/src/sqlite/pool.rs:20–30` carries pack ID, group number, first ordinal, count and digest. Therefore a range route must read and validate the pack control area to derive offsets; claiming it could fetch a catalogue-provided range directly would be wrong. The existing dependency already enables rusqlite's `blob` feature (`C:core/crates/layerfs-storage/Cargo.toml:13`); no added dependency is needed.

The minimal compatible route would target pooled-reader ordinary/pooled-metadata group extraction only: open the BLOB, obtain its actual length, read the complete bounded header/directory, validate it against that length, then read the demanded encoded body. It should reuse/refactor the current parser rather than add a second grammar. Current `pack/layout.rs:252–302,318–393` validates total pack size, every directory entry's continuity, extents, codecs and final coverage. Reading only the demanded directory entry, as parts of legacy do, would weaken current rejection behavior. Preserve full-directory validation, complete selected-group decoding/authentication, locator/ceiling checks, canonical identity verification and per-chain accounting. No body may be trusted merely because its catalogue row exists.

The 79,784 fetches and 7,378,994,999 copied bytes give this direction a credible measurable work target. They do **not** establish a one-second lower bound. SQLite blob-handle/range calls add overhead; repeated use of one pack within a leaf currently benefits from the 4 MiB bounded pack cache (`policy.rs:119`, `pool/read.rs:209–219`). A range route could lose that reuse. Count actual selected body bytes, directory bytes, BLOB opens/ranges, cache hits/evictions and time in fetch/parse before deciding to implement it. Keep the same group/value cache capacities, worker count, roots, read ceiling, format and allocation. RSS could fall, but no amount is measured here. No new persistent cache is proposed.

A tempting shorter shortcut is moving the decoded physical-group cache check before fetching the pack. Current `pool/read.rs:353–381` fetches and validates first, then checks the cache; 52,953 retained physical-group cache hits therefore avoided decompression but do not by themselves prove the same count of removable BLOB fetches. Some pack accesses already hit the local pack cache, and value groups still require reads. Moreover GroupCache (`encoding/decode.rs:32–55`) stores only decoded bytes, without a validated lane/directory descriptor. Blind early return would remove the current extraction checks explicitly described by `core/docs/architecture/05-storage.md:26–31`. Reusing a prior authenticated extraction would need a precise immutable-snapshot/descriptor argument and corruption tests, not just reordered branches. Prefer screening the range-read mechanism before introducing another cache contract.

All required read work stays inside the measured operation. Do not move header reads, authentication or cache population into setup, or add helpers/workers. An eventual algorithm change must update the storage architecture with its validation and bounds in the same commit. Source review alone has not earned such a change.

## Other differences are not an additional shortlist

Both versions rewrite whole open-pack BLOBs: legacy `L:crates/layerfs-layerstack-store/src/objects.rs:2388–2408` assembles and UPDATEs the full pack; core `C:core/crates/layerfs-storage/src/cas/placement.rs:175–200` writes the full selected pack and charges that length. Thus "legacy only appends new bytes, core rewrites" is false. Legacy inserts selected pack batches at `objects/admission.rs:1525–1557`; core seals/writes individual ordinary groups at `cas/placement.rs:107–172`. The legacy cohort checks object/byte bounds before adding at `objects.rs:2310–2340`, while core commits on its charged transaction thresholds at `cas/lifecycle.rs:129–144`. Fewer historical transactions (101 versus current retained 1,149 in the earlier paired study) are not a like-for-like proof that commit overhead is removable. A write trace and exclusive SQL cost are missing; do not enlarge limits or change byte charges to make the count match.

Legacy content candidates are session-local (1,024 slots/8,192 references at `L:crates/layerfs-layerstack-store/src/objects/small_candidates.rs:1–20`) and can receive producer-precomputed signatures (`L:crates/layerfs-layerstack-store/src/objects.rs:890–897`). Core uses the persisted bounded candidate ring and computes signatures in selection (`C:core/crates/layerfs-storage/src/encoding/delta/select.rs:278,298,312,336,369`). Its `candidates.rs:353–380` flushes changed ring slots, and `:388–414` examines a bounded set of hash references, not the whole retained repository. No exclusive one-second signature/index cost was established. Removing the persisted index changes candidate availability and storage quality; it is not a free equivalent lookup optimization.

## Disposition

Do not stop because the new-ID filter failed; it tested a different mechanism. Authorize at most the isolated group-level live pair next. It has the lowest implementation complexity and historical evidence relevant to the newly accepted time/space tradeoff. Keep demanded-range reads as the algorithmic follow-up only if an attributable fetch/parse screen supports at least a one-second opportunity. Neither proposal explains the exact unmatched legacy timer gap today. Product LOC change in this review: **0**. Builds, tests, fresh timing runs and default changes: **NOT_RUN**, because this task is source review only.
