# Cluster-two implementation progress

> **Status:** Current planning checklist; no release candidate exists.

Owner: [tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Design baseline: `c9861bc878583822a468e78dbc0f3740eacbecbe` on local main.
The [implementation plan](../303/07-implementation-validation.md) owns milestone
dependencies and exits. S1–S5 are complete with their exit audits and covering evidence. S0 and S6–S13 remain
unchecked until their complete implementation and required evidence exist.

## Current checkpoint

S5 completion after `f5558fc22`: bounded cells/tails/validity, atomic append and
overwrite, cutoff truncate/regrow, composed reads and canonical zero-run reuse.
See [S5 exit audit](S5-EXIT-AUDIT.md) and retained checks/failures. The original
hang was a test width error (23-byte records expected as 24), followed by an
unbounded tail wait after writer panic. Its correction and panic-safe bounded
proof pass on macOS/Linux. The [S5–S6 handoff](HANDOFF-S5-S6.md) retains the
earlier paused snapshot, not current verification status. P3 is explicitly
carried to backed S10 editing; it is not resolved. S6 lifetimes/reclamation,
failure composition and physical headroom are the next required milestone.

S6 is now in progress: the initial bounded failure-composition/live-maintenance
slice is verified on macOS and Linux. [Live composition](../../architecture/32-live-composition.md)
records its scope; independent orphans, minted file/capture custody, whiteouts
and physical reservations/headroom remain required. The owner subsequently
requested responsibility folders across active core packages. Complete the
coherent composition checkpoint, apply that relocation in a separate checkpoint,
then continue the original S6 exits. No S6 completion or later milestone is
claimed by either checkpoint.

The owner-selected [source organization](SOURCE-ORGANIZATION.md) now has its
[separate implementation receipt](SOURCE-ORGANIZATION-RECEIPT.md). Current source
links use responsibility folders; historical receipts retain their original
paths/pins. S6 additions follow the lifetime/maintenance/database boundaries.

S4 is complete at the local milestone-completion commit after `8d691ab8a`: atomic
ordinary namespace operations as compound owner jobs, maintained directory
counts, verified rename ancestry and owning serial ranges. See the
[S4 exit audit](S4-EXIT-AUDIT.md) and
[namespace operations](../../architecture/30-namespace-operations.md). The next
ready work is now S6 lifetimes and reclamation.

S3 is complete at local-only `c4b49a121aec15a6eae58c04d8074fd9eb2772db`, tree
`67a6b34e5b135ea5d30c48fa5d0bd977c4aa1356`, with its
[tracker completion receipt](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307#issuecomment-5994947845).
The standalone [S4–S6 handoff](HANDOFF-S4-S6.md) records the interfaces, reusable
checks and preserved state the S4–S6 group started from. The dormant predecessor
remains temporarily relocated and counted.
See [base architecture](../../architecture/20-workspace-base.md).

S2 now has a built daemon SQL owner with fair namespaces/classes, finite capture
ordering, lifecycle credits, retained-result accounting and event-driven parked
readiness. Schema v3 install advances an indexed floor and preserves later active
state; captured names use fixed keyset pages. The dormant daemon is temporarily
relocated intact and remains counted. Physical reclamation/native integration and
full reader/orphan/failure composition remain open.

S0/S8 now has a source/build-proven external dependency gate:
[FUSE timestamp blocker](FUSE-TIME-BLOCKER-20261005.md). The published fuser pin
corrupts negative fractional time and panics before the callback at signed minimum.
The owner reaffirmed the third-party restriction; no source exception was
authorized. Keep the published pin while awaiting an allowed corrected release.
Independent ready engine/runtime/prerequisite work continues.

P1 now has public content semantic admission for all thirteen canonical roles:
hash/domain verification, owning grammar decoders, Store-policy whole-file bounds,
expected root scope and references derived from bytes. Real macOS Save rejects
omitted inode dependencies and reconstructs an admitted complete root. This is
local grammar/closure prerequisite evidence; authenticated adapters, contextual
closure and authority remain open.

The SDK is now an active host runtime slice over already-open Handles. Initialized
independent Storage owners support scoped interleaved Saves and saved/pending
demand, authority-bound local capabilities and retained one-attempt completions.
Its old Server-based facade is preserved in excluded `layerfs-sdk-legacy` and
counted. Transport authentication/fair dispatch/history/disconnect fences, complete
root import and aggregate/runtime qualification remain unfinished. See
[SDK runtime architecture](../../architecture/22-sdk-runtime.md).

Bridge is active for initial pinned native KK channels, with fixed borrowed
records, independent direction ownership and shared one-attempt quarantine. SDK
bind requires its completed-handshake VerifiedPeer. Old protocol source is preserved
in excluded `layerfs-bridge-legacy` and counted. Logical codecs, delivery/fair
dispatch, multiplexing and disconnect/restart integration remain required; native
channel proofs do not complete S0/S9.

S0 in progress. Public-API proof in
[interleaved_saves.rs](../../../crates/layerfs-persistence/tests/interleaved_saves.rs)
demonstrates an initialized provider with independently initialized Storage
handles, a stack-owned registry of borrowed Saves, same-Save pending reads,
interleaved finish and demand read, abort, slot reuse and exact CAS reuse.
No lifetime extension, self-referential owner or whole-Commit provider lock is
needed. This establishes a usable lifetime topology, not an authenticated runtime.

The [Linux platform proof](../../../benchmark/cluster2-platform/README.md)
compiled content, bundled rusqlite and fuser 0.18.0 with Rust 1.85.1 on ARM64.
Actual native FUSE mount, file read, detach and worker join passed with `/dev/fuse`
and `CAP_SYS_ADMIN`. This is prerequisite evidence, not product FUSE qualification.

P5 has an owning saved-file metadata length API and authorized SDK delivery.
Whole-file facts avoid payload reads; chunked/empty facts authenticate only small
state roots. Paired actual locator EXPLAIN/runtime counters are retained. Owning Base/stat integration is complete; logical transport integration remains open; see [file lengths](../../architecture/24-file-lengths.md).

Terminal close now revokes new entry, retains exact owner/capture/reply custody
and maintains a ready-only cleanup index. Daemon maintenance rotates short deletion
windows automatically while idle and after finite foreground service. Native
unmount, live-generation/orphan/failure/pressure and aggregate qualification remain
open; see [terminal reclaim](../../architecture/25-terminal-reclaim.md).

## Completed milestones: S1–S4

[S1 exit audit](S1-EXIT-AUDIT.md) maps the engine exits to actual source/evidence.
The current closure slice removes the arbitrary default total-page quota and
adds missing paired payload/name/scratch/lease access evidence. All required S1
covering checks pass on macOS and Linux ARM64; the local milestone-completion
commit records the exact source/LOC identity. The native
fuser gate is scoped to its owning acceptance, and does not obscure engine exits.

[S2 exit audit](S2-EXIT-AUDIT.md) reconciles the generation/service exits. Schema5
retains frozen capture revision and exact ticket generation; lost internal results
leave backed custody and release resident credits. Engine identity prevents cross-
owner routing with matching local keys. Closed captures keep immutable cell access.
Final host565/Linux21 tests and all prescribed core checks pass. Live-generation/
orphan/normal failed-Commit resolution, pressure and native ownership remain later gates.

[S3 exit audit](S3-EXIT-AUDIT.md) now records prepared known install and old-root
retention against actual engine install. Effective point/name/metadata merge, actual source-bound actor install and
owning SDK stat lengths now pass their required evidence. Exact short base-source leases and
finite install readiness now provide the source lifetime prerequisite; see
[source windows](../../architecture/28-base-source-windows.md). S3 is complete; mutable/kernel/resource obligations remain separate.

## Concrete next work

The initial active overlay now provides typed namespace/metadata/cell/scratch/
ownership primitives and publication/reply-attempt/capture routing. Paired EXPLAIN
and actual statement profiles caught and corrected a residual-generation scan.
See [implemented architecture](../../architecture/19-daemon-overlay.md).
The remaining list below includes initial interfaces already established; complete
S0 and later acceptance require the remaining service/lifetime/resource work.

1. S4 is complete. The S4–S6 group continues with S5: derive and record the
   fragmentation/cutoff/hole algorithm and its costs, then implement cells,
   tails, validity, inherited reads, truncate/regrow and holes.
2. Close S0 algorithm gates for truncate/regrow, repeated failed capture and
   orphan composition, physical pressure and fair service; specify all R1–R8
   adversarial bounds and P1–P14 implementation ownership. These remain open,
   so prerequisite smoke proofs do not complete S0.
3. Extend the engine with physical pressure, automatic bounded reclamation and exact
   reader/orphan custody; retain paired SQL plans/profiles and fair short jobs.
4. Extend S3/S4/S5 from the established immutable base and mutable engine APIs
   to effective namespace/payload semantics with bounded streaming windows.
5. Progress the embedded runtime and canonical backed-construction/import
   prerequisites as their public boundaries become ready.

## Preserved state and publication

Untracked `core/docs/issues/301/`, the existing extent-normalization research and
`output/` are unrelated and preserved. Concurrent `docs/README.md` edits and
untracked `docs/general/sandbox-cache-design.md`,
`docs/general/workspace-filesystem-view.md` and
`docs/general/workspace-queue-scheduling.md` are also preserved without staging.
Root `crates/` remains the reference until
S12 qualification permits S13. Checkpoint hashes are local-only; no remote push,
release or deployment is authorized. Append-only tracker texts are retained in
[checkpoints](checkpoints/initial.md). Rust checks/LOC/evidence are recorded in
each checkpoint commit and its tracker comment. S1–S3 are complete; later
engine, native/runtime and integrated qualification exits remain explicit.
