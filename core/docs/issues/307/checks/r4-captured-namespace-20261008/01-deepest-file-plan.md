# R4 deepest-file plan: captured namespace construction and incremental topology

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Written by the lead before any R4 product edit, from six read-only audits and
the lead's own reading of the Content update, validation and state source.
Assignment: [R4 handoff](../../HANDOFF-R4-CAPTURED-NAMESPACE-20261008.md).
Source identity: commit `8885bb765` (product source identical to `37dcf5405`;
only three test files changed since). Nothing below is implemented yet.

## 1. What the audits established

Each line was checked by the lead against the cited source.

- **The validator is route-independent.** `validate::check_operation` runs for
  the resident, streamed and streamed-backed routes alike
  (`filesystem/update.rs:319`). Backed mode changes only `SerialState`,
  `RebuiltRoots`, `InitialRows` and the reducer. Every container in
  `validate.rs` and `validate/cycles.rs` is live on the backed route.
- **No production code calls the filesystem driver yet.** A search of every
  active crate's `src/` finds no caller of `build_filesystem*`,
  `update_filesystem*` or `qualify_root` outside Content. Project Init uses the
  directory and table builders directly. The R4 producer is the first
  production caller, so changing the validator cannot regress Init.
- **The canonical format has no reverse binding.** An inode value is kind,
  link count, content root and metadata root
  (`object/inode_leaf.rs:84-93`); a directory page maps name to serial. A
  non-file, non-root inode always has count 1 (`inode_leaf.rs:101-113`).
  The Overlay has no parent column either (`sql/schema.sql:20-47`);
  `native_parent` is live per-connection custody, not captured state.
- **The alias pass walks the whole base** whenever a change binds an existing
  directory or symlink (`validate.rs:447-535`), and refuses above
  `ordering_bytes / 1024` visited entries. **The cycle pass walks the whole
  effective subtree of every bound directory**, fresh and restated ones
  included, with a fresh `seen` set each time (`validate/cycles.rs:43-104`): a
  chain of K new nested directories costs about K²/2 visits.
- **Total-size refusals are reachable on the backed route**: the declared row,
  name and demand limits (`validate.rs:212`, `:242`, `:260`, `:277`) and the
  walk limit. `prefetch` issues one grouped demand for the whole change.
- **The captured reader serves less than the contract needs.** Pages read the
  sealed generation exactly; one final row per key; whiteouts included. The
  captured inode row carries no content or metadata root. Missing: a
  parent-local name cursor bound to the reader, a name point, any header
  (distinct changed parents with exact counts), the three declared totals, the
  fresh rank, and a way to read a captured symlink target
  (`payload/captured_runs.rs:28-31` refuses every non-file).
- **Indexed operation records have 32-byte keys.** They hold u64-keyed sets,
  not names. The captured `directory_entry` rows are the only sealed,
  name-ordered, point-addressable store of full names.
- **`CapturedFileEdits`** is one instance per file with its own `file_scope`,
  shares the reader and operation owner, refuses a link-count-0 file with
  `OverlayError::Missing`, returns the base root with no output when content is
  unchanged, and never releases anything.
- **The Commit closure returns `Result<FilesystemRootId, CommitError>`.** No
  `CommitError` variant can hold producer custody with a definite
  classification, and `layerfs-workspace` cannot name `CommitError`. The driver
  acquires no captured reader and no operation owner. `Save` has no policy
  accessor; the test closure captures `Arc<Store>`.
