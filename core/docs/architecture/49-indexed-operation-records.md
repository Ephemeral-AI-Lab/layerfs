# Indexed operation records

Terminology update2026-10-07: the current API/SQL names below use overlay
schema16. Earlier algorithm and proof pins retain their original scope; see
[the naming checkpoint](../issues/307/PRE-S8-TERMINOLOGY-20261007.md).

> Status: source implementation checkpoint following R4 commit
> `4090cb9a2d5fa1cd899d63c7a44537ff3f23e3cd`; component verification is recorded
> in the [checkpoint](../issues/307/K1-INDEXED-SCRATCH-E01-20261007.md).
> This implements a neutral Overlay/Daemon record provider. The subsequent
> [Content editor](50-backed-file-edit-state.md) and
> [Workspace adapter](52-workspace-edit-backing-port.md) use it. Captured
> namespace normalization now uses these records too
> ([captured namespace construction](78-captured-namespace-construction.md)); sparse Commit and
> S10/P3 remain unfinished.

Overlay owns these records in its existing daemon database. Daemon schedules
short credited OperationRecord jobs. Content owns the meaning of kinds, draft encodings,
checked parent counts, detached membership, resolution, emission and zero proof.
Workspace supplies their owning adapter. No provider opens another database,
uses global acquisition working state or performs a canonical tree algorithm in SQL.

## Complete keys and bounded raw records

The additive public types are `IndexedOperationRecordScope`, `IndexedOperationRecordKey`, `OperationRecordExpectedValue`,
`IndexedOperationRecordChange` and `IndexedOperationRecordApply`. A scope pairs the engine-minted
`OperationOwner` with an opaque full `u64` file scope. Ordinary construction uses
one scope per changed captured inode and one constructor invocation per scope.
Kinds are opaque `u32` values; keys contain all 32 bytes of identity. A value is raw
record bytes, including a valid empty marker. Absence is distinct from emptiness.

Schema15 introduced the indexed record table; schema16 names it `indexed_operation_record`, a STRICT/WITHOUT ROWID table keyed by
`(ns, operation, file_scope BLOB8, kind, key BLOB32)`. The file scope uses unsigned
big-endian encoding, preserving its full range and binary ordering. Namespace and
operation retain their existing engine identity representation. The primary key
supports every current point/window/cleanup predicate; there is no additional
index or hash truncation. Fresh database creation verifies schema16. Existing
paths are refused by the original creation contract; this is no migration/reopen
or crash-recovery interface. Integer-key `operation_record`/`owned_operation_record` APIs remain.

One raw value has a 65,536-byte schema ceiling. This is not a claim that arbitrary
canonical objects, including WholeFiles, fit the provider. A caller needing
larger raw values must select an explicit sharded/streaming representation.
The mapping-only draft codec and construction algorithm remain Content's scope.

## Exact points and guarded atomic mutation

`indexed_operation_record_contains` projects only 1, so membership does not load a draft
BLOB. `indexed_operation_record_get` returns at most one bounded value. Each checks the
actual operation token, including engine/namespace incarnation. Existing custody
remains readable after logical Workspace close; a stale/released owner refuses.

`indexed_operation_record_apply` accepts strictly ordered unique `(kind,key)` targets.
Each expects either Missing or ExactBytes and supplies an optional new value:
Some writes a BLOB, None deletes. The caller aggregates repeated child
multiplicity and computes/serializes every transition. Overlay interprets none
of those bytes. Duplicate or out-of-order targets and capacity overflow refuse
before SQL effects.

`indexed_operation_record_changes_bytes` checks fixed change descriptors plus retained expected
and new Vec capacities against 65,536. Daemon also includes unused capacity of
the owning outer changes Vec. This is a per-job limit; many bounded jobs can
address operation state larger than 8 MiB. There is no 64-change limit: a grammar-
sized 128-child mapping may require 258 changes. Callers should reserve the exact
needed vector capacity, since doubling to 512 descriptors can waste the window.
The public regression reports the actual compiled descriptor layout against a
conservative pair of 12,460-byte draft buffers, 128 pre/post u64 values and 258
descriptors. That envelope does not qualify the separate Content codec.

The existing short atomic transaction checks live Workspace and operation
ownership, then checks all exact preconditions before any row mutation. It reads
and drops each deciding value rather than retaining a precondition vector.
The first mismatch returns original `NotApplied { index, key, actual }`, with no
changed record. Only then may the complete batch execute exact-key UPSERT/DELETE.
Commit/rollback/quarantine preserve the engine's original error and uncertainty
rules. A NotApplied result ends the construction operation; it is no refetch,
CAS retry, resend or failed-operation replay protocol.

## Detached windows and lifetime cleanup

`indexed_operation_record_first_keys` returns only the first 64 full keys of one
namespace/operation/file/kind prefix, excluding exactly one retained root. Its
ordered primary-key walk can skip at most that one excluded row. It has no after
cursor or OFFSET. Consumption can atomically remove a candidate and add children
below its key; the next window starts from the first eligible key again.

The additive `indexed_operation_record_keys` path takes an optional exclusion. Some uses
the same five-parameter query and excludes only the exact key; None uses a
four-parameter prefix query with 32 bound bytes and excludes no key. Both project
at most 64 keys/2,048 bytes. Content uses None when the retained root is Stored or
belongs to another draft domain. No all-zero key sentinel or digest partition
stands in for that fact. Diagnostics retain both actual EQP and VM programs.

