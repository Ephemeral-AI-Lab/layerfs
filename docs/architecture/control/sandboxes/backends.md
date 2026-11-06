# Backends

[Architecture](../../README.md) › [Control](../README.md) › [Sandboxes](README.md) › Backends

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The sealed BackendAdapter and the backend kinds.

| | |
| --- | --- |
| Component | [Sandboxes](README.md) |
| Source | [`control/crates/layerfs-sandbox-backend/src/backend/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/adapter.rs), [`control/crates/layerfs-sandbox-backend/src/backend/kind.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/kind.rs), [`control/crates/layerfs-sandbox-backend/src/runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs), [`control/crates/layerfs-sandbox-backend/src/types/capabilities.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/capabilities.rs) |

## The sealed BackendAdapter

```text
 Backend::stored_name() / description() / capabilities()      kind.rs
        |
        v
 adapter(backend) -> &'static dyn BackendAdapter   one exhaustive match
        |
        +-- Backend::Docker      => &docker::ADAPTER        (DockerBackend)
        +-- Backend::HostProcess => &host_process::ADAPTER  (HostProcessBackend)
        +-- Backend::Namespaces  => &namespaces::ADAPTER    (NamespacesBackend)

 trait BackendAdapter: Sync          crate-private, no default methods
   fn stored_name(&self)  -> &'static str
   fn description(&self)  -> &'static str
   fn capabilities(&self) -> ProviderCapabilities
```

`BackendAdapter` holds only what is fixed per backend (#535 §6.3, G2). It
is `pub(crate)`, has no default methods and no registry: a new backend is a
new `Backend` variant, and the `match` in `adapter()` does not compile until
it has an arm ([`adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/adapter.rs)).
Each adapter builds on every Unix host, so `stored_name` and `capabilities`
answer even where the backend cannot run.

Launch, reopen and the running Sandbox are concrete types, matched by the
two public enums in [`runtime.rs`](../../../../control/crates/layerfs-sandbox-backend/src/runtime.rs):

| Enum | Variants | Methods |
| --- | --- | --- |
| `Runtime` (one deployment's choice) | `Docker(DockerProvider)`; `HostProcess(HostProcessProvider)` (macOS); `Namespaces(NamespaceProvider)` (Linux) | `backend`, `capabilities`, `local_directory`, `native_paths`, `launch`, `launch_with_binds`, `reopen`, `reopen_stopped` |
| `Sandbox` (the launched owner) | `Docker(docker::Sandbox)`; `HostProcess(host_process::Sandbox)` (macOS and Linux); `Namespaces(Box<namespaces::Sandbox>)` (Linux) | `backend`, `identity`, `endpoint`, `run`, `track`, `log_tail`, `inspect`, `stop`, `delete`, `scratch_directory`, `local_record`, `record` |

On Linux, `Sandbox::HostProcess` exists only to reopen, stop and delete a
native Sandbox recorded before 2.0; Linux has no native provider (#533 O27,
[`host_process/mod.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/mod.rs)).

## Backend kinds

| | `Docker` | `HostProcess` | `Namespaces` |
| --- | --- | --- | --- |
| Description | "Sandboxes run in Docker containers" | "Sandboxes run as this user's processes, served over loopback NFS" | "Sandboxes run in Linux namespaces and a cgroup" |
| `capabilities.name` | `docker` | `native-macos` | `namespaces-linux` |
| `isolation` | `Container` | `HostProcess` | `MicroVm` (the VM holding the kernel is the boundary) |
| `mount_driver` | `Fuse` | `LoopbackNfs` | `Fuse` |
| `requires_image` | yes | no | yes |
| `search_path_policy` | `FixedContainer` | `InheritHost` | `FixedContainer` |
| `supports_resource_limits` | no | no | no |
| `direct_store_access` | yes | yes | yes |
| Launch options | `ProviderLaunchOptions::Docker` | `::Local` | `::Namespaces` |
| Store reaches the daemon as | a volume, volume folder or host folder at `/layerfs-store` | the host's own files | a `Bind` of the Store's directory |
| Process boundary | the container | process groups named in `groups/` | cgroup v2 directory |
| Builds and runs on | any Unix host | macOS (launch) | Linux |
| Topic | [Docker](docker.md) | [Native and namespaces](native-and-namespaces.md) | [Native and namespaces](native-and-namespaces.md) |

Capabilities "describe support, not qualification"
([`capabilities.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/capabilities.rs)).
The conformance kit checks each `mount_driver` against the kernel route of
every `SUPPORT.json` row that names the backend
([`conformance.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/conformance.rs)).

## Stored names

```text
 record / listing / SUPPORT.json        Rust
   "docker"      <---- stored_name ----  Backend::Docker
   "native"      <---- stored_name ----  Backend::HostProcess   (G6)
   "namespaces"  <---- stored_name ----  Backend::Namespaces
                 ----- Backend::parse -->
```

| Rule | Source |
| --- | --- |
| Stored and displayed values do not follow internal renames: `HostProcess` is `"native"` (#535 G6). | [`host_process/adapter.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/host_process/adapter.rs) |
| `Backend::parse` finds the variant whose `stored_name` matches; `Backend::ALL` lists `[Docker, HostProcess, Namespaces]`. | [`kind.rs`](../../../../control/crates/layerfs-sandbox-backend/src/backend/kind.rs) |
| Names round-trip and are unique. | `stored_names_parse_back_and_do_not_follow_the_rust_names` ([`runtime_enum.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/runtime_enum.rs)), `each_stored_name_round_trips_and_is_unique` ([`conformance.rs`](../../../../control/crates/layerfs-sandbox-backend/tests/conformance.rs)) |
| The record variant names the backend too: `SandboxRecord::Docker`, `::Local` (native), `::Namespaces`. | [`types/options.rs`](../../../../control/crates/layerfs-sandbox-backend/src/types/options.rs) |

`capabilities.name` (`native-macos`, `namespaces-linux`) is a different
string from the stored name and is not what records store.

## Related

- [Adding a backend](adding-a-backend.md): the steps that follow from the one match.
- [Support matrix](../support-matrix/README.md): which platform and host get which backend.
- [Machines: Adapter](../machines/adapter.md): the same sealed-trait pattern for platforms.

---

← [Control: Sandboxes](README.md) · [Up: Sandboxes](README.md) · [Sandboxes: Docker](docker.md) →
