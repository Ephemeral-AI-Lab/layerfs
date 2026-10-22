> **Status:** Current general guide. Windows WSL2 support describes current main, added after the v1.0.0 source release.

<h1 align="center"><img src="../docs/assets/readme/layerfs-x-dsh.svg" alt="LayerFS × DSH" width="280"></h1>

<p align="center"><strong>Every agent gets its own Branch. You watch them all, live.</strong></p>

<p align="center">
  <a href="https://github.com/deepseek-ai/deepseek-harness"><img src="https://img.shields.io/badge/plugin%20for-DeepSeek%20Harness-7C3AED.svg" alt="Plugin for DeepSeek Harness"></a>
  <a href="../README.md"><img src="https://img.shields.io/badge/built%20on-LayerFS-A78BFA.svg" alt="Built on LayerFS"></a>
  <img src="https://img.shields.io/badge/node-%E2%89%A522.19-339933.svg?logo=node.js&amp;logoColor=white" alt="Node 22.19 or newer">
  <a href="../LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license"></a>
</p>

<p align="center">
  <a href="https://layerfs.ai/docs/integrations/dsh/"><img src="media/layerfs-dsh.gif" alt="A recorded dsh run at 2× speed: the Work tab shows an agent's fs terminal and Workspaces, then the History tab grows as a lead and five agents commit, merge and publish" width="860"></a>
  <br><sub>A recorded run: a lead and five agents build one application on Branches of their own (2× speed). <a href="https://layerfs.ai/docs/integrations/dsh/">Full video on layerfs.ai</a></sub>
</p>

<p align="center">
  <a href="#-install">Install</a> ·
  <a href="#-two-real-runs">Two real runs</a> ·
  <a href="#-your-first-session">Your first session</a> ·
  <a href="#-what-it-adds">What it adds</a> ·
  <a href="#-develop">Develop</a> ·
  <a href="../README.md">LayerFS</a>
</p>


`dsh-layerfs` is a plugin bundle for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) (dsh). It changes no dsh code.

With the bundle, agents in a dsh session work on a LayerFS project from bash, through short Node scripts that use the LayerFS JavaScript SDK. They fork Branches, test them in Sandboxes, and merge and publish, and you watch every agent's Branch grow, live. The conversation gets **Work** and **History** tabs that draw what each agent did, subagents included. The design is in issue #346.

## 📦 Install

The quickest way is to let a coding agent do it. Paste this into one running on the machine where dsh web will run, in a folder holding a clone of this repository:

```text
Set up LayerFS × DSH for me: the dsh-layerfs plugin bundle in DeepSeek Harness (dsh) web, on
real LayerFS.

1. Read dsh/README.md in https://github.com/Ephemeral-AI-Lab/layerfs, the "Install" section
   and its "Install step by step" steps. It is the guide: follow its steps in order and for
   this machine's OS (macOS, Linux, or Windows; Windows uses the WSL2 setup below).
2. Check what it needs first: Node 22.19 or newer, pnpm, Docker (Docker Desktop on macOS),
   network access, and a built deepseek-harness checkout
   (https://github.com/deepseek-ai/deepseek-harness) beside the layerfs repository; if there
   is none, clone it there and run pnpm install && pnpm run build in it. If something is
   missing that you cannot install, stop and tell me which.
3. Step 1, Install LayerFS: build layerfs-daemon and the SDK's native addon in the
   rust:1.85.1-bookworm image, gather them into the bundle folder, build the runtime image
   and note its image ID.
4. Step 2: in dsh/, run pnpm install and pnpm run build, with DSH_SOURCE set to the
   deepseek-harness checkout's absolute path.
5. Step 3: make a fresh profile with its own DSH_HOME (not ~/.dsh) and add only dsh-layerfs
   with dsh's plugin command. Do not add dsh-layerfs-demo.
6. Step 4: write layerfs-host's settings into the profile's cordis.patch.yml: the image ID,
   plus the bundle folder on macOS or the daemon path on Linux.
7. Step 5: start dsh web with that DSH_HOME and give me the printed
   http://127.0.0.1:…/?token=… link.
8. Step 6: run pnpm test in dsh/, and check that GET /layerfs/status reports no missing
   settings.

Report what you ran and what each step returned. A long cargo build may be run again until
it prints Finished, as the guide says; if anything else fails, stop and give me the exact
error, and don't retry it on your own.
```

It follows the six steps below and asks before anything it cannot do. Expect a few minutes for the first LayerFS build.

<a id="-install-into-a-dsh-profile"></a>
<details>
<summary><b>Install step by step: six steps, without an agent</b></summary>

These steps take a fresh clone of this repository to LayerFS × DSH in dsh web: first LayerFS itself, then the dsh bundle. Only dsh web is supported. dsh desktop serves the same web client but was not tested. The headless profile composes no preset registry, so it offers no preset; the Host and the bash environment still load there.

Live projects run on real LayerFS and need every step. Sim projects run on the simulator and need only steps 2, 3, 5 and 6.

### You need

| What | For | Notes |
|---|---|---|
| Node 22.19 or newer, and pnpm | everything | |
| A built deepseek-harness checkout | the client build and dsh itself | `../deepseek-harness` beside this repository, or `DSH_SOURCE=<path>` for the build. In it, `pnpm install && pnpm run build` (its README) |
| Docker | Live | Docker Desktop on macOS. It builds LayerFS and runs its Sandboxes |
| Network access | step 1 | crates and the base images |

Below, `$LAYERFS` is this repository's absolute path and `$DSH` the deepseek-harness checkout's.

### 1. Install LayerFS

Live needs three things from this repository: the JavaScript SDK, a Linux build of the daemon and the SDK's native addon, and a runtime image. Nothing is published, so you build them here.

**The SDK** (`sdk/js`, `@layerfs/sdk` and `@layerfs/sdk/operator`) is plain JavaScript with no dependencies, so it needs no install or build step. The Host links it as `link:../../../sdk/js`, and step 2's `pnpm install` makes that link. Only its native addon is built, and a fresh clone has none (`sdk/js/native/layerfs.node`). On macOS the Host never loads an addon on the Mac: the Linux build below supplies it. On Linux, see step 4.

