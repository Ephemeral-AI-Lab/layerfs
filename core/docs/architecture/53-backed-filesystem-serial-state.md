# Backed filesystem serial state

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> Status: additive source following checkpoint `dcdf527584675849e7839ca4118d71ac9aa4b193`.
> The preceding ten serial-state public cases and affected host/Linux checks pass
> in the [component checkpoint](../issues/307/SPARSE-SERIAL-PROGRESS-20261007.md).
> The P13 reference/release extension following `71a3a24b8` has thirteen public
> cases passing on host and Linux in the
> [captured-file/reducer checkpoint](../issues/307/CAPTURED-FILE-REDUCER-20261007.md).
> That later evidence does not relabel the earlier checkpoint or complete
> namespace normalization, P14 or S10.

The streamed directory entrypoints already keep each changed-name pass outside a
resident directory Vec. The additive `build_filesystem_streamed_backed` and
`update_filesystem_streamed_backed`, and their timed forms, now put four serial
state domains into the caller's existing construction-record provider: new-parent
membership/held state, rebuilt directory roots, initial retained-binding counts
and the fixed attempt context. The P13 extension adds fresh membership, fixed
reference rows and touched witnesses, zero-release FIFO, release frames and node
states. Both routes use one canonical operation driver, validator, directory/inode
merges and shared reference-row arithmetic/final derivation.

## Public boundary and explicit scope

Each new function receives `FilesystemObjects`, `PreparedDirectoryStreams`, a
mutable `IndexedConstructionBacking`, optional existing `OrderingBacking`, and
coarse phases for its timed form. `IndexedConstructionBacking` and the
`ConstructionRecord*` names are aliases of the existing edit record protocol,
not another trait or wrapper. Workspace's `IndexedConstructionRecords` is the
same owning adapter over `OverlayOperationRecords`; OwnerClient supplies the existing
credited short jobs in the daemon's existing database.

The caller binds that adapter to an actual OperationOwner and a distinct opaque
construction `file_scope`. The namespace attempt must not reuse any regular
file editor's scope/attempt marker. Content acquires no owner/database, discovers
no provider and performs no operation release. Callers retain the original
refusal/Completion, capture/read/Save custody and actual fences before explicit
last-owner release. Drop does not release or repair an operation.

The private filesystem domains are `0x46530000` for context, then parent, rebuilt
root and initial count. Then `0x46530004` through `0x46530009` address fresh,
reference Row96, touched, FIFO work, cursor frame and release node state. They are
disjoint from the file editor's 1–10 meanings.
Every serial key contains all eight big-endian serial bytes in its full 32-byte
key; no hash or probabilistic prefix proves identity. Scope and kind remain
independent coordinates even when raw keys coincide.

The one Missing-guarded context marker is 106 bytes: version, base-presence flag,
all 32 base-root bytes, 32 scope bytes, 32 filesystem-profile bytes and eight
root-serial bytes. None is distinct from an all-zero root identity. The backed
route checks stable input shape before this marker; a busy/reused marker returns
the original deciding NotApplied result before new mutable state or canonical
output. The marker binds this attempt's local interpretation. It is not runtime
authority, root/topology qualification, Save success or history publication.

## Serial state and shared order

[state/store.rs](../../crates/layerfs-content/src/filesystem/state/store.rs)
implements versioned bounded raw points and single guarded transitions. Parent
records are declared Missing before bindings can mark them held. Exact raw bytes
guard every transition. Initial builds declare a zero-count record for each
sealed fresh serial before directory effects, so a stale or unexpectedly missing
count is refused rather than treated as a new zero. Rebuilt roots are inserted
Missing and read by their exact serial. A missing root for a retained directory
is refused; regular/symlink values retain their supplied immutable content root.
During the later supplied-value pass, a declared retained header still requires
its rebuilt ROOT record: disappearance after root replay cannot turn that value
into a metadata-only update and overwrite the constructed root. A genuine
metadata-only directory value with no header retains its existing content root.

Every batch accounts for outer descriptor capacity and nested expected/value Vec
capacities within 65,536 bytes. Provider errors and NotApplied end the operation
at the first deciding call; no refresh, failed-operation replay or memory/stored
fallback follows. The source shape contract remains stable across all passes.
These guards are per short batch, not an assertion that the whole constructor is
one SQL transaction.

The existing parent prepass keeps its meaning: a declared-new parent is dropped
when no stated binding holds it. This is not a root-reachability certificate.
Backed dropped lookups use the real sealed header/fresh point facts to determine
which parent record must exist, then read that exact record. Its scalar count
tracks transitions; it does not scan/count an entire operation records table.

