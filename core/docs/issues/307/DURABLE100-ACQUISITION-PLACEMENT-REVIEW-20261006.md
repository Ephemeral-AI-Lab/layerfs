# Durable100 acquisition-placement review

> **Status:** Research and contract review, owner-selected alongside current VFS
> tracing. No placement/profile/API change is implemented or qualified here.

The current host-global placement is deliberate. [A1](A1-ACQUISITION-CONTRACT.md)
selects three operation-scoped tables in the existing global Store, its one
Session, no per-Init open/attach/database, and one short transaction per port unit.
Section6 distinguishes daemon Workspace/Commit scratch, which belongs in the
daemon's single local overlay database. The generic acquisition port does not
make those two ownership domains interchangeable.

## What the current contract buys and costs

[AcquisitionProvider](../../../crates/layerfs-persistence/src/storage/acquisition/provider.rs)
shares the Session with Storage and history. Its first known `begin` fixes the
session epoch, every unit fences its owner before touching rows, and uncertain
persistence quarantines the shared Session. Later epochs list earlier operations
as abandoned; nothing adopts/resumes/deletes them automatically. An explicit host
fencer must authorize abandoned cleanup. Exact logical charges, source placement
checks, typed one-attempt failures and bounded indexed windows apply.

The [public port](../../../crates/layerfs-storage/src/port/acquisition/contract.rs)
is neutral and supplies no SQL/connection to Project. Its implementation is not
an in-memory tree or custom sorter. The Store's selected persistence profile
covers these tables along with immutable pack and history metadata. Under Durable,
each successful write unit synchronizes its commit; WAL later writes pages into
the main file. A1 explicitly predicted this added cost over the retired
unsynchronized run scratch.

For the retained Durable100 path there are eight acquisition write commits:
begin, three entry windows, complete-files, directory-roots, discard and release.
That count does not mean eight times all input bytes; per-port VFS tracing is
needed for actual submitted page bytes and sync work. The earlier after-tail
diagnostic assigns9,158,710 ns inclusive COMMIT to those units. The qualified
whole-product deficit is37,434,583.3 ns. These observations have different
instrumentation/cache scopes and cannot be subtracted as an exact performance
model; they suggest placement alone should not be assumed sufficient.

Fresh Store creation remains part of the selected Durable100 clock. Application
reuse of an initialized global owner is a real deployment pattern, but cannot
remove bootstrap from this frozen fresh-Store case or credit setup warmth.

## Alternatives and required boundaries

| Alternative | Effect and obligations | Fit with current Durable100 |
| --- | --- | --- |
| Keep global Store; reduce actual repeated dirty-page/publication work | Preserve the current owner/profile/schema/port; show SQL plans, runtime counters and per-publication WAL/main/sync/checkpoint work before changing code. | Fits if every current contract and bound remains. This is the next trace target. |
| Add bounded combined lifecycle units in the current Store | Could combine begin/root or last discard/release acknowledgement, but needs explicit atomic output/error/owner custody and ordinary production callers; no transaction across construction/I/O. | A public port extension requires design/compatibility review. It is not implemented or an assumed parity fix. |
| Reuse a host operational SQLite owner separately from the global Store | Could give acquisition working rows a distinct crash/lifetime contract. Requires startup ownership, monotonic identities, fencing, uncertainty, exact charges, cleanup and capacity rules; include all backing/build/creation/allocation costs. | Supersedes A1's same-Session decision and the selected provider. Requires an explicit new contract and qualification; cannot silently relabel the old Durable100 gate. |
| Put acquisition into the existing daemon overlay | Aligns with daemon-owned source acquisition only after source location, host/daemon authority, adapter protocol and restart/fencing ownership are specified. Preserve one preinitialized local DB and indexed/bounded rows. | Current acquisition is host-side; this is new integration/topology work, not a configuration switch. The host-local macOS provider must not be copied into Docker. |
| Turn sync off for acquisition in the current Session | Changes a Store-wide connection setting and may weaken neighbouring Storage/history acknowledgements. The current schema/provider promises the selected Durable profile. | Does not fit. No per-table durability fallback or unreported profile change. |
| TEMP-memory whole-root state or one import-sized transaction | Loses indexed/backed resident bounds or holds the writer across source I/O/construction, blocking unrelated bounded work and changing failure/publication semantics. | Does not fit. |
| Restore run scratch as a selectable fast path | Revives a second acquisition algorithm and its old lifetime/ordering custody, against current A1/A3 direction. | Does not fit. |

An operational provider would still have to retain exact uncertainty rather than
guess success, replay a failed unit or delete custody on process/command exit.
Disposable backing does not promise survival of a host crash: loss must be
explicitly fenced and reconciled against durable history/object publications.
Filesystem history is not a process/descriptor checkpoint. No change may create
a new per-tool-call or per-Workspace database or weaken the existing daemon's
readiness, fair jobs and last-owner reclamation contract.

## Recommended next decision

Keep the current Durable100 topology/profile fixed for the selected VFS trace.
Rank actual redundant page writes and checkpoint/sync work separately from the
eight acknowledged acquisition units. Use the architecture review to describe a
possible future operational acquisition owner only if trace evidence and the
host/daemon source-owner design justify it. Any such owner is an explicit A1
supersession with its own qualification, not a hidden way to pass this gate.

The owner's reply was **both**: trace current writes/syncs and review placement.
This document completes the placement contract review; it authorizes no durability
relaxation, extra database, provider fallback or public API change. S7/S9 owner
assembly, R3 fencing, greater-than4-GiB proof and resource/rate closure remain open.
