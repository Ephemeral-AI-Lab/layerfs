# connect

> **Status:** Current layer guide (#535). The crates arrived in #535 phase 1;
> `SUPPORT.json` is generated since phase 3.

The connect layer is everything that reaches a machine or the network. The
engine keeps persistence, the daemon, the overlay, Workspaces and the kernel
routes; connect decides where a Sandbox runs, how it runs there and how a
Store is backed up. Layers are `core < engine < connect < api`; nothing in
`engine/` depends on `connect/`.

| Crate | Question | Folder per |
| --- | --- | --- |
| `layerfs-machine` | *Where* can Sandboxes run? | platform: `src/platforms/<platform>/` |
| `layerfs-sandbox` | *How* does a Sandbox run on that machine's kernel? | backend: `src/backend/<backend>/` |
| `layerfs-store-backup` | *How* is a Store backed up to a bucket? | provider: `src/bucket/<provider>/` |

## Reading the tree

- `layerfs-machine/src/platform.rs` is the `Platform` enum and its `Profile`
  type: per platform, the hosts it runs on, the Sandbox backend and kernel
  route per host, whether its sqlite Store calls run in process or in its
  worker, and the options `machine create` or `machine connect` accept
  (`options.rs`). `Platform::profile()` is the one match.
- `layerfs-machine/src/adapter.rs` is the sealed `Adapter` trait (probe,
  guest, create/remove, start/stop, status, host identity) and the one
  `adapter()` match the machine calls dispatch through.
- `layerfs-machine/src/support.rs` is the one support table:
  platform × host × Store kind → supported, or a refusal with its reason.
- `layerfs-machine/src/platforms/<p>/`: `profile.rs` holds its `PROFILE`
  and `adapter.rs` its `ADAPTER` (the guard keeps both out of `mod.rs`);
  `observe.rs`, `create.rs` (create and remove), `power.rs` (start and
  stop); ssh has `connect.rs` (connect and disconnect) instead of
  `create.rs`. `network.rs` holds docker's published port today; `reach`
  and `forward` arrive there in #535 phases 5 and 6.
- `layerfs-sandbox/src/backend/<b>/` is one Sandbox backend, with its
  `ADAPTER` (stored name, description, capabilities) in `adapter.rs`,
  reached through the sealed `BackendAdapter`'s one `adapter()` match in
  `backend/adapter.rs`; `src/local/` is the shared "daemon as a local
  process" plumbing that `host_process` and `namespaces` build on. The
  backend Rust name `host_process` is stored and shown as `"native"`.

## SUPPORT.json

`SUPPORT.json` is generated from `support.rs` (`support_json()`) and
compared with it by the test `connect/crates/layerfs-machine/tests/support.rs`;
do not edit it by hand. After changing the table, write the file from it
and review the diff:

```
LAYERFS_BLESS_SUPPORT=1 cargo +1.85.1 test --locked -p layerfs-machine --test support
```

It has one block per host (`macos`, `linux`, `windows`), each with one row
per platform, the same rows `machine.platforms` returns for the running
host without its live fields (`ready`, `missing`, `version`):

- `platform`, `how` (`built_in`, `created`, `connected`);
- `backend`: the Sandbox backend's stored name (`docker`, `native`,
  `namespaces`), or `null` when there is none or the platform is not on
  that host;
- `kernel`: the kernel route (`fuse`, `nfs`) or `null`;
- `sqlite`: where a sqlite Store's calls run (`in_process` or `worker`), or
  `null` when the platform is not on that host;
- `postgresql`: `store_calls` (always `in_process`), `sandbox` (whether a
  PostgreSQL Store's Sandbox runs there) and `reason`, the exact refusal
  text the product returns, or `null`;
- `options`: the `machine create` or `machine connect` options, each with
  its `kind` (`int`, `size`, `text`, `bool`, `name`);
- `description`: the platform and how its Sandboxes run.

The table encodes what is allowed today; #535 phase 5 flips its
PostgreSQL rows. `layerfs machine platforms` (the call `machine.platforms`)
returns the same data for the running host.

## Adding things

| To add | Touch |
| --- | --- |
| a platform | `platforms/<p>/` (with `profile.rs` and `adapter.rs`); one `Platform` variant; one `profile()` and one `adapter()` arm; its rows in `support.rs`; a `Guest` variant only for a new shell transport |
| a Sandbox backend | `backend/<b>/`; one `Backend` variant; one `adapter()` arm; one record type |
| a bucket provider | `layerfs-store-backup/src/bucket/<p>/` |

A Store kind and a kernel route are engine changes; see #535 §9.