**The Linux build.** Build `layerfs-daemon` and the addon in the Rust 1.85.1 image, from the repository root so Cargo picks up [`.cargo/config.toml`](../.cargo/config.toml) (the ARM64 profile). This is the recipe of [benchmark/AGENTS.md](../benchmark/AGENTS.md#environment), with the crates fetched into a Docker volume instead of the host's `~/.cargo/registry`:

```sh
cd "$LAYERFS"
docker pull rust:1.85.1-bookworm
docker volume create layerfs-cargo && docker volume create layerfs-target
mkdir -p core/target/cluster2-linux
docker run --rm -v "$LAYERFS":/work:ro -v layerfs-cargo:/usr/local/cargo/registry -w /work \
  rust:1.85.1-bookworm cargo fetch --manifest-path core/Cargo.toml --locked
docker run --rm -v "$LAYERFS":/work:ro -v layerfs-target:/work/core/target/cluster2-linux \
  -v layerfs-cargo:/usr/local/cargo/registry:ro -e CARGO_TARGET_DIR=/work/core/target/cluster2-linux -w /work \
  rust:1.85.1-bookworm cargo build --manifest-path core/Cargo.toml --offline --locked --release -j 6 \
  -p layerfs-node --lib -p layerfs-daemon --bins
```

The fetch takes about 30 seconds; on an arm64 Mac with Docker limited to 8 CPUs a cold release build took about 2.5 minutes at `-j 2` and 65 seconds at `-j 6`. Agents in this repository keep every command under 2 minutes: put `timeout 110` before `cargo` inside the container (`… rust:1.85.1-bookworm timeout 110 cargo build …`; macOS has no `timeout` command) and run the same command again until it prints `Finished`, since Cargo keeps what it compiled.

Then gather the build into one folder, the `bundle` (any folder Docker Desktop shares, such as one under your home):

```sh
BUNDLE=$HOME/layerfs-linux-build
mkdir -p "$BUNDLE/sdk/js"
docker run --rm -v layerfs-target:/t:ro -v "$BUNDLE":/out rust:1.85.1-bookworm sh -c \
  "cp /t/release/layerfs-daemon /out/ && cp /t/release/liblayerfs_node.so /out/layerfs.node && chown $(id -u):$(id -g) /out/layerfs-daemon /out/layerfs.node"
cp -R sdk/js/src sdk/js/operator sdk/js/package.json "$BUNDLE/sdk/js/"
```

On macOS the Host uses all three parts: the Store container loads `layerfs.node`, Sandboxes run `layerfs-daemon`, and the Mac side imports `sdk/js` from the bundle.

Instead, [`tools/worktree.py`](../tools/worktree.py) ([parallel worktrees](../docs/general/parallel-worktrees.md)) does the same with per-checkout caches, and seals the folder with a `manifest.json`. It needs the runtime image below to exist first, and was not tried for this guide:

```sh
python3 tools/worktree.py register --worktree "$LAYERFS" --owner dsh \
  --build-image rust:1.85.1-bookworm --runtime-image layerfs-352-runtime:node24 --sandbox-image layerfs-352-runtime:node24
python3 tools/worktree.py prepare --resources <printed resources.json> --platform linux
python3 tools/worktree.py build --resources <printed resources.json> --platform linux --profile release --jobs 6
```

`build` prints the sealed folder, `~/.cache/layerfs-worktrees/<id>/artifacts/build-linux-<hash>`, which serves as `bundle`.

**The runtime image** is the Linux image Sandboxes and the Store container run. The Store container runs `node` from it, so it must carry Node, plus whatever your agents' commands run in Workspaces. The one in use is built from [`tools/history/runtime/Dockerfile`](tools/history/runtime/Dockerfile): Node 24.21 on Debian bookworm (the daemon's own base), with corepack's pnpm and yarn prepared offline, and the base's git, python3, gcc and make.

```sh
docker build -t layerfs-352-runtime:node24 "$LAYERFS/dsh/tools/history/runtime"
docker image inspect --format '{{.Id}}' layerfs-352-runtime:node24     # the image setting
```

Use the ID your machine prints: the same build can get a different ID in another image store. Images are never pulled for LayerFS (the Store container starts with `--pull never`).

### 2. Build the dsh bundle

```sh
cd "$LAYERFS/dsh"
pnpm install
pnpm run build
```

`pnpm run build` bundles the client into `packages/client/lib/` with the checkout's `tsdown`: `$DSH_SOURCE/node_modules/.bin/tsdown`, else `../deepseek-harness` beside the repository. Prefer an absolute `DSH_SOURCE` path (`DSH_SOURCE="$DSH" pnpm run build`). Relative paths resolve from `dsh/packages/client/`, matching the original build command; an empty value uses the default checkout. The launcher passes the resolved absolute path to the client config. If the checkout or its tsdown CLI is missing, the build stops with `tsdown: No such file or directory`. Run it again after changing the client.

### 3. Make a profile

Choose a `DSH_HOME` of your own, so your usual `~/.dsh` is not touched, and use its `web` profile. dsh's `plugin` command makes the profile from the shipped `web` template on first use, installs the packages there with pnpm, and adds each one that is a bundle to the profile's `dsh.profile.bundles`:

```sh
export DSH_HOME=$HOME/dsh-layerfs        # any empty folder
pnpm --dir "$DSH" dsh plugin --profile web add "link:$LAYERFS/dsh/packages/dsh-layerfs"
```

Install `dsh-layerfs` only. The demo bundle `dsh-layerfs-demo` is disabled permanently and loads nothing.

The profile is `$DSH_HOME/profiles/web/`. Its `package.json` should now read:

```json
"dependencies": {
  "dsh-layerfs": "link:…/dsh/packages/dsh-layerfs"
},
"dsh": { "profile": { "bundles": ["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app", "dsh-layerfs"] } }
```

Making the profile by hand is the same: that `package.json`, then `pnpm install` in the profile folder. Keep the `pnpm-workspace.yaml` dsh writes there (`nodeLinker: hoisted`), so the bundle shares dsh's own packages.

### 4. Configure Live

The Host reads its deployment settings from `layerfs-host` in the profile's `cordis.patch.yml` ([`config.js`](packages/host/src/config.js)). dsh writes that file as `[]`; replace the `[]` with the entry list below.

