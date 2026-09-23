# Importing a directory tree

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
>
> **DRAFT — NOT FINALIZED.** The phase plan and every diagram in this paper are a
> working draft. They were written from the public contracts and the tests that
> pin their ceilings; **nothing below has been executed**. The draft is recorded
> to be argued with, not to be built against. See
> [§11.1](#111-status-of-this-paper--read-this-first) for exactly which parts are
> settled and what would finalize the rest.
>
> **Current implementation note:** the Service's public
> `ImportNativeDirectory` scans an operator-owned source, constructs file objects
> with four workers and one bounded C2 handoff, then builds and publishes one
> complete genesis root. The phased import proposal below is not the product
> algorithm and has not been measured as such.

Part of the [replacement-core architecture](README.md) set. Source pin
`1884e3eca`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

Chapter numbers are global to the set, so this paper holds **chapter 11**; the
[contents table](README.md#contents) lists the other chapters.

---

## 11. Importing a directory tree

### 11.1 Status of this paper — read this first

The other papers in this set are **purely descriptive**: they state what the code
does. This one is different, and the difference matters:

| Part | Kind | Authority |
| --- | --- | --- |
| §11.2–§11.7, §11.9 | **Prescriptive** — how an importer *would* drive the API | Guidance. No importer exists in this tree. |
| Constants and cited behaviour | **Descriptive** — read from source at the pin | Same authority as the rest of the set. |
| §11.8 numbers | **Estimates** derived from declared formats | Not measured. Do not cite as results. |

**There is no import API and no adapter in `core/`.** Runtime integration is
Stage 7 ([#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172)); nothing
below has been executed end to end. Everything here is a plan built from the
public contracts that do exist, plus the tests that pin their ceilings.

#### What is settled and what is draft

Settled — these are read from source and will not change without a source change:

- every constant in [§11.7](#117-what-binds-where), and the phases' *existence*
  (the API calls they name are real and public);
- the ordering requirement (all lists strictly ascending) and the
  top-down-not-bottom-up constraint, both pinned by tests;
- the single-threaded fact of [§11.10](#1110-throughput-and-the-tradeoff-that-was-made-on-purpose).

Draft — expected to change once anything is run:

- the **batch sizes** in Phase D, and the claim that batching by the 4 MiB scratch
  budget is the right way to size them;
- the **object accounting** in [§11.8](#118-object-accounting-for-100000-files),
  which is derived from formats rather than counted;
- the **phase decomposition itself** — four phases is a reasonable cut, not a
  proven one. A working importer may fuse C into B, or split D by directory depth;
- every **diagram** in this paper, which draws an intended shape rather than an
  observed one.

#### What would finalize this paper

An external test under `core/crates/layerfs-content/tests/` importing a synthetic
100,000-file tree, run with attribute-root reuse **on and off** as the two arms,
reporting measured `peak_scratch_bytes`, drain-wave and transaction counts, wall
time, and the entry count at which Phase D first refuses. That would settle
§11.8 and the first three `unknown`s in
[§11.11](#1111-what-is-not-verified), and would let [§11.7](#117-what-binds-where)
state the real batch ceiling instead of the declared one.

Until then this paper stays a draft, and any number taken from it is an estimate.

### 11.2 The master pipeline

> **Draft diagram.** Intended shape, not observed behaviour. See
> [§11.1](#111-status-of-this-paper--read-this-first).

Four phases, one shared sink. Phase D depends on all three predecessors; A, B and
C are mutually independent.

```text
        ┌──────────────────── SAVE SINK (C2) ────────────────────┐
        │                                                         │
        │   save.accept(object)                                   │
        │        └─► PendingBatch     drains at 512 objects        │
        │                             OR 512 KiB canonical         │
        │        └─► transaction      seals at 8,191 rows          │
        │                             OR 4 MiB−1 canonical bytes   │
        │        └─► packs + objects table                        │
        │                                                         │
        │   save.finish()  ──► advances retained_pack_ceiling      │
        │                      returns SaveOutcome                 │
        └──────▲───────────────▲──────────────────▲───────────────┘
               │               │                  │
      PHASE A  │      PHASE B  │         PHASE C  │      PHASE D
      Skeleton │      Content │         Metadata │      Populate
      ─────────┘      ────────┘         ─────────┘      ─────────
      build the       per file          per inode       attach file
      empty dir       content           attribute       bindings to
      tree            objects           root            directories
          │               │                  │              ▲
          │               │                  │              │
          └───────────────┴──────────────────┴──────────────┘
                          all three feed D
```

Why this order and not the obvious one:

```text
   ✗ BOTTOM-UP                        ✓ TOP-DOWN (this pipeline)
   ───────────                        ─────────────────────────
   fill a directory, then attach it   attach the directory EMPTY,
   ⇒ the attach walks its subtree       then fill it
   ⇒ > 4,096 entries is REFUSED       ⇒ attach costs ~0
                                        ⇒ filling is walk-free
```

> **The four phase diagrams below are draft sketches** of an intended shape. The
> API calls and constants they name are real; the sequence and the batch sizes are
> not yet validated by a run.

### 11.3 Phase A — Skeleton

Build the directory structure with no files in it.

```text
   directories only, every one EMPTY

   build_filesystem(FilesystemInput {
       base:        None,
       scope:       InodeScope,
       root_serial: <the root's serial>,
       directories: &[DirectoryUpdate { parent, changes }],
       inodes:      &[InodeUpdate { serial, value: dir_value }],
       new_inodes:  &[...],
       resources:   FilesystemResources::default(),
   })
        │
        ├── check_build_reachability: ONE walk from the root,
        │     ONE counter, charging EVERY stated binding
        │     ⇒ no independent count cap for a base-less build
        │
        └── FilesystemRootId ──► save.accept(root object)

   A native-directory import may build the complete skeleton in one call.
```

Every list is **strictly ascending** and unique: `directories` by parent serial,
`inodes` by serial, `new_inodes` by value. `FilesystemInput::check` refuses
anything else with `NonCanonicalOrdering` before touching an object.

For a build, the root serial must appear in `new_inodes` — "a build allocates its
root like any other inode".

### 11.4 Phase B — Content

One call per file, into the same save sink.

```text
   for each file:
        construct_stream(policy, &capacities, file, &mut save, scope)
             │
             ├── content.probe   read at most `cutoff` bytes (128 KiB default)
             │
             ├── < cutoff  ──► 1 WholeFile object      (LFS5SML value)
             │
             └── ≥ cutoff  ──► FastCdc scan
                                 ├── Chunk objects
                                 ├── ExtentLeaf / ExtentBranch pages
                                 └── 1 FileState root
             │
             └── ConstructedFile { root, logical_len }
                        │
                        └─► becomes InodeValue.content_root in Phase D
```

No filesystem walk happens in this phase at all — `save.accept` is pure storage.
This is the phase that dominates wall time: one `open`/`read`/BLAKE3/encode cycle
per file, on one thread.

### 11.5 Phase C — Metadata

`build_filesystem` and `update_filesystem` do **not** build attribute trees. Mode
and mtime enter only through `InodeValue.metadata_root`, which the caller
constructs first.

```text
   per inode, before Phase D:

      emit_value(&mut objects, mode_bytes)   ──► value object ┐
      emit_value(&mut objects, mtime_bytes)  ──► value object ├─ ~3 objects
      build_attribute_tree(2 entries)        ──► 1 leaf       ┘

      InodeValue {
          kind,                  RegularFile | Directory | Symlink
          namespace_ref_count,   ← DERIVED for new inodes; do not trust your own
          content_root,          ← Phase B
          metadata_root,         ← this phase
      }
```

`PortableMetadata` is deliberately narrow: `0o777` for files and symlinks (a
symlink's mode is exactly `0o777`), `0o1777` for directories, nanoseconds below
one billion. **There is no uid, gid or atime** — nowhere to put them.

**Root reuse is the single biggest lever in the whole import.** An attribute root
is just an `ObjectId`; nothing requires building a fresh one per inode. Group
inodes by their `(mode, mtime)` pair, build one tree per distinct pair, and reuse
the identity:

```text
   NAIVE                                  EFFICIENT
   ─────                                  ─────────
   build per file                         build per DISTINCT pair
   ⇒ 3 × 100,000 = 300,000 objects        ⇒ 3 × N_distinct objects
```

An attribute tree with no entries is legal (an empty leaf page), so an inode with
no attributes still needs one `metadata_root` — build it once and share it.

### 11.6 Phase D — Populate

Attach file bindings to their parent directories, in bounded batches.

```text
   per parent directory, batched:

      update_filesystem(FilesystemInput {
          base:        Some(previous_root),
          directories: &[DirectoryUpdate { parent, changes }],
          inodes:      &[InodeUpdate { serial, value }],
          new_inodes:  &[...],
          resources:   FilesystemResources::default(),
      })
           │
           ├── validation
           │     └── effective-cycle check
           │           children are FILES ⇒ walk SKIPPED entirely
           │           (the walk fires only for a binding whose child
           │            is a DIRECTORY)
           │
           ├── sorted engine (LFS6NSP)
           │     merge changes into the directory B+tree, COW
           │     untouched subtrees referenced by identity, never read
           │     bounded by resources.scratch_bytes (4 MiB default)
           │
           ├── reference reducer
           │     derive namespace_ref_count; spill to runs past 4,096 pending
           │
           ├── inode table (LFS6INT)
           │     change-driven: only changed rows are rewritten
           │
           └── new FilesystemRootId ──► save.accept(root)
```

**Batch by the declared scratch budget, not by a file count.** `entries_examined`,
`peak_scratch_bytes` and `sorted.pages_created` are the observables that tell you
how close a batch came; §11.11 records what is not yet measured.

### 11.7 What binds where

| Phase | Limit | Value | Bites? |
| --- | --- | ---: | --- |
| A | supplied binding vector | no independent entry-count cap | work scales with input |
| A | subtree of any bound directory | 4,096 entries | no — directories are empty |
| B, C | `BATCH_OBJECT_LIMIT` | 512 objects | yes — drains waves |
| B, C | `BATCH_CANONICAL_BYTES_LIMIT` | 512 KiB | **usually binds first** |
| B, C | `TRANSACTION_ROW_LIMIT` | 8,191 rows | yes |
| B, C | `TRANSACTION_CANONICAL_BYTES_LIMIT` | 4 MiB−1 | yes |
| D | `resources.scratch_bytes` | 4 MiB | **size batches against this** |
| D | `DEFAULT_MAXIMUM_PENDING` | 4,096 rows | no — spills to runs |
| D | `ordering_bytes` | 64 MiB ⇒ 8.4M serials | no |
| D | `MAXIMUM_READ_DEMANDS` | 4,096 ids/wave | no |

Note the asymmetry: the **write** path drains at 512 objects / 512 KiB, while the
**read** wave is 4,096 ids. They are different bounds; do not conflate them.

### 11.8 Object accounting for 100,000 files

> **Estimates**, derived from the declared formats. Not measured. The only
> measured figures in this set are the format sizes in [§9](06-limits.md).

Assumptions: every file below the 128 KiB cutoff, names ~20 bytes, mode and mtime
preserved.

```text
   ┌──────────────────────────────┬───────────────┬───────────────┐
   │ Component                    │ per-file mtime│ mode only     │
   ├──────────────────────────────┼───────────────┼───────────────┤
   │ Content (1 per file)         │     100,000   │     100,000   │
   │ Attribute values (2/file)    │     200,000   │           6   │
   │ Attribute leaves (1/file)    │     100,000   │           3   │
   │ Directory pages (~740/leaf)  │        ~140   │        ~140   │
   │ Inode pages (~100/leaf)      │      ~1,010   │      ~1,010   │
   │ Root                         │           1   │           1   │
   ├──────────────────────────────┼───────────────┼───────────────┤
   │ TOTAL                        │    ~401,000   │    ~101,000   │
   └──────────────────────────────┴───────────────┴───────────────┘
```

The left column is roughly **four objects per file**; the right is **one**. The
difference is entirely how many distinct `(mode, mtime)` pairs exist — i.e. how
much timestamp fidelity the import chooses to keep. It is a policy decision, not a
technical limit.

At ~101,000 objects the ratio is about one object per file plus tree pages, which
is the natural floor for a content-addressed store.

### 11.9 The anti-pattern

```text
   ✗ BOTTOM-UP

   fill sub/ with 10,000 files      ──►  accepted
   then bind sub/ under parent      ──►  walks sub/'s 10,000 entries
                                         10,000 > 4,096
                                         ──► REFUSED
                                         InvalidRecord("cycle check work limit")

   and permanently thereafter:
   rename or move sub/              ──►  REFUSED, forever
```

Two things make this trap expensive to discover late:

- The refusal is **indistinguishable from a genuine cycle** — the same error, by
  design, because "exceeding a work bound is an explicit refusal, never a claim
  that the tree was proven acyclic".
- It is **permanent**. A directory whose effective subtree reaches 4,096 entries
  can never be renamed or relocated again, however small the change.

Pinned by
`crates/layerfs-content/tests/filesystem_bounds.rs::a_directory_whose_subtree_exceeds_the_entry_ceiling_cannot_be_rebound`.

### 11.10 Throughput, and the tradeoff that was made on purpose

Everything above runs on **one thread**. There is no worker pool: no
`std::thread::spawn`, no rayon, no async runtime, and dependencies of `blake3`,
`rusqlite` and `zstd-sys` only. `TimingScope` is `!Send` and `!Sync`, enforced by a
compile-fail fixture, so an operation cannot be moved across threads;
`Store::begin_save` takes exclusive write ownership once.

This is a deliberate measurement-integrity choice, and
[`AGENTS.md`](../../AGENTS.md) states its cost openly:

> **A performance drop against v0.1.5 is expected** for the single-worker cases
> and is absorbed by the bounded acceptance rule, **never by adding workers back.**

So the honest reading of "is it efficient":

| Dimension | Verdict |
| --- | --- |
| Storage per file | **Yes** — ~1 object/file, deduped, 23-byte whole-file overhead |
| Incremental change | **Yes** — COW; untouched subtrees never re-read |
| Format density | **Yes** — 81-byte inode rows, 12-byte pooled rows, 44-byte empty page |
| Metadata cost | **Only with root reuse** — naive wiring is 4× |
| Raw throughput | **No** — single-threaded by design, drop expected and accepted |

Any parallelism must live in the caller: construct content on N threads into
buffers, then feed the single save operation from one thread.

### 11.11 What is not verified

Recorded as `unknown` rather than guessed, per this set's conventions:

- **The safe Phase D batch size.** The largest single-call count in the test suite
  is 4,088 entries, which fits the default 4 MiB scratch. Nothing pins where the
  refusal begins for a directory update.
- **Reducer run-spilling behaviour across many transactions** at this scale.
- **Wall-clock cost** of ~400,000 emissions, and whether the naive metadata path
  is bandwidth- or CPU-bound.
- **Whether any of this works end to end.** No importer exists; the phases above
  are assembled from public contracts and the tests that pin their ceilings.
- **The object-accounting table** in §11.8 — derived from formats, not counted
  from a run.

An external test under `core/crates/layerfs-content/tests/` importing a synthetic
100,000-file tree would replace §11.8 and the first three items with measured
numbers, with root reuse on and off as the two arms.
