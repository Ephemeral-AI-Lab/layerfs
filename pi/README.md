# LayerFS × Pi

> **Status:** Current guide for the `pi-layerfs` package, checked with Pi 1.3.0 against real LayerFS on Linux and on macOS with colima.

`pi-layerfs` is a [Pi](https://github.com/earendil-works/pi) package. Start Pi with `--layerfs <project>` and the session works on its own LayerFS Branch of that project instead of the local folder: Pi's file tools read and commit on the Branch, every bash call runs in a fresh Workspace and commits what it changed, and the `layerfs` tool shows the history, merges and publishes to `main`. Without the flag Pi behaves as before.

## What you need

- **Linux, or macOS with a Linux build.** On Linux, Pi's process opens the project's Store itself, which LayerFS allows only there, with the Store on a local ext4, xfs or btrfs filesystem. On macOS its Store calls run in a Linux container; see [macOS](#macos).
- **A LayerFS deployment:** Docker, the built daemon and a runtime image, with the `LAYERFS_*` variables below set, as the [JavaScript SDK guide](../sdk/js/README.md) describes.
- **A project.** Each project's Store lives in its own Docker volume. As a user that can write Docker's volumes directory (such as root), make one from a folder:

  ```bash
  docker volume create shop-store
  export LAYERFS_VOLUME=shop-store
  export LAYERFS_STORE_PATH=/var/lib/docker/volumes/shop-store/_data/global/store.sqlite
  node --input-type=module -e 'const { LayerFS } = await import("/path/to/layerfs/sdk/js/src/index.js");
    const lf = new LayerFS("shop"); console.log((await lf.project.init("/path/to/shop")).kind); await lf.close()'
  ```

  It prints `Initialized`; `main` then holds the folder's files. See [project.init](../sdk/js/README.md) for the details.
- **Pi 1.3.0** and Node.js 20 or later.

## Install

```bash
pi install /path/to/layerfs/pi
```

Pi loads the package in place from this checkout. On Linux it imports the SDK from `sdk/js` beside it, and on macOS the ported SDK from `port/` beside it, so there is nothing else to install. To try it for one run, use `pi -e /path/to/layerfs/pi` instead.

Installing from a git URL (`pi install git:github.com/Ephemeral-AI-Lab/layerfs`) does not work: Pi clones the whole repository and looks for a package at its root, where there is none, so nothing loads and `--layerfs` is an unknown option.

## Configure

The package reads the SDK's own environment, nothing else: set the `LAYERFS_*` variables for the project as the [SDK guide](../sdk/js/README.md) lists them.

| Variable | Meaning |
| --- | --- |
| `LAYERFS_HOME` | Private folder where LayerFS records projects and Sandboxes |
| `LAYERFS_DOCKER` | Docker Engine socket (default `/var/run/docker.sock`) |
| `LAYERFS_IMAGE` | Runtime image digest a new Sandbox runs |
| `LAYERFS_VOLUME` | The project's Store volume |
| `LAYERFS_DAEMON` | Host path of the built daemon |
| `LAYERFS_STORE_PATH` | The project's Store file, `global/store.sqlite` in its volume |

`LAYERFS_NATIVE`, `LAYERFS_CONTAINER_PREFIX`, `LAYERFS_DAEMON_HOST` and `LAYERFS_TRACE` work as in the SDK.

## macOS

LayerFS makes Store calls only in a Linux process, so on a Mac the package uses the [ported SDK](../port/README.md), as LayerFS × DSH does. It needs Docker (checked with colima; Docker Desktop is what LayerFS × DSH uses) and a Linux build of LayerFS, the *bundle*: `layerfs-daemon`, `layerfs.node` and `sdk/js`, gathered as in the [LayerFS × DSH guide](../dsh/README.md#1-install-layerfs).

- When a session starts, Pi starts the LayerFS home's Store container, `<LAYERFS_CONTAINER_PREFIX>store` (`layerfs-store` by default), unless it runs. Every Store call of the session runs there, with the bundle's `layerfs.node`. A call that finds it stopped or gone fails with its own error, and Pi starts it again for the next call. It keeps running after Pi exits; `docker rm -f layerfs-store` stops it.
- `LAYERFS_BUNDLE` names the bundle. `LAYERFS_DAEMON` is its `layerfs-daemon`.
- `LAYERFS_DOCKER` is the Mac's Docker socket: with colima, `~/.colima/<profile>/docker.sock`. The containers reach Docker at the engine's own socket.
- `LAYERFS_STORE_PATH` is the path inside Docker's VM, as on Linux: `/var/lib/docker/volumes/<volume>/_data/global/store.sqlite`.
- The bundle, `LAYERFS_HOME` (mode 0700) and a folder you Init must be in a folder Docker's VM shares. colima shares only your home folder, so keep all three under it.
- A dsh Host on the same Docker with the same home shares the Store container. Give Pi its own `LAYERFS_HOME` and `LAYERFS_CONTAINER_PREFIX`: a Store container serving another home is refused, never taken over.

To make a project on a Mac, Init through the ported SDK:

```bash
docker -H unix://$LAYERFS_DOCKER volume create shop-store
export LAYERFS_VOLUME=shop-store
export LAYERFS_STORE_PATH=/var/lib/docker/volumes/shop-store/_data/global/store.sqlite
node --input-type=module -e "await (await import('/path/to/layerfs/port/src/index.js')).useStoreContainer(console.error);
  const { LayerFS } = await import('/path/to/layerfs/port/src/sdk.js');
  const lf = new LayerFS('shop'); console.log((await lf.project.init('$HOME/shop')).kind); await lf.close()"
```

## Your first session

```bash
mkdir -p ~/scratch/shop && cd ~/scratch/shop
pi --layerfs shop
```

- The session forks its own Branch from `main`, named from the session id (`pi-…`). Resuming the session finds the same Branch. `--layerfs-branch <name>` picks the Branch instead, forking it when it does not exist.
- The status bar shows `shop@<branch>`.
- The working directory stands for the Branch root `/`. It stays empty: the files are in LayerFS.
- Ask for work as usual. Each file change and each bash call becomes one Commit on the Branch. When the work is ready, ask Pi to publish it, or call the `layerfs` tool's `publish` yourself.

Pi is told this in its system prompt, along with the merge rule file `.lww` at the Branch root (`%include .gitignore`), which keeps generated files from conflicting when Commits merge.

## Tools

| Pi tool | LayerFS calls | History |
| --- | --- | --- |
| `read` | `fs.read`, one line window per call (2000 lines or 48 KiB) | none |
| `ls`, `find`, `grep` | `fs.list`, `fs.glob`, `fs.grep`, paged up to the tool's limit | none |
| `write` | `fs.apply`: the parent folder and the file | one Commit |
| `edit` | `fs.apply`: one exact replacement per edit, at the Branch's head | one Commit |
| `bash`, `!command` | `workspace.mount`, `exec`, `commit`, `unmount` | one Commit when files changed, also for a failing command |
| `layerfs` | `status`, `log`, `diff` (Branches or main), `diff_commits` (Commit ids from `log`), `merge`, `publish` | merge makes a Branch; publish adds a Layer to `main` |

When one message makes several calls, Pi runs them in parallel unless one writes: `write`, `edit`, `bash` and `layerfs` make the whole message's calls run in order, so a bash call sees the edits made before it. A path outside the working directory is refused. A LayerFS refusal fails the call with its code, such as `NoMatch: …` or `Conflict: …` with the conflicting paths.

## Limits

- A session works on one Branch for its whole life.
- A `--layerfs` session whose setup fails (on macOS without `LAYERFS_BUNDLE`, for one) shows the error, has none of Pi's file and bash tools, and refuses `!commands` with that error: it never falls back to the local folder.
- `read` reads text only; binary files and images are refused.
- `grep` matches literal text, case-sensitively, with no context lines or file filter. Use `grep -rnE` in bash for a regular expression.
- `find` and `grep` search everything, ignored files and `node_modules` included; `find` lists matching folders as well as files.
- Each bash call starts from a fresh Workspace: environment variables, the shell's directory and running processes do not carry over, only committed files do. Pi's `PI_*` variables are not passed in.
- A background process a bash call leaves running (`(server &)`) keeps its Workspace busy: the call's changes are committed, then the unmount is refused `Busy` and the call fails with that refusal. The Workspace stays mounted, since LayerFS unmounts nothing by itself, and its Sandbox keeps running.
- A bash call cannot be cancelled. Aborting it in Pi takes effect when the command ends, after its changes are committed. Pi's timeout runs the command under `timeout`; a command that itself exits 124 reads as timed out.
- If a bash call's Commit meets a `Conflict` (another writer changed the same lines meanwhile), the call fails with the conflicting paths and its Workspace is discarded.
- While an edit runs, Pi's preview, which reads the local file, shows it as missing; the finished call shows the Commit's diff.

## Develop

`tests/e2e.mjs` starts a Pi session with `--layerfs` on a fresh project and calls its tools as the model would: the Branch fork, one Commit per write and edit, an edit that keeps another writer's change, `ls`/`find`/`grep`, a path outside the folder, bash in a subfolder and a failing bash call, nothing left mounted, an edit and a bash call in one model message running in order (Pi's agent loop on its faux model), `log`, `diff_commits` and `publish`, and a session whose setup fails refusing a `!command`. It imports the Pi installed globally with npm (`NPM_CONFIG_PREFIX` selects another installation).

Run it with the rest of the `LAYERFS_*` deployment set, on a new volume each time. On macOS that includes the settings above (with colima, `docker -H unix://$LAYERFS_DOCKER volume create`); the test keeps its scratch folder in your home, which Docker's VM shares.

```bash
docker volume create pi-e2e-1
LAYERFS_VOLUME=pi-e2e-1 LAYERFS_STORE_PATH=/var/lib/docker/volumes/pi-e2e-1/_data/global/store.sqlite node pi/tests/e2e.mjs
```
