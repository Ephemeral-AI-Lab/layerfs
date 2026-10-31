# #527 platform pass: twelve cells on every platform

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Written 2026-11-01 on branch `claude/issue527-platform-pass-e05aa3`, based on
`layerfs_v2_0_0` at `81f8758bcf`. The terminal acceptance pass of #527 ("Sandbox
platform integration and consolidation"): every platform this Mac can run, through
the twelve #516 cells C01–C12. Nothing is pushed or merged; that waits for the owner.

- **Final commit:** the product is final at `97d408cc51`. This document and the
  receipts are committed after it (docs only).
- **Receipts:** `receipts/<commit>/` beside this file. Each JSON names its row,
  cell, verdict, cause, timings, load, identities, lock and teardown.
- **Owner decisions applied:** D1–D7, all at their recommended option (2026-11-01).
  The four #516 VMs are real `vm:` providers (owner, 2026-11-01).
- **Delegation:** the owner was asleep and the parent session held the delegated
  authority. Reversible choices made during the pass are listed under "Decisions
  taken during the pass".

## Result

All 120 cells PASS on ten platform rows, and both refusals PASS.

| Platform (row) | C01 | C02 | C03 | C04 | C05 | C06 | C07 | C08 | C09 | C10 | C11 | C12 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| macOS, local, docker (macos-docker) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| macOS, local, native (macos-native) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| macOS, local, namespaces | REFUSED (expected) | | | | | | | | | | | |
| Linux, local, docker (linux-docker) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Linux, local, native (linux-native) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Linux, local, namespaces (linux-namespaces) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| vm:apple-container, namespaces | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| vm:microsandbox, namespaces | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| vm:lima (vz), namespaces | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| vm:firecracker (nested in Lima vz; pass/fail only) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| vm:ssh, namespaces | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| vm:*, docker | REFUSED (expected) | | | | | | | | | | | |
| Windows, local, docker (WSL2 worker) | NOT_RUN — no Windows host | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |
| Windows, local, native or namespaces | REFUSED by design (step 8); NOT_RUN on a Windows host | | | | | | | | | | | |

The refusals are one cell each, since a refused platform runs no cell body.

- **macOS namespaces** (`refusals-macos-namespaces`, 5431c7e4a9): PASS. The answer
  was `Unsupported` three times: for `LAYERFS_SANDBOX=namespaces` without the image
  settings, with them, and for the former pair (`native` +
  `LAYERFS_SANDBOX_BACKEND=namespaces`). The message is "… namespaces is Linux only;
  use native".
- **Docker on a vm: home** (`refusals-vm-docker`):
  - At 5431c7e4a9 it was **OBSERVED**, a product finding.
    - Placement `vm:docker` was refused `Unsupported` (no such provider).
    - `LAYERFS_SANDBOX_BACKEND=docker` was refused `InvalidInput`.
    - But `LAYERFS_SANDBOX=docker` on `vm:apple-container` reached the VM's worker
      and ended **`Failed`** at Create/Volume with `Io NotFound` (`attempted: false`,
      nothing made). The VM's Linux has no Docker Engine.
  - Fixed by `e9b9646148`: the agent's worker client refuses a Docker (or unset)
    deployment to a worker whose endpoint names a VM, with `Unsupported`, before
    sending it.
  - Rerun at 97d408cc51: **PASS**, with all three answers as documented.

  Both receipts are kept.

## Receipt identities and why they count at the final commit

| Receipts | Product commit | Linux bundle (manifest sha256) | macOS bundle | VM bundle (identity) | run.mjs |
| --- | --- | --- | --- | --- | --- |
| `receipts/5431c7e4a9/` (120 cells, 2 refusals) | 5431c7e4a9 | build-linux-bd7ac86d… | build-macos-c88772be… (`326dd203…`) | cells-bd7ac86d (`a4cc65f2…`) | `9569db7c` ×97, `e8aa4f90` ×25 |
| `receipts/97d408cc51/` (refusals vm-docker) | 97d408cc51 | build-linux-4ee01388… | — | cells-4ee01388 (`8fd20df6…`) | `e8aa4f90` |

- **Sandbox images.** Docker rows use `sha256:afa1ef9c…` (the index digest, arm64,
  node 24). Namespaces and VM rows use the same image as an uncompressed
  docker-archive named by its config digest `sha256:1d91885d…`, which the namespaces
  backend requires.
- **Why the 5431c7e4a9 receipts count at 97d408cc51** (parent decision, AGENTS.md
  §3.4: reuse evidence a change does not affect). `git diff 5431c7e4a9 97d408cc51`
  touches four files:
  - `api/agent/src/transport/worker_client.js`, the only product file: +5 lines, a
    refusal on the path where a Docker deployment is sent to a vm: worker;
  - its test;
  - one sentence of `api/docs/agent.md`;
  - the runner's vm-docker refusal branch (`run.mjs`, `9569db7c` → `e8aa4f90`).

  No row's cell body sends a Docker deployment to a vm: worker. The tests that
  change touched were run once at 97d408cc51 and all passed:
  - `native_deployment`, `calls` (39) and `exec_output`;
  - all 12 `vm_placement` modes;
  - `runtime.mjs` macos-worker, windows-worker, worker-frames and worker-docker.

  The log is `receipts/97d408cc51/js-checks-97d408cc51.log`.
