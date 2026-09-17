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
| C1 `layerfs-content` | 4,487 | 11,001 | +6,514 |
| C2 `layerfs-storage` | 5,941 | 5,964 | +23 |
| C1 + C2 | 10,428 | 16,965 | +6,537 |
| Existing telemetry | 732 | 732 | 0 |
| Core total | 11,160 | 17,697 | +6,537 |

New files under `core/crates/layerfs-content/src/filesystem/` (24 production
files; the plan's 39-file map is reached with fewer, larger files, reported
below). Every file is under the 999-physical-line ceiling and every `mod.rs` is
under 200 lines.

| File | LOC | Plan range | Inside? |
| --- | ---: | ---: | --- |
| `mod.rs` | 28 | 10–24 | above |
| `limits.rs` | 20 | 40–80 | below |
| `identity.rs` | 41 | 80–150 | below |
| `path.rs` | 147 | 180–280 | below |
| `objects.rs` | 95 | — (justified addition) | — |
| `root.rs` | 119 | 100–180 | inside |
| `symlink.rs` | 70 | 50–100 | inside |
| `input.rs` | 128 | 140–240 | below |
| `validate.rs` | 299 | 220–380 | inside |
| `update.rs` | 333 | 200–350 | inside |
| `read.rs` | 196 | 180–300 | inside |
| `sorted/mod.rs` | 9 | 6–14 | inside |
| `sorted/budget.rs` | 87 | 90–150 | below |
| `sorted/format.rs` | 603 | 90–160 | **above** |
| `sorted/page.rs` | 447 | 150–260 | **above** |
| `sorted/merge.rs` | 278 | 260–440 | inside |
| `sorted/finish.rs` | 154 | 140–240 | inside |
| `directory/mod.rs` | 6 | 6–14 | inside |
| `directory/codec.rs` | 156 | 220–360 | below |
| `directory/update.rs` | 39 | 140–240 | below |
| `directory/read.rs` | 260 | 160–280 | inside |
| `inode/mod.rs` | 6 | 6–14 | inside |
| `inode/codec.rs` | 145 | 120–220 | inside |
| `inode/update.rs` | 72 | 100–180 | below |
| `inode/read.rs` | 186 | 150–260 | inside |
| `attributes/mod.rs` | 16 | 8–18 | inside |
| `attributes/keys.rs` | 77 | 60–100 | inside |
| `attributes/portable.rs` | 70 | 70–130 | inside |
| `attributes/codec.rs` | 322 | 130–220 | **above** |
| `attributes/build.rs` | 386 | 180–320 | **above** |
| `attributes/patch.rs` | 181 | 130–230 | inside |
| `attributes/read.rs` | 167 | 120–210 | inside |
| `attributes/value.rs` | 61 | 70–130 | below |
| `references/mod.rs` | 15 | 8–18 | inside |
| `references/record.rs` | 117 | 100–170 | inside |
| `references/backing.rs` | 116 | 120–220 | below |
| `references/runs.rs` | 247 | 160–280 | inside |
| `references/merge.rs` | 121 | 200–340 | below |
| `references/reduce.rs` | 465 | 240–400 | **above** |
| `references/release.rs` | 132 | 160–280 | below |

Four files exceed their recommended maximum and six fall below their minimum;
correctness and one-responsibility-per-file drove the split. Deviations are
reported, not hidden: `sorted/format.rs` carries both real page formats plus the
exact-size arithmetic the encoder shares; `sorted/page.rs` carries the read paths
and the decoded-page lifecycle; `attributes/build.rs` and `references/reduce.rs`
carry the streaming partitioner and the reducer's bounded waves.

Edited existing files: `src/lib.rs` (+7), `src/error.rs` (+33),
`src/object/inode_leaf.rs` (+28), `src/object/output.rs` (+52). C2:
`src/pack/layout.rs`, `src/encoding/full.rs`, `src/encoding/delta/select.rs`,
`sql/schema.sql`, plus `src/policy.rs`'s existing default arm.

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
- Ordering: one fixed 88-byte record grammar (version, tag, length, optional
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
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked` | PASS (all targets, including the eight new Stage 5 targets) |
| `cargo +1.85.1 test --locked -p layerfs-content --test stage5_reference_fixtures` (reference workspace) | PASS — fixtures regenerated identically |
| `python3 core/tools/check_product_boundary.py` | PASS — 115 production files |
| `python3 -m unittest discover -s core/tools -p 'test_*.py'` | PASS |
| `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | PASS |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | PASS |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | PASS |
| `python3 tools/production_loc.py --files` | see §2 |
| Reference-vs-replacement timing campaign | **NOT RUN** (no comparative arm exists) |

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
