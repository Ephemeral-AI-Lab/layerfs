# Limitations

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract.

Firecracker, Microsandbox and SSH machines were removed in 2.0 (owner,
2026-11-08); their code is gone. A record of one that a home still holds
shows in `machine list` and `machine get` as `state: "unsupported"` with
the reason (``platform `ssh` was removed in LayerFS 2.0``, or the same for
`firecracker` or `microsandbox`). Every other call on it is `Unsupported`
with that reason. `machine remove` forgets the record only; nothing on its
host is touched, so remove a microsandbox VM yourself with
`msb remove --force <vm>` and `msb volume remove <vm>-data`, and a
firecracker microVM's files from its Lima host (the note names them). See
[machines](machines-and-stores.md#removed-platforms-firecracker-microsandbox-and-ssh)
and [removed platforms](../../architecture/control/support-matrix/disabled-platforms.md).

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
  all seven machine kinds (owner direction, 2026-11-02; Firecracker,
  Microsandbox and SSH were removed afterwards, on 2026-11-08). See
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
| A machine, Sandbox or Store call on a removed platform's record (`firecracker`, `ssh`, `microsandbox`) | ``platform `<name>` was removed in LayerFS 2.0``; only `machine list`, `machine get` and `machine remove` (forgets the record) answer |
| A Sandbox on a platform and Store kind the support table refuses (`layerfs machine platforms`, `control/SUPPORT.json`): docker on Windows | the table's reason, word for word, such as "docker through WSL2 is not designed yet" |
| A Sandbox on a machine other than its sqlite Store's | names the Store's machine; `store pull` and `store push` move the Store |
| `project remove` from a PostgreSQL Store | "removing a project from a PostgreSQL Store is not built" |
| `backup restore` over a Store that is not on `native` | `store pull` it to `native` first |
| Resizing a VM with `machine update` | "not built yet; nothing was changed" (`InUse` first if Sandboxes run on it) |
| Creating a machine without the sealed Linux bundle | names the expected folder in `detail.missing` (`bundle:<folder>`); fetching the bundle into the cache is not built, a build places it |
| A machine kind whose tool is missing | `detail.missing`, such as `tool:limactl` |
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
- **A cancel that a worker takes and never answers has no time limit.**
  `workspace exec-cancel` (and an unmount or close that cancels first)
  waits for the worker's answer until its connection closes, then answers
  `unknown` and keeps the command's record; there is no operation timeout.
- **A cancel before a command starts, on a Sandbox this host reaches
  directly.** It answers `cancelled` (`SIGKILL`) at once; the exec's own
  process ends the command in the worker as it starts, in one attempt.
  If the worker cannot be reached for that one attempt, the command runs
  on although the cancel answered `cancelled`; cancel it again by its id.
- **A command's children on macOS `native`.** `workspace exec-cancel` ends
  the command's process group; a child that starts its own session
  (`setsid`) is not in it and keeps running. On Linux every command has its
  own cgroup and nothing it starts escapes.
- **`workspace exec-cancel` on a Linux Docker Engine** was not run for this
  manual (no such host; layerfs-project#114). The docker machine on Docker
  Desktop (macOS) and `native` were.
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

- **VM kinds.** A `lima` (Lima 2.2.1, vz) and an `apple-container`
  (container 1.6.1) machine were created, started, stopped and started
  again on macOS with the release CLI, and ran the sqlite smoke and
  `workspace exec-cancel` (on Lima also stale and unreachable forwarded
  records); PostgreSQL Sandboxes on them were not run in that pass
  ([receipt](../../../core/docs/issues/project-113/receipts/fb02aa0666/machines-live/RESULTS.md)). On macOS the `docker` machine's
  worker did run (a private test bundle, `LAYERFS_BUNDLE`): `store move` (now `store push`) to
  `docker`, its Sandboxes and the twelve cells passed; see the terminal pass.
  That run used the retired docker backend: the `docker` machine running
  `namespaces` Sandboxes in its worker container (D-8) has not been run
  live.
- **Machine boot and worker start.** `machine start` and `machine stop`
  (#535 P1) and a worker call refused `Unavailable` on a stopped machine
  (D11, R1) were tested with stand-in tools and workers, and live on the
  `lima` and `apple-container` machines above; no Docker
  worker was started or stopped by them. There is no idle
  stop and no start on demand (D11).
- **The docker default image** (#535 P5). Loading the bundle's
  `sandbox-image.tar` with `docker load` and running the worker container
  on the id it reports was tested with a stand-in `docker`
  only; a real Engine (Docker Desktop's classic or containerd image store)
  was not used.
- **One worker serving several Stores** and the guest-to-host send speed.
- **Linux.** No Linux host was used. Linux namespaces Sandboxes, Init into
  `docker` on Linux, the Linux route of the `docker` machine (its worker,
  as on macOS: #535 G8) and the Linux arm64 wheel (O20) are `NOT_RUN`. A
  Linux x64 wheel, addon and command were built and installed outside the
  checkout (sqlite Stores; PostgreSQL Stores only in the opt-in feature build), in
  [artifact-qualification-2.0.0.md](../../general/artifact-qualification-2.0.0.md).
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
- `project init --progress` printed no progress lines for a small folder
  (a later build reports its phases on every route; not rerun).

## Seen in the code, not tested

- `file.apply` changes name their paths `path`, `from_path` and
  `to_path` and their texts `old`, `new` and `content`. The Store's own
  names (`from`, `to`, `oldText`, `newText`, `bytes`) are refused
  `InvalidInput`, so a folder scope checks every path a change names.
- `workspace exec --stream` and the Node and Python `stdout`/`stderr`
  callbacks receive the output after the command ends, not live.