- **Runner hash in the middle of the pass.** `97d408cc51` was committed while the
  rows were running, and the rows read `run.mjs` from the worktree. So 25 cells
  started after it carry `e8aa4f90`:
  - linux-namespaces C12;
  - vm-microsandbox C08–C12;
  - vm-lima C11–C12;
  - vm-firecracker C08–C12;
  - vm-ssh C01–C12.

  The two runners differ only inside the `refusals vm-docker` branch, which these
  cells never enter. `cell.mjs` (`4d8c3431…`) and `workloads.mjs` (`e22d812d…`) are
  the same in every receipt.

## Rows running at the same time

The final pass ran in five parallel groups:
- macOS rows and the refusals;
- the Linux rows;
- Apple Container then Microsandbox;
- Lima then SSH;
- Firecracker.

The runner's host-wide lock lets only one timed sample run at a time. Setup,
warm-up and verification may overlap other rows. The overlaps below come from the
receipts' `startedAt`/`endedAt`; the runner was not changed mid-pass to record them.
No cell failed, so no contention rerun was needed (parent rule: such a rerun would
be labelled and shown beside the first receipt).

| Row | Cells run (UTC) | run.mjs | Rows running at the same time (any cell) | Cells with no other row |
| --- | --- | --- | --- | ---: |
| macos-docker | 21:36:13–21:37:39 | `9569db7c` ×12 | linux-docker, vm-apple-container, vm-firecracker, vm-lima | 0 |
| macos-native | 21:37:52–21:40:15 | `9569db7c` ×12 | linux-docker, linux-namespaces, linux-native, vm-apple-container, vm-firecracker, vm-lima | 0 |
| linux-docker | 21:36:13–21:38:06 | `9569db7c` ×12 | macos-docker, macos-native, vm-apple-container, vm-firecracker, vm-lima | 0 |
| linux-native | 21:38:14–21:39:42 | `9569db7c` ×12 | macos-native, vm-apple-container, vm-firecracker, vm-lima | 0 |
| linux-namespaces | 21:39:47–21:44:08 | `9569db7c` ×11, `e8aa4f90` ×1 | macos-native, refusals macos-namespaces, refusals vm-docker, vm-apple-container, vm-firecracker, vm-lima, vm-microsandbox | 0 |
| vm-apple-container | 21:36:13–21:41:09 | `9569db7c` ×12 | linux-docker, linux-namespaces, linux-native, macos-docker, macos-native, refusals macos-namespaces, refusals vm-docker, vm-firecracker, vm-lima | 0 |
| vm-microsandbox | 21:41:32–21:45:33 | `9569db7c` ×7, `e8aa4f90` ×5 | linux-namespaces, vm-firecracker, vm-lima | 0 |
| vm-lima | 21:36:13–21:44:58 | `9569db7c` ×10, `e8aa4f90` ×2 | linux-docker, linux-namespaces, linux-native, macos-docker, macos-native, refusals macos-namespaces, refusals vm-docker, vm-apple-container, vm-firecracker, vm-microsandbox | 0 |
| vm-firecracker | 21:34:12–21:48:53 | `9569db7c` ×7, `e8aa4f90` ×5 | linux-docker, linux-namespaces, linux-native, macos-docker, macos-native, refusals macos-namespaces, refusals vm-docker, vm-apple-container, vm-lima, vm-microsandbox, vm-ssh | 1 |
| vm-ssh | 21:45:46–21:50:08 | `e8aa4f90` ×12 | vm-firecracker | 1 |
| refusals macos-namespaces | 21:40:22–21:40:22 | `9569db7c` ×1 | linux-namespaces, vm-apple-container, vm-firecracker, vm-lima | 0 |
| refusals vm-docker | 21:40:26–21:40:26 | `9569db7c` ×1 | linux-namespaces, vm-apple-container, vm-firecracker, vm-lima | 0 |

## Setup and budget per row

Every cell runs in one command of at most 120 s, with a 115 s runner budget.

| Row | Project setup (`driver.initMs`) | Budget left for the cell |
| --- | --- | --- |
| macos-docker | 1.1–8.0 s | 86–94 s |
| macos-native | 0.02–1.7 s | 93–95 s |
| linux-docker / -native / -namespaces | ≤ 0.6 s | 87–90 s |
| vm-apple-container | 6.8–18.9 s | 66–78 s |
| vm-microsandbox | 8.4–14.2 s | 71–76 s |
| vm-lima | 28.7–42.0 s | 43–56 s |
| vm-firecracker | 35.6–48.1 s | 37–49 s |
| vm-ssh | 13.8–22.5 s | 62–71 s |

## The first pass (50e2c2bc87), kept

`receipts/50e2c2bc87/` holds the first pass, run before the gated steps.

| Row | Result |
| --- | --- |
| linux-docker | 12/12 PASS |
| linux-native | 12/12 PASS |
| linux-namespaces | 12/12 PASS, but `teardownOk` false: a namespaces Sandbox reopened through a helper path that no longer existed (fixed by `44d41efa33`) |
| macos-docker | 12/12 PASS |
| macos-native | 12/12 PASS |
| vm-microsandbox | 12/12 PASS |
| vm-ssh | 12/12 PASS |
| vm-apple-container | 6 PASS, 6 FAIL in setup (below) |
| refusal macos-namespaces | PASS; it found the ordering fixed by `cb7dc814ea` |

