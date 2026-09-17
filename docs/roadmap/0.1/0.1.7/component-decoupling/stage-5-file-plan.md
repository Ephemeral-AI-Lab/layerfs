# Stage 5: exact file plan and recommended production LOC

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Companion to the [Stage 5 handoff](stage-5-handoff.md) for
[#170](https://github.com/Ephemeral-AI-Lab/layerfs/issues/170).
This is the concrete starting map. Every new file requires real implementation;
no empty scaffolding. A justified merge/split/rename is allowed and must be reported.

## 1. Baseline and counting

Planning baseline: `b6b83162a8f4ca3ddb1a59adf1b1ea0cfb6d3369`. Recheck the actual start tree.
At this snapshot, `tools/production_loc.py --json` reports:

| Existing production scope | LOC |
| --- | ---: |
| C1 layerfs-content | 4,487 |
| C2 layerfs-storage, including SQL | 5,941 |
| C1 + C2 | 10,428 |
| Existing telemetry | 732 |
| Core total | 11,160 |

Ranges below are **final nonblank/non-comment production LOC**, including existing
code in modified files, imports/declarations and runtime SQL. They are not added
LOC, physical lines, savings promises or targets to fill. Tests/docs/examples/
tooling/manifests/third-party/generated code are excluded.

Hard caps separately count all physical lines: production <=999; lib.rs/mod.rs
<=200 and declarations/reexports/direct delegation only. Split by responsibility,
never minify or move implementation into tests/macros to satisfy a cap.
Report below/within/above recommendations honestly; correctness wins over estimates.

## 2. New C1 component

Home: `core/crates/layerfs-content/src/filesystem/`.
No new crate. The private sorted engine has real directory and inode formats;
it is not a runtime plugin or a generic framework for hypothetical types.
Attributes keep their exact existing partition algorithm, sharing checked helpers
where equivalent rather than forcing a different tree shape.
The selected scope is portable mode/mtime plus bounded generic attribute storage.
No Apple-specific module, native metadata calls or APFS integration is planned.

```text
filesystem/
├── attributes/
│   ├── build.rs
│   ├── codec.rs
│   ├── keys.rs
│   ├── mod.rs
│   ├── patch.rs
│   ├── portable.rs
│   ├── read.rs
│   └── value.rs
├── directory/
│   ├── codec.rs
│   ├── mod.rs
│   ├── read.rs
│   └── update.rs
├── identity.rs
├── inode/
│   ├── codec.rs
│   ├── mod.rs
│   ├── read.rs
│   └── update.rs
├── input.rs
├── limits.rs
├── mod.rs
├── path.rs
├── read.rs
├── references/
│   ├── backing.rs
│   ├── merge.rs
│   ├── mod.rs
│   ├── record.rs
│   ├── reduce.rs
│   ├── release.rs
│   └── runs.rs
├── root.rs
├── sorted/
│   ├── budget.rs
│   ├── finish.rs
│   ├── format.rs
│   ├── merge.rs
│   ├── mod.rs
│   └── page.rs
├── symlink.rs
├── update.rs
└── validate.rs
```

All these files are new: before=0.

| File under filesystem/ | Recommended final LOC | Responsibility |
| --- | ---: | --- |
| `mod.rs` | 10–24 | Public filesystem declarations/reexports only |
| `limits.rs` | 40–80 | Named path/page/identity and operation-resource limits |
| `identity.rs` | 80–150 | Scoped inode identity and caller-authorized serials |
| `path.rs` | 180–280 | Canonical names/paths, byte ordering and validation |
| `root.rs` | 100–180 | Checked scoped filesystem-root framing and known result |
| `input.rs` | 140–240 | Stable native binding/value streams and checked input ownership |
| `validate.rs` | 220–380 | Incremental membership/topology/identity checks and valid-input boundary |
| `update.rs` | 200–350 | One complete filesystem operation; ordering/finality/completion coordination |
| `read.rs` | 180–300 | Bounded resolve/stat/list coordination and shared inode demands |
| `symlink.rs` | 50–100 | Exact stored target grammar and readlink semantics |
| `sorted/mod.rs` | 6–14 | Private sorted-engine declarations |
| `sorted/format.rs` | 90–160 | Real directory/inode format operations and typed leaf values |
| `sorted/budget.rs` | 90–150 | Reserve-before-growth internal 4-MiB owner/leases |
| `sorted/page.rs` | 150–260 | Checked decoded pages/summaries and grouped acquisition |
| `sorted/merge.rs` | 260–440 | Sorted changes, untouched-subtree reuse and unresolved siblings |
| `sorted/finish.rs` | 140–240 | Exact partition/rebalance and final child-first emission |
| `directory/mod.rs` | 6–14 | Directory API exports |
| `directory/codec.rs` | 220–360 | Compact name-to-serial leaf/branch grammar |
| `directory/update.rs` | 140–240 | Sorted initial/update bindings and semantic edge observation |
| `directory/read.rs` | 160–280 | Grouped name lookups and bounded paginated listing |
| `inode/mod.rs` | 6–14 | Inode table API exports |
| `inode/codec.rs` | 120–220 | Branch grammar and context checks; reuse existing leaf/value codec |
| `inode/update.rs` | 100–180 | Sorted typed final records/tombstones directly to inline leaves |
| `inode/read.rs` | 150–260 | Shared ancestor traversal and ordered duplicate-key results |
| `attributes/mod.rs` | 8–18 | Attribute API exports |
| `attributes/keys.rs` | 60–100 | Generic structural domain/key checks and ordering; reserved portable keys |
| `attributes/portable.rs` | 70–130 | Portable mode/mtime values only |
| `attributes/codec.rs` | 130–220 | Checked exact page sizing and canonical attribute grammar |
| `attributes/build.rs` | 180–320 | Streaming construction, exact tail partition and final encode |
| `attributes/patch.rs` | 130–230 | Generic sorted patches preserving untouched keys and opaque value roots |
| `attributes/read.rs` | 120–210 | Bounded multi-key acquisition and typed fixed-size reads |
| `attributes/value.rs` | 70–130 | Extent-only value roots via existing file mapping bodies |
| `references/mod.rs` | 8–18 | Reference reducer declarations |
| `references/record.rs` | 100–170 | Compact edge/final-value record grammar without Workspace fields |
| `references/backing.rs` | 120–220 | Caller-supplied seekable record capability, quotas and cleanup ownership |
| `references/runs.rs` | 160–280 | Bounded pending values and tier/run descriptors |
| `references/merge.rs` | 200–340 | Two-reader tiered ordering with newest-value precedence |
| `references/reduce.rs` | 240–400 | New-inode counts, additions first, existing aliases and final values |
| `references/release.rs` | 160–280 | Paged zero-reference descendant release with latest pending state |

The existing `object/inode_leaf.rs` remains the single definition of InodeKind,
InodeValue, inline leaf values and pooled leaf interoperability. Do not create a
second 73-byte record type in filesystem/inode/. New inode/codec.rs covers branches
and context checks around that existing grammar. Resolve the handoff's 73-versus-81
summary discrepancy before relying on it for reference-exact filesystem roots.

The reference reducer's backing is a narrow supplied record capability. The caller
provides owned seekable handles with an explicit cleanup contract; C1 owns the
record format, ordering/merge algorithm and quota accounting. No Workspace path,
SQL scratch database, generic payload spool or implicit remote protocol belongs
in this API. A small operation may need no backing allocation; crossing a declared
record threshold is planned work, never a recovery path after allocation/I/O fails.

## 3. Existing C1 files to extend

Paths below are under `core/crates/layerfs-content/`. Unlisted source is retained;
if real integration requires more edits, report their paths/counts and justification.

| Existing file | Before LOC | Recommended final LOC | Responsibility |
| --- | ---: | ---: | --- |
| `src/lib.rs` | 24 | 26–50 | Expose the real filesystem API |
| `src/error.rs` | 120 | 160–250 | Precise filesystem/input/resource errors |
| `src/object/output.rs` | 121 | 145–230 | New logical roles and final referenced-object ownership |
| `src/object/inode_leaf.rs` | 307 | 260–420 | Golden-byte/header compatibility gate; one canonical leaf/value definition |
| `src/object/access.rs` | 18 | 18–60 | Retain narrow provider; scope/ownership adjustments only if actually needed |
| `src/file/mapping/build.rs` | 309 | 309–360 | Expose necessary extent-only attribute construction without a duplicate builder |
| `src/file/mapping/read.rs` | 223 | 223–280 | Reuse charged bounded value reads |

The edited baseline subtotal is **1,122** LOC; recommended after is
**1,141–1,650**. The remaining **3,365** C1 LOC are retained in this estimate.
Provider/timing changes are conditional on a real integration need: if the existing
contract suffices, retain it and report the smaller actual result.

## 4. Existing C2 integration files

Paths below are under `core/crates/layerfs-storage/`. No new C2 module, table or
metadata-pooling implementation is planned. Extend ordinary role dispatch and exact
schema validation; reuse the existing physical inode pool, dependencies and CAS.
If a source audit identifies another required file, report it explicitly.

| Existing file | Before LOC | Recommended final LOC | Responsibility |
| --- | ---: | ---: | --- |
| `src/cas/dependencies.rs` | 60 | 60–100 | Use final explicit object references; no inode-serial to fake-object-ID conversion |
| `src/encoding/full.rs` | 178 | 190–250 | Ordinary FULL for new logical roles; retain pooled inode leaf path |
| `src/encoding/delta/select.rs` | 268 | 275–330 | Explicit role/provenance eligibility; no payload delta for ordinary tree roles |
| `src/pack/layout.rs` | 390 | 400–470 | New role dispatch within existing qualified framing lanes |
| `src/policy.rs` | 187 | 190–260 | Actual accepted schema/profile support |
| `src/sqlite/schema.rs` | 232 | 245–340 | Exact schema/role compatibility validation |
| `sql/schema.sql` | 48 | 48–70 | Widen legitimate role constraints under the declared schema contract |

The edited subtotal is **1,363** LOC; recommended after is **1,408–1,820**.
The remaining **4,578** C2 LOC are retained. Current schema is version 4 with four
tables/twenty-one columns; new persisted role constraints require an explicit
compatibility/schema decision, not a silent rewrite of existing Stores.
Changing a role code is not permission to change canonical bytes or hashes.

## 5. Recursive directory and package estimates

New paths below are under `layerfs-content/src/`. Parent rows include descendants;
do not sum them with child rows.

| Directory | Before LOC | Recommended final LOC |
| --- | ---: | ---: |
| `filesystem/` | 0 | 4,594–7,982 |
| `filesystem/sorted/` | 0 | 736–1,264 |
| `filesystem/directory/` | 0 | 526–894 |
| `filesystem/inode/` | 0 | 376–674 |
| `filesystem/attributes/` | 0 | 768–1,358 |
| `filesystem/references/` | 0 | 988–1,708 |

Affected existing directories (under core/crates/), also recursive:

| Directory | Before LOC | Recommended final LOC |
| --- | ---: | ---: |
| `layerfs-content/src/` | 4,487 | 9,100–12,997 |
| `layerfs-content/src/object/` | 717 | 694–981 |
| `layerfs-content/src/file/` | 3,489 | 3,489–3,597 |
| `layerfs-content/src/file/mapping/` | 1,035 | 1,035–1,143 |
| `layerfs-storage/src/` | 5,893 | 5,938–6,328 |
| `layerfs-storage/src/cas/` | 1,413 | 1,413–1,453 |
| `layerfs-storage/src/encoding/` | 2,648 | 2,667–2,782 |
| `layerfs-storage/src/encoding/delta/` | 823 | 830–885 |
| `layerfs-storage/src/pack/` | 778 | 788–858 |
| `layerfs-storage/src/sqlite/` | 751 | 764–859 |
| `layerfs-storage/sql/` | 48 | 48–70 |

| Disjoint scope | Before | Recommended final | Estimated net change |
| --- | ---: | ---: | ---: |
| C1: retained + modified + new filesystem | 4,487 | 9,100–12,997 | +4,613 to +8,510 |
| C2: retained + integration changes | 5,941 | 5,986–6,398 | +45 to +457 |
| **C1 + C2** | **10,428** | **15,086–19,395** | **+4,658 to +8,967** |
| Existing telemetry | 732 | 732 | 0 |
| **Core including telemetry** | **11,160** | **15,818–20,127** | **+4,658 to +8,967** |

New filesystem directory: **39 production files, 4,594–7,982 LOC**.
The plan touches **14 existing production files**. It grows capability relative
to the file-content-only core; it does not claim net LOC reduction against that
smaller feature set. Compare simplification only with matched v0.1.6 responsibilities.
Reference retirement stays outside Stage 5. Extend guards for any real new shipped
source format; named backing records implemented in Rust are product code too.

## 6. Supporting files and tests

These are non-production. Keep all existing Stage 0–4 tests active.

```text
core/crates/layerfs-content/
├── README.md                            update actual filesystem APIs and limits
├── tests/
│   ├── filesystem_codec.rs               NEW exact framing/roles/path/root fixtures
│   ├── filesystem_reference.rs           NEW sealed reference roots and partitions
│   ├── filesystem_sorted.rs              NEW directory/inode engine boundaries
│   ├── filesystem_read.rs                NEW shared lookup/list/stat/readlink
│   ├── filesystem_updates.rs             NEW native final-state create/update/move/remove
│   ├── filesystem_hardlinks.rs           NEW aliases, counts and moved-out descendants
│   ├── filesystem_topology.rs            NEW effective-tree cycles/parents/retention
│   ├── filesystem_attributes.rs          NEW portable/generic keys, opaque preservation, sizing
│   ├── filesystem_ordering.rs            NEW compact records, tiers and resource ownership
│   ├── filesystem_failure.rs             NEW late input/read/output/backing errors
│   ├── filesystem_bounds.rs              NEW live-state, size/count/depth/limit evidence
│   ├── filesystem_timing.rs              NEW independent real C1 timing and disabled path
│   └── support/
│       ├── mod.rs                       existing; preserve existing helpers
│       └── filesystem.rs                NEW shared external fixture/provider helpers
└── examples/
    └── filesystem_timing_c1.rs           NEW DB-free native filesystem operation

core/crates/layerfs-storage/
├── README.md                            update supported roles/profile and integration
├── tests/
│   ├── filesystem_pipeline.rs            NEW real C1/C2 save/reopen/readback
│   ├── filesystem_failure.rs             NEW private visibility/late failure/old roots
│   └── support/
│       └── filesystem.rs                NEW only genuinely shared C2 test helpers
└── examples/
    └── measure_filesystem.rs             NEW C1/C2/integrated real modes
```

Recommended sizes: 120–400 nonblank code lines per new external test target,
50–180 per helper, 100–250 for the C1 example and 160–320 for the integrated example.
These are guidance, not production LOC. Split tests for a real concern, not one
test per implementation function. Oracle fixture files are small external test
assets or retained sealed evidence; do not include private src/ implementations
from tests or add product-only visibility to make them accessible.

Modify core/README.md, the roadmap index/tracker and actual API documentation.
No new crate/member/dependency is expected. Manifests/lockfile change only for a
demonstrated need under existing dependency rules, never to add a test-only feature.
Counter/guard changes require a real coverage gap and focused external tool tests.

Before new benchmark work the implementation agent creates and commits
`stage-5-verification.md` beside this file, linked to #170 and the existing harness.
Completion produces `stage-5-report.md` and fresh evidence under the v0.1.7 evidence
directory. These are future deliverables, not present qualification results.

## 7. Actual result reporting

Use the same audited counter/version on exact before/after snapshots. Audit SQL
and legacy test exclusions; don't substitute raw line counts or Git diff stats.
For every changed/new/removed file and recursive directory report:

```text
path | before production LOC | actual after | signed delta
     | recommended final range | below/within/above | physical lines
     | responsibility / deviation explanation
```

Include actual package/reference/adapter/combined totals and each commit's
first-parent comparison with required commit-message accounting. Preserve inherited
source during migration; relocation, new behavior and reference deletion are
different effects. Recompute after amendments/rebases or scope changes.
