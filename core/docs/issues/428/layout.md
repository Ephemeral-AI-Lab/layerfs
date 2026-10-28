# #428 Expected layout after the SDK and worker consolidation

> **Status:** Proposal; target LayerFS release to be set by the owner; not a
> released contract.

Issue: [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428).
Designs: [layerfs-agent.md](layerfs-agent.md) and
[layerfs-backend-runtime.md](layerfs-backend-runtime.md).

This is the file and folder structure the SDK and the workers are expected to
have once [layerfs-agent.md](layerfs-agent.md) and
[layerfs-backend-runtime.md](layerfs-backend-runtime.md) are built. It is a
snapshot of the target, not a scaffolding requirement: existing source and the
package manifests still decide what is built.

**Later relocation.** The listings below keep the paths planned at commit
`1aa56ece78`. The folders have since moved: `sdk/js/packages/agent` to
`api/agent/sdk`, `sdk/js/packages/backend` to `api/backend`,
`sdk/js/packages/runtime` to `api/runtime`, the addon crate `sdk/js/native` to
`api/binding`, and the `layerfs` command from `cli/` to `api/agent/cli`. See
[api/README.md](../../../../api/README.md).

**Out of scope here:** `dsh/`, which stays as it is until its move to
dsh-layerfs is discussed. There is no UI package: dsh's routes, UI reads,
`model`, `view` and `client` stay in dsh-layerfs
([ui-frontend.md](ui-frontend.md)). Where runtime code
comes from a dsh Host file, the annotation names the file; the dsh copy is not
changed by this plan.

**Scope and counter.** Commit `1aa56ece78`. Production LOC from
`python3 tools/production_loc.py --files`, scopes `sdk` (`sdk/js/src`,
`sdk/js/operator`, `sdk/js/native/src`) and `dsh` (`dsh/packages/host/src`).
The counter does not count `.mjs` files; those carry `n/c`. Listings follow the
[layout rule](../../../../AGENTS.md#file-and-folder-layout-listings): `+` new,
`-` removed, a space for kept, edited or moved in. Tests stay outside `src/`
and are not counted: today's `sdk/js/test/` becomes one `test/` folder per
package.

## 1. Top level

`sdk/js/` becomes three packages beside the native addon crate. The folder
names match the package names. There is no separate shared package: the
client, the transports and the Store sharing live in `agent`, because
`backend` takes an opened agent client (`LayerFSBackend.of(shop)`) and
`runtime` builds on both.

| Folder under `sdk/js/` | Change | Prod LOC today | Listing |
| --- | --- | ---: | --- |
| `native/` | stays: the Rust addon crate `layerfs-node`, edited in place | 2,543 | section 6 |
| `src/` | removed: moves to `packages/agent/src/` | 485 | sections 2 and 5 |
| `operator/` | removed: `index.js` to `packages/backend/src/`, `inspect/snapshot.js` to `packages/runtime/src/project/state.js` | 62 | section 5 |
| `packages/agent/` | new: `@layerfs/agent` | — | section 2 |
| `packages/backend/` | new: `@layerfs/backend` | — | section 3 |
| `packages/runtime/` | new: `@layerfs/runtime` | — | section 4 |

Today's three folders add up to the counter's `sdk` total, 3,090.

```text
agent ◄── backend ◄── runtime
  │
  └──► layerfs.node (Linux), or the worker (macOS, Windows)
```

All three ship together at one version, with `layerfs.node` and
`layerfs-daemon`; `backend` and `runtime` depend on an exact `agent` version.

## 2. `@layerfs/agent`: the client, the agent methods, the transports

Each method keeps one file, as today. `index.js` also refuses to load a second
copy of the package in one process, so the same Store is never opened twice.

```diff
  packages/agent/src/                      [485]  (all of today's sdk/js/src/)
    index.js                                 64   (LayerFS: open, init, close; no public constructor; one-copy guard)
    client.js                                76   (invokeClient; shared Store per process; closed flag)
    errors.js                                18   (plus Capacity)
    trace.js                                118
    transport/                               [13]  (.mjs sources not counted)
+     index.js                                —   new: picks linux, macos or windows
      inprocess.js                           13   (from src/native.js: load layerfs.node)
+     worker_client.js                        —   new: one worker client, the other end of runtime's worker/server.js; merged from dsh port/binding.js 123 and windows/binding.js 88
      frames.js                             n/c   (from dsh port/frames.mjs; wire tag now "layerfs-worker")
    project/                                  [9]
      init.js                                 5   (called by LayerFS.init)
      list_branches.js                        4
+     list_sandboxes.js                       —   new: Sandbox records, read-only
    branch/                                  [24]
      diff_branches.js                        4
      diff_commits.js                         4
      fork.js                                 3
      list_commits.js                         4
      merge.js                                6
      publish.js                              3
    fs/                                      [92]
      apply.js                                5
      blame.js                                3
      body.js                                24
      cp.js                                   4
      diff_files.js                           3
      edit.js                                 4
      glob.js                                 4
      grep.js                                 4
      index.js                               14
      list.js                                 4
      mkdir.js                                4
      mv.js                                   4
      read.js                                 3
      rm.js                                   4
      stat.js                                 3
      write.js                                5
+   sandbox/                                  —   new
+     create.js                               —   new
+     list_workspaces.js                      —   new
+     close.js                                —   new
    workspace/                               [71]
      commit.js                               4
      exec.js                                51
      mount.js                                7   (optional sandbox)
-     status.js                               5   (moves to backend daemon/workspace/status.js)
      unmount.js                              4
```

## 3. `@layerfs/backend`: Store and daemon calls

One file per method, like the agent. Today these calls are inline in
`sdk/js/operator/index.js`.

```diff
  packages/backend/src/                     [34]
    index.js                                 29   (from sdk/js/operator/index.js: LayerFSBackend.of; store and daemon groups)
+   store/                                    —   new
+     project/                                —   new
+       get_comparable.js                     —   new: from dsh reads.js asCommit
+       get_history.js                        —   new
+       get_storage.js                        —   new
+       list_overwritten.js                   —   new
+     branch/                                 —   new
+       list_commit_storage.js                —   new
+     fs/                                     —   new
+       get_storage.js                        —   new
+       list_revisions.js                     —   new
    daemon/                                   [5]
      workspace/                              [5]
        status.js                             5   (from agent's workspace/status.js)
+       list_changes.js                       —   new
+       get_storage.js                        —   new
+     process/                                —   new
+       start.js                              —   new
+       list.js                               —   new
+       cancel.js                             —   new
+     sandbox/                                —   new
+       get_log.js                            —   new
```

Six of the seven `store` methods and five of the seven `daemon` methods are
today's operator calls. `store/project/get_comparable.js` is new (today's
`asCommit` in dsh `reads.js`), `daemon/workspace/status.js` is today's public
`workspace.status`, and `daemon/sandbox/get_log.js` is new.

