# Workspace immutable base access

> **Status:** Current general guide.

Implemented source: the S3 checkpoint on local main following `f2a381119`, for
[tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307). This is an
initial active library, not complete Workspace or S3 install/lease qualification.

The replacement [Workspace](../../crates/layerfs-workspace/src/workspace.rs)
binds a checked canonical filesystem root to one overlay route. `BaseView::open`
acquires one root through public `FilesystemRead` and checks allocation scope;
it does not enumerate paths or reconstruct payload. `Workspace::open` then
creates bounded logical state in the already initialized overlay. No special
empty-base branch exists. Runtime root/Branch authority and native mount readiness
remain separate future integration work.

[BaseView](../../crates/layerfs-workspace/src/base.rs) uses current public content
APIs for child/inode lookup, directory listing, portable metadata and symlink
targets. Its file range plan opens public `FileView`, clamps EOF, retains that
immutable file root/classification and streams to a caller sink. The 128 KiB read
window limits one request, not file length. A retained plan remains on its old
root when a new BaseView binds another root; actual overlay install/reader lease
integration is still outstanding. Global cluster one currently has no content GC;
this pointer retention is not a future distributed retention lease.

[CanonicalClient](../../crates/layerfs-workspace/src/client.rs) reauthenticates
returned canonical bytes against exact object IDs, checks demand cardinality and
canonical-object/window limits, and restores demand order. Cache locks are released
before upstream calls and authentication. There is no retry or error substitution.
The upstream public provider must enforce its own authority/allocation contract
before returning a batch; the client cannot undo upstream allocation. Authenticated
runtime framing/admission is S9/P1 work, not established by this library seam.

The [cache](../../crates/layerfs-workspace/src/cache.rs) keys immutable object IDs,
never a mutable path/serial. Bounded standard maps track values and recency; there
is no authoritative namespace mirror. An object larger than the cache allowance
bypasses retention and stays readable. Cache hits check remaining demand allowance
before cloning. Charges include canonical bytes plus 256 bytes of bookkeeping per
entry; this is logical cache admission, not measured RSS or exact allocator bytes.
The source, result window, cache copies and operation-owned FileView have separate
lifetimes and costs. Demand count is <=4096, canonical object <=16 MiB and logical
batch <=32 MiB; continuing operations use further windows.

Root binding performs one object read. Point/list/read cost follows the canonical
trees' visited paths/returned names/demanded extents, with real authentication and
copy work. Cache lookup/recency maintenance is O(log E), where E is admitted cache
entries; total eviction work charges evicted entries and copied bytes. Metadata
currently classifies regular-file roots to derive length; small whole-file roots
can require their payload. P5's cheap runtime length lookup remains open. No cold
latency, native buffer or aggregate resident-bound claim is made.

The external [tests](../../crates/layerfs-workspace/tests/base.rs) build actual
file/attribute/symlink/filesystem objects through public constructors, including
regular-file aliases and `.git`, ignored dependency/cache/output paths. They check
single-object binding, pagination, maximum portable timestamps, sticky directory
mode, EOF, missing-path distinctions, exact cache identity/eviction and a retained
read across a newly constructed root. This is small deterministic correctness
coverage, not the full development fixture or real authenticated Store/runtime.

The previous excluded Workspace implementation is temporarily preserved at
`core/crates/layerfs-workspace-legacy/`, as allowed by the implementation plan.
Its implementation source bytes are unchanged; its package manifest is explicitly
renamed for the temporary reference location. It is excluded and supplies no active dependency,
include or fallback. Its 26,835 production LOC remain in the core subtotal as
explicit relocation. S11 must remove it after replacement coverage; root
`crates/` remains independently retained until qualified S13 retirement.

The same checkpoint corrects the initial overlay metadata schema to version 2:
separate signed seconds plus nanoseconds preserve the canonical timestamp range,
and kind-checked modes preserve directory sticky bits. There is no disposable
database reopen/migration API; startup creates fresh state and reads back schema
identity. Exact nested rollback errors are preserved in uncertain custody.

Remaining: base/overlay merge, mutable operations, install/read leases, orphan/
failure composition, fair service/reclamation, authenticated runtime and canonical
Commit construction, actual mounted daemon/Exec and required qualification.

## Prepared install checkpoint (#307)

The S3 slice after `8e2976e4e` adds a scope-checked prepared next base and a
paired one-attempt engine/Workspace binding transition. `Workspace::base()` now
returns a retained BaseView through WorkspaceResult, releasing the binding lock
before content I/O. Old plans survive actual install; refusal returns original
error plus prepared custody. See [prepared install](27-prepared-base-install.md)
and [S3 exit audit](../issues/307/S3-EXIT-AUDIT.md). Effective namespace merge and
P5 stat/runtime integration remain unfinished; the earlier source pins are unchanged.

## S3 completion reconciliation

The completion after `bdc6ed4af` adds effective source-qualified read/ordered-name
composition, actual paired actor install, original unattempted-command custody and
owning SDK stat lengths. See [effective base view](29-effective-base-view.md) and
[S3 exit audit](../issues/307/S3-EXIT-AUDIT.md). Earlier limitations/evidence above
retain their source scope; native/logical runtime transport, mutable byte semantics
and aggregate resource acceptance remain unfinished.