Exact `release_operation` removes the original nonrecycled owner and atomically
enqueues both the existing owned integer OperationRecord target and a separate indexed
OperationRecord target. No Drop or process exit guesses release. Caller-side construction,
Save and transport ownership must actually end before release; unknown outcomes
retain custody under the owning phase protocol.

Live indexed cleanup selects at most 64 `(file_scope,kind,key,length(value))`
metadata rows for the released operation. Terminal namespace cleanup uses the
same primary-key prefix and includes operation in its ordering. Neither loads
value BLOBs. A turn deletes at most 64 rows and 65,536 declared value bytes with
exact full keys, then restarts from the first remaining key. New owners cannot
reuse the released identity, and cleanup cannot delete their records. The
existing fair maintenance lane continues during live activity and idle periods.

Insert/update/delete triggers maintain existing aggregate `operation_record_rows` and
`operation_record_bytes` at namespace and daemon scopes. Last-owner cleanup decreases
them only as physical rows are deleted. Shared file/pages/freelist/reservation
and debt observations remain separately attributed; logical value bytes do not
include index/pager/journal/OS cache residency and do not imply file shrinkage.

## Daemon jobs and original result custody

`Command::IndexedOperationRecord` carries `IndexedOperationRecordJob::{Contains,Get,Apply,
FirstKeys,FirstKeysAll,KeysAfter}`. `Response::IndexedOperationRecord` contains the exact typed reply. These
share the existing OperationRecord service class and fair namespace rotation. Each job
performs bounded SQL only, releasing the owner before Content/provider work.

Admission charges actual input capacities, fixed descriptors and the maximum
bounded result: 65,536 for get or deciding apply mismatch, 2048 for 64 keys. Refusal
returns the original command before effects. An attempted error or NotApplied
stays in the original credited `Completion`; retaining it retains service
credit. A healthy independent owner job may progress while that result is held.
Workspace's adapter must retain the first exact completion/unattempted command
and operation token beneath Content's narrower error and stop further requests.
Successful caller copies/windows are bounded but still pay their actual work.

## Verification and evidence limits

External Overlay tests cover complete-key/scope isolation, point projections,
atomic guarded refusal/update/delete, structural and capacity refusals, 258-change
admission, lower-key detached insertion, state above 8 MiB, nonrecycled owners,
read-after-close/write refusal and bounded live/terminal deletion. Daemon tests
cover original NotApplied/held credits, unrelated progress, wrong-route refusal,
outer retained capacity refusal, real jobs and automatic last-owner cleanup.

`explain_indexed_operation_record` uses the actual production templates. It records EQP
for point membership/get/delete, detached exclusion and operation/namespace
cleanup, plus full UPSERT/delete/keys VM programs including accounting triggers.
Its human-readable output is an operator diagnostic, never a production parser.
`examples/indexed_operation_record_profile.rs` creates one caller-selected fresh artifact,
prints those plans and actual scoped execution/transaction/resource observations,
and leaves the artifact in caller custody. It has no timing/cold/speed claim.

The correlated runtime observations use existing `DatabaseWork`: executions,
VM steps, returned rows/value/BLOB bytes, bound bytes, direct/trigger changes,
full-scan steps, sorts, autoindex and reprepare counts. Daemon's original
Completion has its exclusive job work and separate automatic maintenance work.
Indexed visited-row counts are not currently directly observed. Full-scan
counters are not a substitute; correlate the exact loop and actual VM work and
state the unavailable counter. Inclusive statement wall is not exclusive CPU.

Public count-driven tests vary unrelated populations while keeping target rows
and requested windows fixed. No campaign or qualifying cold measurement follows
from this source. Content aggregate residency, actual sparse/captured editing,
runtime Save/transport ownership, held cleanup debt and complete Commit speed/
storage qualification remain separate required work.

## Non-destructive complete-key cursor

The additive `indexed_operation_record_keys_after(scope, kind, after)` projects at most
64 complete keys without changing any record. None uses the existing exact
no-exclusion query and includes the all-zero identity. Some adds the full BLOB32
`key > boundary` predicate to the same namespace/operation/file-scope/kind prefix,
with `ORDER BY key LIMIT 64`; an absent boundary needs no point probe. The exact
production query is included in both EQP and VM diagnostics.

This cursor serves sealed membership passes. A newly inserted lower key cannot
appear behind an already consumed boundary; the owning constructor must complete
its sealed pass before such insertions and restart from None for the later phase.
Detached/release work queues retain their separate first-key traversal because
they consume records and may admit new work. These meanings are not interchangeable.

The neutral construction port and Workspace adapter forward this as `keys_after`
and `operation_record_keys_after`. Older providers return an explicit unavailable-capability
error; there is no whole-set collection, destructive-query substitution or zero
sentinel. The first original error remains terminal in the existing adapter.
Daemon charges the fixed typed input plus at most 2,048 reply bytes, copies the
successful key window while its Completion is held, and preserves original
attempted/unattempted custody on refusal. No owner acquisition/release or schema
change is introduced.

External cursor cases correlate full-prefix range EQP/VM with actual returned-key
bytes and scoped SQL work, preserving all records across several windows. Root/
lease work, indexed visits, SQL pages/overflow/cache and physical I/O remain
separate costs; these functional fixtures provide no numerical qualification.