The six vm-apple-container failures were all in setup:
- **The cause:** project setup took 67–78 s, because `container copy` moved the
  2.6 GB bundle at about 35 MB/s.
- **C03, C07, C08, C12:** stopped at the runner's 15 s minimum budget.
- **C01, C02:** killed by the 120 s bound with no receipt, because setup was not
  bounded by the runner's deadline. That runner flaw is fixed in `a1bf3fd4b6`.
- **The fix,** `915522d51b`: the bundle is staged through the home bind mount, and
  the guest copies it onto its own disk. Setup dropped to 6.8–18.9 s.

`receipts/ac-fix-try/` is the one diagnostic proof cell for that fix. It ran on a
derived bundle, not a sealed one.

## §10 checks

| Check | Outcome |
| --- | --- |
| `vm_real.mjs` Apple Container and Microsandbox | PASS at the base bundle with the h39 busybox image: Apple Container 30.6 s (attempt 1 failed on the test image's missing bash; the image was then rebuilt with a static bash), Microsandbox 8.2 s. Both rows also pass 12/12 at the final pass |
| `vm_real.mjs` Lima vz and Firecracker (new) | PASS on the providers branch with the h39 bundle and `34d49eb577`: Lima 65.2 s, Firecracker 87.3 s |
| Real `vm:ssh` run | vm-ssh 12/12 PASS (the target is the Lima VM `lfs527-ssh`) |
| `namespaces_userns`, `namespaces_lifecycle` as root | 1/1 and 5/5 in the Lima VM at `44d41efa33`, and again at the ends of steps 9b and 9c (`2d6770fb47`); `layerfs-sandbox` is unchanged since then |
| Linux native end to end as root | linux-native 12/12 PASS |
| Bare macOS SDK with a Docker deployment | `kernel_guard` admits the APFS Store (by design, D5); Init, fork and write succeed; Docker `sandbox.create` fails at Listener ("daemon I/O: No such file or directory") and leaves nothing behind. **Open finding:** `kernel_guard` checks the OS, not the deployment, so this combination reaches Docker and then fails. Recommendation: refuse a Docker deployment in-process on macOS before Init. Not done here |
| macOS worker `releaseStore` | Was a no-op; fixed by `c8f5d8378b`. `macos_real.mjs` at 97d408cc51: PASS in 2.7 s. `project.delete` with the worker holding the Store removed the volume with no error and left nothing behind |
| Windows build (D6) | `layerfs-sandbox` passes Clippy for `x86_64-pc-windows-msvc`, and a build-only `windows-build` CI lane that reports and never gates was added. `layerfs-node` and `layerfs-api` do not compile for Windows: `layerfs-store-service` has 14 Unix-only errors (permission modes, `MetadataExt::uid`, `nix::sys::statvfs`, `STORE_HEADROOM_BYTES`), not gated in this pass. No Windows host: Windows rows NOT_RUN |

Every helper also ran its own checks before committing. Each commit's message lists
them: Clippy `-D warnings` on macOS and Linux, the product boundary guard, Linux
tests as root, and every `runtime.mjs` and `vm_placement.mjs` mode.

## Decisions taken during the pass

All of these are reversible. The owner's own decisions are D1–D7 and the
four-VM requirement.

1. **Cell protocol details:**
   - each cell forks branch `lane` from `main`;
   - the warm-up run is unmounted with discard, with no Commit;
   - each cell uses one Sandbox, made by `sandbox.create`.
2. **Linux rows run as root in a Lima QEMU VM** (`lfs527-linux`, Ubuntu 24.04,
   kernel 6.8.0-147, 4 vCPUs, 4 GiB), because this Mac has no Linux host. The runner
   relays into it. It is QEMU because vz was blocked when the VM was made (Lima
   1.0.4). The `vm:ssh` target `lfs527-ssh` is the same kind of VM.
3. **Namespaces and VM rows use the config-digest archive** `1d91885d…`. The
   namespaces backend refuses gzip layers and index-digest names.
4. **Release bundles, sealed by `tools/worktree.py build`,** for every row. The VM
   bundle is a copy carrying `images/<config digest>.tar`.
5. **`brew upgrade lima` from 1.0.4 to 2.2.1**, at the user's request to solve the
   Lima issue.
   - Cause: Lima 1.0.4's gvisor-tap-vsock v0.8.2 watches `/private/var/run` and
     fails on Docker Desktop's `docker.sock` symlink (the same cause #516 found).
   - colima had no instance. The old keg was removed after the QEMU VMs were
     deleted.
6. **Apple Container: stage through the home bind mount** rather than bind-mount
   the bundle at VM create. Bind-mounting at create would make a VM unable to
   restart once the bundle folder is gone.
7. **Firecracker** uses #516's recipe, all downloads checksum-verified:
   - Firecracker v1.17.0, CI kernel 6.18.56, a CI Ubuntu 24.04 rootfs with Node
     v22.23.4;
   - run nested in Lima vz `lfs527-vzkvm` (4 vCPUs, 4 GiB, `nestedVirtualization`,
     1,100 huge pages);
   - each microVM gets 2G and no virtual PMU.
   The two-hop bundle stream fit (36–48 s), so the parent's fallback (stage the
   bundle in the Lima host) was not needed.
8. **Lima Store disks are named `lfs` + 8 hex characters.** A longer name was cut
   to ext4's 16-byte label, so Lima reformatted the disk on every boot.
