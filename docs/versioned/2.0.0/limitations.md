# Limitations

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Part of the [2.0.0 manual](README.md). This page lists what is not built,
what the code refuses as `Unsupported`, and what was not verified on branch
`issue533-v2`. Sources: the "Implementation choices" of
[DECISIONS.md](../../../core/docs/issues/533/DECISIONS.md), the refusals in
`engine/crates/layerfs-api/src/`, the step commits' checks, and the runs
for this manual.

## Release state

- 2.0.0 is not tagged or released. No package is published to npm, PyPI or
  crates.io.
- The terminal pass of the work order (step 19) ran on one Mac only: the
  twelve cells passed on `native` and (at `a487409fe5`) on the macOS
  `docker` machine, as did MinIO backups, PostgreSQL Init and Sandboxes,
  and the three front ends compared; every VM kind, ssh, R2, Linux and
  Windows were `NOT_RUN`. Step 19 passes only when the twelve cells pass on
  all seven machine kinds (owner direction, 2026-11-02). See
  [TERMINAL-PASS.md](../../../core/docs/issues/533/TERMINAL-PASS.md) for
  the latest pass.
- The workspace's Cargo version is `2.0.0` (since `085376a7a8`), as are
  the Node package and the Python wheel; the bundle and daemon cache paths
  use `<cache>/layerfs/bundle/2.0.0/`.
- Only the Disposable Store profile runs. Durable execution is `NOT_RUN —
  disabled by owner until explicit reauthorization`.
- Every call is one attempt. There is no retry, busy handler or replay.

## Not built (refused as `Unsupported`)

