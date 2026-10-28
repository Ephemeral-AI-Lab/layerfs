# #428 change list

> **Status:** Dated planning checkpoint (2026-10-24); not release evidence or a
> product contract.

What issue [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428)
changed, grouped by theme, with the commits that carry each change. The design
is in [README.md](README.md); this list records what was built and where it
stands.

Where the work lives:

| Place | Branch | State on 2026-10-24 |
| --- | --- | --- |
| layerfs | `main` | design docs pushed; the dsh move and this list not pushed |
| layerfs | `claude/issue428-sdk-packages-cd9412` | the SDK packages, CLI and tests: 28 commits ahead of `main`, 14 of them not pushed, not merged |
| dsh-layerfs | `main` | the plugin: 19 commits not pushed, more uncommitted |

## 1. dsh moved out of layerfs

The dsh LayerFS plugin now lives in its own repository,
[dsh-layerfs](https://github.com/Ephemeral-AI-Lab/dsh-layerfs), which pins the
LayerFS commit it builds on. layerfs holds no dsh code.

- `0c7d9c47c1` move `dsh/` to dsh-layerfs (layerfs `main`)
- `0db63e2`, `e8c1f59` import the plugin; read LayerFS through `LAYERFS_SOURCE` (dsh-layerfs)

Adjacent, not tagged #428: `7b0648223e` moved `benchmark/` to
layerfs-benchmark the same day.

## 2. SDK consolidation

The JavaScript SDK is split into three packages with one protocol.

- `@layerfs/agent`: factories, Projects, Branches, files, Sandboxes and
  Workspaces, over the native addon (Linux) or a home's worker (macOS,
  Windows). Each client keeps the transport it was made with.
- `@layerfs/backend`: Store and daemon calls. Protocol methods `operator.*`
  are renamed `backend.*`.
- `@layerfs/runtime`: what an integration host runs on a LayerFS home:
  status, the worker, project Init and removal, Sandbox upkeep and live call
  events.

Commits: `ba8573f931`, `a140fdd408`, `28c56f2573`, `6124ca0702`, `a5ebb508a1`,
`4d897386b3`, `dee1274bc1` (`@layerfs/agent/internal` is not a public API),
`aa02619714` (skills use the new packages). Design: `fab561998a`,
`cea713e767`, `5db88b3fe1` (layerfs `main`).

## 3. The `layerfs` command

One `@layerfs/agent` call per command. It replaces `@layerfs/runtime`'s `lfs`
command, and agents run it from their shell.

- `9f663a07b1` the command; `ee93735f27` `lfs` retired
- `77ff5f25ae` `layerfs branch publish` takes the commit as optional, like the SDK
- `2a50ffd61d` diff commands accept `--no-hunks`
- `3fe978baca` E2E bundles carry the command

## 4. The LayerFS home

**Built:**

- The runtime requires an absolute home folder holding every project's entry
  and record; Store calls, Init and the worker run against one home.
- One worker per home: a Linux container on macOS, a Node process in WSL2 on
  Windows. A second home asking for the same worker is refused `Busy`.
- On Linux a home owned by another uid is refused. Volumes are labelled with
  their home, so a volume of another home is told apart.
- The agent package reads `LAYERFS_HOME`, `_DOCKER`, `_IMAGE`, `_VOLUME` and
  `_DAEMON` to reach a home's worker.
- **Decided (owner, 2026-10-23):** Init deletes its `sealed.sqlite` once the
  install is recorded (`2d51d85b91`).

**Documented:** `3690e8bd54` adds [layerfs-home.md](layerfs-home.md), the
layout on macOS, Linux and Windows.

**Proposed, open:** a default home `~/.layerfs`; a copy of each project's
manifest in its volume, so a lost `project.record` can be rebuilt.

## 5. Init and mount

- `fa2db50e45`, `cdc9493982` (#431) Init has no fixed 60 s deadline; its
  time limit is optional and defaults to 300 s. `LayerFS.open` and `init`
  take no options.
- `05faf7dfe6` `workspace.mount(sandbox, branch)` names its Sandbox; no
  placement.

## 6. The macOS worker, hardened

- `7acd4badf8` the worker secret is kept off docker's command line
- `c2e2e39b9a` unauthenticated frames are capped and the secret read times out
- `6d8a97e03a` the older Store container is listed and retired on confirm

## 7. Docker upkeep (from #423)

- `0aa1714da1` the runtime lists each Sandbox's scratch and the Store volumes,
  and sweeps stopped Sandboxes
- `5d0d7db5ec` a Sandbox container's log is capped at two 10 MB files

## 8. Tests on a real Store

- `a6e512fbaa` one stand-in addon for the SDK and CLI tests
- `69ba498af5` one case table for every SDK call
- `b9404ecf34` the CLI checked against the SDK, case by case
- `d1d86a4b50` every agent call against a real Store, through the SDK and the CLI
- `8260c9763d` the real scenario streams exec output through both

## 9. Scripted model (mock) scenarios in dsh-layerfs

The LayerFS scripted model plays scripted agent runs on real LayerFS, with no
language model.

- `c12166a` the simulator (Mocked LayerFS) is removed; every scenario runs on LayerFS
- `852731e` Scenarios tab and Run bar; every mount names its Sandbox
- `04cd3b6`, `30da9f3` the shop, depot and crew replays become scenarios,
  creating Sandboxes from each recorded run's image
- `0147ca8` scenario format version 3, literal commands checked live; five
  run through the SDK, five through the CLI
- `b4980e0` on by default (owner, 2026-10-24); `aafa499` a run's model is not
  saved as the user's default model

## 10. dsh-layerfs Host and UI

**Host:** `ba8b2b1`, `c0fdf21`, `efd6896`, `38905b3`, `c8d1f5d`, `8eec80a`,
`60bb758` move the Host onto `@layerfs/runtime` and the #428 method names;
`e147c2a` puts `layerfs` on agents' PATH and serves each call's trace lines.

**All projects page:** `bef1a01`, `fa64d7e`, `69996f7` a health strip, the
upgrade banner for the older Store container, running and idle projects,
volume leftovers, and removal of a binding with nothing left.

**Init dialog:** `aeb4903`, `3ef3857` an optional time limit.

**Smaller:** `8df0c8b` History kept per project and checked with the Store's
version; `b23bbf9` the Runs tab removed; `360855d` a File tab only while a
file is open.

**Uncommitted (2026-10-24):** the Work tab shows its layout before a
session's first call; a project chip in the session header with a light
`GET /layerfs/session/<id>/binding` route; the Scenarios tab renamed Tests,
with icons on the page's tabs and status icons on the Tests page.

## 11. Recovery tool

`ed41c5c427` a one-off tool that rebuilds `project.record` from each volume's
Store, used once to recover the 3080 profile. Not pushed.

## Open items

- The Host's Branch poll calls `project.list_branches` repeatedly.
- Stray "Layer … does not exist" 409 answers on list reads.
- `tools/worktree.py` on layerfs `main` still expects `sdk/js/src`.
- The two home proposals in section 4.
