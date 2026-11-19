# LayerFS 2.0.0 manual

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

This manual describes what is implemented on `layerfs_v2_0_0` with the 2.0
call surface (branch `call-surface-2-0`; the frozen specification is
[`core/docs/issues/call-surface-2-0/`](../../../core/docs/issues/call-surface-2-0/README.md),
version 19), which grew out of
[#533](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/533/README.md). It is not a release
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
  built in; VM kinds (`apple-container`, `lima`) are created by name.
  Firecracker, Microsandbox and SSH machines were removed in 2.0. Storage is
  Stores: a sqlite Store is one file on one machine, a postgresql Store is a
  database. Backups of sqlite Stores go to r2 or MinIO buckets. See
  [machines, Stores and backups](machines-and-stores.md).
- **One call table.** 83 calls in ten families: `home`, `project`, `branch`,
  `commit`, `file`, `sandbox`, `workspace`, `machine`, `store` and `backup`.
  The `layerfs` command, the Node package, the Python package and the MCP
  server are thin front ends over one Rust library and take the same
  arguments. Every call answers a short form, or its long form with
  `--detail`; things carry one `state`; a refusal is one of thirteen codes
  with a `fix` line when there is one; six doctors check without changing
  anything. See [the calls](calls.md).
- **Projects are `<store>/<project>`.** Init imports a host folder into an
  existing Store. There is no default project.
- **Sandboxes are named.** A Sandbox is bound to one project and runs on a
  named machine. A Workspace is written `<sandbox>:<name>`.

## Contents

1. [Quickstart](quickstart.md): one home, a Store, Init, Branch and file
   calls, a Sandbox and a Workspace, a Commit; in the shell, JavaScript and
   Python.
2. [The home](home.md): the layout of `LAYERFS_HOME`, and what an older
   LayerFS's home gets (it is not converted).
3. [The calls](calls.md): all 83 calls with their positionals and options,
   the form of their answers, the doctors, errors and exit codes, the shell
   switches and the folder scope.
4. [Machines, Stores and backups](machines-and-stores.md): machine kinds,
   manual start and stop, sqlite and postgresql Stores, moving a Store, and
   backups.
5. [Node and Python](sdk.md): the `@layerfs/node` and `layerfs` packages, clients, options,
   errors, progress and folder scope.
6. [Limitations](limitations.md): what is not built, not verified or refused
   as `Unsupported`.
7. [Migrating from 1.1.0 and #521](migration.md): each old package,
   constructor, method and behaviour mapped to its 2.0 replacement, or to
   why it is gone.
8. [Examples](examples/README.md): runnable Node, Python (sync and async) and
   shell flows, with their output, and the Workspace route, all run for this
   version of the manual.

## Sources

The code wins over every document below where they differ.

- [The call table](../../../api/crates/layerfs-api/src/), one file per call
  under its family (`<family>/<call>.rs`): the 83 calls, their arguments,
  options and the form of their answers (`layerfs help --json` prints it).
- [The frozen call surface specification](../../../core/docs/issues/call-surface-2-0/README.md):
  each call's row (`calls.json`) and the rulings (`DECISIONS.md`); a
  contract test compares the built table with the rows.
- [The #533 work order](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/533/README.md) and
  [its decisions log](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/issues/533/DECISIONS.md): owner
  decisions and the implementation choices taken on this branch.
- The two design proposals:
  [the `layerfs` command and SDK](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/proposals/layerfs-sdk-cli.md)
  (called S) and
  [one control center for every platform](https://github.com/Ephemeral-AI-Lab/layerfs/blob/af2ad92bce866b4e3e11365f016b12806828a53b/core/docs/proposals/host-platform-connections.md)
  (called H). The family named `fs` in S is `file` in the code (O13).
- [The JavaScript package guide](../../../api/README.md) and
  [the Python package guide](../../../api/python/README.md).

## Compatibility with 1.1.0

2.0 does not keep the 1.1.0 surface ([migration](migration.md) has a row for
each old name):

- **Packages.** The four 1.1.0 packages (`@layerfs/agent`, `@layerfs/cli`,
  `@layerfs/backend`, `@layerfs/runtime`) are removed. Their last source is
  the 1.1.0 tag. There is one package for Node (`@layerfs/node`), one for
  Python (`layerfs`), one MCP server (`@layerfs/mcp`) and one native command,
  `layerfs`.
- **Entry points.** `LayerFS.init` and `LayerFS.open` are removed with no
  alias (D5). Every call names its project as `<store>/<project>`.
- **Calls.** 1.1.0's `fs` family is now `file`. The old Store calls, the
  start/get job pairs, the 14 `@layerfs/backend` operator and diagnostic
  calls (including background processes) and the trace subscription are
  removed (O9, O22, O25, O26). `project.delete`, `workspace.status` and
  `machine.update` are added (O24, O14, O11).
- **Results.** Result keys are snake_case in every front end (`exit_code`,
  not `exitCode`).
- **Settings.** Only `LAYERFS_HOME` and the trace and measurement settings
  are read; a 1.1.0 environment is not imported.
- **Homes.** An older LayerFS's home (1.1.0, or 2.0 layout 2) is not
  converted (owner, 2026-11-19): every call refuses it with the fix
  `mv <home> <home>.old && layerfs home init`. See
  [the home](home.md#an-older-home).

The Store profile changed: every sqlite Store is opened with WAL and `synchronous=NORMAL` (owner,
2026-11-17, ahead of its safety gate, so no power-loss guarantee is
claimed); its record's profile label stays `disposable` (1.1.0 used
`synchronous=OFF`). Durable execution is `NOT_RUN — disabled by owner until
explicit reauthorization`. Every call is one attempt: there is no retry,
busy handler or replay.
