# R1d compatibility reference-tier stream prospective correction

> **Prospective source freeze, 2026-10-01; implementation and owning checks
> unrun.** Developed against published `50a4f3a19` and the current retained R1
> source. Root must explicitly confirm the ongoing full command has completed
> before product edits begin. This corrects the old public C1 compatibility
> reducer; the supplied native Canonical8 Count/Release route is unchanged.

SC-03/05/06/07 are affected. The original parent-lookup quota matrix remains
unchanged, including its eight historically supported cells. No ordering quota,
pending limit, merge-buffer allowance, worker count or deadline increases.

## Cause and required ownership

The actual full-Core failure is retained in
`benchmark-results/fs-bench-pro/issue287-r1-final-full-core/cargo.log`:
pending1/ordering864/base-batch1 refuses actual960. The preserved effects-before-
values sequence is five additions to serial7 followed by directory values2..6.
It leaves immutable runs `[2,3,4,7]` (384 bytes) and `[5]` (96 bytes), plus
pending6. Materializing a five-row consolidation output while both inputs
remain open requires480+480=960 bytes. Before R1d commit `d1e478ffb`, adopting
the newest96-byte input removed its charge while its handle stayed live; the
historical864 admission therefore omitted that owner. The R1d custody fix stays.

Both `touched_serials` and `finish` currently consolidate. Removing only the
first consolidation merely moves the refusal to `finish`. The selected repair
removes both unnecessary materializations and streams the original live tiers.

## Concrete algorithm and source boundary

Own `filesystem/references/{runs,reduce,merge}.rs`, one focused private
`tier_stream.rs`, its declaration, meaningful new external ordering tests, this
freeze and `architecture/04-filesystem.md`. A compatibility-output caller is
changed only if required to preserve cleanup before filesystem-root emission.
No Server, native state, SQL, pool, guard, Canonical8, source format or manifest
change is selected.

One shared cursor contains a fixed array of at most `MAXIMUM_LEVELS=32` tier
controls, each with an optional complete Row head, next byte offset, exact
remaining count and previous serial. It borrows each exact immutable Run per
advance. One inline96-byte raw buffer decodes the next row; there is no new
per-tier Vec buffer, population vector, map, prefix replay or consolidation
output. All Run metadata is checked before the first head is read. Each read
checks exact reported length, complete96-byte grammar, bounds, first/last,
strictly increasing serials and checked offset/count arithmetic.

The cursor selects the smallest head serial. On ties, the lowest populated
tier wins because it is newest; every head at that serial advances exactly once
before the winner is returned. Hidden older duplicates are still decoded and
validated. There is one physical forward traversal per original tier, including
duplicates. No backward point changes this cursor; RunStore::find retains its
independent permanent forward/high-water and exact fixed-row point behavior.

The borrowed touched-row visitor uses this same cursor, preserving pending-map
precedence and the existing explicit compatibility touched-state collection.
It leaves all unchanged tier lookup high-water controls and buffers intact;
the replacement borrowed head composition fits the former visitor's additional
RunReaders and buffers while those lookup controls remain live.
`FinalRows` owns a fixed `[Option<Run>;32]` transferred from the store, the same
cursor, and the existing bounded authenticated base-read/lookahead wave. The
pending BTreeMap moves through `into_values`; it is not collected into a second
Vec. Pending wins every run at an equal key. Complete Count/Effect row semantics,
stored base count authority, final value/removal order and root retention remain
the existing algorithm. FinalRows does not borrow the backing, so the existing
public finish lifetime and caller API remain usable.

## Capacity, completion and failure custody

The unchanged `ordering_bytes` ceiling continues to admit encoded pending/spill
storage, every still-owned input file and prospective producer output before
creation. Before ownership transfer, check the complete current live composition
against that ceiling. No output file is created for touched/final scans. Keep
the transferred input total charged in RunStore until known checked backing
release; transfer does not refund it. Actual backing-held bytes remain the
conservative floor, including failed removal. FinalRows owns every transferred
handle; drop/EOF closes the actual input owners, then the established caller
checks backing release exactly once before any filesystem-root object exists.

Resident tier controls are a separate fixed compatibility working composition,
as the existing at-most32 lookup controls and independently declared merge
buffers already are. Before source approval, root must confirm this distinction:
the fixed controls are not added as encoded run bytes to an864-byte storage
ceiling. Compile-time actual `size_of` expressions must show the new fixed
cursor/handle composition fits the previous maximum tier-control/scan/buffer
composition at the existing minimum96-byte merge buffer; no per-tier buffer or
64KiB/native8/global/physical fit claim is introduced. Actual returned/base
waves retain their existing declared batch bound. No allocation failure selects
another algorithm.

The source assertion uses actual Rust types, not encoded96 as a substitute for
Row layout: `size_of::<TierHeads>() <= 32 *
(size_of::<RunReader<'static>>() + ROW_BYTES)` for the borrowed visitor, and
`size_of::<[Option<Run>;32]>() + size_of::<TierHeads>() <= 32 *
(size_of::<Option<Run>>() + size_of::<Option<LookupScan>>() + ROW_BYTES)`
for the final owner. Initial final transfer first destroys lookup scans and
their Vec allocation, then transfers handles and destroys the old levels Vec
allocation before creating heads. The transient old-levels/new-handle-array
overlap is bounded by the same latter expression without heads. Each source
buffer/metadata owner is actually dropped before its former working capacity
is treated as available. The pending BTreeMap into-values cursor keeps its
original nodes; no additional row population is created. It replaces the old
pending-to-Vec copy whose BTreeMap nodes and new row Vec briefly coexisted.

Metadata mismatch, truncated read, malformed row, out-of-order row, original
provider error or checked arithmetic failure is terminal. Cursor and FinalRows
retain the first error; another next call returns it without another read or
fallback. A newer-tier error cannot adopt an older value. Partial inode objects
may exist under the established sink contract, but failure cannot emit a final
FilesystemRoot. No retry, in-place input append, guessed refund or durability
claim is introduced.

## Independent owning vectors and exits

Retain the original parent quota matrix unchanged. Add focused public-API
ReferenceReducer cases with independently folded complete Count/Effect rows
across genuinely overlapping tiers, pending supersession, descending names and
both base batches1/64. A real-file observer records no backing creation/append
between completion of mutations and touched/final streaming; final rows must
match the independent event fold and cleanup must leave no owned storage.
Corrupt/truncate real selected input rows through the existing external backing
observer, including hidden older duplicates, and require the original terminal
error, no reread after failure and no final root adoption. Existing immutable
input/consolidation, scan/seek, reference, hardlink and parent proofs remain
required; the explicit public consolidate method is unchanged and keeps exact
live-input/output admission.

Root alone runs the covering locked command after the coherent source freeze.
Passing prior cases are reused where unaffected; all retained failures remain.
Whole-Core checks, physical/global/cache/native qualification, formal maximum
profiles, R3 content lifetime and full R1 completion remain separate gates.
