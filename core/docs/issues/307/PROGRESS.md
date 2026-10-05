# Cluster-two implementation progress

> **Status:** Current planning checklist; no release candidate exists.

Owner: [tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307).
Design baseline: `c9861bc878583822a468e78dbc0f3740eacbecbe` on local main.
The [implementation plan](../303/07-implementation-validation.md) owns milestone
dependencies and exits. S1 and S2 are complete with their exit audits and covering evidence. S0 and S3–S13 remain
unchecked until their complete implementation and required evidence exist.

## Current checkpoint

Initial overlay/prerequisite checkpoint `f2a381119` is committed locally, with
all required host checks passing. S3 now has initial public content-built root
binding/read APIs and immutable caching; its dormant predecessor is temporarily
relocated intact. See [base architecture](../../architecture/20-workspace-base.md).
The immutable base remains partial implementation; S1 engine closure is recorded below.

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
state roots. Paired actual locator EXPLAIN/runtime counters are retained. Base/stat
and logical transport integration remain open; see [file lengths](../../architecture/24-file-lengths.md).

Terminal close now revokes new entry, retains exact owner/capture/reply custody
and maintains a ready-only cleanup index. Daemon maintenance rotates short deletion
windows automatically while idle and after finite foreground service. Native
unmount, live-generation/orphan/failure/pressure and aggregate qualification remain
open; see [terminal reclaim](../../architecture/25-terminal-reclaim.md).

## Completed milestones: S1 and S2; next completion target: S3

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
retention against actual engine install. Effective point/name/metadata merge and
P5 stat/runtime integration remain required. S3 is still unchecked.

## Concrete next work

The initial active overlay now provides typed namespace/metadata/cell/scratch/
ownership primitives and publication/reply-attempt/capture routing. Paired EXPLAIN
and actual statement profiles caught and corrected a residual-generation scan.
See [implemented architecture](../../architecture/19-daemon-overlay.md).
The remaining list below includes initial interfaces already established; complete
S0 and later acceptance require the remaining service/lifetime/resource work.

1. Audit and implement S3 effective base-overlay merge and retained-root integration.
   Existing real-root reads and S2 engine interfaces satisfy its usable dependencies;
   prove each remaining exit before starting the S4 namespace completion target.
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
untracked `docs/general/sandbox-cache-design.md` are also preserved without staging. Root `crates/` remains the reference until
S12 qualification permits S13. Checkpoint hashes are local-only; no remote push,
release or deployment is authorized. Append-only tracker texts are retained in
[checkpoints](checkpoints/initial.md). Rust checks/LOC/evidence are recorded in
each checkpoint commit and its tracker comment. S1 and S2 are complete; later
engine, native/runtime and integrated qualification exits remain explicit.
