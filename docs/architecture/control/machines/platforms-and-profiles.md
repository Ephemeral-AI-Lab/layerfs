# Platforms and profiles

[Architecture](../../README.md) › [Control](../README.md) › [Machines](README.md) › Platforms and profiles

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Firecracker, Microsandbox and SSH machines were removed in 2.0; a record
an older home still holds for one is shown, refused and only forgotten.
See [removed platforms](../support-matrix/disabled-platforms.md).

The Platform enum, its Profile and options.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Machines](README.md) |
| Source | [`control/crates/layerfs-platform/src/platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs), [`control/crates/layerfs-platform/src/options.rs`](../../../../control/crates/layerfs-platform/src/options.rs), [`control/crates/layerfs-platform/src/spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs), `platforms/<p>/profile.rs` |

## Platform

```text
 Platform (4 variants, Platform::ALL order)
   |
   +-- profile() ---> &'static Profile      one match, platform.rs
   |                    platforms/<p>/profile.rs: PROFILE
   |
   +-- adapter() ---> &'static dyn Adapter  one match, adapter.rs
                        platforms/<p>/adapter.rs: ADAPTER
                          |
                          v
                        platforms/<p>/observe.rs, create.rs, power.rs, ...

 Platform::Lima --profile()--> lima::PROFILE  { name: "lima", ... }
               --adapter()---> lima::ADAPTER  (struct Lima)
                                 -> lima/create.rs, power.rs, observe.rs,
                                    template.rs
```

| Variant | `name()` | `How` | Folder |
| --- | --- | --- | --- |
| `Native` | `native` | `BuiltIn` | `platforms/native/` |
| `Docker` | `docker` | `BuiltIn` | `platforms/docker/` |
| `AppleContainer` | `apple-container` | `Created` | `platforms/apple_container/` |
| `Lima` | `lima` | `Created` | `platforms/lima/` |

Every other per-platform value is read from the profile:
`name`, `parse` (exact name only), `built_in`, `created`, `tool`,
`backend(host)`, `kernel(host)`, `executor(host)`, `route(host)` and
`worker_bind` ([`platform.rs`](../../../../control/crates/layerfs-platform/src/platform.rs)).
`Firecracker`, `Microsandbox` and `Ssh` were removed in 2.0; `parse` does
not know their names, and a record naming one is handled by
`layerfs-machine` (`MachineRecord::removed`; see
[removed platforms](../support-matrix/disabled-platforms.md)).

| Enum | Variants | Meaning |
| --- | --- | --- |
| `HostOs` | `Linux`, `Macos`, `Windows`, `Other` | The caller's host; `current()` reads `cfg!(target_os)`; `NAMED` is the first three. |
| `How` | `BuiltIn`, `Created` | How a machine comes to exist; `as_str` gives `built_in`, `created`. |
| `ExecutorKind` | `Local`, `Worker` | Who runs a sqlite Store's calls (G8). |
| `Route` | `InProcess`, `Worker` | The same answer as a route; `route(host)` maps `Local` to `InProcess`. |
| `KernelRoute` | `Fuse`, `Nfs` | Re-exported from `layerfs-bridge`; see [Kernel routes](../../engine/kernel-routes/README.md). |

## Profile

| Field | Type | Meaning |
| --- | --- | --- |
| `name` | `&'static str` | The name callers write (`layerfs machine create dev lima`). |
| `tool` | `Option<&'static str>` | The provider CLI the platform runs. |
| `how` | `How` | Built in or created. |
| `hosts` | `&'static [HostOs]` | The hosts a machine of this platform can be on. |
| `backend` | `fn(HostOs) -> Option<Backend>` | The Sandbox backend per host. |
| `kernel` | `fn(HostOs) -> Option<KernelRoute>` | The kernel route its daemons serve per host. |
| `executor` | `fn(HostOs) -> ExecutorKind` | Who runs a sqlite Store's calls per host. |
| `worker_bind` | `Ipv4Addr` | The guest address the worker binds. |
| `options` | `&'static [OptSpec]` | What `machine create` takes. |
| `description` | `&'static str` | One line: what a machine of this platform is. |

Each platform's values, from its `profile.rs`:

