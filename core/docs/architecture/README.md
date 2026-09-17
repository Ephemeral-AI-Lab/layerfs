# C1 vs C2: storage and content architecture of the replacement core

> **Status:** Research; source-backed description of the current tree. Informative,
> not a product contract. This set selects no architecture, freezes no scope
> and decides no open ruling.

Issue: [#175](https://github.com/Ephemeral-AI-Lab/layerfs/issues/175).
Release: [v0.1.7](../../../docs/roadmap/0.1/0.1.7/README.md). Design discussion:
[#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

- **Source pin:** every citation below was read at commit `1884e3eca`
  (`fix(storage): complete the audited FFI inventory in the codec module doc`).
  The tree was clean at that commit apart from in-flight Stage 5 evidence
  documents under `docs/`; no file under `core/crates/*/src/` was modified.
- **Scope:** the **replacement** product under `core/` only. The reference tree
  under root `crates/` is a separate, isolated product and is *not* described here.
  Do not read these papers' identifiers, formats or figures as describing the
  reference store, or vice versa.
- **Method:** source reads only — no builds, tests, benchmarks or code changes.
  Every load-bearing claim carries a file reference. Anything not established from
  source is recorded as `unknown` rather than guessed.
- **Measurement status:** this set contains **no performance numbers and no
  qualification claim**. Stage 6
  ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)) owns measured
  acceptance. The figures here are format sizes, declared bounds and code
  structure — the things a document can state from source alone.

These papers live with the product they describe. `core/AGENTS.md` requires that
a change under `crates/*/src/` or `crates/*/sql/` which alters a declared boundary,
a canonical or physical format, an algorithm or a named bound **updates this
document in the same commit**; advancing the source pin without a content change
is allowed only when the change touches none of those, and must be said rather
than done silently.

## Why these papers exist

The only architecture study in this release,
[`architecture-overview.md`](../../../docs/roadmap/0.1/0.1.7/architecture-overview.md), is pinned to the
reference tree: it cites `layerfs-layerstack-store`, `objects/admission.rs`,
`file/rope/edit.rs`, pack v3/v4 and the frozen schema-10 Store. None of those
exist in `core/`, whose packages are `layerfs-content`, `layerfs-storage` and
`layerfs-telemetry`. That study therefore does not describe the replacement, and
the replacement had no architecture document of its own.

The names are also easy to confuse, which is the second reason this set exists.
The component called **C2 is `layerfs-storage`** while C1 is
**`layerfs-content`** — so the pair reads as "storage vs content" but the storage
role sits on the content-named package's opposite number. Everything below keeps
the C1/C2 labels primary and names the package whenever the distinction matters.

### Actual size at the pin

Counted with the repository's own production counter, which excludes comments,
blank lines, tests, examples, docs and manifests, and which counts shipped runtime
SQL:

```sh
python3 tools/production_loc.py --detail
```

| Package | Component | Production lines | Files |
| --- | --- | ---: | ---: |
| `layerfs-content` | C1 | 11,917 | — |
| `layerfs-storage` | C2 | 6,112 | — |
| `layerfs-telemetry` | timer | 763 | — |
| **core total** | | **18,792** | **116** |

For comparison at the same command: the reference tree counts 65,417 production
lines, and the combined figure is 84,209.

Two cautions on these numbers, because both have already caused drift:

- Physical `wc -l` over `core/crates/*/src` gives 24,493 lines. That is a
  *physical file-line* total including comments and blanks, not a production
  count. **18,792 is the production figure; 24,493 is not a substitute for it.**
- The figures published elsewhere for this workspace are stale.
  [`core/README.md`](../../README.md) still states 10,983 LOC, which is
  the Stages 3–4 packet snapshot at `aa4b5a9e4`; the
  [closeout report](../../../docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-closeout-report.md) records 11,058 at that batch
  tip. Neither describes the tree at `1884e3eca`.

## Contents

Seven papers. Chapter numbers are global to the set, so a reference such as
"§6.8" resolves to one place no matter which paper it appears in.

