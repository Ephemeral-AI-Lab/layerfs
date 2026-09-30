# R1d-run-seek selected responsibility and exit contract

> **Status: Next implementation boundary; unrun.**
> Selected2026-10-01 after source preparation against published06fe8363d.
> R1c is the prerequisite and must publish first. Its actual resulting parent
> is recorded in the append-only implementation log before product edits here.
> This note is a concrete plan, not an implemented or qualified milestone.

SC-03/05/06/07 are affected. The smallest complete responsibility is exact
reference-run lookup without repeated prefix restart on descending/interleaved
serial demands. It serves the existing ReferenceReducer entry/state and release
callers through RunStore::find. It changes no canonical format, IndexedState
table, SQL schema, Bridge grammar, writer count or declared resource allowance.

## Replaced work and selected algorithm

The existing find resets a tier scan to byte0 when its requested serial is behind
the cursor. Name ordering does not imply serial ordering; a sequence
`1,N,2,N,3,N,...` can repeatedly pay for the same prefix. Moving that sequence
unchanged into SQLite would preserve the wrong accumulated-work law.

Each immutable finalized tier keeps its sequential high-water cursor permanently.
Fresh forward demands continue that cursor; a backward demand uses an independent
allocation-free binary search over the fixed96-byte rows. The point search never
rewinds, reseeds or replaces the sequential scan, resume or buffered bytes. A
hit in the pending map still wins; tier0 remains newest and a newer-tier error
propagates without trying an older tier or another backing. A gap proof belongs
only to its exact immutable run. Replacing a tier invalidates all its lookup state.

The point helper searches `[0,count)`, checks midpoint/offset multiplication,
reads exactly96 bytes into one stack buffer, decodes the complete Row, and compares
its serial. Short read, malformed row, owner/length mismatch or provider error is
terminal. It does not start a sequential rescue scan or force consolidation.

Spill/merge producers establish finalized metadata: positive serials, strictly
ordered unique rows, first/last/count, checked count*96 and exact reported length.
RunStore::spill must check map-key equals row.serial before backing creation,
because iterating map values alone does not prove that predicate. C1 consumes
the completed handle without later append. Generic flush remains visibility;
it is not repurposed as a global immutable-seal method. Cached FileRun length
does not prove native file identity/fstat; stronger backing custody is separate.

For an unchanged tier of n rows, total sequential physical bytes are<=96*n;
Q backward points add<=96*Q*(ceil(log2(n+1))+1). Charge every replaced run's
lifetime and merge input/output; do not equate bounded buffered replay with
physical reads. Two-reader geometric merge and its existing admitted buffers
remain unchanged. No full run/frontier vector or new lifetime registry is added.

## Ownership and independent expected result

The owning worker receives references/runs.rs, merge.rs, a focused seek.rs,
references/mod.rs and the external ordering tests. Root retains integration,
Bridge, docs/counter/staging/commit/push/#287 ownership. Other C1/C2/runtime files
are outside that worker's scope. Workers preserve others' edits and run no
parallel Cargo command; root freezes the coherent source before checks.

An ordinary external observer wraps real FileBacking and records bounded scalar
read calls/requested bytes/maximum request/appends/flushes/releases. Expected
complete Count/Effect rows come from an independent last-assignment/event map of
input snapshots, never prior candidate lookup or consolidation output.

Required cases: fresh ascending one-pass; descending/repeated hits and gaps;
primeN then alternatinglow/high; multiple genuinely overlapping tiers with
distinguishable complete rows; newest-tier gap falling through to older data;
real ReferenceReducer spill with retained/removed/supplied-value effects; and
real-file short/corrupt read propagating its original error without fallback.

The old ordering_scan warmup-then-sweep expectation depends on restart. Split a
fresh ascending physical-read proof from already-passed-key point proof instead
of enlarging its bound. The old consolidate earlier-key branch assigns the same
serial expression; replace it with actual duplicate keys and independent newest
assignments. Allocation observation remains separate from native/physical proof.

## Exit and retained gates

Freeze and select locked owning content tests: filesystem_ordering_scan,
filesystem_ordering_consolidate, filesystem_ordering, filesystem_reference,
filesystem_hardlinks and the actual new count target when introduced. Run the
scoped examples/Clippy, whole-Core fmt, product boundary and guard self-tests
at the handoff. Reuse unaffected R1c evidence; full Core checks remain final gates.
Count/cause diagnostics are labelled and source-pinned, without a speed PASS,
new runner/family or #288 campaign.

Native OrderingBacking partial-append credit, release/Drop retry, removal identity
and stronger containment are existing unresolved custody gates. This slice must
preserve errors and checked completion before filesystem-root emission; it cannot
promote R1c scratch's stronger owner into a proof for the legacy backing. The
larger R1d paged draft/frontier/binding/reference/graph populations and R1e engine/
protected/physical resources remain open. R1 and concurrency stay disabled until
their actual prerequisite exits pass.
