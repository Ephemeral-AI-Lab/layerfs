# Agent rules for control

> **Status:** Current general guide (#535 connect layer; renamed control and
> split by concern, layerfs-project#101, owner 2026-11-06).

Read [repository instructions](../AGENTS.md) first. The control layer holds
the management code and everything that reaches a machine or the network on
behalf of the engine:

- `crates/layerfs-home`: the home and its records, knowing no entity
  (#103): layout, paths, record files, locks, the `op` intent, typed names,
  outcomes, secrets, the call trace and the process identity.
- `crates/layerfs-machine`: machine records, their lifecycle, the sealed
  bundle and the worker protocol and client (#104). What a machine holds
  comes from its caller (`Related`).
- `crates/layerfs-store`: Store records and calls (#105): the kind-neutral
  root (records, Store calls, placement, leases), `sqlite/` and
  `postgresql/` (which never import each other; a test checks it),
  `backup/` and `project/`. Who uses a Store comes from its caller
  (`Related`).
- `crates/layerfs-control`: the management library (#536; moved here from
  `api/crates` by #101 D-2): Sandboxes and their Workspaces, and what spans
  entities (the worker server, `inventory`, `doctor`, `migrate`, and the
  machine, Store, project and backup calls with their `Related` reads).
  #101 continues the split into `layerfs-sandbox`.
- `crates/layerfs-platform` (was `layerfs-machine`): *where* Sandboxes can
  run. One folder per platform under `src/platforms/` (`native`, `docker`,
  `apple_container`, `microsandbox`, `lima`, `firecracker`, `ssh`). Host
  builds only.
- `crates/layerfs-isolation` (was `layerfs-sandbox`): *how* a Sandbox runs
  on that machine's kernel. One folder per backend under `src/backend/`
  (`docker`, `host_process`, `namespaces`), over the shared local-process
  plumbing in `src/local/`. Host and guest builds. Its
  `layerfs-sandbox-helper` binary keeps its name.
- `crates/layerfs-bucket` (was `layerfs-store-backup`): *how* a Store is
  backed up to a bucket (`src/backup/` and `src/bucket/<provider>/`).

See the [control README](README.md) for the layout and how to read
`SUPPORT.json`, [#535](../core/docs/issues/535/README.md) for the drivers'
design and the [compute plane 2.0 spec](../core/docs/proposals/compute-plane-2.0.md)
§6 for the split.

## Dependencies

- Layers are `core < engine < control < api`. Control crates may depend on
  engine and core crates, including dev dependencies and test includes.
  **Nothing in `engine/` or `core/` may depend on `control/`**; the
  [boundary guard](../tools/check_product_boundary.py) refuses the edge, and
  unknown crates fail closed. `api/` depends on control, never the reverse.
- Inside control: `layerfs-control ──► layerfs-store ──► layerfs-machine
  ──► layerfs-home`; `layerfs-store ──► layerfs-bucket`; `layerfs-control
  ──► layerfs-isolation, layerfs-bucket`; `layerfs-machine
  ──► layerfs-platform` only through its optional `machine` feature, which
  `layerfs-control` and `layerfs-api` pass through; `layerfs-home ──►
  layerfs-isolation` (the process identity records name); `layerfs-platform ──►
  layerfs-isolation ──► layerfs-bridge`; `layerfs-bucket ──►
  layerfs-store-service, layerfs-persistence, layerfs-storage`.
- Drivers take every input as arguments, know nothing of the home and write
  no records (#101 rule 6).

## Extension points

Each extension point is **one crate-private, sealed trait with no default
methods, reached through one exhaustive `match`** (#535 G2): `Adapter` for a
platform, `BackendAdapter` for a Sandbox backend. There is no public provider
trait, registry, factory or service locator. Adding a platform is one new
folder under `platforms/`, one `Platform` variant and one `adapter()` arm;
adding a backend is one folder under `backend/`, one `Backend` variant and one
arm. Every refusal of a platform, host or Store combination comes from the one
support table (`layerfs-platform` `support.rs`), which consumes the shared permanent restrictions in
`layerfs-isolation/src/platform_policy.rs` (also available without machine
adapters); `SUPPORT.json` is generated
from it and a test compares them.

## Rules that stay

- sqlite Store calls run in a process on the Store's kernel (in process on
  `native`, else that machine's worker); PostgreSQL Store calls and bucket
  I/O run in the caller's process (#533 G2). A sqlite Store is on exactly one
  machine; a PostgreSQL Store is usable from every machine the support
  table allows, with no attach records (#535 G3).
- Workers listen on loopback behind a secret; one attempt per call, no
  retry, no replay; an uncertain outcome is reported, never resent.
- A PostgreSQL password is resolved only in the caller, travels only in one
  worker request and the Sandbox's private daemon setup file, and is never
  written into a record, log, trace or `Debug` output.
- Stored and displayed values do not follow internal renames: the
  `host_process` backend is stored and shown as `"native"` (#535 G6).

All control product code follows the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](../engine/AGENTS.md#checks-and-completion):
production files at most 999 lines, `lib.rs`/`mod.rs` at most 200 lines of
declarations and re-exports, no tests or fixtures in `src/`, bounded test
commands, locked dependencies, per-commit production LOC and the
Disposable-only profile. The unsafe allowance stays the one macOS file the
guard names.
