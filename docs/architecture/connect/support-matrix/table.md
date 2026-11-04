# Table

[Architecture](../../README.md) › [Connect](../README.md) › [Support matrix](README.md) › Table

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The support rows and what each field means.

**How to present:** ASCII diagrams first, then tables, then prose. Put each
diagram in a ```` ```text ```` block, keep it within 80 columns, and use prose only
for what a diagram or table cannot say.

| | |
| --- | --- |
| Component | [Support matrix](README.md) |
| Source | [`connect/crates/layerfs-machine/src/support.rs`](../../../../connect/crates/layerfs-machine/src/support.rs), [`platform.rs`](../../../../connect/crates/layerfs-machine/src/platform.rs), [`connect/SUPPORT.json`](../../../../connect/SUPPORT.json) |

## Rows

```text
 Platform::ALL (7)  x  HostOs::NAMED (macos, linux, windows)  = 21 rows
                       (+ HostOs::Other, answered but not written to JSON)

 support(platform, host):
   profile   = platform.profile()
   on_host   = host_refusal(platform, host) is None
   backend   = profile.backend(host)   if on_host
   kernel    = profile.kernel(host)    if on_host
   sqlite    = profile.executor(host)  if on_host  (Local->in_process,
                                                    Worker->worker)
   postgresql= sandbox_allowed(platform, host, Postgresql)
```

The 21 rows as `SUPPORT.json` holds them. A cell reads
backend · kernel · sqlite call site · PostgreSQL Sandbox; "—" is `null`.

| Platform | How | macOS | Linux | Windows |
| --- | --- | --- | --- | --- |
| `native` | built_in | native · nfs · in_process · yes | namespaces · fuse · in_process · yes | — · — · worker · no |
| `docker` | built_in | docker · fuse · worker · yes | docker · fuse · worker · yes | — · — · — · no |
| `apple-container` | created | namespaces · fuse · worker · yes | — · — · — · no | — · — · — · no |
| `microsandbox` | created | namespaces · fuse · worker · yes | namespaces · fuse · worker · yes | namespaces · fuse · worker · yes |
| `lima` | created | namespaces · fuse · worker · yes | — · — · — · no | — · — · — · no |
| `firecracker` | created | namespaces · fuse · worker · yes | — · — · — · no | — · — · — · no |
| `ssh` | connected | namespaces · fuse · worker · yes | namespaces · fuse · worker · yes | namespaces · fuse · worker · yes |

Every row has `postgresql.store_calls` = `in_process`. The Windows column is
what the table allows, not a built host: the crate is `#![cfg(unix)]`
([`lib.rs`](../../../../connect/crates/layerfs-machine/src/lib.rs)). Rows are in
`Platform::ALL` order, the same on every host.

## Fields

| Field (`SupportRow`) | Type | JSON key | Meaning | Source |
| --- | --- | --- | --- | --- |
| `platform` | `Platform` | `platform` | The platform's name as callers write it (`Profile::name`). | [`platform.rs`](../../../../connect/crates/layerfs-machine/src/platform.rs) |
| `host` | `HostOs` | the block's `host` | The host the row answers for. | same |
| `how` | `How` | `how` | `built_in`, `created` or `connected`. | same |
| `backend` | `Option<Backend>` | `backend` | The Sandbox backend's stored name (`Backend::stored_name`); `None` when there is none or the platform is not on the host. `HostProcess` is stored as `native` (#535 G6). | [`support.rs`](../../../../connect/crates/layerfs-machine/src/support.rs) |
| `kernel` | `Option<KernelRoute>` | `kernel` | The kernel route the backend's daemons serve. | same |
| `sqlite` | `Option<CallSite>` | `sqlite` | Where a sqlite Store's calls run; `None` only when the platform is not on the host. | same |
| `postgresql.store_calls` | `CallSite` | `postgresql.store_calls` | Always `InProcess`. | same |
| `postgresql.sandbox` | `bool` | `postgresql.sandbox` | Whether a Sandbox of a PostgreSQL Store runs here. | same |
| `postgresql.reason` | `Option<&'static str>` | `postgresql.reason` | The exact refusal text, or `None`. | same |
| `options` | `&'static [OptSpec]` | `options` | The `machine create` or `machine connect` options, each with its `kind`. | [`options.rs`](../../../../connect/crates/layerfs-machine/src/options.rs) |
| `description` | `String` | `description` | `Profile::description`, plus `"; "` and the backend's description when there is a backend. | [`support.rs`](../../../../connect/crates/layerfs-machine/src/support.rs) |

`SupportRow::supports()` applies a row to one machine:

| `Supports` field | Value |
| --- | --- |
| `sqlite` | `sandbox_allowed(platform, host, Sqlite).is_ok()` |
| `postgresql_sandbox` | the row's `postgresql.sandbox` |
| `reason` | the row's `postgresql.reason` |

## Refusal reasons

```text
 sandbox_allowed(platform, host, store)
   |
   +-- host_refusal(platform, host)
   |     host == Other ..................... Err(OTHER_HOST)
   |     host not in profile.hosts ......... Err(per-platform text)
   +-- platform.backend(host) is None ...... Err(NO_BACKEND)
   +-- store == Sqlite ..................... Ok(())
   +-- store == Postgresql
         postgresql_refusal(platform, host)  Err(text) or Ok(())
```

| Reason text (word for word) | Given for | Source |
| --- | --- | --- |
| `LayerFS names no host other than macOS, Linux and Windows` | any platform on `HostOs::Other` | `OTHER_HOST` |
| `no Sandbox backend on Windows (O5)` | `native` on Windows (no backend) | `NO_BACKEND` |
| `docker through WSL2 is not designed yet` | `docker` on Windows | `host_refusal` |
| `apple-container runs on macOS hosts only` | `apple-container` on Linux or Windows | `host_refusal` |
| `lima runs on macOS hosts only` | `lima` on Linux or Windows | `host_refusal` |
| `firecracker runs on macOS hosts only, in a nested lima machine` | `firecracker` on Linux or Windows | `host_refusal` |

`postgresql_refusal` today refuses only `native` on Windows and `Other`,
which `NO_BACKEND` already refuses first, so a PostgreSQL Sandbox runs
wherever a sqlite one does. #535 phase 5a enabled `native`, `docker` and
`ssh`; the [G7 live receipt](../../../../core/docs/issues/535/receipts/g7-live/RECEIPT.md)
enabled `lima`, `apple-container`, `microsandbox` and `firecracker`. A
refusal reaches the caller as `Unsupported`; `sandbox create` wraps it as
"a Sandbox of <kind> Store <s> cannot run on machine <m> (<platform>):
<reason>" ([`sandbox/create.rs`](../../../../api/crates/layerfs-control/src/sandbox/create.rs)).

## Related

- [Generated JSON](generated-json.md): how these rows are written and compared.
- [Platforms and profiles](../machines/platforms-and-profiles.md): the
  `Profile` fields the rows read.
- [Sandboxes: Backends](../sandboxes/backends.md): the backends named in `backend`.
- [Kernel routes](../../engine/kernel-routes/README.md): `fuse` and `nfs`.
- [Network: Reach](../network/reach.md): how a PostgreSQL Sandbox reaches its server.
- [Glossary](../../glossary.md).

---

← [Connect: Support matrix](README.md) · [Up: Support matrix](README.md) · [Support matrix: Generated JSON](generated-json.md) →
