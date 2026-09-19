# Stage 7 — the unchanged-consumer substitution matrix

> **Status:** Acceptance evidence for the concrete scenario in
> [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172). Append-only.
> **No product source was changed**, no timing claim is made, and nothing here
> amends Stage 6's receipts.

`stage-7-architecture-review.md` and #172 require "a reproducible substitution
matrix for C1, C2 and their composition": can integration develop against one
qualified C1/C2 revision while another worktree optimizes C1/C2, then adopt the
optimized revision through dependency selection and rebuild **without rewriting
integration logic**? This note answers it with a build-and-run experiment rather
than with source inspection.

## 1. Revisions and consumer

| | value |
| --- | --- |
| baseline revision | `66bce8378b5e9ecb1135b1636f8ee2ffac46ccf5` (qualified before the retained-history optimization) |
| candidate revision | `f8c3e3d5f` (contains `795fb1a2f`: the C1 chunk cursor, the persisted similarity index, the lean row grammar, schema 4 → 6) |
| consumer | frozen copies under `core/tools/substitution-proof/consumer/`, compiled unchanged into every arm |
| toolchain | `cargo +1.85.1` |
| profile | `debug` (this is a compatibility proof, not a measurement) |

The consumer is the repository's **own external test and example**, not a
purpose-built stub:

| file | sha256 | role |
| --- | --- | --- |
| `core_pipeline.rs` | `f22fc3dd5441c05c…` | integrated C1 → C2 consumer: `construct_stream` → `SaveHandoff` → `finish` → reopen → `StoreProvider` logical read |
| `support/mod.rs` | `0f944594b970e3be…` | its support module |
| `support/filesystem.rs` | `74340ebec9fd191e…` | its filesystem helper module |
| `filesystem_primitives_candidate.rs` | `1b50e9c5b908b0ea…` | C1 primitives driver that prints deterministic identities |
| `cross_revision.rs` | `18ff0a425be6e212…` | written for this proof: `create <path>` / `read <path> <root>` for the persisted-data checks |

`filesystem_primitives_candidate.rs`'s digest matches the one recorded for that
path in the pair-3 packet's `source-snapshot.json`, which confirms the frozen copy
is the pinned revision's file.

## 2. Method

Four arms. The consumer source is identical in all of them; **the only difference
is which revision each product dependency resolves to**:

| arm | C1 | C2 |
| --- | --- | --- |
| `baseline` | `66bce8378` | `66bce8378` |
| `c1-only` | candidate | `66bce8378` |
| `c2-only` | `66bce8378` | candidate |
| `combined` | candidate | candidate |

Each arm gets its own revision tree; `layerfs-telemetry` is byte-identical in the
two revisions (`git diff --stat 66bce8378 795fb1a2f -- core/crates/layerfs-telemetry`
is empty) and is materialized once per tree so a mixed arm still has exactly one
copy in its graph. There is **no `[patch]`, no vendoring, no fork and no third-party
modification**; the arms differ only in manifest path selection, and every
manifest, tree and consumer hash is recorded in `matrix.json`.

Reproduce:

```sh
python3 core/tools/substitution-proof/run.py \
    --baseline 66bce8378 --candidate HEAD
```

## 3. Results — fresh data

| arm | `core_pipeline` tests | identity example | canonical identity |
| --- | --- | --- | --- |
| `baseline` | **ok** | ok | reference |
| `c1-only` | **ok** | ok | **identical** |
| `c2-only` | **ok** | ok | **identical** |
| `combined` | **ok** | ok | **identical** |

`matrix.json` records `test_passed_in_every_arm: true` and
`canonical_identity.identical_in_every_arm: true` over the eight printed identity
lines (`base_directory`, `base_table`, `base_root`, the updated directory/table/root
and `prepared_objects 49`). The one timing field the example prints is normalized
out of the comparison, because a timing difference is not what this proof is about.

**So: C1 alone, C2 alone and the compatible pair all substitute with an unchanged
consumer, and produce byte-identical canonical identities.**

## 4. Results — persisted data (the classification the scenario demands)

`cross_revision` was run across arms over one Store file each:

| writer → reader | create | read | result |
| --- | --: | --: | --- |
| baseline → baseline | 0 | 0 | `read bytes 4096 digest eeeb77cc…` |
| candidate → candidate | 0 | 0 | `read bytes 4096 digest eeeb77cc…` — **the same digest** |
| baseline → candidate | 0 | **3** | `refused-on-open unsupported storage policy: schema identity` |
| candidate → baseline | 0 | **3** | `refused-on-open unsupported storage policy: schema identity` |

**Classification: the candidate C2 is a *format change*, not an algorithm-only
substitution.** `SCHEMA_VERSION` moved 4 → 6, `objects.base_object_id` and two
indexes were removed, `content_signatures` was added, and older Stores are refused
rather than migrated (`core/crates/layerfs-storage/sql/schema.sql`). A drop-in
claim is therefore **not** made for persisted data in either direction; the refusal
is fail-closed and typed, which is the behaviour the contract requires. C1 is
algorithm-only: its API additions are additive and its canonical output is
unchanged, as the identical digests above show.

## 5. Finding disposition (the second open item of #172)

The first audit's seven findings, adjudicated with the evidence now available:

| # | finding | disposition |
| --: | --- | --- |
| P1 | persisted-data substitution is not generally drop-in | **Confirmed and now demonstrated** (§4). Standing rule: every candidate revision is classified algorithm-only or format-changing before it is offered as a substitute; two-way reopen is required only where compatibility is promised. Carried to [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165) and [#180](https://github.com/Ephemeral-AI-Lab/layerfs/issues/180). |
| P2 | no production same-save C1 read/write bridge | **Decision required by [#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179)**, not a defect: it depends on the accumulator boundary. Carried. |
| P3 | public exposure is wider than the integration facade | **Confirmed by source.** Remedy accepted: designate facade versus advanced canonical/physical surface, and inspect external callers before any visibility change. Carried as a design item; no visibility change is made here. |
| P4 | C1-only replacement needs one Rust type identity throughout the graph | **Confirmed and resolved by this proof's assembly procedure** — each arm materializes one C1 path that both C2 and the caller resolve, and the matrix records it. No source edit was needed. |
| P5 | C2 has legitimate canonical-format coupling that must be explicit | **Confirmed.** The identical cross-arm digests are evidence the canonical contracts survive a C1 swap; the format-ABI inventory and codec fixtures remain owed before any grammar change. Carried. |
| P6 | provider failure detail and filesystem telemetry do not compose fully | **Decision required**: which error context and timing depth integration needs. Carried to #179. |
| P7 | reader capacity agreement is implicit rather than negotiated | **Confirmed.** Make the supported batch/resource profile an explicit integration precondition with a test; add negotiation only if a second real provider needs a different ceiling. Carried. |

## 6. Not claimed

- **No timing or resource claim.** The elapsed field is normalized out; the profile
  is `debug`; nothing here enters any measurement distribution.
- **No ABI claim.** This is source/dependency selection and rebuild, not dynamic
  loading or live replacement.
- **No migration.** The refusal is recorded as the correct behaviour; an upgrade
  path would be a separate design.
- **One consumer.** It covers construction → save → reopen → logical read, the edit
  path and the C1 primitives; it does not cover Workspace/FUSE, transport,
  tenancy or history.
- **The candidate tree's own copy of `support/mod.rs` is 88 lines longer** than the
  frozen baseline copy (helpers added for a new test). The frozen baseline copy
  compiles and passes against both revisions, which is the stronger statement; the
  addition is recorded rather than smoothed.
