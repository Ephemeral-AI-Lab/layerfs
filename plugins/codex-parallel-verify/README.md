One shared LayerFS Store, one Branch per Codex hypothesis, and verify commands in parallel. Disk stays near the size of one tree instead of N git worktrees.

Built for [Codex](https://github.com/openai/codex) on LayerFS v1.1.0. The public packages are `@layerfs/agent` and `@layerfs/runtime`. This plugin does not import `@layerfs/agent/internal` or `@layerfs/backend`. Every LayerFS call is one attempt. A refusal is not retried.

## What it adds

Codex CLI sets the session root with `-C` / `--cd`. `codex exec --worktree` and `codex --worktree` create a managed git worktree and use that checkout as the session root. The desktop app gives each thread its own worktree. A worktree is a full checkout. Dependencies are not in git, so each one runs its own install. N candidates cost about N copies of the tree plus N copies of `node_modules`.

This plugin is the replacement. You do not pass `-C` once per candidate, and you do not pass a LayerFS Workspace path to `-C`.

| Codex worktree | This plugin |
| --- | --- |
| `git worktree add ../hyp-1 -b hyp-1` | `spawn-n` forks a Branch, with no file copy |
| `npm ci` in every worktree | one `init --install`, committed on `deps` and published to `main` |
| `codex exec -C ../hyp-1 "npm test"` | edit branch `hyp-1`, then `verify-all --command "npm test"` |
| `codex exec --worktree` for another try | another Branch forked from the same `main` |
| desktop thread per worktree | one Branch per hypothesis, then one `verify-all` |
| `rm -rf ../hyp-2` | `discard --name shop hyp-2` |

- `init --name shop --source "$PWD"` imports a directory once. `--install` runs once on branch `deps` and is published to `main` only when it exits 0.
- `spawn-n --count 4` forks that many Branches from `main` (prefix `hyp` by default).
- `verify-all --command "npm test"` runs the command on each Branch.
- `report` prints how much disk the Store used against a worktree baseline.
- `discard` and `keep` only change the ledger. They do not copy a tree, check out a worktree, or delete commits.

`workspace.mount` returns a path such as `/workspaces/1`. That path is inside the Sandbox. It is not a directory on the host, so `codex exec -C /workspaces/1` does not see the candidate. Host Codex stays in the real checkout. Edits for a candidate go to that candidate's Branch (`fs.write` / `fs.apply`, or one `workspace.exec` plus one `workspace.commit`). The verify command runs with `workspace.exec`, which is the step that used to run in the directory passed to `-C`.

One Codex session, one Branch. The next hypothesis is another Branch, not another `-C`.

## How it works

`verify-all` runs mount, then exec, then commit, then unmount on each Branch. A non-zero verify command is still committed once, with the exit code in the message. Default concurrency is 4. `--sequenced` is one Branch at a time. `--parallel` is all of them. Sandboxes hold 16 Workspaces; One is created when a Branch is about to mount and every Sandbox already started for this run is full, then each is closed once after the run.

If two commits change the same path on one Branch, `workspace.commit` throws `Conflict`. `detail.conflicts` lists the paths and `detail.head` is the head that won the race. Do not send the commit again and do not pass `force`. `verify-all` records the refusal, unmounts that Workspace once with discard set, and moves on. To keep the edit, read the conflicting paths once, resolve them on a new fork, and commit there once.

On Linux, Store calls stay in this process and `LAYERFS_WORKER_ENDPOINT` stays unset. On macOS and Windows the CLI calls `runtime.worker.start()` once and sets `LAYERFS_WORKER_ENDPOINT` to `$LAYERFS_HOME/worker/endpoint.json`. Do not start a second worker for the same home. Windows can pass `--wsl-distro`, `--wsl-user`, `--wsl-node`, and `--wsl-home` when the runtime defaults (Ubuntu, root, `/usr/local/bin/node`) are wrong.

`report` and `verify-all` print:

| Field | Meaning |
| --- | --- |
| `stored` | Bytes the Store occupies on disk, from `runtime.volume.list` for this project's volume, or one `stat` of `global/store.sqlite` when that figure is missing |
| `unitBytes` | One materialized tree. After a successful `--install`, this is measured inside the Workspace and includes dependencies. Otherwise it is the source directory scan |
| `worktreeBaseline` | candidates times `unitBytes`: the working-tree cost of one git worktree per candidate |
| `savedBytes` | `worktreeBaseline` minus `stored` |
| `timesSmaller` | `worktreeBaseline` divided by `stored` |

Forked candidates stay in the baseline after `discard`, because a worktree workflow would have had those trees on disk and the Store still holds their commits.

## You need

- The Sandbox runtime image needs `node` and `npm` (`init --install`, `verify-all --command "npm test"`, and the size probe).
- Node 20 or newer (`node -v` starts with `v20` or a newer major)
- This repository at the listed commit, and a LayerFS checkout at the commit recorded in its `layerfs.json`
- `npm --prefix "$LAYERFS_REPO/api" install` in that checkout, which links `@layerfs/agent` and `@layerfs/runtime` from the checkout rather than the registry
- For a live project: a Linux `layerfs-daemon` and a runtime image built from that same LayerFS commit, then `LAYERFS_HOME`, `LAYERFS_IMAGE`, and `LAYERFS_DAEMON`. On macOS and Windows, also `LAYERFS_BUNDLE`, a sealed bundle that contains `layerfs-daemon`, `layerfs.node`, and `manifest.json`

`npm test` in this repository uses stand-in clients. It checks that each LayerFS call is made once, that a `Conflict` is not committed again, that macOS and Windows set `LAYERFS_WORKER_ENDPOINT` after one `worker.start`, and that the report's `stored` size is compared with the worktree baseline. It does not need Docker, a daemon, or a Rust toolchain.

## Limits

- The default tests do not open a real Store.
- Exit 125 from the CLI means LayerFS refused the call. `Exists` on a second `init` of the same name is not a prompt to retry.
- `discard` and `keep` do not delete commits. Discarded losers stay as deduplicated history. Unmounting with discard drops an uncommitted overlay and copies nothing.
- A Cursor plugin, the DeepSeek Harness UI, and changes to LayerFS itself are out of scope.
