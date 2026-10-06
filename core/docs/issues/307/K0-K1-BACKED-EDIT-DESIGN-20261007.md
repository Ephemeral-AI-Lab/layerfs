# Captured construction inputs and backed file-edit design

> Status: implementation proposal and source investigation; not an implemented
> backed editor, measured algorithm or S10/P3 completion. Source pin `86f766750029ab7f4224b587e0f20a7779813e89`.

This records the ready K0/K1 owning corrections selected by the
[S7–S13 plan](IMPLEMENTATION-PLAN-S7-S13-20261007.md) and the
[Commit contract](../303/workspace-api/commit.md). The [source inventory](checks/k0-k1-design-20261007/source.json)
fixes the inspected checkpoint. Content owns canonical editing; Workspace owns
captured normalization and neutral adapters; Overlay owns indexed local scratch
in the existing daemon database; Daemon owns short fair jobs and credited replies.
The Project acquisition schema/global Store supplies no Commit scratch.

## One captured frontier and exact phase ownership

Normalize final state from existing captured file/namespace readers, installed
generation floors and the retained immutable base. Do not replay chronological
FUSE events as canonical edits. Multiple sequential/concurrent calls can contribute
to the same capture. Later active changes remain separate and must survive a known
paired install. Capturing retains existing stable input/custody; it must not bulk
copy the overlay or hide a whole-base validation during bind/setup.

| Phase | Original owners and exact disposition |
| --- | --- |
| Admission/capture | At most one pending/unresolved Commit for this Workspace. Refusal occurs before capture effects. Retain exact captured readers/root/generation and any operation scratch ownership |
| Normalization/context | Indexed final changed inode, name, metadata and sparse-run cursors; bounded pages. A definite failure leaves active state and committed base intact. Keep original reader/port failure; no missing-draft/base fallback |
| Canonical construction | One constructor/Save producer, children before parents, Store-derived policy, exact captured base/scope/serial/provenance. Same-Save reads see accepted children. Published waves may already exist when a later phase fails |
| SaveFinish | Attempt once, retain exact acknowledged/definite/cleanup/unknown outcome. Known finish precedes Stage. A lost reply is no permission to repeat or delete acknowledged objects |
| Stage/CommitStaged | Exact captured expectation fields and acknowledged token; once-only. Conflict retains deciding stage disposition. Unknown history publication stays terminal custody under P10; unfenced read/refresh is no resolver |
| Known local install | Prepare checked new base, then paired engine/root install preserving later mutations and effective live view. Report known publication plus failed local install separately. No guessed replay/rebase |
| Release | End actual construction/read/transport ownership before releasing eligible scratch/capture. Automatic bounded live/idle cleanup follows last owner. Unknown calls retain custody; terminal unmount never implies global history deletion |

K2 must provide incremental topology/reverse-binding and provenance evidence before
root publication. The current direct-child admission/root check is a bounded slice
and supplies neither full closure nor a whole-topology certificate.

## Exact incumbent file-edit defects

The active editor is `EditObjects`, not an active EditBatch. Its `tree.rs` has
978 physical lines at this source pin. It holds drafts, parent-reference counts,
detached IDs and resolved IDs in resident trees. The8MiB-derived deferred refusal
counts Draft memory only; it does not count reference/detached/resolved/emitted
sets. Settling collects every detached key before draining, and final emission
retains output-sized dedupe state. The u32 draft counter and unchecked parent
reference increments become smaller hidden limits when only the deferred refusal
is removed. These cannot become bounded by renaming an outer cursor.

`load_node` also inserts every missed stored canonical page without calling the
existing `PageCache::make_room_for`; insert relies on that caller-side eviction.
The same path clones canonical bytes before decoding. A separate focused repair
will enforce the current64-page allowance and move checked canonical bytes. This
does not repair draft/index state or establish aggregate residency.

`apply.rs` materializes every Segment even when the final result fits a small
WholeFile. Replacement construction and comparison currently read byte windows
through huge zero ranges, so optimizing emission alone does not remove O(hole
length) work. Existing split/concat and DFS frontier depth is constrained by the
canonical31-level format; that stack is separate from the growing identity sets.

## Additive owning port and existing database provider

Preserve current public signatures and algorithms. Add `apply_edits_backed` with
an explicit caller-owned `IndexedEditBacking` argument, alongside the existing
memory-profile entry point. The latter has no backing parameter; preserving it
does not make it unbounded. Ordinary Commit selects the backed route directly,
with no hidden SQLite/file creation, TLS provider or failure-driven substitution.
The exact remaining memory-profile limit remains documented and unqualified for
the huge/fragmented Commit target.

The proposed Content port uses full ObjectId keys and bounded raw records:
Draft, ParentRefs, Detached, Resolved, Emitted and demand-built ZeroProof where
needed. Required operations are exact contains/get, bounded atomic record changes
and a detached-key window excluding one retained live root. Values and combined
changes stay within the existing64KiB owning byte window; detached windows return
at most64keys. C1 owns temporary-draft serialization, checked counts and canonical
semantics. A raw draft cannot be encoded as a final canonical node while child IDs
and root/non-root fill remain undecided. Use checked address-width counters and
report platform/format limits; do not introduce a smaller total-edit refusal.

