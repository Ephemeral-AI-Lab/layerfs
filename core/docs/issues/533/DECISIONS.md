# #533 decisions log

> **Status:** Record of the gate and open-decision answers taken for the
> #533 implementation on branch `issue533-v2`.

The owner delegated every remaining gate and open decision on 2026-11-02
("do not ask question, because you got the authority to answer"). Each row
below is the **recommended answer** from the spec's §3 or H §13, taken on
2026-11-02, **delegated by owner, recommended answer**, unless its row says
the recommendation was replaced, and why.

Spec: [README.md](README.md). H: [host-platform-connections.md](../../proposals/host-platform-connections.md).
S: [layerfs-sdk-cli.md](../../proposals/layerfs-sdk-cli.md).

## Gates

| Gate | Answer taken (2026-11-02, delegated by owner, recommended answer) |
| --- | --- |
| G0 | Base is `ddc7c5696b` (head of PR #530, containing #528 and `layerfs_v2_0_0`). No PR is merged on GitHub; branching from that head satisfies it. **Update (owner, 2026-11-02, via the parent session):** the base is local `layerfs_v2_0_0` at `ddc7c5696b`, fast-forwarded locally (not pushed) to this commit; `issue533-v2` is stacked directly on it. If local `layerfs_v2_0_0` moves, its new tip comes into `issue533-v2` by a merge, never a rebase. The recommendation (merge first) was not followed; see [Replaced recommendations](#replaced-recommendations). |
| G1 | H and S are accepted as the 2.0 direction. |
| G2 | The H §9.1 AGENTS.md wording is approved and applied in step 1. |
| G3 | #531 is held; this issue supersedes it. Nothing here builds on #531. |
| G4 | #521 is not merged on GitHub. Its branch (`origin/area-p/complete-p1-p15`) is read; folder scope is ported into the library (step 14) and its PyO3 binding is used as a reference (step 16). MCP follows O32. The recommendation (merge first) was not followed; see [Replaced recommendations](#replaced-recommendations). |
| G5 | O21a: `layerfs-api` grows into the library (no `layerfs-sdk` crate). O21b: the crates stay in `engine/crates/` for 2.0. |
| G6 | PyO3 is accepted as a new dependency, and maturin for wheels. |
| G7 | O22: one `on_progress` stream; no job registry. |
| G8 | O23: one backup link per Store per bucket; replaces `send` and `store.unlink`. |
| G9 | O24: `project.remove` added as a 55th call. O25: the 14 `@layerfs/backend` operator and diagnostic calls are dropped from the 54; what Connect needs stays behind the trace files. O26: trace files are the feed; no subscribe call. |
| G10 | O11 widened: `machine update` is added (a call beyond the 54), also for `native` and `docker` (image, Docker socket, WSL distribution). Resizing a VM is deferred; see [Replaced recommendations](#replaced-recommendations). |
| G11 | O16: the sealed Linux bundle and the built-in machines' binaries come from a per-build user cache, outside every home, pinned by the package version. |
| G12 | O17: `machine create --nested` adds Firecracker's files to the Lima host. |
| G13 | O3: PostgreSQL Sandboxes on `native` now (plus `docker` as today); VMs and ssh later. |
| G14 | O4 widened: old settings and old Connect and dsh homes are removed in 2.0, with `layerfs import-env` run once. |
| G15 | O13 `fs` renamed `file`; O14 `workspace.status` added back; O15 package name `layerfs` (npm fallback `@layerfs/layerfs`); O19 synchronous `import layerfs` plus `layerfs.aio`; O20 prebuilt Linux x64/arm64 glibc, macOS arm64, Windows front end; O29 `fs.diff` positional. See the O rows below. |
| G16 | O27: the Linux `native` backend is removed; `native` on Linux uses namespaces. |

## Open decisions

| # | Answer taken (2026-11-02, delegated by owner, recommended answer) |
| --- | --- |
| O1 | Secret references are `env:LAYERFS_…` and `file:…` only; no keychain. |
| O2 | Machine idle time is 10 minutes. |
| O3 | PostgreSQL Sandboxes: `native` (a host build with the `postgresql` feature) and `docker`; VMs and ssh later. |
| O4 | Old settings are removed in 2.0; `layerfs import-env` writes records from the current environment once. |
| O5 | Windows: the Windows home holds the records; `native` on Windows holds only records, its sqlite Stores and Init live in the WSL worker; Sandboxes run on `docker`. Not run (no Windows host). |
| O6 | `store connect` opens a database whose single LayerFS schema has another name, and refuses one with several. |
| O7 | `native` and `docker` Stores together on macOS are allowed, each Store on one of them (the D5 macOS native Store exception). |
| O8 | Later; not built: a Linux `docker` Sandbox on a `native` Store stays `Unsupported`. |
| O9 | The old Store calls are removed in 2.0 (no wrappers). |
| O10 | The built-in machine is named `native`. |
| O11 | `machine update <name>` with `cpus`, `memory`, and for `native`/`docker` the image, Docker socket and WSL distribution; refused `InUse` while Sandboxes run on it. |
| O12 | A move between two non-native machines goes through `native` in one call. |
| O13 | The family is named `file` (`file.read`, `layerfs file read`), not `fs`. |
| O14 | `workspace.status` is added back (a call beyond the 54). |
| O15 | Package name `layerfs` on npm, PyPI and crates.io; `@layerfs/layerfs` is the npm fallback. Not checked against the registries (no publish here). |
| O16 | The sealed Linux bundle comes from a per-build user cache outside every home, checked by hash and pinned by the `layerfs` version. |
| O17 | `machine create --nested` puts Firecracker's files in the Lima host; the guest kernel and root filesystem are added to the bundle. |
| O18 | ssh Store roots may be ext4, xfs or btrfs. |
| O19 | Python: `import layerfs` is synchronous; `layerfs.aio` has the same calls as coroutines. |
| O20 | Prebuilt targets: Linux x64 and arm64 (glibc), macOS arm64, and the Windows front end. |
| O21a | `layerfs-api` grows into the library. |
| O21b | The crates stay in `engine/crates/` for 2.0. |
| O22 | One progress stream: Node `{ on_progress }`, Python `on_progress=`, shell `--progress` (JSON lines on stderr). |
| O23 | One link per Store per bucket; `backup create/restore` take an optional `file`. |
| O24 | `project.remove(project)` is added (a 55th call). |
| O25 | The 14 `@layerfs/backend` calls are dropped from 2.0's surface. |
| O26 | Trace files under `trace/` are the feed; no subscribe call. |
| O27 | The Linux `native` pairing and `backend/native/linux/` are removed. |
| O28 | An unset `LAYERFS_HOME` means `~/.layerfs`. |
| O29 | `fs.diff` (now `file.diff`) takes positional sides: `(project, left_branch, left_path, right_branch, right_path, { left_commit, right_commit })`. |
| O30 | Commits stay ids. |
| O31 | `sandbox.close(name, { force })` stops the Sandbox through its backend and records `Unknown` for Workspaces it could not drain. |
| O32 | Folder scope is ported into the library as a client option of every front end; MCP is dropped from 2.0 (not a fourth front end here). Since `797ecca2c7` (review P1-19) the scope is `{ project, folder, branch? }`, bound to one project as #521's client was, and allows only the file, Branch (list, get, fork, merge, publish) and Workspace calls; the shell form is `--scope <folder> --scope-project <store>/<project> [--scope-branch <name>]`, with `--scope-project` required. |
| O33 | The `docker` machine names containers with the home id from `layerfs.json`. |
| O34 | `machine.status` reports state, free disk, the provider tool's version and why a machine is down; no change event. |
| O35 | A lease `store/<name>/open/<pid>` whose process is not alive on this host is stale, removed under the home lock, and the removal is logged. |

### O36: default Sandbox image (owner decision, 2026-11-02)

The owner's decision, relayed by the parent session: "Keep runtime image".
2.0's default Sandbox image is the current runtime image,
`layerfs-352-runtime:node24-https-apt-20261024`, config digest
`sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e`,
the image every step-19 cell ran in.

- Contents, checked at this decision: Debian 12 (bookworm), arm64; Node
  v24.21.0; Rust and Cargo 1.85.1; the SQLite headers
  (`/usr/include/sqlite3.h`); Chromium 156.0.8078.47 and Selenium 4.8.3
  (Python). It also has `/usr/sbin/poweroff`.
- Size: Docker reports 954.7 MB for the image; the sealed
  `sandbox-image.tar` (a docker-archive with uncompressed layers, #527) is
  2.75 GB.
- `tools/linux_bundle.py seal` saves it into the bundle's
  `sandbox-image.tar` (its `--image` default is the launcher registration's
  `sandbox` image, which is this digest); the sealed bundle's `bundle.json`
  records the archive's config digest as `image` (`1d91885d…`) and this
  image as `saved_from` (`afa1ef9c…`).
- Another image stays possible as before: `machine create --image` for a
  VM machine, `machine update native|docker --image`, and a docker
  Sandbox's own image digest.
- Not done for a release (HANDOFF, open items): publishing the image under
  a registry name pinned by digest, so the O16 fetch can get it.

## Consequences of the answers for the call count

With O13, O14, O24 and O11 accepted, the surface is S's 54 calls with `fs`
renamed `file`, plus `project.remove`, `workspace.status` and
`machine.update`: 57 calls in nine families.

## Replaced recommendations

Rows added here when a recommendation proved wrong against the code, or
the work took another route than the spec's. Each names what was taken
instead and why.

| # | Recommended | Taken instead | Why |
| --- | --- | --- | --- |
| H §7.7 route table | sqlite on `docker` runs in process on Linux | Every sqlite Store not on `native` goes to its machine's worker, on every host | On Linux a `docker` Store lives in a Docker volume whose mountpoint belongs to root; opening it in the caller's process would need the volume path and root access. One route per machine kind is the smaller safe choice; the worker already opens the Store on the same kernel. (2026-11-02, delegated by owner, step 7) |
| O27 | Remove the Linux `native` pairing and `backend/native/linux/` | Removed the pairing (`NativeLinuxProvider`, `Runtime::LinuxNative`; a Linux `native` Sandbox request is refused `Unsupported`, naming namespaces); kept `backend/native/linux/` | The namespaces backend runs on native's shared lifecycle and uses `boot`, `stat` and `mounts::filesystem` from that folder, and the host still reopens, stops and deletes `native` Sandbox records a Linux host wrote before 2.0, which uses the rest. (2026-11-02, delegated by owner) |
| G0 | #528 and #530 merge into `layerfs_v2_0_0` first; this work branches from there | Branched from the unmerged head of PR #530, `ddc7c5696b`; nothing merged on GitHub. Later (owner, 2026-11-02) local `layerfs_v2_0_0` was fast-forwarded to `ddc7c5696b`, not pushed | Merging a PR on GitHub was outside the delegation (no push, no merge, no comment). `ddc7c5696b` contains #528, and `origin/layerfs_v2_0_0` (`81f8758bcf`) is its ancestor, so the merge G0 asks for would bring no source this base lacks; the PRs themselves stay open. Recorded after review P2-31. |
| G4 | Merge #521 first; port its Python binding and folder scope into the library | #521 not merged: its branch `origin/area-p/complete-p1-p15` was read; the folder scope was ported (step 14, bound to its project as #521's client was since `797ecca2c7`) and its PyO3 binding was a reference for step 16 | The same delegation limit as G0. The parts G4 uses were ported; its MCP server is dropped by O32, so no #521 code is needed as merged code. #521 stays open. Recorded after review P2-31. |
| Step 5.2 | `machine/` holds the Machine trait, one adapter per kind | No trait: `machine/kind.rs` holds the kind table and `setup.rs`'s `create`, `start` and the other calls match once on the kind (`9e52bb9306`) | Every caller already knows the kind from the machine record, so nothing needs dynamic dispatch; one `match` per call keeps each kind's steps in its own module without a trait and seven impl blocks. Recorded after review P2-31. |
| Workflow | One branch per step, `issue533-<step>` | One branch, `issue533-v2`, with one or more counted commits per step; subagent worktrees (`issue533-v2-step5`, `-o27`, `-pg`, and the review fix branches `-store`, `-home`, `-backups`, `-calls`) were cherry-picked onto it | Each step builds on the one before (call table, home layout, routes), so per-step branches would have been one chain anyway; one branch keeps the stacking on `layerfs_v2_0_0` (G0) in one place. Every commit message names its step or finding, and cherry-picks name their source. Recorded after review P2-31. |
| G10 / O11 resize | `machine update` with new `cpus` and `memory`, restarting the VM where the provider cannot resize live | **Deferred.** `machine update` sets the built-in machines' `image`, `docker` and `wsl`; `cpus` or `memory` on a VM is refused `InUse` while Sandboxes run, else `Unsupported` ("resizing a <kind> machine is not built yet; nothing was changed"). `cpus`/`memory` on `native` or `docker` is `InvalidInput` | Not built in this pass. Building it needs, per VM kind, the provider's own way to change a stopped VM's CPUs and memory (none is wired in `layerfs-sandbox` `machine/`); a stop of the machine with no Sandbox, open Store or call in flight, under its `start.lock`; the new size written to the record before the next on-demand start; the outcome `Unknown` and logged when the provider's change is uncertain; and a live test per kind. ssh hosts have no size. Recorded after review P2-31. |

## Implementation choices (not owner decisions)

Recorded here so a reviewer sees them with the decisions. 2026-11-02, delegated by owner.

- `commit.get` pages the Branch's Commits 1000 at a time until it finds the id (or an id prefix of 8+ characters). The Store has no by-id Commit lookup on the public method table; adding one is a later change. **Since `88b033cfa9`** (review P1-17) it follows `nextPagePos` to the end, since a page holds only what fits the reply budget; see Review fixes.
- Step 7 was committed before step 6: the route reads `worker.json` directly until step 6 lets it start a stopped machine. The order changes no behaviour of either step.
- A Sandbox's 2.0 record (name, project, machine, id, platform) is
  `machine/<machine>/sandbox/<name>/record.json`; the host Sandbox code's
  own state (daemon keys, mounts, processes) stays where 1.1.0 kept it,
  under the project's folder, keyed by the Sandbox's number. Moving that
  state is a later refactor; the name is the only handle callers see.
- A `native` Sandbox's daemon executable is the native machine record's
  `daemon`, else the internal `LAYERFS_DAEMON`, else
  `~/.cache/layerfs/bundle/<version>/<os>-<arch>/layerfs-daemon`.
- Sandboxes on a VM or ssh machine are sent to its worker (`sandbox.host`),
  which writes the Store's and project's records under its root and runs
  the same host Sandbox code on its own `native` (namespaces in the Linux
  guest). Sandboxes on the `docker` machine and Sandboxes of PostgreSQL
  Stores were refused `Unsupported` in step 12 (the 1.1.0 Docker path needs
  the image and volume settings the 1.1.0 JS runtime supplied). **Closed
  2026-11-02 by owner instruction** (relayed by the parent session: build
  both before step 19); see the last section.
- Step 14: the four 1.1.0 packages (`@layerfs/agent`, `@layerfs/cli`,
  `@layerfs/backend`, `@layerfs/runtime`) are deleted, not kept beside
  `layerfs`; their last source is the 1.1.0 tag. The Node binding is one
  `call(name, argsJson, home?, scopeJson?, onEvent?)` plus `table()`; the
  package builds its families from the table, so it names no call.
  `api/scripts/recover-records.mjs` and `layerfs-node`'s
  `examples/restore_record.rs` (1.1.0 `project.record` recovery) go with
  them: step 4's migration reads 1.1.0 records instead.
- Step 14 deletes `layerfs-node`'s four Linux test files (21 tests). They
  drove the removed `call`/`backendCall` entry points over 1.1.0's Docker
  discovery, admission and connection wait. The behaviours they guarded
  (#463 discovery reclaim, #508 admission and connection wait) are not
  re-covered on the 2.0 entry point here: there is no Linux host in this
  pass. Recorded as a coverage gap in HANDOFF.md.
- Step 15: the command is `engine/crates/layerfs-cli` (binary `layerfs`),
  built from the table like the package. The shell form of a folder scope
  is `--scope <folder>` with `--scope-branch <name>` (S names none);
  since `797ecca2c7`, also `--scope-project <store>/<project>`, required
  with `--scope` (see O32). `--text`
  is one generic readable form (a file's text as is; a result's list of
  records as a table; other fields as `key  value` lines), not the per-call
  layouts S sketches. `workspace exec --stream` writes the output the
  library sends after the command ends (the library has no live output
  stream yet), then exits with the command's code. Content from stdin is
  text when it is UTF-8, else bytes. `layerfs worker` serves
  `/layerfs/data`; the internal `LAYERFS_WORKER_ROOT` names another root
  for tests. The worktree build bundle carries `layerfs` beside the daemon;
  a sealed machine bundle (O16, `bundle.json`) is still placed by hand.
- Step 16: the Python package lives in `api/python` (beside the Node
  package); its PyO3 module is the engine crate `layerfs-python`
  (`layerfs._native`, abi3 for Python 3.10+). PyO3 0.30.1 and its build
  helper were fetched into this worktree's private Cargo home (G6); no other
  dependency changed. `layerfs.aio` runs each synchronous call on a thread
  (`asyncio.to_thread`); the native call releases the interpreter. (Since
  `771823d728`, its functions are marshalled to the event loop; see Review
  fixes.) Wheels:
  only macOS arm64 was built here (maturin 1.9.6); the Linux x64/arm64 glibc
  wheels (O20) are NOT_RUN, no Linux build host in this pass. The
  production LOC counter does not count Python; the package's Python lines
  are reported beside it in the commit.
- Steps 10b/10c ran in a subagent worktree (branch `issue533-v2-pg`, from
  step 9) and were cherry-picked onto step 16. The library wiring is a
  separate commit: `store.connect` on an empty database makes the fixed
  `layerfs` schema with a new binding (an existing LayerFS schema keeps
  its binding), and `project.init` into a PostgreSQL Store runs the live
  add in the caller's process and writes the project record with its
  LayerStack. Removing a project from a PostgreSQL Store stays
  `Unsupported`. Sandboxes on PostgreSQL Stores stay `Unsupported` (O3
  needs the docker machine's Sandbox path, not built in step 12).
- Step 17: the Rust product reads `LAYERFS_HOME`, the trace and
  measurement settings and the internal names only (`LAYERFS_BUNDLE`,
  `LAYERFS_WORKER_*`, the daemon's `LAYERFS_DAEMON_LISTEN`, `LAYERFS_KERNEL`,
  `LAYERFS_SANDBOX_HELPER`, and the daemon's own `LAYERFS_NFS_*` test
  overrides, which the host no longer passes it). Removed: the host's
  `LAYERFS_NFS_*` passthrough to native daemons, the `LAYERFS_DAEMON`
  fallback (the native machine record's `daemon` replaces it) and
  `MinioSettings::from_env` (its two tests build the settings themselves).
  `layerfs import-env` is a shell command (not a table call, S §2): it
  fills only absent record fields, never writes a secret, and answers the
  `machine create`/`machine connect` command for VM and ssh settings
  instead of making a VM or connecting a host, because those have effects
  of their own. 1.1.0's `LAYERFS_LOCAL_DIR` is reported as ignored: native
  Sandboxes live under `machine/native/` (step 12).
- Correction to the H §7.7 row above (found while writing the 2.0
  manual): the code does not do what that row says. Store routes follow
  `Kind::route` (`layerfs-sandbox` `machine/kind.rs`), which keeps `docker`
  on a Linux host in-process, and that machine's adapter starts no worker
  there; a Store moved to `docker` on Linux would then be opened from the
  home's own store file, not from a Docker volume. Nothing ran on Linux in
  this pass, so neither route is verified there. A Linux follow-up must
  either give `docker` on Linux a worker (the row's choice) or make the
  in-process route open the volume's file. **Resolved by `4c740437cd`**
  (the row's choice; see "Terminal pass fixes" below).

## Step-12 deferral closed (owner instruction, 2026-11-02)

The owner's instruction, relayed by the parent session ("Yes, send it"):
build the `docker` and PostgreSQL Sandbox paths before step 19 instead of
shipping them `Unsupported`. Built in `a487409fe5`. The choices below are
implementation choices, recorded because they are not in the spec.

| Topic | Choice | Why |
| --- | --- | --- |
| Where a docker Sandbox's Store comes from | Docker's `-v` source syntax in the existing `store_volume` string: `volume` (1.1.0, Store at `global/store.sqlite`), `volume/sub/path` (a volume subpath), `/host/folder` (a bind mount); the last two put the Store at `/layerfs-store/store.sqlite`. | The 2.0 docker machine keeps every Store in one data volume (`store/<name>/`), so a Sandbox mounts only its Store's folder. Keeping the string keeps the Sandbox record format; volume names hold no `/`, so the forms never overlap. |
| macOS and Windows | The caller sends the docker machine's image (and the volume folder and `daemonHost` `host.docker.internal`) with the worker call; the worker adds its own Engine socket and the daemon beside its executable, and makes the Store folder `0700`/`0600` (root's) first. | H §7.7: guests never read the home. The daemon requires a private Store folder owned by its user (root in the container). |
| Linux | The call runs in the caller's process (as `Kind::route` routes it) and bind mounts the Store's home folder; a caller that is not root is refused before any effect. | The daemon (root in the container) must own the folder. **Unverified: nothing ran on Linux.** The earlier correction row still applies; a Linux follow-up may prefer a worker container for `docker` on Linux. **Superseded by `4c740437cd`:** Linux now takes the macOS row's route through the worker container. |
| PostgreSQL Sandboxes | `native` and `docker` from the caller's process, which resolves the password; a namespaces Sandbox (Linux `native`) and VMs/ssh are refused. Under Docker Desktop (not Linux) a loopback URL becomes `host.docker.internal` for the daemon only. | O3/G13. In 2.0 the caller makes Store calls itself, so a loopback server must also be reachable from the container. |
| One-shot calls | An in-process Sandbox call opens its Store first (`store::open::hold`), as Store calls do. | The host code reads the Branch through the open Store; a one-shot command had none. |
| The docker worker's own image | Falls back to the docker machine record's `image`, as 1.1.0's macOS worker did. | Nothing set `os_image`, so the docker worker could never start. |
| Docker 29 "no such object" | Matched without case. | Docker 29.5.2 lowercases it, which made `store move <s> docker` fail at its check. |

## Terminal pass condition (owner direction, 2026-11-02)

The owner's direction, relayed by the parent session: "can we update the
terminal pass condition to be all 7 platforms to pass the 12 cell tests?"
Step 19 now passes only when the 12 #516 cells PASS on all seven machine
kinds (`native`, `docker`, `apple-container`, `microsandbox`, `lima`,
`firecracker`, `ssh`); a NOT_RUN or a refusal on any kind fails it. README
§1, §3 and step 19 say so. The owner also pointed at #527's platform pass
as the method:

| Topic | Choice | Why |
| --- | --- | --- |
| microsandbox | `~/.microsandbox/bin` on `PATH` for the cell runs, recorded in TERMINAL-PASS; the probe still looks on `PATH` only and still reports `missing: tool:msb` without it. | As #527 and #529 ran it (msb 0.7.11); 1.1.0 also found `msb` on `PATH` only. |
| Linux rows | `native` (namespaces) and `docker` on Linux, as root in a Lima QEMU VM (Ubuntu 24.04) relayed by the runner, labelled as such. | This Mac has no Linux host; #527 ran its Linux rows this way. Replaces the earlier plan of a privileged Docker container. |
| ssh | A throwaway Lima VM that LayerFS does not manage, removed after. | #527's `vm:ssh` target. |
| firecracker | #516's recipe as #527 used it: Firecracker v1.17.0, the CI kernel and rootfs, nested in a Lima `vz` host with nested virtualisation and huge pages, 2 GiB per microVM; files already on this host are reused. | Needs the P0-6 fixes (dfd444bd9b): the host stays up and starts first, and the microVM resets. |
| The sealed bundle's builder | `tools/linux_bundle.py` (6263b95871) builds and places `<cache>/layerfs/bundle/<version>/linux-<arch>/`; `tools/worktree.py build --bundle` is not reused. | `worktree.py build` builds four packages in one Cargo command (the owner asked for one bounded command per package), and seals the E2E SDK bundle (`manifest.json`, `api/`, `layerfs.node`), not the guest bundle: no Sandbox image archive, no `bundle.json` hash, and nothing placed where LayerFS looks. Both share the launcher's registered Linux target and helpers (`docker_build`, `execute`, `inputs`). |

## Review fixes (2026-11-02)

The review of `707b534eb2` (items P0 1–8, P1 9–28, P2 29–32) was fixed
under the same delegation, on four fix branches (`issue533-v2-store`,
`-home`, `-backups`, `-calls`) cherry-picked onto `issue533-v2`, plus
commits made on it directly; the docs items were done on
`issue533-v2-docs`. Each fix was checked against the code first; each
commit message names its findings, its tests and its Production LOC line.
No finding was found wrong. Two were already fixed by commits after
`707b534eb2` and before the fix series: P2-32's `layerfs store import-env`
text (`73e38ecf27`) and P0-4's docker worker image key (`a487409fe5`, the
fallback to the record's `image`).

| Finding | Status |
| --- | --- |
| P0-1 Store call vs move/remove race | Fixed in `e375e333db`: `lease::hold` takes the home lock, re-reads the record and refuses `InUse` during a move or remove, the mover's own process included (no own-pid exemption); the mover re-checks holders, its own process's leases and Sandboxes under the lock before the snapshot. |
| P0-2 reaper releases a Store mid-call | Fixed in `73624c56b9`: each open entry counts its running calls; the reaper closes only an idle entry with none, and `open::close` refuses `InUse` while one runs. |
| P0-3 worker open cache holds Stores unleased | Fixed in `73624c56b9`: a worker holds the Store's lease while its cache keeps it open; snapshot, install, retire and remove close this process's open copy first. |
| P0-4 namespaces Sandboxes never start; image keys | Fixed in `6f2a43c4fd` (after the move `c3175b6eb9`): `deploy.imageArchive` and `deploy.image` are passed, the worker uses its bundle's daemon and archive, a record's `image` naming a file is the machine's archive. The docker worker's image key was fixed in `a487409fe5`. |
| P0-5 VM worker idles off under Sandboxes | Fixed in `155e11f2fa`: it counts the Sandboxes it runs and those recorded under `store/<s>/project/<p>/sandboxes/`. |
| P0-6 Firecracker host | Fixed in `dfd444bd9b`: the Lima host counts live microVMs, a Firecracker start starts its host first, an idle microVM resets (`sync; reboot -f`). |
| P0-7 store remove deadlocks on a stopped machine | Fixed in `e375e333db`: the machine is started (or refused) before the home lock; a machine that moved meanwhile is `Conflict`. |
| P0-8 restore over live Sandboxes; concurrent restores | Fixed in `842b4058b9`: `InUse` while a Sandbox runs or SQLite's sidecars exist; a new name is re-checked under the lock, the first install wins, the other is `Exists`. |
| P1-9 home lock held across slow work | Fixed for Stores (`e375e333db`), backups (`842b4058b9`), PostgreSQL (`67a4723ea3`) and machines (`539cc0e771`): the lock covers checks and record writes only; a move whose record phase fails keeps `in_progress` with both copies. |
| P1-10 store status boots a VM | Fixed in `75f83ec9b6`: `route::endpoint_now` starts nothing; `state: "down"` with `down_reason` (O34). |
| P1-11 shared temporary name; writers outside the lock | Fixed in `bd8f90860b`: a per-process counter in the temporary name; the record writers take the (now reentrant) home lock. |
| P1-12 every new home looks like 1.1.0 | Fixed in `d1153d1776`: a 1.1.0 home is recognised by its own records. |
| P1-13 migrated Stores have no file | Fixed in `92c2db72b3`: the 1.1.0 file is hard-linked or copied (sha256 checked) to `store/<name>/store.sqlite` where this host can read it; otherwise reported. Not verified: Linux, the copy fallback, a real Docker volume on macOS (report only). |
| P1-14 backup list states; exported-from; `Expected::Any` | Fixed in `943363f5b0` (seven states, `unknown` with `reason`) and `dfe87f9b68` (`layerfs-exported-from` written and checked, `Exists` for another home; a link without sha256 expects the file absent). Deferred: reading heads of a Store not on `native` through its worker (it is `unknown` meanwhile), and an exported-from column in `--text`. |
| P1-15 PostgreSQL second name; URL password | Fixed in `67a4723ea3`. |
| P1-16 remove treats an unknown machine as native | Fixed in `e375e333db` (`Unavailable`), with the follow-up `d6c844eb33` (an in-process machine's Store deleted once). |
| P1-17 commit.get stops at a short page | Fixed in `88b033cfa9`: follows `nextPagePos` to the end; a position that does not move on is `Failed`. Its cost grows with the Branch's history (the calls agent noted it may be quadratic; not measured). |
| P1-18 exec output over 1 MiB lost | Fixed in `37f1bbd2b9`: spooled streams go to the caller as output events, 1 MiB per page, and the spool is closed; the reply keeps the stream `null` with its byte count and `streamed: true`; non-UTF-8 inline output is bytes. Still after the command ends, not live. |
| P1-19 scope lets uncovered calls through | Fixed in `797ecca2c7`: an allow list, bound to `{ project, folder, branch? }`; the shell needs `--scope-project`. |
| P1-20 workspace.commit drops `force` | Fixed in `37f1bbd2b9`. |
| P1-21 O31 not built | Fixed in `37f1bbd2b9` (with `cc75955474`, mounts found by the real path). |
| P1-22 Sandbox records without id; names; lock; stale project record | Fixed in `37f1bbd2b9`. |
| P1-23 start.lock in create/remove; microsandbox volume | Fixed in `539cc0e771`. |
| P1-24 Node handler kills the process; late events | Fixed in `f67d5a9f6f`. |
| P1-25 codes, paging, stream, exit 125, scoped branch.list, aio | Fixed: `NoSuchStore`/`NoSuchProject` (`9a4d9528e0`), `NoSuchBranch`/`NoSuchCommit` (`88b033cfa9`), Sandbox paging (`37f1bbd2b9`), Node `stream` (`f67d5a9f6f`), Python `stream` and aio on the loop with cancel stopping the functions (`771823d728`, `94671c974c`), `--stream` exit 0 for no code (`0f7b2b652f`), scoped `branch.list` across pages (`797ecca2c7`). Not done: cancelling the native call itself (it runs to its end; the docstring says so). |
| P1-26 worker frames read before the secret; no timeouts | Fixed in `e70c038586`. |
| P1-27 lease pid reuse; pid 0 / u32::MAX | Fixed in `6908c9a0af`. |
| P1-28 Windows cfg gates | **Not done: paused, waiting on the owner.** A Windows `cargo check` on this Mac could not get past the `zstd-sys` and `blake3` build scripts, which need an MSVC C toolchain this Mac lacks, so the gates could not be checked. |
| P2-29 LOC lines; deleted JS tests; postgres_connect | Done in the docs series: [HANDOFF.md](HANDOFF.md) has a recounted LOC table for every commit, the 63 files and the `d244cdb3af` note. History is not rewritten. |
| P2-30 temp paths in receipts; test timeouts | Done in the docs series: receipts read `<tmp>/…`; TERMINAL-PASS's command cells and the api and Python READMEs show the ≤120 s limit. |
| P2-31 Replaced rows | Done in the docs series: G0, G4, step 5.2, the single branch; G10 resize recorded as deferred. |
| P2-32 small fixes | `layerfs import-env` text: `73e38ecf27` (code) and the docs series (limitations); `LAYERFS_LOCAL_DIR` text and `start.lock` in the H §7.8 layout: `7cf2368a9d`; maturin: `2793ee114c` (requires `>=1.9.6,<2.0`); a guest worker's home id (O33): `e6a5f38b68`; machine update `InUse` while Sandboxes run: `539cc0e771`. |

The same series also built what the review's covering message asked
besides the findings: the version `2.0.0` (`085376a7a8`) and the sealed
Linux bundle builder `tools/linux_bundle.py` (`6263b95871`, O16).

### Implementation choices in the fixes (not owner decisions)

| Topic | Choice | Where |
| --- | --- | --- |
| Lease identity | A lease file records `<at> <start> <boot>`; a move's `in_progress` records `pid_start` and `pid_boot` (`LocalProcess`: `proc_pidinfo` on macOS, `/proc` on Linux). A reused id is stale; a lease without them is matched by id alone; ids 0 and above `i32::MAX` are never alive. | `6908c9a0af` |
| Worker hello | A connection opens with `{ secret }`, at most 4 KiB, read within 10 s, answered `{ ready: true }` or `Denied`, before the request `{ op, args, home_id }` (at most 30 s between bytes). `ping` waits 10 s for its answer; other operations as long as they run. No answer before the request is `Unavailable`; only a connection lost after it is `Unknown`. | `e70c038586` |
| Node events | The native call resolves to `{ events, answer }`; the package settles once it has delivered that many events. A throwing function is a `LayerFSHandlerWarning`. | `f67d5a9f6f` |
| Home lock | Reentrant for the thread that holds it, so a record writer locks whether or not its caller does. | `bd8f90860b` |
| Home id to workers | Every worker request carries the calling home's id; a worker's root is marked and never converted or given its own id. | `e6a5f38b68` |
| Exported-from | `layerfs-exported-from: <home id>/<machine>` on every backup object; a `native` Store's snapshot is staged in this process and uploaded with its sha256, heads, binding and exported-from (`remote::push` is no longer used; this route does not pre-check free space or bound the manifest). | `dfe87f9b68` |
| URL passwords | Refused in the user part and in any query parameter whose decoded name contains `password`, any case, before any effect. | `67a4723ea3` |
| Exec output | Over 1 MiB per stream, sent as output events (`{ stream, bytes }`) after the command ends, 1 MiB per event; the reply's stream is `null` with `stdout_bytes`/`stderr_bytes` and `streamed: true`. A caller with no function, `stream` or `--stream` does not get those bytes. | `37f1bbd2b9` |
| Forced close (O31) | The Workspaces not drained are answered in the reply as `unknown` (their paths); nothing else records them. O31's "records `Unknown`" is read as this answer. | `37f1bbd2b9` |
| Removing a machine | Not a new `state` value: the record gets `removing: { pid }` under the home lock while the VM is deleted outside it; `machine_of` refuses `InUse` while that process lives. A refused delete restores the record; an unknown one sets `state: "unknown"` and `error`. | `539cc0e771` |
| `start.lock` on create and remove | Both take it (readiness wait up to 600 s) before the home lock, as the on-demand start does; the start re-reads the record once it holds it. | `539cc0e771` |

## Terminal pass fixes (2026-11-02)

Running the seven-kind pass found seven defects that no stand-in test
reached. Each was fixed, given a test, and the affected cells were run again
as new samples; the failed receipts are kept (TERMINAL-PASS, "Samples that
failed"). None of these is an owner decision. The pass then ran 12/12 on all
seven kinds and both Linux rows at `6837bcfce2`. Running the other step-19
items afterwards found two more, in `machine.list`/`status` only, which no
cell step calls (the last two rows; the first is a delegated owner
decision).

| Topic | Choice | Where |
| --- | --- | --- |
| Folders above a namespaces Sandbox's local root | The worker's root (`/layerfs/data`) is 0701, not 0700. Before a namespaces Sandbox starts, `home::traversable` adds o+x (never read) to the home's folders down to `machine/native/sandbox-root`; folders above the home are unchanged, and the runtime still refuses one that is closed. macOS native is unchanged. Not taken: moving the local root outside the home, which H §7.8's layout keeps inside it. | `005a30c93c` (first C01 on `lima`: "`/layerfs/data` is not traversable by others") |
| The host's private-folder check | A folder may carry o+x and nothing else from group or others; a file still may carry nothing. | `44db721d95` (the next C01: "`/layerfs/data` must be owned by this user and private") |
| Firecracker's files | The bundle copy every machine gets leaves out `firecracker/`; a nested Lima host copies that folder from the host straight to `/layerfs/data/firecracker`, once per bundle hash, before its worker starts. Before, a multi-GB root filesystem would have been copied into every VM, ssh host and microVM. `bundle.json`'s hash does not cover `firecracker/`. | `e1cfcde498` |
| Idle stop in containers | `apple-container` and `microsandbox` machines never stopped when idle: `poweroff` is missing in the Sandbox image, and the main process ignored the signal. The worker now powers off with `reboot(2)` when `poweroff` is missing (`layerfs_api::worker::power_off`); a new stop mode `init` (`sync; kill -TERM 1`) is `apple-container`'s, whose main process now exits on TERM. Seen live afterwards: `ac1`, `msb1`, `lima1` and `fch` stopped themselves, `ssh1`'s worker exited. | `0170995ce4` |
| Firecracker huge pages | Create writes `"huge_pages":"None"`; each boot picks `2M` when the host has enough free 2 MiB pages for `mem_size_mib`, otherwise `None`. Before, a config made with huge pages could not boot after the host rebooted and lost them. | `164e1297cb` (a warm-up move after a reboot: "Cannot load kernel due to invalid memory configuration") |
| Ending an old worker | The guest's launch script (`LAUNCH`) and the ssh `END_WORKER` end only a process whose command line is exactly `<bundle>/layerfs worker` and which is not the script itself. Before, a pattern match also matched the launching shell when its pid was reused, so it killed itself (exit 143). | `b10dbcac8c` (`ac1` C01 restarting from stopped) |
| `docker` on Linux | Routes to its worker container, as on macOS (`Kind::route`: only `native` is in process); `docker/linux.rs` is removed and `macos.rs` serves both. Before, a Store moved to `docker` on Linux stayed in the home's own file, so the next Store of that name was refused `Exists`. Resolves the correction to the H §7.7 row. Deleted test: `docker_on_linux_runs_in_process_and_starts_nothing`. | `4c740437cd` (Linux `docker` C01: "machine docker already holds a Store file named cells") |

| `machine list` and `get` state | Delegated owner decision (recommended answer), relayed by the parent session after the pass showed `lb` idle-stopped but still listed "running": the list (and `get`) report the same live probe as `machine.status` for VM and ssh machines. All probes run at once within 10 s, start nothing, take no lock, and write nothing back. A failed or late probe is `unknown` with the reason in `error`. Built-ins and a machine being created or removed keep their record. Connect's Compute page reads `machine.list().machines[].state`. | `c89727281a` |
| What counts as an up worker | Found while showing the above: an `ssh -L` (or Lima) forward accepts on the host with no worker behind it, so `status` and `list` called `ssh1` "running" after its worker idled out. `worker::answers` now needs the secret hello answered `{"ready":true}` (2 s) and sends no request. | `1f41e6bc4f` |
| What restarts a worker's idle clock | Delegated owner decision (recommended answer): only an authenticated request that does work, which is any request but `worker.ping`. A connection, the secret alone, a ping, or a denied or malformed frame does not (this closes the rest of review P1-26). Before, every accepted connection restarted it, so polling `machine.list` would have kept every machine up. The guest worker changed, so the bundle was resealed at `f43ec45fa0`: hash `c8baa5a9b660a8ee816b544e6a57dad0ccb2177408d57b77afa73884f06fc6f9`, the same Sandbox image. | `f43ec45fa0` |

Gaps seen in the pass and left open:
- A microVM whose start fails after it booted keeps running with no worker,
  so nothing idle-stops it, and its Lima host stays up behind it (the host's
  worker sees a live `firecracker.pid`). Seen once (below); the pass stopped
  that microVM by hand. `machine stop` would cover it (not built).
- Without huge pages, copying a new bundle into a Firecracker microVM is
  slow and fragile on a 4 GiB nested host. After `fch`'s idle-stop reboot
  dropped its huge pages, the 2 GiB microVM ran on ordinary memory. Copying
  the 2.75 GB Sandbox image took 375 s, and then the `.sealed` write failed
  ("Connection timed out during banner exchange"; 46 MB free in the guest,
  85 MB on the host). With 1,100 huge pages restored, the same cold start
  and copy took 31.7 s. Nothing in the product sets huge pages (gap above);
  a smaller image or a host sized for the microVM would also help.
- Removing a nested Firecracker machine whose Lima host is stopped is
  refused `Unavailable`, and the host's removal is refused `InUse` while it
  hosts the microVM; with no call to start a machine, the second cleanup
  started the host with `limactl` first (TERMINAL-PASS, "Cleanup").

- A running machine keeps the worker and bundle it started with until it
  stops (idle stop or reboot); there is no `machine stop`. To load a rebuilt
  bundle mid-pass, a VM was stopped with `limactl stop`, which has the same
  effect as the idle stop.
- Nothing sets a nested host's huge pages; since `164e1297cb` the microVM
  uses 2 MiB pages when enough are free at boot and boots without them
  otherwise. The pass set 1,100 on the Firecracker host by hand, as #516 and
  #527 did (declared setup).
- `tools/linux_bundle.py` does not build or place `firecracker/`; the pass
  built it from #516's checksum-verified downloads and added it to the sealed
  bundle after sealing (see TERMINAL-PASS).
- The sealed bundle's hash differs on every seal of the same source: the
  layer tar records file times. Two seals of `6837bcfce2` are not
  byte-identical; the pass used one seal (`4a77c75e…`) for every row.
- Linux setup is the host's: `native` Sandboxes need
  `user.max_user_namespaces=0` (#527 lists it), and the Linux VM needed
  Docker installed by hand. Neither is done by LayerFS.
