# Stages 3–4 final review: correctness, efficiency and actual limits

> **Status:** self-review, written by the implementing agent at the owner's request.
> **Independence is not claimed.** The reviewer prompt
> ([`stages-3-4-reviewer-handoff.md`](stages-3-4-reviewer-handoff.md)) is written for an
> independent reviewer; that pass remains open and is the owner's decision. This review
> changed no product source: the only source-touching change in its commit is the
> candidate oracle fixture and the reference oracle example it needed (see §2.4).

**Reviewed snapshot.** HEAD `dfd54fd8e` plus the uncommitted work of the commit that
adds this file: the ninth oracle case, its evidence directory, and the report/verification
updates. Product source is identical to `5a0fef716` (production LOC 10 929); the commit
adding this file reports delta 0.

**Method.** Read the Stages 3–4 handoff, the file plan, the reviewer prompt and the
criterion tables in [`stages-3-4-report.md`](stages-3-4-report.md); ran the full workspace
suite, `clippy -D warnings`, `fmt --check`, the product-boundary guard, both tool
unit-test suites and the LOC counter; inspected the code paths named below and the
retained receipts in `evidence/`. Counts quoted here are from those runs.

---

## 1. Q1 — resulting structure and production LOC

### 1.1 Annotated tree (production files: production LOC / physical lines)

```text
crates/  (10929 prod)
  layerfs-content/  (4385 prod)
    src/  (4385 prod)
      error.rs  (120 prod / 174 physical)
      file/  (3366 prod)
        cdc/  (499 prod)
          gear.rs  (494 prod / 538 physical)
          mod.rs  (5 prod / 10 physical)
        content.rs  (195 prod / 249 physical)
        edit/  (1391 prod)
          apply.rs  (391 prod / 447 physical)
          compare.rs  (67 prod / 86 physical)
          concat.rs  (19 prod / 29 physical)
          finish.rs  (38 prod / 54 physical)
          input.rs  (286 prod / 390 physical)
          mod.rs  (15 prod / 20 physical)
          split.rs  (23 prod / 32 physical)
          tree.rs  (552 prod / 645 physical)
        mapping/  (1011 prod)
          build.rs  (301 prod / 371 physical)
          codec.rs  (303 prod / 334 physical)
          mod.rs  (15 prod / 20 physical)
          read.rs  (210 prod / 241 physical)
          types.rs  (182 prod / 248 physical)
        mod.rs  (17 prod / 23 physical)
        read.rs  (111 prod / 127 physical)
        view.rs  (142 prod / 171 physical)
      lib.rs  (24 prod / 41 physical)
      object/  (739 prod)
        access.rs  (18 prod / 36 physical)
        codec.rs  (107 prod / 138 physical)
        id.rs  (89 prod / 119 physical)
        inode_leaf.rs  (311 prod / 397 physical)
        mod.rs  (23 prod / 29 physical)
        output.rs  (124 prod / 192 physical)
        predecessor.rs  (67 prod / 107 physical)
      policy.rs  (136 prod / 226 physical)
  layerfs-storage/  (5812 prod)
    sql/  (48 prod)
      schema.sql  (48 prod / 69 physical)
    src/  (5764 prod)
      cas/  (1410 prod)
        batch.rs  (58 prod / 87 physical)
        dependencies.rs  (62 prod / 88 physical)
        finish.rs  (16 prod / 25 physical)
        membership.rs  (32 prod / 47 physical)
        mod.rs  (11 prod / 16 physical)
        owner.rs  (710 prod / 890 physical)
        read.rs  (74 prod / 101 physical)
        save.rs  (52 prod / 73 physical)
        store.rs  (395 prod / 530 physical)
      encoding/  (2587 prod)
        codec.rs  (508 prod / 632 physical)
        decode.rs  (170 prod / 192 physical)
        delta/  (795 prod)
          candidates.rs  (126 prod / 161 physical)
          mod.rs  (4 prod / 8 physical)
          read.rs  (202 prod / 261 physical)
          record.rs  (201 prod / 240 physical)
          select.rs  (262 prod / 343 physical)
        full.rs  (177 prod / 213 physical)
        mod.rs  (11 prod / 17 physical)
        pool/  (926 prod)
          delta.rs  (262 prod / 289 physical)
          index.rs  (203 prod / 259 physical)
          leaf.rs  (118 prod / 161 physical)
          mod.rs  (9 prod / 14 physical)
          read.rs  (255 prod / 294 physical)
          value_group.rs  (79 prod / 103 physical)
      error.rs  (105 prod / 154 physical)
      lib.rs  (11 prod / 30 physical)
      pack/  (739 prod)
        assemble.rs  (223 prod / 252 physical)
        layout.rs  (381 prod / 466 physical)
        mod.rs  (12 prod / 17 physical)
        placement.rs  (123 prod / 159 physical)
      policy.rs  (210 prod / 332 physical)
      sqlite/  (702 prod)
        cleanup.rs  (58 prod / 77 physical)
        connection.rs  (50 prod / 72 physical)
        lookup.rs  (141 prod / 177 physical)
        mod.rs  (10 prod / 15 physical)
        pool.rs  (128 prod / 160 physical)
        schema.rs  (232 prod / 267 physical)
        write.rs  (83 prod / 117 physical)
  layerfs-telemetry/  (732 prod)
    src/  (732 prod)
      lib.rs  (3 prod / 16 physical)
      timer/  (729 prod)
        format.rs  (69 prod / 82 physical)
        json.rs  (115 prod / 136 physical)
        mod.rs  (8 prod / 25 physical)
        recording.rs  (232 prod / 291 physical)
        report.rs  (170 prod / 249 physical)
        scope.rs  (135 prod / 206 physical)
```

