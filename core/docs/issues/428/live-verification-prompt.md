# #428 live verification: agent prompt for macOS, Linux and Windows

> **Status:** Current verification checklist for the LayerFS 1.1.0 candidate.
> It replaces the Windows-only prompt (`windows-verification-prompt.md`, at
> `fb83195e9e`).

Paste everything below the line into a Claude Code session on the machine to
verify, after filling in the platform. It installs LayerFS and the dsh-layerfs
plugin there, then runs the plugin's scenario tests in dsh web on the default
profile at port 3080, and writes a record. Runs so far:

| Platform | Date | LayerFS | dsh-layerfs | Result | Record |
| --- | --- | --- | --- | --- | --- |
| Windows (WSL2 worker) | 2026-10-24 | `3c21c62c6e` | `02b9dc9` | 10 of 10 scenarios passed | dsh-layerfs `docs/windows-live-20261024/` |
| macOS (Docker Desktop worker) | 2026-10-24 | `3c21c62c6e` | `02b9dc9` | 9 of 10 passed in Run all; Depot stopped at its first command because the Sandbox image had no `cargo`, then passed on one rerun after the image was rebuilt (with HTTPS Debian sources) | dsh-layerfs `docs/macos-live-verification-20261024.md` |
| Linux (in process) | — | — | — | NOT_RUN | — |

---

You are installing LayerFS and its dsh plugin on this machine, then running
the plugin's scenario tests in dsh web. This machine is **<macOS | Linux |
Windows>**; follow the steps for it and skip the others. Two parts, in order:

1. **Install** everything below.
2. **Run the scenario tests** on the default dsh profile at
   `http://127.0.0.1:3080`, and report.

This is an install-and-test run, not a fix run. Do not change product code in
either repository. When something fails, find the cause from source and
bounded output, write it down with a proposed fix, and go on.

## Sources (pin these exactly)

| What | Repository | Ref |
| --- | --- | --- |
| LayerFS | github.com/Ephemeral-AI-Lab/layerfs | `commit` in the plugin's `layerfs.json` (`3c21c62c6e` for the runs above) |
| Plugin | github.com/Ephemeral-AI-Lab/dsh-layerfs | the commit to verify (`02b9dc9` for the runs above); its `layerfs.json` pins the LayerFS commit |
| dsh | deepseek-harness, checked out beside dsh-layerfs | a version inside `packages/dsh-layerfs/package.json` `engines.dsh` |

Where the LayerFS checkout goes:

| Platform | Checkouts |
| --- | --- |
| macOS | One, on the Mac. It feeds both the sealed bundle (built in Docker) and the dsh Host. |
| Linux | One. It feeds the Linux build and the dsh Host. |
| Windows | Two of the **same commit**: one in WSL, for the sealed bundle, and one on Windows, for the dsh Host and agents. The worker refuses a Windows SDK that differs from the bundle's, apart from line endings. |

Facts that matter at these commits:
- LayerFS's JavaScript API is under `api/`: `api/agent/sdk` (`@layerfs/agent`),
  `api/agent/cli` (the `layerfs` command), `api/backend`, `api/runtime` and
  `api/binding` (the `layerfs-node` addon). `api/README.md` maps them. Notes
  that name `sdk/js/...` or a root `cli/` are older than this layout.
- LayerFS is plugin-neutral. It reads `LAYERFS_*` settings only and has no
  adapter setting. dsh-layerfs maps its `DSH_LAYERFS_*` names in its own
  wrappers (`packages/host/src/agent/`). Do not add `adapter` or
  `WORKER_ADAPTER` anywhere.
- Where Store calls run: on **Linux**, in the Host's and agents' own processes;
  on **macOS**, in one long-lived Linux container per LayerFS home (the worker,
  `<containerPrefix>worker`); on **Windows**, in a Node worker in a WSL2
  distribution. On macOS and Windows the worker runs a **sealed bundle**: it
  must contain `api/runtime/src/worker/server.js`, and the worker checks every
  file against the bundle's `manifest.json` before it listens. A bundle from
  an older commit will not start, or cannot talk to this Host.

## Rules

- Read `AGENTS.md` in both repositories first; they bind this run.
- Every test command gets an explicit wall timeout of at most 120 s and is
  stopped when it expires. Keep every other command under 2 minutes too: put
  `timeout 110` before long steps (macOS has no `timeout`: use
  `perl -e 'alarm 110; exec @ARGV' <command>`, or put `timeout 110` inside the
  container) and run them again until they finish (Cargo keeps what it
  compiled).
