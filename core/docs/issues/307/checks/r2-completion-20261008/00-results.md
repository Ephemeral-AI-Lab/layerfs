# R2 completion checks

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Functional evidence for the R2 native read path, mount/Ready and normal unmount.
Every run used natural caches. Nothing here is a cold, latency, storage or
resident-memory claim. The global Store was Disposable/WAL/synchronous=OFF in
every run, selected explicitly; Durable is `NOT_RUN — disabled by owner until
explicit reauthorization`. `LAYERFS_CONSTRUCTION_WORKERS=1` was exported.

## Environment

- Linux runs: Docker image
  `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`,
  kernel `6.12.76-linuxkit`, locked Cargo 1.85.1, FUSE ABI 7.41 as negotiated.
- Native test containers: `--device /dev/fuse --cap-add SYS_ADMIN
  --security-opt apparmor=unconfined`, `Privileged=false`.
- Store files were on the container's native filesystem or a named in-VM volume,
  never under the repository bind mount.
- One command at a time under a worktree-local nonblocking lock. Each test binary
  had its own 100 s wall limit; none reached it.
- Product source identity and binary hashes are in
  [source-identity.json](source-identity.json).

## Full topology

Host Project Init and seal on macOS, one Sandbox-created container running the
actual `layerfs-daemon` binary, install into a named VM volume, SDK
`mount` (bind then attach) over the authenticated control channel, and ordinary
commands launched through the Sandbox as uid 501 with no daemon registration.

| Attempt | Limit | Result | Receipt |
| --- | --- | --- | --- |
| 1 | 9 s | **FAILED**: wall limit expired after the last proof line and before container deletion | [output](attempt1-full-topology.txt), daemon [stdout](attempt1-daemon-stdout.txt)/[stderr](attempt1-daemon-stderr.txt) |
| 2 | 9 s | **FAILED**: same expiry, rerun once with elapsed marks to locate the time | [output](attempt2-full-topology.txt), daemon [stdout](attempt2-daemon-stdout.txt)/[stderr](attempt2-daemon-stderr.txt) |
| 3 | 9 s | PASS in 4.6 s with the sustained command reduced from 40 to 10 rounds | [output](attempt3-full-topology.txt) |
| 4 | 15 s | PASS in 8.0 s with the original 40-round workload | [output](attempt4-full-topology.txt) |

Attempts 1 and 2 are failures and stay failures. The marks in attempt 2 show the
40-round sustained command took 3.6 s and Sandbox stop/delete about 2.3 s; every
proof line had already been produced. Attempt 3 changed the workload, so it is a
different case and not a repair of attempt 2. The owner then allowed a somewhat
larger limit, and attempt 4 reran the original workload under a declared 15 s
limit. Attempt 4 is the covering receipt. These walls are diagnostic, not a
performance result.

After attempt 4 the only product source change was a two-line header comment in
`layerfs-daemon/src/lib.rs`. The daemon rebuilt at the final source is
byte-identical to the binary attempt 4 ran (same SHA-256, recorded in the
identity file), so the proof was not repeated.

Attempt 4 observed:

- Ready: `/workspaces/1`, mount id 124, device 0:80, ABI 7.41, max write and
  readahead 131072, background and congestion 1, page size 4096, two loops.
- Mount table: `rw,nosuid,nodev,noatime - fuse layerfs
  rw,user_id=0,group_id=0,default_permissions,allow_other,max_read=131072`.
- `/dev/fuse` mode 600 owned 0:0.
- Command identity uid 501 gid 20, `CapEff=0`, `NoNewPrivs=1`; private backing,
  `/proc` aliases and the FUSE device denied; mutation refused.
- Complete root against the independent host manifest: 10 paths, 400106 regular
  bytes, hard link, symlink, ignored files and dependencies.
- Unmount while a command held a working directory: `Busy` at `unmount:kernel`,
  Workspace still Ready, command untouched.
- Terminal unmount: directory removed, mount-table entry absent, routing Missing.
- Three further fresh mounts, each Ready, read and unmounted.
- Owned container stopped and deleted.

`abort_bound: false` in every Ready record: fusectl is not mounted in the
container, so no abort control was bound. `forget_units: 0` at unmount: the
kernel delivered no FORGET before the detach aborted the connection, so lookup
ownership was retired by revocation maintenance rather than by FORGET.

The [R1 lifecycle regression](regression-sandbox-lifecycle.txt) passes with the
new Sandbox container configuration (two daemons, shared Store, reopen).

## Real-mount component tests

In-process daemon Service, real Overlay Owner and Store, real kernel mount.

| Binary | Result | Covers |
| --- | --- | --- |
| [native_mount](final-linux-native_mount.txt) | 3 PASS | Ready evidence and receipt; complete root with exact mode, mtime, ownership, listings, bytes, link targets and hard-link identity; declared refusals and inline answers; external nonroot `ls`/`cat`, mode enforcement for another uid; Busy for an open descriptor, an `O_PATH` descriptor and a running command's working directory, each returning to Ready on the same connection; terminal unmount with joined loops and bounded retirement; Attach refusals without effect; two Workspaces on one dispatcher unmounting independently |
| [native_mount_routes](final-linux-native_mount_routes.txt) | 2 PASS | Lost Mount and Attach acknowledgements observed by Locate with no replay and no second connection; SDK two-acknowledgement mount and failed second acknowledgement; eight sequential fresh mounts with identical inode, size, times, mode, ownership and link count for every entry; two live mounts with equal identity and different `st_dev`; 300 uncached `O_DIRECT` rounds from two callers on one connection with no retained, terminal or unadmitted unit |

The identity assertions were added after the first suite run. Their
[first pass with counters](routes-identity-first-pass.txt) recorded 3885
completed requests for the sustained rounds; the table links the final-source run.