[state/roots.rs](../../crates/layerfs-content/src/filesystem/state/roots.rs)
consumes the sealed parent headers to obtain rebuilt roots, one bounded batch at
a time. It needs no operation records-key scan, resident root map or alternate ordering
algorithm in the backed route. The resident route keeps its existing sorted map.
All directory binding effects still precede value insertion. The final retained
parent/base batch remains for the existing bounded read reuse.

[state/initial.rs](../../crates/layerfs-content/src/filesystem/state/initial.rs)
streams initial inode rows through the sealed fresh cursor and exact count/root/
value points. It stops at the first original iterator/point/shape failure and
checks complete count consumption. Backed construction collects neither all
counts nor all final inode rows. Resident construction retains its count Vec and
complete final-row Vec, including its rows-before-inode-output error order and
phase placement. Existing public signatures, `validate::check` and `CheckedInput`
remain unchanged; their explicit resident contract is preserved.

An input failure during backed final-row consumption may follow accepted child
pages. Those pages remain with the actual consumer/Save. No final filesystem root
is emitted after that failure. Final-root consumer refusal retains the consumer's
own custody contract; a returned error never becomes guessed success. Ordering
run cleanup keeps its existing checked-before-root boundary. Construction-record
scope cleanup belongs to actual caller fencing/owner release, not a new Drop job.

## Indexed reference rows and release

The old public `ReferenceReducer`, `FinalRows` and `release_zero_count` remain
resident/run APIs. The backed streamed update instead uses a private direct
record store, not a mutable tree, sorter, RunStore fallback or collected list.
[references/meaning.rs](../../crates/layerfs-content/src/filesystem/references/meaning.rs)
owns one shared arithmetic and final derivation for both routes. The Row96
encoding is unchanged. Stored base count plus signed Effect supplies an existing
count; supplied values supply only kind/content/metadata. Count rows derive their
count from retained bindings. Checked positive overflow now returns LengthOverflow
rather than converting a count above u64::MAX to zero in the seed or wrapping a
final value. The selected negative-count clamp remains zero on both routes.

Fresh declaration is separate from touched membership. Declaration atomically
guards fresh/row/touch absence; it does not eagerly create a count row for an
unused allocated identity. Each real point mutation guards the exact existing
Row96 bytes and touched witness (or both Missing) in a single bounded job. A
missing half, wrong key/serial, malformed version or Count/Effect disagreement
with actual sealed fresh facts refuses before further requests. Touched
cardinality is a checked scalar of successful row insertions, not a resident set.

The raw port's additive `keys_after(kind, after)` returns at most64 complete keys,
strictly greater than the actual full32-byte boundary. None includes all-zero
identities. Unsupported providers refuse explicitly. Content checks returned
capacity/count/order, serial-key grammar and every required row/touch/fresh point.
A complete pass checks actual consumed membership against the successful row
cardinality. Rows stay intact across passes. No required missing record becomes
zero or an initial/base fallback.

The complete zero-seed pass finishes before any directory release. It appends
versioned work items carrying the already read authenticated base fact to a FIFO
and node state, together in a guarded batch. The work ordinal/head/tail are exact
BE8 counters, independent of inode serial order. First-key windows are consumed
only after guarded transfer into the next cursor state; empty/missing or skipped
FIFO work is an error, never completion. Directory frames retain exact root,
selected reader provenance, actual PathName continuation and finished state.
One current frame and one bounded page/base wave are local. Frames and FIFO work
are backed; all independent seeds are pushed before paging, preserving resident
traversal order. `ReleaseWork::peak_depth` remains peak simultaneous release
cursor-frame occupancy, including sibling seeds. It is not namespace path depth,
an aggregate resident-memory measurement or O(path-depth) backing-size evidence.

Each child decrement, row/touch guard, optional newly queued directory, node
state and exact consumed name progress is one all-guards-before-effects change.
The cursor never advances/delete-acks first or refetches/resends after uncertainty.
Repeated bindings still perform every decrement; existing node state prevents a
second traversal of a directory already queued/transferred. Rows needed by later
counts are retained. Release can discover a lower serial; only after release seals
does the final row cursor restart from None and stream canonical serial order.

`FilesystemObjects::new_with_accepted(reader, consumer, accepted)` explicitly
supplies the actual same-Save authorized accepted-object reader. For a released
directory, a fresh selected root or a root differing exactly from its retained
authenticated base root uses that capability. Unchanged roots use the initial
reader. Capability absence is `filesystem accepted-object reader unavailable`;
an original accepted-read error is retained without querying the initial provider
as a fallback. This constructor is application capability data, not a topology,
authority or durability certificate. The Save producer must compose the real
same-Save capability; this component's external memory fixture does not implement
or qualify that integration.

The shared public rule still refuses removal of a Count/new row and an unbound
new final row. Specifically, a fresh child added inside an existing directory
rebuilt and then released in the same update currently refuses on required base
child absence (`released child`), with accepted children retained and no final
filesystem root. This is a remaining normalization/release semantic correction,
not an external permission gate or complete Commit support. No guessed count,
base absence fallback or silent new-row removal was introduced for that case.

