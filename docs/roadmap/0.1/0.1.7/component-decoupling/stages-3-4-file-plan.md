# Stages 3–4: exact starting file plan and recommended production LOC

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Companion to the [Stages 3–4 handoff](stages-3-4-handoff.md), covering
[#168](https://github.com/Ephemeral-AI-Lab/layerfs/issues/168) and
[#169](https://github.com/Ephemeral-AI-Lab/layerfs/issues/169).
This is a concrete starting map, not permission to create empty modules. Implement
a file only with its real responsibility. A justified merge/split/rename is allowed;
report the changed map, actual counts and reason. Keep helpers private unless the
real standalone/integrated API needs them.

## 1. Baseline, units and hard limits

Planning baseline: commit `5e8b8cbc2`, after the Stage 2 visibility and pending-group
fixes. Re-resolve its full commit/tree IDs and the chosen implementation start before
editing; later valid work supersedes this planning observation.

The repository counter currently reports C1 **2,336**, C2 **3,084** including
46 runtime SQL LOC, combined **5,420** across 45 files. Existing telemetry is
732 production LOC and is outside the new estimates. These are starting counts,
not a statement that all earlier qualification gaps are resolved.
Reproduce with `python3 tools/production_loc.py --files` and `--json`.
Use the final audited counter identically on before/after snapshots; the prior
review found reference `cfg(not(test))` classification errors. Do not propagate
uncorrected reference totals into a commit comparison.

All ranges below describe **final production LOC after Stages 3–4**, including
existing code in a touched file, not additions. They count nonblank/non-comment
product source, imports/declarations and shipped SQL. They exclude tests, fixtures,
examples, docs, manifests, benchmark/tool code and third-party/generated output.
A new path has before=0; the old codec file has after=0 because its implementation
moves into a responsibility-based folder. Relocation is not an algorithmic saving.

Independent physical limits: every production file <=999 lines including comments/
blanks; lib.rs/mod.rs <=200, declarations/reexports/direct delegation only. Split
by responsibility before exceeding a limit. Never minify or remove checks to fit
a range. Estimates are recommendations: report below/within/above and explain
material differences. Do not pad a small implementation to reach the low estimate.

## 2. Exact product source tree

Existing packages and existing telemetry are reused. No additional crate, generic
plugin framework, buffer manager, Workspace adapter or benchmark runtime is created.
The following tree lists every planned final C1/C2 production file; Cargo.toml,
README.md, tests/ and examples/ are supporting files described in section 5.

### C1: `core/crates/layerfs-content/`

```text
└── src/
    ├── error.rs
    ├── file/
    │   ├── cdc/
    │   │   ├── gear.rs
    │   │   └── mod.rs
    │   ├── content.rs
    │   ├── edit/
    │   │   ├── apply.rs
    │   │   ├── compare.rs
    │   │   ├── concat.rs
    │   │   ├── finish.rs
    │   │   ├── frontier.rs
    │   │   ├── input.rs
    │   │   ├── mod.rs
    │   │   └── split.rs
    │   ├── mapping/
    │   │   ├── build.rs
    │   │   ├── codec.rs
    │   │   ├── mod.rs
    │   │   ├── predecessor.rs
    │   │   ├── read.rs
    │   │   └── types.rs
    │   ├── mod.rs
    │   ├── read.rs
    │   └── view.rs
    ├── lib.rs
    ├── object/
    │   ├── access.rs
    │   ├── codec.rs
    │   ├── id.rs
    │   ├── inode_leaf.rs
    │   ├── mod.rs
    │   ├── output.rs
    │   └── predecessor.rs
    └── policy.rs
```

| File | Action | Before LOC | Recommended final LOC | Responsibility |
| --- | --- | ---: | ---: | --- |
| `src/lib.rs` | Update | 16 | 18–35 | Public edit and object reexports |
| `src/error.rs` | Update | 116 | 120–190 | Edit/input/capacity errors |
| `src/policy.rs` | Update | 106 | 150–250 | Validated configurable policy and derived C1 capacities |
| `src/object/mod.rs` | Update | 12 | 14–24 | Declarations/reexports |
| `src/object/id.rs` | Retain | 89 | 89 | Frozen identity |
| `src/object/codec.rs` | Update | 107 | 107–150 | Canonical limits independent of routing cutoff |
| `src/object/access.rs` | Update | 18 | 40–90 | Bounded shared authenticated owners; ordered demands |
| `src/object/output.rs` | Update | 111 | 100–170 | Moved final bytes and bounded advisory predecessors |
| `src/object/predecessor.rs` | New | 0 | 45–85 | Bounded candidates and explicit provenance |
| `src/object/inode_leaf.rs` | New | 0 | 160–260 | Checked compact inode-leaf grammar needed for C2 pooling only |
| `src/file/mod.rs` | Update | 10 | 14–24 | File operation reexports |
| `src/file/content.rs` | Update | 195 | 150–230 | Capacity-aware complete/whole construction |
| `src/file/read.rs` | Update | 111 | 90–160 | Read using an operation-local authenticated view |
| `src/file/view.rs` | New | 0 | 90–160 | Open/authenticate base once; scoped logical reads |
| `src/file/cdc/mod.rs` | Retain | 5 | 5 | Frozen CDC exports |
| `src/file/cdc/gear.rs` | Retain | 494 | 494 | Frozen table and algorithm |
| `src/file/mapping/mod.rs` | Update | 15 | 18–30 | Mapping reexports |
| `src/file/mapping/types.rs` | Update | 182 | 180–250 | Checked summaries, extent bounds and roles |
| `src/file/mapping/codec.rs` | Update | 303 | 280–380 | Frozen canonical node grammar; reuse checked decode |
| `src/file/mapping/build.rs` | Update | 236 | 220–330 | Final child-first streaming emission |
| `src/file/mapping/read.rs` | Update | 210 | 180–290 | Grouped node/payload acquisition and shared repeated demand |
| `src/file/mapping/predecessor.rs` | New | 0 | 100–180 | Bounded reference cursor with original/current coordinate distinction |
| `src/file/edit/mod.rs` | New | 0 | 8–16 | Edit declarations/reexports |
| `src/file/edit/input.rs` | New | 0 | 90–160 | Checked ordered edit stream and stable replacement range capability |
| `src/file/edit/apply.rs` | New | 0 | 130–230 | Known final-size dispatch and sequential edit operation |
| `src/file/edit/compare.rs` | New | 0 | 80–140 | Bounded applicable no-op comparison and replay |
| `src/file/edit/frontier.rs` | New | 0 | 180–300 | Owned decoded unfinished nodes and charged bounds |
| `src/file/edit/split.rs` | New | 0 | 160–260 | Path-local split preserving slices/subtrees |
| `src/file/edit/concat.rs` | New | 0 | 210–350 | Join/coalesce/partition/root-collapse rules |
| `src/file/edit/finish.rs` | New | 0 | 110–190 | Proven finality, child-first sealing and final root |

The new `object/inode_leaf.rs` only owns the checked canonical leaf/value grammar
needed by physical pooling. Port the exact existing representation and necessary
reference validation; it does not implement directory traversal, inode allocation,
hardlink updates or filesystem-tree construction. Stage 5 reuses that grammar.
`object/predecessor.rs` carries bounded advisory identities/provenance, never a
codec, SQL connection, host role or mutable Workspace handle.

### C2: `core/crates/layerfs-storage/`

```text
├── sql/
│   └── schema.sql
└── src/
    ├── cas/
    │   ├── batch.rs
    │   ├── dependencies.rs
    │   ├── finish.rs
    │   ├── membership.rs
    │   ├── mod.rs
    │   ├── owner.rs
    │   ├── read.rs
    │   ├── save.rs
    │   └── store.rs
    ├── encoding/
    │   ├── codec/
    │   │   ├── decode.rs
    │   │   ├── encode.rs
    │   │   ├── mod.rs
    │   │   └── profile.rs
    │   ├── decode.rs
    │   ├── delta/
    │   │   ├── candidates.rs
    │   │   ├── mod.rs
    │   │   ├── read.rs
    │   │   ├── record.rs
    │   │   └── select.rs
    │   ├── full.rs
    │   ├── mod.rs
    │   └── pool/
    │       ├── delta.rs
    │       ├── index.rs
    │       ├── leaf.rs
    │       ├── mod.rs
    │       ├── read.rs
    │       └── value_group.rs
    ├── error.rs
    ├── lib.rs
    ├── pack/
    │   ├── assemble.rs
    │   ├── layout.rs
    │   ├── mod.rs
    │   ├── placement.rs
    │   ├── read.rs
    │   └── singleton.rs
    ├── policy.rs
    └── sqlite/
        ├── cleanup.rs
        ├── connection.rs
        ├── lookup.rs
        ├── mod.rs
        ├── pool.rs
        ├── schema.rs
        └── write.rs
```

| File | Action | Before LOC | Recommended final LOC | Responsibility |
| --- | --- | ---: | ---: | --- |
| `src/lib.rs` | Update | 11 | 14–28 | Storage/encoding public reexports only |
| `src/error.rs` | Update | 105 | 120–200 | Explicit physical/dependency/capacity failures |
| `src/policy.rs` | Update | 148 | 180–300 | Resolved profile, cutoff/depth/work/frame capacities |
| `src/cas/mod.rs` | Update | 11 | 14–24 | CAS declarations/reexports |
| `src/cas/store.rs` | Update | 327 | 260–420 | Scoped store/read/save ownership and bounded Store-owned caches |
| `src/cas/owner.rs` | Update | 364 | 320–520 | Writer state, physical lanes, publication/cleanup invariants |
| `src/cas/batch.rs` | Update | 58 | 90–160 | Byte/count admission and planned singleton |
| `src/cas/save.rs` | Update | 49 | 90–170 | Batched membership/bases and moved input; no per-object clone |
| `src/cas/membership.rs` | Update | 42 | 70–140 | Exact reuse/collision under valid ownership |
| `src/cas/dependencies.rs` | Update | 62 | 90–170 | Logical references plus selected physical dependencies |
| `src/cas/read.rs` | Update | 65 | 120–220 | Grouped acquisition, retained ceiling and shared decoded owners |
| `src/cas/finish.rs` | Update | 16 | 20–50 | Final drain, acknowledgement and one terminal disposition |
| `src/encoding/mod.rs` | Update | 9 | 12–24 | Encoding exports |
| `src/encoding/full.rs` | Update | 112 | 110–190 | Capacity-aware existing FULL alternatives |
| `src/encoding/decode.rs` | Update | 103 | 100–180 | Explicit physical dispatch and canonical authentication |
| `src/encoding/codec.rs` | Retire | 367 | 0 | Move and extend into codec/; count relocation once |
| `src/encoding/codec/mod.rs` | New | 0 | 8–16 | Codec declarations/reexports |
| `src/encoding/codec/profile.rs` | New | 0 | 80–140 | Pinned parameters and checked workspace/frame capacities |
| `src/encoding/codec/encode.rs` | New | 0 | 220–340 | Reused bounded FULL/PREFIX and group compression workspace |
| `src/encoding/codec/decode.rs` | New | 0 | 260–420 | Frame checks, prefix lifetimes and bounded decompression |
| `src/encoding/delta/mod.rs` | New | 0 | 8–16 | Payload delta declarations/reexports |
| `src/encoding/delta/record.rs` | New | 0 | 100–180 | WHOLE_FILE/CHUNK FULL/PREFIX framing |
| `src/encoding/delta/select.rs` | New | 0 | 160–280 | One trial, role-aware cost/eligibility/work accounting |
| `src/encoding/delta/read.rs` | New | 0 | 170–300 | Iterative dependency reconstruction and intermediate checks |
| `src/encoding/delta/candidates.rs` | New | 0 | 100–180 | Admitted-FULL cache/signatures and ordered bounded candidate acquisition |
| `src/encoding/pool/mod.rs` | New | 0 | 8–16 | Physical pooling exports |
| `src/encoding/pool/value_group.rs` | New | 0 | 140–240 | Build/hash/compress exact value-group body once |
| `src/encoding/pool/index.rs` | New | 0 | 160–280 | Bounded BTreeSet window, sync, equality candidates and invalidation |
| `src/encoding/pool/leaf.rs` | New | 0 | 140–240 | Canonical rows to pooled ordinals and inverse mapping |
| `src/encoding/pool/delta.rs` | New | 0 | 160–280 | Pooled COPY/INSERT selection/codec with exact cost rules |
| `src/encoding/pool/read.rs` | New | 0 | 160–280 | Bounded value-group authentication and per-chain work accounting |
| `src/pack/mod.rs` | Update | 10 | 14–24 | Pack declarations/reexports |
| `src/pack/layout.rs` | Update | 321 | 280–440 | Checked explicit format/locator grammar including selected capacities |
| `src/pack/placement.rs` | Update | 123 | 130–230 | Fit/base chronology before assembly |
| `src/pack/assemble.rs` | Update | 195 | 180–300 | Borrowed groups; assemble one selected write |
| `src/pack/read.rs` | New | 0 | 100–190 | Grouped body/record views; avoid repeated group decode |
| `src/pack/singleton.rs` | New | 0 | 90–160 | Budget-checked consuming singleton assembly |
| `src/sqlite/mod.rs` | Update | 8 | 10–20 | SQLite exports |
| `src/sqlite/connection.rs` | Update | 50 | 50–80 | Selected no-WAL/no-sync/zero-retry profile |
| `src/sqlite/schema.rs` | Update | 223 | 220–340 | Explicit schema/profile compatibility and new valid policy ranges |
| `src/sqlite/lookup.rs` | Update | 118 | 130–220 | Paged locator/base lookups and required indexes |
| `src/sqlite/write.rs` | Update | 83 | 120–220 | Atomic packs/locators/base/catalogue writes |
| `src/sqlite/cleanup.rs` | Update | 58 | 80–150 | Known-owned reverse dependency cleanup and cache invalidation |
| `src/sqlite/pool.rs` | New | 0 | 120–200 | Bounded value-group catalogue queries/writes under one ceiling |
| `sql/schema.sql` | Update | 46 | 80–130 | Exact roles/policy/constraints/indexes; retain publication watermark |

Move the existing `encoding/codec.rs` into `encoding/codec/` as its FULL/PREFIX
encoding, decoding and capacity responsibilities grow. Preserve the actual pinned
parameters, reset/error behavior and FFI safety checks. This is one implementation,
not retained old/new dispatch. The new `pack/read.rs` owns grouped record/body
views; `encoding/decode.rs` continues to own representation dispatch and reconstruction.

Stage 2 already implements basic compression, pack placement, visibility and cleanup.
Extend those paths. Do not count their existing cuts a second time or rewrite them
solely to match this map. Pending-group sealing on demand must retain its exact-reuse
and same-save-read regression tests; do not restore whole-payload retention just to
avoid a seal without proving its simultaneous memory and packing costs.

## 3. Recursive directory estimates

All paths below are under `core/crates/`. Parent rows include child directories;
do not sum them together. Retired `encoding/codec.rs` contributes to the old
encoding/ total; the new codec/ directory starts at zero.

| Directory | Before production LOC | Recommended final production LOC |
| --- | ---: | ---: |
| `layerfs-content/src/` | 2336 | 3632–5522 |
| `layerfs-content/src/object/` | 337 | 555–868 |
| `layerfs-content/src/file/` | 1761 | 2789–4179 |
| `layerfs-content/src/file/cdc/` | 499 | 499 |
| `layerfs-content/src/file/mapping/` | 946 | 978–1460 |
| `layerfs-content/src/file/edit/` | 0 | 968–1646 |
| `layerfs-storage/src/` | 3038 | 5008–8578 |
| `layerfs-storage/src/cas/` | 994 | 1074–1874 |
| `layerfs-storage/src/encoding/` | 591 | 2096–3602 |
| `layerfs-storage/src/encoding/codec/` | 0 | 568–916 |
| `layerfs-storage/src/encoding/delta/` | 0 | 538–956 |
| `layerfs-storage/src/encoding/pool/` | 0 | 768–1336 |
| `layerfs-storage/src/pack/` | 649 | 794–1344 |
| `layerfs-storage/src/sqlite/` | 540 | 730–1230 |
| `layerfs-storage/sql/` | 46 | 80–130 |

| Disjoint scope | Before | Recommended after | Recommended net change |
| --- | ---: | ---: | ---: |
| C1 | 2,336 | 3,632–5,522 | +1,296 to +3,186 |
| C2, including SQL | 3,084 | 5,088–8,708 | +2,004 to +5,624 |
| **C1 + C2** | **5,420** | **8,720–14,230** | **+3,300 to +8,810** |

Final map: **74 production files**: 30 C1 and 44 C2. It adds 30 paths and retires
one existing path. This expands the supported product capabilities; it is not a
claim of net code reduction against the much smaller Stage 2 feature set.
Compare algorithm simplification only against the corresponding v0.1.6
responsibilities with a declared source map. Reference retirement remains later.

## 4. Required actual-size report

In `stages-3-4-report.md`, include:

```text
path | action | before production LOC | actual after | signed delta
     | recommended final range | below/within/above
     | physical lines | explanation and responsibility
```

Include every added, removed, merged or renamed file, recursive directory totals
and disjoint package totals. Report C1/C2/telemetry/reference/application/combined
product subtotals where applicable. Audit each Git commit against its first parent
and exact committed tree using the same counter; preserve the required commit
message `Production LOC: <before> -> <after> (delta <signed difference>)`.
Recompute after amendments/rebases. Working-tree counts are a separate observation,
not a substitute for commit accounting.

## 5. Supporting files and external tests

These are **not production LOC**. Keep existing tests and adapt them to the genuine
public API when necessary. No src/ includes, test-only visibility or fake product
hooks. Tiny oracle fixtures may live under the actual tests/ directory.

```text
core/crates/layerfs-content/
├── Cargo.toml                          existing; change only for a real dependency need
├── README.md                           update actual APIs, policy and limits
└── tests/
    ├── object_identity.rs              existing
    ├── file_complete.rs                extend accepted-cutoff coverage
    ├── file_read.rs                    extend grouped/repeated demand coverage
    ├── streaming.rs                    extend slow-consumer/error bounds
    ├── timing.rs                       existing complete-file timing
    ├── inode_leaf.rs                   NEW checked pooling-input grammar
    ├── edit_single.rs                  NEW single-edit operations and boundaries
    ├── edit_batch.rs                   NEW ordered stream and frontier/finality
    ├── edit_transitions.rs             NEW threshold/empty conversions
    ├── edit_noop.rs                    NEW exact-root preservation and replay
    ├── edit_model.rs                   NEW frozen-reference/model equivalence
    ├── edit_bounds.rs                  NEW ownership/bounds/failures
    ├── edit_timing.rs                  NEW independently callable edit timing
    └── support/
        ├── mod.rs                     existing
        └── edits.rs                   NEW only shared external test helpers

core/crates/layerfs-storage/
├── Cargo.toml                          existing; reuse exact dependency pins
├── README.md                           update schema/profile/memory/format contract
├── examples/
│   ├── measure_components.rs           preserve existing real modes
│   └── measure_edits.rs                NEW real C1/C2/integrated edit demonstration
└── tests/
    ├── cas_roundtrip.rs                existing; new roles and declared formats
    ├── cas_reuse.rs                    preserve all same/cross-wave fixes
    ├── pack_locator.rs                 extend lanes/append/singletons
    ├── persistence_failure.rs          extend selected-base/pool cleanup
    ├── memory_bounds.rs                extend simultaneous allocation accounting
    ├── timing.rs                       extend encoding/pooling and disabled behavior
    ├── core_pipeline.rs               existing complete-file regressions
    ├── visibility.rs                  preserve publication watermark/quarantine
    ├── delta_payload.rs                NEW role-specific selection and candidates
    ├── delta_chains.rs                 NEW iterative reconstruction/limits/errors
    ├── metadata_pool.rs                NEW value groups and pooled FULL/DELTA
    ├── metadata_pool_index.rs          NEW window/ordinal/collision/sync equivalence
    ├── physical_formats.rs             NEW explicit dispatch and compatibility
    ├── policy_capacity.rs              NEW configurable cutoff/depth/capacity
    ├── edit_pipeline.rs                NEW real edits/save/reopen/readback
    └── support/
        ├── mod.rs                     existing
        └── physical.rs                NEW only shared external test helpers
```

Recommended external-test sizes: 100–350 nonblank code lines per new target;
40–160 per shared helper; 150–300 for measure_edits.rs. These are guidance only,
separate from production estimates. Add a focused test for a real missing case
instead of one test per private helper. Non-product placement is not permission
to hide shipped implementation in tests/examples.

Update `core/README.md`, the roadmap index, implementation issue mapping and the
new completion report. Change `core/Cargo.toml` / lockfile only if genuinely needed;
no new members or third-party dependency should be needed by this plan.
Touch boundary/LOC tools only for a demonstrated coverage defect, with focused
tool tests. No aggregate preflight, CI workflow or replacement wrapper is added.

The handoff owns test coverage and commands. It also requires a committed,
case-specific verification/measurement contract before benchmark implementation or
collection, reusing existing harness mechanisms. That specification and retained
evidence are non-production artifacts; this map does not scaffold a new harness.