9. **The final pass ran in five parallel groups**, with the host lock serializing
   timed samples. Overlaps are derived from the receipt timestamps.
10. **The runner changed mid-pass** (`97d408cc51`, vm-docker branch only). This is
    disclosed above rather than rerunning the 25 cells.
11. **The 5431c7e4a9 receipts are kept for the final commit** (parent decision); the
    diff scope is above.
12. **Step 10b behaviour change, accepted by the parent:** Init refuses `Exists`
    when its Store volume already exists, instead of reusing it.
13. **Step 12 discovery** uses a new backend-agnostic Rust call,
    `runtime.sandbox.records` (parent-approved). A record of another backend is
    listed, never reopened.
14. **Windows `releaseStore`** returns quietly when no worker ever started.
15. **The agent refuses a backend by where its Sandbox runs** (`34d49eb577`): Linux
    for a worker, this OS otherwise. Found by the vm:lima real proof:
    `cb7dc814ea` had refused every vm: row run from macOS.
16. **Docker on a vm: home is refused `Unsupported`** before it is sent
    (`e9b9646148`).
17. **Step 7 departures** (Rust helper):
    - only `local.rs`'s other-platform branch was removed; those in `sandbox.rs` and
      `discovery.rs` stay live on other targets;
    - the JSON parser is `json.rs` beside `json/`, because the guard forbids code in
      `mod.rs`;
    - `ContainerId::random` is built on Linux and macOS only.

## Findings not fixed in this pass

- **`kernel_guard` on macOS checks the OS, not the deployment** (§10 row above).
- **`layerfs-store-service` does not compile for Windows** (14 errors), so
  `layerfs-node` and `layerfs-api` cannot be gated for Windows yet.
- **`discovery.rs` is at 997 lines**, just under the 999-line ceiling.
- **`lima.js` copies the bundle through the home mount** in the same way as
  `apple-container.js`, as duplicated code. The VM providers' `df -kP` parsing
  could use `shared.js` `dfFree`.
- **`76-native-mount-session.md`'s revocation paragraph is stale.**
- **A comment in `api/agent/src/index.js` is wrong:** it says
  `LAYERFS_SANDBOX_BACKEND` is read per call, but it is read when the client is
  made.
- **`vm:ssh` leaves `/layerfs/{home,store}` and a bind mount on its host.** The
  runner removes them; the provider does not.
- **Removed tests:** `native_platform::windows_is_refused_before_any_effect`,
  deleted with `Runtime::Windows` (step 7).

## Commits

Production LOC uses `tools/production_loc.py` on the first parent and the staged
tree. The scope is core, engine and api; the reference is absent (retired R9c; last
counted 65,417 at tree 498dd1917812). Every commit message carries its own line.
Over the branch: **123141 → 123811 (delta +670)**. Of that, +402 is the two new VM
providers and +143 is step 12's discovery call.