Not shown above because they are not production: `core/crates/*/tests/` (37 external test
targets), `core/crates/*/examples/` (`measure_edits`, `measure_pooled`,
`measure_components`, `fingerprint_collision_search`, `timer_*`) and
`core/tools/check_product_boundary.py`. `core/crates/layerfs-storage/sql/schema.sql` is
shown because shipped runtime SQL counts as production. `crates/` is the pinned v0.1.6
reference, untouched except two development examples.

### 1.2 Caps

| Rule | Result |
| --- | --- |
| ≤ 999 physical lines per production file | largest is `cas/owner.rs` at 890; `file/edit/tree.rs` 645; `encoding/codec.rs` 632 |
| ≤ 200 lines for `lib.rs`/`mod.rs`, declarations/reexports only | largest is `sqlite/mod.rs` at 15; `storage/src/lib.rs` is 30 physical (11 production) |
| product-only source | `check_product_boundary.py`: PASS, 75 production Rust/SQL files scanned |

### 1.3 Per-file change table for this batch (production LOC)

| File | Before | After | Delta | Role |
| --- | ---: | ---: | ---: | --- |
| `object/inode_leaf.rs` | 0 | 311 | +311 | new: compact inode grammar and pooled value codec |
| `file/edit/tree.rs` | 0 | 552 | +552 | new: stored-tree load/split/coalesce/concat/emit |
| `file/edit/apply.rs` | 370 | 391 | +21 | rewritten chunked route; streaming plan reader removed |
| `file/edit/frontier.rs` | 95 | 0 | −95 | deleted: whole-mapping frontier |
| `file/edit/{input,compare,split,concat,finish}.rs` | 313 | 433 | +120 | validated stream, no-op verdict, tree helpers, emission |
| `file/edit/mod.rs`, `file/mod.rs` | 24 | 32 | +8 | re-exports |
| `encoding/pool/*` | 0 | 926 | +926 | new: value groups, pooled FULL/DELTA, index, reader |
| `sqlite/pool.rs` | 0 | 128 | +128 | new: catalogue rows and ordinals |
| `cas/owner.rs` | 546 | 710 | +164 | pooled lane, chain admission, invalidation |
| `cas/store.rs`, `cas/{save,read,batch}.rs` | 559 | 579 | +20 | pooled counters, footprint accessors |
| `encoding/delta/select.rs` | 199 | 262 | +63 | `ChainCost` caching and payload cost recording |
| `pack/{layout,assemble}.rs` | 577 | 604 | +27 | v6/v7 lanes |
| `policy.rs` (both crates) | 314 | 346 | +32 | pooling constants, metadata depth, schema 4 |
| `sql/schema.sql` | 44 | 48 | +4 | `metadata_value_groups`, depth column, role range |
| other `.rs` not listed above | — | — | +80 | helpers, error labels, view wiring |

