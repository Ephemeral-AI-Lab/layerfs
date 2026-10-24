# LayerFS v1.1.0

> **Status:** Release candidate for LayerFS 1.1.0: the draft body of the
> GitHub release, not yet published. Everything below the rule is the body.

---

# LayerFS v1.1.0

LayerFS 1.1.0 splits the JavaScript SDK into three packages, adds the `layerfs`
command for agents that only have Bash, and makes Sandboxes explicit. It runs
on macOS and Windows through a LayerFS home's worker.

> **This release breaks 1.0.0 SDK code.** v1.0.0 said an incompatible SDK
> change needs a new major version. The owner chose 1.1.0 anyway and waived
> that promise for this release only; see
> [what breaks](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v1.1.0/docs/release-notes/1.1.0/README.md#owner-waiver-of-the-100-major-version-promise).

## What's new since v1.0.0

- **Three packages.** `@layerfs/sdk` is now `@layerfs/agent` (Projects,
  Branches, files, Sandboxes, Workspaces, Exec and Commit), `@layerfs/backend`
  (Store and daemon reads for UIs and tools; replaces `@layerfs/sdk/operator`)
  and `@layerfs/runtime` (Init and removal, the worker, Sandbox and volume
  upkeep on a LayerFS home). The sources moved from `sdk/js/` to `api/`.
- **The `layerfs` command.** One `@layerfs/agent` call per command, with JSON
  results: `layerfs sandbox create`, `layerfs workspace mount 1 main`,
  `layerfs workspace exec … "npm test" --stream`. Its tests require the same
  answer as the SDK for every call.
- **Explicit Sandboxes.** `sandbox.create()` starts a Sandbox and
  `workspace.mount(sandbox, branch)` mounts in the one it names; there is no
  automatic placement. `project.list_sandboxes`, `sandbox.list_workspaces` and
  `sandbox.close` are new.
- **Factories.** `await LayerFS.open(name)` and `await LayerFS.init(name,
  folder, timeout?)` make clients; settings come from `LAYERFS_*` variables.
- **macOS and Windows.** Store calls go to the LayerFS home's worker: a Linux
  container on macOS, a Node process in WSL2 on Windows, on loopback behind a
  secret. Any integration may use it.
- **Init.** An optional time limit, 300 s by default, instead of a fixed 60 s;
  a refused source entry is named; the sealed copy is deleted after install.
- **Fixes.** Binary write bodies of every JavaScript type, a hard-linked file
  kept through a merge, the daemon's first-connection admission, and Sandbox
  log caps and cleanup.
- **Repository.** The LayerFS tab for DeepSeek Harness is now
  [dsh-layerfs](https://github.com/Ephemeral-AI-Lab/dsh-layerfs), and the
  benchmarks are [layerfs-benchmark](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark).

## Verification

- All 10 dsh-layerfs live scenarios pass on Windows (5 through the SDK, 5
  through the `layerfs` command), at LayerFS `3c21c62c6e`.
- On macOS, 9 of the 10 pass at the same commits. Depot stopped at its first
  command because that run's Sandbox image had no `cargo`; after the image was
  rebuilt, one Depot rerun passed.
- No Linux host ran this release: the Linux FUSE mount tests and the
  real-Store JavaScript tests did not run, and the owner waived them.
- Clippy, every package's tests on macOS and the JavaScript stand-in tests
  pass; see the [release record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v1.1.0/docs/release-notes/1.1.0/README.md#verification-evidence)
  for what did not run.

This release makes no new performance claim; the 1.0.0 results on
[layerfs.ai](https://layerfs.ai/benchmarks/) were measured on 1.0.0 source.

## Compatibility

- **SDK.** 1.0.0 code needs changes: the package name, `LayerFS.open` instead
  of `new LayerFS`, `sandbox.create` before `workspace.mount(sandbox, branch)`,
  and `@layerfs/backend` instead of `operator` and `workspace.status`. The
  [release record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v1.1.0/docs/release-notes/1.1.0/README.md#what-breaks)
  lists every change.
- **Stores.** The Store format is unchanged from 1.0.0, so 1.0.0 Stores pass
  the open check; this was read from source, not tested. Stores from v0.1.7 or
  earlier are refused.
- **Version matching.** The packages, the native addon, the daemon and the
  runtime image must come from this tag; nothing checks it for you.
- **Store profile.** Only the Disposable profile runs: there is no crash or
  power-loss durability.

The release is source-only: no npm package, prebuilt binary or public image.

## Links

- [README](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v1.1.0/README.md) and the [JavaScript packages wiki](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v1.1.0/api/README.md)
- [Release record](https://github.com/Ephemeral-AI-Lab/layerfs/blob/v1.1.0/docs/release-notes/1.1.0/README.md), with the waiver and checks
- [Docs](https://layerfs.ai/docs/) and [benchmarks](https://layerfs.ai/benchmarks/) on layerfs.ai
- Community: [Discord](https://discord.gg/8mmAMCDc2)
