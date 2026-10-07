# Backed filesystem serial state

> Status: additive source following checkpoint `dcdf527584675849e7839ca4118d71ac9aa4b193`.
> Ten public cases and affected host/Linux checks pass in the
> [component checkpoint](../issues/307/SPARSE-SERIAL-PROGRESS-20261007.md). This is a component state boundary, not
> captured normalization, complete bounded filesystem construction, P14 or S10.

The streamed directory entrypoints already keep each changed-name pass outside a
resident directory Vec. The additive `build_filesystem_streamed_backed` and
`update_filesystem_streamed_backed`, and their timed forms, now put four serial
state domains into the caller's existing construction-record provider: new-parent
membership/held state, rebuilt directory roots, initial retained-binding counts
and the fixed attempt context. They use the same canonical operation driver,
validator, directory/inode merges and reference-effect algorithms.

## Public boundary and explicit scope

Each new function receives `FilesystemObjects`, `PreparedDirectoryStreams`, a
mutable `IndexedConstructionBacking`, optional existing `OrderingBacking`, and
coarse phases for its timed form. `IndexedConstructionBacking` and the
`ConstructionRecord*` names are aliases of the existing edit record protocol,
not another trait or wrapper. Workspace's `IndexedConstructionRecords` is the
same owning adapter over `OverlayScratch`; OwnerClient supplies the existing
credited short jobs in the daemon's existing database.

The caller binds that adapter to an actual OperationOwner and a distinct opaque
construction `file_scope`. The namespace attempt must not reuse any regular
file editor's scope/attempt marker. Content acquires no owner/database, discovers
no provider and performs no operation release. Callers retain the original
refusal/Completion, capture/read/Save custody and actual fences before explicit
last-owner release. Drop does not release or repair an operation.

The private filesystem domains are `0x46530000` for context, then parent, rebuilt
root and initial count. They are disjoint from the file editor's 1–10 meanings.
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
tracks transitions; it does not scan/count an entire scratch table.

[state/roots.rs](../../crates/layerfs-content/src/filesystem/state/roots.rs)
consumes the sealed parent headers to obtain rebuilt roots, one bounded batch at
a time. It needs no scratch-key scan, resident root map or alternate ordering
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

## Remaining domains and bounds

The parent map's `ordering_bytes / 1024` resident allowance applies only to the
old memory state. The backed parent domain can continue through more bounded
jobs, subject to physical admission. This does not remove every other total-state
refusal. Initial row totals and validation still use the existing ordering-derived
allowances, and the ordinary updated reducer/release paths remain unchanged.

Deferred P13 state includes grouped demands; additions/by-parent/candidate/bound
maps; seen/frontier state; reducer `declared_new`; pending and tiered ordering
runs; complete touched/zero collections; release starting/prefetched/queued state
and its namespace-depth cursor stack. The alias whole-base and rebound subtree
walks remain. Existing `declared_limit`, `walk_limit`, touched and ordering-byte
ceilings remain visible and can still refuse an operation. P4 sparse semantics,
real captured ordered-name/count/tri-state jobs, typed normalization and complete
Commit/Save/history/install composition are separate work.

P14 requires owning qualification of the exact immutable root and actual checked
membership/changed-parent evidence. Neither the local context marker, an input
boolean, a reverse SQL hint, SDK binding nor BaseView's bounded root check
establishes complete reachability, alias uniqueness or acyclicity. No whole-base
scan is hidden in this state constructor, scope acquisition or mount/bind setup.
The existing validator is not bypassed and no alternate graph engine is selected.

These changes pay actual point/guard jobs, raw codec work, extra input passes and
service-return copies. Existing FilesystemUpdateCounters still measure canonical
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
state rather than falling back; and asserts capacity-accounted jobs. The fixture's
`first_keys` panics, so a hidden scratch-domain collection cannot satisfy it.

The fixture is external and resident. It is not SQL/runtime, memory-residency or
performance evidence. The integration owner ran locked no-run first, bounded
bodies and affected host/Linux checks. The checkpoint retains their exact
commands, failures and repairs. Actual filesystem/Save/history/owner-cleanup
composition remains required.
