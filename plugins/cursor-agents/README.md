Cursor coding agents work on LayerFS Branches and Workspaces instead of git worktrees. One task gets one Branch. File edits go through `fs.*`. Bash runs as `workspace.mount`, then `exec`, `commit`, and `unmount`. Each LayerFS call is attempted once.

Plugin id: `cursor-agents`. Package: `layerfs-cursor-agents` `0.1.0`. This repository is for Cursor only. It does not install Claude Code skills, hooks, or configuration, and it does not ship a history dashboard.

The LayerFS commit this version was built against is `commit` in [`layerfs.json`](https://github.com/earsonlau/layerfs-cursor-agents/blob/1802b525cca2ff9730678c01f8aefcc107223940/layerfs.json): `ad78914e36b258b360daf13aec164b5655814689` (LayerFS v1.1.0).

## What it adds

- A Cursor skill at [`.cursor/skills/layerfs-agent/SKILL.md`](https://github.com/earsonlau/layerfs-cursor-agents/blob/1802b525cca2ff9730678c01f8aefcc107223940/.cursor/skills/layerfs-agent/SKILL.md). The same skill is at [`.agents/skills/layerfs-agent/SKILL.md`](https://github.com/earsonlau/layerfs-cursor-agents/blob/1802b525cca2ff9730678c01f8aefcc107223940/.agents/skills/layerfs-agent/SKILL.md) for agents that look there.
- `layerfs-cursor-setup`, which checks out that LayerFS commit, links the official `layerfs` command onto `PATH`, and writes `LAYERFS_*` settings.
- On Linux, Store calls stay in the agent process. On macOS and Windows the setup starts the home worker with `@layerfs/runtime` and sets `LAYERFS_WORKER_ENDPOINT`.

## How it works

The skill is the procedure. Short form:

1. Source `layerfs.env`. Use `layerfs` or `LayerFS.open(process.env.LAYERFS_PROJECT)`.
2. `branch.fork` one Branch for the task. Do not add a git worktree.
3. Edit with `fs.write`, `fs.edit`, `fs.apply`, and the other `fs.*` calls.
4. Run bash as `sandbox.create` (when you need a Sandbox), then `workspace.mount`, `workspace.exec`, `workspace.commit`, and `workspace.unmount`. Unmount in `finally`. On `Conflict`, stay in that Workspace until you resolve it or discard it.
5. On `Conflict`, `Busy`, or `NoSuchSandbox`, follow [`refusals.md`](https://github.com/earsonlau/layerfs-cursor-agents/blob/1802b525cca2ff9730678c01f8aefcc107223940/.cursor/skills/layerfs-agent/references/refusals.md) once. Do not replay the refused call.

`examples/task-branch.mjs` forks a Branch (or keeps it if the name is taken) and writes one file. It imports the public SDK from `LAYERFS_REPO`.

Setup reads `layerfs.json` and requires a 40-character `commit`. It clones [Ephemeral-AI-Lab/layerfs](https://github.com/Ephemeral-AI-Lab/layerfs) into `--layerfs` (default `../layerfs`, or `LAYERFS_REPO` when that is set) and checks out that commit. An existing checkout already on the commit is left as it is. A checkout on another commit is updated only when `git status` is clean: one `git fetch` of that commit, then one `git checkout`. A dirty tree is not touched. It then links `@layerfs/agent` and `@layerfs/runtime` into this repo's `node_modules` (symlinks to the checkout, not copies of LayerFS source) and writes a `layerfs` shim that runs the official command.

On Linux it does not start a worker. On macOS and Windows it opens `@layerfs/runtime` with `sweep: false` and calls `worker.start()` once. macOS then closes the runtime object; the worker container keeps running. Windows keeps the setup process alive because the runtime owns the worker on stdin.

## You need

- Node.js 20 or newer, `git`, and `npm`.
- A LayerFS checkout at the pinned commit. The setup script clones it if you do not already have one.
- Linux, for real Store calls: the Rust 1.85.1 toolchain (`rustup toolchain install 1.85.1`), so the native addon can be built, and `LAYERFS_STORE_PATH` pointing at the project's installed Store (`global/store.sqlite` on a local filesystem the daemons share).
- Sandboxes (any OS): Docker, `LAYERFS_IMAGE` (an image digest), `LAYERFS_VOLUME`, and `LAYERFS_DAEMON`.
- macOS worker: Docker, `LAYERFS_IMAGE`, and a sealed Linux bundle.
- Windows worker: WSL2, Docker inside it, `LAYERFS_IMAGE`, and a sealed Linux bundle. The setup process must stay running; it owns the worker.

Default tests do not need Docker, Cargo, or a LayerFS checkout.

## Tests

`node --test` uses no Docker, no Store, and no daemon. It checks the pin, the skill, refusal handling, and the setup plan with a fake filesystem.

When `LAYERFS_REPO` points at the pinned checkout, the same command also imports the public `@layerfs/agent` entry over LayerFS's stand-in native binding and runs `layerfs help`. Still no Docker.

`LAYERFS_CURSOR_E2E=1 node --test test/e2e.test.mjs` is skipped unless that variable is `1`. It then needs Docker and `LAYERFS_REPO`, `LAYERFS_HOME`, `LAYERFS_STORE_PATH`, `LAYERFS_IMAGE`, `LAYERFS_VOLUME`, `LAYERFS_DAEMON`, and `LAYERFS_PROJECT`, and it runs one mount, exec, commit, and unmount. A missing value or a refused call fails the test. It does not retry.

## Limits

- Cursor only. No Claude Code, DSH, Pi, or `@layerfs/backend` UI.
- Packages used here are the public `@layerfs/agent` and `@layerfs/runtime` APIs. Nothing imports `@layerfs/agent/internal`.
- Settings written for agents are `LAYERFS_*` names. The SDK does not take an options object.
- One attempt per call. Setup does not retry a failed git, npm, or worker start. Agents do not retry a refused SDK call.
- Linux has no worker. `worker.start` on Linux is `InvalidInput` in `@layerfs/runtime`, and this setup does not call it.
- macOS and Windows need a sealed bundle and an image digest before the worker can start. This plugin does not build that image or the bundle.
- The Windows worker dies when the setup process exits.
- `main` changes only through `branch.publish`.
- This repo does not modify the LayerFS checkout except for the setup script's clone, checkout, `npm install`, and optional native build.
- Conflict markers from `workspace.commit` exist only in the Workspace that hit the conflict.
- The `layerfs` command starts a new process per call. A long loop belongs in one Node script.

## License

MIT. See [LICENSE](https://github.com/earsonlau/layerfs-cursor-agents/blob/1802b525cca2ff9730678c01f8aefcc107223940/LICENSE).