| Setting | Linux | macOS |
|---|---|---|
| `image` | required | required |
| `daemon` | required | defaults to `<bundle>/layerfs-daemon` |
| `bundle` | not used | required |
| `docker` | `/var/run/docker.sock` by default | the same |
| `volumes` | `/var/lib/docker/volumes` by default (Docker's data-root); the Host's user must write it | not used |
| `home`, `traceDir` | `<OS temp>/dsh-layerfs/{home,traces}` by default | the same |
| `containerPrefix` | `layerfs-` by default: Sandboxes `<prefix><project>-<n>` | the same, and the Store container `<prefix>store` |
| `stateDir` | `<profile>/dsh-layerfs` by default (bindings, agent names, journals) | the same |

A Live Init without the required settings is refused with "LayerFS is not configured: set … for layerfs-host in the profile patch"; `GET /layerfs/status` lists what is `missing`.

```yaml
# $DSH_HOME/profiles/web/cordis.patch.yml
- id: layerfs-host
  config:
    image: "sha256:<64 hex>"                   # step 1's image ID
    bundle: /Users/<you>/layerfs-linux-build    # macOS: step 1's folder
    # daemon: /path/to/layerfs-daemon           # Linux
```

**On macOS** the Mac runs no Store call itself. The Host starts one long-lived Linux container per LayerFS home, the Store container (`<containerPrefix>store`, `layerfs-store` by default, labelled `layerfs.role=store`; [`box.js`](packages/host/src/box.js)), and every SDK call of the Host and of agents' scripts runs there with the bundle's `layerfs.node`. A new `bundle` or worker replaces it when the Host starts it (at Host start and before an Init), so load one only while no dsh run is live. It runs with `--restart unless-stopped`, so Docker starts it again after its own restart. A call of the Host or an agent that finds it stopped, missing or not answering keeps its own error, and the Host starts it again once (`docker start`, or a new one if it is gone) and waits for its worker; a running container that does not answer is left as it is. The Host removes an older-named `dsh-layerfs-box-<hash>` container of its home, and the containers of the retired `<home>/_sandboxes` records, when it starts.

**On Linux** the Host and agents' scripts load the SDK in their own process, from `sdk/js` in this repository. It loads the addon from `LAYERFS_NATIVE`, or else `sdk/js/native/layerfs.node`: build it on that machine with `npm --prefix sdk/js run build:native` ([SDK README](../sdk/js/README.md#builds-and-compatibility); it needs Cargo 1.85.1 on the host), or set `LAYERFS_NATIVE` in dsh web's environment to a Linux build's `layerfs.node`. `daemon` is that build's `layerfs-daemon`. The Store must be on a local Linux filesystem (ext4, xfs or btrfs). Init installs the Store straight into the new project's volume directory under Docker's volumes directory (`volumes`, `/var/lib/docker/volumes` by default), which Docker keeps root-only, so run dsh web as a user that can write it, such as root, and give it a `home` that user owns; otherwise Init refuses with `host-access` before it makes a volume.

<a id="windows-live"></a>

**On Windows** dsh web and the default profile remain on Windows. The separate
[`windows/` adapter](packages/host/src/windows/host.js) starts one long-lived Node
worker in a selected WSL2 distribution. That distribution needs its own running
Docker Engine, `/dev/fuse`, Node 22.19 or newer, and a **sealed Linux bundle** from
`tools/worktree.py build` (run the build inside WSL, using the private resources
described above). Docker Desktop's Windows named pipe is not used by this adapter.

Install and build the plugin on Windows with steps 2–3. You can add it to your
existing default `web` profile; omit the example's `DSH_HOME` override to do so.
Keep that profile's existing patch entries and add:

```yaml
- id: layerfs-host
  config:
    image: "sha256:<exact image ID from the WSL Docker Engine>"
    bundle: /home/<linux-user>/layerfs-build   # sealed Linux bundle, including manifest.json
    wslDistribution: Ubuntu
    # wslUser: root                          # default; must access Docker and its Store volumes
    # wslNode: /usr/local/bin/node             # default; absolute Linux executable path
    # wslHome: /var/lib/dsh-layerfs-windows/my-profile
    containerPrefix: layerfs-windows-
```

`daemon` defaults to `<bundle>/layerfs-daemon`. `bundle`, `daemon`, `wslNode`, and
`wslHome` are Linux paths in that distribution. `home`, `traceDir`, and `stateDir`
remain Windows paths. The default Linux home is a persistent directory under
`/var/lib/dsh-layerfs-windows`, derived from the Windows home. Stores live in the
WSL Docker Engine's named volumes on its local Linux filesystem. Never put the
Store or Linux discovery home on `/mnt/c` or another Windows-backed mount.

The Windows SDK entry preserves the regular SDK API for Host clients and agents.
It forwards native calls to the worker; the product SDK and daemon are unchanged.
Imports translate drive paths with that distribution's `wslpath`; WSL UNC paths
must name the selected distribution. Init reads everything through the Linux
filesystem view, including `.git` and ignored files. Windows ACLs are not converted
to POSIX metadata; imported metadata is what WSL exposes.

The worker validates the sealed bundle's file hashes, listens only on Linux
loopback, and requires a random secret delivered over its startup pipe. Its
Windows endpoint file is ACL-restricted to the current Windows user and SYSTEM.
The Windows SDK source must match the sealed Linux SDK (allowing Git line-ending
normalization). Windows binding-file readers share the writers' short admission
lock so reads cannot overlap atomic file replacement; failed writes are not retried.
Each project's native Store stays open while used and closes after a quiet minute.
Project removal explicitly releases its cached Store before deleting the volume.
Disposal drains admitted calls without killing mounted commands. Runtime Start
and confirmed Restart act on this worker; a silent or busy worker is retained.
Changing WSL deployment settings requires restarting dsh.

A request lost before it is sent is `Failed`; a sent request whose reply is lost
is `Unknown` and is never replayed. Failed Windows Init attempts retain their
volume and Linux discovery state, with the original failure and volume recorded
in `windows-init-failures.jsonl` under `stateDir`. Inspect that evidence before
deciding how to recover. The macOS container adapter and Windows WSL adapter
share the worker codec, but keep separate deployment and lifecycle code.

The codec is versioned JSON with explicit binary values, independent of V8
serialization versions. Upgrade the Host and worker together, with no active
calls; old V8 workers are incompatible and must be restarted. A protocol
failure is reported explicitly and never replays a call. An uncertain Init reply
also retains the macOS home and volume, recorded in `init-failures.jsonl` under
`stateDir`, so protocol errors cannot trigger deletion of a completed Init.

This setup uses the repository pnpm workspace and a dsh source checkout. Packaged
dsh desktop and npm/plugin-market installation are not qualified
([#406](https://github.com/Ephemeral-AI-Lab/layerfs/issues/406)). Docker Desktop
volume visibility from a WSL Linux host is a separate unsupported topology;
see [#380](https://github.com/Ephemeral-AI-Lab/layerfs/issues/380).

From the dsh checkout, restart your normal Windows `dsh web` server, keeping its
usual port (for example `--port 3080`). `GET /layerfs/status` should report an empty
`missing` list. The LayerFS runtime page reports both the WSL worker and its Docker
Engine. This does not require running a second dsh server in WSL.
The [verification record](docs/windows-live-verification-20261022.md) pins the
tested source, native bundle, outcomes and platform gaps.

**On macOS without Docker (`sandbox: native`).** With `sandbox: native` in `layerfs-host` (or `LAYERFS_SANDBOX=native` in dsh web's environment) the Host and agents' scripts load the SDK in their own process, as on Linux, and need only `daemon`: a macOS build's `layerfs-daemon`, with that build's `layerfs.node` in `sdk/js/native/` (agents' bash calls are not given `LAYERFS_NATIVE`). There is no image, bundle, Store container or Docker volume: each project's Store is a folder `<stores>/<volume>` on the Mac's APFS, and Sandboxes are local daemons under `localDir` whose Workspaces macOS's own NFS client mounts (`mount_nfs`, no root, no macFUSE), with commands run as you ([native Sandboxes](../sdk/js/README.md#native-sandboxes-macos)). `localDir` and `stores` default to `<OS temp>/dsh-layerfs/{local,stores}`, inside dsh's Workspace Write sandbox, so agents' bash calls can also work in a mounted Workspace's folder. `nfsMount` replaces the daemon's mount command when the default options fail. Removing a project is refused `busy` while one of its Sandbox daemons runs or one of its Workspaces is mounted (a Sandbox closes 2 minutes after its last unmount). On this route the All projects page shows no running-Sandbox cards (a project's Sandboxes tab does), and background processes (`&`) are not available yet.

**Running two dsh webs on one machine.** The default `home` and `traceDir` are the same for every profile, so two dsh webs would share projects and the Store container. Give each its own pair under the OS temp area, and its own `containerPrefix`, since container names are per Docker engine (the Store container of another home is refused, never taken over):

```yaml
    home: /private/var/folders/…/T/dsh-layerfs-<name>/home        # node -p "require('fs').realpathSync(require('os').tmpdir())"
    traceDir: /private/var/folders/…/T/dsh-layerfs-<name>/traces
    containerPrefix: <name>-
```

Keep both in the OS temp area. Bash calls in Workspace Write mode run in dsh's sandbox, which lets them write only there, and a LayerFS call writes both (a mount elsewhere fails `PermissionDenied`).

### 5. Start dsh web

```sh
DSH_HOME=$HOME/dsh-layerfs pnpm --dir "$DSH" dsh web --no-open --port 3090
```

Open the printed `http://127.0.0.1:3090/?token=…` link. Run the preset's sessions in Workspace Write mode; subagents inherit it. Agents use dsh's own model, which needs dsh's sign-in or keys, as usual. A fresh `DSH_HOME` has neither: sign in from the page's account menu.

What you see ([TUTORIAL.md](TUTORIAL.md) walks through a first run with pictures):
- A fresh `DSH_HOME` has no workspace. **Add workspace** (the sidebar's folder button, or the new session's workspace chip) picks one; the preset and project chips appear once a workspace is chosen. On macOS, dsh web's folder picker is the system's folder panel, which opens in front of the browser, not in the page.
- **LayerFS × DSH minimalist** is the default preset of a new session. A preset picked in dsh's settings still wins.
- The New Session row has a **project chip** beside the preset chip. Its menu lists every project (the workspace folder's first) and "No LayerFS", with **New empty project** (a random name, such as calm-otter) and **Import project…** below. Import opens the same folder picker and imports the folder as a new project, which becomes the workspace folder's default; importing the same folder again gives `name-1`, `name-2`, …. Both make Live projects. Picking a project switches a blank session to the minimalist preset.
- A session on a project has two more tabs beside Chat and Trajectory: **Work** (the agents' terminals, Branches and Sandboxes) and **History** (File, History, Storage and Sandboxes: the project's Sandboxes tab, as on the LayerFS page).
- The sidebar's **LayerFS** page shows one project at a time: its Files, History, Storage and Sandboxes, a New session action, and Refresh and Remove. Its **All projects · Sandboxes** tab is the switcher: a card per project with its Sandboxes, below Docker, the Store container and free disk.

A **Sim project** needs no Docker, image or build, but the chip's Import makes Live projects. Ask the Host for one directly. The `?token=` link sets the cookie the route needs:

  ```sh
  curl -s -c jar.txt -o /dev/null 'http://127.0.0.1:3090/?token=…'
  curl -s -b jar.txt -H 'content-type: application/json' \
    -d '{"folder":"/abs/workspace/folder","backend":"sim","name":"my-sim"}' http://127.0.0.1:3090/layerfs/init
  curl -s -b jar.txt http://127.0.0.1:3090/layerfs/init/<job id>     # until "step": "done"
  ```

  The project becomes that folder's default, and the chip lists it in every workspace. Leave out `backend` for a Live Init the same way.

### 6. Check

```sh
cd "$LAYERFS/dsh" && pnpm test
node tools/check-session.mjs 'http://127.0.0.1:3090/?token=…' <session id>
```

`pnpm test` runs the Host, simulator, model, mock, playback and history tests. `check-session.mjs` checks that a finished session's tabs draw what LayerFS reports. The page's URL does not carry the session id: a session is the folder `$DSH_HOME/sessions/<workspace>/session-<uuid>`, whose name is the id.

</details>

## 🏁 Two real runs

On 2026-10-20, one dsh main agent and seven worker subagents on `deepseek-flash` took an empty LayerFS project to a published, verified `main`, twice: a small app in under an hour and a hard one in an afternoon. Every change went through the SDK as Branches, Commits, merges and publishes.

<table>
  <tr>
    <td width="50%" valign="top"><a href="demo/wayfare/README.md"><img src="demo/wayfare/assets/history-sideways.png" alt="Wayfare's History at the end of the run: main at L12 after 147 Commits"></a></td>
    <td width="50%" valign="top"><a href="demo/quarry/README.md"><img src="demo/quarry/assets/work-store-stats.png" alt="Quarry's storage at the end of the run: 8.90 GB on disk for 4.93 TB of history"></a></td>
  </tr>
  <tr>
    <td valign="top"><b><a href="demo/wayfare/README.md">Wayfare</a></b>: a collaborative trip planner in Node 24, with no third-party dependencies.</td>
    <td valign="top"><b><a href="demo/quarry/README.md">Quarry</a></b>: a SQL database in Rust, checked row by row against SQLite, with every build's <code>target/</code> committed.</td>
  </tr>
</table>

| | Wayfare | Quarry |
|---|---|---|
| Time | 56 min, 8 agents | 4 h 16 min, 8 agents |
| History | 147 Commits, 31 Branches, 13 Layers | 913 Commits, 56 Branches, 9 Layers |
| Checks | 334/334 tests, A01–A12 pass | 891 tests, 800/800 sqllogictest, Q01–Q12 pass |
| Store | 1.21 MB on disk for 91.5 MB of snapshots, 27× dedupe | 8.90 GB on disk for 4.93 TB of snapshots, 166× dedupe: less than one copy of `main` (13.5 GB) |

Each record has the task, the spec, the system prompt, the progress ledger, the published build report and every number above. The screenshots are from the end of each run, taken with an earlier version of the tab, before it split into Work and History.

## 🚀 Your first session

[TUTORIAL.md](TUTORIAL.md) walks through a first run step by step, with 19 screenshots: an import, two subagents on Branches of their own, a merge and a publish.

**1. Start a session on a project.** In dsh web, start a new session. Its row has the preset chip, which reads **LayerFS × DSH minimalist**, and beside it the **project chip**. Open the project chip and pick **New empty project**, or **Import project…** to choose a folder and import it as a new project; a card under the chip shows the import's progress. The session's project is fixed at its first bash call that uses it.

<table>
  <tr>
    <td width="50%" valign="top"><img src="media/first-project-menu.jpg" alt="The project chip's menu: every project, No LayerFS, New empty project and Import project…"><br><sub>The project chip's menu.</sub></td>
    <td width="50%" valign="top"><img src="media/first-importing.jpg" alt="The project chip reads Importing tutorial-site…, with a progress card under it"><br><sub>An import in progress.</sub></td>
  </tr>
</table>

**2. Switch to Workspace Write.** Set the session's mode to **Workspace Write**. Bash calls then run in dsh's sandbox, which lets LayerFS write its home and traces; subagents inherit the mode.

**3. Give it a task.** The preset already knows the SDK and its rules, so ask for work as you would any agent. Work that splits across subagents shows the most. For example:

```text
Build a small two-page website in this project: a home page and an about page that link to
each other, plus a check script (node check.mjs) that fails if either page is missing or a
link is broken.

Give each page to its own subagent, each on its own Branch forked from main. Each subagent
runs node check.mjs in a Workspace before it commits. When both are done, merge their
Branches into one, fix any Conflict, run the check once more, and publish to main.
```

**4. Watch the Work tab.** **Work** sits beside Chat and Trajectory. Its chips are `main` and the Branches. On `main` you see the Store's size and the project terminal, which prints every Branch call (fork, merge, publish) with the agent that made it. A Branch's chip shows its `fs` terminal, the Store reads and writes on that Branch, and one pane per Workspace mounted on it, with the commands run there. Calls appear in step with the run, and **Follow** keeps up with the newest.

<table>
  <tr>
    <td width="50%" valign="top"><img src="media/work-main.webp" alt="The Work tab on main: storage, 7.51 MB on disk for 2.57 GB of history, the Branches list, and the project terminal"><br><sub><code>main</code>: the Store's size and the project terminal.</sub></td>
    <td width="50%" valign="top"><img src="media/work-branch.webp" alt="The Work tab on a Branch: the fs terminal with one Commit per apply, and two agents' Workspace panes"><br><sub>A Branch: its <code>fs</code> terminal and each agent's Workspace.</sub></td>
  </tr>
</table>

**5. Watch History grow.** **History** draws every Branch and Commit, which agent made it, and the Layers on `main`, one per publish. It grows as the agents work; **Play** replays it from the start. **File** shows the files at any point and **Storage** what the Store holds.

<p align="center"><img src="media/history.webp" alt="The History tab: main with Layers L6 to L10, and agents' Branches forking, committing and merging around it" width="860"><br><sub>History: 110 Commits on a dozen Branches, <code>main</code> at L10.</sub></p>

**6. Come back to it later.** The sidebar's **LayerFS** page shows one project at a time: where `main` is and what was published last, Files beside File, History, Storage and Sandboxes (click a History node to see the files at that point), **New session** to start another session on the project, and, in its ⋯ menu, **Remove project…**, which asks for the project's name first. Its **All projects · Sandboxes** tab is the switcher: a card per project with its Sandboxes, below Docker, the Store container and free disk.

<table>
  <tr>
    <td width="50%" valign="top"><img src="media/first-layerfs-page.jpg" alt="The LayerFS page for tutorial-site: main's files beside its History, two subagent Branches merged and published as L1"><br><sub>A project: <code>main</code>'s files and its History.</sub></td>
    <td width="50%" valign="top"><img src="media/first-all-projects.jpg" alt="All projects · Sandboxes: Docker, the Store container and free disk, then a card per project"><br><sub>All projects · Sandboxes.</sub></td>
  </tr>
</table>

## ✨ What it adds

| Piece | Package | Where it shows |
|---|---|---|
| Host plugin `layerfs-host` | `@dsh-layerfs/host` | bash environment, session records, the `/layerfs` web route |
| Client plugin `layerfs-client` | `@dsh-layerfs/client` | Work and History tabs, project chip, LayerFS page (dsh web) |
| Agent preset `layerfs-dsh-minimalist` | `dsh-layerfs` (`presets/`) | "LayerFS × DSH minimalist" in the preset menu |
| View model | `@dsh-layerfs/model` | used by the client and the Host |
| Renderer | `@dsh-layerfs/view` | used by the client |
| Mock model `layerfs-mock` (demo bundle `dsh-layerfs-demo`) | `@dsh-layerfs/mock` | disabled permanently (owner, 2026-10-21): not installed or released |

The Work and History tabs and the LayerFS page are the only LayerFS UI. The standalone HTML pages that used to live in the root `sdk_sim/` (the terminal UI page and its design preview) and their live server are removed; do not build or revive a standalone HTML or TUI page. What remains, the simulation engine, lives in [`sim/`](sim/): `simulation_engine/model.js` and `files.js`, which the client and the Host import, the terminal engine (`terminal.js`, `backend/sim.js`) and demo stories (`demo.js`) that the demos were written from, and the avatars (`avatars/`).

## ⚙️ How it works

dsh session (agents, subagents, bash) → `layerfs-host` (bash environment, records) → LayerFS JS SDK (short Node scripts) → Store and Sandboxes.

A bash call in a session bound to a project gets the project, the SDK and a trace file per agent (`DSH_LAYERFS_*`). Each finished SDK call becomes a `plugin:layerfs/call` record in the calling agent's own session, and the client plugin reads those records back to draw the tabs. On Linux, the Host and the agents' scripts open the Store in their own process. On Windows, SDK calls run in the authenticated WSL worker, whose Docker Engine owns the Sandbox volumes. On macOS the Mac runs no Store call itself: the Host starts one long-lived Linux container, `layerfs-store`, and every SDK call runs there. Agents' Workspaces are placed in the project's numbered Sandboxes, 16 to a Sandbox, and each Sandbox closes itself two minutes after its last Workspace is unmounted.

## 📖 Reference

<details>
<summary><b>🔌 Host plugin: bash environment, records, projects, the /layerfs route</b></summary>

- **Bash environment.** A bash call in a session bound to a project gets:
  - `DSH_LAYERFS_PROJECT` and the deployment settings;
  - `DSH_LAYERFS_SDK` and `DSH_LAYERFS_OPERATOR` (file URLs of the SDK);
  - a trace file per agent (`DSH_LAYERFS_TRACE`) and the bash call id as the trace step.

  The SDK reads the `DSH_LAYERFS_*` names as fallbacks for its own `LAYERFS_*` settings.
- **Records.** Each finished SDK call becomes a `plugin:layerfs/call` record in the calling agent's own session. The root session gets one `plugin:layerfs/project` record.
- **Journal.** Finished lines are also journaled per root session under the state folder, because trace files live in temporary storage.
- **Projects.** Each project is attached to at most one workspace folder, or detached; a folder can have several. Init imports a source folder (the workspace folder unless another is chosen) and attaches the new project to the workspace folder. A session uses the folder's only (or last used) project unless it picks another or none. The choice is fixed at the first bash call that uses it.
- **Agent names.** Subagents are named by the label their parent gave them.
- **Web route.** `/layerfs` sits behind dsh's own connection checks:

  | Request | Answer |
  |---|---|
  | `GET /layerfs/status` | configuration |
  | `GET /layerfs/workspace?folder=` | a folder's projects |
  | `POST /layerfs/init {folder, source?, name?}` | check the request and start Init; answers its job at once |
  | `GET /layerfs/init/<job>` | the Init's step (`scan`, `volume`, `import`, `done` or `failed`), the source's file and byte counts, then its project or error |
  | `GET /layerfs/session/<id>` | project, agents and trace lines of the session's root |
  | `POST /layerfs/session/<id>/project {project}` | choose a project, or `null` for none |
  | `GET /layerfs/session/<id>/state` | the project as `inspect.snapshot` reports it |
  | `GET /layerfs/session/<id>/tree?ref=<saved id>` | a Commit's or Layer's files |
  | `GET /layerfs/projects` | every project and the workspace folder it is attached to |
  | `GET /layerfs/project/<name>/state` | the project as `inspect.snapshot` reports it, plus who made each node and when Layers and forks happened, from the journals |
  | `GET /layerfs/project/<name>/tree?ref=<saved id>` | as the session's tree route |
  | `POST /layerfs/project/<name>/attach {folder}` | attach to a workspace folder, or `null` to detach |
  | `POST /layerfs/project/<name>/remove {confirm}` | remove the project; `confirm` must repeat its name |

  Init first counts the source (every entry, links not followed, as LayerFS reads it), then makes the volume, then runs LayerFS Init (in the WSL worker on Windows, or a container of its own on macOS), which scans, imports and installs the Store into the volume in one process; it starts no Sandbox. While LayerFS imports, the Host reads its counts every 250 ms (the SDK operator's `project.progress()`: step, entries scanned, files and bytes built) into the job's `imported`. The page reads the job every 300 ms and fills the bar from those counts over the Host's totals: scan 0–15%, files 15–95% (bytes plus 4 KB per file), then tree, publish and seal.

  Removal deletes the project's Docker volume and LayerFS home entry, and drops it from the bindings; sessions on it become unbound. It is refused while any container still uses the volume. Workspace files and transcripts stay. A LayerFS project is its home entry, so a removed name can be initialized again at once.

  The Host reads each project (state and trees) through one shared SDK client, released after a quiet minute. Reading starts no Sandbox: history comes from the project's Store (in the WSL worker on Windows or the Store container on macOS), and Workspaces only from Sandboxes already running, so a project nobody works on shows its history with no container. LayerFS places agents' Workspaces in the project's numbered Sandboxes, `layerfs-<project>-<n>`, 16 to a Sandbox; each Sandbox closes itself two minutes after its last Workspace is unmounted, and the tab then draws it closed.

  The Files pane reads only what is on screen, one SDK call each ([`reads.js`](packages/host/src/reads.js)): a folder a page of up to 1,000 names at a time, a file a window of at most 2,000 lines or 48 KiB. A read costs the same in a small project and a huge one. `node_modules`, `.git` and `.next` start collapsed.

</details>

<details>
<summary><b>🖥️ Client plugin: Work and History tabs, project chip, LayerFS page</b></summary>

- **The Work and History tabs** (beside Chat and Trajectory) replay the root session's lines while one is shown:
  - Work: the project terminal with agent tags, the Branches and Sandboxes, and the agents' avatars;
  - History: File, History, Storage and the project's Sandboxes.

  An unbound session shows Setup: pick one of the folder's projects, or Init.

  **Playback.** The tab plays calls in step with the run, for real agents and demos alike ([`playback.ts`](packages/client/src/client/playback.ts)):
  - each call shows 2 s after it ended, so the gaps on screen are the run's own gaps and the tab never falls behind. The 2 s are longer than the Host poll (1.5 s), so a call is usually here before its time;
  - calls whose time has passed (the tab opened, a rebuild when an agent is named, a late poll) appear at once, drawn once. Nothing plays twice;
  - a bash call that mounts, runs, commits and unmounts a Workspace within a second shows each step at its time, and the Workspace's pane then fades out over 1.2 s ("unmounted"), a trail kept by the view only ([`view.js`](packages/view/src/view.js)).

  **Demo speed.** A demo's strip, above the composer, has a speed control beside Sim and Live: 1×, 2×, 4× and Max. It sets how fast the mock runs the demo, not how fast the tab animates it: the speed divides the scenario's pauses and the typing between streamed words, and Max leaves them out ([`pace.js`](packages/mock/src/pace.js), `POST /layerfs-demo/<id>/speed`). A pause under way follows a change at once. Commands, LayerFS calls and waits on other agents take their own time at any speed. A demo starts at its scenario's `speed`, 1× for every demo. Where a scenario's pauses order two agents' commits (5 agents: ben's `--lint` lands before cy's `--profile`, which then conflicts), a faster speed narrows that margin while the commands keep their time, so a planned Conflict may not happen; check-demo then fails it. It applies to a run under way too; the LayerFS tab has no speed control.

  **Terminals and their command lines.** Every SDK call an agent makes prints one command line in one terminal. The single table for both is [`packages/model/src/cli.js`](packages/model/src/cli.js); `node packages/model/tests/cli.mjs` fails when an SDK or operator method has no row there.

  | Terminal | Prints | Calls |
  | --- | --- | --- |
  | project (one per project) | the project and its Branches | `project.*`, `branch.*`, and a mount that failed (it has no pane); an older trace's `sandbox.create` and `sandbox.close` |
  | fs (one per Branch) | Store reads and writes on that Branch | `fs.*` |
  | workspace (one pane per mounted Workspace) | the Workspace's life and the commands run in it | `workspace.mount`, `workspace.exec`, `workspace.status`, `workspace.commit`, `workspace.unmount` (an older trace's `sandbox.*` forms too), operator `process.start` and `process.cancel` |
  | none | the client's own calls and operator reads | `operator.sandbox.list`, the other operator methods |

  A command line is `<group> <method> <required args> [--optional value] [-m "message"]`, using the SDK's own group and method names: `branch merge feature try-ui release -m "join"`, `fs read /app/page.tsx --start-line 5`, `workspace commit -m "add hero" --force`.
  - Required arguments are positional, in SDK order. Optional ones are `--kebab-case` flags shown only when they differ from the SDK default; a true boolean is a bare flag (`--force`, `--discard`).
  - An image is shortened to `sha256:` and 12 hex digits. File content is never printed, and `fs apply` shows its change count.
  - A line does not repeat what its terminal names. The fs terminal hides the `fs` group and the Branch (`read /app/page.tsx`); a workspace pane hides the `workspace` group, the Sandbox and the path (`commit -m "add hero"`, `unmount --discard`), but keeps the operator's `process cancel`. The project terminal prints every group. `fs diff_files` prints both of its refs.
  - A command run in a Workspace prints as the shell command itself, and a background start ends with ` &`.
  - Reads (`list_*`, `diff_*`, `fs read/list/stat/grep/glob/blame/diff_files`, `workspace status`) and `workspace mount` are dimmed.
  - A call with no row prints its own name, dimmed, in the project terminal.

  The composer is hidden while the tab shows; switch to Chat to send a message.
- **The project chip** sits in the New Session row beside the preset chip, drawn like dsh's own chips. Its menu lists the folder's projects and "No LayerFS", with "Import project…" pinned below. Like "Add workspace…", Import opens the directory picker (Finder in dsh web on a Mac); the chosen folder becomes a new project attached to the workspace folder and is chosen for the session. While it imports, a card under the chip shows the percentage, the step with its counts ("Importing 12,000 of 30,000 files · 40 MB of 90 MB") and the elapsed time over a bar. The bar moves without a fraction while the Host counts the source; the Init dialog shows the same.
- **The LayerFS page** (sidebar) has one tab per project. Each shows:
  - ellen_joe, with a line on where `main` is and what was published last;
  - the workspace the project is attached to, which you can change or detach (a session already on the project keeps it);
  - History and Files side by side, where clicking a History node shows Files at that point;
  - "New LayerFS session" in the attached workspace, on the preset;
  - "Remove project…", which asks for the project's name before removing it.

  Workspaces without a project offer Init.
- **The Init dialog** (Setup and the page) imports the workspace folder by default. "Choose folder…" browses for another source: in the page where dsh offers it, otherwise with the native picker. The project is attached to the workspace folder either way. The name is optional and defaults to the source folder's.

</details>

<details>
<summary><b>🎛️ Preset</b></summary>

The preset has bash with background jobs, subagents (spawn) with their control tools, and compaction. It has no read, write or edit tools. Its persona carries:
- an SDK cheat-sheet with a stdin script template;
- the working rules: one Workspace per agent, merge conflicts belong to whoever merged, only the main agent publishes, run again on `Busy`, unmount every Workspace.

Run the preset's sessions in Workspace Write mode; subagents inherit it.

</details>

<details>
<summary><b>🧪 Simulated projects</b></summary>

Sim needs no Docker, image or daemon. `POST /layerfs/init` with `backend: "sim"` imports the folder into the simulator (`packages/host/src/sim/`) instead of a Store. A simulated project's bash calls get the SDK on the simulator as `DSH_LAYERFS_SDK`, so the same scripts run on both:
- `sim/sdk.js` is the SDK itself with `LAYERFS_NATIVE` set to `sim/binding.js`, a native binding in JavaScript. The SDK's argument checks, refusals and trace lines are unchanged.
- The binding's decisions (Commits, stale Workspaces, merges, conflict markers, `.lww`, publish) are the simulation engine's model (`sim/`). Its answers have the real binding's shapes and LayerFS's id formats, so the views and checks read both alike.
- The state is `<LayerFS home>/<project>/sim/state.json`, changed by one call at a time under a lock file.
- A mounted Workspace is a folder, `sim/ws/<sandbox>/<n>`. Commands run there with `/bin/sh`, and each call reads the folder back. On macOS a GNU-compatible `sed` (`sim/sed.js`) comes first on the command's PATH.
- Background processes run detached and leave their exit code in `sim/proc/`. Removing the project stops them.

The Host's own reads of a simulated project (Init, state, trees, storage) go to the binding directly. Store sizes are estimates from file text. Sim runs commands on the Host without a container.

Known limits:
- dsh's sidebar orders sessions by last activity by default, so the demo played last moves to the top. The manual order keeps each demo's place.
- Sim imports the simulation engine's model from `dsh/sim/`, so the Host package needs this `dsh/` tree beside it to Init or read a simulated project.
- `demo-workspace` is inside this repository. A turn's "Edited" card lists every change made in the repository during the turn, including other work in the checkout.

</details>

## ⚠️ Limits

- Only dsh web is checked. dsh desktop serves the same web client but was not tested; the headless profile has no preset registry, so the preset is not offered there.
- A session's project is fixed at its first bash call that uses it.
- Sessions run in Workspace Write mode. Windows defaults agent traces to
  `<workspace>/.dsh-layerfs-traces`, because its restricted shell can write
  the workspace and its private temp directory, but not the server’s temp
  directory. The Host retains each trace path across restarts. An explicit
  `traceDir` must be writable by the agents. Other platforms keep the default
  traces in the OS temp area.
- Everything else follows LayerFS's platform limits: Store calls need Linux and a local ext4, xfs or btrfs filesystem, which on macOS the Store container provides.
- Windows Live uses WSL2 and its Linux Docker Engine through the Windows adapter;
  native Windows Store execution remains unsupported. Sim's local `/bin/sh`
  executor is not ported to Windows. macOS execution was not rerun for this change.

## 🧰 Develop

| Command | Does |
|---|---|
| `pnpm test` | Host tests, the mock model's scenario tests, and the model's trace replay check |
| `node tools/init-project.mjs <profile dir> <folder> <image> <daemon>` | Init outside dsh |
| `node tools/demo.mjs [<demo> <folder>]` | lists the demos, or copies the demo website into another folder |
| `node tools/check-demo.mjs '<dsh web URL with ?token=…>' <session id> <demo> [--wait <s>]` | checks a finished demo session: every agent ran, and each bash step with `expect` left those LayerFS outcomes on the trace. For a replayed run also: every Sandbox of the project closed itself (it waits up to `--wait` seconds, default 150), every Branch's history is as recorded, and `main`'s tracked files match `final.json` |
| `node tools/run-scenario.mjs <demo> --out <dir> [--pace] [--keep] [--wait <s>]` | plays a replayed demo headlessly on Live LayerFS, without dsh web or the mock model, agents at the same time, then runs the same gates and writes `receipt.json`. Pauses are skipped unless `--pace`; order comes only from the recorded order and `until` |
| `node tools/restore-store.mjs <id> <folder with global/> <work dir> <project>` | opens a copy of a stopped Store in a new volume and LayerFS home, for `import-run.mjs` (needs `LAYERFS_READER`, a checkout built at the Store's schema) |
| `node tools/history/plan.mjs --repo <dsh.git> --out <dir> [--seed 352] [--from N]` | the #352 history replay plan: ranges, lanes, agents, steps and planned conflicts, from a full bare clone of deepseek-harness; checks that each range's plan alone reaches its checkpoint. `--from N` rewrites only ranges N and later |
| `node tools/history/oracle.mjs build\|checkout …` | each range's oracle (checkpoint, tree, expected history and outcomes), and one working tree moved to a range's checkpoint |
| `node tools/history/report.mjs --metrics <dir> --plan <dir> --docs <dir>` | the per-range reports and `RANGES.md` from the runner's metrics, with the gate decision; exits 3 on optimize and 4 on stop |
| `node tools/history/snapshot.mjs take\|verify\|fork\|orphans …` | #352's block-incremental Store snapshots: a manifest of 1 MiB block hashes over one shared block store; take writes only new blocks from a sealed Store; fork rebuilds one into a new project |
| `node tools/history/inspect.mjs ls\|cat\|diff\|stats\|trend\|open …` | a checkpoint's files, its difference from git by blob id, and a fork's Store figures (SDK stats, SQLite page use, objects by role) |
| `node tools/history/calibrate-tests.mjs --repo <dsh.git> --plan <dir> --image I --pnpm-store <volume>` | runs each range's candidate unit tests once natively, offline, and writes which it runs (`tests-NNN.json`) |
| `node tools/import-run.mjs <shop\|depot\|crew> <id> <store.json> key=value…` | generates `scenarios/<id>/` from a recorded run. `verification=on\|off\|no-browser` runs or disables the commands that only verify, or only those of a browser proof (Crew: no-browser); `cache=<dir>` (outside the repository) keeps what it read from the Store, so a re-import takes seconds |
| `node tools/check-scenario.mjs <demo>` | a converted scenario's static checks, in seconds: run files, expects, disabled commands, and the lead's starts, turns and waits |
| `LAYERFS_DEMO_FAST=1` (dsh web's environment) | the mock and `terminal.mjs` skip typing, thinking and terminal delays; nothing else changes |
| `node tools/check-session.mjs '<dsh web URL with ?token=…>' <session id>` | checks that a session's tab draws what LayerFS reports: Branches, heads, `main`, Sandboxes, which agent made each Commit and Layer, and bash-call groups |

`assets/` holds unlicensed character art and is never committed. `lib/` is build output.

<details>
<summary><b>🎬 Mock model and demos (disabled permanently)</b></summary>

> **Disabled permanently** (owner, 2026-10-21). The `dsh-layerfs-demo` bundle loads nothing (its `cordis.patch.yml` is empty), is not part of the release, and the install guide above does not add it. This section describes the code that stays in the repository.

The `dsh-layerfs-demo` bundle adds the LayerFS mock model: an LLM provider that is explicitly not a language model. Each scenario in `scenarios/` becomes one model, "LayerFS mock · <title>". It replies with scripted calls to the preset's own tools: bash, subagent, send_message, list_agents and the job tools. dsh runs those calls for real, against a real LayerFS project, and subagents inherit the mock model, so they run their own part of the script. No API key is needed.

| Demo | Agents | Shows |
|---|---|---|
| `tour` | root | the Tour story, 24 steps: fs Commits, a Sandbox terminal that installs, fails lint, builds and serves (unmount Busy), a stale Workspace that merges on commit, a second Workspace, a fork from a Commit, two publishes |
| `parallel` | root, header, footer | two subagents on their own Branches; the root publishes one, then asks the other to merge main in first |
| `conflicts` | root | the Conflicts story, 15 steps: a commit Conflict resolved in the Workspace, a merge Conflict resolved by making the Branches agree |
| `merge` | root | the Commit & merge story, 21 steps: a clean stale commit, a commit Conflict, merge into a new Branch, a merge Conflict, publish |
| `sandbox` | root | two Workspaces placed in one Sandbox, a background build job, and the Busy and Dirty refusals |
| `agents` | root and five | the 5 agents story, 52 steps (62 bash calls): a Workspace per call, stale merges, `.lww`, a same-line Conflict fixed in the kept Workspace, a dev server and curl, a try on a fork, merge, publish and blame |
| `shop` | root | Live only. #334's Builder 2 replayed, 19 steps: a small web shop built over six milestones, 12 recorded commands (npm, esbuild, node tests, a server smoke) run again |
| `depot` | root | Live only. #337's Builder 2 replayed, 103 steps: a Rust inventory service and its browser client, 82 recorded commands (cargo fetch and builds from crates.io, cargo and Python tests, Chromium) run again in Workspaces that stay mounted across steps |
| `crew` | root + 5 | Live only. #340's Builder 2 r3 replayed, 1,550 steps: a lead and 17 agents playing as 6 roles build one application on Branches of their own; 304 recorded commands, of which the 147 that only verify and belong to a browser proof are disabled (their run files say `"verification": "no-browser"`) and the other 157 run; the roles run at the same time and wait for each other only where they touched the same Branch |
| `checkpoints` | root, a01, a02 | #352's checkpoints, small: four ranges in which two agents write on lanes of their own and the lead merges, publishes and verifies; each range ends at a checkpoint once its (fake) gates pass. The strip shows the checkpoint timeline; Play or Replay first asks where to start: from the beginning or after a checkpoint the last run passed, whose ranges are then restored by the lib's `restore` (standing in for #352 P2's Store snapshots), not replayed |

#### Replayed runs

`shop`, `depot` and `crew` (see [#347](../core/docs/issues/347/SPEC.md)) replay a recorded application run on Live LayerFS. Each scenario sets `"live": true`, so the strip offers only Live and the Host refuses a Sim Init of its project; `image` names the runtime image its Sandboxes use, and `seed.json` is the folder the original Init imported, which Play writes out once under `<profile>/dsh-layerfs-demo/seeds/` and Inits from. Each bash step is `step(<NNN>)` of [`scenarios/shared/replay.mjs`](scenarios/shared/replay.mjs), which makes the recorded SDK calls of `run/NNN.json` in order: recorded writes are applied as they were, recorded commands run again in a Workspace of the same Branch (except, where the run file says `"verification": "off"`, those that only verify: their step says so and runs nothing), and each reply must be the recorded one (a command's exit code included), or the step exits 1 and the run stops. `final.json` holds each tracked file's SHA-256 on the recorded final `main` and each Branch's recorded history, for check-demo; its `varies` names what a replay cannot reproduce byte for byte (a clock reading in a name, SQLite databases, bytecode, server logs), decided from the record.

`tools/import-run.mjs` generates these folders from a run's records and a restored copy of its stopped Store (`tools/restore-store.mjs`). It keeps steps that write, run a command or change history and drops reads; pauses are `clamp(gap × k, 400, 3000)` ms with `k` set so they add up to the demo's target thinking time.

dsh's bash tool reports a command that exits non-zero as an ordinary result ending in `[exit code: N]`; the mock stops the run on that marker (and on a signal or timeout marker) as it does on an error result.

A scenario is `scenario.json` (format `layerfs-mock-scenario` version 2), with its `scripts/` or a shared `lib`. Each agent has a list of steps:
- a call step: one preset tool and its arguments. A bash step names a `script` (sent as `node --input-type=module <<'JS'`), a `run` (calls of the scenario's `lib`, imported and awaited in such a script) or a `command`. `expect` lists the LayerFS outcomes its trace should show, `retry` reruns it on matching output, and `as` names a background job;
- a reply step: the turn's closing text. `until` makes the agent wait for those subagents to report first;
- a pause step: milliseconds before the next step.

A root step may carry `checkpoint: { label, verdict?, note?, restore? }` (#352): when the root agent finishes it, the run has reached that checkpoint. `verdict` is pass, watch, optimize or stop; watch is a pass with a note, and optimize or stop halt the run there. `restore` is a `run` that brings the project to the checkpoint at once: a run can start after a passed checkpoint that has one, and its first message then ends " from after <label>."; the root runs `restore`, starts the subagents that had already started, each at its next turn, and plays on. The mock keeps each demo's reached checkpoints in `<profile>/dsh-layerfs-demo/checkpoints/<demo>.json`, so they outlive a dsh web restart (the run then reads as interrupted), and the strip draws them as a timeline: green when passed, red where the run stopped, striped ↺ when restored. With more than 12 the segments carry no labels, and the start picker lists them in a scrolling, filtered box.

`@name` in an argument is a subagent or job started earlier. A subagent's `description` is its name in the scenario. `order` places the demo in lists and `speed` sets how fast the mock runs it at first. Every scenario's `fixture` is `demo-workspace/`, the shared website; its install, build, test and serve are small shell scripts, not npm: test checks each page's heading and that every `@/…` import resolves, and serve answers each page's heading on localhost:3000 (with python3). The sandbox demo's scripts keep their notes between bash calls under `<temp>/dsh-layerfs/demo-notes/<project>/`, outside the workspace folder, because Init imports everything in it.

The tour, conflicts, merge and agents demos are the simulation engine's demo stories ([`sim/demo.js`](sim/demo.js)) step for step: each step is one bash call of [`scenarios/shared/terminal.mjs`](scenarios/shared/terminal.mjs), which makes the same SDK calls as that step's terminal line and prints what the terminal printed (`npm ci`, `npm run build`, `npm test` and `npm run start` become the shell scripts). An agent's call mounts a Workspace of its own in its Branch's shared Sandbox, runs, commits and unmounts; after a Conflict the Workspace is kept for the agent's next call, and a trailing `&` keeps it for the server. The lead wakes the agents of each step with send_message and waits for them; each agent's pause is the story's thinking time, and the call takes its typing and running time, so a burst's calls overlap and commit in the same order as in the story's schedule. The calls keep their notes (a terminal's Workspace, an agent's kept one, the C<n> labels) under the project's LayerFS home entry, which is removed with the project.

#### The demo workspace

With the bundle installed, dsh web gets a workspace for `dsh/demo-workspace` with one session per demo, in scenario order. The client registers the workspace and makes any missing session when it loads. Each session opens with "What does this demo show?", which the mock answers with the demo's blurb and agents; dsh's sidebar lists no session before its first turn.

The demo sessions belong to their demos, so the client hides dsh's controls that would add, fork, archive, pin, rename or move one: the workspace row's New session and menu, each session row's menu, Archive and Pin, rename by double-click, dragging, the chat's Branch button, and the fork, archive and rename shortcuts while a demo session is open. The workspace's folder row has a violet gradient and a DEMO badge. dsh has no per-workspace setting for these: a stylesheet keyed to the sidebar's row keys and the chat's session attribute hides them, and the client's key listener runs before dsh's.

Above the composer of a demo session, one strip shows:
- a progress bar: the steps every agent has finished out of all the scenario's steps, from the mock's own replies, with the root agent's phase and the elapsed time;
- Sim or Live: the simulator or real LayerFS. The choice is kept in the browser;
- Play, or Replay after a run.

Play (Replay after a run, also allowed while one goes) runs the demo in a new session that takes the old one's place; dsh cannot empty a session. In order:
1. A session is made in the demo workspace at the old one's place, with the preset, the demo's title and its mock model. The sidebar lists it only from its first message.
2. The old session is archived, with its turn, subagents and jobs stopped.
3. The demo's projects are removed, and its project for the mode (`demo-<id>`, or `demo-<id>-sim` on Sim) is Inited again from `demo-workspace/`, with the Init's progress on the bar. Removal stops the Sandboxes a stopped run left open (only Play asks for that; removing other projects still refuses while a Sandbox runs) and clears their records in the LayerFS home.
4. The new session becomes the demo's, the project is chosen, "Run the demo." is sent, and the new session opens.

Each step is attempted once. A failure before step 4 unarchives the old session, which stays the demo's, archives the unused new one, and shows the message on the strip. The strip reads the demo again as soon as Play ends. A run stops on the strip when a request of it is aborted, or, for a stop between requests, which the mock does not see, once dsh says the root session is not running and no step has finished for 4 seconds. When the client loads, a demo whose session is gone or archived gets a new one, opened with the intro question.

The Host side is the mock plugin's `/layerfs-demo` route. It serves the workspace folder, the demos, each demo's session (kept in `<profile>/dsh-layerfs-demo/sessions.json`) and each demo's latest run.

</details>
