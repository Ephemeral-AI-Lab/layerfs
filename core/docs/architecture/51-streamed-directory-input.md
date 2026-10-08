# Streamed directory inputs

> Status: additive source implementation after foundation `889836c44`.
> Component checks are recorded in the [joint checkpoint](../issues/307/K1-BACKED-STREAM-SUPERVISOR-20261007.md).
> This input boundary does not
> complete backed filesystem construction, topology provenance or S10.

The canonical directory merge already accepts a fallible ordered iterator. The
new boundary lets a caller supply one changed directory through fixed metadata,
a replayable change cursor and exact changed-name points. It removes the need
for that caller to return `DirectoryUpdate.changes` as one resident Vec. It adds
no database, directory-tree algorithm, automatic fallback or root certificate.

## Public contracts and compatibility

The additive types are exported by `filesystem::rows` and `filesystem`:

```text
DirectoryHeader { parent: u64, change_rows: u64 }
DirectoryChangeLookup = Unchanged | Removed | Bound(u64)
DirectoryHeaderSource::next_row() -> Result<Option<DirectoryHeader>>
DirectoryChangeSource::next_row() -> Result<Option<(PathName, Option<u64>)>>
StreamedRowSource
PreparedDirectoryStreams
StreamedFilesystemInput { base, scope, root_serial, resources, rows }
```

`StreamedRowSource` declares directory/inode/fresh totals; supplies a sorted
header pass, a header point, one per-parent changed-name cursor, a tri-state
changed-name point, and the existing typed inode/fresh cursor and lookup shapes.
Header absence means no directory update. A present zero-count header is an
empty update: it retains an existing directory or builds the actual empty page
for a new one. Unchanged means no changed-name row; Removed means a row declaring
absence. Those two answers are not interchangeable.

Every pass starts from the beginning and retains the same sealed captured
owner/root/frontier. Header and changed-name points must agree with their
cursors. A mutable current-view query or a refreshed source does not satisfy
this contract. The constructor checks observed header totals, parent ordering,
exact per-parent change counts, strict name ordering, serial grammar, matching
header points and matching points for every declared changed name. Source
completeness also requires points for names outside that sequence to report
Unchanged; this is the owning source contract, not a topology assertion.

`build_filesystem_streamed`, `update_filesystem_streamed` and their timed forms
run the same private operation driver as the existing build/update entrypoints.
`check_streamed_input` exposes the shared shape checks. Existing `PreparedRows`,
`RowSource`, `DirectoryUpdate`, `FilesystemInput`, `validate::check`,
`CheckedInput`, and all old function signatures were unchanged at this stage.
(R4 later removed `validate::check`'s unused dropped-parent argument and
`CheckedInput.additions`.) The old source
continues to return its explicit resident directory row. A streamed source is
a separate trait and never invokes those legacy Vec-returning methods.

## One owning driver

[rows/view.rs](../../crates/layerfs-content/src/filesystem/rows/view.rs) adapts
resident and streamed input to the same private operation view. Its resident
row owns the old Vec; its streamed row holds only a header and source reference.
Each streamed change iterator retains a previous name, count and first-error
termination state. No complete directory-change stream is collected there.
An original iterator error is returned once and the operation stops; a later
cursor is not opened to repair or replay it.

The ordinary update passes that iterator directly to the existing canonical
`directory::update::apply_bindings`. Existing additions-before-removals and
all-effects-before-values ordering remain. Final inode construction, reference
reduction, cleanup and root emission retain their owning implementations and
coarse phase scopes. An input failure stops before final-root emission; already
accepted child objects still belong to the caller's Save/consumer custody. A
consumer failure while accepting the final object retains that consumer's own
custody contract and never becomes a successful result.

At this stage alias survival used the tri-state stream point on the streamed
route and a changed-name map on the resident route. R4 removed the alias pass
on both: a second parent is refused by the derived count or by a second
placement ([captured namespace construction](78-captured-namespace-construction.md)).

[validate/entries.rs](../../crates/layerfs-content/src/filesystem/validate/entries.rs)
consumes the existing effective-name merge with one bounded canonical directory
page and one changed-name lookahead. The cycle walk no longer needs a complete
base/merged directory Vec. Initial reachability opens each selected directory's
cursor instead of collecting a resident adjacency map. These are input-consumption
changes. The alias whole-base and cycle subtree walks that remained at this
stage were replaced in R4
([captured namespace construction](78-captured-namespace-construction.md#validation)). Paged
consumption can interleave object reads with child inspection; it supplies no
new topology certificate or fixed physical-I/O/latency claim.

## Remaining state and ownership

This boundary removes the per-directory input Vec requirement, not every
input-sized collection. At this stage validation still had grouped demanded
serials, additions/candidate maps and examined/seen/frontier state; R4 replaced
them on the backed route with 64-row windows and indexed topology records. The
resource-sized base memo remains. The additive [backed serial-state route](53-backed-filesystem-serial-state.md)
now puts new-parent membership, rebuilt directory roots and initial count/final
row iteration behind the existing neutral construction records and sealed input
cursors. The resident route keeps its former maps/arrays. Reducer fresh membership,
touched/zero collections and release frontiers/prefetch records remain under
their existing ceilings and algorithms in both routes.
The legacy explicit row/spool APIs retain their existing resident-row limits.

Workspace still owes the real normalized captured source, with retained
CapturedReader/OperationOwner custody, exact root/floor/revision, sealed totals,
ordered name queries, provider failure retention and no hidden materialization.
Overlay supplies neutral owned records/queries in its existing daemon database.
Content continues to own interpretation, canonical sorted merges, reference
effects and the validator. Root-qualified parent evidence, backed reducer and
release state, real captured normalization and actual Commit remain later slices.
The [run-aware file editor](54-run-aware-localized-file-edits.md) supplies selected
sparse construction behavior without closing that integration scope.

The existing `FilesystemResources` ordering allowances still govern this
driver, including validation work and touched-state refusals. A streamed input
does not turn them into a total-state-independent physical admission profile.
There is no whole-operation allocation/RSS, cold-cache or performance acceptance
from this API addition.

## External verification scope

[filesystem_streamed.rs](../../crates/layerfs-content/tests/filesystem_streamed.rs)
generates 1,031 changed names without storing that input sequence in a Vec and
compares exact canonical root/value with the resident constructor. Its legacy
methods panic if invoked. Other cases preserve an original cursor failure,
stop before future rows/output, refuse count/order/header/point disagreements,
distinguish a move from a surviving second parent, compare a multi-page effective
directory merge with removals and insertions, and reject a self-cycle through
the same validator. Borrowed fixture streams test compatibility; they are not
a production SQLite/captured source or a resource qualification.

The author ran no Cargo/build/test/Docker/sample commands. The integration owner
executes locked no-run first, then bounded public bodies and affected format,
warning-denying Clippy and source-boundary checks at the final source identity.