## 4. `@layerfs/runtime`

The worker and the platform folders come from the dsh Host's `port/`, `box.js`
and `windows/`; the rest of runtime from the Host's runtime files. LOC on a
moved-in file is its dsh source's count. There is no `http/` folder: dsh's
routes and UI reads stay in dsh-layerfs ([ui-frontend.md](ui-frontend.md)).

```diff
  packages/runtime/src/                    [756]
+   index.js                                  —   new: LayerFSRuntime.open, close
    settings.js                              37   (from dsh config.js)
+   status.js                                 —   new: from part of dsh runtime.js 207
    clients.js                               76   (from dsh layerfs-client.js; uses @layerfs/agent's public API only)
    worker/                                   [0]  (.mjs sources not counted)
+     server.js                               —   new: one worker program, merged from dsh port/worker.mjs and windows/worker.js 108
      call.js                               n/c   (from dsh port/call.mjs)
      init.js                               n/c   (from dsh port/run.mjs: Init with a folder mounted)
    platform/                               [277]
+     linux/                                  —   new
+       index.js                              —   new: no worker; the Docker volumes path
      macos/                                 [83]
        index.js                             83   (from dsh box.js: the worker container)
      windows/                              [194]
        index.js                            165   (from dsh windows/host.js: the WSL worker)
        paths.js                             29   (from dsh windows/paths.js: wslpath)
    project/                                [247]
+     list.js                                 —   new
      init.js                               139   (from dsh init.js: start_init; demo scenario check dropped)
      jobs.js                                28   (from dsh jobs.js: get_init)
      remove.js                              47   (from dsh remove.js)
      state.js                               33   (from sdk/js/operator/inspect/snapshot.js: get_state)
+   sandbox/                                  —   new
+     reclaim.js                              —   new: the reclaim step of today's operator.sandbox.list
+     leftovers.js                            —   new: from part of dsh runtime.js 207
+   events/                                   —   new
+     subscribe.js                            —   new: calls from every process's trace
-   cli/                                    [119]  (retired 2026-10-23: the `layerfs` command in api/agent/cli/ replaces it)
-     lfs.js                                119   (from dsh lfs.js)
```

## 5. What leaves today's SDK folders

```diff
  sdk/js/operator/                          [62]
-   index.js                                 29   (moves to packages/backend/src/index.js)
-   inspect/                                 [33]
-     snapshot.js                            33   (moves to runtime project/state.js)
```

```diff
  sdk/js/src/                               [13]  (only native.js shown)
-   native.js                                13   (moves to agent transport/inprocess.js)
```

Every other file in `sdk/js/src/` moves to `packages/agent/src/` at the same
relative path (section 2).

## 6. The native addon crate

`layerfs-node` stays at `sdk/js/native/`. Its files are edited in place.

```diff
  sdk/js/native/src/                      [2543]
    lib.rs                                   39
    convert.rs                               84
    direct.rs                               165
    discovery.rs                            566
    dispatch.rs                             105   (backend.* names; sandbox.create, close, list_workspaces; project.list_sandboxes)
    exec.rs                                 271   (process calls under backend.daemon.process)
    job.rs                                   83
    output.rs                               165
    pool.rs                                 182
    project.rs                              196
    registry.rs                             123
    sandbox.rs                              306   (create and close exposed; reclaim kept for runtime)
    trace.rs                                 79
    workspace.rs                            179   (mount takes an optional sandbox; list_workspaces)
```

The Rust crates under `core/crates` change only their dispatch names
(`layerfs-store-service/src/operator/dispatch.rs`,
`layerfs-daemon/src/operator/dispatch.rs`); their folders don't move.

## 7. Totals

| Package | Today (prod LOC) | Note |
| --- | ---: | --- |
| `@layerfs/agent` | 485 | all of today's `sdk/js/src/`, before new files and before `workspace/status.js` (5) moves to backend |
| `@layerfs/backend` | 34 | `sdk/js/operator/index.js` (29) and `workspace/status.js` (5) |
| `@layerfs/runtime` | 756 | moved in: 723 from the dsh Host, 33 from `sdk/js/operator/inspect/snapshot.js` |
| `layerfs-node` | 2,543 | edited in place |

These are the counts of today's files at their new places. The new files'
sizes are not estimated, and the commits that build this report their own
before and after counts. Runtime code moved from the dsh Host is a relocation
in those counts, reported with `dsh` and `sdk` subtotals.