- **Captured row shapes** for every R3 state are in the
  [state audit summary](#6-captured-states-and-what-the-producer-emits).

## 2. Decisions the lead made

These are the lead's decisions. Those marked **owner review** change a contract
point and are repeated in section 8.

**D-1. The base is valid by induction (owner review).** An update trusts that
its base root was produced by this algorithm or admitted by `qualify_root`, so
a stored non-file inode with count 1 has exactly one base binding. The module
already states this premise (`validate.rs:4-5`); the existing alias walk does
not prove more, because it accepts on the first base binding it finds.

**D-2. Alias validation becomes the derived-count invariant.** The reducer
already derives every touched inode's final count from the base count and the
before/after pairs the directory merge observes. `references/meaning.rs`
`finish`, shared by the resident and indexed reducers, refuses a non-file final
value whose count exceeds 1 with `InvalidRecord("multiple parents")`. This is
the leaf grammar itself, enforced where the value is produced. The whole-base
alias walk and its containers are deleted. Consequence: the refusal now arrives
after directory pages were offered to the consumer rather than before. Nothing
is published either way; the unfinished Save holds unreferenced objects.

**D-3. Cycle validation becomes placement evidence plus territory.** Details in
section 4. No walk at all unless a stored directory changes parent into a
stored, non-root directory; then the walk covers only the moved directories'
own subtrees, once each. A chain of new directories costs linear work.

**D-4. Change-proportional cycle proof for a moved directory is not achievable
in R4 (owner review).** Proving that a new parent is outside a moved
directory's subtree needs the new parent's ancestor chain. Neither the
canonical format nor the Capture records it, and the live `native_parent`
index is not captured state and can be forgotten by the kernel. Supplying it
needs either ancestry rows written at mutation time (schema and R3 mutation
path) or a canonical reverse-binding index (format change). R4 implements the
best the existing formats allow and reports the residual cost.

**D-5. No artificial totals on the backed route.** The `ordering_bytes`-derived
row, name, demand and walk refusals stop applying when serial state is backed.
The resident route keeps its declared limits unchanged.

**D-6. `CheckedInput.additions` is removed (owner review).** The public field
has no reader in any crate or test. Keeping it would keep a change-sized
resident map on every route.

**D-7. The producer seals what the reader cannot answer.** One normalization
pass writes headers (parent → exact change count), typed inode values and the
dense fresh rank into the operation's indexed records. Names stay in the sealed
captured rows and are read through new reader-bound parent-local jobs.

**D-8. Tombstones are filtered by the producer.** A non-root captured inode
with link count 0 is never declared fresh, never given a value and never
constructed. A header whose parent is a fresh tombstone is dropped. A header
whose parent is a removed base directory is kept: its whiteouts are the
evidence that moved children left it.

**D-9. Custody stays with the producer's caller.** The producer returns an
attempt holding the result and all original custody, as `CapturedFileEdits`
does. The Commit closure deposits that custody in its borrowed environment and
maps the result to a `CommitError`. How `CommitFailure` should carry producer
custody, and whether a producer failure is definite or uncertain, is R5 and
owner review; the driver is not changed.

**D-10. The driver glue is test code in R4.** Acquiring the captured reader and
operation owner, calling the producer and releasing afterwards is written in
the daemon integration test. Product wiring of that glue is R5.

## 3. Layout

Scope: commit `8885bb765`, `tools/production_loc.py --files` (SHA-256
`c0fe7f36…624adb`), `prod` column. Only files R4 touches or reuses directly are
shown; bracket sums cover the files shown. Planned names are candidates.

```
layerfs-content/src/filesystem/     [2559]  (part)
  mod.rs                               45   (CheckedInput export adjusted)
  update.rs                           684   (passes the serial state to validation)
  validate.rs                         634   (windowed classification; alias pass removed)
  references/                        [107]  (part)
    meaning.rs                        107   (non-file count invariant)
  rows/                              [351]  (part)
    view.rs                           351   (binding-point cache removed)
  state/                             [414]  (part)
    codec.rs                           76   (topology record kinds)
    mod.rs                             10
    store.rs                          328   (reused: read, apply, key windows)
  validate/                          [252]
    cycles.rs                         141   (rewritten: placement evidence)
    entries.rs                        111   (reused by the territory walk)
    backed.rs                           —   new
    incremental.rs                      —   new
```

`validate/backed.rs` owns the topology records: placements, territory marks,
rooted marks and the territory queue, in both modes (backed through
`SerialState`, resident maps under the resident limits). It cannot live in
`state/store.rs` (328 lines, serial construction state) without mixing
validation into construction state. `validate/incremental.rs` owns the three
topology passes of section 4. `validate.rs` keeps its entry and the binding
classification.

```
layerfs-overlay/src/                 [1015]  (part)
  lib.rs                               57   (exports)
  contract/                          [202]  (part)
    types.rs                          202   (unchanged unless a reply type is needed)
  database/                          [106]  (part)
    statements.rs                     106   (three reader-bound statements)
  lifetime/                          [190]  (part)
    captured_reader.rs                190   (unchanged)
  namespace/                         [460]  (part)
    mod.rs                              3
    compound.rs                       347   (unchanged)
    directory_entry.rs                129   (unchanged)
    captured_namespace.rs               —   new
```

`namespace/captured_namespace.rs` owns the reader-bound parent-local cursor,
the name point and the symlink target point. `captured_reader.rs` keeps reader
lifetime; adding three more query bodies there would mix lifetime and
namespace reads.

```
layerfs-daemon/src/overlay/          [956]  (part)
  mod.rs                               13
  captured_namespace_port.rs           45   (three more port methods)
  commands.rs                         749   (three read jobs)
  indexed_operation_record.rs         109   (unchanged)
```

`store/commit.rs` and every other `store/` file are unchanged in R4. The
handoff listed `store/ports.rs (extend)`; the audits found no need.

```
layerfs-workspace/src/               [1702]  (part)
  lib.rs                               55   (exports)
  ports/                             [390]
    captured_namespace.rs              36   (three more methods)
    captured_runs.rs                   19
    files.rs                           37
    lengths.rs                          4
    mod.rs                             15
    operation_record.rs               197
    overlay.rs                         82
  construction/                     [1263]
    mod.rs                              4
    records.rs                        247   (reused)
    driver.rs                           —   new
    outcome.rs                          —   new
    captured/                       [1012]  (reused as is)
      context.rs                       84
      mod.rs                            7
      normalize.rs                    116
      owner.rs                        256
      scan.rs                         246
      source.rs                       185
      state.rs                        118
    namespace/                          —   new
      mod.rs                            —
      records.rs                        —
      normalize.rs                      —
      inodes.rs                         —
      cursor.rs                         —
```

- `driver.rs`: the one public producer entry and its phase order.
- `outcome.rs`: attempt, custody and work types.
- `namespace/records.rs`: the producer's sealed record kinds and codecs.
- `namespace/normalize.rs`: the two passes that seal headers, ranks and totals.
- `namespace/inodes.rs`: one captured inode row to one typed value: file root
  through `CapturedFileEdits`, symlink target, metadata patch.
- `namespace/cursor.rs`: the `StreamedRowSource` implementation.

The handoff's `context.rs`, `directories.rs`, `assemble.rs` and `scratch/` are
not planned: their responsibilities fit the files above. Add one only if a file
reaches the line limit.

Tests: `layerfs-content/tests/filesystem_*.rs`, `layerfs-overlay/tests/`,
`layerfs-workspace/tests/`, `layerfs-daemon/tests/`.

## 4. Content design (track C)

### 4.1 State

| Today | R4 |
| --- | --- |
| `additions`, `by_parent`, `retained`, `candidates`, `bound`, alias `pending`/`seen`, `restated` | Deleted with the alias walk (D-2) |
| `demanded`, `missing`, one grouped prefetch | Classification in windows of at most 64 change rows, one grouped lookup per window |
| `records`/`absent` memo sized by `ordering_bytes / 1024` | Fixed window on the backed route |
| Cycle `pending`/`seen` per bound directory | Topology records, one bounded frame |
| Build `declared`, `edges`, `seen`, `pending` | The same placement records and rooted proof |
| `ResidentInput.points` | Deleted (only the alias walk used it) |

Topology record kinds, in the filesystem scope beside the existing kinds,
`0x4653_0020..=0x4653_002F` reserved:

| Kind | Key | Value | Meaning |
| --- | --- | --- | --- |
| PLACED | directory serial | parent serial, stored flag | This operation places the directory under that parent |
| TERRITORY | directory serial | moved directory serial | This header directory lies in that moved directory's surviving subtree |
| ROOTED | directory serial | marker | Proven to reach the root |
| QUEUE | ordinal | serial, base listing root | FIFO of one territory walk |

### 4.2 Passes

1. **Classification** (streaming, windowed). For each bound child that is a
   directory: when it has a base record and its parent has one, a base point
   lookup of that name decides *restated* (skip). Otherwise it is *placed*:
   write PLACED guarded on absence; a second placement is
   `"multiple parents"`. Count placed stored directories whose parent is
   stored, not placed and not the root (`moved`).
2. **Territory** (only when `moved > 0`). For each placed stored directory, a
   queue walk of its effective subtree through stored, unplaced directories,
   using the existing `EffectiveEntries` merge and grouped inode lookups per
   page. A visited directory that has a header gets TERRITORY. Placed and fresh
   directories are not entered: they carry their own placement.
3. **Rooted proof.** For each PLACED key, walk upward: the root or a ROOTED
   directory ends it; a placed directory continues at its PLACED parent; a
   stored unplaced directory continues at its TERRITORY owner if it has one and
   otherwise ends it (base position, base ancestry intact); a fresh directory
   nothing binds ends it (dropped subtree). A walk longer than twice the
   placement count plus two is `"effective tree cycle"`. A second pass marks
   the path ROOTED, so every directory is walked a bounded number of times.

Soundness, given every directory has at most one final binding: a cycle
contains no root-reachable node and at least one placed edge, and step 3 proves
every placed directory's parent reaches the root. Until `finish` runs, a stored
directory can still have two bindings; steps 2 and 3 terminate regardless
(step 2 follows base edges only, step 3 is step-bounded) and the operation
then fails in `finish`.

Cost, in the optimization guide's variables: classification O(K) rows with one
base name lookup per bound stored directory; rooted proof O(placed
directories); territory Σ over moved stored directories of their surviving
subtree entries, zero when no stored directory moves under a stored non-root
parent. No term in N.

### 4.3 What track C must preserve

Public entries and traits are unchanged: `update_filesystem_streamed_backed`,
`build_filesystem_streamed_backed`, `StreamedRowSource`,
`PreparedDirectoryStreams`, `StreamedFilesystemInput`,
`IndexedConstructionBacking`, `OrderingBacking`, `FilesystemObjects`,
`FilesystemResources`. Refusal labels `"multiple parents"` and
`"effective tree cycle"` keep their text. `"cycle check work limit"` is
retired. `ValidationWork` keeps its fields and gains counted ones
(placements, ancestry steps, territory directories and entries).
`inode_pages_by_site.aliases` stays and reads 0, because the benchmark harness
names it.

## 5. Captured reader additions (track P)

Three reader-bound read jobs, each `ServiceClass::Read`, each checked by
`check_captured_reader`, each over the sealed generation only:

| Port method | Statement shape | Bound |
| --- | --- | --- |
| `captured_directory_entries(reader, parent, after)` | `directory_entry_capture`, `ns=? AND gen=? AND parent=? AND name>?`, name order | 64 rows, one parent |
| `captured_directory_entry(reader, parent, name)` | primary key, `ns=? AND parent=? AND name=? AND gen=?` | 1 row |
| `captured_symlink(reader, serial)` | the reader's sealed inode and payload layers | one target, at most 4,096 bytes |

Each needs its `EXPLAIN QUERY PLAN` and VM-step evidence at growing active
populations, as the existing capture statements have. The Owner adapter keeps
its rule: synchronous, constructor thread only, original command or completion
retained on failure.

## 6. Captured states and what the producer emits

| Captured rows | Producer output |
| --- | --- |
| Non-root inode, `nlink = 0` (unlinked while held, removed directory, fresh and removed) | Nothing for the inode. No content construction |
| Root inode (`nlink = 0`, alive) | A metadata value; never fresh |
| Live inode, `born` above the floor | Fresh serial, typed value. File: `construct_runs`. Symlink: captured target. Directory: header, zero changes if no names |
| Live base inode | Typed value: base roots, metadata patched with mode and mtime only. File: `CapturedFileEdits` (base root when content is unchanged) |
| Entry `Some(serial)` | Change `Bound(serial)`, restated or not |
| Entry `None` | Change `Removed`, including under a removed base parent |
| Entries under a fresh tombstone parent | Header dropped |
| Moved directory | Two name changes; no row and no value for it or its descendants |

Record scope: the namespace's own records and Content's filesystem records use
`file_scope = 0`; each file's `CapturedFileEdits` uses `file_scope = serial`.
Producer kinds are `0x434E_0000..=0x434E_000F`, outside Content's and the file
editor's.

## 7. Frozen interfaces between tracks

### 7.1 Content, as the producer uses it (C → W)

```rust
layerfs_content::filesystem::update_filesystem_streamed_backed(
    objects: &mut FilesystemObjects<'_>,
    input: &impl PreparedDirectoryStreams,          // StreamedFilesystemInput over the cursor
    records: &mut dyn IndexedConstructionBacking,   // scope (operation, file_scope 0)
    ordering: Option<&mut dyn OrderingBacking>,     // None
) -> ContentResult<FilesystemResult>
FilesystemObjects::new_with_accepted(reader, consumer, accepted)
```

Behaviour W relies on, each to be covered by a Content test in track C:

1. No header, inode or fresh row returns the base root identity unchanged.
2. A `Removed` change for a name the base does not bind is a no-op.
3. A change restating the base binding is a no-op and causes no base walk.
4. A fresh directory with a zero-change header gets an empty directory page;
   its supplied content root is ignored.
5. A header under a base directory this operation removes is accepted, and the
   directory is released.
6. A value for a base directory with no header keeps the supplied content
   root; with a header it takes the rebuilt root.
7. The supplied link count is ignored; counts are derived.
8. On the backed route no refusal depends on `ordering_bytes`.

### 7.2 Ports, as the producer uses them (P → W)

```rust
pub trait OverlayCapturedNamespace: OverlayCapturedRuns {
    fn captured_inode_page(&self, reader: CapturedReader, after: u64)
        -> WorkspaceResult<Vec<Inode>>;                                  // existing
    fn captured_directory_entry_page(&self, reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>) -> WorkspaceResult<Vec<DirectoryEntry>>; // existing
    /// At most PAGE_ROWS rows of exactly this parent, strictly after `after`
    /// in binary name order, whiteouts included. Empty ends the sequence.
    fn captured_directory_entries(&self, reader: CapturedReader, parent: u64,
        after: Option<Vec<u8>>) -> WorkspaceResult<Vec<DirectoryEntry>>;
    /// The exact sealed row for this name, or None when the capture has none.
    fn captured_directory_entry(&self, reader: CapturedReader, parent: u64,
        name: &[u8]) -> WorkspaceResult<Option<DirectoryEntry>>;
    /// The exact captured target of a live symlink in the reader's sealed range.
    fn captured_symlink(&self, reader: CapturedReader, serial: u64)
        -> WorkspaceResult<Vec<u8>>;
}
```

`OverlayCapturedRuns::captured_inode` and `captured_run_step` and
`OverlayOperationRecords` are unchanged. Implementations: `Overlay` directly
and the daemon `OwnerClient`.

### 7.3 The producer entry (W → I)

```rust
pub struct CapturedNamespace<'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>;
impl<'a, P> CapturedNamespace<'a, P> {
    /// Binds exact inputs. Performs no I/O.
    pub fn new(workspace: &Workspace, provider: &'a P, reader: CapturedReader,
               operation: OperationOwner) -> Self;
    /// One attempt: normalize, construct changed files and metadata, then the
    /// canonical update. Releases nothing.
    pub fn construct(self, policy: ConstructionPolicy, capacities: &ConstructionCapacities,
                     objects: &dyn AuthenticatedObjects, consumer: &mut dyn FinalizedConsumer,
                     scope: TimingScope<'_>) -> CapturedNamespaceAttempt;
}
pub struct CapturedNamespaceAttempt {
    pub result: ContentResult<FilesystemResult>,
    pub custody: CapturedNamespaceCustody,
}
pub struct CapturedNamespaceCustody {
    pub reader: CapturedReader,
    pub operation: OperationOwner,
    pub records: ConstructionBackingCustody,        // namespace scope
    pub file: Option<Box<CapturedFileCustody>>,     // the failing file, if any
    pub failure: Option<WorkspaceError>,            // first original namespace/reader failure
    pub work: CapturedNamespaceWork,
}
```

`objects` serves base reads and same-Save reads; in a Commit it is the `Save`.
The first original failure wins and stays in exactly one custody slot; the
`result` error is its Content label. The caller drops custody, then releases
the reader, then the operation owner.

An interface change stops the track and comes back to the lead.

## 8. Open questions for the owner

1. **Ancestry evidence (D-4).** Accept subtree-proportional cycle validation
   for a directory moved under a stored non-root parent, or authorize one of:
   ancestry rows recorded at mutation time, or a canonical reverse-binding
   index.
2. **Inductive base validity (D-1)** as the basis of alias validation.
3. **Refusal timing (D-2).** `"multiple parents"` arrives after directory
   pages were offered to the consumer.
4. **`CheckedInput.additions` removed (D-6).**
5. **Producer custody in `CommitFailure` (D-9)** and the definite or uncertain
   classification of each producer failure. R5.
6. **Admission during construction.** The synchronous ports make one attempt;
   live activity can fill the namespace's job lane and end the Commit with an
   unattempted read. A readiness wait before the attempt, or accept.
7. **Page and point predicates differ** (`gen = captured` against
   `(installed, captured]`). The audits argue they agree; track P proves it by
   test, including after a failed capture is folded forward.
8. **Rows written by `Overlay::publish`** can carry shapes Workspace never
   writes (a whiteout over nothing). The producer passes them through as
   no-op removals.
9. **Sibling reads in the sorted merge.** A one-row change reads every child
   page of each touched branch, so counted reads grow with fan-out and tree
   height. This is cluster-one behaviour R4 does not change; R4-6 reports it.

## 9. Proof ownership

| Row | Where |
| --- | --- |
| R4-1, R4-3, R4-4 | Workspace producer tests over a direct Overlay, independent model oracle; repeated through the daemon in track I |
| R4-2 | Content tests (track C) and one producer case |
| R4-5 | Overlay and daemon reader tests (track P); producer cursor test (track W) |
| R4-6, R4-7 | Counted Content work (track C) and counted producer, SQL and release work at three base sizes (track I) |
| R4-8, R4-10 | Daemon integration over a real Disposable Store (track I) |
| R4-9 | Producer tests with a failing provider and consumer (track W); daemon custody (track I) |

## Amendment 1 (2026-10-08): corrections found by track C before any product edit

Track C traced section 4 against the existing topology tests before editing
and stopped on a defect in the plan. No product source had changed. The
counter-example is retained as a test that passes on the validator this plan
replaces (`filesystem_topology::a_cycle_through_a_fresh_directory_inside_the_moved_directory_is_refused`,
receipt `C-attempt1-filesystem_topology.txt`).

### A-1 The territory gate in 4.2 was unsound

Base `/d/e`, both stored. One batch allocates a fresh directory `p`, binds
`e/p`, binds `p/d` and unbinds `d` from the root. Under 4.2 as written the
moved stored directory `d` has a fresh parent, so no territory walk ran; the
rooted proof then walked `d -> p -> e`, found `e` stored, unplaced and unmarked,
and accepted `d -> p -> e -> d` with nothing under the root. Every derived
count is 1, so D-2 does not catch it either.

Corrected gate. The territory pass runs when both hold:

1. at least one stored directory is placed (moved), and
2. at least one placement of any directory, fresh or stored, has a parent that
   is stored, unplaced and not the root.

Soundness: with a valid base (D-1) and one final binding per directory (D-2), a
cycle containing a stored unplaced directory must leave the base tree through a
moved stored ancestor (1) and re-enter stored unplaced territory through a
placement (2). A cycle made only of placed edges is caught by the step-bounded
upward walk without any territory.

Cost consequence, replacing the sentence in D-3: no base walk happens unless a
stored directory moves **and** some directory is placed under a stored non-root
directory in the same batch. `mkdir existing/new; mv /old existing/new/old` now
walks `old`'s subtree once; a move under a fresh directory that hangs off the
root, or any batch that only creates, removes or renames within one parent,
stays walk-free. D-4 (owner question 1) is unchanged: ancestry evidence would
remove the walk.

### A-2 A fresh directory bound only inside a dropped directory is refused on both routes

A dropped directory is a declared-new directory that carries a header and is
never bound. A fresh directory `F` bound only inside it was refused by the old
build walk (`effective tree cycle`) and accepted by the old update walk, which
then emitted an unreachable directory inode. The rooted proof refuses it on
both routes with `effective tree cycle`: an upward walk that ends at an
unplaced fresh directory reaches neither the root nor a base position. This
strengthens the update route; no existing test pins the old acceptance. Owner
question 10.

Related finding, recorded not changed: files and symlinks bound only inside a
dropped directory are still counted and emitted with no reachable name (existing
behaviour on both routes). A stored directory or symlink that keeps its base
binding and is bound again inside a dropped directory used to be emitted with a
non-file count of 2; D-2 now refuses it with `multiple parents`.

### A-3 The classification window is a route property

4.2 said classification runs in windows of at most 64 rows. On the resident
routes the input is resident by definition and
`filesystem_bounds::binding_lookups_are_batched_per_phase` pins that the phase's
base demands are one grouped read whose wave count does not grow with the number
of bindings. That property is kept: the resident routes classify the whole
input as one window. The backed streamed route, which is the only route the
producer uses, classifies in windows bounded by a named constant row count, so
its resident state is bounded independently of the base and of the total change
and its wave count grows with change rows divided by the window. The exact
demand count in that test changes because one pass replaces two.

### A-4 Smaller points

- `FilesystemResources::check()` keeps its minimum `ordering_bytes` sanity
  refusal on every route; 7.1 behaviour 8 means no `ordering_bytes`-derived
  refusal of a total, not the removal of that minimum.
- On the resident routes, which have no backing, the topology containers are
  bounded by the existing resident ordering budget and refuse on container size
  with the error the existing resident limits already use. Work examined is
  never a refusal. The label `cycle check work limit` stays retired.
- The benchmark probe
  `core/benchmark/fs-bench-pro-storage-content/tests/namespace_batch_probe.rs`
  and architecture records 04, 06, 10, 51 and 53 describe the retired walks and
  limits; they are updated by the lead in the documentation step.

## Amendment 2 (2026-10-08): corrections after implementation and independent review

This amendment records what the committed plan got wrong or left out. The plan
text above is kept as written; where the two disagree this amendment governs,
and source at the R4 completion commit governs both.

### B-1 Five bracketed subtotals in the section 3 layout were wrong

The per-file `prod` numbers in the section 3 listing were taken from the pinned
`tools/production_loc.py --files` at `37dcf5405` and are correct. Five directory
brackets did not equal the sum of the files listed beneath them. Recomputed
from the listed files:

| Line | Printed | Sum of listed files |
| --- | --- | --- |
| `layerfs-content/src/filesystem/` | `[2559]` | 2487 |
| `layerfs-overlay/src/namespace/` | `[460]` | 479 |
| `layerfs-overlay/src/` | `[1015]` | 1034 |
| `layerfs-daemon/src/overlay/` | `[956]` | 916 |
| `layerfs-workspace/src/` | `[1702]` | 1708 |

No decision depended on a bracket. The commit comparisons use the whole-tree
counter, not this listing.

### B-2 In-place renames (not in the plan; added during track C)

Under A-1 a stored directory renamed inside its own parent (`mv p/old p/new`)
is a placement of a stored directory, so with the gate open its whole subtree
was listed although it cannot have moved. Track C added one pass,
`validate/in_place.rs`, between classification and the territory pass:

- For each stored parent that holds a stored directory's placement, the
  parent's change rows are read once through the input's own cursor, 64 names
  at a time, and resolved in the parent's base listing with one grouped lookup
  per window. Record kind `0x4653_0024` marks a scanned parent so several
  renames in one parent cost one scan.
- A base binding is *displaced* when the row removes its name or binds the name
  to another inode. A displaced stored directory whose placement names that
  same parent is marked in place. It then counts as stored and unplaced: it is
  not walked as moved, and a placement under it counts as landing under a
  stored directory.
- A name the row restates is not displaced. A directory that keeps its base
  name and gains another has two bindings and is refused by its derived count.
- The first version recognized removed names only. An independent reviewer
  showed it missed `mv d d.old; mkdir d`; the rule above replaced it in the
  track C review round, with tests for the recreate, rotation and swap shapes
  and for cycles through them.
- Deviation from the gate's wording: the pass also scans the root as a parent.
  A rename directly under the root is then in place rather than a move to the
  root. Both readings are sound; the scan costs the root's change rows.

Counters added for it: `in_place_scans`, `in_place_rows`,
`in_place_directories`.

### B-3 A-3 restated: what is one window and what is the whole input

A-3 said the resident routes classify the whole input as one window. That is
not what the code does. On every route rows are classified 64 at a time, with
one grouped restated-name lookup per stored parent and one placement batch per
window. What is whole-input on the resident routes is one grouped inode demand
made before the first window (it keeps the pinned wave count), the memo of
`ordering_bytes / 1024` records, and the topology evidence held in resident
maps under the same bound. `peak_window_rows` reports the rows that demand
covered: the total on the resident routes, at most 64 on the backed route.

### B-4 The territory gate is one decision for the whole operation

A-1's gate opens when some stored directory is placed out of place and some
placement lands under a stored, unmoved, non-root directory. The two need not
be related. An unrelated `mkdir` under a stored directory in the same operation
therefore makes a directory moved to the root list its subtree. This is sound
and bounded by the moved subtrees, but it is not the least work. A narrower
sound gate exists (list a moved stored directory only when its own upward chain
of placements lands on a stored, unmoved, non-root directory). It is not
implemented in R4 and is reported for owner decision.

### B-5 Rules the plan stated loosely

- A walk that ends at an allocated directory nothing binds is refused with
  `effective tree cycle` on build and update alike, whether the directory
  placed inside it is fresh or stored. A-2 named only the fresh case.
- The producer's Content result on failure is a label. The unchanged Commit
  driver treats that label as an unknown outcome, so a caller must hand the
  driver the original cause from `CapturedNamespaceCustody`. Section 7.3 did
  not say this. The R4 daemon test closure does it; the product closure is R5.
