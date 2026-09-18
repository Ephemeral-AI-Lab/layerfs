# Phase 2 implementation plan (2026-09-18): DB / engine and the owner split, #178

> **Status:** The implementation plan for Phase 2 of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178) — the DB/engine
> boxes (`P2-1`..`P2-8`) plus the structural and instrument prerequisites that
> make them measurable. Built by static source reading of the working tree at
> `2137c8487` (clean apart from an untracked `core/docs/benchmark/`): every
> `path:line` below was re-read in the **core** tree before entry, and where the
> register's citation resolves only in the reference or under a different module
> name the difference is recorded per item. **No builds, tests, timings or
> measurements were taken for this plan**; no performance claim is made, and where
> a figure is an estimate it is labelled one. It is not authority to change code:
> each item is a separate, measured, single-variable commit under
> [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)'s contract.
>
> Registers: [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176)
> (complexity + round trips) and
> [#177](https://github.com/Ephemeral-AI-Lab/layerfs/issues/177) (parallelism +
> batching). Phase 1's plan is
> [`phase1-implementation-plan-20260917.md`](phase1-implementation-plan-20260917.md).

## 0. Entry conditions

1. **Phase 1 is closed by owner ruling (2026-09-18)** with 14 of its 16 items
   landed plus the prerequisites `C1`, `V1`–`V4`: production LOC **18,792 →
   19,264 (+472)**, core only, **no new production files**, 456 tests passed /
   0 failed, all eight checks exit 0. The sequencing premise Phase 2 was waiting
   on ("the algorithm phase defines the call pattern the DB phase measures") is
   therefore satisfied to the extent the owner requires.
2. **Two Phase 1 items are NOT DELIVERED** — `P1-13` (merge fanout 4 / size-tiered
   cascade) and `P1-15` (hybrid binary-search-on-restart). They are carried by
   [`phase1-continuation-handoff-20260918.md`](phase1-continuation-handoff-20260918.md)
   and are **not Phase 2's to finish**. Phase 2 must not bank on their wins: a
   read-path measurement of `P2-1` taken on a tree without them is a measurement
   of that tree and must say so.
