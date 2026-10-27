Parallel agent workspaces in [Herdr](https://herdr.dev) without copying your repo. Each workspace the plugin opens is a LayerFS Branch: forking one copies no file bytes, closing it saves it, and after Herdr or the machine restarts the plugin mounts the branches again and reopens their workspaces.

## What it adds

Herdr actions, also runnable from a shell or by an agent as `node <plugin_root>/lfs.mjs <command>`:

- `init` (**LayerFS: import this repo**): imports the focused repo's `HEAD` as a LayerFS project.
- `fork` (**LayerFS: new branch workspace**, or **new named branch**): forks the project, or the focused branch after saving it, and opens it as a workspace `lfs:<name>` whose first pane is inside the branch.
- `commit`: saves the workspace on its branch.
- `drop`: discards the workspace without saving.
- `reopen`: mounts a saved branch again.
- `list` (**LayerFS: status**): projects and branches.

Two hooks: closing a LayerFS workspace saves and unmounts it; on Herdr startup, branches whose mounts are gone are mounted and reopened.

## How it works

The plugin calls `@layerfs/agent` from short Node processes that Herdr starts for each action and hook, and keeps a small state file mapping Herdr workspaces to branches. In docker mode (LayerFS `main`, Linux) the branch lives in the LayerFS Sandbox container and the workspace's pane enters it with `docker exec`.

On Debian 13 (4 vCPU, Docker 29, Herdr 0.9.5) with a 603 MB repo: import 3.2 s, a new branch workspace 0.07–0.19 s, save 0.06 s, discard 0.4 s. A server restart and a reboot-style restart (containers stopped) both brought back all 3 workspaces with their saved files.

`herdr plugin action invoke` acts on the workspace focused in Herdr's UI, so agents should run `lfs.mjs` directly: it then uses their own pane's workspace.

## You need

- Linux with Docker, Node 22+, Herdr 0.9+, and Rust 1.85.1 to build LayerFS.
- A LayerFS checkout at the pinned commit with the daemon and native binding built, a runtime image with git and a shell, and a Docker volume (the install prompt covers all three).

## Limits

- Docker mode reads the Store inside the Docker volume, so Herdr runs as a user that can read `/var/lib/docker/volumes` (root on a default install).
- Edits a branch has not saved are lost if the machine reboots.
- Uncommitted changes in the source repo are not imported; `HEAD` is.
- The repository also has a native mode for macOS (each branch a host folder and a linked git worktree, shown under the repo in Herdr's sidebar). It needs LayerFS PR #427, which is not on `main`, so it is not part of this listing.