| What | Refusal |
| --- | --- |
| A Sandbox on a platform and Store kind the support table refuses (`layerfs machine platforms`, `connect/SUPPORT.json`): for PostgreSQL, Linux `native` and ssh until #535 phase 5a; `lima`, `apple-container`, `microsandbox` and `firecracker` until a live receipt (G7); docker on Windows | the table's reason, word for word, such as "lima host alias not verified (G7)" |
| On Linux, a `docker` Sandbox of a sqlite Store by a caller that is not root | the daemon (root in its container) must own the bind-mounted Store folder; run as root, or keep the Store on `native` |
| A Sandbox on a machine other than its sqlite Store's | names the Store's machine; `store move` moves the Store |
| `project remove` from a PostgreSQL Store | "removing a project from a PostgreSQL Store is not built" |
| `project init` into a sqlite Store that is not on `native` | move it to `native`, Init, then move it back |
| `backup restore` over a Store that is not on `native` | move it to `native` first |
| Resizing a VM with `machine update` | "not built yet; nothing was changed" (`InUse` first if Sandboxes run on it) |
| Creating or connecting a machine without the sealed Linux bundle | names the expected folder in `detail.missing` (`bundle:<folder>`); fetching the bundle into the cache is not built, a build places it |
| A machine kind whose tool is missing | `detail.missing`, such as `tool:msb` |
| PostgreSQL calls in a build without the `postgresql` feature | "this LayerFS build has no PostgreSQL engine" |
| `machine` calls in a build without the `machine` feature (the guest worker's build) | names the #533 step |
| A `native` Sandbox on Linux started by a non-root user | namespaces need root for FUSE |
| A Store made without the acquisition tables projects are added through | LayerFS adds no tables to an existing Store |
| A home whose `layerfs.json` has a newer layout | "this build reads layout 2" |

Also not built:

- **Backups of PostgreSQL Stores** (`InvalidInput`: use PostgreSQL's own
  tools).
- **1.1.0's `local_folder` backup provider.** A 1.1.0 home's
  `local_folder` remote is kept only in `log/1.1.0/remotes`.
- **A live output stream for `workspace exec`.** Output reaches the
  `stdout` and `stderr` functions, `stream: true` and `--stream` after the
  command ends. A stream over 1 MiB comes only as output events: the reply
  has that stream `null` with its byte count (`stdout_bytes`,
  `stderr_bytes`) and `streamed: true`, so a caller with no function, no
  `stream` and no `--stream` does not get those bytes.
- **Cancelling a call.** `layerfs.aio` stops calling a cancelled call's
  functions, but the native call runs to its end on its thread; read the
  state again after a cancel.
- **`backup list` of a Store not on `native`.** Its heads are read on
  `native` only, so its state is `unknown` with a `reason`; reading heads
  through the machine's worker is not built.
- **Background processes, operator and diagnostic calls, and the trace
  subscription** of 1.1.0's `@layerfs/backend` (O25, O26). Trace files
  under `trace/` are the feed.
- **An MCP server** (O32). Folder scope is a client option instead.
- **A by-id Commit lookup.** `commit.get` reads the Branch's Commits page
  by page, following `next_page_pos` (a page holds what fits the reply
  budget, about 47 Commits), until it finds the id or prefix; its cost grows
  with the Branch's history.
- **Moving a Sandbox's own state.** Daemon keys and mounts stay under the
  project's folder, as in 1.1.0; only the 2.0 record is under
  `machine/<machine>/sandbox/<name>/`.
- **Per-call `--text` layouts.** `--text` is one generic readable form.

## Not verified

None of these was run on this branch, except where a line says what ran.
They are taken from the work order's "Not verified at writing" list and the
step commits.

- **VM kinds and ssh.** No real `apple-container`, `microsandbox`, `lima`,
  `firecracker` or ssh machine was created or connected; their adapters
  were tested with provider tests only. On macOS the `docker` machine's
  worker did run (a private test bundle, `LAYERFS_BUNDLE`): `store move` to
  `docker`, its Sandboxes and the twelve cells passed; see the terminal pass.
- **Machine boot, worker start and idle stop.** A guest powering off after
  10 idle minutes and the next call starting it again were not run.
  `machine start` and `machine stop` (#535 P1), and a worker call that
  starts its machine only after a dial that sent nothing (R1), were tested
  with stand-in tools and workers only; no VM, ssh host or Docker worker
  was started or stopped by them.
- **The docker default image** (#535 P5). Loading the bundle's
  `sandbox-image.tar` with `docker load` and running the worker container
  and Sandboxes on the id it reports was tested with a stand-in `docker`
  only; a real Engine (Docker Desktop's classic or containerd image store)
  was not used.
- **One worker serving several Stores** and the guest-to-host send speed.
- **Linux.** No Linux host was used. Linux namespaces Sandboxes, Init into
  `docker` on Linux, the Linux route of the `docker` machine (its worker,
  as on macOS: #535 G8) and the Linux wheels (O20) are `NOT_RUN`.
- **Windows** (O5). `native` on Windows holds only records; its sqlite
  Stores and Init are meant to live in a WSL worker. Not run. The library
  does not build for Windows yet: `home/`, `store/`, `worker/`, `start.rs`
  and the Sandbox family use `std::os::unix` without a cfg gate (review
  P1-28, paused for the owner; a Windows `cargo check` on this Mac stopped
  earlier, at the `zstd-sys` and `blake3` build scripts, which need an MSVC
  C toolchain).
- **R2.** Backups were run against MinIO only; R2 is `NOT_RUN` (no
  credentials).
- **PostgreSQL.** `store connect`, Init and Sandboxes on `native` and
  `docker` (macOS) ran against a throwaway `postgres:17` container. Init cost against a sqlite Init was
  not measured. The O6 handling of a LayerFS schema with another name is
  not visible in the connect code, which names only the schema `layerfs`.
- **Init on macOS's disk** against Init through Docker (owners, modes and
  name case) was not compared.
- **Deleted tests.** `layerfs-node`'s four Linux test files (21 tests) were
  deleted in step 14. The behaviours they guarded (#463 discovery reclaim,
  #508 admission and connection wait) are not re-covered on the 2.0 entry
  point. Step 14 also deleted the four 1.1.0 packages' 63 test files
  (listed in [HANDOFF.md](../../../core/docs/issues/533/HANDOFF.md)).

## Observed in the runs for this manual

On macOS arm64, with the debug build, in a private temporary home:

- `file.read` of a 3-byte binary file is refused `Binary`. Bytes can be
  written (`file.write` with a `Buffer` or `bytes`), but no 2.0 call reads
  them back as bytes.
- `project init --progress` printed no progress lines for a small folder.

## Seen in the code, not tested

- `file.apply` changes name their paths `path`, `from_path` and
  `to_path` and their texts `old`, `new` and `content`. The Store's own
  names (`from`, `to`, `oldText`, `newText`, `bytes`) are refused
  `InvalidInput`, so a folder scope checks every path a change names.
- `workspace exec --stream` and the Node and Python `stdout`/`stderr`
  callbacks receive the output after the command ends, not live.
