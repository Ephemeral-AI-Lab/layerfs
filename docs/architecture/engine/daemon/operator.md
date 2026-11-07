# Operator

[Architecture](../../README.md) › [Engine](../README.md) › [Daemon](README.md) › Operator

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Operator calls and the filesystem port.

| | |
| --- | --- |
| Component | [Daemon](README.md) |
| Source | [`engine/crates/layerfs-daemon/src/operator`](../../../../engine/crates/layerfs-daemon/src/operator), [`engine/crates/layerfs-daemon/src/service`](../../../../engine/crates/layerfs-daemon/src/service), [`engine/crates/layerfs-daemon/src/install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |

## Operator dispatch

```text
Request::Sdk(SdkCall { method, args })
  operator::admits(method)?   method starts with "backend."
                              or == "sandbox.list_workspaces"
     yes -> operator::dispatch -> Reply::Sdk | Reply::SdkRefused
     no  -> SdkRefused { code "UnknownMethod" }:
            Store calls run in the SDK's process
```

| Method | Answer | Source |
| --- | --- | --- |
| `backend.daemon.workspace.get_storage` | The overlay's own row counts from `Overlay::resources()`: `inodes`, `entries`, `payload` bytes, `metadataRows`; `metadata` and `changes` are `null` | [`operator/workspace.rs`](../../../../engine/crates/layerfs-daemon/src/operator/workspace.rs) |
| `backend.daemon.workspace.list_changes` | One page of changed paths (`path`, `kind`, `markers`), with `hasMore` and `nextPagePos` | [`operator/workspace.rs`](../../../../engine/crates/layerfs-daemon/src/operator/workspace.rs) |
| `sandbox.list_workspaces` | `inspect.snapshot`: history from the Store plus one view per bound Workspace (namespace, activity, mounted directory, revision, dirty counts, pending merge) | [`control/inspect.rs`](../../../../engine/crates/layerfs-daemon/src/control/inspect.rs) |
| any other `backend.*` | `UnknownMethod` | [`operator/dispatch.rs`](../../../../engine/crates/layerfs-daemon/src/operator/dispatch.rs) |

| Rule | Source |
| --- | --- |
| The Workspace is chosen by its mount directory (`path`); only a `Ready` entry matches. | [`control/observation.rs`](../../../../engine/crates/layerfs-daemon/src/control/observation.rs) |
| `list_changes` takes one diagnostic `BaseSource`, walks base and live trees side by side in bounded windows, reports conflict markers, and releases the source exactly once. | [`operator/workspace.rs`](../../../../engine/crates/layerfs-daemon/src/operator/workspace.rs) |
| `get_storage` reads the overlay database alone; it does not walk the base tree. | [`operator/workspace.rs`](../../../../engine/crates/layerfs-daemon/src/operator/workspace.rs) |
| The snapshot shape is shared with the Store-side operator read model. | [`operator/mod.rs`](../../../../engine/crates/layerfs-daemon/src/operator/mod.rs) |

## Filesystem port

```text
kernel request (FUSE loop or NFS export)
  Mounted(Arc<BoundWorkspace>)::request(fence)
    -> FilesystemPort(StoreOperation, Fence)   one per request
         fenced?  checked only before an attempt
         call(job): owner client job under the Workspace lock
           -> ServiceReply(value, owner client)  keeps backing alive
         base_client(): this Workspace's opened Store reader
```

| `RequestServices` method | What it runs | Overlay work |
| --- | --- | --- |
| `observe_visit`, `mutate_visit` | One kernel visit job: lookup/getattr, or a mutation with its reply custody | [`kernel_visit.rs`](../../../../engine/crates/layerfs-overlay/src/lifetime/kernel_visit.rs) |
| `directory_visit`, `publish_cookies`, `close_directory` | READDIR page, its cookies, RELEASEDIR | [Open state](../overlay/open-state.md#open-directories) |
| `read_visit`, `close_file` | READ window, RELEASE | [Payload: Streams](../overlay/payload.md#streams) |
| `base` | The immutable base view through this Workspace's Store reader | none |
| `recognize` | Write bytes compared with the file's committed content to form `WriteClaims`; no SQL, no owner turn | none |
| `forget` | `kernel_forget` | [Open state](../overlay/open-state.md#kernel-references) |
| `reserve_serial` | Take an inode serial; may make one early refill attempt at low water | none |
| `reply_attempted` | Settle a reply ticket; no SQL | [Lifetimes: Tickets](../overlay/lifetimes-and-reclamation.md#tickets) |

| Rule | Source |
| --- | --- |
| The fence is consulted only before an attempt: owner admission, Store reader admission and `reserve_serial`. An admitted job is always awaited to its original result; disposal calls never consult the fence. | [`filesystem_port.rs`](../../../../engine/crates/layerfs-daemon/src/service/filesystem_port.rs) |
| A failed Store read for this request (provider error, no reader to admit it) ends this request alone through a `BaseDemandFailed` marker; other causes pass unchanged. | [`filesystem_port.rs`](../../../../engine/crates/layerfs-daemon/src/service/filesystem_port.rs) |
| Hashing and page reads for `recognize` never run on the loop that received the request. | [`filesystem_port.rs`](../../../../engine/crates/layerfs-daemon/src/service/filesystem_port.rs) |
| A refill refused by SQLite `Busy` at its one `BEGIN` reserved nothing and is not asked again; any other early failure is the create's own error. | [`filesystem_port.rs`](../../../../engine/crates/layerfs-daemon/src/service/filesystem_port.rs) |

The port's contract (`MountServices`, `RequestServices`, `Fence`) is
defined by [Kernel routes: Ports](../kernel-routes/ports.md).

## Installs

| Phase | Daemon step | On failure | Source |
| --- | --- | --- | --- |
| Admission | Only in `InstallPending`; phase becomes `Installing` | A second installer gets `Refused(Request, Exists)` | [`application/serve.rs`](../../../../engine/crates/layerfs-daemon/src/application/serve.rs) |
| `Request` | Manifest must be `Disposable`, name the configured Store path, carry no `daemon_sqlite` | `Protocol` refusal | [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `Claim` | `create_new` of the temporary name; target must be absent; send `Ready` | The claim name is recorded before the attempt | [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs), [`install_file.rs`](../../../../engine/crates/layerfs-daemon/src/install_file.rs) |
| `Transfer` | Write each record with positive progress until `bytes`; then `INSTALL_FINISH` | Partial file kept | [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `Publish` | One `rename` to the Store path; no sync for `Disposable` | — | [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `Open` | `open_store_observed` with the `Disposable` profile; check the initial Branch and root | Opened Store kept in `InstallFailure` | [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| `Reply` | Send `Installed(manifest + daemon_sqlite)`; startup builds the `Service`, phase `ControlReady` | Published and checked: still `ControlReady`; otherwise `Retained` | [`application/serve.rs`](../../../../engine/crates/layerfs-daemon/src/application/serve.rs) |

Every failure sends one `Refused(InstallRefusal)` unless the channel itself
failed, then fences the sender once; files are not removed automatically.
The manifest type still names a Durable profile and `install.rs` has sync
branches for it, but the `Request` check refuses any profile other than
`Disposable` first, so they never run. `InstallWork` records records,
write calls, bytes written, sync calls and the largest record. Source:
[`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs),
[`install_types.rs`](../../../../engine/crates/layerfs-daemon/src/install_types.rs).

## Related

- [Bridge: Provisioning](bridge.md#provisioning): the install records.
- [Startup](startup.md): phases and the reserved installer slot.
- [Project Init: Seal and install](../project-init/seal-and-install.md): the sealed file and the installer.
- [Kernel routes: Ports](../kernel-routes/ports.md): the request service contract.
- [Workspace: Ports](../workspace/ports.md): the Store and overlay ports a `StoreOperation` holds.
- [Glossary](../../glossary.md).

---

← [Daemon: Bridge](bridge.md) · [Up: Daemon](README.md) · [Control](../../control/README.md) →
