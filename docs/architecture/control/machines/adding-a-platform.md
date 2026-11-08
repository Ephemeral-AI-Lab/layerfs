# Adding a platform

[Architecture](../../README.md) › [Control](../README.md) › [Machines](README.md) › Adding a platform

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The recipe for a new platform.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](README.md) |
| Source | [`control/crates/layerfs-platform/src/platforms`](../../../../control/crates/layerfs-platform/src/platforms), [`control/crates/layerfs-platform/src/support.rs`](../../../../control/crates/layerfs-platform/src/support.rs), [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs), [`adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs) |

## Steps

```text
 control/crates/layerfs-platform/src/
   platform.rs        + Platform::<P>, in ALL (len + 1), profile() arm
   adapter.rs         + adapter() arm
   support.rs         + host_refusal() arm (the match is exhaustive)
   platforms/mod.rs   + pub(crate) mod <p>;
   platforms/<p>/     new folder
     mod.rs             declarations and re-exports only
     profile.rs         PROFILE: Profile
     adapter.rs         ADAPTER, impl Adapter (every method, no defaults)
     observe.rs         state from the provider, guest, status
     create.rs          create and remove
     power.rs           start and stop
   setup/guest.rs     + Guest variant only for a new shell transport
 control/SUPPORT.json   blessed from support.rs, never edited
 control/crates/layerfs-platform/tests/conformance.rs
                      + loopback() and daemon_port() arms
```

| Step | What to write | Why it is needed | Source |
| --- | --- | --- | --- |
| 1 | The folder `platforms/<p>/` with `profile.rs` and `adapter.rs`; `mod.rs` holds only declarations. | `PROFILE` and `ADAPTER` live outside `mod.rs` under the `lib.rs`/`mod.rs` declaration rule. | [control guide](../../../../control/AGENTS.md), [`control/README.md`](../../../../control/README.md) |
| 2 | One `Platform` variant, added to `Platform::ALL` and to `profile()`. | `profile()` is the one match for facts. | [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs) |
| 3 | One `adapter()` arm. | The one match for behaviour (#535 G2). | [`adapter.rs`](../../../../control/crates/layerfs-platform/src/adapter.rs) |
| 4 | `Profile`: `name`, `tool`, `how`, `hosts`, `backend`, `kernel`, `executor`, `worker_bind`, `options`, `description`. Reuse `namespaces`, `fuse`, `worker`, `ANY_HOST`, `MACOS` and the shared `CPUS`, `MEMORY`, `DISK`, `IMAGE`. | The support rows and `SUPPORT.json` are built from it. | [`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs), [`options.rs`](../../../../control/crates/layerfs-platform/src/options.rs) |
| 5 | Every `Adapter` method. Refuse what the platform does not do with `refused(spec, why)` or `forward::refuse`, before any effect. | The trait has no default methods. | [Adapter](adapter.md) |
| 6 | The platform's arm in `host_refusal`, and any `postgresql_refusal` arm. | `host_refusal` matches every platform; it is the only place refusal text lives. | [`support.rs`](../../../../control/crates/layerfs-platform/src/support.rs) |
| 7 | A `Guest` variant only if the guest is reached by a new transport (not ssh, `docker exec`, `container exec`). | `setup/worker.rs` runs steps 2–4 over any `Guest`. | [`setup/guest.rs`](../../../../control/crates/layerfs-platform/src/setup/guest.rs) |
| 8 | Bless `SUPPORT.json` and review its diff. | The comparing test fails until the file matches. | [Generated JSON](../support-matrix/generated-json.md#regenerating-it) |
| 9 | Its arms in the conformance kit's `loopback` and `daemon_port`, and `field` for any new option name. | The kit does not compile (or panics) until the platform declares how it reaches the caller's loopback and how its daemons are reached. | [`tests/conformance.rs`](../../../../control/crates/layerfs-platform/tests/conformance.rs) |
| 10 | Its page under `docs/architecture/control/machines/platforms/`, and the manual's support tables. | Pages change with the source. | [Architecture](../../README.md) |

A created platform's `create` should follow the shared shape the existing
VM platforms use:

```text
 host check / probe facts      MachineError::missing(tool:..., host:...)
 refuse Exists if the VM or its data volume is already there
 make the data disk, then the VM           (setup/vm.rs: call, checked)
   failure, nothing under the name:  remove the disk this create made
   failure, VM exists:               .kept() -> vm_exists = true
 boot, attach /layerfs/data
 worker::bring_up(spec, guest, bundle, port)
   prepare (folders, user namespaces off) -> install (bundle, image)
   -> launch (secret, worker on worker_bind:7070) -> ready (60 s)
 Running { endpoint, lasting: Lasting { port: Some(port), .. } }
```

Outside this crate, `layerfs-control` holds a few per-platform cases (for
example `machine/endpoint.rs` for `docker`,
`sandbox/route.rs` for `apple-container`).
A new option also needs its entry in the call table
([`layerfs-api/src/machine/create.rs`](../../../../api/crates/layerfs-api/src/machine/create.rs))
and its `Spec` mapping in
[`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs).
A new Store kind or kernel route is an engine change, not a platform
(#535 §9).

## Tests

| Test | What a new platform gets | Command |
| --- | --- | --- |
| [`conformance.rs`](../../../../control/crates/layerfs-platform/tests/conformance.rs) | Runs over `Platform::ALL`, so the platform is covered once it is a variant: profile vs support rows on every host, `SUPPORT.json` row by row, each lifecycle call through its own tool or the fixed refusal of its `How`, `reach`, `forward`, options vs `Spec::validate`. | `cargo +1.85.1 test --locked -p layerfs-platform --test conformance` |
| [`support.rs`](../../../../control/crates/layerfs-platform/tests/support.rs) | The committed `SUPPORT.json` matches; refusals are the row's reason. | `cargo +1.85.1 test --locked -p layerfs-platform --test support` |
| [`machine_platform.rs`](../../../../control/crates/layerfs-platform/tests/machine_platform.rs) | Pins every platform's name, backend, route, bind and options; extend it with the new values. | `cargo +1.85.1 test --locked -p layerfs-platform --test machine_platform` |
| a new `machine_<p>.rs` | The provider argv and failure paths against stand-in tools, as [`machine_lima.rs`](../../../../control/crates/layerfs-platform/tests/machine_lima.rs) and [`machine_providers.rs`](../../../../control/crates/layerfs-platform/tests/machine_providers.rs) do. | `cargo +1.85.1 test --locked -p layerfs-platform --test machine_<p>` |

The kit's stand-in tools log each call and refuse, so a lifecycle call
reaches the provider and stops there; it needs no VM, Engine, host or
root. A real platform is then run live, as the #535 terminal pass did, and
its receipts are linked from its page. Every test file is `#![cfg(unix)]`.
The commands were not run while writing this page.

## Related

- [Platforms and profiles](platforms-and-profiles.md): the `Profile` to fill.
- [Adapter](adapter.md): the methods to implement and the fixed refusals.
- [Support matrix: Table](../support-matrix/table.md): the rows a platform adds.
- [Sandboxes: Adding a backend](../sandboxes/adding-a-backend.md): the parallel recipe for a backend.
- [Network: Reach](../network/reach.md) and [Network: Forwards](../network/forwards.md).

---

← [Machines: Lima](platforms/lima.md) · [Up: Machines](README.md) · [Control: Network](../network/README.md) →