Extend Overlay's scratch domain in its one existing database with indexed keys
`(namespace, operation, file_scope, kind, key BLOB32)` and bounded value rows. The
current integer-key owned_scratch keyset port lacks exact point get/delete, full
ObjectId indexes and atomic reference transitions; lossy u64 hashes or linear
collision chains are not acceptable substitutes. Reuse the engine-minted
OperationOwner and Scratch service class. Extend exact namespace/operation
release and automatic SCRATCH reclamation to these rows. Daemon adds short typed
commands/credited receipts; Workspace adapts the neutral port. SQL never performs
Content's split/concat algorithm and never opens a second database.

Back all draft/reference/detached/resolved/emitted state, keeping only grammar-sized
pages and the format-depth stack. Preserve repeated-child multiplicity, stored
subtree reuse, half partitions and emission of final reachable mappings only.
Drain detached state from the first indexed eligible key each bounded round,
deleting consumed candidates, so children inserted below an earlier key are found
without growing OFFSET or a whole-set collection. Emitted state distinguishes an
original accept attempt from its acknowledgement; it does not stand in for
SaveFinish/history publication. Preserve the first typed backing/consumer cause
in adapter custody beneath Content's narrower error interface.

## Sparse replacement and comparison

An additive default `EditSource::read_run_at` can preserve existing implementers
through Data/read_at while captured sources expose Zero spans. Runs and their
replayable offsets must sum exactly to each replacement declaration and remain
stable across comparison/construction. The ordinary captured provider needs an
indexed composed-run cursor over exact captured generation/root/floor, validity,
cutoffs and shrink epochs. Repeating128KiB byte reads through a huge hole is not
that cursor.

Reuse Scanner.consume/zeros/finish and ExtentBuilder's repeated-zero-chunk path
inside a shared chunk-run helper. Calling construct_runs for a localized middle
would select WholeFile/FileState/cutoff behavior that differs from an extent
subtree; it is not a compatible replacement. WholeFile→chunked construction uses
a lazy Plan run cursor and continuous scanner; below-cutoff construction consumes
Plan lazily and holds only bounded final bytes. Preserve predecessor provenance.

Comparison must consume Zero runs too. Authenticated mapping/payload zero evidence
can be memoized on demand in the same operation backing using the frozen zero
identities. A verified zero subtree skips its covered span; partial/nonzero ranges
descend the overlap and pay every actual canonical/payload demand. Diverse
fragmentation still pays its actual nodes. No whole-base pre-validation or universal
O(log file length) claim follows.

## Implementation order and required evidence

1. Repair the actual PageCache miss path and lazy small-result Plan processing.
2. Split tree responsibilities before extending the978-line file, define C1's
   indexed port/draft codec, implement Overlay/Daemon jobs and Workspace adapter.
3. Add sparse run construction/comparison and captured final-state normalization.
4. Integrate K3's single producer, full K2 context evidence and exact K4/K5 phases.

Public regressions retain current canonical/reference/no-op/cutoff/predecessor
results on the backed route and cover scratch exceeding8MiB, dense/sparse>4GiB,
fragmented edits, lower-key detached insertion, repeated-child DAG references,
duplicate emission, huge edit count shrinking to a small result and two operation/
file scopes. Original source/backing/consumer failures must end once with exact
custody. Sparse cases include odd CDC boundaries, zero/nonzero transitions,
insert/overwrite/shrink/regrow, representation transitions, zero no-op and later
mutations across capture. Independent root/range oracles must retain their real
acquisition and payload scope.

SQL qualification requires exact EXPLAIN and correlated actual VM/bind/returned/
visited/trigger/byte/debt work. Observe Content windows separately from pager/
journal, the268435456-byte reservation, OS caches, runtime Save/transport and held
cleanup debt. A corrected cache or backed draft count alone is no whole-system
residency/speed proof. No campaign is launched by this design. The immutable
Init/history failures/passes, cache gates and one-attempt policy remain unchanged.

## First implemented correction

The [R4/cache checkpoint](R4-UPSTREAM-CACHE-20261007.md) implements the identified
page-cache miss eviction and validated-byte move, with public retained/evicted/
invalid-page regressions. It leaves this source-pinned investigation intact.
Lazy Plan, backed state, sparse/captured normalization and integrated Commit remain
proposed/unfinished; the bounded page repair does not establish those capabilities.

The later [indexed-scratch/lazy-Plan checkpoint](K1-INDEXED-SCRATCH-E01-20261007.md)
implements lazy small-result Plan consumption and extracts the unchanged memory
state from the tree algorithm. Overlay/Daemon now provide the neutral indexed
records. Actual Content-backed state, its fallible Workspace adapter, sparse runs,
captured normalization and Commit wiring remain unfinished. This later correction
does not change the investigation's source pin or qualify its proposed limits.
