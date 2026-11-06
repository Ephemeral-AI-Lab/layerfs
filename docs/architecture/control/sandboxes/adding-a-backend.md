# Adding a backend

[Architecture](../../README.md) › [Control](../README.md) › [Sandboxes](README.md) › Adding a backend

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The recipe for a new Sandbox backend.

| | |
| --- | --- |
| Component | [Sandboxes](README.md) |
| Source | [`control/crates/layerfs-isolation/src/backend`](../../../../control/crates/layerfs-isolation/src/backend), [`control/crates/layerfs-isolation/src/runtime.rs`](../../../../control/crates/layerfs-isolation/src/runtime.rs), [`control/crates/layerfs-isolation/src/types/options.rs`](../../../../control/crates/layerfs-isolation/src/types/options.rs), [`control/README.md`](../../../../control/README.md) ("Adding things"), [`control/AGENTS.md`](../../../../control/AGENTS.md) ("Extension points") |

## Steps

There is no provider trait, registry or factory. A backend is one folder,
one enum variant and one arm in each exhaustive `match`; the compiler lists
every place that still needs it.

Scope: commit `d2c3f2c97f`, `tools/production_loc.py --files`, the
`layerfs-isolation` and `layerfs-platform` crates; only the files a backend
touches are shown, so each bracket covers the files shown.

```diff
  layerfs-isolation/src/                [457]  (partial)
    runtime.rs                         276   (+ two variants, every arm)
    backend/                           [44]  (partial)
      mod.rs                             7   (+ pub mod <b>)
      kind.rs                           23   (+ Backend::<B>, Backend::ALL)
      adapter.rs                        14   (+ one arm in adapter())
+     <b>/                               —   new
+       mod.rs                           —   new
+       adapter.rs                       —   new (stored name, capabilities)
+       provider.rs                      —   new (what Runtime holds)
+       create.rs                        —   new (start one Sandbox)
+       sandbox.rs                       —   new (stop, delete, inspect)
+       exec.rs                          —   new (one command)
    types/                            [137]  (partial)
      options.rs                       137   (+ launch options, record)
```

```diff
  layerfs-platform/src/                [351]  (partial)
    platform.rs                        148   (backend per platform)
    support.rs                         203   (rows; regenerate SUPPORT.json)
```

The `profile.rs` of each platform that uses the backend names it in
`backend` (for example
[`native/profile.rs`](../../../../control/crates/layerfs-platform/src/platforms/native/profile.rs)).

| # | Step | Where | What breaks until done |
| --- | --- | --- | --- |
| 1 | Add `Backend::<B>` and put it in `Backend::ALL`. | [`kind.rs`](../../../../control/crates/layerfs-isolation/src/backend/kind.rs) | `adapter()` match |
| 2 | Write `<b>/adapter.rs`: a `static ADAPTER` implementing `BackendAdapter` (`stored_name`, `description`, `capabilities`); add its arm to `adapter()`. Pick a stored name once: records keep it even if the Rust name changes. | [`adapter.rs`](../../../../control/crates/layerfs-isolation/src/backend/adapter.rs) | — |
| 3 | Add a `ProviderLaunchOptions` variant if the launch needs its own inputs, and a `SandboxRecord` variant: the record names the backend, so no other provider reopens it. | [`types/options.rs`](../../../../control/crates/layerfs-isolation/src/types/options.rs) | provider refusals |
| 4 | Write the provider (`launch`, or `launch_with_binds` if it binds; `reopen`; `reopen_stopped`) and the owner (`identity`, `endpoint`, `run`, `track`, `log_tail`, `inspect`, `stop`, `delete`). Reuse `local/` for a process on this kernel. | `<b>/` | — |
| 5 | Add `Runtime::<B>` and `Sandbox::<B>` with every arm in [`runtime.rs`](../../../../control/crates/layerfs-isolation/src/runtime.rs); refuse binds before any effect if it takes none. | `runtime.rs` | every `match self` |
| 6 | Map the platforms that use it (`Profile::backend`) and their rows in the support table. | [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs), [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) | `SUPPORT.json` comparison test |
| 7 | Map its stored name in the API's Sandbox records and launch (`ProviderCreate` options). | [`control/crates/layerfs-control/src/sandbox`](../../../../control/crates/layerfs-sandbox/src) | Not established from source in full; follow the compiler from `Runtime`. |

Rules each backend keeps, from the existing three:

| Rule | Where it shows |
| --- | --- |
| One attempt per operation; `*_attempted` flags refuse a second stop, delete or upload. | each `sandbox.rs` |
| A launch failure after an effect returns the owner (`LaunchFailure::instance`) for one stop and delete. | `provider.rs` |
| The daemon setup is private (0600) and never in `Debug`, argv, env or labels. | [Network: Credentials](../network/credentials.md) |
| Commands run as the nonroot `CommandIdentity`, cannot create a user namespace, and end with the Sandbox. | [Native and namespaces](native-and-namespaces.md), [Docker](docker.md) |
| `run` streams to sinks and reports the root's exit; it never treats end of stream as exit. | [Commands and streams](commands-and-streams.md) |
| The daemon announces `LAYERFS_DAEMON_LISTEN <loopback addr>`; the backend waits for it once. | [`local/listener.rs`](../../../../control/crates/layerfs-isolation/src/local/listener.rs) |

Production files stay at most 999 lines and `mod.rs` at most 200 lines of
declarations ([control guide](../../../../control/AGENTS.md)).

## Tests

| Test | What a new backend must pass | Command (Unix; not run while writing this page) |
| --- | --- | --- |
| [`conformance.rs`](../../../../control/crates/layerfs-isolation/tests/conformance.rs) | Runs over `Backend::ALL`: stored name round-trips and is unique; capabilities match the kernel route of every `SUPPORT.json` row that names it; bind refusal or carriage through `HelperSpec`; the runtime on this host refuses other backends' options and records; the lifecycle it can run here. Its exhaustive matches (`binds`, `options`, `record`, `runtime`, `lifecycle`) do not compile until the backend states its answers. | `cargo +1.85.1 test --locked -p layerfs-isolation --test conformance` |
| [`runtime_enum.rs`](../../../../control/crates/layerfs-isolation/tests/runtime_enum.rs) | Stored names in order and parse-back; each backend's capabilities; binds refused where the backend does not bind. Update its expected lists. | `cargo +1.85.1 test --locked -p layerfs-isolation --test runtime_enum` |
| [`machine` conformance](../../../../control/crates/layerfs-platform/tests/conformance.rs) | Profiles against `support.rs` and `SUPPORT.json`. | `cargo +1.85.1 test --locked -p layerfs-platform --test conformance` |
| A lifecycle test of its own | Like [`native_provider.rs`](../../../../control/crates/layerfs-isolation/tests/native_provider.rs) (stub daemon script) or [`docker_provider.rs`](../../../../control/crates/layerfs-isolation/tests/docker_provider.rs) (scripted Engine peer), gated with `#[cfg(target_os = ...)]` for where it runs. | per test |

The kits use stand-ins and need no Docker Engine, VM, ssh host or root;
platforms themselves run them in the terminal pass ([`control/README.md`](../../../../control/README.md)).

## Related

- [Backends](backends.md): the sealed adapter and the match this recipe extends.
- [Machines: Adding a platform](../machines/adding-a-platform.md): the same recipe for a platform.
- [Support matrix: Table](../support-matrix/table.md): where the new rows go.

---

← [Sandboxes: Commands and streams](commands-and-streams.md) · [Up: Sandboxes](README.md) · [Control: Backups](../backups/README.md) →