## Remaining domains and bounds

The parent map and resident reducer/run/touched/zero/release containers retain
their existing allowances only on the explicit old route. Backed serial/reference
state can advance through more short jobs subject to physical admission. The
backed route does not create ordering runs or check their total row-byte ceiling;
optional supplied OrderingBacking still receives its independent checked cleanup
before a final root, while construction-record owner release stays caller fenced.
There is no OperationOwner release in Content or Drop.

This does not remove validator totals or every input-size container. Validation
still derives declared/walk limits from ordering_bytes and can refuse: grouped
complete demand vectors, additions/by-parent/candidate/bound maps, seen/frontier
sets and the alias whole-base/rebound subtree walks remain. Initial validator
row totals still use those limits even though its count/final rows are backed.
Directory-tree listing retains its existing canonical-tree traversal internals;
this is not the deferred complete P14 qualification. Captured ordered-name/count/
tri-state jobs, typed namespace normalization and complete Commit/Save/history/
install composition remain separate work. The regular-file adapter is described
in [captured file normalization](56-captured-file-normalization.md).

P14 requires owning qualification of the exact immutable root and actual checked
membership/changed-parent evidence. Neither the local context marker, an input
boolean, a reverse SQL hint, SDK binding nor BaseView's bounded root check
establishes complete reachability, alias uniqueness or acyclicity. No whole-base
scan is hidden in this state constructor, scope acquisition or mount/bind setup.
The existing validator is not bypassed and no alternate graph engine is selected.

These changes pay actual point/guard jobs, raw codec work, extra input passes and
service-return copies. `ReferenceWork::rows_touched` retains its historical
resident pending insertion/reload meaning and reports raw row creations on the
indexed route. It is not the total mutation count; adapter job diagnostics own
that work domain. Existing FilesystemUpdateCounters still measure canonical
work; adapter counters observe only their declared conversion/reply domains.
They are not aggregate heap/RSS or pager/journal/OS-cache bounds. No speed,
cold-cache, physical-storage or complete no-cap qualification follows here.

## External verification scope

[filesystem_state_backed.rs](../../crates/layerfs-content/tests/filesystem_state_backed.rs)
uses the public constructor and existing neutral port. It checks canonical roots,
values and independently read bindings/counts; exceeds the old parent-map
allowance while respecting remaining declared limits; retains a dropped empty
parent; exercises regular-file aliases and ordered rebuilt-root consumption;
refuses context reuse/stale serial guards and separates file-editor kinds;
preserves original failures and accepted child pages; rejects malformed/missing
state rather than falling back; and asserts capacity-accounted jobs. Its point-
only serial-state path never enumerates parent/root/count domains. The updated
fixture permits only WORK first-key windows and TOUCH non-destructive windows.

The fixture is external and resident. It is not SQL/runtime, memory-residency or
performance evidence. The integration owner ran locked no-run first, bounded
bodies and affected host/Linux checks. The checkpoint retains their exact
commands, failures and repairs. Actual filesystem/Save/history/owner-cleanup
composition remains required.

[filesystem_references_backed.rs](../../crates/layerfs-content/tests/filesystem_references_backed.rs)
public cases cover bounded multiwindow seeds/final scans, lower serial discovery,
repeated bindings and surviving aliases, a70-directory chain and independent
sibling frame occupancy, UTF-8 exact paged boundaries, row/touch/fresh/key errors,
atomic guard/original vector custody, missing work, accepted-reader absence and
original refusal, known accepted children with no final root, the explicit fresh
child release limitation, authenticated u64 count overflow/control behavior, and
late final-row failure with no EOF finalization/postfailure provider or emission.
The final change adapter forwards the original fallible row error into the sorted
inode engine; it does not turn that error into end-of-stream before checking it.
Their fixtures retain whole input/output in memory deliberately; they establish
functional component assertions, not SQL/runtime/residency/performance evidence.
The integration owner's [host new-body receipt](../issues/307/checks/captured-reducer-20261007/09-content-new-bodies.json)
and [Linux Content receipt](../issues/307/checks/captured-reducer-20261007/14-linux-content-bodies.json)
record thirteen reference/release cases and ten serial-state cases passing on
each platform, alongside the seven indexed-view cases. The host's 257 affected
existing Content cases are retained in the
[regression receipt](../issues/307/checks/captured-reducer-20261007/11-content-regressions.json);
Linux's combined receipt records all 287 Content bodies. Original build failures
07 and 12 and captured fixture failure 16 remain in the checkpoint, with their
exact repairs and separately retained passing selections. Actual same-Save,
filesystem/history and owner-cleanup composition remains required.
