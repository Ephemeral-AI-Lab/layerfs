# Workspace edit-record adapter

> Status: source implementation following local checkpoint
> `889836c446507c726a53f0ccf1e4418bd4d0946f`; component checks are recorded in the
> [joint checkpoint](../issues/307/K1-BACKED-STREAM-SUPERVISOR-20261007.md).
> This is the owning adapter for the [backed file editor](50-backed-file-edit-state.md),
> not captured normalization, Save/history/known install or S10 completion.

Content defines `IndexedEditBacking` and owns every raw record meaning, codec,
reference count, detached/resolved key and emission acknowledgement. Workspace's
`IndexedEditRecords` binds an explicit engine-minted OperationOwner and full
unsigned file scope to an existing `OverlayScratch` provider. It acquires no
owner, performs no root acquisition, opens no database and releases no operation
on Drop. Captured file/reader/root ownership and actual Save/transport fences
remain with the caller. The caller transfers `EditBackingCustody` explicitly and
releases the engine owner only after those obligations are known complete.

The neutral service boundary belongs in Workspace because Workspace cannot
import Daemon. Direct Overlay and Daemon OwnerClient implement it. Daemon submits
one identical typed Scratch command per method, preserves the original
unattempted command/cause or attempted Completion, and uses the existing fair
owner/credit class. No provider or Content I/O runs under the SQL owner.

| Content operation | Actual local boundary |
| --- | --- |
| contains/get | Exact scoped complete-key membership or one raw value |
| apply | Sorted unique guarded raw changes, all preconditions before effects |
| first keys | One prefix's first64 keys, optionally excluding exactly one root |

The existing Some-exclusion API and query remain. The additive None path has
its own primary-key query with four prefix parameters and32 bound bytes;
it excludes no identity. All-zero and all-ones keys remain representable.
When the live root belongs to another draft domain or is Stored, the Content
engine uses None. No sentinel, OFFSET, whole-set collection or absence probe
stands in for this condition. Plans and VM programs include both paths.

## First original failure and bounded conversion

The adapter retains the first original Workspace error in its custody. Daemon
NotApplied returns its bounded deciding body once and retains the original
credited Completion; the adapter becomes terminal before returning it. The
Content engine then ends construction. Subsequent adapter calls submit no job
and leave that original error untouched. A healthy independent scope still
progresses. A pending wait/disconnect error does not imply a failed or successful
SQL publication; the scope is retained without replay or guessed deletion.

Before converting a Content changes Vec, the adapter sums its actual outer
capacity and both expected/new raw buffer capacities against65,536. A refused
input retains its original Vec before SQL or partial conversion. An accepted
conversion allocates exact needed Overlay descriptors and moves nested byte
buffers. Both descriptor allocations overlap temporarily; they are observed
explicitly instead of claiming a zero-copy complete operation. Daemon admission
also charges the actual converted outer spare capacity and bounded deciding
reply. The direct owned service port checks its outer capacity too; the underlying
borrowed Overlay API keeps its separate caller-owned input scope.

The `OverlayScratch` result carries source-scoped service-return copy data.
OwnerClient clones get/key/NotApplied windows while the original completion is
owned; the original deciding Completion remains held on refusal. Direct Overlay
moves successful SQL values, but clones a deciding value to retain its original
NotApplied outcome. SQL column acquisition remains a separate actual cost.

`EditBackingWork` records provider calls, converted descriptors/allocations,
conversion overlap, returned heap-window copy bytes/allocations/capacity and
calls refused because the adapter is already terminal. Saturation is explicit.
Those values do not measure total editor/SQL/pager/Save/transport/process residency,
whole-operation copies, physical I/O or exact eligible debt. Conversion overlap
contains the incoming nested capacities plus both descriptor allocations; reply
capacity covers only the independently returned copied heap window. Bool/fixed
stack fields, SQL delivery buffers and Content codec copies remain outside those
observations. Root/consumer/capture phase instrumentation is still required.

## Verification scope

Public tests exercise actual OwnerClient jobs, first original NotApplied credit
custody, no later submissions, independent scope progress, all-zero key windows,
pre-admission outer-capacity refusal and stopped-owner original commands. The
real-record integration uses public Content construction/edit/read APIs plus a
real Daemon Overlay provider; its byte oracle independently applies the edits.
Source/consumer objects in that proof are an external memory fixture, so it is
no authenticated runtime, Commit or phase-cache performance qualification.
Scratch persists after adapter Drop and becomes automatically reclaimable only
after explicit last-owner release. Existing schema15/accounting/cleanup scope
remains the [indexed operation provider](49-indexed-operation-scratch.md).

`IndexedConstructionRecords`, `ConstructionBackingCustody` and
`ConstructionBackingWork` are neutral aliases of the same adapter and custody
types. They add no provider or failure owner. Filesystem serial state uses its
own explicit scope and disjoint kinds. The separate
[captured point/run port](55-captured-sparse-run-cursor.md) retains reader/root/floor
custody through existing fair Daemon Read jobs; it still requires the owning
normalizer to retain the first failure and fence all owners before release.
