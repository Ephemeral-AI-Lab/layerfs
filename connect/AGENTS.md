# Agent rules for connect

> **Status:** Current general guide (#535; the crates arrive in its phase 1).

Read [repository instructions](../AGENTS.md) first. The connect layer holds
everything that reaches a machine or the network on behalf of the engine:

- `crates/layerfs-machine`: *where* Sandboxes can run. One folder per
  platform under `src/platforms/` (`native`, `docker`, `apple_container`,
  `microsandbox`, `lima`, `firecracker`, `ssh`). Host builds only.
- `crates/layerfs-sandbox`: *how* a Sandbox runs on that machine's kernel.
  One folder per backend under `src/backend/` (`docker`, `host_process`,
  `namespaces`), over the shared local-process plumbing in `src/local/`.
  Host and guest builds.
- `crates/layerfs-store-backup`: *how* a Store is backed up to a bucket
  (`src/backup/` and `src/bucket/<provider>/`).

See the [connect README](README.md) for the layout and how to read
`SUPPORT.json`, and [#535](../core/docs/issues/535/README.md) for the design.

## Dependencies

- Layers are `core < engine < connect < api`. Connect crates may depend on
  engine and core crates, including dev dependencies and test includes.
  **Nothing in `engine/` or `core/` may depend on `connect/`**; the
  [boundary guard](../tools/check_product_boundary.py) refuses the edge, and
  unknown crates fail closed.
- Inside connect: `layerfs-machine ──► layerfs-sandbox ──► layerfs-bridge`;
  `layerfs-store-backup ──► layerfs-store-service, layerfs-persistence,
  layerfs-storage`. `layerfs-api` reaches `layerfs-machine` only through its
  optional `machine` feature.

## Extension points

Each extension point is **one crate-private, sealed trait with no default
methods, reached through one exhaustive `match`** (#535 G2): `Adapter` for a
platform, `BackendAdapter` for a Sandbox backend. There is no public provider
trait, registry, factory or service locator. Adding a platform is one new
folder under `platforms/`, one `Platform` variant and one `adapter()` arm;
adding a backend is one folder under `backend/`, one `Backend` variant and one
arm. Every refusal of a platform, host or Store combination comes from the one
support table (`layerfs-machine` `support.rs`), which consumes the shared permanent restrictions in
`layerfs-sandbox/src/platform_policy.rs` (also available without machine
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

All connect product code follows the
[production size and responsibility rules](../engine/AGENTS.md#production-file-size-and-responsibility)
and [checks and completion rules](../engine/AGENTS.md#checks-and-completion):
production files at most 999 lines, `lib.rs`/`mod.rs` at most 200 lines of
declarations and re-exports, no tests or fixtures in `src/`, bounded test
commands, locked dependencies, per-commit production LOC and the
Disposable-only profile. The unsafe allowance stays the one macOS file the
guard names.
