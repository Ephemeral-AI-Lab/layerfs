# Public read-only WorkspaceViewLease contract (finalized before implementation)

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> This is the finalized contract for the owner-directed 1A implementation, recorded
> before any product line was written. It follows the
> [handoff appendix proposal](HANDOFF-PREINTEGRATION.md#appendix--prospective-public-read-only-sdk-view-contract-20260928)
> with the names, bounds and ownership finalized against the actual source at
> `29fc5747d70eadf01fd8794bf84a50cf57ef3660`. It authorizes no benchmark sample,
> PR merge, or release claim, and it does not relabel any retained receipt.

## 1. Capability and read-only boundary

One bounded, non-cloneable opaque `WorkspaceViewLease` pins the *current* selected
G1 view of the same attached Workspace before an ordinary `commit`. All view
lookup, list, read and readlink operations are **read-only for contents and
namespace**: they resolve through the exact pinned selection (the held
`ActiveSnapshot` from `ActiveBacking::pin_view`, its pack watermark, the pinned
canonical base and the pinned publication's origins), never by silently
switching to the latest view, remounting a committed Store or returning an
unrelated serial. The old view cannot mutate or Commit through the lease.
`mount`, `exec`, `commit`, `status` and `unmount` behavior is unchanged.

Acquiring and releasing a lease **does** change charged pin/retirement
bookkeeping: the pinned revision is held in the active index's frozen set, the
lease and every entry it issues are charged to the Workspace Budget, and the
final release runs the checked retirement selector, which unlinks only verified
retired physical pages and refunds only verified charge. A release failure or
unknown outcome retains custody (the workspace stops) and is never silently
retried. `Drop` alone is a best-effort release and is **not** a checked remote
release; the checked remote release is the explicit `release_view` call.
Unmount/close-clean fail closed while a lease is held.

## 2. Public surface (layerfs-api-core + layerfs-sdk)

```rust
impl WorkspaceApi<'_> {
    pub fn pin_view(&self, id: &WorkspaceId) -> Result<WorkspaceViewLease, WorkspaceError>;
    pub fn view_lookup(&self, lease: &WorkspaceViewLease, parent: &WorkspaceViewEntry,
                       name: &[u8]) -> Result<WorkspaceViewEntry, WorkspaceError>;
    pub fn view_list(&self, lease: &WorkspaceViewLease, dir: &WorkspaceViewEntry,
                     after: Option<&[u8]>, max_entries: usize)
                     -> Result<WorkspaceViewDirectoryPage, WorkspaceError>;
    pub fn view_read(&self, lease: &WorkspaceViewLease, file: &WorkspaceViewEntry,
                     offset: u64, max_bytes: usize) -> Result<WorkspaceViewRead, WorkspaceError>;
    pub fn view_readlink(&self, lease: &WorkspaceViewLease, link: &WorkspaceViewEntry)
                         -> Result<Vec<u8>, WorkspaceError>;
    pub fn view_status(&self, lease: &WorkspaceViewLease)
                       -> Result<WorkspaceViewStatus, WorkspaceError>;
    pub fn release_view(&self, lease: &WorkspaceViewLease)
                        -> Result<WorkspaceViewRelease, WorkspaceError>;
}
impl WorkspaceViewLease {
    pub fn root(&self) -> &WorkspaceViewEntry;   // local accessor, no remote call
    pub fn generation(&self) -> u64;             // pinned generation observation
    pub fn revision(&self) -> u64;               // pinned revision observation
}
```

- `WorkspaceViewLease` binds `WorkspaceId` + incarnation + a 33-byte opaque
  lease token (1 tag + 32 daemon-generated random bytes). It is `Clone` for
  sharing within one client and carries no I/O. A lease token is dead after a
  `Completed` release; further view operations on it fail `Denied`.
- `WorkspaceViewEntry` carries the lease binding, selected `serial`, `kind`
  (File/Directory/Symlink), `size`, `references`, `mode` and `mtime`. It exposes
  **no physical page identity**. An entry resolved by one lease is refused by
  every other lease (binding checked server-side against the issuing registry),
  and forged or stale entries fail `Denied`/`NotFound`.
- `WorkspaceViewDirectoryPage` carries bounded `(name, serial)` entries plus one
  continuation name; full entries come from `view_lookup` (component-relative,
  #264-compatible).
- `WorkspaceViewRead` carries `bytes`, `eof` and the pinned `size`; bounds are
  `max_bytes <= 1 MiB`. `view_readlink` returns the exact pinned target
  (<= 4,096 bytes, no NUL).
- `WorkspaceViewStatus` is a read-only custody observation (pinned
  generation/revision, held entries, retirement and custody counters); it is
  never a pin, a release or a numeric admission proof.
- `WorkspaceViewRelease` reports `Completed` only after the checked release
  finished; `Retained { cause }` means the failed unlink cohort stays in charged
  custody and the Workspace stops. The lease token is spent either way; the
  observation of retained custody is the workspace's stopped state and its
  retirement/custody counters.

## 3. Wire, daemon and ownership map

| Concern | File (NEW unless noted) |
| --- | --- |
| Public types | `core/crates/layerfs-api/core/src/workspace_view.rs` |
| SDK methods | `core/crates/layerfs-api/sdk/src/workspace_view.rs` |
| Contract validation + wire | `core/crates/layerfs-bridge/src/contract/workspace_view.rs` |
| Request/response variants | `contract/request.rs`, `contract/outcome.rs`, `contract/mod.rs` (existing, extended) |
| Native codec | `core/crates/layerfs-bridge/src/adapters/native/protocol/workspace_view.rs` |
| Codec dispatch | `protocol/metadata.rs`, `protocol/response.rs` (existing, extended) |
| Authorized dispatch | `core/crates/layerfs-daemon/src/control_view.rs` |
| Control routing + grants | `core/crates/layerfs-daemon/src/control.rs` (existing, extended) |
| Charged lease registry | `core/crates/layerfs-workspace/src/runtime/view_leases.rs` |
| Pinned-view reads | `core/crates/layerfs-workspace/src/filesystem/view_reads.rs` |
| SDK external proof | `core/crates/layerfs-api/sdk/tests/workspace_view.rs` |

Operations (opcodes 21-27; one shared daemon view-authority grant bit `64`, the
only free control bit — view pin/release and every view read are one capability;
per-operation deadline budgets 5 s except release 15 s):

```text
WorkspacePinView      { workspace, incarnation }
WorkspaceViewLookup   { workspace, incarnation, view, parent, name }        name 1..=255
WorkspaceViewList     { workspace, incarnation, view, directory, after, entries } 1..=128
WorkspaceViewRead     { workspace, incarnation, view, file, offset, bytes } bytes <= 1 MiB
WorkspaceViewReadlink { workspace, incarnation, view, link }
WorkspaceViewStatus   { workspace, incarnation, view }
WorkspaceReleaseView  { workspace, incarnation, view }
```

The lease token is generated by the daemon (32 bytes from the OS randomness
device, tag byte 1) and registered by the Workspace. The Workspace keeps the
held pinned `View`, the pinned root record, and a charged server-side registry
of every entry it issued (serial, pinned path, kind, canonical content root),
so a lookup never trusts a client-supplied path and a forged serial cannot
resolve through an unrelated live directory. At most **32 leases** are held per
Workspace; the 33rd pins nothing and fails `Capacity`. Every response buffer is
charged to the Workspace Budget; an exhausted Budget refuses before resolution.

All view operations take the daemon's existing control slot under `try_lock`
exactly like the other control operations, so a view call during an in-flight
Commit fails `Busy` rather than blocking or interleaving. Sequential view reads
across a finished Commit (pin before `commit`, read after) are the supported
order; concurrent reads inside an in-flight Commit are **not** promised by this
contract.

## 4. Falsifiers (external SDK route, Linux native)

1. Pinned root, generation/revision, pack watermark and canonical base are
   observable and stable across a later Commit.
2. Old G1 bytes through the lease and live G2 bytes through `exec` are both
   exact and independently verified in one attached Workspace, same process.
3. A stale (released) lease, a cross-lease entry, a forged token and a wrong
   kind are each refused (`Denied`/`NotFound`/`WrongKind`).
4. 32 leases are admitted; the 33rd is refused without pinning.
5. Deadline and stopping (cancellation) refusals fail closed.
6. Budget/quota refusal: pinning and read responses fail `Capacity` under an
   exhausted Budget, with no partial entry registered.
7. A known-successful C1 Commit followed by a local C5 failure keeps the pinned
   view readable with custody retained; an uncertain release outcome retains
   custody and is not retried.
8. The final release's verified unlink/refund returns retired physical bytes
   and the charge; `close_clean` succeeds only after every lease released.

The two registered clean/one-edit benchmark controls remain **NOT_RUN** until
this public capability proves their operation on a prospectively sealed harness
identity; nothing in this contract revives the committed-and-reattached
retained fixture or re-labels historical receipts.
