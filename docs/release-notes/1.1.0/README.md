# LayerFS 1.1.0

> **Status:** Release candidate for LayerFS 1.1.0. Not tagged. The candidate
> is branch `claude/issue428-sdk-packages-cd9412`: product source as at
> `3c21c62c6e` (its child `fb83195e9e` adds one documentation file), plus the
> version bump and this record. The tag `v1.1.0` is to point at the release
> commit that adds this record.

## Owner direction

On 2026-10-24 the owner chose the version **1.1.0, with a waiver**. 1.1.0
breaks code written for the 1.0.0 JavaScript SDK. The
[v1.0.0 release](https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v1.0.0)
promised that "an incompatible change to the JavaScript SDK needs a new major
version", and the [release policy](../../general/release-policy.md#version-meaning)
says 1.0 and later follow ordinary semantic versioning. Under that promise this
release would be 2.0.0. The owner waived the promise for this release; the
[waiver](#owner-waiver-of-the-100-major-version-promise) below records what
breaks, why the version is 1.1.0 and the residual risk.

## What 1.1.0 is

The work of issue [#428](https://github.com/Ephemeral-AI-Lab/layerfs/issues/428)
(the [design](../../../core/docs/issues/428/README.md) and its
[change list](../../../core/docs/issues/428/CHANGES.md)) and the fixes merged
since v1.0.0. How to use it is in the
[JavaScript packages wiki](../../../api/README.md) and its guides for
[`@layerfs/agent`](../../../api/how-to-use/agent.md),
[`@layerfs/backend`](../../../api/how-to-use/backend.md) and
[`@layerfs/runtime`](../../../api/how-to-use/runtime.md).

- **Three packages instead of one.** `@layerfs/sdk` is split into
  `@layerfs/agent` (Projects, Branches, files, Sandboxes, Workspaces, Exec and
  Commit), `@layerfs/backend` (Store and daemon reads for UIs and tools, which
  replaces `@layerfs/sdk/operator`) and `@layerfs/runtime` (what a host runs on
  a LayerFS home: status, Init and removal, the macOS and Windows worker,
  Sandbox and volume upkeep, live call events). The sources moved from
  `sdk/js/` to `api/`; the native addon crate `layerfs-node` is
  `api/binding/`.
- **The `layerfs` command** (`api/agent/cli/`, package `@layerfs/cli`). One
  `@layerfs/agent` call per command with JSON results, so an agent whose only
  tool is Bash can use LayerFS. Its tests run every SDK case through both the
  SDK and the command and require the same answer.
- **Sandboxes are explicit.** `sandbox.create()` starts a Sandbox (the only
  call that does) and `workspace.mount(sandbox, branch)` mounts in the Sandbox
  it names. There is no automatic placement. New calls:
  `project.list_sandboxes`, `sandbox.create`, `sandbox.list_workspaces` and
  `sandbox.close`.
- **Clients are opened by factories.** `await LayerFS.open(name)` opens an
  existing project and checks it exists; `await LayerFS.init(name, folder,
  timeout?)` creates one. Settings come from `LAYERFS_*` environment variables
  only. A closed client refuses later calls.
- **macOS and Windows.** Store calls still run in the caller's process on
  Linux only. On macOS and Windows, any process given a LayerFS home's worker
  endpoint (`LAYERFS_WORKER_ENDPOINT`) sends its calls to that worker: a Linux
  container on macOS, a Node process in a WSL2 distribution on Windows. The
  worker listens on loopback behind a secret, caps unauthenticated frames and
  times out the secret read. LayerFS names no plugin; any integration may use
  the route (owner, 2026-10-24).
- **Init.** Init has no fixed 60 s deadline: its time limit is optional and
  defaults to 300 s (#431). It names the source entry it refuses (#410).
  Once the install is recorded, Init deletes its sealed copy of the Store and
  answers the installed Store's path.
- **Fixes.** Every JavaScript binary value is accepted as a write body
  (#413). A merge keeps a new hard-linked file that it still binds elsewhere.
  A process waits for the daemon to admit its first connection instead of
  failing `UnexpectedEof`. A Sandbox container's log is capped at two 10 MB
  files, and the runtime lists Sandbox scratch and Store volumes and sweeps
  stopped Sandboxes (#423).
- **Repository.** The LayerFS tab for DeepSeek Harness moved to its own
  repository, [dsh-layerfs](https://github.com/Ephemeral-AI-Lab/dsh-layerfs),
  and the benchmark harnesses to
  [layerfs-benchmark](https://github.com/Ephemeral-AI-Lab/layerfs-benchmark).
  GitHub Actions now reports a fast lane and a nightly lane; it gates nothing.

## Owner waiver of the 1.0.0 major-version promise

### What breaks

Checked against the source at `v1.0.0` (`sdk/js/`) and at the candidate
(`api/`). Code written for 1.0.0 must change as follows.

| 1.0.0 | 1.1.0 |
| --- | --- |
| `import { LayerFS } from "@layerfs/sdk"` | `import { LayerFS } from "@layerfs/agent"` |
| `new LayerFS(name, options)`, lazy, no I/O | `await LayerFS.open(name)`, which opens the Store and refuses a missing project at once (`NoSuchProject`). The constructor is not public |
| `options.storePath`, `daemonHost`, `trace`, `traceStep`, `storePolicy` | `LAYERFS_STORE_PATH`, `LAYERFS_DAEMON_HOST`, `LAYERFS_TRACE`, `LAYERFS_TRACE_STEP`, `LAYERFS_STORE_POLICY`. An options argument is refused `InvalidInput`, not ignored |
| `DSH_LAYERFS_*` twins of every setting | Not read; only `LAYERFS_*` names |
| `client.project.init(path)` | `await LayerFS.init(name, folder, timeout?)`; default limit 300 s instead of a fixed 60 s |
| `workspace.mount(branch, { image })`, placed in a Sandbox LayerFS chose or started | `const { sandbox } = await client.sandbox.create({ image })`, then `workspace.mount(sandbox, branch)`. A mount never starts a Sandbox; it is refused `NoSuchSandbox` or `Capacity` |
| `workspace.status(ws)` | `@layerfs/backend`: `LayerFSBackend.of(client).daemon.workspace.status(ws)` |
| `@layerfs/sdk/operator` (`LayerFSOperator`: `store.*`, `files.*`, `workspace.*`, `inspect.snapshot`, `sandbox.list`, `project.progress`, `process.*`) | `@layerfs/backend` (`store.*`, `daemon.*`), `sandbox.list_workspaces`, `project.list_sandboxes` and `@layerfs/runtime` (`project.get_init`) |
| Protocol methods `operator.*` | `backend.*`; `RENAMED_METHODS` in `@layerfs/agent` maps each old name, for older trace files |
| `close()`, after which the next call opened the Store again | `close()`, after which every call is refused `InvalidInput` |
| Init's answer `store` named `sealed.sqlite` | `store` names the installed Store; the sealed copy is deleted |
| `sdk/js/`, `npm --prefix sdk/js run build:native`, addon `sdk/js/native/layerfs.node` | `api/`, `npm --prefix api run build:native`, addon `api/binding/layerfs.node` |

The 1.0.0 `fs` (14 methods), `branch` (6), `project.list_branches`,
`workspace.exec`, `workspace.commit` and `workspace.unmount` keep their names
and arguments.

### Why the version is 1.1.0

Because the owner decided so on 2026-10-24; this record adds no other reason.
For context only: the release is source-only (every package is `private`, and
nothing is published to npm), v1.0.0 was released on 2026-10-21, and the dsh
LayerFS plugin, which builds on a pinned LayerFS commit, already uses the 1.1.0
API. This record is the explicit exception the policy requires, so the version
number does not hide the break.

### Residual risk

- Code written for 1.0.0 fails under 1.1.0, mostly at once (the package name,
  the constructor and options are refused), and in one case only when it runs:
  `workspace.mount(branch)` is refused `InvalidInput` because its first
  argument is not a Sandbox number.
- A reader who trusts the version number alone expects 1.0.0 code to keep
  working. The release body and this record say otherwise.
- The waiver covers 1.1.0 only. It does not lower the promise for later
  releases: the next incompatible SDK change needs a new major version unless
  the owner records another waiver.

## Compatibility

- **Stores.** The Store format did not change from v1.0.0. Between `v1.0.0`
  and the candidate, `layerfs-persistence/src` only moves the Init entry's
  metadata blob into its own module; `layerfs-history/src` is unchanged,
  `layerfs-storage/src` changes one comment, and `layerfs-content/src` changes
  merge behaviour (the hard-link fix) and formatting, not encoding. The open check
  (`application_id` and `user_version`) is unchanged. New Stores use schema
  16–18 or 22–24 with history identity format 3, as in 1.0.0. So a v1.0.0
  Store passes 1.1.0's open check. Reading this from source is not a test:
  opening a v1.0.0 Store with 1.1.0 was NOT_RUN. Stores from v0.1.7 and
  earlier are refused, as in 1.0.0.
- **Project records.** The addon's record code (`registry.rs`) is byte-for-byte
  the 1.0.0 code. The LayerFS home layout (`LAYERFS_HOME/<project>/`) is as in
  1.0.0; the [home proposal](../../../core/docs/issues/428/layerfs-home.md)
  (`~/.layerfs`) is not part of this release.
- **Version matching.** `@layerfs/agent`, `@layerfs/backend`,
  `@layerfs/runtime`, `@layerfs/cli`, the native addon, the daemon and the
  runtime image must all come from this tag. `@layerfs/backend` and
  `@layerfs/runtime` depend on `@layerfs/agent` at exactly `1.1.0`, and
  `@layerfs/agent/internal` exists only for them at that version. No
  handshake compares package versions: the worker frames carry only wire
  version 1, and a 1.0.0 client against a 1.1.0 daemon is untested and
  unsupported (the protocol renamed `operator.*` to `backend.*`).
- **Platforms.** Store calls run in the caller's process on Linux, with the
  Store on ext4, xfs or btrfs. On macOS and Windows they go through the home's
  worker; without a worker they fail `Unsupported`.
- **Store profile.** Disposable is the only executed profile; there is no
  crash or power-loss durability. Durable is NOT_RUN — disabled by owner until
  explicit reauthorization.

## Verification evidence

| Evidence | Source identity | Result |
| --- | --- | --- |
| dsh-layerfs live, Windows: all 10 scripted scenarios (Tour, Parallel Branches, Conflicts, Commit & merge, One Sandbox two Workspaces, 5 agents, Shop, Depot, Crew, Checkpoints; 5 through the SDK, 5 through the `layerfs` command) | LayerFS `3c21c62c6e`, recorded by dsh-layerfs commit [`02b9dc9`](https://github.com/Ephemeral-AI-Lab/dsh-layerfs/commit/02b9dc964f50bcf321cef9c3939718abc0dc3947) (`docs/windows-live-verification-20261024.md`) | PASS, one run per scenario. Machine reboot and the historical-order/final-tree trace checks NOT_RUN there |
| dsh-layerfs live, macOS (Docker Desktop worker): one Run all of the same 10 scenarios, 2026-10-24 | LayerFS `3c21c62c6e` (clean detached worktree), dsh-layerfs [`02b9dc9`](https://github.com/Ephemeral-AI-Lab/dsh-layerfs/commit/02b9dc964f50bcf321cef9c3939718abc0dc3947), dsh 0.2.1-rc.2, macOS 26.4.1 arm64, Docker Desktop 4.76.0; record `docs/macos-live-verification-20261024.md` in dsh-layerfs | 9 of 10 PASS in Run all. Depot FAIL at its first command there: the profile's runtime image had no `cargo` (an image defect, not LayerFS). After a setup fix, one Depot rerun PASS: 98 of 98 steps, 376.8 s, `tools/checks/run.mjs` 97 of 97 calls matched. That rerun ran on image `afa1ef9c`, built from the `02b9dc9` Dockerfile plus one change to HTTPS Debian sources (Docker Desktop's proxy cut plain-HTTP downloads), so it qualifies Depot on that image, not the unmodified Dockerfile. Reboot and order/final-tree trace checks NOT_RUN |
| Release checks on the candidate tree (below) | `fb83195e9e` plus the docs restructure, the rustfmt fix, the version bump and this record, run before they were committed as the three commits after it; macOS host, 2026-10-24 | See the checklist |

Release checks, run once each on the macOS host (aarch64, Darwin 25.4.0,
`cargo +1.85.1`, Node 24.21.0), each under a 110 s wall alarm:

| Check | Result |
| --- | --- |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | PASS after the fix. It first failed on one assertion in `core/crates/layerfs-sandbox/tests/container_labels.rs` (from `5d0d7db5ec`, #423); the owner chose to fix it, and `cargo fmt` reformatted that one assertion (test code, no product source) |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` | PASS (31 s), all 15 workspace packages |
| `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --all-targets --no-run` | PASS (two passes: the first reached the alarm while compiling, the second finished) |
| `cargo +1.85.1 test … --locked -p <package> --all-targets`, every package (`layerfs-api` in three target groups) | PASS: 1,222 passed, 0 failed, 21 ignored. The ignored tests need prepared Stores or setup (`LAYERFS_355_*`, `LAYERFS_364_*`, `LAYERFS_Q1_PREPARED`, `LAYERFS_INSTALL_DAEMON_BINARY`) or are large diagnostics; NOT_RUN |
| Linux-only `layerfs-daemon` FUSE mount targets (`#![cfg(target_os = "linux")]`, the CI mount lane) | NOT_RUN: they compile to nothing on macOS |
| `python3 -B core/tools/check_product_boundary.py` | **FAIL, owner waiver** ([below](#owner-waivers-of-release-gates)): 4 test-only attributes (`#[cfg(test)]` modules) in `api/binding/src` (`discovery.rs` 607 and 635, `pool.rs` 248, `sandbox.rs` 504), all older than #428. With `--skip-sdk` (CI's scope, core only): PASS, 817 files |
| `python3 -B -m unittest discover -s core/tools -p 'test_*.py'` | PASS, 57 tests |
| `node api/agent/sdk/test/calls.mjs` | PASS, 36 cases |
| `node --test api/agent/cli/test/cli.test.mjs` | PASS, 70 tests (including `--version` 1.1.0 and the command tables of `api/how-to-use/agent.md`) |
| `packaging.mjs` (7 cases), `trace.mjs` (9), `exec_output.mjs`, `backend.mjs` (4), `runtime.mjs` (11), one case per command | PASS, all 32. `packaging dispatch` first failed `MODULE_NOT_FOUND` because no addon was built; after `npm --prefix api run build:native -- --debug` it passed |
| `init_refusals.mjs`, `reinit.mjs`, `binary_write.mjs` and the `test/verify/*_real.mjs` tests (real Store, Docker and daemon, under the E2E launcher) | NOT_RUN: Linux with Docker |
| `git diff --check` | PASS |

## Release requirements

The [release policy](../../general/release-policy.md#release-requirements)
lists what makes a candidate eligible.

| Requirement | Status | Evidence |
| --- | --- | --- |
| The workspace builds and all tests pass | **PARTIAL, owner waiver** ([below](#owner-waivers-of-release-gates)): build PASS; tests PASS on macOS; Linux-only tests NOT_RUN | Checks above. Linux FUSE mount tests and the real-Store/Docker JavaScript tests did not run on the macOS host; the Windows live scenarios exercise mounted Workspaces in Linux containers |
| Formatting, warning-denying Clippy and `git diff --check` pass | PASS | rustfmt PASS after the one-assertion fix; Clippy and `git diff --check` PASS |
| The public SDK and CLI reference match their exported surface | PASS for `@layerfs/agent` and `layerfs`; NOT_RUN as a mechanical check for `@layerfs/backend` and `@layerfs/runtime` | `cli.test.mjs` checks the command tables in `api/how-to-use/agent.md` and that every SDK call has a command; `packaging`, `backend` and `runtime` `surface` cases check each package's method sets. Nothing compares `backend.md` and `runtime.md` with their packages |
| The SQLite schema and static SQL manifest pass their structural tests | PASS | `layerfs-persistence`: 118 passed, including the schema-reopen and refusal tests |
| Supported FUSE and container-runtime gates pass on a capable host | **NOT_RUN, owner waiver** ([below](#owner-waivers-of-release-gates)); Windows live PASS; macOS live 9 of 10 in Run all, and Depot PASS on one rerun after an image fix | No Linux mount lane ran for this candidate |
| The versioned manual and limitations are complete | **NOT MET, owner waiver** ([below](#owner-waivers-of-release-gates)) | There is no `docs/versioned/1.1.0/`, as there was none for 1.0.0. The manual for 1.1.0 is the API wiki `api/README.md` and the guides in `api/how-to-use/`, whose banners name LayerFS 1.1.0; the limits are below |
| Every published benchmark links to reproducible raw evidence and identifies the exact source | Not applicable | 1.1.0 publishes no benchmark. The 1.0.0 results were measured at `e5c7aaae05` and are not 1.1.0 claims |
| The release tag resolves to the reviewed source tree | Checked after tagging | This record cannot name its own tag's tree; the GitHub release body records `git rev-parse v1.1.0^{tree}` against the reviewed merge |

Each item that is FAIL, PARTIAL, NOT_RUN or NOT MET either needs a fix and a
rerun or an owner waiver recorded here before the tag. A waiver records that a
gate was not met; it does not turn it into a pass.

## Owner waivers of release gates

The owner decided these on 2026-10-24, for 1.1.0 only:

| Gate | Not met | Decision and residual risk |
| --- | --- | --- |
| Full-scope product boundary guard | 4 `#[cfg(test)]` modules in `api/binding/src` (`discovery.rs` 607 and 635, `pool.rs` 248, `sandbox.rs` 504), all older than #428; core-only scope passes | Waived. The modules are test code compiled only for tests, so the shipped addon is unaffected; they break the rule that product source holds no tests. Move them to `api/binding` tests in a later change |
| Linux-only tests and the Linux FUSE mount gate | The `layerfs-daemon` FUSE mount targets (the nightly mount lane) and the real-Store/Docker JavaScript tests (`init_refusals.mjs`, `reinit.mjs`, `binary_write.mjs`, `test/verify/*_real.mjs`) did not run for this candidate; no Linux host was used, and dsh-layerfs live has no Linux run | Waived. Mounted Workspaces were exercised only inside Linux containers behind the macOS and Windows workers (dsh-layerfs live). The last nightly run on `main` before #428 merged (2026-10-23, `22514d093d`, already in this candidate) failed 4 of its mount binaries: `fp21_detach_stages` (a bounded observation expired), two `mounted_commit` tests (`mktemp` could not write `/tmp` in the test container) and one `mounted_install` test (no write landed between capture and return in its 2 samples, a timing condition). They look like runner-environment and timing faults, not product defects, but were not diagnosed. Residual risk: a Linux FUSE or in-process Store regression since then would ship undetected, and 1.0.0 Stores opening under 1.1.0 is read from source, not tested |
| Versioned manual | No `docs/versioned/1.1.0/` | Skipped, as for 1.0.0. `api/README.md` and `api/how-to-use/` are the 1.1.0 manual; they are not frozen, so they can drift from 1.1.0 after later changes, and readers of an old version use the `v1.1.0` tag's copy |

## Known limits

Unchanged from 1.0.0: no restart recovery, deferred zombie reaping and the
existing replaced-inode FUSE policy, Disposable only. Open #428 items are in the
[change list](../../../core/docs/issues/428/CHANGES.md#open-items).
