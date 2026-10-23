# #428 Windows verification: agent prompt

> **Status:** Current planning checklist; no release candidate exists.

Paste everything below the line into a Claude Code session on the Windows
machine. It installs LayerFS and the dsh-layerfs plugin on Windows (with the
LayerFS worker in WSL2), then runs the plugin's scenario tests in dsh web on
the default profile at port 3080. On macOS (2026-10-24) these commits passed
every LayerFS Linux real test the plugin-neutral change touches, and in dsh
web on 3080 Run all had 8 of 10 scenarios passed and none failed, with Depot
and Crew, the two long ones, still running cleanly when this was written.

---

You are installing LayerFS and its dsh plugin on this Windows machine, then
running the plugin's scenario tests in dsh web. Two parts, in order:

1. **Install** everything below.
2. **Run the scenario tests** on the default dsh profile at
   `http://127.0.0.1:3080`, and report.

This is an install-and-test run, not a fix run. Do not change product code in
either repository. When something fails, find the cause from source and
bounded output, write it down with a proposed fix, and go on.

## Sources (pin these exactly)

| What | Repository | Ref |
| --- | --- | --- |
| LayerFS | github.com/Ephemeral-AI-Lab/layerfs | branch `claude/issue428-sdk-packages-cd9412`, commit `3c21c62c6edf800d3aabb8ecd83620a75ecf7084` |
| Plugin | github.com/Ephemeral-AI-Lab/dsh-layerfs | `main` at `56fc353` (its `layerfs.json` pins the LayerFS commit above) |
| dsh | deepseek-harness, checked out beside dsh-layerfs | a version inside `packages/dsh-layerfs/package.json` `engines.dsh` (`>=0.2.1-alpha.4 <0.3.0`) |

Check out the **same LayerFS commit** twice: in WSL, for the sealed bundle,
and on Windows, for the dsh Host and agents. The worker refuses a Windows SDK
that differs from the bundle's SDK, apart from line endings.

Facts that matter at these commits:
- LayerFS's JavaScript API is under `api/`: `api/agent/sdk` (`@layerfs/agent`),
  `api/agent/cli` (the `layerfs` command), `api/backend`, `api/runtime` and
  `api/binding` (the `layerfs-node` addon). Notes that name `sdk/js/...` or a
  root `cli/` are older than this layout.
- LayerFS is plugin-neutral. It reads `LAYERFS_*` settings only and has no
  adapter setting. dsh-layerfs maps its `DSH_LAYERFS_*` names in its own
  wrappers (`packages/host/src/agent/`). Do not add `adapter` or
  `WORKER_ADAPTER` anywhere.
- The bundle must contain `api/runtime/src/worker/server.js`. A bundle from an
  older commit will not start, or its worker cannot talk to this Host (the
  wire tag changed).

## Rules

- Read `AGENTS.md` in both repositories first; they bind this run.
- Every test command gets an explicit wall timeout of at most 120 s and is
  stopped when it expires. Keep every other command under 2 minutes too: put
  `timeout 110` before long steps and run them again until they finish (Cargo
  keeps what it compiled).
- One attempt per check. No retry loops, and no reruns to turn a failure into
  a pass.
- Disposable Stores only. Durable is `NOT_RUN — disabled by owner until
  explicit reauthorization`.
- Never put a Store or the Linux LayerFS home on `/mnt/c` or any
  Windows-backed mount.
- Keep at least 20 GiB free inside the WSL Docker Engine; check before builds.
- The dsh profile to use is the **default** one: no `DSH_HOME` override, its
  `web` profile, and dsh web on port 3080. Before changing it, copy
  `cordis.patch.yml` and `package.json` to a dated backup folder. Keep its
  existing patch entries. Do not restart a dsh web that has a run in
  progress.
- Do not commit, push, delete other people's volumes or containers, or
  install system-wide software without asking the owner first.

## Part 1: install

### In WSL2

In the selected distribution (default `Ubuntu`; record which):

