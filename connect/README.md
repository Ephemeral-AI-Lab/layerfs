# connect

> **Status:** Current layer guide (#535). The crates arrive in #535 phase 1;
> `SUPPORT.json` is generated from phase 3.

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
  table: per platform, the hosts it runs on, the Sandbox backend and kernel
  route per host, whether its Store calls run in process or in its worker,
  and the options `machine create` or `machine connect` accept.
- `layerfs-machine/src/support.rs` is the one support table:
  platform × host × Store kind → supported, or a refusal with its reason.
- `layerfs-machine/src/platforms/<p>/`: `mod.rs` holds the profile and the
  adapter; `observe.rs`, `create.rs` (create and remove), `power.rs` (start
  and stop), `network.rs` (`reach` and `forward`); ssh has `connect.rs`
  (connect and disconnect) instead of `create.rs`.
- `layerfs-sandbox/src/backend/<b>/` is one Sandbox backend; `src/local/` is
  the shared "daemon as a local process" plumbing that `host_process` and
  `namespaces` build on. The backend Rust name `host_process` is stored and
  shown as `"native"`.

## SUPPORT.json

`SUPPORT.json` is generated from `support.rs` and compared with it by a
test; do not edit it by hand. It has one entry per platform and host:

- `platform`, `host`: the machine platform and the host OS;
- `backend`: the Sandbox backend's stored name (`docker`, `native`,
  `namespaces`) with a `description`, or `null` when there is none;
- `kernel`: the kernel route (`fuse`, `nfs`) or `null`;
- `stores`: for `sqlite` and `postgresql`, where Store calls run
  (`in_process` or `worker`) and whether Sandboxes of that kind are
  supported, or the refusal text the product returns.

`layerfs machine platforms` (the call `machine.platforms`) returns the same
data for the running host.

## Adding things

| To add | Touch |
| --- | --- |
| a platform | `platforms/<p>/`; one `Platform` variant; one `adapter()` arm; a `Guest` variant only for a new shell transport |
| a Sandbox backend | `backend/<b>/`; one `Backend` variant; one `adapter()` arm; one record type |
| a bucket provider | `layerfs-store-backup/src/bucket/<p>/` |

A Store kind and a kernel route are engine changes; see #535 §9.