Directory rollups, the eight-commit first-parent table and the migration subtotals are in
[`stages-3-4-report.md`](stages-3-4-report.md) §3. Totals: core 10 929 (content 4 385,
storage 5 812, telemetry 732), reference 68 476 (unchanged — coexistence, not removal),
combined 79 405.

### 1.4 What the structure says about responsibility

C1 owns identity, framing, CDC, the mapping grammar and now the stored-tree edit
algorithm; it has no C2, SQLite, Workspace, FUSE or daemon dependency (enforced by the
boundary guard and by the C1-only targets, which link no storage crate). C2 owns exact
CAS, the record/lane matrix, packs, the catalogue and the pooled-value derivation; it
performs no file construction. The one shared surface is the finalized-object handoff,
which is why `SaveHandoff` exists instead of a callback into C1.

---

## 2. Q2 — are the Stage 3 and Stage 4 criteria met?

### 2.1 Stage 3 (#168) — verdict: **PASS** (every item evidenced)

| Area | Evidence | Verdict |
| --- | --- | --- |
| Configurable cutoff/depths with checked ranges and rejection | `policy_capacity` (7), `edit_transitions`, `delta_chains::an_increased_depth_is_honoured_up_to_its_boundary` | PASS |
| Payload FULL/PREFIX, ordering, cost comparison, one trial | `delta_payload` (10) | PASS |
| Iterative chains, depth/work/memory bounds, corruption rejection | `delta_chains` (8) | PASS |
| Pooled inode grammar and value identity | `inode_leaf` (6), `metadata_pool` (9) | PASS |
| Ordinals, digests, catalogue integrity | `metadata_pool`, `metadata_pool_index` (5) | PASS |
| Pooled FULL / COPY-INSERT DELTA | `metadata_pool`, `metadata_chain` (2) | PASS |
| Index window bound, whole-window reset, cold-start replay, invalidate-on-failure | `metadata_window` (2), `metadata_pool_index` | PASS |
| Candidate filter is a filter, not equality | `metadata_fingerprint_collision` (2) plus the retained searched pair | PASS |
| Format matrix, lanes, singleton, bounded grouped acquisition | `physical_formats` (5), `pack_locator` (7) | PASS |
| Storage regressions: reuse, watermark, cleanup, failure paths | `cas_reuse` (8), `persistence_failure` (7), `visibility` (5), `memory_bounds` (7) | PASS |

Demonstrated defect found and fixed during this batch (not missing evidence): the pooled
lane admitted a delta on depth alone, so a save could create a chain the reader refused
(`Integrity("pooled chain work")`). Fixed in `5a0fef716` by admitting on the chain budget
as well, with `PoolCounters::work_exceeded` recording the refusal;
`metadata_chain::maximal_records_hit_the_canonical_budget_before_the_depth_cap` covers it,
and the three measurement arms show `full leaves = work-exceeded + 1` at every size.

### 2.2 Stage 4 (#169) — verdict: **PASS** (every item evidenced)

