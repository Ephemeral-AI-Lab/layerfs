# LayerFS for Magpie

[LayerFS](https://github.com/Ephemeral-AI-Lab/layerfs) workspaces for every
agent [magpie](https://usemagpie.ai) connects: instead of each agent copying
a project tree to work on it, agents share one durable, deduplicated history
and work through **stable filesystem paths**. Forking a branch for a
parallel agent copies **no file bytes**; a checkpoint stores **only what
changed**; any agent can resolve, read and build on what the others
committed.

给每个 agent 的 LayerFS 工作区：多个 agent 共享一份可去重的持久历史，而不是
各自复制整棵项目树。给并行 agent 开分支不复制任何文件字节；存检查点只写
变化的部分；路径稳定，任何 agent 都能定位、读取并接续其他 agent 的提交。

```sh
magpie plugin add @magpie-community/tools-layerfs
magpie plugin            # lists layerfs_provision, layerfs_checkpoint, ...
```

## Tools

Served on magpie's MCP server as `layerfs_<tool>`:

| Tool | What it does |
|---|---|
| `provision` | Gives the agent a workspace for a project branch and answers the path to work in. The first provision imports the source directory as the project's first checkpoint; later provisions of the same branch answer the same path without copying the tree again. |
| `checkpoint` | Saves the workspace's state on its branch, storing only the blobs the store has not seen; answers added / modified / deleted counts and the new-bytes vs saved-bytes accounting. |
| `fork` | Forks a branch for a parallel agent: a pointer at an existing checkpoint, ready immediately, 0 bytes copied. |
| `publish` | Publishes a branch's head as main's new Layer, so finished work becomes what new work forks from. Guarded: main's head must be in the branch's history, so a publish can only move main forward — if main moved on its own, the publish is refused (a real dsh run lost main's 238 tests to a stale publish; this is that lesson, encoded). `force: true` overwrites deliberately and reports what it replaced. |
| `exec` | Runs a shell command in a branch's workspace and checkpoints whatever it changed as one commit — the dsh shape, where every bash call is mount, run, commit. A no-change command commits nothing; a failing command's changes are still checkpointed, with its exit code reported. |
| `history` | Lists a branch's checkpoints, newest first, with each one's logical tree size and the bytes it actually stored. |
| `resolve` | Resolves a project / branch / file to a real filesystem path, its checkpoint and content hash — an O(1) lookup, no copying. |
| `status` | Backend in use, projects and branches held, and the storage accounting: logical bytes of all checkpoints vs bytes the store occupies. |

## Backends

- **native** — the real LayerFS through its JavaScript SDK
  (`@layerfs/sdk` from a [layerfs](https://github.com/Ephemeral-AI-Lab/layerfs)
  checkout; the SDK is not on npm, and its native binding must be built —
  `sdk/js/scripts/build-native.mjs`). Set `LAYERFS_SDK_PATH` (the SDK
  entry, `sdk/js/src/index.js`); `LAYERFS_STORE_PATH` is optional and
  only anchors the store volume elsewhere. Adapted and verified against
  a locally built binding (LayerFS 1.0.0): the SDK's store calls
  (project / branch / fs) run in-process, so provision materializes a
  *checkout* of the branch through `fs.list` / `fs.read`, and a
  checkpoint diffs that checkout and lands as one `fs.apply` commit in
  the engine's own store, with the engine's chunk-level dedup. Real
  workspace mounts (`workspace.*`) need a LayerFS deployment (Docker,
  `LAYERFS_IMAGE`, a daemon) that a magpie host does not provide; where
  one exists, that is the natural next step. Two engine limits shape
  this backend, both reported upstream: the fs surface is text-only in
  practice (binary files enter a project through the initial import,
  and checkouts/checkpoints name the binary files they cannot move
  instead of dropping them), and `project.init` refuses a source tree
  on overlayfs with an opaque `Project(InvalidInput)`.
- **sim** (default when the SDK is not configured) — a local
  content-addressed store implementing the same model: immutable blobs
  named by SHA-256, branches as manifest pointers, checkpoints as
  deduplicated manifests, under `MAGPIE_LAYERFS_HOME`
  (default `~/.magpie-layerfs`). It runs anywhere, which is what the tests
  and the benchmark below use.

`MAGPIE_LAYERFS_BACKEND=auto|native|sim` selects explicitly; `auto` picks
native when `LAYERFS_SDK_PATH` is set.

## Benchmark

`bun bench/bench.mjs` — a 184-file, 26.8 MB project; 6 parallel agents,
each keeping 3 checkpoints (sim backend, this machine):

| | naive copies | this plugin |
|---|---|---|
| fork one more agent | 321 ms, 26.8 MB copied | **~1 ms, 0 bytes** |
| keep 18 checkpoints | 482.6 MB of snapshots | **27.2 MB** of store for 509.4 MB of logical checkpoints (**18.7× dedup**) |
| resolve a file path | walk the copy's tree | **138 µs**, with its checkpoint + SHA-256 |
| total disk for the run | 643.5 MB | **188.1 MB** (store + the 6 live workspaces) |

![test run](docs/assets/tests.png)
![benchmark](docs/assets/benchmark.png)

Honest limits: a first provision still materializes the tree once, and the
sim backend rehashes the tree at each checkpoint (the native engine tracks
changes through its daemons instead). What the plugin removes is the
per-agent copy, the per-snapshot copy, and the bytes they waste — the
engine's own benchmarks are at
[layerfs.ai/benchmarks](https://layerfs.ai/benchmarks/).

Against the real engine (`bun bench/bench-native.mjs`, native backend,
same scenario, generated-text fixture of 184 files / 27.0 MB): import
3.6 s once; a fork plus its checkout 1.8 s (the branch itself is
metadata; the checkout pays the file reads); a checkpoint 1.9 s (the
checkout diff reads the branch back through the fs surface, then one
`fs.apply` lands it); the engine's store file held **5.6 MB for
513 MB of logical checkpoints** on this compressible text fixture —
the sim fixture above is random bytes, the incompressible extreme, so
read the two ratios as the ends of a range, not a single number.

## The page: watch the trajectory

The package also declares a magpie page, **LayerFS trajectory**
(轨迹), which replays how the agents' worktrees grew, drawn the way
LayerFS's own History picture draws them: main is the green middle
bough, each branch a bough of its own, a checkpoint a leaf, a fork a
split, and a publish a graft back onto main that adds a Layer band
(L0, L1, …). Two running totals carry the argument — what those
checkpoints would have cost as plain copies against what the
deduplicated store actually wrote, with a store.stats line in
LayerFS's own accounting words. Play it, scrub it, click a leaf for
its checkpoint's message and byte split, or click a branch's name tag
to focus that branch alone: that is what the plugin does for an
agent's edits, made visible.

A magpie page may read no live data — its sandbox forbids fetch — so
after every tool call the plugin writes the recorded trajectory beside
the page as `page/trajectory.js`, and the page animates that recording.
Reopen the page to see the newest state. The `trajectory.js` bundled in
the repository is a labeled sample recorded from the benchmark scenario
(one project, six agents, three checkpoints each); the first real tool
call replaces it. The page declares no `magpie.read` permissions: the
trajectory is the plugin's own record, not magpie's data.

## Develop

```sh
bun test                 # the suite (bun:test): tools end to end, plus the page's layout math
bun bench/bench.mjs      # the benchmark above; writes bench/results.json
node tools/gen-trajectory.mjs   # regenerate the page's bundled sample trajectory (run it last: the suite's own tool calls refresh page/trajectory.js too, by design)
```

Zero runtime dependencies; the arguments are JSON Schema, like the other
tools packages. MIT.
