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

### Correction 2026-09-18 (R2-F4): six per-commit LOC disclosures do not reproduce

The round-2 review found that six Stage-5 commits disclose a Production LOC the
committed tree does not have, that the disclosure chain switches scope without a
stated convention change, and that one merge carries no disclosure at all. History
is not rewritten; the recomputed rows are published here beside the disclosed
ones. The re-audit receipt is
[`../evidence/stage-5-terminal-20260918T120000Z/per-commit-loc-reread.log`](../evidence/stage-5-terminal-20260918T120000Z/per-commit-loc-reread.log)
(`tools/production_loc.py` on each commit's exact first parent and committed
tree, via `git archive`; run 2026-09-18) and it reproduces the review's own
`per-commit-loc-stage5.log` column for column.

| commit | disclosed | recomputed | drift |
| --- | --- | --- | --- |
| `07f0fe8eb` | 11160 -> 17497 (+6337) | 11160 -> 17523 (+6363) | +26 |
| `5d08d9e83` | 17497 -> 17525 (+28) | 17523 -> 17563 (+40) | +12 |
| `bfd7abf2c` | 17525 -> 17730 (+205) | 17563 -> 17698 (+135) | -32/-38 |
| `c17f59bef` | 17730 -> 17730 (0) | 17698 -> 17697 (-1) | -32 |
| `821ddbe30` | 17909 -> 17913 (+4) | 17909 -> 17898 (-11) | -15 |
| `f723663a5` | 17913 -> 17905 (-8) | 17898 -> 17905 (+7) | -15 |

The `drift` column is the round-2 review's own annotation, kept as published; it
mixes delta-drift and endpoint-drift per row, so read it as the review's gloss on
its two exact columns (disclosed, recomputed), not as a single formula.

The other fourteen Stage-5 production commits reproduce exactly, as the review
also found. The disclosed chain is internally self-consistent
(17497 -> 17525 -> 17730) and diverges from the committed trees by at most 38
lines: the numbers were prepared from a pre-commit tree and never re-confirmed
after the commit, which is what the repository rule forbids.

**One more row, found by this round's own audit (2026-09-18):** `de648507b`
(post-review, documentation only) discloses `18650 -> 18650 (delta 0)` where the
counter reproduces `18708 -> 18708 (delta 0)` - the delta is right, but the
levels were copied from the preceding product commit's message
(`2fe2a4642`, +58) instead of being re-run on `de648507b`'s own first parent.
Every other commit from `f288d2af7` to the closing tree reproduces exactly
(re-audited 2026-09-18 across
[`per-commit-loc-reread.log`](../evidence/stage-5-terminal-20260918T120000Z/per-commit-loc-reread.log),
[`…-2.log`](../evidence/stage-5-terminal-20260918T120000Z/per-commit-loc-reread-2.log)
covering `f288d2af7..99743b2cf`, and
[`…-3.log`](../evidence/stage-5-terminal-20260918T120000Z/per-commit-loc-reread-3.log)
covering `99743b2cf..HEAD`, including the closing falsifier's own recount of
`bfd7abf2c`, `3ecb952c8` and the final tree).

Two further disclosure defects, both stated here rather than repaired in history:

- **Scope switch at `01d9f70f3`.** Commits up to `2b2dbc028` (the last
  combined-disclosing commit, `64e3f9d6a` among them) disclose **combined**
  core+reference totals (e.g. `74628 -> 77136`); commits from `01d9f70f3`
  disclose **core-only** totals (e.g. `10415 -> 10893`). Each commit states its
  own scope in its Method line, so no single commit lies, but the chain cannot
  be read across the switch without knowing this. From the remediation rounds
  onward every disclosure names its scope explicitly (core subtotals, reference
  subtotal, combined total).