| Platform | `tool` | `hosts` | `backend` | `kernel` | `executor` | `worker_bind` |
| --- | --- | --- | --- | --- | --- | --- |
| `native` | none | all three | macOS `HostProcess`, Linux `Namespaces`, else none | macOS `Nfs`, Linux `Fuse`, else none | `Worker` on Windows, else `Local` | `0.0.0.0` (unused) |
| `docker` | `docker` | macOS, Linux | `Namespaces` (in the worker container, D-8) | `Fuse` | `Worker` | `0.0.0.0` |
| `apple-container` | `container` | macOS | `Namespaces` | `Fuse` | `Worker` | `0.0.0.0` |
| `lima` | `limactl` | macOS | `Namespaces` | `Fuse` | `Worker` | `127.0.0.1` |

`worker_bind` is loopback where the provider's forward reaches the guest's
loopback (Lima's port forward) and every interface where the
forward arrives on the guest's network (a published port). The worker's secret guards it either way. The `backend` function
answers even on a host the platform is not on (`docker` on Windows answers
`Namespaces`); the [support table](../support-matrix/table.md) hides it there.

## Options

| `OptKind` | `as_str` | Value |
| --- | --- | --- |
| `Int` | `int` | A whole number. |
| `Size` | `size` | Bytes, or a number with a binary unit (`8G`). |
| `Text` | `text` | Free text: a path or an image reference. |

| Platform | Options (`name`: kind) |
| --- | --- |
| `native`, `docker` | none |
| `apple-container` | `cpus`: int, `memory`: size, `disk`: size, `image`: text |
| `lima` | the four above |

`CPUS`, `MEMORY`, `DISK` and `IMAGE` are shared constants in
[`options.rs`](../../../../control/crates/layerfs-platform/src/options.rs),
built with `opt()`. The crate declares options; it does not parse them. The call table
reads them by `kind`, and `layerfs-control`'s
[`spec.rs`](../../../../control/crates/layerfs-machine/src/spec.rs)
turns a record into a `Spec`. There an absolute-path `image` becomes
`Spec::image` (the Sandbox image archive), while `Spec::os_image` comes from
the record's `os_image` or, on `docker` only, from `image`.

## Spec

```text
 Spec
  name, kind: Platform, vm_name       "<prefix><machine>", the caller's
  cpus, memory, disk                  bytes; disk mounted at /layerfs/data
  image: Option<PathBuf>              Sandbox image archive, copied once
  os_image: Option<String>            the VM's own system, else default
  state: Lasting { port, gateway }    stored by the caller, passed back
  worker: Option<Endpoint>            the last worker.json, if any
  work_dir: PathBuf                   the machine's logs
  tools: Tools { path, docker_socket }
```

| Check in `Spec::validate` | Refusal (`InvalidInput`) |
| --- | --- |
| `name`, and `vm_name` off `native`, are plain names | "machine name … is not a plain name" / "VM name …" |
| `work_dir` is absolute | "the machine's work folder must be an absolute path" |
| a created VM: `cpus` ≥ 1, `memory` ≥ 256 MiB, `disk` ≥ 1 GiB | "a VM needs at least one CPU" and the like |

| Value | Meaning | Source |
| --- | --- | --- |
| `WORKER_PORT` | 7070, the worker's port inside the guest | [`spec.rs`](../../../../control/crates/layerfs-platform/src/spec.rs) |
| `DATA`, `BUNDLE`, `STATE`, `IMAGE` | `/layerfs/data`, `/layerfs/bundle`, `/layerfs/state`, `/layerfs/image` in every guest | same |
| `Tools::from_env` | `PATH`; socket `/var/run/docker.sock` | same |
| `parse_size` | bytes or `K`/`M`/`G`/`T` (powers of 1024), optional `iB`/`B`; 0 refused | same |
| `Bundle` | `dir` and hex `hash` of the sealed Linux bundle | same |
| `Endpoint` | host loopback `address` and `secret` | same |
| `Forward` | host loopback `address` of a daemon, optional `ssh -L` `pid` | same |
| `Running` | `endpoint` and the `lasting` state to record | same |

## Related

- [Adapter](adapter.md): what each platform does with a `Spec`.
- [Support matrix: Table](../support-matrix/table.md): the rows built from profiles.
- [Sandboxes: Backends](../sandboxes/backends.md): `HostProcess`, `Namespaces`.
- [Network: Workers](../network/workers.md): the worker an `Endpoint` names.
- [Control: Management](../management/README.md): where records become a `Spec`.
- [Glossary](../../glossary.md).

---

← [Control: Machines](README.md) · [Up: Machines](README.md) · [Machines: Adapter](adapter.md) →