| Area | Evidence | Verdict |
| --- | --- | --- |
| Ordered normalized stream, current-result coordinates | `edit_batch` (7), `edit_model` (6) | PASS |
| Rejection of overlapping, inapplicable and length-mismatched edits | `edit_batch`, `edit_single` (9) | PASS |
| Single edits and complete deletion | `edit_single`, `file_complete` (14) | PASS |
| Transitions at every accepted cutoff | `edit_transitions` (5) | PASS |
| No-op preservation, bounded compare/replay | `edit_noop` (7) | PASS |
| Page occupancy boundaries and streaming | `edit_bounds` (8) | PASS |
| Stored-tree split/concat reuse, identity retention | `edit_localized` (5): 3 retained payloads demanded at 8 MB and at 24 MB while pages grow 5 → 12; untouched payloads still referenced | PASS |
| Bounded work, many files/edits, late failures, slow consumer | `edit_bounds::{many_files_and_many_edits_stay_bounded, a_source_that_ends_early_*, a_consumer_that_rejects_*}` | PASS |
| Emission finality | `edit_batch::every_object_the_edit_emits_is_reachable_from_its_root`, `edit_single::the_base_objects_are_never_rewritten` | PASS |
| **80 + 100 → 90 + 90 partition case** | `edit_reference::repartition-80-100`: edited partition `2/a1ff5278 90/73c20991 90/3a98cf7d`, root `d70a88de…`, untouched right leaf survives by identity | PASS |
| Frozen reference equivalence across shapes | `edit_reference` — all nine cases match root, partition and survivors exactly | PASS |
| Real edit → C2 → reopen → readback | `edit_pipeline` (4), the `e2-pipeline-*` receipts | PASS |
| Enabled/disabled timing equivalence | `edit_timing` (4), the `e3*` receipts | PASS |
| Existing-or-better qualified latency/storage/memory | matched C1 pair, `evidence/stages-3-4-matched-c1-20260917T050000Z/` | **INCOMPLETE**: identical reference root, but four observations interleave (reference 224 875–351 666 ns, candidate 186 000–468 333 ns); n=1 cannot qualify latency, the storage boundaries differ and memory is unmeasured |

The 80+100 gap recorded in the previous report is **closed**: the new oracle case
concatenates the 80-extent and 100-extent fixtures, replaces the whole extent run
straddling the seam with the same number of bytes, and the reference repartitions the join
to exactly 90 + 90 while keeping the far leaf's identity; the candidate reproduces it.

### 2.3 Criteria that are met but not compared to v0.1.6

No matched timing, storage or memory comparison exists, because the two products share no
public edit or pooled-save surface to time. The reference is used as a **result** oracle
(nine cases, exact roots/partitions/identities) and as the source of the frozen profile.
Speed and storage *improvements* are therefore **unproven**, and no such claim is made in
any report of this batch.

### 2.4 Committed versus uncommitted honesty

The reviewed snapshot is the commit that adds this file. `core/AGENTS.md` and several
owner-maintained roadmap documents carry pre-existing uncommitted edits that this batch
did not write and did not revert; they are not part of the reviewed product source.

---

## 3. Q3 — what can still be simplified

Findings are ordered by value; none was implemented during this review.

1. **`file/edit/split.rs` (23) and `concat.rs` (19) are thin wrappers over `tree.rs`.**
   Current behaviour: they forward to the `split_at`/`concat` helpers. Smallest change:
   delete both and re-export from `tree.rs`. Callers: `apply.rs`. Benefit ≈ 40 production
   lines and one less hop; verification: `edit_bounds`, `edit_reference`, `edit_localized`
   unchanged. Invariant preserved: same canonical partition, same emission order.
2. **The small↔large rebuild route can be expressed as a repartition on the stored tree.**
   Current behaviour: `apply.rs` rebuilds through `assemble_final` when the file crosses
   the cutoff, so those two directions do not use the stored-tree path. Smallest change:
   run the canonical repartition pass that `edit_transitions` already validates after the
   stored-tree edit. Estimated benefit ≈ 60 production lines and one route to maintain;
   verification: `edit_transitions`, `file_complete`, `e2-pipeline-large-to-small`. This
   is an estimate, not a measurement.
3. **`DepthCache::depth_of` is now a wrapper over `cost_of`.** One caller (payload
   eligibility) uses only the depth. Keeping both is deliberate: the depth-only call site
   should not pay for cost bookkeeping it discards, so no change is recommended.
4. **`encoding/pool/mod.rs` is a re-export module** (9 production lines). It is a
   declaration file, not a stub; folding it into `encoding/mod.rs` would save 9 lines and
   cost a clearer module name. Not recommended.
5. **Fixture duplication between `measure_edits` and `measure_pooled`** (leaf/value
   builders, timing wrapper). Test-side only; consolidating would couple two tools that
   are meant to stay independently runnable. Not recommended.
6. **`PoolIndex` and the payload `Candidates` are two candidate structures** with
   different semantics (ordered window versus min-hash signature). Merging them would
   create the generic buffer manager the handoff forbids. Not recommended.

No finding removes validation, exact comparison, backpressure, atomicity, visibility or
cleanup.

---

## 4. Q4 — statistics and evidence