- **Merge `a8a1ba848` (PR #163) carries no `Production LOC:` line.** Its
  first-parent comparison, recomputed now: `732 -> 732` (delta 0, first parent
  `e6ecb70d1`; the change was documentation).

### Totals of the tree that carries this correction (R2-F15)

Stated at the final round-4 tree (`4d887a6b9` and the closing remedies after it):
C1 **11,917**, C2 **6,112**, telemetry **763**, core total **18,792**, reference
unchanged at **65,417**, combined **84,209** (`tools/production_loc.py`, same
counter and scope; reproduced by the closing falsifier's independent recount).
The first version of this block, landed at `134b8df73`, stated 18,797 / 84,214
and promised the production totals would not move again in the round; that
promise was wrong - the verification-remedy commit `3ecb952c8` deleted the dead
`4 MiB − 1` scratch twin and its accessor (−5 production lines), and the closing
verification pass caught the stale headline as its finding F2 (the exact failure
mode this row is about). Every per-commit disclosure in the round reproduces
exactly; this block now states the tree that carries it.

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
| Merge buffers | `merge_buffer_bytes`, default 16 KiB | one per live tier - a merge reader or a retained lookup scan - plus one output | dropped with the run; lookup scans are dropped when their tier's run is replaced |
| Release cursors | one per released directory level | depth-bounded stack, 64-entry pages | popped when the page ends |
| Object boundary | none: reads are borrowed from the provider | one outstanding page group | per call |

Measured peaks are reported by the runs (`peak_scratch_bytes`, `rows_spilled`,
`released`, `report_nodes`). No cgroup or lifetime figure is claimed.

### Simultaneous memory and backing during one update (TR-5, measured 2026-09-18)

What one update operation holds **at once**, with scope, unit and the counter
that produced it. Receipt:
[`../evidence/stage-5-terminal-20260918T120000Z/simultaneous-memory.log`](../evidence/stage-5-terminal-20260918T120000Z/simultaneous-memory.log)
and the scaling grid beside it, one sample per case, release profile, public
entry points only, `maximum_pending_records = 64`, a 4,000-file base and 2,000
rename pairs:

| Simultaneous owner (references phase) | Scope | Unit and value | Counter |
| --- | --- | --- | --- |
| pending rows | the reducer's pending map | 64 rows = 6,144 B | `references.peak_pending` × 96 B/row |
| ordering bytes | live runs + spilled-but-unmerged inputs + pending + reserved outputs | 568,320 B | `references.runs.peak_run_bytes` |
| live-tier scan buffers | one retained reader buffer per live tier, ≤ 32 × 16 KiB | 5 tiers = 81,920 B | `references.runs.peak_live_runs` × `merge_buffer_bytes` |
| merge-reader buffers | two fresh reader buffers per live merge, coexisting with the tier scans during the phase's final consolidate (which merges before the scans are dropped) | 2 × 16,320 B; with the scans, ≈ 7 × 16,320 = 114,240 B in this fixture; ≤ (32 + 2) buffers in general | derived from `merge.rs` (`merge_runs` holds two readers); `verify-TR5.md` finding F1 |
| `FinalRows` stream | the reducer's output stream, allocated in the references phase and persisting into the inodes phase (run buffer 32 × 96 × 4 = 12,288 B plus the re-collected pending rows and the ≤ 32-row lookahead/wave/serials/bases state, ≈ 15 KB class) | ≈ 15 KB class | `reduce.rs`; `verify-TR5.md` finding F2 |
| caller backing | the physical owner of the run files | 568,320 B peak | `FileBacking::peak_bytes` |

Pre-phase owners disclosed beside the table (found by the verification pass,
`verify-TR5.md` finding F3): the touched-serials collection (`Vec<u64>`, one per
touched inode, counter `references.serials_scanned`, bytes counted nowhere) and
the release cursors (depth-bounded, empty in this fixture) coexist with the
pending map and the runs between the directories and references phases.

The scratch leases are held in their own sequential phases, not during the
references phase: directories `peak_scratch_bytes` 376,110 B, inodes
`peak_scratch_bytes` 349,820 B. The phases are sequential by construction
(`validate`, `directories`, `references`, `inodes`, `cleanup`, `root.encode`).
**No process-level RSS, cgroup or page-cache figure is claimed**: every number
above is one of the operation's own counters, and a process-level measurement
remains Stage 6's to take.

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
| Attribute value | enforced | ≤ 32,768 bytes (the chunk maximum), always extent-backed |
| Symlink target | enforced | ≤ 4,096 bytes, no NUL |
| Inode serial | enforced | 1 .. `i64::MAX` |
| Read wave | enforced | ≤ 32 payload objects and ≤ 1,048,576 bytes (`READ_WAVE_OBJECTS × MAXIMUM_CHUNK_BYTES`): the count is enforced as payloads are demanded, and each decoded payload is refused above the chunk maximum, so the byte figure is a bound on what a wave can acquire rather than an estimate. The two constants are asserted equal to the product by the wave's own case |
| Whole-tree walk entries | enforced | ≤ 4,096 bindings **per walk**, charged once per walk and not once per operation: a build stating 4,096 bindings is accepted and 4,097 is the first refused, and a directory whose effective subtree reaches the ceiling can never be rebound, because the base-tree walk charges the rest of the tree beside it first (`MAXIMUM_WALK_ENTRIES`; figures corrected 2026-09-18 from the review's file-count phrasing - see §13.2) |
| Operation scratch | configurable | ≥ 1,024 bytes, default 4 MiB (`MAXIMUM_OPERATION_SCRATCH_BYTES`, one constant; the unused `4 MiB − 1` twin and its dead accessor were deleted 2026-09-18) |
| Pending records | configurable | ≥ 1, default 4,096 |
| Read-wave demands | enforced | ≤ 4,096 ids per wave through `FilesystemObjects::read_batch` (`MAXIMUM_READ_DEMANDS`): 4,096 demands pass the count check and reach the provider, 4,097 is refused by the declared bound (boundary case `filesystem_limits::a_read_wave_is_accepted_at_4096_demands_and_refused_at_4097`) |
| Ordering tiers | derived, unverified at scale | ≤ 32 (`MAXIMUM_LEVELS`); tiers fill like a binary counter's bits, so all 32 are occupied only after 2³² − 1 spills and the 33rd-tier refusal is not fixture-reachable; the figure bounds the store's retained lookup buffers and is pinned by `filesystem_limits` |
| Pack group count | enforced at placement and parse | ≤ 256 groups per lane pack (`GROUP_COUNT_LIMIT`): placement starts a new pack at a lane's ceiling and the pack parser refuses counts outside 1..=256; no fixture fills a pack to the ceiling through a real save - derived, unverified at scale, value and per-lane limits pinned by `storage_limits` |
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

1. **§6, the attribute-value row was wrong, and the bound is now the row.**
   The enforced bound is **32,768 bytes**, not 1 MiB: an attribute value is one
   extent-only root whose payload is one canonical chunk object, so the 1 MiB
   branch the table advertised was unreachable. Reproduced through the public API:
   a 1 MiB value is refused with `object limit 32768 exceeded by 1048576`. Matrix
   row `AT-4` was promoted on the wrong figure. **Remedied** (WP-D, round 3):
   `limits.rs` now derives `MAXIMUM_ATTRIBUTE_VALUE_BYTES` from
   `cdc::MAXIMUM_CHUNK_BYTES` instead of restating a larger figure, so the two
   cannot drift; the §6 row above states 32,768 and names the grammar that fixes
   it; and the boundary case
   `filesystem_failure::the_attribute_value_bound_is_the_chunk_maximum_at_its_boundary`
   emits and reads back a value of exactly 32,768 bytes and refuses 32,769 with the
   declared bound as the reported limit. The previous case wrote 4,096 bytes and so
   did not discriminate at any bound.
2. **§6 omits a capacity limit that changes what a Workspace can build.**
   `MAXIMUM_CYCLE_CHECK_ENTRIES = 4,096` (`filesystem/validate.rs:53`) is enforced
   per whole-tree walk, so a single `build_filesystem` call is refused above
   **4,095 directory bindings** (4,095 accepted, 4,096 `BUILD REFUSED invalid
   record: cycle check work limit`), and an existing directory whose effective
   subtree exceeds 4,096 entries can never be rebound (a 4,195-entry rename is
   refused the same way).

   *Erratum corrected 2026-09-18:* the figures in this item are the round-2
   review's own probe outputs, counted in **files inside the built directory**,
   which excludes the directory's own binding edge. Counted in the bindings the
   walk charges - the counting the §6 row now uses - a build stating exactly
   4,096 bindings is accepted and 4,097 is the first refusal, and the rebind
   refusal begins when the subtree reaches 4,096, because the base-tree walk
   charges the rest of the tree beside the rebound directory first. A round-4
   verification probe reproduced both tight figures through the public API
   (`verify-R2-F7.md`); the ceiling and both consequences are unchanged.
3. **§2 totals are one production commit stale.** This section is re-derived at
   `b3df5461c`; the reviewed tree is C1 **11,875**, C2 **6,043**, core **18,650**
   (+104 in `eb42c1347`, which discloses its own +35). The reference total is
   unchanged at 65,417 and the combined total is 84,067.
4. **§11.3 overstates the comparison — resolved by owner decision, 2026-09-17.**
   The addendum §5 rows this section pointed at came from
   `stage-5-component-comparison-20260917T073017Z`, which its own successor
   `…T143008Z/README.md:5-11` declares **not identity-matched and diagnostic only**.
   The owner has decided that the **eligible collection at `eb42c1347` governs**
   (0.653 / 0.272 / 0.480 candidate/reference, identity MATCH on all six pinned
   identities), that addendum §5 is corrected to cite it, and that the older
   collection is superseded. Addendum §5.1 now carries the governing rows and §5.2
   retains the older ones as the record. The two collections disagree by up to 2.3x
   on the candidate arm; that spread is an open investigation, not a licence to
   quote either set outside its own run.

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

## 14. Round-3 implementation progress, 2026-09-18 (R3)

> **Status: implementation progress, not acceptance.** This section records what
> the round changed and what it did not. It marks **no** row PASS: a row flips
> only when an independent reviewer reproduces its evidence from the receipts.

| | |
| --- | --- |
| Tree | `5ca20eb925134fb675eb1451af5f03436a8639bf` (branch `main`, pushed) |
| Commits | `5e2a20a0c` harness case selection + §6 corrections, `2fe2a4642` product bounds/telemetry/C1 contracts, `afcb76c0e` receipts, `9b58d1a17` this section, `5ca20eb92` the external probe |
| Evidence | [`../evidence/stage-5-terminal-20260918T020000Z/`](../evidence/stage-5-terminal-20260918T020000Z/) |
| Production LOC | core 18,650 → **18,708** (+58); C1 11,875 → 11,902 (+27), C2 6,043 → 6,043 (0), telemetry 732 → 763 (+31); reference 65,417 unchanged |
| Checks | all eight handoff §6 checks exit 0; 62 result blocks, **422 passed / 0 failed** |

### Rows this round implemented, with the receipt that decides them

| Row | Change | Receipt |
| --- | --- | --- |
| `R2-F1` | `filesystem_timing_c1 --case attributes` now applies a real attribute patch inside the timed region and charges its object boundary to the row | `case-selection/c1-attribute-case/*.log` |
| `R2-F2` | `measure_filesystem --mode c2` applies the case's own change set outside the timed region and admits those objects | `case-selection/c2-*.log` |
| `R2-F3` | `measure_edits --mode c2` builds its two C1 objects in a labelled untimed step and reads back in its own labelled region | `case-selection/edits-c2/small.log` |
| `R2-F6`, `AT-4`, `R2-F25` | the value bound is the chunk maximum (32,768), derived rather than restated; boundary case writes 32,768 and refuses 32,769 | `attribute-boundary.log` |
| `R2-F7`, `VF-4` | `limits::MAXIMUM_WALK_ENTRIES` declared with both consequences; a case grows a directory past it and shows the rename refused | `walk-ceiling.log` |
| `R2-F9` | the dead `LookupScan::settled` state and its false doc are deleted; the duplicated `from` arm is collapsed | source; `check-cargo-test.log` |
| `R2-F12`, `R2-F13` | `Completeness::{Disabled,Complete,Clipped}` and `NodeOutcome::Unknown` | `telemetry.log` |
| `R2-F14` | `measure_components` fails a clipped run with a non-zero exit | source; `check-clippy.log` |
| `R2-F17` | `encode_node(node, root)` validates the page in its own context; the builder, edit tree and attribute value pass theirs | `encoder-context.log` |
| `R2-F18` | `construct_bytes`, `construct_stream`, `apply_edits` call `validated()` before any work | `policy-validation.log` |
| `R2-F19` | the whole-file encoder's comment states the two allocations the memory ledger records | source |
| `R2-F20` | the chunked edit route uses the view's decoded state; a counting provider asserts one read of the base root | `edit-single-read.log` |
| `R2-F21`, `N-11` | the payload wave enforces the chunk maximum on every decoded payload and the wave byte ceiling; the largest legal wave equals the declared figure | `read-wave.log` |
| `R2-F22` | `into_parts` returns `ObjectParts`, which carries the advisory predecessors | `into-parts.log` |
| `R2-F26` | one scratch ceiling, one name (`MAXIMUM_OPERATION_SCRATCH_BYTES`) | source |
| `N-16`, `VF-4` | `filesystem_limits` and `storage_limits` boundary suites, plus the named limits no fixture can reach with their arithmetic | `filesystem-limits.log`, `storage-limits.log` |

### Rows this round did **not** close

- **`R2-F8` / `N-6`** — `RunStore::find` still builds a `RunReader` per lookup. The
  reader borrows the tier's run, so one reader per tier needs the store to own
  each reader beside its run; that ownership change was not made, and **no scaling
  receipt** is claimed.
- **`R2-F4` / `VF-7`** — §2's totals are corrected in §13 but the six per-commit
  LOC disclosures are not yet recomputed row by row.
- **`R2-F5` / `VF-5`** — comparison governance is not decided; the addendum still
  cites the superseded collection.
- **`VF-6`** — the complete-operation comparison remains `NOT_RUN` and still needs
  an owner disposition.
- **`TR-5`**, **`VF-3`**, **`N-13`** — the simultaneous-memory row, the
  non-discriminating cases and the `forbid(unsafe_code)` decision are unchanged.

### Independent review status

**Not obtained.** The handoff requires a fresh reviewer for each round; no
external agent could be started in this environment (`codex exec` rejects every
model available to this account with *"not supported when using Codex with a
ChatGPT account"* / *"requires a newer version of Codex"*, and the `claude` CLI
fails with *"OAuth access token has been revoked"*). The
`diagnostics/s5check/` client retained with the round's evidence is an
**author-run** reproduction of the public-API rows and is labelled as such; it is
not a substitute for the review, and no row in §14 is marked PASS on its
strength.

## 15. Round-4 implementation progress, 2026-09-18 (R4)

> **Status: implementation progress; the verification-subagent pass is run
> > separately and its outcome is recorded in §16.** This section marks no row
> > PASS on its own strength.

| | |
| --- | --- |
| Commits | `6c00e0f53` provider failures / pragma set / profile verification / unsafe boundary, `9327f6695` one reader per ordering tier |
| Evidence | [`../evidence/stage-5-terminal-20260918T120000Z/`](../evidence/stage-5-terminal-20260918T120000Z/) |
| Production LOC | core 18,708 → **18,797** (+89); C1 11,902 → 11,922 (+20), C2 6,043 → 6,112 (+69), telemetry 763 → 763 (0); reference 65,417 unchanged |
| Diagnostics client | `diagnostics/s5term/` in the evidence directory, public entry points only, one sample per case, release profile |

### Rows this round implemented, with the receipt that decides them

| Row | Change | Receipt |
| --- | --- | --- |
| `R2-F10` / `N-7` | `StoreProvider` maps `ObjectMissing` to `MissingObject` (the value-root absence contract) and every corrupt/refused class to the new `ContentError::ProviderFailure`; the provider trait contract states the distinction | `tests/provider_errors.rs` (3 cases: corrupted pack, absent object, unpublished record) |
| `R2-F23` / `N-17` | the integer-pragma helper takes a closed `Pragma` enum; `format!("PRAGMA {name}")` is gone | source; `tests/policy_capacity.rs` re-pointed at the enum |
| `R2-F24` | `MutationOwner::acquire` re-verifies the connection profile (journal/synchronous/foreign-keys/busy-timeout) before its first write | `tests/connection_profile.rs` (4 cases) |
| `N-13` | `unsafe` denied crate-wide, allowed on exactly the audited `encoding/codec.rs` with its FFI inventory; the boundary guard enforces it; the `forbid` deviation is an accepted design note with the inventory | `physical-encoding-and-packing.md` note; guard self-tests; `check-boundary.log` |
| `R2-F8` / `N-6` | one reader and buffer per tier, kept across lookups; a continuing lookup is served from the retained buffer, a restart keeps it; the dead `LookupScan::total` is gone; the module docs match the code | `tests/filesystem_ordering_scan.rs` (counting allocator: the lookup wave allocates zero times); `ordering-scaling.log` |
| `R2-F11` / `N-8` (+ `F27`) | the SQL transaction and preparation-batch rows are stated as commit/flush triggers with the code's own semantics; nothing was weakened | `admission-and-persistence.md` correction note |
| `N-14` | the caller-owned input bound is declared as an explicit adapter obligation (protocol-level request ceiling, enforced by refusal) | `filesystem-tree.md` §9 declaration |
| `TR-5` | what one update holds at once, measured with the operation's own counters, with scope/unit/counter per row and no process-level claim | `simultaneous-memory.log` |
| `R2-F4` / `VF-7` | the six per-commit LOC rows recomputed beside the disclosed ones, the scope switch stated, the merge's missing line supplied; the audit re-runs and reproduces both columns | `per-commit-loc-reread.log`; §2 correction above |
| `R2-F15` / `VF-7` | §2 states the totals of the tree that carries it | §2 totals block above |

### The ordering scaling receipt, stated honestly

`ordering-scaling.log` repeats the round-2 review's P1 grid on the fixed tree
(4,000-file base, `maximum_pending_records = 64`, release profile, one sample
per case). The work counters are **identical** to the review's pre-fix grid —
rows spilled 448/960/1,984/3,968, `rows_read` 2,198/6,684/19,960/59,007,
`rows_written` 1,588/4,328/10,896/25,760, runs 14/30/62/124, peak owned bytes
66,432/139,008/284,160/568,320 — so the reader-per-tier hoist changes no
observable work. The per-doubling read ratio remains ~×3.0 (2,198 → 59,007 over
three doublings, ≈ n^1.58): **the read amplification is still superlinear, and
the reason is the tiered merge itself, which is O(n log n) in the change count
at a fixed pending ceiling.** No O(changes) claim is made. Elapsed times are
16.3 / 32.7 / 79.6 / 157.6 ms against the review's 15.8 / 35.6 / 87.6 / 154.5 ms
single samples on the same machine, within sample noise (the verification pass
measured a ±15% elapsed spread between identical binaries on this machine, and
the receipt's figures sit within ±9% of the review's; `verify-R2-F8-scaling.md`),
no faster claim. What
the hoist removes is not visible in these counters and is pinned by the product
test instead: the per-lookup 16 KiB allocation and buffer re-read are gone (the
counting-allocator test asserts a zero-allocation lookup wave and a one-pass
ascending sweep).

**Dated correction (2026-09-17, research):** the attribution two paragraphs
above — "the reason is the tiered merge itself" — is incomplete. Decomposing the
same receipt shows the write term (rows written by merges/copies/spills) indeed
tracks the tiered-merge floor (×2.72/2.52/2.36 per doubling ≈ Θ(r·log₂(r/P))),
but the **larger read residual** (rows read minus rows written: ×3.86/3.84/3.67
per doubling ≈ n^1.9) comes from the lookup path: `spill()` resets every tier's
scan even for tiers whose runs the spill did not touch, so a passed cursor
restarts its tier from the front. The receipt's numbers are unchanged; the
mechanism is now correctly attributed, and removing it is a documented
optimization opportunity — see
[`complexity-and-roundtrip-research-20260917.md`](complexity-and-roundtrip-research-20260917.md)
(entry 4 of its Tier 0 register).

### What this round does **not** close

- Nothing else. Every row in the terminal handoff's ledger is now either
  implemented with a receipt (rounds 3-4), owner-decided (`VF-5`, `VF-6`), or a
  dated record. The remaining work is verification: the round-3 rows have never
  been independently verified, and §16 records the verification-subagent pass
  over the whole ledger on the final tree.

## 16. Final matrices and the verification pass (round 4, 2026-09-18)

> **Status: the terminal state.** Every row below carries the evidence that
> decides it. The round-4 verification pass ran one read-only verification
> subagent per row group on the round tree, adjudicated every finding, remedied
> what was real (commit `3ecb952c8`), and re-verified each remedy; the two
> measurement receipts were reproduced by their own verifiers. The verifier
> reports are the `verify-*.md` files in
> [`../evidence/stage-5-terminal-20260918T120000Z/`](../evidence/stage-5-terminal-20260918T120000Z/).

### Verification outcomes, row group by row group

| Verifier report | Rows | Outcome |
| --- | --- | --- |
| `verify-R2-F1-F2-F3-F14.md` | TR-1, TR-2 (F14 half), N-1 | PASS; F14's clipped-run receipt established as impossible with legal inputs (statement in the round README); the round-3 commit message's "2 read / 6 emitted" phrasing recorded as a loose aggregate of the printed lines |
| `verify-R2-F6-AT4-F25.md` | AT-4, VF-3 (named case), N-4, R2-F25 | PASS |
| `verify-R2-F7.md` | N-5, VF-4 (walk row), N-15 | PASS, with the boundary erratum (4,096 accepted / 4,097 refused) corrected in the remedy commit |
| `verify-R2-F12-F13.md` | N-9, N-10, TEL-4 | PASS |
| `verify-R2-F17-F18.md` | R2-F17, R2-F18 | PASS |
| `verify-R2-F19-F20-F22-F26.md` | R2-F19, R2-F20, R2-F22, R2-F26 | PASS; F20's chunked-route coverage gap remediated with a new case and re-verified |
| `verify-N11-F21.md` | N-11, R2-F21 | PASS, with the qualification recorded: the byte ceiling lives at C1's payload wave (where the bytes flow); C2's own wave and `FilesystemObjects::read_batch` remain count-only, documented as counts, with the review's seam arithmetic for a future adapter |
| `verify-N16-VF4.md` | N-16, VF-4 | PASS after remedy (the `MAXIMUM_LEVELS` derived note and pin, the `MAXIMUM_READ_DEMANDS` boundary case, the group-count row, the scratch figure correction, the dead twin deleted, the stale references fixed) |
| `verify-R2-F10-F23-F24.md` | N-7, N-17, R2-F10, R2-F23, R2-F24 | PASS, with three recorded caveats (the dependency-read collapse to absence on a tampered store is unreachable in product-written stores; locator-byte errors pass through as content errors; the public low-level sqlite helpers accept any connection) |
| `verify-N13.md` | N-13 | PASS after remedy (the FFI inventory completed to all nineteen entry points, re-verified) |
| `verify-F11-F27-N14.md` | N-8, N-14, S2-4 (F27), R2-F11 | PASS; the N-14 walk-ceiling attribution and the F11 overshoot characterization corrected in the remedy commit |
| `verify-R2-F4-F15.md` | N-2, VF-7, R2-F4, R2-F15 | PASS; the scope-switch boundary and the drift column's nature recorded; the round's own audit found and recorded `de648507b`'s stale disclosed levels |
| `verify-VF5-VF6-F5.md` | N-3, VF-5, VF-6, R2-F5 | PASS; two superseded "10% slower" echoes in the completion report corrected by its §11 |
| `verify-VF3.md` | VF-3 | PASS after remedy (the two named-vs-asserted gaps strengthened and re-verified) |
| `verify-R2-F8-N6.md` | N-6, R2-F8, R2-F9 | PASS (code, docs, dead states, allocation test) |
| `verify-R2-F8-scaling.md` | N-6 (scaling receipt) | the receipt reproduced: work counters identical to the review's pre-fix grid, amplification still superlinear with the O(n log n) reason stated |
| `verify-TR5.md` | TR-5 | the coexistence receipt reproduced with the operation's own counters; scope/unit/counter per row; no process-level claim |
| `verify-close-correctness.md` | Stage 5 CI/PS/SI/SC/WT/RA/OR | closing pass: all 41 rows CONFIRMED; the cited artifacts byte-unchanged since the review tree where claimed; falsification (golden bytes, multi-move cycles) positive |
| `verify-close-reads-attributes.md` | Stage 5 RD/AT/C2/TR | closing pass: all groups CONFIRMED; AT-4 and RD-4 falsified live through the public API; TR-1 re-ran the examples; the extended §5 coexistence table matches the code |
| `verify-close-evidence-limits.md` | Stage 5 VF + N | closing pass: CONFIRMED; VF-6's disposition verified and no complete-operation claim anywhere; all 19 §6 rows classed; the corrected walk figures reproduced by its own probe |
| `verify-close-cumulative.md` | cumulative matrix + routes | closing pass: CONFIRMED (34 PASS + 2 owner-WAIVED of 36; waived rows unpromoted; routes green; the sixth follows VF-6) |
| `verify-close-terminal-checklist.md` | handoff §8 items 1-10 | closing falsifier: items 1, 3, 4, 7, 10 satisfied; items 2, 6, 9 satisfied with six bookkeeping findings (F1-F6), all remediated below |

The closing falsifier's six findings and their remedies (all bookkeeping; the
falsifier itself confirmed every per-commit disclosure and every measurement
receipt it checked reproduces exactly):

- **F1** — §16's Stage 5 table displayed 84 row-equivalents (the cumulative
  matrix's N-13 double-listed) against the stated denominator 83. Remedied: the
  N-13 row is now annotated as a cumulative-matrix row counted only in that
  denominator.
- **F2** — §2's R2-F15 totals block stated 18,797 / 84,214 and promised no
  further moves; the remedy commit's −5 dead-code deletion made the final tree
  18,792 / 84,209. Remedied: the block now states the final figures and records
  the failure it caught (the exact failure mode the row is about).
- **F3** — the case-selection receipts covered 18/18 `measure_filesystem`
  combinations but only 2/6 `filesystem_timing_c1` cases and 1/15
  `measure_edits` combinations. Remedied: `case-selection-r4/` in the evidence
  directory completes the matrix (all six `filesystem_timing_c1` cases, all
  fifteen `measure_edits` case×mode combinations, each exit 0 with distinct
  per-case work), and the N-1 row carries the coverage note, including the five
  WP-A examples that have no `--case` flag to select.
- **F4** — the LOC re-audit stopped at `99743b2cf`. Remedied:
  `per-commit-loc-reread-3.log` audits every commit from there to the closing
  tree (all reproduce exactly, including the owner's architecture commits).
- **F5** — `head.txt` was refreshed mid-round (9327f6695 → 134b8df73) against
  the README's "nothing was edited" letter. Remedied: the README now states the
  identity files are tree pointers refreshed at the check-log commit to name
  the tree the checks ran on; the receipts themselves are unedited.
- **F6** — the walk ceiling's tight boundary (4,096 accepted / 4,097 refused)
  rested on the verifier's probe, not a committed test. Remedied:
  `the_cycle_check_work_limit_is_reachable_and_reported` now pins both sides of
  the tight boundary and asserts the accepted build charges exactly the
  ceiling's entries.

### Stage 5 matrix - final (denominator 83)

Rows whose round-2 status was PASS and whose artifacts are unchanged carry
"PASS (round 2)" with the round-2 review's citation; the round-4 suite on the
final tree is green (65 result blocks, 434 tests passed, 0 failed). Remediated
rows cite the round that remediated them and the verifier that confirmed it.

| id | criterion | status | final evidence |
| --- | --- | --- | --- |
| CI-1..CI-6 | shared inode value codec, one grammar, golden bytes, framing/role identity, page ordering, gated oracle | PASS (round 2) | round-2 review §4.1; suites green on the final tree |
| PS-1..PS-4 | schema/profile checks, refusal without migration, pool expansion, non-compact refusal | PASS (round 2) | round-2 review §4.1 |
| SI-1..SI-4 | scoped identity, range, no fake ids, derived counts | PASS (round 2) | round-2 review §4.1 |
| SC-1..SC-6 | optional base, exact partitions, tail rebalance, height collapse, changed-path work, grouped reads | PASS (round 2) | round-2 review §4.1 |
| WT-1..WT-6 | effective topology, cycles, duplicates, single parent, root ops, disconnected inputs | PASS (round 2) | round-2 review §4.1 |
| RA-1..RA-7 | additions before removals, counts derived once, aliases, precedence, survival, bounded release, old roots | PASS (round 2) | round-2 review §4.1 |
| OR-1..OR-8 | record grammar, thresholds, tombstones, overflow, backing, cleanup, no alternate route, honest counters | PASS | OR-8's F8 caveat resolved: the counters describe the work and §15 states the amplification honestly; `verify-R2-F8-N6.md`, `verify-R2-F8-scaling.md` |
| RD-1..RD-6 | public reads, duplicate demands, shared ancestors, bounded pagination, rejection, no hidden collection | PASS | RD-6's F7 caveat resolved: the walk ceiling is declared; `verify-R2-F7.md` |
| AT-1..AT-3 | portable grammar, key bounds, opaque preservation | PASS (round 2) | round-2 review §4.1 |
| AT-4 | bounded extent-only values | PASS | 32,768 enforced and documented; boundary case; `verify-R2-F6-AT4-F25.md` |
| AT-5..AT-7 | exact sizing, no platform dispatch, scoped parity | PASS (round 2) | round-2 review §4.1 |
| C2-1..C2-4 | roles through real save, references, pooling, no per-object flush | PASS (round 2) | round-2 review §4.1 |
| TR-1 | real bodies, the case selects the operation | PASS | round-3 remedy; `verify-R2-F1-F2-F3-F14.md` |
| TR-2 | bounded reports, honest clipping | PASS | the pair (round 2) plus `measure_components` (round 3); the clipped-run receipt's impossibility stated in the round README |
| TR-3, TR-4 | same body on/off, attributed waits | PASS (round 2) | round-2 review §4.1 |
| TR-5 | simultaneous memory and backing costs | PASS | measured coexistence row with scope/unit/counter; `verify-TR5.md`, `simultaneous-memory.log` |
| VF-1, VF-2 | targets exist, seals recorded | PASS (round 2) | round-2 review §4.1 |
| VF-3 | meaningful assertions | PASS | the named case discriminates at the real bound; the hunt found two weak bodies, both strengthened; `verify-VF3.md` |
| VF-4 | limits evidence | PASS | complete table, every row with a boundary case or a derived note with arithmetic; `verify-N16-VF4.md` |
| VF-5 | required comparison against the pinned reference | PASS | owner decision 2026-09-17: the eligible collection at `eb42c1347` governs; `verify-VF5-VF6-F5.md` |
| VF-6 | complete-operation comparison | **NOT_RUN - owner disposition** | deferred to Stage 6 (#171) by owner decision, recorded in addendum §6; not a waiver, not promoted; Stage 5 makes no complete-operation claim |
| VF-7 | LOC census and plan accounting | PASS | §2 corrections; `verify-R2-F4-F15.md` |
| VF-8 | oracle reproduced independently | PASS (round 2) | round-2 review §4.1 |
| N-1 | every `--case`/`--mode` selects the claimed operation | PASS | `verify-R2-F1-F2-F3-F14.md`; the closing pass completed the receipt matrix (`case-selection-r4/`: all six `filesystem_timing_c1` cases and all fifteen `measure_edits` case×mode combinations, each exit 0 with distinct per-case work) - the three examples with a `--case` flag are fully receipted, and the five WP-A examples without the flag (`measure_components`, `measure_pooled`, `filesystem_primitives_candidate`, `edit_timing_c1`, `memory_ledger`) select nothing and claim nothing |
| N-2 | per-commit LOC disclosure reproducible | PASS | correction record + re-audit receipts; `verify-R2-F4-F15.md` |
| N-3 | addendum cites the eligible receipt | PASS | `verify-VF5-VF6-F5.md` |
| N-4 | attribute value bound documented as enforced | PASS | `verify-R2-F6-AT4-F25.md` |
| N-5 | whole-tree operation ceiling declared | PASS | declared with both consequences; tight figures; `verify-R2-F7.md` |
| N-6 | ordering work measured, state honest | PASS | one reader per tier, dead states gone, zero-allocation lookups, scaling receipt with the amplification stated; `verify-R2-F8-N6.md`, `verify-R2-F8-scaling.md` |
| N-7 | C2 failures reach C1 with distinguishable classes | PASS | `verify-R2-F10-F23-F24.md` |
| N-8 | declared transaction bound is a bound | PASS | stated as a commit trigger with the code's semantics; `verify-F11-F27-N14.md` |
| N-9 | disabled and clipped distinguishable | PASS | `verify-R2-F12-F13.md` |
| N-10 | a failed child reports a failed outcome | PASS | `verify-R2-F12-F13.md` |
| N-11 | read wave has a declared byte ceiling | PASS | enforced at the payload wave with a refusal test; qualification recorded; `verify-N11-F21.md` |
| N-12 | revision and payload reclamation | NOT_APPLICABLE | unchanged (no DELETE exists except failed-save cleanup; retention is a later owner's) |
| **N-13** | `layerfs-storage` unsafe boundary (a cumulative-matrix row, shown here for completeness; counted in the cumulative denominator only, not in this table's 83) | PASS | audited module boundary, deny elsewhere, guard-enforced, design note with the complete inventory; `verify-N13.md` |

**Stage 5 totals: 81 PASS, 0 FAIL, 0 PARTIAL/INCOMPLETE, 1 NOT_RUN with a
written owner disposition (VF-6, deferred to Stage 6), 1 NOT_APPLICABLE (N-12),
0 unowned - of 83.** The two Stages 3-4 owner-WAIVED rows are not in this
denominator and remain waived, unmeasured and unpromoted.

### Cumulative Stages 0-5 matrix - final (denominator 36)

| id | criterion | status | final evidence |
| --- | --- | --- | --- |
| S01-1..S01-6 | Stages 0-1 contracts and construction | PASS (round 2) | round-2 review §4.2; suites green on the final tree |
| S2-1..S2-9 | Stages 2 CAS, batches, profile, visibility, cleanup | PASS | S2-4's batch byte bound now stated as a flush trigger (F27 note); `verify-F11-F27-N14.md` |
| S3-1..S3-5 | cutoffs, deltas, reconstruction, pooling | PASS (round 2) | round-2 review §4.2 |
| S3-6 | Stage 3 performance/resource claims | **owner-WAIVED** | unchanged; not counted as PASS |
| S4-1..S4-4 | edits, splits, convergence, transitions | PASS (round 2) | round-2 review §4.2 |
| S4-5 | Stage 4 performance claims | **owner-WAIVED** | unchanged; not counted as PASS |
| TEL-1..TEL-4 | one hierarchy, disabled path, bounded retention, one monitor | PASS | TEL-4's F12/F13 caveats resolved; `verify-R2-F12-F13.md` |
| X-1 | no retry/fallback/fsync/WAL | PASS (round 2) | round-2 review §4.2; the round-4 changes add no such path |
| N-13 | unsafe boundary | PASS | `verify-N13.md` |
| N-14 | operation ceiling covers the whole operation | PASS | the caller-owned input bound declared as an adapter obligation; `verify-F11-F27-N14.md` |
| N-15 | a build's own operation is bounded | PASS | ceiling enforced and declared with tight figures; `verify-R2-F7.md` |
| N-16 | every public limit has boundary evidence | PASS | two boundary suites plus derived notes with arithmetic; `verify-N16-VF4.md` |
| N-17 | no caller string interpolated into SQL | PASS | closed `Pragma` enum; `verify-R2-F10-F23-F24.md` |

**Cumulative totals: 34 PASS, 0 FAIL, 0 PARTIAL/INCOMPLETE, 2 owner-WAIVED
(excluded from the PASS count), 0 unowned - of 36.**

### Cumulative integration routes - final

The five routes the round-2 review verified PASS remain PASS on the final tree
(suites green). The sixth, "measured route with real payloads", was INCOMPLETE:
it is the complete-operation measurement row and follows `VF-6`'s owner
disposition - deferred to Stage 6 (#171). The real-payload correctness proof on
the composed routes is unchanged and is carried by the tests (`core_pipeline`
6 MiB + 1 end to end, `filesystem_pipeline`'s save/reopen cases), not by the
measurement harness, exactly as the round-2 review recorded.

### Rows that remain unmeasured, with their reasons

- **Complete-operation comparison (`VF-6`)**: deferred to Stage 6 by owner
  decision; Stage 5 makes no complete-operation performance claim.
- **Cold-cache, pack-footprint and process-level memory rows**: warm
  in-process fixtures only; the operation's own counters are the Stage 5
  evidence; a process-level figure is Stage 6's to take.
- **`MAXIMUM_LEVELS`, the 256-group pack ceiling, a level-31 tree**: derived
  arithmetic recorded, unverified at scale, for the reasons in the limits
  suites' notes.
- **The storage side's fixed work budgets** (`ENCODE_WORKSPACE_BYTES`,
  `DECODE_WORKSPACE_BYTES`, the pooled index/cache windows, the comparison
  window): sized allocations rather than refusal boundaries - a caller cannot
  exceed them, the codec charges them before use, and they are declared in the
  Stages 3-4 record. They are not Stage 5 limit rows and carry no boundary case,
  by scope rather than by omission (`verify-N16-VF4.md` records the same
  reading).
- **The two Stages 3-4 performance rows**: owner-waived, unmeasured,
  unpromoted.
