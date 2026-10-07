# Serving-scope completion and exact terminal custody

> **Status:** Historical implementation record. The host-mediated transport
> described here was retired by F12 after `52e1f2e18`; use
> [the current retirement/Init guide](66-host-transport-retirement.md).
> Original source links below refer to their recorded Git revisions, not the
> current tree. Historical measurements and verdicts are unchanged.

The SDK's custody owner (`core/crates/layerfs-api/sdk/src/runtime/custody/owner.rs` at local Git `52e1f2e18`)
adds one explicit consuming `Sessions::fence`. It ends the host-local borrowed serving
scope and returns `ServingCustody`, containing every original configured slot in
order, including vacant positions. It does not open a provider, call SaveFinish,
stage/publish/discard history, delete objects, refresh a Branch or infer publication.
The initialized Runtime remains the application owner of its existing Store handles.

## Admission and one consuming fence

The service-owner gauge must be zero before the operation begins. A live Service
holds ownership; its queued/dispatched jobs and caller-held ServiceCompletions keep
their exact credits. Rust borrowing prevents consumption while a Service still
borrows Sessions, and the runtime gauge refuses consumption after that Service has
dropped if caller-held original completions remain. `ScopeFenceRefusal` returns the
original Sessions intact with the exact owner count. No slot or producer changes.

The fixed report Vec is allocated before moving/dropping any slot. An allocation
refusal returns the original allocator error, requested slot count and original
Sessions. A caller can take a new explicit action after ownership/admission changes;
there is no automatic busy loop or replay of provider work. A successful call consumes
the Sessions value once, so the same serving scope cannot be fenced again.

For each occupied slot, the fence explicitly drops any remaining Save producer and
moves its original Binding, SaveId, optional Completion and HistoryReceipts. It copies
only the typed capability's fixed fields; it does not clone original errors/results.
SaveId's existing `token()` retains the original runtime/slot/serial wire identity.
Binding keeps the captured peer, Workspace, Branch/root, scope/profile, catalog and
runtime continuity context. There is no borrowed Storage/Save in the returned report.

Existing Save Drop invalidates its bounded candidate/pool state and cached read/
signature windows and preserves unused-tail bookkeeping. It does not finish the
producer or delete acknowledged publication waves. Pending/unpublished producer data
can end locally, while earlier acknowledged immutable objects remain in the Store.
The fence synthesizes no Abort/Finish receipt; absence of Completion remains absence
of a recorded terminal attempt, not proof that no global objects were published.

## Disposition and exact retained results

Typed knowledge (`core/crates/layerfs-api/sdk/src/runtime/custody/knowledge.rs` at local Git `52e1f2e18`)
uses owning error variants and predicates rather than Display strings or provider
reads. Direct UnknownOutcome and unknown original/cleanup causes nested inside
CleanupFailed remain TerminalUnknown. CleanupFailed with known causes is a distinct
RetainedFailure. History uses `HistoryError::unknown()`, including WithStage's
original cause and exact deciding-transaction stage disposition.

| Slot disposition | Exact local meaning |
| --- | --- |
| ActiveProducerEnded | No terminal Completion was recorded; the fence dropped the remaining producer. Earlier acknowledged waves may exist |
| KnownTerminal | Original terminal result is known. It may still own an acknowledged stage/token |
| RetainedFailure | Original known failure has incomplete cleanup; original and cleanup causes remain distinct |
| TerminalUnknown | An exact original Save/history operation or cleanup suboperation has unknown persistence outcome |

SlotCustody separately exposes Save failure knowledge and recorded history
uncertainty. Unknown Save before staging does not become unknown Branch publication.
Completion preserves its original Accept/Finish/Abort phase and complete typed
outcome. HistoryReceipts preserves independent optional Stage/Commit/Discard results,
captured expectations and tokens. An absent history receipt means no recorded attempt
by that slot; it is not a fresh global catalog observation.

`retains_custody()` preserves the existing owning release gate: unknown outcomes,
failed cleanup and acknowledged stages requiring exact disposition stay retained.
KnownTerminal is not permission to release/discard a stage, start another Commit or
perform ordinary terminal unmount. Application orchestration must retain these
records and apply the owning admission/teardown policy. `into_parts`/`into_slots`
move the original records to that application's chosen owner and perform no provider
action. This API supplies no resolver and cannot manufacture fresh authority.

## Connection loss and process restart are different boundaries

[R1 attachment fencing](45-runtime-supervision.md) cancels queued provider work,
retains original dispatched results and joins both native I/O workers. A disconnected
or restarted consumer does not end the host Sessions or its Saves/history receipts.
The host may still know the exact original result even when that consumer's call
only knows that its reply was lost. No request is resent or Branch refreshed.

`Sessions::fence` is the provider-serving-scope boundary after service ownership has
been released. It does not replace R1's native worker joins or establish process/
kernel/Workspace teardown. Input/output reports and their independent transport
credits remain with their respective owners. Moving known/unknown slot custody to
the host application is an explicit local ownership transfer, not crash recovery.

New Sessions from the same Runtime continue burning capability serials, so an old
SaveId cannot acquire a freshly admitted slot. An authority-assigned fresh Runtime
incarnation rejects old Bindings and SaveIds even when catalog, peer, Workspace and
captured root facts match. The application must supply a fresh nonzero incarnation;
the SDK does not invent, reuse or refresh it.

A host-process crash loses this in-memory serving registry, original attempt
knowledge and application-owned reports unless another explicitly supplied owner
persisted them with a defined protocol. Durable Store history alone does not persist
those session/attempt records. Disposable Store gains no crash guarantee. This
checkpoint adds no persisted session schema, receipt journal, automatic reopen,
resend, re-stage, rollback, guessed deletion or unknown resolver. P10 remains exact
terminal-unknown custody; any richer recovery policy stays an explicit prerequisite.

## Cost and verification limits

For S configured processing slots, the new report owns O(S) fixed positions and
moves each occupied record once. Existing bounded producer/cache buffers also pay
their actual Drop work; this is not an O(S)-only foreground-cost claim. With E owned
error nodes and nesting depth D, typed failure classification visits relevant nodes
in O(E) and uses O(D) call stack, following existing typed nested errors. It adds no
namespace walk, payload copy, new SQL statement/schema, database creation or immutable
construction. Original Binding names/errors/receipts retain their existing allocations;
the report Vec is an additional allocation chosen before effects.

The external custody tests (`core/crates/layerfs-api/sdk/tests/custody.rs` at local Git `52e1f2e18`) cover
original fixed slots, active/finished/aborted producers, acknowledged-stage retention,
cancelled and known dispatched completions preventing a pre-effect fence, stale
capabilities across scope/runtime epochs and an already acknowledged publication
wave surviving active-producer end. Portable tests inspect exact public typed errors
for direct/nested unknown and known cleanup classification. They do not inject a
publisher failure into the product path and establish no real unknown-publication
or process-crash recovery proof. Real Store cases are macOS-only; fresh-epoch cases
use equal catalog/root context and prove identity fencing, not Store restart recovery.

Scoped validation receipts own actual check results. No cold-cache, physical I/O,
phase residency, sustained service or performance gate follows from these functional
cases or source counters. Full contextual admission, real integrated consumers,
native execution, S7/E/Q resource evidence and S10 Commit/recovery obligations remain
separate acceptance work.