3. **Two open owner rulings from the closure** (`P1-7`'s third pinned test;
   `P1-10`'s missing architecture-document update for a changed named bound) are
   not Phase 2's to resolve. Phase 2 must not compound the second: every item that
   adds or moves a counter, a bound or a cache updates the architecture document
   **in the same commit** (`core/AGENTS.md`).
4. **Phase 0 dispositions stand** unless overridden: `P2-1`/O1 is deprioritized
   (P0-2 found **no cache spilling** at 1× or 4× the transaction ceiling, proven
   with a live control); the ordering items kept the forced-64 anchor; the
   missing vehicles were built before the items that need them.
5. **The read-path half of `P2-1` is now measurable**: `P1-2` landed (`D21`–`D24`
   connection opens 3 → 1), which was its stated prerequisite. The write-path half
   remains unsupported by P0-2 and needs an owner disposition (§9).
6. **`P2-2`'s named vehicle cannot be a match.** `pipeline.c2` is `NOT_RUN` — the
   reference's `WorkspaceAdmission` is public but has no public method. `P2-2` is a
   core-only A/B on the `c2.ceiling` workload.
7. **Work counters gate every item; elapsed is diagnostic-grade.** P0-3 measured
   `elapsed_ns` moving **+17.6%** on the same binary and input, and P0-2's
   counter was only reachable because it was proven live by a control.

## 1. The checklist

Notation: `r` rows in one transaction · `g` groups in one open pack · `k`
statements per multi-row INSERT · `w` provider waves. **Space is identical before
and after every item** — the parity invariant, guarded by the 34-test
sealed-oracle set. "LOC" figures are **estimates** (§6), not measurements.

### Structural prerequisite

| id | item | effect | gate | LOC est. |
| --- | --- | --- | --- | --- |
| **P2-0** | split `cas/owner.rs` (962 of 999 physical lines) into focused files by responsibility (§2.1) | no behaviour change; buys the headroom `P2-2` and `P2-5` need in the two files they must edit | **no test file changes**, parity green, identical counters, production LOC delta = imports + visibility markers only | +25..60 |

### Instrument prerequisites (product telemetry + vehicle prints; no behaviour change)

| id | item | why it exists | gate | LOC est. |
| --- | --- | --- | --- | --- |
| **V5** | SQL-statement counter (`OutcomeCounters` → `SaveOutcome`) | `P2-2`'s claim (8,191 statements → 64) is **unobservable today**: `OutcomeCounters` (`cas/owner.rs:73`) and `SaveOutcome` (`cas/store.rs:29-54`) carry `inserted`, `reused`, `packs_created`, `pack_appends`, `transactions`, `commits` — no statement count, anywhere in core | before-baseline on `c2.ceiling`: statements ≈ rows | +15..25 |
| **V6** | ordinary-lane group-decode counter (`ReadCounters`) | `P2-4`'s stated gate ("pack/decompress counters") **does not exist**: `ReadCounters` (`cas/read.rs:25`) carries `objects`, `packs_read`, `pages`, `ceiling`, `edges`, `max_depth`, `canonical_bytes` | before-baseline: decodes > distinct groups | +15..30 |
| **V7** | save-connection cache observability | `P2-1`'s write-path half needs `SQLITE_DBSTATUS_CACHE_SPILL` read on the **product's own** save connection; P0-2 recorded that `MutationOwner` is private and `SaveOperation` exposes no connection, so product-side observation needs instrumentation as a separate change | the counter is proven live by a control before any `P2-1` claim | +15..30 |

### The eight items

| id | item | change | verification gate | LOC est. |
| --- | --- | --- | --- | --- |
| **P2-1** | `cache_size` + `cache_spill` (#177 F2) | set 32 MiB / spilling OFF on the product connection, verified by read-back like the existing pragmas | A/B one save at the transaction ceiling + one read-heavy case; **V7**; declared per-connection memory; may end **measured-and-declined** | +10..25 |
| **P2-2** | multi-row INSERT (#177 F8) | one statement per `k` rows, `k` **derived** from SQLite's own limits (§4d) | **V5**: statements `r` → `⌈r/k⌉`; `commits` unchanged; parity green | +40..80 |
| **P2-3** | `locking_mode = EXCLUSIVE` (#177 F3) | pragma + scope reconciliation; a read-only Store must not take it | commits / statements / wall on the same save; the multi-store test surface | +5..15 |
| **P2-4** | decoded-group cache on the ordinary path (#176 T0-9) | bounded decoded-group cache, ported from `PoolReader`'s existing discipline | **V6**: decodes → distinct groups; **ceiling check before any cache hit** | +50..100 |
| **P2-5** | batch dependency presence + one reader per save (#176 T0-10) | one presence query per wave instead of one per offered object; reuse the owner's `pool_reader` | `edges`/`pages` + **V6**; parity; reader state must not leak across trials | +20..45 |
| **P2-6** | hash the requested object once per wave (#176 T0-11, safe half) | return the verified hash instead of recomputing it | a test pins one hash per requested object; tamper detection unchanged | +10..25 |
| **P2-7** | drop `copy_run` in `consolidate()` (#176 T0-12) | delete the copy **after** proving `merge_runs` never aliases an input | byte-identical output; ordering + root-identity suites; `rows_written`/`runs_created` move | −15..0 |
| **P2-8** | `append_fits` running total (#176 T0-15, C2 half) | keep the assembled length in the open-lane state instead of re-summing every group | `packs_created`/`pack_appends` and emitted pack bytes identical; boundary test at `pack_limit` | +10..25 |

**Explicitly not Phase 2:** the parked register (producer pool, group target,
branch-row summaries, pack-BLOB chunking, membership single-hash, persisted pool
cursor) — each needs an owner decision first. `P1-13`/`P1-15` stay with the
Phase 1 continuation handoff.

## 2. Prerequisites in detail

### 2.1 P2-0 — the `cas/owner.rs` split (pure relocation)

`cas/owner.rs` is **962 physical lines against the 999 ceiling** — 37 lines of
headroom, and both `P2-2` (the per-row insert loop, `owner.rs:803-829`) and `P2-5`
(the per-trial reader, `owner.rs:748`) must be written there. `core/AGENTS.md`
requires the split before the limit is reached: "Split by responsibility
(lookup, reconstruction, placement, transactions), not arbitrary numbered parts or
a renamed god object." The file's own module doc already names three of those
responsibilities, and the field block (`owner.rs:99-145`) already clusters into
three groups, so the seams are drawn by the code, not by this plan.

| target file | responsibility | members moved (current lines) | lines |
| --- | --- | --- | --- |
| `cas/pool_lane.rs` | pooled metadata lane: ordinals, value groups, index sync, pooled bases | `PENDING_VALUES_LIMIT` (26-36), `PoolCounters` (146-166), `select_pooled` (465-573), `pooled_full` (574-590), `sync_pool_index` (591-613), `assign_ordinal` (614-626), `write_value_groups` (627-713), `pool_base` (714-766), `pool_counters` (767-771) | 339 |
| `cas/placement.rs` | framing, lane placement, group sealing, open tails | `PendingMember` (38-53), `PendingGroup`+impl (54-70), `pending_member` (258-264), `retained_tail_bytes` (270-288), `seal_pending` (289-312), `seal_group` (772-829), `write_pack` (830-844) | 161 |
| `cas/lifecycle.rs` | acquisition, commit cadence, acknowledgement, terminal disposition | `acquire` (167-243), `baseline_pack_id` (244-248), `connection` (249-257), `maybe_commit` (845-867), `finish` (868-887), `finish_inner` (888-927), `abandon` (928-946), `mark_terminal` (947-954), `quarantine` (955-961) | 208 |
| `cas/selection.rs` | representation selection and its counters | `offer` (348-411), `select_record` (412-441), `delta_counters` (442-448), `chain_counters` (449-453), `candidate_index_bytes` (454-464) | 117 |
| `cas/owner.rs` (kept) | the state struct, the operation's outcome counters, demands answered inside its own transaction | docs (1-25), `OutcomeCounters` (71-97), `MutationOwner` (98-145), `note_reuse` (265-269), `read_batch` (313-324), `resolve_location` (325-347) | ~105 |

Coupling is one-directional and was checked: `seal_pending:307`, `offer:388,407`
and `finish_inner:891` call `seal_group`; `select_record:418` calls `select_pooled`;
`select_pooled` calls only its own cluster. `read_batch`/`resolve_location` stay in
`owner.rs` deliberately — they are the owner answering a demand through the
transaction it holds, and a 35-line module is not a responsibility.

**Mechanics (compiler-checked, no signature churn):**

- `impl MutationOwner` blocks in sibling files are legal — inherent impls must be
  in the same **crate**, not the same module — and only `lib.rs`/`mod.rs` are
  barred from `impl` by `core/tools/check_product_boundary.py`.
- Fields and cross-module methods become `pub(super)`, which from `owner.rs`
  resolves to `pub(in crate::cas)`: visible in `cas` and its descendants, which is
  every caller (`store.rs`, `save.rs`, `finish.rs`, `membership.rs`, `batch.rs`).
  It is not a public-API change — `mod owner;` is private.
- `cas/mod.rs` gains four `mod` lines and splits its re-export:
  `pub use owner::OutcomeCounters;` + `pub use pool_lane::PoolCounters;`
  (`store.rs:55` and `:439` are the only other `cas::owner::PoolCounters` paths).
- No `#[cfg(test)]` may enter product source, so verification stays in
  `core/crates/layerfs-storage/tests/`.

### 2.2 V5 — statement counter

Charge **statements**, not rows, at the two insert entry points
(`sqlite/write.rs:76` `insert_pack`, `:100` `insert_object`) or at the owner's
loop (`cas/owner.rs:803-829`), carry it on `OutcomeCounters`, surface it on
`SaveOutcome` (`cas/store.rs:29-54`), and print it on the `c2.ceiling` vehicle.
A row-based counter would not see `P2-2`'s change at all.

### 2.3 V6 — ordinary-lane group-decode counter

Charge in the ordinary arm of `encoding/decode.rs:44-52` (the
`GroupCodec::Zstandard` branch) and/or at its caller
`encoding/delta/read.rs:223`, carry it on `ReadCounters` (`cas/read.rs:25`),
and print it beside the existing read counters. `packs_read` is already charged
once per pack per wave and cannot substitute: the defect `P2-4` removes is one
decode per **record**, within a pack that is read once.

### 2.4 V7 — save-connection cache observability

P0-2's stated limit: the spill counter was read on harness-owned connections
replaying the row shape, because `MutationOwner` is private and
`SaveOperation` exposes no connection. Authorise either a bounded accessor on the
save (pragma values + `SQLITE_DBSTATUS_CACHE_SPILL`) or a recorded acquisition
profile. This is an instrument, not a behaviour change, and it is the gate for
`P2-1`'s write-path half — without it the item can only be argued, not measured.

## 3. Item detail: sites verified in the core tree

- **P2-1 / P2-3 — `sqlite/connection.rs` (138 lines).** Today's profile sets
  `journal_mode = MEMORY` (read-back verified), `synchronous = OFF` (verified),
  `temp_store = MEMORY`, `foreign_keys = ON` (verified), `busy_timeout = 0`
  (verified); `cache_size`, `cache_spill`, `mmap_size`, `locking_mode` and
  `threads` are **unset** (study §2.2). Both items add pragmas with the same
  read-back verification the existing ones use. `P2-3` is a **scope** change: it
  must be reconciled with one-save-owner, a read-only Store must not take it, and
  `busy_timeout = 0` stays. The register's paths for these are the study's, not
  code citations.
- **P2-2 — `cas/owner.rs:803-829`** (`for (record_number, member) in
  pending.members.iter().enumerate()` → `write::insert_object`) and
  **`sqlite/write.rs:100`**. The register's `cas/owner.rs` citation is right for
  the loop; the statement itself is `sqlite/write.rs:100`, not a `pack/write.rs`.
  Risks: per-row error attribution (an integrity failure must still name the row),
  parameter binding within `SQLITE_LIMIT_VARIABLE_NUMBER`, and `prepare_cached`
  reuse for the bounded set of distinct chunk sizes.
- **P2-4 — `encoding/decode.rs:44-52`** (ordinary arm), reached from
  **`encoding/delta/read.rs:223`**; the pattern to port is
  **`encoding/pool/read.rs:128-140`** (a `groups` map keyed by
  `row.first_ordinal`, charged to `decoded_work` against
  `POOLED_VALUE_CACHE_BYTES` = 512 KiB, `policy.rs:121`, released wholesale
  when the bound is crossed). **The ceiling must be checked before any cache hit**
  — `pool/read.rs:125-131` refuses `row.pack_id > ceiling` *before* consulting the
  cache, and a port that consults the cache first would let a cached group bypass
  a visibility refusal. The register's `decode.rs:48-50` / `delta/read.rs:223`
  citations resolve in the core tree only as `encoding/decode.rs:49` and
  `encoding/delta/read.rs:223`.
- **P2-5 — `cas/dependencies.rs:50-66`** (`lookup::present(connection, &missing,
  ceiling)` once per offered object) and **`cas/owner.rs:748`** (a fresh
  `PoolReader::new()` per pooled trial; `encoding/delta/read.rs:121` does the same
  in the delta path). The owner already owns a `pool_reader` field
  (`owner.rs:133`, constructed `:235`, used `:498`/:606): reuse must define the
  reader's state lifetime across trials — its retained groups are bounded by
  `POOLED_VALUE_CACHE_BYTES` and its window is per wave, so sharing without a
  reset could serve one trial's decoded values to another.
- **P2-6 — `encoding/delta/read.rs:176`** and **`cas/read.rs:92`** (the register
  cites `cas/read.rs:91`; the call is at `:92`). Both recompute
  `ObjectId::for_bytes` over bytes the other path already hashed for the same
  wave (`encoding/delta/read.rs:129` is a third site, in the chain path).
- **P2-7 — `filesystem/references/runs.rs:397`** (`consolidate`) calling
  **`copy_run` at `:436`**, defined at **`:582`** ("Copies one run into a fresh
  handle so a merge never aliases its own input"). The register's
  `runs.rs:418-424` and `merge.rs:211-214` citations predate the P1-5/P1-10
  landings and no longer resolve to those lines. Removing the copy is gated on
  proving the aliasing comment is not load-bearing — two research reports flagged
  it **UNKNOWN**, so this needs a test that fails if `merge_runs` ever writes into
  an input handle, not an argument.
- **P2-8 — `pack/layout.rs:199-213`** (`append_fits`, which calls
  `assembled_length` at `:216` to re-sum every group), called from the placement
  loop at **`pack/placement.rs:79`** (`assembled_length` also at `:57`). The
  running total belongs in the open-lane placement state; the first-fit decision
  must stay exact, so an off-by-one at `pack_limit` would move pack boundaries on
  disk — not canonical roots, but persisted bytes that the parity set covers.

## 4. Configurable numbers: what may be a knob, and what may not

Phase 2 touches several numbers. They fall into six classes, and only two of them
are legitimately configurable.

**(a) Persisted and validated per store — already configurable at creation.**
`StoragePolicy` (`policy.rs:145`): `format_profile`,
`small_file_threshold_bytes`, `whole_file_delta_max_depth`,
`chunk_delta_max_depth`, `metadata_delta_max_depth` (via
`with_metadata_depth`). `validated()` rejects any profile or value this slice does
not implement, the row is persisted, open **refuses conflicting overrides**, and
schema version 3 "widened the persisted policy ranges to the supported
configurable profile" — older Stores are rejected, never migrated. These values
change *canonical output* (they select representations), which is exactly why they
live in the store rather than being passed per operation.

**(b) Derived from the policy — not independently settable.** `StorageCapacities`
(`policy.rs:256-286`): `pack_limit`, `singleton_pack_limit`, `group_limit`,
`metadata_group_limit`, the three delta depths, the chain budgets. Changing (a)
moves these; nothing should set them directly.

**(c) Declared per operation by the caller (C1).** `FilesystemResources`
(`filesystem/input.rs:56-80`): `scratch_bytes` (the 4 MiB lease),
`maximum_pending_records` (4,096 default), `merge_buffer_bytes`,
`base_read_batch`, `ordering_bytes` (64 MiB default) —
with `maximum_touched_serials()` derived as `ordering_bytes / 16` (P1-10's named
bound). #176 Tier 1-19 treats the pending-ceiling dial as already configurable and
P1-16 documented it; these are per-workload declarations, and a run that changes
one must declare it.

**(d) Must be derived at runtime, not hardcoded — `P2-2`'s chunk size.** The one
number Phase 2 *should* compute rather than fix:
`k = min(SQLITE_LIMIT_VARIABLE_NUMBER / params, SQLITE_LIMIT_SQL_LENGTH /
row_bytes, 128)`, read from the connection's reported limits, exactly as the
reference's `sql_rows` does. Hardcoding 128 would be a copied constant with no
relationship to the deployment's SQLite.

**(e) Must stay a compile-time profile constant — no runtime knob.** Anything
that changes emitted partitions, framing or format: `GROUP_TARGET` (48 KiB,
`policy.rs:53`), `GROUP_LIMIT`, `PACK_LIMIT` (256 KiB),
`GROUP_COUNT_LIMIT` (256), `RECORD_COUNT_LIMIT` (8,191), `VALUES_PER_GROUP`
(165), `METADATA_GROUP_LIMIT`, `POOLED_LEAF_ROWS_LIMIT`, and the transaction
ceilings (`TRANSACTION_ROW_LIMIT` 8,191 /
`TRANSACTION_CANONICAL_BYTES_LIMIT` 4 MiB−1). Making one of these a knob changes
the bytes on disk, which requires a new oracle seal — precisely what the parked
Tier 2 items (branch-row summaries, pack-BLOB chunking) are about, and why they
need an owner decision first.

**(f) Must NOT become a knob even though it looks harmless — and the one Phase 2
number under this rule is `cache_size`/`cache_spill`/`locking_mode`/`threads`.**
The measurement contract requires one declared, enforced profile per arm, and
`AGENTS.md` forbids "changing worker counts, or relaxing a cache/buffer policy to
turn a miss into a pass". If `P2-1` or `P2-3` lands, it lands as a **fixed**
profile change proven by an A/B receipt — never as an environment variable or a
per-open option. The same rule covers workers:
`LAYERFS_CONSTRUCTION_WORKERS` is a harness cap whose only legal measured value is
1. Today **no core product code reads an environment variable at all**; adding one
would create a second, undeclared profile.

**Already-bounded byte budgets** (`POOLED_VALUE_CACHE_BYTES` 512 KiB (`policy.rs:121`),
`DEPENDENCY_PACK_CACHE_BYTES` 4 MiB, `METADATA_DECODED_WORK_LIMIT` 32 MiB,
`READ_OBJECT_LIMIT` 4,096, `CLEANUP_PAGE_ROWS` 128) are declared limits, not
tuning knobs. An item that adds a cache (`P2-4`) must declare its owner, bound,
multiplicity, lifetime and release in the same style the pooled cache already
documents.

## 5. Expected file and folder structure

**Before (`2137c8487`):**

```text
core/crates/layerfs-storage/src/
  cas/   owner.rs 962 · store.rs 543 · read.rs 162 · dependencies.rs 86
         save.rs 76 · batch.rs · membership.rs 47 · finish.rs · provider.rs 160 · mod.rs 18
  sqlite/  connection.rs 138 · write.rs 123 · lookup.rs 172 · schema.rs
  encoding/  decode.rs 192 · delta/read.rs 298 · pool/read.rs 366 · codec.rs
  pack/  layout.rs 488 · placement.rs · assemble.rs · mod.rs
core/crates/layerfs-content/src/filesystem/references/
  runs.rs 611 · merge.rs 284 · reduce.rs · backing.rs · record.rs
```

**After Phase 2 (sizes in physical lines; post-move figures are estimates):**

```text
core/crates/layerfs-storage/src/
  cas/
    mod.rs          18   ->   ~23      four module declarations + split re-export [P2-0]
    owner.rs        962  ->   ~105-140 state + OutcomeCounters + demands [P2-0]
    pool_lane.rs    new  ->   ~350-370 pooled lane [P2-0], then P2-5's reader reuse
    placement.rs    new  ->   ~175-190 framing/sealing [P2-0], then P2-2's batched insert
    lifecycle.rs    new  ->   ~220-235 acquisition/commit/terminate [P2-0]
    selection.rs    new  ->   ~130-145 representation selection [P2-0]
    read.rs         162  ->   ~170-200 ReadCounters.groups_decoded + charge [V6, P2-4]
    dependencies.rs 86   ->   ~110-130 wave-level presence query [P2-5]
    store.rs        543  ->   ~555-565 SaveOutcome.statements [V5]
    save.rs · batch.rs · membership.rs · finish.rs · provider.rs   unchanged
  sqlite/
    connection.rs   138  ->   ~165-195 cache/spill/locking pragmas + read-back [P2-1, P2-3]
    write.rs        123  ->   ~165-205 multi-row insert + statement charge [P2-2, V5]
  encoding/
    decode.rs       192  ->   ~200-230 cache consultation + decode charge [P2-4, V6]
    delta/read.rs   298  ->   ~305-330 reader reuse, single hash [P2-5, P2-6]
    pool/read.rs    366  unchanged (the pattern source)
  pack/
    layout.rs       488  ->   ~500     running-total helper [P2-8]
    placement.rs         ->   +10-20   open-lane assembled length [P2-8]
core/crates/layerfs-content/src/filesystem/references/
    runs.rs         611  ->   ~590     copy_run deleted [P2-7]
```

**Tests stay outside `src/`** (the boundary checker rejects test-only attributes
in product source). Existing files are extended where the subject already exists:
`core/crates/layerfs-storage/tests/connection_profile.rs` (P2-1/P2-3),
`visibility.rs` (P2-4's ceiling-before-cache pin), `metadata_pool.rs` and
`cas_reuse.rs` (P2-5), `pack_locator.rs` (P2-8's boundary decision),
`core/crates/layerfs-content/tests/filesystem_ordering_scan.rs` (P2-7). New files
are limited to the two counters that need their own subject:
`tests/statement_batching.rs` (V5 + P2-2) and `tests/group_decodes.rs` (V6 +
P2-4). **P2-0 changes no test file** — that absence is its evidence.

**Evidence tree** (append-only, mirrors Phase 1):

```text
docs/roadmap/0.1/0.1.7/evidence/phase2-execution-<UTCstamp>/
  CONTRACT.md                  written and frozen before any collection
  commands.tsv                 every command, exit code, wall time
  logs/                        including retained failures
  rounds/p2-0/{receipt.md,verify-p2-0.md}
  rounds/v5|v6|v7/{receipt.md,verify-*.md}
  rounds/p2-1 .. p2-8/{receipt.md,verify-p2-N.md}
```

## 6. Production LOC

**Measured before** (method: `python3 tools/production_loc.py --detail`, working
tree `2137c8487`): core **19,264 production lines in 116 files**; reference
`crates/` **65,417**; combined **84,681**. Per-crate core: `layerfs-content`
12,320 · `layerfs-storage` 6,181 · `layerfs-telemetry` 763.

**Estimate bands** (estimates from the shapes above, not predictions; per-commit
actuals are disclosed as the rule requires):

| item | estimate | item | estimate |
| --- | ---: | --- | ---: |
| P2-0 split (imports + `pub(super)`) | +25..60 | P2-3 locking mode | +5..15 |
| V5 statement counter | +15..25 | P2-4 group cache | +50..100 |
| V6 decode counter | +15..30 | P2-5 presence + reader reuse | +20..45 |
| V7 cache observability | +15..30 | P2-6 single hash | +10..25 |
| P2-1 cache profile | +10..25 | P2-7 delete `copy_run` | −15..0 |
| P2-2 multi-row INSERT | +40..80 | P2-8 running total | +10..25 |

**Total: core ≈ +200..+460**, i.e. ≈ 19,464..19,724 if every item lands — before
any measured-and-declined outcome. For calibration: Phase 1's plan estimated
+450..+950 and the phase closed at **+472**, so the band above is deliberately
narrow and the low end is the likelier landing. Tests, examples and docs do not
enter this number. `P2-0` is a **relocation**: its delta is imports and visibility
markers, and it must be labelled relocation — never called a simplification.

## 7. Landing order

1. **P2-0** — the split. It is first because it is a pure move, and because it
   creates the room `P2-2` and `P2-5` need.
2. **V5, V6, V7** — instruments before their dependents; each is its own commit
   with a before-baseline. Nothing that needs an instrument may precede it.
3. **P2-8, P2-6** — small, independent, one file each.
4. **P2-7** — gated on the aliasing proof; may end **not delivered** if the proof
   does not hold, which is a recorded outcome, not a failure.
5. **P2-4, then P2-5** — the read path; `P2-4` needs V6, and `P2-5`'s reader reuse
   touches the same pooled-lane state `P2-4` caches beside.
6. **P2-2** — the write path; needs V5 and lands in `cas/placement.rs`.
7. **P2-1, then P2-3** — profile items last, so the phase does not gate on the one
   item P0-2 already undercut, and because any engine-profile change re-bases
   every later measurement taken against it.

Read-path (`P2-4`/`P2-5`) and write-path (`P2-2`) work may interleave with a
Phase-1 continuation round if the paths are measured separately — they do not
share a counter or a file.

## 8. Verification protocol

Every item, without exception:

1. **One measured variable.** A deterministic work counter is the gate; `elapsed`
   is reported as diagnostic only. A receipt that shows only a wall time is not
   evidence (P0-3: +17.6% spread on one binary and input).
2. **The frozen set re-collected before and after** on the same vehicle, with the
   cache state declared and enforced equally in both arms, one sample per case per
   arm, fresh `--output`.
3. **Parity:** the 34-test sealed-oracle set green and unchanged. No emitted byte
   may move. A test that must move is an owner question, not an implementer edit.
4. **Single-variable commit + receipt + author-verified** `verify-p2-N.md` under
   `rounds/p2-N/`, append-only; failures and `NOT_RUN` rows stay on disk.
5. **The architecture document is updated in the same commit** as any accepted
   change to a counter, a bound or a cache (`core/AGENTS.md`).
6. **Wall-time budget:** a selection's complete command (timer + lifecycle +
   cleanup) ≤ 15 s; a declared exception list is allowed to 25 s and must be
   reported with its measured wall time.
7. **Checks:** `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml
   --locked` and `python3 core/tools/check_product_boundary.py`. This repository
   runs no CI and no aggregate preflight; report exactly which checks ran and
   which did not.
8. **Production LOC** before → after with the signed delta and the counting
   method, per commit; `P2-0` labelled relocation.

## 9. Open owner questions

1. **Authorise P2-0?** It is a production-file change with a non-zero LOC delta
   (imports and visibility markers) for zero behaviour change — Phase 1's
   precedent was "no new production files". Without it, `P2-2` and `P2-5` are
   written into a file with 37 lines of headroom.
2. **Disposition of P2-1** given P0-2 (no spilling found at 1× or 4× the ceiling):
   proceed, defer to a Phase 2 tail, or close it as **measured-and-declined**? The
   read-path half is now measurable; the write-path half needs V7 first.
3. **Authorise V5–V7?** Each is product telemetry plus a vehicle print, and each
   is a harness change — the same class Phase 1 approved as V1–V4.
4. **P2-3's scope change**: is an exclusive write lock compatible with the
   one-save-owner contract, and with a Store opened read-only?
5. **Does Phase 2 start before the two Phase 1 closure rulings are settled**
   (P1-7's third pinned test, P1-10's architecture-document gap)?
6. **Confirm the parked register stays parked** (producer pool, group target,
   branch-row summaries, pack-BLOB chunking, membership single-hash, persisted
   pool cursor), and that `P1-13`/`P1-15` stay with the Phase 1 continuation
   handoff.

## 10. Non-goals and prohibitions

- **Not Stage 6 qualification.** #171 owns measured acceptance; these receipts
  are inputs to it, not a substitute.
- **No canonical change.** No emitted byte, partition, framing or pack boundary
  moves. Tier 2 format items are out of scope and need an owner decision plus a
  new oracle seal.
- **No new knob** for anything a receipt must declare (§4f), and no worker-count,
  timeout or cache-policy change to turn a failing case green.
- **No third-party patch, vendoring or `[patch]` section**; builds stay `--locked`.
- **Nothing is closed by argument.** An item that measurement shows to be
  pointless is closed as **measured-and-declined** with its receipt, never
  silently dropped, and a failing cell is never removed from a report.
- **Not Phase 2's to finish:** `P1-13`, `P1-15`, and the parked register.