- One attempt per check. No retry loops, and no reruns to turn a failure into
  a pass.
- Disposable Stores only. Durable is `NOT_RUN — disabled by owner until
  explicit reauthorization`.
- Stores live on a local Linux filesystem (ext4, xfs or btrfs). On Windows
  never put a Store or the Linux LayerFS home on `/mnt/c` or any
  Windows-backed mount.
- Keep at least 20 GiB free inside the Docker Engine the worker or Sandboxes
  use; check before builds.
- The dsh profile to use is the **default** one: no `DSH_HOME` override, its
  `web` profile, and dsh web on port 3080. Before changing it, copy
  `cordis.patch.yml` and `package.json` to a dated backup folder. Keep its
  existing patch entries. Do not restart a dsh web that has a run in
  progress: a restart ends in-flight agent turns.
- Do not commit, push, delete other people's volumes or containers, or
  install system-wide software without asking the owner first.

## Part 1: install

1. **Prerequisites.** Record each version.
   - All: Node 22.19 or newer, pnpm, `python3`, network access, Docker.
   - macOS: Docker Desktop running.
   - Linux: a Docker Engine, `/dev/fuse`, Cargo 1.85.1 if you build the addon
     on the host, and a Store filesystem that is ext4, xfs or btrfs.
   - Windows: WSL2 with a selected distribution (default `Ubuntu`; record
     which) that has `/dev/fuse`, its own running Docker Engine (not Docker
     Desktop's Windows pipe), and Node 22.19 or newer at an absolute path
     (default `/usr/local/bin/node`).
2. **LayerFS checkout(s)** at the pinned commit, as the table above says. On
   the checkout the dsh Host uses (the Windows one on Windows), run
   `npm install` in `api` and `npm --prefix api/agent/cli install`. Each only
   links packages, and dsh-layerfs's install stops if either is missing. On
   Windows record `core.autocrlf`.
3. **Runtime image.** Build it from dsh-layerfs `tools/runtime/Dockerfile` in
   the Docker Engine the worker and Sandboxes use (on Windows, the WSL one).
   Record its exact `sha256:` image ID.
4. **LayerFS build.**
   - macOS and Windows: build the sealed Linux bundle with LayerFS's
     `tools/worktree.py` (`register`, `prepare`, `build --platform linux
     --profile release`; see `docs/general/parallel-worktrees.md`), on Windows
     inside WSL. Record the printed `build-linux-<hash>` folder and its
     `manifest.json` hash. Confirm the folder has
     `api/runtime/src/worker/server.js`, `layerfs-daemon` and `layerfs.node`.
     A bundle already built from exactly this commit may be reused; record
     how you established that.
   - Linux: build `layerfs-daemon` and the addon (dsh-layerfs `README.md`
     step 1's Docker recipe, or `npm --prefix api run build:native` on the
     host). Record the daemon path and the `layerfs.node` that dsh web will
     load (`LAYERFS_NATIVE`, or `api/binding/layerfs.node`).
5. **Plugin.** Clone dsh-layerfs at the pinned commit beside a
   deepseek-harness checkout. Set `LAYERFS_SOURCE` to the Host's LayerFS
   checkout, then run `pnpm install` and `pnpm build` (with `DSH_SOURCE` set
   to the deepseek-harness checkout). `pnpm run check:layerfs` must report
   the checkout "at the pinned commit". If an earlier install linked
   `.layerfs` elsewhere, `pnpm install` keeps that link: run
   `pnpm run link:layerfs` (with `LAYERFS_SOURCE` set) and check again.
6. **Plugin tests**, each bounded at 120 s: `pnpm test` and
   `pnpm run test:scenarios`; on Windows also `pnpm run test:windows`. Record
   the failures and whether each is platform-specific.
7. **Profile.** Add the plugin to the default `web` profile, as dsh-layerfs
   `README.md` steps 3 and 4 describe, but without the `DSH_HOME` override.
   Its `layerfs-host` entry in the profile's `cordis.patch.yml`:

   | Setting | macOS | Linux | Windows |
   | --- | --- | --- | --- |
   | `image` | your sha256 | your sha256 | your sha256 (WSL Engine) |
   | `bundle` | the sealed bundle | not used | its WSL path |
   | `daemon` | defaults to `<bundle>/layerfs-daemon` | required | defaults to `<bundle>/layerfs-daemon` |
   | `wslDistribution` | — | — | required |
   | `volumes` | — | Docker's volumes directory; dsh web's user must write it | — |

   Keep `containerPrefix` at its default unless something else already uses
   `layerfs-` containers in that Docker Engine; if so, record what you chose.
   On Linux, run dsh web as a user that can write Docker's volumes directory
   (such as root) with a `home` that user owns, or Init fails `Unsupported`.
   On macOS, a worker of another build is replaced when the Host starts:
   change `bundle` only while no dsh run is live.
8. **Start dsh web** on the default profile, with the Node recorded in step 1
   (a login shell may pick an older one):
   `pnpm --dir <deepseek-harness> dsh web --no-open --port 3080`.
   `GET /layerfs/status` must report an empty `missing` list. Open the
   printed link.
9. On the LayerFS page, open **All projects**. Docker must be green, with host
   and Docker free space shown. On macOS and Windows "Store calls · worker
   ready" must be green; if the worker is not ready, use its Start button once
   and record what happened.

## Part 2: run the scenario tests on 3080

10. Open the LayerFS page's **Tests** tab. It runs the LayerFS scripted model,
    which ships switched on. Press **Run all** once. It queues every scenario
    and runs up to 2 at once. Five drive the SDK (`sdk`) and five the
    `layerfs` command (`cli`). Most take under a minute; Depot and Crew take
    several minutes each, and the whole run takes roughly 15–20 minutes.
    Don't press Run again while it's going.
11. When every row has finished, record each test's result and time. For a
    failure, record the reason the row shows. Then check its session:
    `node tools/checks/run.mjs <profile dir> <root session id>` in
    dsh-layerfs, bounded at 120 s. Leave the failed project as it is: the row
    keeps it.

### Known risks to check, not to assume

All platforms:
- **A stale runtime image.** Depot runs `cargo`; an image built from an older
  dsh-layerfs Dockerfile has Node only. Before Run all, run
  `docker run --rm <image> cargo --version` and record it. A Depot failure
  with exit 127 at its first command is this.
- **The worker and the image.** A worker is replaced when its bundle's build
  changes, not when `image` changes. After changing `image`, check which image
  the running worker and new Sandboxes use.

macOS:
- **The runtime image build behind a proxy.** Docker Desktop's proxy can cut
  plain-http Debian downloads short (`apt-get update` exits 100), and shows
  network failures as HTTP 500. Record the error. Do not edit the Dockerfile:
  ask the owner before building a changed image.
- **No `timeout`.** Use `perl -e 'alarm 110; exec @ARGV' …`.

Windows:
- **The `layerfs` command.** The Host puts its `layerfs` shim on agents' PATH
  everywhere except Windows: `packages/host/src/env.js` `shimOnPath` returns
  early on `win32`. The `cli` scenarios type `layerfs`; `node
  "$DSH_LAYERFS_CLI"` works without the shim. Find which shell dsh's bash tool
  uses and what agents see on PATH, and record whether the `cli` scenarios
  fail for this reason.
- **Scenario commands are bash** (`jq`, `$(...)`, heredocs). Record whether
  they run in that shell.
- **Line endings.** A Windows checkout with `core.autocrlf` must still match
  the sealed bundle's SDK. If the worker refuses the SDK, record the setting
  and the message.

Linux (no run yet):
- **Volumes directory.** Init writes Docker's volumes directory directly.
  Run as a user who cannot write it and Init fails `Unsupported`; record the
  user, the `volumes` setting and the `home` owner.
- **Store filesystem.** Store calls run in dsh web's own process, so its
  `home` must be on ext4, xfs or btrfs; record `stat -f -c %T` of it.
- **One build.** The addon dsh web loads and the daemon must come from the
  same commit; no worker checks them for you. Record both paths and their
  build commit.

## Report

Write `docs/<platform>-live-verification-<yyyymmdd>.md` in dsh-layerfs, in the
shape of the earlier records (`docs/windows-live-verification-20261022.md`,
`docs/windows-live-20261024/`). Include:

- identity:
  - both commits and the dsh version;
  - Node (on Windows, both Windows and WSL);
  - the OS version (on Windows, the WSL distribution and kernel);
  - the Docker Engine;
  - the image ID;
  - the bundle folder and its manifest hash (macOS, Windows), or the daemon
    and addon paths (Linux);
- every install step and check above with its exact command, timeout and
  outcome (PASS, FAIL, or NOT_RUN with the reason);
- the Tests tab table: each scenario, its tag, its result, its time and, for
  a failure, the reason;
- each failure's cause and proposed fix;
- what remains unverified.

Then tell the owner what passed, what failed and why. Ask before committing
the record, with the production LOC comparison the commit needs.