1. Confirm WSL2, `/dev/fuse`, and a running Docker Engine inside the
   distribution (not Docker Desktop's Windows pipe). Also confirm Node 22.19
   or newer at an absolute path (default `/usr/local/bin/node`) and
   `python3`. Record each version.
2. Clone LayerFS at the pinned commit on the distribution's own filesystem.
3. Build the runtime image from dsh-layerfs `tools/runtime/Dockerfile` in
   that Docker Engine. Record its exact `sha256:` image ID.
4. Build the sealed Linux bundle with LayerFS's `tools/worktree.py`
   (`register`, `prepare`, `build --platform linux --profile release`; see
   `docs/general/parallel-worktrees.md`). Record the printed
   `build-linux-<hash>` folder and its `manifest.json` hash. Confirm the
   folder has `api/runtime/src/worker/server.js`, `layerfs-daemon` and
   `layerfs.node`.

### On Windows

5. Node (record the version) and pnpm. Clone LayerFS at the same commit on a
   Windows drive. Then run `npm install` in `api` and
   `npm --prefix api/agent/cli install`. Each only links packages, and
   dsh-layerfs's install stops if either is missing. Record `core.autocrlf`.
6. Clone dsh-layerfs at `56fc353` beside a deepseek-harness checkout. Set
   `LAYERFS_SOURCE` to the Windows LayerFS checkout, then run `pnpm install`
   and `pnpm build`. `pnpm run check:layerfs` must report the checkout "at
   the pinned commit".
7. Run `pnpm run test:windows` and `pnpm test`, each bounded at 120 s.
   Record the failures and whether each is Windows-specific.
8. Add the plugin to the default `web` profile, as dsh-layerfs `README.md`
   steps 3 and 4 describe, but without the `DSH_HOME` override. Its **On
   Windows** paragraph has the details. Add a `layerfs-host` entry to the
   profile's `cordis.patch.yml` with:
   - `image`: your sha256;
   - `bundle`: the WSL path of the sealed bundle;
   - `wslDistribution`.

   Keep `containerPrefix` at its default unless something else already uses
   `layerfs-` containers in that Docker Engine; if so, record what you chose.
9. Start dsh web on the default profile:
   `pnpm --dir <deepseek-harness> dsh web --no-open --port 3080`.
   `GET /layerfs/status` must report an empty `missing` list. Open the
   printed link.
10. On the LayerFS page, open **All projects**. Docker and "Store calls ·
    worker ready" must be green, with host and Docker free space shown. If
    the worker is not ready, use its Start button once and record what
    happened.

## Part 2: run the scenario tests on 3080

11. Open the LayerFS page's **Tests** tab. It runs the LayerFS scripted model,
    which ships switched on. Press **Run all** once. It queues every scenario
    and runs up to 2 at once. Five drive the SDK (`sdk`) and five the
    `layerfs` command (`cli`). Most take under a minute; Depot and Crew take
    several minutes each, and the whole run takes roughly 15–20 minutes.
    Don't press Run again while it's going.
12. When every row has finished, record each test's result and time. For a
    failure, record the reason the row shows. Then check its session:
    `node tools/checks/run.mjs <profile dir> <root session id>` in
    dsh-layerfs, bounded at 120 s. Leave the failed project as it is: the row
    keeps it.

### Known risks to check, not to assume

- **The `layerfs` command on Windows.** The Host puts its `layerfs` shim on
  agents' PATH everywhere except Windows: `packages/host/src/env.js`
  `shimOnPath` returns early on `win32`. The `cli` scenarios type `layerfs`.
  `node "$DSH_LAYERFS_CLI"` works without the shim. Find which shell dsh's
  bash tool uses on Windows and what agents see on PATH, and record whether
  the `cli` scenarios fail for this reason.
- **Scenario commands are bash** (`jq`, `$(...)`, heredocs). Record whether
  they run in that shell.
- **Line endings.** A Windows checkout with `core.autocrlf` must still match
  the sealed bundle's SDK. If the worker refuses the SDK, record the setting
  and the message.

## Report

Write `docs/windows-live-verification-<yyyymmdd>.md` in dsh-layerfs, in the
shape of `docs/windows-live-verification-20261022.md`. Include:

- identity:
  - both commits and the dsh version;
  - Windows and WSL Node;
  - the WSL distribution and kernel;
  - the Docker Engine;
  - the image ID;
  - the bundle folder and its manifest hash;
- every install step and check above with its exact command, timeout and
  outcome (PASS, FAIL, or NOT_RUN with the reason);
- the Tests tab table: each scenario, its tag, its result, its time and, for
  a failure, the reason;
- each failure's cause and proposed fix;
- what remains unverified.

Then tell the owner what passed, what failed and why. Ask before committing
the record, with the production LOC comparison the commit needs.