| Commit | Subject | Production LOC |
| --- | --- | --- |
| `133145ffe2` | node: one Sandbox name for every backend (#527 step 5) | 123141 -> 123133 (delta -8) |
| `527c32f626` | sandbox: the provider alone chooses FUSE or NFS (#527 step 6) | 123133 -> 123145 (delta +12) |
| `c6f5dd227d` | sandbox: provider/ into types/, LaunchFailure into the runtime (#527 step 9a) | 123145 -> 123137 (delta -8) |
| `27d1784df3` | runtime: vm: homes refuse Store-level calls before any effect (#527 step 2) | 123137 -> 123155 (delta +18) |
| `18b0810804` | runtime: both deployments name the Sandbox backend (#527 step 3) | 123155 -> 123158 (delta +3) |
| `4083e75590` | runtime, agent: refuse VM providers and native backends on hosts they do not run on (#527 step 8) | 123158 -> 123165 (delta +7) |
| `7080ac9160` | runtime: platform capabilities replace platform.name branches in Init (#527 step 10d) | 123165 -> 123167 (delta +2) |
| `9952d16451` | docs: fix the platform documentation drift of F7 (#527 step 1) | 123167 -> 123167 (delta 0) |
| `24c3c5f828` | sandbox: unpack a Debian image's root entry and long link targets | 123167 -> 123188 (delta +21) |
| `781e1c0236` | runtime: vm:ssh copies carry no AppleDouble files | 123188 -> 123188 (delta 0) |
| `38ad8ab4c2` | node: reopen requires the deployment's own backend (#527 step 4) | 123188 -> 123194 (delta +6) |
| `50e2c2bc87` | test: the twelve-cell runner for the #527 platform pass | 123194 -> 123194 (delta 0) |
| `cb7dc814ea` | agent: refuse namespaces off Linux before reading its image settings | 123194 -> 123197 (delta +3) |
| `c8f5d8378b` | runtime: the macOS worker releases a Store before its volume goes | 123197 -> 123206 (delta +9) |
| `44d41efa33` | sandbox: a namespaces Sandbox reopens with its own helper copy | 123206 -> 123217 (delta +11) |
| `ab00731d51` | sandbox: drop Runtime::Windows, build layerfs-sandbox for Windows (#527 step 7) | 123217 -> 123204 (delta -13) |
| `ad7cb42ee9` | sandbox: backend/ into platform/, json/ and root files (#527 step 9b) | 123204 -> 123207 (delta +3) |
| `2d6770fb47` | sandbox: Docker's files by role, docker/client/, os, ExecExit (#527 step 9c) | 123207 -> 123182 (delta -25) |
| `34d49eb577` | agent: refuse a backend by where its Sandbox runs, not by this OS | 123182 -> 123184 (delta +2) |
| `9b307f35cb` | runtime: the local placement's Docker group under platform/docker/ (#527 step 10a) | 123184 -> 123205 (delta +21) |
| `3f28127edc` | runtime: Store hooks keyed by Store; createStore makes every Store volume (#527 step 10b) | 123205 -> 123209 (delta +4) |
| `9a241c04f2` | runtime: F11's duplicates land once (#527 step 10c) | 123209 -> 123216 (delta +7) |
| `72bd04ccfb` | runtime: sweep finds ended Sandboxes by their labels, not their names (#527 step 12, sweep only) | 123216 -> 123214 (delta -2) |
| `915522d51b` | runtime: Apple Container copies a folder through the home bind mount (#527 platform pass) | 123214 -> 123233 (delta +19) |
| `89226d75e3` | node, agent, runtime: one Sandbox setting, LAYERFS_SANDBOX=docker\|native\|namespaces (#527 step 11) | 123233 -> 123262 (delta +29) |
| `d82d6a3d58` | node, runtime: every backend's Sandbox records listed, and local discovery reads them (#527 step 12, discovery) | 123262 -> 123405 (delta +143) |
| `a1f91f0a9f` | runtime: Windows releases a Store quietly when no worker holds it | 123405 -> 123406 (delta +1) |
| `a1bf3fd4b6` | test: a cell's setup is bounded by the runner's deadline | 123406 -> 123406 (delta 0) |
| `09bf9d1bb0` | docs: AGENTS.md names native macOS as a direct Store exception (#527 D5) | 123406 -> 123406 (delta 0) |
| `d610a82ac1` | test: the cell runner's vm-lima and vm-firecracker rows | 123406 -> 123406 (delta 0) |
| `b39550bd5c` | runtime: vm:lima placement (#527 platform pass) | 123406 -> 123560 (delta +154) |
| `a8cb674f03` | runtime: vm:firecracker placement (#527 platform pass) | 123560 -> 123808 (delta +248) |
| `5431c7e4a9` | test: the cell runner names namespaces with the one Sandbox setting | 123808 -> 123808 (delta 0) |
| `e9b9646148` | agent: a vm: home refuses a Docker deployment before sending it | 123808 -> 123811 (delta +3) |
| `97d408cc51` | test: the vm-docker refusal cell expects the documented answers | 123811 -> 123811 (delta 0) |

## Cleanup

Removed:
- **VMs:** every per-cell VM, microVM, Sandbox, container and volume (each cell's
  teardown, checked by every row helper), and the pass's Lima VMs `lfs527-linux`,
  `lfs527-ssh` and `lfs527-vzkvm` with their disks.
- **Lima:** Lima's download cache (`limactl prune`) and the old Lima 1.0.4 keg.
- **Pulled images:** `node:22-trixie-slim` from Apple Container and Microsandbox.
- **Apple Container:** its service is stopped (the pass had started it).
- **Caches under `~/.cache/layerfs-test`:** the bundle copies, the image archives
  (2.7 GB + 0.9 GB + the 6 MB h39 image), the runner copies, the SSH test keys, and
  the scratch folders `lfs527-tests-2` and `lfs527-vmreal`.
- **Receipt folders:** the originals (copied here first).
- **Helper worktrees and branches:** `claude/issue527-{js-steps,js-gated,vm-providers,apple-container,lead-side}`,
  whose commits are all on this branch.

Kept:
- **This worktree**, its sealed bundles (166 MB) and its private Cargo target
  volumes, warm for the owner's review. Export a cleanup scope for them
  (`tools/worktree.py cleanup-scope --release-caches`) once the branch is merged or
  dropped.
- **`~/.lima`** (Lima's SSH key and two empty folders), recreated by this pass.
  Removing it was refused by this session's permission check; delete it by hand if
  it is not wanted.
- **The Docker build cache** of two failed slim-image builds. It is shared, so it
  was not pruned.

Host free space after cleanup: 225.7 GB. Docker guest: 41 GB free.

## Appendix: per-row identities and timings

Timings are diagnostic: one sample per cell, never rerun for speed. vm-firecracker
timings describe the nesting (two hypervisors) and are not comparable with other
rows. "lock wait" is the time a cell waited for the host-wide sampling lock.

### linux-docker

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max max 100000; image sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 0.0–10.8; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 813.5 | 867.1 | 16.4 | 189.6 (Committed) | 10.6 | 304.0 |
| C02 | PASS | 1807.0 | 1880.8 | 7.8 | 120.4 (Committed) | 5.7 | 0.0 |
| C03 | PASS | 1330.3 | 1362.7 | 6.4 | 38.3 (UpToDate) | 8.8 | 0.0 |
| C04 | PASS | 2224.1 | 2318.8 | 23.5 | 273.2 (Committed) | 30.7 | 0.0 |
| C05 | PASS | 1395.2 | 1421.0 | 10.9 | 138.6 (Committed) | 4.2 | 915.0 |
| C06 | PASS | 224.0 | 249.1 | 4.0 | 650.7 (Committed) | 14.2 | 0.0 |
| C07 | PASS | 608.9 | 1078.0 | 7.1 | 719.3 (Committed) | 20.8 | 1.0 |
| C08 | PASS | 223.4 | 249.3 | 4.5 | 332.3 (Committed) | 10.2 | 0.0 |
| C09 | PASS | 67.9 | 90.0 | 10.6 | 1.4 (UpToDate) | 4.6 | 0.0 |
| C10 | PASS | 349.5 | 375.1 | 9.2 | 61.4 (Committed) | 16.7 | 609.0 |
| C11 | PASS | 176.1 | 198.7 | 8.4 | 163.2 (UpToDate) | 12.5 | 0.0 |
| C12 | PASS | 486.7 | 520.0 | 7.3 | 74.4 (Committed) | 4.3 | 1.0 |

### linux-native

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; image None; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 0.4–9.3; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 658.1 | 661.6 | 5.2 | 101.7 (Committed) | 4.1 | 3437.0 |
| C02 | PASS | 1163.1 | 1167.0 | 4.6 | 100.2 (Committed) | 2.8 | 0.0 |
| C03 | PASS | 1089.3 | 1093.9 | 6.4 | 32.5 (UpToDate) | 4.4 | 0.0 |
| C04 | PASS | 1401.7 | 1408.8 | 10.5 | 132.8 (Committed) | 4.8 | 3094.0 |
| C05 | PASS | 933.1 | 936.0 | 3.6 | 109.0 (Committed) | 3.0 | 0.0 |
| C06 | PASS | 221.1 | 225.0 | 3.5 | 323.7 (Committed) | 12.5 | 508.0 |
| C07 | PASS | 592.2 | 595.2 | 3.8 | 753.6 (Committed) | 22.8 | 0.0 |
| C08 | PASS | 254.9 | 258.9 | 3.8 | 327.7 (Committed) | 14.0 | 0.0 |
| C09 | PASS | 113.3 | 117.0 | 7.4 | 0.7 (UpToDate) | 4.7 | 0.0 |
| C10 | PASS | 604.9 | 611.1 | 9.1 | 63.5 (Committed) | 18.4 | 1.0 |
| C11 | PASS | 367.0 | 372.5 | 8.9 | 157.8 (UpToDate) | 12.8 | 2.0 |
| C12 | PASS | 347.2 | 350.6 | 5.1 | 46.3 (Committed) | 3.0 | 0.0 |

### linux-namespaces

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 0.8–12.3; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 548.2 | 567.4 | 3.8 | 179.8 (Committed) | 3.8 | 4.0 |
| C02 | PASS | 2010.0 | 2042.0 | 46.6 | 214.9 (Committed) | 5.8 | 2.0 |
| C03 | PASS | 828.4 | 847.4 | 4.0 | 14.6 (UpToDate) | 2.3 | 2.0 |
| C04 | PASS | 1286.6 | 1301.0 | 3.9 | 114.3 (Committed) | 2.5 | 0.0 |
| C05 | PASS | 1270.4 | 1288.7 | 3.9 | 107.7 (Committed) | 2.9 | 3.0 |
| C06 | PASS | 203.9 | 225.5 | 3.1 | 327.9 (Committed) | 11.2 | 1.0 |
| C07 | PASS | 501.6 | 520.3 | 3.3 | 635.9 (Committed) | 18.5 | 1.0 |
| C08 | PASS | 234.2 | 247.1 | 4.0 | 344.9 (Committed) | 9.5 | 0.0 |
| C09 | PASS | 135.0 | 151.6 | 13.1 | 0.8 (UpToDate) | 7.0 | 2.0 |
| C10 | PASS | 300.7 | 321.2 | 7.7 | 51.1 (Committed) | 12.6 | 1.0 |
| C11 | PASS | 129.1 | 145.5 | 6.6 | 145.9 (UpToDate) | 10.3 | 0.0 |
| C12 | PASS | 245.9 | 265.2 | 3.9 | 43.5 (Committed) | 2.1 | 1.0 |

### macos-docker

Where: Linux 6.12.76-linuxkit aarch64, 8 vCPUs, 7.8 GiB, cpu.max max 100000; image sha256:afa1ef9c27294f9c7298820c06483e6fb1ddafc43a7cc7d6dec8cd2e67abf38e; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 3.8–11.4; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 415.7 | 460.4 | 10.2 | 198.3 (Committed) | 10.9 | 1.0 |
| C02 | PASS | 1971.8 | 2074.6 | 33.1 | 125.7 (Committed) | 10.5 | 1.0 |
| C03 | PASS | 473.3 | 519.8 | 12.9 | 18.6 (UpToDate) | 8.9 | 2.0 |
| C04 | PASS | 1014.0 | 1076.4 | 13.2 | 119.7 (Committed) | 8.7 | 0.0 |
| C05 | PASS | 868.6 | 908.8 | 11.5 | 111.8 (Committed) | 7.2 | 0.0 |
| C06 | PASS | 175.3 | 211.0 | 9.6 | 330.4 (Committed) | 13.3 | 0.0 |
| C07 | PASS | 451.7 | 492.2 | 14.7 | 668.1 (Committed) | 21.8 | 1.0 |
| C08 | PASS | 222.3 | 271.8 | 12.5 | 347.8 (Committed) | 15.5 | 0.0 |
| C09 | PASS | 39.5 | 76.3 | 12.1 | 1.9 (UpToDate) | 7.9 | 0.0 |
| C10 | PASS | 212.1 | 250.7 | 12.8 | 55.5 (Committed) | 14.2 | 0.0 |
| C11 | PASS | 174.8 | 240.5 | 161.2 | 160.2 (UpToDate) | 13.9 | 28.0 |
| C12 | PASS | 243.9 | 297.0 | 16.4 | 72.7 (Committed) | 10.7 | 0.0 |

### macos-native

Where: Darwin 25.4.0 arm64, 14 vCPUs, 36.0 GiB, cpu.max None; image None; source 5431c7e4a9; bundle build-macos-c88772be7b1949f1af6289022adb1718; 1-min load 6.8–9.4; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 941.6 | 967.6 | 28.8 | 890.4 (Committed) | 28.9 | 0.0 |
| C02 | PASS | 7892.6 | 7913.1 | 29.8 | 283.9 (Committed) | 33.2 | 1.0 |
| C03 | PASS | 1422.8 | 1437.7 | 23.3 | 146.6 (UpToDate) | 20.6 | 0.0 |
| C04 | PASS | 7387.8 | 7492.2 | 28.9 | 939.8 (Committed) | 39.9 | 1.0 |
| C05 | PASS | 4540.2 | 4555.0 | 22.4 | 472.9 (Committed) | 36.0 | 0.0 |
| C06 | PASS | 792.6 | 838.6 | 24.8 | 664.1 (Committed) | 27.6 | 0.0 |
| C07 | PASS | 565.7 | 589.7 | 28.5 | 1128.2 (Committed) | 39.5 | 0.0 |
| C08 | PASS | 424.1 | 470.0 | 46.9 | 1222.5 (Committed) | 37.3 | 0.0 |
| C09 | PASS | 42.0 | 58.0 | 62.5 | 90.0 (UpToDate) | 21.6 | 0.0 |
| C10 | PASS | 270.1 | 289.8 | 24.8 | 448.4 (Committed) | 49.4 | 0.0 |
| C11 | PASS | 213.6 | 230.4 | 21.7 | 565.7 (UpToDate) | 32.7 | 0.0 |
| C12 | PASS | 594.5 | 614.9 | 49.9 | 838.5 (Committed) | 27.2 | 1.0 |

### vm-apple-container

Where: Linux 6.18.35 aarch64, 5 vCPUs, 4.0 GiB, cpu.max None; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 3.6–10.8; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 528.8 | 552.7 | 8.1 | 171.7 (Committed) | 9.0 | 207.0 |
| C02 | PASS | 915.4 | 936.8 | 10.1 | 110.1 (Committed) | 10.4 | 306.0 |
| C03 | PASS | 618.6 | 634.7 | 10.5 | 10.1 (UpToDate) | 8.7 | 305.0 |
| C04 | PASS | 648.4 | 669.0 | 5.6 | 142.6 (Committed) | 6.3 | 1.0 |
| C05 | PASS | 875.5 | 887.3 | 11.6 | 107.3 (Committed) | 7.6 | 8794.0 |
| C06 | PASS | 142.7 | 166.8 | 11.8 | 302.1 (Committed) | 10.0 | 916.0 |
| C07 | PASS | 294.3 | 315.5 | 3.7 | 600.0 (Committed) | 15.3 | 0.0 |
| C08 | PASS | 187.5 | 205.6 | 5.9 | 308.1 (Committed) | 113.6 | 1.0 |
| C09 | PASS | 52.9 | 73.0 | 6.6 | 1.5 (UpToDate) | 10.8 | 1.0 |
| C10 | PASS | 225.9 | 245.9 | 6.8 | 51.4 (Committed) | 12.4 | 0.0 |
| C11 | PASS | 151.0 | 168.8 | 13.9 | 162.5 (UpToDate) | 17.3 | 2.0 |
| C12 | PASS | 165.2 | 193.4 | 6.3 | 50.8 (Committed) | 6.0 | 1.0 |

### vm-microsandbox

Where: Linux 6.12.113 aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 8.5–12.6; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 381.6 | 409.7 | 8.1 | 106.2 (Committed) | 8.9 | 26.0 |
| C02 | PASS | 968.7 | 1030.4 | 5.6 | 95.6 (Committed) | 11.5 | 1.0 |
| C03 | PASS | 530.1 | 573.4 | 6.9 | 9.4 (UpToDate) | 6.3 | 102.0 |
| C04 | PASS | 589.9 | 634.3 | 5.2 | 437.9 (Committed) | 110.8 | 0.0 |
| C05 | PASS | 842.1 | 893.1 | 6.4 | 104.0 (Committed) | 8.2 | 0.0 |
| C06 | PASS | 171.1 | 233.1 | 5.8 | 304.9 (Committed) | 11.9 | 1.0 |
| C07 | PASS | 291.5 | 340.0 | 5.4 | 586.4 (Committed) | 16.2 | 0.0 |
| C08 | PASS | 133.8 | 189.4 | 5.5 | 297.7 (Committed) | 21.6 | 1.0 |
| C09 | PASS | 84.4 | 160.3 | 11.0 | 2.9 (UpToDate) | 142.0 | 3.0 |
| C10 | PASS | 180.5 | 234.9 | 7.7 | 45.9 (Committed) | 12.9 | 0.0 |
| C11 | PASS | 104.7 | 163.3 | 8.0 | 139.8 (UpToDate) | 11.3 | 1.0 |
| C12 | PASS | 128.2 | 167.4 | 4.5 | 42.6 (Committed) | 7.7 | 1.0 |

### vm-lima

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 3.8–12.5; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 272.7 | 287.7 | 10.0 | 100.1 (Committed) | 17.3 | 1.0 |
| C02 | PASS | 1066.5 | 1078.1 | 6.4 | 101.0 (Committed) | 7.4 | 1.0 |
| C03 | PASS | 656.0 | 670.1 | 9.5 | 24.1 (UpToDate) | 7.2 | 1522.0 |
| C04 | PASS | 1270.3 | 1292.6 | 11.2 | 760.5 (Committed) | 24.5 | 3.0 |
| C05 | PASS | 764.6 | 779.5 | 4.8 | 104.8 (Committed) | 6.5 | 1.0 |
| C06 | PASS | 200.8 | 332.6 | 11.0 | 479.0 (Committed) | 16.8 | 0.0 |
| C07 | PASS | 420.7 | 433.8 | 5.7 | 674.2 (Committed) | 23.6 | 2.0 |
| C08 | PASS | 176.7 | 191.2 | 6.0 | 323.1 (Committed) | 15.9 | 2.0 |
| C09 | PASS | 75.0 | 91.5 | 36.2 | 1.6 (UpToDate) | 10.3 | 0.0 |
| C10 | PASS | 256.5 | 271.8 | 11.5 | 57.4 (Committed) | 20.8 | 13.0 |
| C11 | PASS | 164.6 | 185.3 | 11.0 | 153.5 (UpToDate) | 16.8 | 6.0 |
| C12 | PASS | 164.1 | 188.4 | 4.5 | 45.6 (Committed) | 7.1 | 0.0 |

### vm-firecracker

Where: Linux 6.18.56+ aarch64, 4 vCPUs, 1.9 GiB, cpu.max None; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 3.6–11.3; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 596.0 | 643.6 | 8.3 | 120.2 (Committed) | 7.3 | 0.0 |
| C02 | PASS | 2047.6 | 2083.8 | 20.5 | 124.0 (Committed) | 8.5 | 1120.0 |
| C03 | PASS | 1173.5 | 1205.9 | 11.2 | 17.2 (UpToDate) | 4.6 | 854.0 |
| C04 | PASS | 1728.7 | 1772.1 | 10.9 | 123.1 (Committed) | 5.5 | 0.0 |
| C05 | PASS | 2159.3 | 2247.2 | 10.1 | 125.1 (Committed) | 8.1 | 3.0 |
| C06 | PASS | 359.5 | 441.9 | 24.1 | 514.8 (Committed) | 26.0 | 0.0 |
| C07 | PASS | 679.6 | 770.4 | 14.2 | 784.6 (Committed) | 34.1 | 0.0 |
| C08 | PASS | 573.6 | 896.5 | 53.2 | 523.4 (Committed) | 31.3 | 9.0 |
| C09 | PASS | 113.2 | 152.1 | 17.0 | 3.2 (UpToDate) | 7.0 | 0.0 |
| C10 | PASS | 447.6 | 518.4 | 17.5 | 100.1 (Committed) | 14.7 | 0.0 |
| C11 | PASS | 236.5 | 272.0 | 20.8 | 150.8 (UpToDate) | 10.7 | 1.0 |
| C12 | PASS | 487.8 | 530.9 | 8.5 | 60.3 (Committed) | 5.5 | 0.0 |

### vm-ssh

Where: Linux 6.8.0-147-generic aarch64, 4 vCPUs, 3.8 GiB, cpu.max None; image sha256:1d91885de27a20805af0d1b99602ace066f0514983613e391ee1f73ac8e7c2e9; source 5431c7e4a9; bundle build-linux-bd7ac86d01304d4e89a2e98057d26d66; 1-min load 5.5–9.5; teardown clean in all: True.

| Cell | Verdict | command ms | exec wall ms | mount ms | Commit ms (kind) | unmount ms | lock wait ms |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: |
| C01 | PASS | 438.9 | 461.3 | 4.0 | 101.2 (Committed) | 4.1 | 1.0 |
| C02 | PASS | 1003.0 | 1017.9 | 5.9 | 97.7 (Committed) | 3.9 | 0.0 |
| C03 | PASS | 961.0 | 977.8 | 3.9 | 8.4 (UpToDate) | 3.3 | 1.0 |
| C04 | PASS | 899.2 | 919.4 | 3.8 | 108.4 (Committed) | 3.4 | 0.0 |
| C05 | PASS | 878.7 | 891.8 | 4.8 | 104.0 (Committed) | 3.7 | 1.0 |
| C06 | PASS | 190.4 | 209.6 | 4.1 | 314.6 (Committed) | 10.4 | 0.0 |
| C07 | PASS | 396.4 | 412.4 | 4.6 | 604.9 (Committed) | 17.5 | 1.0 |
| C08 | PASS | 165.5 | 177.1 | 3.7 | 306.5 (Committed) | 10.1 | 0.0 |
| C09 | PASS | 80.0 | 99.8 | 7.2 | 1.8 (UpToDate) | 6.0 | 0.0 |
| C10 | PASS | 216.8 | 239.9 | 7.0 | 51.0 (Committed) | 13.4 | 0.0 |
| C11 | PASS | 102.2 | 125.7 | 6.8 | 137.9 (UpToDate) | 10.5 | 1.0 |
| C12 | PASS | 236.0 | 253.3 | 3.9 | 45.6 (Committed) | 3.0 | 1.0 |

