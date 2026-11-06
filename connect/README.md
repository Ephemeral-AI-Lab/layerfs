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

Firecracker and SSH are **permanently disabled for new product work**. Their
adapters remain compiled; inspection and explicit cleanup remain available.
The [restriction contract](../docs/architecture/connect/support-matrix/disabled-platforms.md)
defines existing-resource handling and re-enablement. Platform profiles below
describe retained mechanics; `SUPPORT.json` decides availability.

## Reading the tree

- `layerfs-machine/src/platform.rs` is the `Platform` enum and its `Profile`
  type: per platform, the hosts it runs on, the Sandbox backend and kernel
  route per host, whether its sqlite Store calls run in process or in its
  worker, and the options `machine create` or `machine connect` accept
  (`options.rs`). `Platform::profile()` is the one match.
- `layerfs-machine/src/adapter.rs` is the sealed `Adapter` trait (probe,
  guest, create/remove, start/stop, status, host identity, reach, forward)
  and the one `adapter()` match the machine calls dispatch through.
- `layerfs-machine/src/reach.rs` is `reach` (the call `machine::reach`):
  the PostgreSQL Store URL a Sandbox daemon on the machine dials. Each
  adapter names how its machine reaches the caller's loopback: `native`
  shares it; `docker` (`host.docker.internal`) and `lima`
  (`192.168.5.2`, the address of `host.lima.internal`) rewrite a loopback
  host to their alias, and so do
  `apple-container` (its network's gateway from `container inspect`),
  `microsandbox` (`host.microsandbox.internal`; machines get
  `--net public,private,host`) and `firecracker` (`192.168.5.2`, through the Lima
  host's masquerade); `ssh` refuses it. Facts: the G7 receipt in
  `core/docs/issues/535/receipts/g7-reach/`. A
  rewrite is refused for `sslmode=verify-full`; other hosts are used as
  given.
- `machine::forward` (#535 G4, phase 6) is the host loopback address at
  which the caller reaches a Sandbox daemon listening on
  `127.0.0.1:<guest port>` in the machine, so Workspace calls skip the
  worker; `machine::forward_close` ends what it started. `native` answers
  the port as it is. `ssh` and `lima` start a detached
  `ssh -N -L 127.0.0.1:<free port>:127.0.0.1:<guest port>`
  (`setup/forward.rs`, the worker forward's code; lima through its own
  `ssh.config`, as its template forwards no guest port but the worker's)
  and answer its pid. `docker` refuses `InvalidInput`: each docker Sandbox
  publishes its daemon's port on the host's loopback itself, and that is
  its endpoint. `apple-container` and `microsandbox` (ports are published
  only at create) and `firecracker` (its daemons listen on the microVM's
  loopback; the Lima host's forward reaches only its tap address) refuse
  `Unsupported`, and their Workspace calls stay on the worker.
- `layerfs-machine/src/support.rs` is the one support table:
  platform × host × Store kind → supported, or a refusal with its reason.
  It consumes the fixed restrictions in `layerfs-sandbox/src/platform_policy.rs`,
  also used by the API dispatcher in builds without machine adapters.
- `layerfs-machine/src/platforms/<p>/`: `profile.rs` holds its `PROFILE`
  and `adapter.rs` its `ADAPTER` (the guard keeps both out of `mod.rs`);
  `observe.rs`, `create.rs` (create and remove), `power.rs` (start and
  stop); ssh has `connect.rs` (connect and disconnect) instead of
  `create.rs`. `network.rs` holds the docker worker's published port.
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

The table encodes what is allowed; Firecracker and SSH now refuse new work.
Historical enablements remain evidence only: #535 phase 5a enabled PostgreSQL
Sandboxes on `native` (macOS and Linux), `docker` and `ssh`; the G7 live
receipts (`core/docs/issues/535/receipts/g7-live/`) enabled them on `lima`,
`apple-container`, `microsandbox` and `firecracker`. `layerfs machine platforms` (the call `machine.platforms`)
returns the same data for the running host.

## Adding things

| To add | Touch |
| --- | --- |
| a platform | `platforms/<p>/` (with `profile.rs` and `adapter.rs`); one `Platform` variant; one `profile()` and one `adapter()` arm; its rows in `support.rs`; a `Guest` variant only for a new shell transport |
| a Sandbox backend | `backend/<b>/`; one `Backend` variant; one `adapter()` arm; one record type |
| a bucket provider | `layerfs-store-backup/src/bucket/<p>/` |

A Store kind and a kernel route are engine changes; see #535 §9.

## Conformance kit

Each crate has one `tests/conformance.rs` (#536 §11 step 9) that every
platform or backend passes, with stand-in tools and no Docker Engine, VM,
ssh host or root. The machine kit runs over `Platform::ALL`: the profile
against `support.rs` and `SUPPORT.json`, each lifecycle call running
through the platform's tool or refused with the fixed text its `How`
declares, `reach` and `forward` against the rules above, and the declared
options against `Spec::validate`. The Sandbox kit runs over `Backend::ALL`:
the stored name, capabilities against `SUPPORT.json`'s kernel routes,
binds, the runtime this host has and the lifecycle it can run here. A new
variant is covered automatically; the kits' exhaustive matches (`loopback`,
`daemon_port`; `binds`, `options`, `record`, `runtime`, `lifecycle`) do not
compile until it declares its answers. Run them with `cargo +1.85.1 test --locked -p
layerfs-machine --test conformance` and `-p layerfs-sandbox --test
conformance`; real platforms run them in the terminal pass.
