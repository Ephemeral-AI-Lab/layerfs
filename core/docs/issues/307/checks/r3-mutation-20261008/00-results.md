# R3 checks: native mutation and kernel coherence

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Functional receipts for R3. Every run used natural caches; nothing here is a
cold, latency, storage or resident-memory claim. The global Store was
Disposable/WAL/synchronous=OFF in every run, selected explicitly; Durable is
`NOT_RUN — disabled by owner until explicit reauthorization`.
`LAYERFS_CONSTRUCTION_WORKERS=1` was exported.

## Environment

- Linux runs: Docker image
  `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`,
  kernel `6.12.76-linuxkit`, locked Cargo 1.85.1.
- Native test containers: `--device /dev/fuse --cap-add SYS_ADMIN
  --security-opt apparmor=unconfined`, `Privileged=false`, repository bound
  read-only at `/work`. Store, Overlay and mount directories were under the
  container's own `/tmp`, never under the repository bind mount.
- One command at a time under the worktree-local nonblocking lock
  `core/target/r3-mutation.lock`. Each test binary ran once per attempt under a
  100 s limit inside the container and a 110 s limit outside it.

## Step 1: the two R2 paths that had never executed

Test binary `layerfs-daemon/tests/native_custody.rs`: in-process daemon Service,
real Overlay Owner and Store, real kernel mount, product source unchanged from
R2 (`core/crates` tree `d0f378aec`).

| Attempt | Result | Receipt |
| --- | --- | --- |
| 1 | **FAILED 1/2** | [output](step1-attempt1-native_custody.txt) |
| 2 | PASS 2/2 | [output](step1-attempt2-native_custody.txt) |

Attempt 1 stays a failure. The FORGET case passed in it. The Retained case
panicked in the test's own helper: it submitted a nonwaiting owner job
immediately after an `fs::read`, while that read's asynchronous RELEASE and
processing releases still held both lifecycle slots of the namespace, and the
owner answered the exact unattempted refusal `AdmissionFull`. That is a race in
the test, diagnosed from `OwnerClient::try_submit`; no product source changed.
The test now waits, bounded, for the connection to be quiescent first. Attempt 2
is that new test source, not a repeat to obtain a pass. After attempt 2 the test
file was reformatted by rustfmt only.

### Kernel FORGET with its exact decrement

`kernel_forget_arrives_on_a_live_connection_with_its_exact_decrement`

- 27 names over 26 inodes were looked up by ordinary `lstat`; the in-root hard
  link gives one inode two kernel references. Engine lookup owners rose by
  exactly 26.
- Stimulus: one write of `1G` to the container's own
  `/sys/fs/cgroup/memory.reclaim` (remounted read-write inside the container).
  It is scoped to that container's memory cgroup; no system-wide cache was
  dropped and no other container was touched.
- Observed on the still-mounted, still-Ready connection: `forget_units: 26`,
  `retained: 0`, `unadmitted: 0`, `terminal: 0`, and engine lookup owners back
  at the attach baseline. A short decrement would leave a row; an excess one is
  an underflow failure retained on the lane. Neither occurred.
- The same names then bound the same inodes again, each reacquiring one owner,
  and the mount unmounted with a complete drain.

This closes the R2 gap "kernel FORGET delivery was not observed". The decrement
count per inode is checked through owner-row conservation and the absence of a
retained request; the per-unit `nlookup` value is not printed by the product.

### An unmount that ends `Retained`

`an_unmount_that_cannot_revoke_stops_retained_and_later_replies_repeat_it`

- A request source was acquired through the public owner and held: a service
  consumer that connection drain cannot see.
- Unmount: probe succeeded, detach known, both loops joined, lane released,
  then `NativeJob::Revoke` was refused with `BaseSourcesPending`. Reply
  `Retained` with stage `Revoke`, `detached: true`, loops 2/2 joined, no
  received, admitted or retained request.
- Status reported phase `Retained`, the original Ready record and activity
  `Closing`. The kernel mount entry was gone. The engine mount was neither
  revoked nor replaced.
- A later Unmount and a later Attach each returned the identical custody and
  attempted nothing. Releasing the held source changed nothing by itself: the
  entry stayed `Retained` and a further Unmount again returned the same custody.

This executes one `Retained` stage (`Revoke`). The `Detach`, `Join`, `Owner`,
`Requests`, `Lane`, `Close` and `Registry` stages still have no executed case.
A Workspace that reaches `Retained` has no path back to service or to terminal
success in the current product; that recovery is R6 work and is restated in the
completion record.
