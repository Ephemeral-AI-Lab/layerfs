# Dispatched R2–R5 deepest-file plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Input4236225ee; all changes below remain proposed until separately recorded.

## R2 dependency and component checkpoints

| Owning files | Change and original custody | First verification |
| --- | --- | --- |
| `core/vendor/fuser-0.18.0/src/session.rs`, a focused lifecycle module and public reexports, only if expressly authorized | Add observable receiver entry/exit and retain every created join on failed startup/panic. Keep explicit first-party mount/detach authority. Current timestamp-only exception forbids this change | Public lifecycle tests for all loops, exit before Ready, partial startup and exact joins; then FP-1/21 on Linux |
| `layerfs-daemon/src/service/completion.rs` | Event notification of original completion/lost publisher; registration before/after publication; consume result once, retain credit until disposal; synchronous wait remains | Public Owner/Pending tests with bounded waits, including registration races and held Completion |
| `layerfs-daemon/src/overlay/{credits,queue,owner}.rs` | Bounded admission notifications, atomic registration-versus-release/stop ordering, exact returned unattempted command. Notify outside SQL/queue locks | Saturated admission, release before/after waiter registration, terminal wakeup, no replay |
| `layerfs-daemon/src/store/{open,ports,bind}.rs`, conditional `read_service.rs` | Idle healthy fixed-reader admission, per-operation original errors; snapshot from a read session; no provider lock across parks | Public read/health/failure-scope proofs; native FP-27 later |
| `layerfs-workspace/src/workspace/view.rs`, focused `operations/read_plan.rs` and existing mutation driver | Reusable fact/Need plans producing one consistent decision; original source/processing custody remains in the plan | Concurrent mutation/read observation and exact source-root retention |
| `layerfs-overlay/src/namespace/read_compound.rs`, `lifetime/{lookup,native_group,native_lookup}.rs`, affected schema/runtime SQL | Positive answer/acquisition in the deciding transaction; checked mount/namespace/serial count changes; bounded revoked-group retirement; independent opens/readers | Foreign incarnation, underflow, lost reply, retained removed inode, bounded retirement; EXPLAIN plus runtime counts where claimed |
| Existing Overlay operation records/lifetimes; `layerfs-fuse/src/operations/directory.rs` | Indexed directory handle/cookie to full name boundaries; consume only accepted entries; live last-owner reclamation | Old offsets, partial replies, whiteout-only pages, release before terminal unmount |
| `layerfs-fuse` predecessor path and `core/Cargo.toml` | Preserve predecessor by a separately accounted relocation before real replacement activation | Byte-identical relocation, exact classification delta0; no early retirement |
| Replacement Fuse `mount/{config,profile,syscalls}.rs`, `session/{startup,readiness,state,drain}.rs` | Safe explicit fd/mount/detach; selected profile; all-loop serving and failure custody | Actual mountinfo/negotiation/protected-control evidence; no helper/Drop/lazy fallback |
| Fuse `request/{callbacks,decode,types,reply}.rs`, `dispatch/{admission,queue,workers,pending,completion}.rs`, `ports.rs` | R16+N2 before-copy admission; one K=read_handles+2 pool across mounts; event-driven original continuations and one explicit reply/disposal | FP-8/34 scope; no worker/receiver prerequisite waits or future-request dependency |
| Fuse `operations/{lookup,attributes,directory,open,read,readlink}.rs`, `attributes.rs` | Current Workspace semantics, exact native owners, root1/stable serial mapping, checked attributes/time | Complete declared-root namespace/bytes/metadata oracle, FP-3/4/17/31 |
| Daemon `application/filesystem.rs`, `service/filesystem_port.rs`, `control/{registry,operations,status,attach,unmount}.rs` | Assemble Fuse and narrow asynchronous ports; compose aggregate Ready/drain in the sole registry | Busy probe remains usable; namespace consumers prevent Close; retained original failure survives later observation |
| Bridge `control_{types,native,request,reply}.rs`; SDK `control/connection.rs`, `workspace/{mount,types,api}.rs` | Add Attach/Locate/Ready and bounded native observations; retain bind token on lost Attach | Protocol/public SDK tests, lost original replies without duplicate binding/attachment |
| Sandbox `backend/docker/{container,container_types,endpoint,topology}.rs` | Selected FUSE device/SYS_ADMIN and mount security profile; explicit nonroot ordinary execution/NNP and protected aliases | Reprove actual powers/visibility; old R1 access receipts do not qualify new powers |

Relative crate paths above are under `core/crates/`. Keep each lib.rs/mod.rs
declarative≤200 physical lines and other new production files≤999. Add files
only for implemented responsibilities, not to reproduce every proposed filename.

## R3

Fuse operation/coherence files drive existing Workspace mutation fact rounds.
Entry-producing mutation jobs acquire lookup custody atomically. After the one
reply attempt, release the original publication ticket. Preserve cached-I/O
WRITE_CACHE semantics, full writes/exact reads, portable metadata/refusals and
open-unlinked/cwd/O_PATH/mapping owners. No per-WRITE invalidation. Extend the
actual external tests with FP-10–18/20/28/29/31 and serial refill EAGAIN proof.

## R4

Workspace `ports/captured_namespace.rs` adapts exact retained ReaderInodes,
ReaderInode and ReaderDirectoryEntries pages/points through daemon
`overlay/captured_namespace_port.rs`. `construction/{context,driver,outcome}.rs`
and focused namespace files produce sealed indexed StreamedFilesystemInput:
directory headers/names, inode changes, fresh serials and fresh positions.
Reuse IndexedConstructionRecords, 64-row/64KiB windows, CapturedFileEdits,
Store-derived policy and Save::sink with same-Save reads. Preserve first-original
failure plus every input/operation/reader owner through actual consumer disposal.

Content's existing validation/entry/cycle and indexed reference-reduction owners
gain backed demanded-serial/addition/parent/cycle state and incremental topology.
No resident front end, whole-base non-file alias scan or quadratic replacement
qualifies the resource requirement. Component proof independently checks complete
namespace/bytes/metadata/link classes, wide/deep/sparse/alias/cycle cases,
fragmentation and bounded construction/SQL/I/O/copy/release work.

## R5

Connect the completed producer to existing daemon `store/commit.rs` and its
control admission. Preserve capture→construction→Save.finish→History→known
install, overwrite/captured parent, later active mutations and exact retained
reads. Keep the Store open and one construction producer. Cover Committed,
UpToDate, real Busy, missing dependencies, definite failure, unknown publication
and known publication/local-install failure. The independent oracle mounts the
published Branch and reads all declared names/bytes/metadata/links, including
.git, ignored/cache/output membership. No host Init or materialized fallback.

## Selection discipline

Before any invocation, read its current report/family contract, record identities
and use fresh receipts. Build --locked/--no-run first; tests get an explicit≤120s
stop (normally100s), independent native proof normally9s. Serialize builds and
proofs in the primary checkout with its own targets/locks. The initial native
selection is FP-1/2/3/4/read17/20/21/22-FS/31/34 plus owning count rows, after the
dependency lifecycle mechanism is established. Nothing here registers or runs a
performance sample. No R1 resampling, R6–R9 execution or retirement is selected.
