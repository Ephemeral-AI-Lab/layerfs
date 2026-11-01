# LayerFS 2.0.0 manual

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

This manual describes what is implemented on branch `issue533-v2`
([#533](../../../core/docs/issues/533/README.md)). It is not a release
receipt: nothing here is tagged, published to npm, PyPI or crates.io, or
covered by a terminal pass. Each page says what was run, what was not run
and what is not built.

## What 2.0 is

LayerFS keeps versioned project files in Stores. Programs fork Branches,
change files directly or in a mounted Workspace where ordinary commands run,
Commit, merge, and publish to `main`.

2.0 changes how a host reaches all of this:

- **One home, one setting.** Every record lives under `LAYERFS_HOME`
  (default `~/.layerfs`). The 1.1.0 platform settings are gone. See
  [the home](home.md).
- **Machines and Stores.** Compute is machines: `native` and `docker` are
  built in; VM kinds are created by name; ssh hosts are connected. Storage is
  Stores: a sqlite Store is one file on one machine, a postgresql Store is a
  database. Backups of sqlite Stores go to r2 or MinIO buckets. See
  [machines, Stores and backups](machines-and-stores.md).
- **One call table.** 57 calls in nine families: `project`, `branch`,
  `commit`, `file`, `sandbox`, `workspace`, `machine`, `store` and `backup`.
  The `layerfs` command, the Node package and the Python package are thin
  front ends over one Rust library and take the same arguments. See
  [the calls](calls.md).
- **Projects are `<store>/<project>`.** Init imports a host folder into an
  existing Store. There is no default project.
- **Sandboxes are named.** A Sandbox is bound to one project and runs on a
  named machine. A Workspace is written `<sandbox>:<path>`.

## Contents

1. [Quickstart](quickstart.md): one home, a Store, Init, Branch and file
   calls, a Sandbox and a Workspace, a Commit; in the shell, JavaScript and
   Python.
2. [The home](home.md): the layout of `LAYERFS_HOME`, the one-time
   conversion of a 1.1.0 home, and `layerfs import-env`.
3. [The calls](calls.md): all 57 calls with their positionals and options,
   results, errors and exit codes, and the shell switches.
4. [Machines, Stores and backups](machines-and-stores.md): machine kinds,
   start on demand and idle stop, sqlite and postgresql Stores, moves and
   backups.
5. [Node and Python](sdk.md): the `layerfs` packages, clients, options,
   errors, progress and folder scope.
6. [Limitations](limitations.md): what is not built, not verified or refused
   as `Unsupported`.

## Sources

The code wins over every document below where they differ.

- [The call table](../../../engine/crates/layerfs-api/src/calls/table.rs):
  the 57 calls, their arguments and options.
- [The #533 work order](../../../core/docs/issues/533/README.md) and
  [its decisions log](../../../core/docs/issues/533/DECISIONS.md): owner
  decisions and the implementation choices taken on this branch.
- The two design proposals:
  [the `layerfs` command and SDK](../../../core/docs/proposals/layerfs-sdk-cli.md)
  (called S) and
  [one control center for every platform](../../../core/docs/proposals/host-platform-connections.md)
  (called H). The family named `fs` in S is `file` in the code (O13).
- [The JavaScript package guide](../../../api/README.md) and
  [the Python package guide](../../../api/python/README.md).

## Compatibility with 1.1.0

2.0 does not keep the 1.1.0 surface:

- **Packages.** The four 1.1.0 packages (`@layerfs/agent`, `@layerfs/cli`,
  `@layerfs/backend`, `@layerfs/runtime`) are removed. Their last source is
  the 1.1.0 tag. There is one package, `layerfs`, for Node and for Python,
  and one native command, `layerfs`.
- **Entry points.** `LayerFS.init` and `LayerFS.open` are removed with no
  alias (D5). Every call names its project as `<store>/<project>`.
- **Calls.** 1.1.0's `fs` family is now `file`. The old Store calls, the
  start/get job pairs, the 14 `@layerfs/backend` operator and diagnostic
  calls (including background processes) and the trace subscription are
  removed (O9, O22, O25, O26). `project.remove`, `workspace.status` and
  `machine.update` are added (O24, O14, O11).
- **Results.** Result keys are snake_case in every front end (`exit_code`,
  not `exitCode`).
- **Settings.** Only `LAYERFS_HOME` and the trace and measurement settings
  are read. `layerfs import-env` writes a 1.1.0 environment as records once.
- **Homes.** A 1.1.0 home is converted once, on its first 2.0 call. See
  [the home](home.md#a-110-home).

The Store profile is unchanged: every sqlite Store is Disposable (WAL,
`synchronous=OFF`). Durable execution is `NOT_RUN — disabled by owner until
explicit reauthorization`. Every call is one attempt: there is no retry,
busy handler or replay.