| Paper | Chapters | Covers |
| --- | --- | --- |
| [`01-boundary.md`](01-boundary.md) | 1 | The C1/C2 split, the two boundary traits, dependency direction, timing seams |
| [`02-objects.md`](02-objects.md) | 2 | Identity, the `LFSO` envelope, the thirteen roles, `FinalizedObject` |
| [`03-files.md`](03-files.md) | 3–4 | Construction, CDC, the extent tree, localized edits, logical reads |
| [`04-filesystem.md`](04-filesystem.md) | 5 | Scoped root, inode identity, the two page formats, the B+tree engine, the reference reducer |
| [`05-storage.md`](05-storage.md) | 6 | Schema, publication watermark, pack framings, representation selection, pooling, reconstruction |
| [`06-limits.md`](06-limits.md) | 7, 9 | End-to-end flows, then every declared bound with the check that enforces it |
| [`07-importing.md`](07-importing.md) | 11 | **Proposal — draft, not finalized:** the four-phase plan for importing a directory tree, with the master pipeline diagram and what binds where. No importer exists; nothing in it has been executed. |

This paper adds the module map ([§8](#8-module-map)), what the set does not claim
([§10](#10-what-this-set-does-not-claim)) and the source index.

## Conventions

- Every paper records the source commit it was written against and is re-read from
  source, not from an earlier draft.
- Every load-bearing claim carries a file reference. Anything not established from
  source is recorded as `unknown` rather than guessed.
- Diagrams are **fenced text**, so they stay reviewable in a diff and legible in a
  plain Markdown reader. No image binaries and no rendering dependency.
- The set carries **no performance numbers and no qualification claim**. Stage 6
  ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)) owns measured
  acceptance.
- Most papers are **descriptive** — they state what the code does. A paper that
  contains guidance rather than description says so in its own status section and
  labels which parts are prescriptive. `07-importing.md` is the first such paper:
  no importer exists in this tree, and its phase plan is a proposal.

## Keeping this current

A change under `crates/*/src` or `crates/*/sql` that alters a declared boundary, a
canonical or physical format, an algorithm or a named bound **updates the affected
paper in the same commit**; this is required by
[`core/AGENTS.md`](../../AGENTS.md). Advancing the recorded source pin without a
content change is allowed only when the change touches none of those, and must be
said rather than done silently.

## Production-line figures

Size figures in this set come from the repository's own counter, which excludes
comments, blanks, tests, examples, docs and manifests, and which counts shipped
runtime SQL:

```sh
python3 tools/production_loc.py --detail
```

Do not substitute a physical `wc -l` total for a production figure; the two are
different measures and the difference is large enough to mislead.

## 8. Module map

```text
   core/crates/
   ├── layerfs-content/                        C1   11,917 production lines
   │   ├── src/lib.rs                          public surface, #![forbid(unsafe_code)]
   │   ├── src/policy.rs                       ConstructionPolicy, capacities, selector
   │   ├── src/object/
   │   │   ├── id.rs                           ObjectId, domain, digest width
   │   │   ├── codec.rs                        LFSO envelope, checked decode
   │   │   ├── access.rs                       AuthenticatedObjects trait
   │   │   ├── output.rs                       13 roles, FinalizedObject, consumer
   │   │   ├── predecessor.rs                  advisory predecessors + provenance
   │   │   └── inode_leaf.rs                   inode value grammar + pooled layout
   │   ├── src/file/
   │   │   ├── content.rs                      representation dispatch, whole-file form
   │   │   ├── read.rs / view.rs               logical reads, operation-local base view
   │   │   ├── cdc/gear.rs                     FROZEN GEAR profile + profile_id()
   │   │   ├── mapping/                        build (streaming) · codec · read · types
   │   │   └── edit/                           apply · tree (frontier) · compare
   │   │                                       · input · split · concat · finish
   │   └── src/filesystem/
   │       ├── root.rs / identity.rs           scoped root, scope+serial identity
   │       ├── input.rs / objects.rs / update.rs   operation boundary + orchestration
   │       ├── validate.rs / limits.rs         checked input, named bounds
   │       ├── sorted/                         format · page · merge · finish · budget
   │       ├── directory/ · inode/             codec · read · update per page format
   │       ├── attributes/                     build · codec · keys · patch · portable
   │       └── references/                     backing · runs · merge · reduce · release
   │
   ├── layerfs-storage/                        C2    6,112 production lines
   │   ├── src/lib.rs                          #![deny(unsafe_code)] + one exception
   │   ├── src/policy.rs                       StoragePolicy, capacities, all bounds
   │   ├── src/cas/                            store · owner · batch · save · finish
   │   │                                       · read · membership · dependencies
   │   │                                       · provider (the C1 bridge)
   │   ├── src/encoding/
   │   │   ├── codec.rs                        ⚠ the ONE audited unsafe module (zstd FFI)
   │   │   ├── full.rs / decode.rs             FULL/PREFIX records, reconstruction
   │   │   ├── delta/                          select · read · record · candidates
   │   │   └── pool/                           index · leaf · read · delta · value_group
   │   ├── src/pack/                           layout · assemble · placement
   │   ├── src/sqlite/                         schema · connection · lookup · write
   │   │                                       · pool · cleanup
   │   └── sql/schema.sql                      shipped runtime SQL (counted as production)
   │
   └── layerfs-telemetry/                      timer    763 production lines
       ├── src/lib.rs                          std-only, no product dependency
       └── src/timer/                          scope · recording · report · format · json
```

## 10. What this set does not claim

- **No performance numbers.** Not one. Stage 6
  ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)) owns measured
  acceptance; the figures here are format sizes, declared bounds and code
  structure.