### 4.1 Measured observations

Pooled-metadata lane (receipts `evidence/stages-3-4-timing-20260917T031000Z/e1*`, debug
profile, one sample each, in-process fixture, no warm-cache credit):

| Leaves × rows | Values new / reused | Full / delta leaves | Trials | Work-exceeded | Retained index | Store bytes | Wall |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 24 × 100 | 123 / 2 277 | 3 / 21 | 21 | 2 | 123 entries, 2 952 B | 49 152 | 0.208 s |
| 128 × 100 | 227 / 12 573 | 16 / 112 | 112 | 15 | 227, 5 448 B | 114 688 | 0.834 s |
| 512 × 100 | 611 / 50 589 | 64 / 448 | 448 | 63 | 611, 14 664 B | 323 584 | 3.155 s |

**Correction (added 2026-09-17, W8.7, by the implementation agent).** The
`512 x 100` row above is quoted from a telemetry-clipped receipt: its `stdout.log`
prints `pooled.save 3.105s [incomplete]`, its timing tree carries
`"incomplete": true` on the root and on one `storage.accept`, and 57.5 % of that
scope is unattributed. The receipt is retained unchanged; the 3.155 s column above
is a whole-command wall time, and the counts in the row are read from the receipt's
own summary lines, which are complete. Nothing else in the row or in the paragraph
below depends on the clipped detail.

Read as: 51 200 admitted rows in 512 leaves collapse to 611 pooled values (98.8% reuse),
seven of eight leaves are COPY/INSERT deltas, and the chain restarts exactly where the
65 536-byte canonical budget binds (`full leaves = work-exceeded + 1` at all three sizes;
512 / 8 = 64). The same rows unframed would be 512 × 8 144 = 4 169 728 canonical bytes, so
this fixture stores 7.8% of a naive layout — a structural observation about this shape,
not a general storage claim.

Edit lanes (15 receipts `e2-*`, threshold 131 072 B): every run exited 0 and verified its
readback; the C1-only and integrated `small` runs return the same edited root
`8ddfe36c…`, and the C2-only lane reads back the same 65 559 canonical bytes. Wall times
0.05–0.10 s. Localization (`edit_localized`, external accounting, not a timing claim): one
three-extent overwrite demands 11 objects at 8 MB and 13 at 24 MB while the mapping grows
from 5 to 12 pages; payload reads are identical (3) at both sizes; a mid-extent overwrite
demands exactly the two boundary extents; an append demands 2 pages and no retained
payload.

### 4.2 Source-derived bounds (not measurements)

Deferred-node ceiling `EDIT_DEFERRED_LIMIT = 8 MiB − 1`; batched-read bounds (512 objects,
512 KiB canonical); transaction bounds (8 191 rows, 4 MiB − 1 canonical); pooled window
131 072 entries; chain budgets 512 KiB / 256 KiB (payload) and 65 536 / 139 281
(metadata); match budget 128 KiB; pack, group and record bounds in §5. These are enforced
at runtime and covered by capacity refusals, but they are declared limits, not observed
peaks.

### 4.3 Unavailable (null, never zero)

- **Heap/RSS attribution** for either package: no instrumentation was collected, so the
  memory conclusion rests on declared capacities plus external accounting.
- **Canonical copies, hashes, encodes, codec trials and pack assemblies as counters**: the
  product does not count them. `SaveOutcome` reports `inserted`, `reused`, `packs_created`,
  `pack_appends`, `commits`, `full_records`, `prefix_records`, `delta.*`, `chain.*` and
  `pool.*`; byte-level copy counts are not among them, and the timing tree shows only the
  scopes the product records.
- **p95 or variance**: one sample per case by rule; no statistical claim is made.
- **Cold-cache contrast**: not measured; it would need the cold contract's invalidation and
  residency check, which an in-process fixture cannot provide.

### 4.4 Timer cost

The `--timing on|off` pairs (`e1a`/`e3a`, `e3b`/`e3c`) produce identical product lines:
policy, counters, readback verification, both readback identities, retained footprint,
catalogue rows and result roots. Only the timing tree, the `timing:` line and the run's own
output path differ, so the observable products do not depend on recording. The absolute
node counts are in the receipts; a per-node overhead figure is not computed, because
subtracting overlapping spans is not a CPU measurement.

