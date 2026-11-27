# Agent rules for control

> **Status:** Current general guide (#535 connect layer; renamed control and
> split by concern, layerfs-project#101, owner 2026-11-06).

Read [repository instructions](../AGENTS.md) first. The control layer holds
the management code and everything that reaches a machine or the network on
behalf of the engine:

- `crates/layerfs-home`: the home and its records, knowing no entity and
  no driver (#103, #112): layout, paths, record files, locks, the `op`
  intent, typed names, outcomes, secrets, the call trace and the process
  identity records name (over `layerfs_os::process`).
- `crates/layerfs-machine`: machine records, their lifecycle, the sealed
  bundle and the worker protocol and client (#104). What a machine holds
  comes from its caller (`Related`). Since #828 it also holds *where*
  Sandboxes can run (was `layerfs-platform`): one folder per platform under
  `src/platform/` (`native`, `docker`, `apple_container`, `lima`), built
  only with the `machine` feature (host builds), and the tables machines and
  Sandboxes read, the support table included, under `src/table/`.
- `crates/layerfs-store`: Store records and calls (#105): the kind-neutral
  root (records, Store calls, placement, leases), `sqlite/` and
  `postgresql/` (which never import each other; a test checks it),
  `backup/` and `project/`. Who uses a Store comes from its caller
  (`Related`). Since #828 `src/backup/` also holds *how* a Store is backed
  up to a bucket (was `layerfs-bucket`): `bucket/` and one folder per
  provider under `providers/`.
- `crates/layerfs-sandbox`: Sandboxes (#106): records, launch, host
  operations, exec and the daemon channel (`daemon/`). Since #828 it also
  holds *how* a Sandbox runs on that machine's kernel (was
  `layerfs-sandbox-backend`; before that `layerfs-isolation`, D-9):
  `src/runtime/` is the backend-neutral surface (`Runtime`, `Sandbox`,
  cancel, trace, shared types); `src/spawn/` the process lifecycle both
  backends use; one folder per backend under `src/backend/` (`process`,
  the `native` backend, and `namespaces`) holds everything only that
  backend uses, its OS facts included (macOS runs only `native`, Linux only
  `namespaces`). The docker backend and its Engine client were retired by
  D-8 (layerfs-project#111): the `docker` platform runs `namespaces` in its
  worker container. Host and guest builds. Its `layerfs-sandbox-helper`
  binary (`src/bin/`) keeps its name.
- `crates/layerfs-workspace` (#828; was `layerfs-sandbox`'s `workspace/`
  module): a Sandbox's Workspaces: the `workspace` calls, their executing
  side (`machine/`) and the unmount guard. `layerfs-sandbox` never depends
  on it; a host operation's local runner is passed in
  (`route::machine::send::Local`).
- `crates/layerfs-inspect` (was `layerfs-control`, #828): what spans
  entities (#536; moved here from `api/crates` by #101 D-2): the worker
  server, `inventory`, `doctor`, and the machine, Store, project and backup
  calls with the `Related` reads they need from other entities. The call
  machinery (`calls/`) and `Context` are `layerfs-api`'s (#107): control
  functions take the home and, when they emit, `layerfs-home`'s `Events`.

See the [control README](README.md) for the layout and how to read
`SUPPORT.json`, [#535](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/535/README.md) for the drivers'
design and the [compute plane 2.0 spec](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/proposals/compute-plane-2.0.md)
§6 for the split.

## Dependencies

- Layers are `core < engine < control < api`. Control crates may depend on
  engine and core crates, including dev dependencies and test includes.
  **Nothing in `engine/` or `core/` may depend on `control/`**; the
  [boundary guard](../tools/check_product_boundary.py) refuses the edge, and
  unknown crates fail closed. `api/` depends on control, never the reverse.
- Inside control the order is `layerfs-home < layerfs-machine <
  layerfs-store < layerfs-sandbox < layerfs-workspace < layerfs-inspect`:
  `layerfs-workspace ──► layerfs-sandbox ──► layerfs-store ──►
  layerfs-machine ──► layerfs-home`, and `layerfs-inspect ──►
  layerfs-sandbox`. Edges that skip down the chain are allowed
  (`layerfs-workspace`, `layerfs-inspect` and `layerfs-sandbox` reach
  `layerfs-machine`, `layerfs-store` and `layerfs-home` directly); nothing
  points up. What an entity reads from the entities above it comes through
  a trait: `layerfs_machine::MachineUsers` and `layerfs_store::StoreUsers`,
  implemented in `layerfs_sandbox::users` (`MACHINE`, `STORE`); callers
  take `&dyn MachineUsers` and `&dyn StoreUsers`. `layerfs-machine`'s
  platform adapters build only with its optional `machine` feature, which
  `layerfs-store`, `layerfs-sandbox`, `layerfs-workspace`,
  `layerfs-inspect` and `layerfs-api` pass through.
  `layerfs-home` reaches no driver (#112): the process identity it and the
  Sandbox backends share (`LocalProcess` and its `/proc` and `proc_pidinfo`
  probe) is `layerfs_os::process`, in the engine crate below both.
- The driver modules (`layerfs-machine` `platform/`, `layerfs-sandbox`
  `backend/`, `layerfs-store` `backup/`) take every input as arguments, know nothing of the home and write
  no records (#101 rule 6).

## Extension points

Each extension point is **one crate-private, sealed trait with no default
methods, reached through one exhaustive `match`** (#535 G2): `Adapter` for a
platform, `BackendAdapter` for a Sandbox backend. There is no public provider
trait, registry, factory or service locator. Adding a platform is one new
folder under `platform/`, one `Platform` variant and one `adapter()` arm;
adding a backend is one folder under `backend/`, one `Backend` variant and one
arm. Every refusal of a platform, host or Store combination comes from the one
support table (`layerfs-machine` `table/support.rs`), which consumes the shared permanent restrictions in
`layerfs-machine/src/table/removed.rs` (also built without the `machine`
feature); `SUPPORT.json` is generated
from it and a test compares them.

## Rules that stay

- The routes, worker and Store-kind rules are the root guide's
  [Where calls run](../AGENTS.md#where-calls-run); control code implements
  them in `place.rs` and `support.rs`.
- A PostgreSQL password is never written into a record, log, trace or
  `Debug` output.
- Stored and displayed values do not follow internal renames: the
  `Native` backend is stored and shown as `"native"` (#535 G6).

Control follows the engine guide's
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks](../engine/AGENTS.md#checks-and-completion), and the root guide's
[rules you must not break](../AGENTS.md#rules-you-must-not-break). The unsafe
allowance in control stays the one file
the guard names: the Linux `backend/namespaces/seccomp.rs` of
`layerfs-sandbox`. The macOS process facts (`proc_pidinfo`) moved
with `LocalProcess` to the audited `process/darwin.rs` of the bridge (#112),
now `layerfs-os` (#828).