## Linux suite

All 65 test binaries of the six changed packages were built with `--no-run` and
run one at a time, twice: once before the test repairs below
([build](linux-test-build.txt), [results](linux-suite-first-run.txt): 61 exit 0,
4 exit 101) and once at the final source
([build](final-linux-test-build.txt), [results](final-linux-suite.txt): 63 exit 0,
2 exit 101). The second run is a new source identity, not a repeat to obtain a
pass. No binary reached its 100 s limit.

| Binary | First run | Disposition |
| --- | --- | --- |
| admission_future | [FAILED 6/7](linux-admission_future-original-failure.txt) | The test polled before the publisher's last credit was released, a race in the test. It now waits, bounded, for the service to be idle first. [Final 7/7 PASS](final-linux-admission_future.txt) |
| native_jobs | [FAILED 0/1](linux-native_jobs-original-failure.txt) | It asserted the superseded rule that an open handle refuses revocation. That assertion was removed; the request-source fence assertion remains. [Final 1/1 PASS](final-linux-native_jobs.txt) |
| complete_installed_roots | [FAILED 3/4](linux-complete_installed_roots-precondition.txt) | `huge_native_namespace_is_complete_after_install` requires an explicit closed preparation that was not supplied. Same outcome in the final run. **NOT_RUN**, not repaired |
| shared_processes | [FAILED 0/1](linux-shared_processes-precondition.txt) | Requires an explicit named-volume placement that was not supplied. Same outcome in the final run. **NOT_RUN**, not repaired |

Other final-source receipts kept with full output:
[overlay native_revocation](final-linux-native_revocation.txt),
[fuse dispatch](final-linux-dispatch.txt),
[daemon captured_runs](final-linux-captured_runs.txt).

## Host suite (macOS)

The same 65 binaries were built with the pinned 1.85.1 toolchain and run once
each under a 100 s limit ([results](host-suite-first-run.txt): 62 exit 0, 3 exit
101). Native mount tests are Linux-only and contain no host case.

| Binary | Result | Disposition |
| --- | --- | --- |
| captured_runs | [FAILED 2/3](host-captured_runs-original-failure.txt) | `credited_bytes` was 2000 immediately after the last consumer drop: the same asynchronous publisher credit release as admission_future, in a test and product source this change does not touch. The test now bounds a wait for the release. [Rerun 3/3 PASS](final-host-captured_runs.txt), and PASS on Linux at the final source |
| complete_installed_roots | [FAILED 3/4](host-complete_installed_roots-precondition.txt) | Same unsupplied closed preparation. **NOT_RUN** |
| host_handoff | [FAILED 0/1](host-host_handoff-precondition.txt) | Requires an explicitly listed Linux daemon binary that was not supplied. **NOT_RUN** |

The other 62 host binaries were not rerun after the captured_runs test edit;
only that test file changed.

## Static checks at the final source

[final-checks.txt](final-checks.txt) records the commands and outcomes:
`cargo fmt --all --check`; warning-denying Clippy for all targets and features
on Linux (six changed packages, Cargo 1.85.1) and on the host (whole workspace,
pinned 1.85.1); the product boundary guard; the fuser provenance check; the
tooling unit tests. Host Clippy under the machine's default 1.96.0 toolchain
fails on three newer lints in the untouched `layerfs-content` crate; that
toolchain is not the pinned one and the failure is recorded there, not fixed.

## Proof rows

| Row | Status | Basis and gap |
| --- | --- | --- |
| FP-1 fresh lifecycle | PASS, functional | Full topology and native_mount. Thread joins come from the drain receipt; no buffer or memory observation |
| FP-2 negotiation receipt | PARTIAL | Selected limits and mount-table options asserted. fusectl agreement **NOT_RUN** (not mounted). The recorded selected bits are 4194337 (async read, big writes, max pages), so writeback caching is absent, but no test compares them against the forbidden set; observed maximum request size is not separately instrumented |
| FP-3 complete-root read | PASS at the declared small roots | Independent manifest in the topology proof and fixture oracle in native_mount. Huge namespace **NOT_RUN** |
| FP-4 identity across mounts | PASS at the small root | Sequential and concurrent mounts in native_mount_routes |
| FP-8 parked requests | Component only | Dispatcher and port suites from the preceding checkpoints. A native external holding fixture is **NOT_RUN** |
| FP-9 background head-of-line | **NOT_RUN** | |
| FP-17 read side | PARTIAL | Kernel mode enforcement for another uid and raw symlink targets. `chmod`, ctime after mutation and target-length limits belong to mutation and are **NOT_RUN** |
| FP-20 read-only probe | PASS, functional | Three kinds of retained reference, restored Ready, later unmount. No concurrent request race between probe and `EBUSY` was staged |
| FP-21 complete drain | PARTIAL | Drain stages and bounded indexed retirement of unreleased handles (overlay native_revocation, 70 files and 70 directories, window 64). Held service/Store consumers after loop exit were not staged, so the `Retained` path has no executed case |
| FP-31 lookup counts | PARTIAL | Exact counts and bounded retirement at component level. Kernel FORGET delivery was not observed in these runs |
| FP-34 receive slots and stop | Component only | Dispatch suite, including the new event-driven quiescence wait |
| FP-25-Routes | PASS for control replies | Lost Mount and Attach acknowledgements. The attach-failure-after-mount race was not staged; a lost runtime result is runtime-owned and unchanged from R1 |

## Not run

Native mutation, mounted Commit, forced unmount and abort, daemon-wide graceful
drain, crash and restart of an attached daemon, resource and memory observation,
any timing measurement, the huge-namespace root, Durable.
