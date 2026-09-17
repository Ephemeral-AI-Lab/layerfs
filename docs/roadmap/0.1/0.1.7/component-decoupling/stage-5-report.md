# Stage 5 completion report: filesystem trees, inodes, attributes and ordering

> **Status:** implementation report for
> [#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170); target LayerFS
> v0.1.7. Evidence is the committed tree and the commands in §7. Not every
> criterion is complete; §8 lists exactly what is open.

## 1. Source identity, profile and compatibility

| Field | Value |
| --- | --- |
| Start commit | `b6b83162a8f4ca3ddb1a59adf1b1ea0cfb6d3369` (planning HEAD) |
| Handoff inputs | committed as received in `429f586dc` (handoff, file plan, updated `core/AGENTS.md`, roadmap routing, attribute contract) |
| Reference oracle | v0.1.6 `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`, verified identical for the tree/filesystem paths at planning time |
| Write profile | compact scoped-inline only: `layerfs/namespace-profile/scoped-inline/v1`, 8-KiB pages, depth 31 |
| Persisted roles | 1–13: payload/file-mapping 1–6 (pooled inode leaf keeps 6), tree roles 7–13 |
| C2 schema | unchanged version 4, four tables, twenty-one columns |

### The inode-leaf header gate

`object/inode_leaf.rs` recorded `subtree_bytes = row_count * 73`; the reference
`tree/compact.rs::encode_inode` records `row_count * 81`. Both are the *encoded*
row width (8-byte serial plus 73-byte value) only in the reference. This was
verified three ways before changing anything:

1. the reference encoder's own expression (`total.checked_mul(81)`);
2. the sealed reference fixture `inode-leaf.bin`, whose header records 8,019 for
   99 rows — exactly `99 * 81`;
3. a purpose-built sealed fixture from the generator: a two-row leaf whose header
   records 162, and a two-child branch whose recorded total is `128 * 81`.

The field is now the encoded row width, decoding validates every encoder
invariant directly instead of re-encoding, and `filesystem_codec` asserts the
reference bytes for a leaf, a branch, a directory leaf/branch, a symlink, a root
and an attribute leaf. **Value:** the same logical inode page now has the
reference identity instead of a different one.

### Profile and old-Store compatibility

- Readers accept exactly the compact scoped-inline profile. A root with another
  profile fails with `UnsupportedProfile` before any mutation; nothing is
  converted, normalized or migrated silently.
- Non-compact grammars (`LFS4NSP`, `LFS4FSR` with 32-byte inode ids) are not
  supported by the replacement and are refused the same way. No old-format
  handling is deleted from the reference; the replacement never had it.
- A Store created by the previous revision keeps its `CHECK (object_role BETWEEN
  1 AND 6)` and therefore rejects the new tree roles explicitly
  (`CHECK constraint failed: object_role BETWEEN 1 AND 6`, reproduced with the
  previous schema text). The new roles require a Store created with the widened
  constraint. There is no silent rewrite, no migration and no role remapping.
- Attribute roots and file-content roots keep their values across an update; only
  the objects this operation rebuilt change identity.

## 2. Source tree and size

Production LOC from `python3 tools/production_loc.py --json`, same counter and
scope for before and after (`core/crates`, excluding tests/examples/tooling):

| Scope | Before | After | Delta |
| --- | ---: | ---: | ---: |
| C1 `layerfs-content` | 4,487 | 11,772 | +7,285 |
| C2 `layerfs-storage` | 5,941 | 6,042 | +101 |
| C1 + C2 | 10,428 | 17,814 | +7,386 |
| Existing telemetry | 732 | 732 | 0 |
| Core total | 11,160 | 18,546 | +7,386 |

New files under `core/crates/layerfs-content/src/filesystem/` — **40 production
files**: the plan's 39-file map plus the justified `objects.rs`. Every file is
under the 999-physical-line ceiling and every `mod.rs` is under 200 lines. The
per-file table below is re-derived at this round's commit; see "Correction
2026-09-17 (R16)" at the end of this section for what was stale in the reviewed
version and for the plan-wide counts.

| File | LOC | Plan range | Inside? |
| --- | ---: | ---: | --- |
| `mod.rs` | 28 | 10–24 | above |
| `limits.rs` | 22 | 40–80 | below |
| `identity.rs` | 32 | 80–150 | below |
| `path.rs` | 147 | 180–280 | below |
| `objects.rs` | 102 | — (justified addition) | — |
| `root.rs` | 119 | 100–180 | inside |
| `symlink.rs` | 70 | 50–100 | inside |
| `input.rs` | 158 | 140–240 | inside |
| `validate.rs` | 576 | 220–380 | above |
| `update.rs` | 405 | 200–350 | above |
| `read.rs` | 191 | 180–300 | inside |
| `sorted/mod.rs` | 9 | 6–14 | inside |
| `sorted/budget.rs` | 87 | 90–150 | below |
| `sorted/format.rs` | 592 | 90–160 | above |
| `sorted/page.rs` | 449 | 150–260 | above |
| `sorted/merge.rs` | 282 | 260–440 | inside |
| `sorted/finish.rs` | 156 | 140–240 | inside |
| `directory/mod.rs` | 6 | 6–14 | inside |
| `directory/codec.rs` | 156 | 220–360 | below |
| `directory/update.rs` | 39 | 140–240 | below |
| `directory/read.rs` | 272 | 160–280 | inside |
| `inode/mod.rs` | 6 | 6–14 | inside |
| `inode/codec.rs` | 145 | 120–220 | inside |
| `inode/update.rs` | 72 | 100–180 | below |
| `inode/read.rs` | 186 | 150–260 | inside |
| `attributes/mod.rs` | 16 | 8–18 | inside |
| `attributes/keys.rs` | 77 | 60–100 | inside |
| `attributes/portable.rs` | 70 | 70–130 | inside |
| `attributes/codec.rs` | 322 | 130–220 | above |
| `attributes/build.rs` | 386 | 180–320 | above |
| `attributes/patch.rs` | 181 | 130–230 | inside |
| `attributes/read.rs` | 167 | 120–210 | inside |
| `attributes/value.rs` | 70 | 70–130 | inside |
| `references/mod.rs` | 15 | 8–18 | inside |
| `references/record.rs` | 120 | 100–170 | inside |
| `references/backing.rs` | 208 | 120–220 | inside |
| `references/runs.rs` | 417 | 160–280 | above |
| `references/merge.rs` | 154 | 200–340 | below |
| `references/reduce.rs` | 469 | 240–400 | above |
| `references/release.rs` | 132 | 160–280 | below |


Across the 53 rows the Stage 5 plan names, **9 sit above their recommended range,
13 below and 31 within**; inside `filesystem/` alone it is 9 above and 9 below of
the 39 plan-named rows. Correctness and one-responsibility-per-file drove the
split. Deviations are reported, not hidden: `sorted/format.rs` carries both real page formats plus the
exact-size arithmetic the encoder shares; `sorted/page.rs` carries the read paths
and the decoded-page lifecycle; `attributes/build.rs` and `references/reduce.rs`
carry the streaming partitioner and the reducer's bounded waves.

Edited existing files: `src/lib.rs` (+7), `src/error.rs` (+33),
`src/object/inode_leaf.rs` (+28), `src/object/output.rs` (+52). C2:
`src/pack/layout.rs`, `src/encoding/full.rs`, `src/encoding/delta/select.rs`,
`sql/schema.sql`, plus `src/policy.rs`'s existing default arm.

### Correction 2026-09-17 (R16): this section re-derived at the remediation commit

The tables and prose above were measured at an earlier commit and are corrected
here rather than rewritten silently. Every figure in this section was re-derived
at `b3df5461c` with the same counter, scope and exclusions
(`python3 tools/production_loc.py`), and the pre-Stage-5 column at its own
boundary `4f1b7d847`.

What was wrong in the reviewed version of this section:

- **the file count**: it said "24 production files" where the tree has **40**
  (the plan's 39 plus the justified `objects.rs`) - and its own table already
  printed 40 rows;
- **the above/below prose**: it said "four files exceed their recommended maximum
  and six fall below their minimum". At the reviewed commit the true counts across
  the 53 plan-named rows were **8 above, 17 below, 28 within**; at this round's
  commit they are **9 above, 13 below, 31 within**;
- **ten per-file LOC values** were stale at the reviewed commit (`update.rs`,
  `input.rs`, `read.rs`, `sorted/page.rs`, `sorted/finish.rs`,
  `references/record.rs`, `references/backing.rs`, `references/runs.rs`,
  `references/merge.rs`, `references/reduce.rs`), and this round's WP1-WP4 moved eleven more
  files (`validate.rs` 299 -> 576, `runs.rs` 323 -> 417, `update.rs` 359 -> 405,
  `input.rs` 135 -> 158, `directory/read.rs` 260 -> 272, `attributes/value.rs`
  61 -> 70, `identity.rs` 41 -> 32, `sorted/format.rs` 603 -> 592,
  `reduce.rs` 472 -> 469, `limits.rs` 20 -> 22, `objects.rs` 95 -> 102);
- **the totals table**: its "After" column read 11,001 / 5,964 / 16,965 / 17,697.
  Re-derived at `b3df5461c` the column is 11,772 / 6,042 / 17,814 / **18,546**.

Cross-check method: re-running the same counter against the reviewer's own
`loc-tables.md` at the reviewed commit `c99a8d9f9` reproduces all 53 of its
per-file LOC values exactly and its 8/17/28 split, so the re-derivation here is
the same measurement on a later tree, not a different one.

## 3. Criteria and checkpoints

### A. Executable contract, canonical oracles, one tiny real root — **complete**

- Contracts frozen in `limits.rs`, `identity.rs`, `path.rs`, `root.rs`,
  `input.rs`: profile, supported readers, native input semantics, supplied
  identity contract, logical roles, size limits, resource owners.
- The oracle is the pinned reference implementation itself: the generator
  (`crates/layerfs-content/tests/stage5_reference_fixtures.rs`) drives it and
  seals roots, objects, page shapes and finals into
  `core/crates/layerfs-content/tests/fixtures/filesystem/`. Before sealing an
  update case the generator asserts that the empty change set reproduces the base
  root, and each update case is produced by the reference *update* route.
- A real empty filesystem and a tiny directory/file/symlink tree build through
  C1, save through C2, reopen and read back (`filesystem_updates`,
  `filesystem_pipeline`).

### B. Sorted directory and direct inline-inode COW — **complete**

- `filesystem/sorted/`: reserve-before-growth budget, grouped child acquisition,
  untouched-subtree reuse, unresolved neighbouring pages, exact fill/partition
  rules, reserve-before-growth, root collapse, single final encode per page.
- No provisional empty seed on construction; an empty directory is emitted only
  when it is the real result. Typed values and tombstones reach inline leaves
  directly: no synthetic record identity, no encode/store/reread cycle.
- Parity: construction and all five update cases reproduce the reference root and
  the reference page partitions exactly.

### C. Reads and attributes — **complete, with one scoped equivalence**

- Reads: resolve/stat/list/readlink, grouped name and inode batches sharing each
  level's wave, ordered duplicate demands, count and byte limited pagination with
  a correct continuation across pages.
- Attributes: checked portable mode/mtime, generic domain/key grammar with no
  platform whitelist, opaque value preservation through patches, extent-only
  value ropes at every size, exact-size partition decisions shared with the
  encoder, tail rebalance preserving the reference partitions.
- **Scoped equivalence.** Attribute *pages* are byte-identical to the reference
  for identical value roots (sealed `attr-single`, `attr-pair`, `attr-wide`
  fixtures), and full-tree roots are identical for operations whose attribute
  roots are supplied unchanged. The reference cannot build generic domains at
  all — its validator rejects them — so the multi-key case is sealed with
  `apple.xattr`-spelled keys, which the replacement stores as ordinary opaque
  data. Attribute *value* roots are the replacement's Stage 3/4 file
  representation, so a tree the reference builds together with its own value
  ropes is not byte-comparable; that is a pre-existing representation change, not
  a Stage 5 divergence, and it is not claimed as equal.

### D. Topology, reference effects and ordered final values — **complete**

- One complete native operation: final bindings per directory, typed final
  values, declared new identities; additions accounted before removals; derived
  counts for new inodes from retained bindings; stored counts for existing
  inodes; paged release of zero-count directories with a moved-out or
  externally-linked child surviving.
- Refusals: unsorted/duplicate changes, reused identities, multiple parents for a
  directory or symlink, effective-tree cycles formed by several changes at once,
  disconnected new records, forged caller counts, foreign scope or profile.
- Ordering: one fixed 96-byte record grammar (version, tag, length, optional
  typed value, tally), a bounded pending map, caller-supplied run backing with a
  checked cleanup, tiered merges with newest-row precedence and one reader per
  live tier. Records carry no Workspace node, checkpoint receipt or temporary
  inode-record identity; each removed field had no remaining consumer.

### E. Real C2 and independent timing — **partial**

- New logical roles flow through existing admission: ordinary lane, pooled inode
  leaves unchanged, packs and SQLite reused, no flush per directory or inode, no
  second database.
- C2 changes are deliberately small: lane mapping, framed-payload arm, delta
  eligibility (tree roles never choose a payload delta) and the persisted role
  range.
- **Corrected 2026-09-17 (remediation).** An earlier revision of this section
  implied that an acknowledged save necessarily holds every object a persisted
  root names. It does not, and the contract now says so explicitly
  (`admission-and-persistence.md`, "Caller-authorized value roots"): a value root
  inside a 73-byte inode value is a logical pointer, not a declared reference, so
  the dependency check covers the page edges a save declares and not the roots its
  inode values name. Accepted, stored and readable as metadata; the first read of a
  root the Store does not hold fails with `MissingObject`. Pinned by
  `a_caller_authorized_value_root_is_not_an_object_dependency`
  (`layerfs-storage/tests/filesystem_pipeline.rs`).
- Coarse phases (`validate`, `directories`, `references`, `inodes`,
  `root.encode`) are recorded through the real telemetry scopes; disabled timing
  runs the identical body and emits the identical objects and counters
  (`filesystem_timing`).
- **Not done:** the comparative reference arm, cold-cache rows, pack-footprint
  and simultaneous-memory measurement. No speedup is claimed.

### F. Reference parity, limits and final review — **partial**

- Parity is complete for construction and updates; limits are enforced and
  tested for names/paths/pages/fill/scratch/ordering/value sizes.
- Missing: the C1 `filesystem_ordering`, `filesystem_failure`,
  `filesystem_bounds` targets and the C2 `filesystem_failure` target; the
  reference-versus-replacement measurement campaign in
  [stage-5-verification.md](stage-5-verification.md) was defined and the smoke
  commands run, but no comparative collection happened.

## 4. Verification actually run

| Command | Result |
| --- | --- |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked` | PASS (see the coverage correction in §13: the count is tests that exist, not targets that were built) |
| `cargo +1.85.1 test --locked -p layerfs-content --test stage5_reference_fixtures` (reference workspace) | PASS — fixtures regenerated identically |
| `python3 core/tools/check_product_boundary.py` | PASS — 115 production files |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | PASS |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | PASS |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | PASS |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | PASS |
| `python3 tools/production_loc.py --files` | see §2 |
| Reference-vs-replacement timing campaign | **NOT RUN** (no comparative arm exists) |

New external targets and their exact results:

| Target | Tests | Result |
| --- | ---: | --- |
| `filesystem_codec` | 6 | PASS |
| `filesystem_reference` | 2 | PASS |
| `filesystem_sorted` | 6 | PASS |
| `filesystem_read` | 4 | PASS |
| `filesystem_updates` | 6 | PASS |
| `filesystem_hardlinks` | 5 | PASS |
| `filesystem_topology` | 8 | PASS |
| `filesystem_attributes` | 9 | PASS |
| `filesystem_timing` | 2 | PASS |
| `filesystem_pipeline` (C2) | 3 | PASS |

The three smoke commands from the handoff were executed and print actual roots,
counters and nanosecond timings:

```text
filesystem_timing_c1 --case hardlink-move: root e21709c5…, elapsed_ns 301,958,
  phases validate 74,625 / directories 72,750 / references 3,375 / inodes 85,709 /
  root.encode 12,708
measure_filesystem --mode c2 --case inode-update: 8 supplied objects, inserted 8,
  packs 2, commits 1, acknowledged true, pooled 202, elapsed_ns 14,808,917
measure_filesystem --mode pipeline --case subtree-remove: root d5f91c97…,
  construction+handoff+save elapsed_ns 18,080,125, readback separately 77,042 ns
```

These are correctness and wiring smoke runs on a warm in-process fixture; they are
not a latency or comparative claim.

## 5. Memory and ordering-disk ledger

| Owner | Capacity | Multiplicity | Release |
| --- | --- | --- | --- |
| Sorted merge scratch | `FilesystemResources::scratch_bytes`, default 4 MiB − 1 | one per directory merge, then one per inode merge (sequential) | `Lease` drop; peak reported as `peak_scratch_bytes` |
| Decoded page + batch | charged inside the same budget | one group of ≤ 32 children | lease drop after the chunk |
| Reference pending map | `maximum_pending_records`, default 4,096 rows | one map | cleared on spill and consumed by the final stream |
| Ordering runs | caller backing; `FileBacking` reports `held_bytes`/`peak_bytes` | one run per tier, ≤ 32 tiers | explicit `release()`; `Drop` is the cancellation net |
| Merge buffers | `merge_buffer_bytes`, default 16 KiB | one per live tier reader plus one output | dropped with the run |
| Release cursors | one per released directory level | depth-bounded stack, 64-entry pages | popped when the page ends |
| Object boundary | none: reads are borrowed from the provider | one outstanding page group | per call |

Measured peaks are reported by the runs (`peak_scratch_bytes`, `rows_spilled`,
`released`, `report_nodes`). No cgroup or lifetime figure is claimed.

## 6. Limits

| Limit | Kind | Value |
| --- | --- | --- |
| Name component | enforced | ≤ 255 bytes, UTF-8, no NUL / `/` / `\`, never `.`/`..` |
| Path | enforced | ≤ 4,096 bytes and ≤ 256 components |
| Page | format | ≤ 8,192 canonical bytes; depth ≤ 31 |
| Inode leaf | format | 50–100 rows non-root, root may be 1..100 |
| Inode branch | format | 64–127 children non-root |
| Directory page | format | 2/5 fill = 3,277 canonical bytes |
| Attribute key | enforced | domain ≤ 64 UTF-8 bytes, key ≤ 255 bytes, no NUL |
| Attribute value | enforced | ≤ 1 MiB, always extent-backed |
| Symlink target | enforced | ≤ 4,096 bytes, no NUL |
| Inode serial | enforced | 1 .. `i64::MAX` |
| Operation scratch | configurable | ≥ 1,024 bytes, default 4 MiB − 1 |
| Pending records | configurable | ≥ 1, default 4,096 |
| Theoretical | not claimed | no unlimited-workspace or constant-RSS claim is made |
| Verified | measured | 900-entry construction, 400-entry updates, 60-name pagination, 8 directory levels, 303-inode sealed tree |

## 7. Concrete cuts and their evidence

| Cut | Where | Evidence |
| --- | --- | --- |
| Sorted merge replaces per-name mutation and deferred structural graphs | `directory/update.rs`, `sorted/*` | identical roots and page partitions against the sealed reference update route |
| Optional base root replaces a provisional empty seed | `sorted/finish.rs` | `filesystem_sorted::initial_construction_needs_no_provisional_seed`, empty-result case |
| Typed inode values replace encode → store → ID rewrite → reread | `inode/update.rs`, `sorted/merge.rs` | sealed update parity; the reducer's row grammar has no record identity |
| Synthetic inode-value identity removed | `sorted/format.rs` | no fake `ObjectId` is built from a serial anywhere in `filesystem/` |
| Root/profile rereads removed | `sorted/finish.rs`, `update.rs` | the operation carries profile, scope, serial and the table identity it built |
| Exact sizing replaces trial clone/encode-for-fit | `attributes/build.rs` | partition decisions use `bytes()` arithmetic; `attributes_pages_...` asserts the same boundaries |
| Decode-then-re-encode validation removed | `object/inode_leaf.rs` | every encoder invariant is checked directly; golden and malformed fixtures cover them |
| Caller metadata readback removed | `update.rs` | a changed directory's content root comes from the merge; the stored record supplies kind and attribute root |

No performance gain is claimed for any cut: the comparative arm does not exist
yet.

## 8. Open criteria and blockers

1. **C1 `filesystem_ordering` target** — record grammar golden/truncated rows,
   configured threshold crossings, tier carries, backing cleanup and quota
   failures. The reducer is implemented and used, but those specific external
   proof cases are not written.
2. **C1 `filesystem_failure` target** — late input/order error, corrupt demanded
   page, refused output, cancellation. The behaviours exist (and are exercised
   incidentally), but the dedicated matrix is missing.
3. **C1 `filesystem_bounds` target** — decoded-page/C2 overlap, released-subtree
   counters and read waves as explicit evidence.
4. **C2 `filesystem_failure` target** — private early output, one owned cleanup,
   catalogue/dependency correctness, retained old roots, unavailable owner.
5. **Comparative measurement campaign** — no reference arm, therefore no
   reference-versus-replacement row and no performance claim. Stage 6 (#171)
   qualifies the whole core; Stage 5 does not move this requirement there.
6. **Non-compact profile support** — deliberately unsupported and refused; no
   converter is planned in this stage.

These items block a claim that #170 is fully closed. Everything in §3 A–D and the
parity part of F is implemented, tested and evidenced.

## 11. Dated corrections (2026-09-17)

This section corrects claims in the sections above; it does not rewrite them.

1. **"No demonstrated product defect" was wrong.** The
   [blocker investigation](stage-5-blocker-investigation-20260917.md) reproduced
   three defects on the source this report describes: backing held/peak bytes were
   always zero, the operation never performed its checked cleanup, and the returned
   ordering counters were zero after real spills. The
   [completion report](stage-5-completion-report-20260917.md#2-the-confirmed-defects-and-two-more-this-work-found)
   records their fixes and two further defects found while fixing them.
2. **"§8 Open criteria" is superseded.** The four missing targets now exist:
   `filesystem_ordering` (8 cases), `filesystem_failure` (5), `filesystem_bounds`
   (4) and C2 `filesystem_failure` (4), plus one added pipeline case. The open list
   is now only the independent review and the complete-operation comparison.
3. **"No performance claim"** still holds for the complete operation, but a
   matched component comparison now exists in
   [stage-5-verification-addendum-20260917.md](stage-5-verification-addendum-20260917.md#5-collected-rows-componentprimitives-release-2026-09-17):
   two cases faster, one 10% slower, all identities matching.
4. **Production LOC changed**: C1 11,001 → 11,209 and the core total 17,697 →
   17,905; see §9 of the completion report.
5. **The ordering row is 96 bytes, not 88.** The 88-byte layout overlapped its own
   fields; the grammar now has disjoint fields and one authoritative tally.
6. **The verification document's "frozen" banner was wrong** about performance
   gates; it is corrected in place and superseded for new rows by the dated
   addendum.
## 12. Coverage correction, 2026-09-17 (WP5, R42)

The verification row above used to read "PASS (all targets, including the eight new
Stage 5 targets)". **Targets are not coverage.** On the remediation tree the suite
discovers **59 targets**, of which **53 carry at least one test** and **6 carry
none** - three `unittests src/lib.rs` binaries (`layerfs-content`,
`layerfs-storage`, `layerfs-telemetry`) and three doc-test targets. A
test-bearing target count of 53 and a zero-test target count of 6 is the honest
statement; the six contribute nothing to any criterion row, and the recorded
result is **393 passing tests, 0 failed, 0 ignored**.

The Stage 5 report's original "363 passed" figure was taken from a run that
included the same six empty targets in its target count. Nothing was wrong with
any individual test; the summary simply counted binaries. Later rows in this
document state tests, not targets.

## 13. Round-2 independent review outcome, 2026-09-17 (R2)

> **Status: #170 is NOT ACCEPTED.** This section is the outcome of the second
> independent acceptance review and it corrects four statements this report makes.

| | |
| --- | --- |
| Reviewer report | [`stages-1-5-review-20260917T230700Z.md`](stages-1-5-review-20260917T230700Z.md) |
| Reviewer evidence | `docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T230700Z/` (30 files) |
| Reviewed snapshot | `f288d2af7ecdc7e00f7df153073398d333461aa3`, tracked tree clean, identity re-checked |
| Stage 5 matrix | 64 PASS / **9 FAIL** / 8 PARTIAL-INCOMPLETE / 1 NOT_RUN / 1 NOT_APPLICABLE of 83 |
| Cumulative matrix | 30 PASS / **3 FAIL** / 1 PARTIAL / 2 owner-WAIVED of 36 |
| Issue record | [comment on #170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170#issuecomment-5717192709) |

### Corrections to this document

1. **§6, the attribute-value row is wrong.** The enforced bound is **32,768 bytes**,
   not 1 MiB: `attributes/value.rs:22-56` emits exactly one chunk object, so the
   1 MiB branch at `:26-31` is unreachable. Reproduced through the public API: a
   1 MiB value is refused with `object limit 32768 exceeded by 1048576`. Matrix row
   `AT-4` was promoted on the wrong figure.
2. **§6 omits a capacity limit that changes what a Workspace can build.**
   `MAXIMUM_CYCLE_CHECK_ENTRIES = 4,096` (`filesystem/validate.rs:53`) is enforced
   per whole-tree walk, so a single `build_filesystem` call is refused above
   **4,095 directory bindings** (4,095 accepted, 4,096 `BUILD REFUSED invalid
   record: cycle check work limit`), and an existing directory whose effective
   subtree exceeds 4,096 entries can never be rebound (a 4,195-entry rename is
   refused the same way).
3. **§2 totals are one production commit stale.** This section is re-derived at
   `b3df5461c`; the reviewed tree is C1 **11,875**, C2 **6,043**, core **18,650**
   (+104 in `eb42c1347`, which discloses its own +35). The reference total is
   unchanged at 65,417 and the combined total is 84,067.
4. **§11.3 overstates the comparison.** The addendum §5 rows it points at come from
   `stage-5-component-comparison-20260917T073017Z`, which its own successor
   `…T143008Z/README.md:5-11` declares **not identity-matched and diagnostic only**.
   The eligible collection at `eb42c1347` reports 0.653 / 0.272 / 0.480
   candidate/reference with identity MATCH on all six pinned identities; the two
   collections disagree by up to 2.3x, so neither is a stable absolute.

### What the review confirmed as fixed (round-1 R1-R21)

Second-parent refusal, build reachability, the inode-serial range, the value bound
moved to the write path, a single leaf grammar, the old-Store role-constraint
refusal at open, the byte-bound listing refusal, the gated oracle seal, the
ordering byte owner and the honest expiry counters all stand on the current source,
and no round-1 HIGH/MEDIUM correctness finding reproduces.

### Blocking work routed to the terminal handoff

The five blocking actions and the non-blocking follow-ups are routed by
[`stage-5-terminal-handoff-20260917.md`](stage-5-terminal-handoff-20260917.md),
whose terminal condition is every Stage 5 and cumulative criterion passing with no
FAIL, INCOMPLETE or unowned row, followed by a clean closing review and the closure
of #170.