### 4.5 Matched C1 pair (the only reference comparison collected)

`evidence/stages-3-4-matched-c1-20260917T050000Z/`: same 3 300 000-byte base, same
replacement of `[1 650 000, 1 690 000)`, release profile, one declared sample per arm,
all four observations listed in its ledger. Both arms return the identical root
`b6dca354…c65207b` in every observation, which is independent reference-equivalence
evidence outside the fixtures. The elapsed times interleave (reference
224 875–351 666 ns, candidate 186 000–468 333 ns), so **the latency gate stays
unqualified**; the byte accounting is not a like-for-like boundary (5 objects /
13 826 B against 7 objects / 47 357 B, dominated by where the replacement content is
emitted); memory is unmeasured.

### 4.6 v0.1.6 comparison status

| Dimension | Status |
| --- | --- |
| Algorithm/result equivalence | **Proven for edits**: nine oracle cases with exact roots, partitions and survivor identities. Not applicable to pooling, because the reference's pooled lane exposes no public C2 save/read operation to drive from a test. |
| Speed | **Unqualified.** One matched C1 pair exists and its observations interleave; see §4.5. |
| Storage | **Unqualified.** The pooled lane uses the reference's own formats; the only matched pair has non-aligned write boundaries. |
| Memory | **Unmeasured.** Declared-capacity evidence only; neither the candidate nor the reference was instrumented. |
| New capabilities the reference does not expose (stored-tree COW as a public edit path, v6/v7 lanes with explicit dispatch) | Reported separately; never counted as a speedup. |

---

## 5. Q5 — limits

### 5.1 Profiles and numeric ranges (validated on creation)

| Item | Accepted range | Default |
| --- | --- | --- |
| Construction cutoff `T` | 131 072 … 1 048 576, powers of two | 131 072 |
| Whole-file delta depth | 0 … 50 (`0` disables) | 8 |
| Chunk delta depth | 0 … 50 (`0` disables) | 4 |
| Metadata delta depth | 0 … 50 (`0` disables) | 8 |
| Format profile | 1 only | 1 |
| Schema | `application_id` 1279677261, `user_version` 4, four tables, twenty-one columns | — |

### 5.2 Object and structure limits

| Limit | Value | Source |
| --- | --- | --- |
| Canonical object | ≤ 16 MiB | `MAX_CANONICAL_OBJECT_BYTES`, `CANONICAL_LIMIT` |
| Whole-file raw record | ≤ 131 071 B (frame ≤ 135 168) | `WHOLE_FILE_RAW_LIMIT` |
| Chunk raw record | ≤ 32 768 B (frame ≤ 33 024) | `CHUNK_RAW_LIMIT` |
| Mapping page entries | 64 … 128 non-root; ≤ 128 root | `MIN_ENTRIES`, `MAX_ENTRIES` |
| Mapping level | ≤ 31 | `MAX_LEVEL` |
| Mapping node object | ≤ 8 192 B | `MAX_NODE_OBJECT_BYTES` |
| Pack | ≤ 256 KiB (v1/v2/v4/v6); ≤ 16 MiB + 4 096 singleton (v7) | `PACK_LIMIT`, `SINGLETON_PACK_LIMIT` |
| Pack groups / records | ≤ 256 groups; ≤ 8 191 records per group | `GROUP_COUNT_LIMIT`, `RECORD_COUNT_LIMIT` |
| Group body | ≤ 64 KiB ordinary; ≤ 16 KiB pooled | `GROUP_LIMIT`, `METADATA_GROUP_LIMIT` |
| Pooled leaf rows / value / record | ≤ 100 rows; 73-byte value; ≤ 8 192-byte record | `MAXIMUM_LEAF_ROWS`, `METADATA_RECORD_LIMIT` |
| Pooled window | ≤ 131 072 entries, whole-window reset | `METADATA_INDEX_VALUES` |
| Batch | ≤ 512 objects and ≤ 512 KiB canonical per read wave | `BATCH_OBJECT_LIMIT`, `BATCH_CANONICAL_BYTES_LIMIT` |
| Transaction | ≤ 8 191 rows and ≤ 4 MiB − 1 canonical | `TRANSACTION_*` |
| Edit deferred state | ≤ 8 MiB − 1 | `EDIT_DEFERRED_LIMIT` |