- **No qualification.** The tree is feature-complete at its implemented scope and
  unqualified. Nothing above is a PASS.
- **No architecture selection.** This is a description of what the code does and
  which invariant each part owns. Open rulings stay open.
- **No reference comparison.** Where the reference tree has a similar mechanism,
  this set does not assert equivalence. Canonical compatibility is a claim
  that needs evidence, and Stage 5's acceptance record is where that evidence
  lives.
- **No adapter design.** [§1.1](01-boundary.md#11-the-one-sentence-separation) shows where the adapter boundary is; Stage 7
  ([#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172)) owns what the
  runtime integration actually becomes.

### Known open items at the pin

Two things a reader should carry forward rather than assume:

1. **The inode-leaf canonical-header compatibility gate.** `core/AGENTS.md` still
   directs that this gate be resolved before relying on that codec for
   filesystem-root equivalence. The source at the pin documents the encoded-row-width
   rule and pins it with a sealed fixture
   (`tests/fixtures/filesystem/codec-inode-leaf.bin`), and the module doc states
   that an earlier revision wrote the value width and produced different object
   identities. Whether the gate is formally **closed** is a status question this
   document does not decide — check the Stage 5 acceptance record.
2. **`core/README.md` is stale on size.** It states 10,983 LOC, which was the
   Stages 3–4 packet snapshot at `aa4b5a9e4`; the tree at this pin counts 18,792
   production lines. Correcting it is tracked by
   [#175](https://github.com/Ephemeral-AI-Lab/layerfs/issues/175).

### Claims recorded as `unknown`

Following this repository's convention, things not established from source are
named rather than guessed:

- **Relative cost of the two page formats** and the real distribution of page fill
  in a live tree — a measurement question, not a source-reading one.
- **Which production caller writes attribute-tree pages in practice.** The grammar,
  encoders and tests exist; the current core has no adapter, so the end-to-end
  writer of that lane is `unknown` at this pin.
- **Real-world chain depth distributions** under the delta policy — the bounds are
  declared and enforced, but the observed depths are a measurement.

## References

Source files cited above, by package:

**C1 `layerfs-content`**
`src/policy.rs` · `src/object/{id,codec,access,output,predecessor,inode_leaf}.rs` ·
`src/file/content.rs` · `src/file/{read,view}.rs` · `src/file/cdc/gear.rs` ·
`src/file/mapping/{types,build,codec,read}.rs` ·
`src/file/edit/{apply,tree,compare,input,split,concat,finish}.rs` ·
`src/filesystem/{root,identity,input,objects,update,validate,limits,path,symlink}.rs` ·
`src/filesystem/sorted/{format,page,merge,finish,budget}.rs` ·
`src/filesystem/references/{backing,runs,merge,reduce,release,record}.rs`

**C2 `layerfs-storage`**
`src/policy.rs` · `src/cas/{store,owner,batch,save,finish,read,membership,dependencies,provider}.rs` ·
`src/encoding/{full,decode,codec}.rs` · `src/encoding/delta/{select,read,record,candidates}.rs` ·
`src/encoding/pool/{index,leaf,read,delta,value_group}.rs` ·
`src/pack/{layout,assemble,placement}.rs` ·
`src/sqlite/{schema,connection,lookup,write,pool,cleanup}.rs` · `sql/schema.sql`

**Timer `layerfs-telemetry`**
`src/timer/{scope,recording,report,format,json}.rs`

**Repository rules and counters**
[`AGENTS.md`](../../../AGENTS.md) ·
[`core/AGENTS.md`](../../AGENTS.md) ·
[`core/README.md`](../../README.md) ·
[`tools/production_loc.py`](../../../tools/production_loc.py) ·
[`core/tools/check_product_boundary.py`](../../tools/check_product_boundary.py)
