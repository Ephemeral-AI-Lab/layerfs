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
| G0 | Base is `ddc7c5696b` (head of PR #530, containing #528 and `layerfs_v2_0_0`). No PR is merged on GitHub; branching from that head satisfies it. |
| G1 | H and S are accepted as the 2.0 direction. |
| G2 | The H §9.1 AGENTS.md wording is approved and applied in step 1. |
| G3 | #531 is held; this issue supersedes it. Nothing here builds on #531. |
| G4 | #521 is not merged on GitHub. Its branch (`origin/area-p/complete-p1-p15`) is read; folder scope is ported into the library (step 14) and its PyO3 binding is used as a reference (step 16). MCP follows O32. |
| G5 | O21a: `layerfs-api` grows into the library (no `layerfs-sdk` crate). O21b: the crates stay in `engine/crates/` for 2.0. |
| G6 | PyO3 is accepted as a new dependency, and maturin for wheels. |
| G7 | O22: one `on_progress` stream; no job registry. |
| G8 | O23: one backup link per Store per bucket; replaces `send` and `store.unlink`. |
| G9 | O24: `project.remove` added as a 55th call. O25: the 14 `@layerfs/backend` operator and diagnostic calls are dropped from the 54; what Connect needs stays behind the trace files. O26: trace files are the feed; no subscribe call. |
| G10 | O11 widened: `machine update` is added (a call beyond the 54), also for `native` and `docker` (image, Docker socket, WSL distribution). |
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
| O32 | Folder scope is ported into the library as a client option of every front end; MCP is dropped from 2.0 (not a fourth front end here). |
| O33 | The `docker` machine names containers with the home id from `layerfs.json`. |
| O34 | `machine.status` reports state, free disk, the provider tool's version and why a machine is down; no change event. |
| O35 | A lease `store/<name>/open/<pid>` whose process is not alive on this host is stale, removed under the home lock, and the removal is logged. |

## Consequences of the answers for the call count

With O13, O14, O24 and O11 accepted, the surface is S's 54 calls with `fs`
renamed `file`, plus `project.remove`, `workspace.status` and
`machine.update`: 57 calls in nine families.

## Replaced recommendations

Rows added here when a recommendation proved wrong against the code. Each
names the smallest safe alternative the docs offer and why.

| # | Recommended | Taken instead | Why |
| --- | --- | --- | --- |