### 5.3 Tested sizes versus derived capacity

| Question | Answer |
| --- | --- |
| Largest file tested | 24 MiB (localization, 1 293 extents), 5 MB and 3.4 MB (oracle), 4 MiB (measurement fixtures) |
| File size ceiling | derived: extents per file are bounded by the mapping grammar (≤ 128 entries per page, ≤ 31 levels) and the logical length is `u64`. **Derived, untested.** |
| File revisions | not modelled in C1/C2; the chain bounds above limit *revision dependence* to depth ≤ 50 per role. **NOT_APPLICABLE to Stages 3–4; deferred to Stage 7.** |
| Files per workspace | the inode-leaf grammar holds ≤ 100 rows per leaf with metadata depth ≤ 8, so a namespace tree derives to ~100⁹ entries; the namespace algorithm itself is Stage 5. **Derived, untested.** |
| Directory size, name length, path length, depth | not modelled in C1/C2. **NOT_APPLICABLE to Stages 3–4.** |
| Workspace size | derived: pack ids are `i64`, ≤ 256 groups per pack, ≤ 8 191 records per group. **Derived, untested.** |
| Underlying storage | one SQLite file per Store (STRICT, `WITHOUT ROWID`, MEMORY journal, synchronous OFF, no WAL) plus pack files; no filesystem-specific limit is relied on beyond POSIX file size |

---

## 6. The seven required outputs

| # | Output | Where |
| --- | --- | --- |
| 1 | Source/profile/schema compatibility and supported numeric ranges | §5, report §2 |
| 2 | File tree, per-file and per-directory LOC, caps, per-commit LOC | §1, report §3 |
| 3 | Separate #168 and #169 criterion tables with code/test/evidence | §2, report §5–§6 |
| 4 | Standalone and integrated timer artifacts, failures and exclusions | `evidence/stages-3-4-timing-20260917T031000Z/` (21 receipts plus `ledger.md`), report §4 |
| 5 | v0.1.6 versus candidate mechanism, performance, storage and memory evidence | §4.5, report §7 |
| 6 | Bounded-memory ledger, FFI/codec ownership checks, corrected findings | §4.2–§4.3, `memory_bounds`, `edit_bounds`, `metadata_window`, the pooled-chain defect in §2.1 |
| 7 | Removed, retained, relocated, deferred and incomplete work | report §8 |

## 7. Verdict

**Implementation is complete for both stages.** Every functional acceptance item in
§2.1 and §2.2 has production code and an external test that exercises that code, the
whole-mapping rebuild route is gone, and the sealed reference oracle agrees in all nine
cases. The two open items below are measurement gates; they are the reason the issues
stay open, not missing functionality.

- **Stage 3 (#168) — functional criteria PASS, one qualification gate open.** E1–E3,
  the window/reopen/failure semantics and the packed lane all have code, external tests
  and retained evidence; one defect found during the batch was fixed and covered. The
  open gate is #168's own "simultaneous index/codec/SQL memory" requirement: no
  instrumentation exists, so it is **unmeasured**, and the pooled lane has no matched
  reference counterpart to compare against.
- **Stage 4 (#169) — functional criteria PASS, latency qualification open.** Every
  functional item in §2.2 is evidenced, including the 80 + 100 → 90 + 90 case and all
  nine oracle cases. #169's "existing-or-better qualified latency/storage/memory" item
  is **INCOMPLETE**: the matched C1 pair returns the identical reference root but its
  observations interleave at n=1, the storage boundaries differ and memory is
  unmeasured. Neither issue should be closed on this evidence.
- **Not claimed:** any v0.1.6 speed, storage or memory improvement; cold-cache behaviour;
  heap/RSS attribution; any release, tag or deployment state.
- **Open elsewhere:** #165 stays open, and Stages 5–7 (filesystem algorithms, Stage 6
  broadened qualification, Stage 7 Workspace/FUSE/cloud) are untouched by this batch.
- **Independent review:** still outstanding; this document is a self-review and must not be
  read as an independent acceptance result.
