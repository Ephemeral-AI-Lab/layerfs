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

## Steps 2–4: writable OPEN, WRITE, SETATTR and namespace mutation

Product identity: the working tree committed with this section. Test binary
`layerfs-daemon/tests/native_mutation.rs`: in-process daemon Service, real
Overlay Owner and Store, real kernel mount, ordinary syscalls only.

| Binary | Attempt | Result | Receipt |
| --- | --- | --- | --- |
| native_mutation | 1 | PASS 2/2 | [output](step2-4-attempt1-native_mutation.txt) |
| native_mount | 1 | PASS 3/3 | [output](step2-4-attempt1-native_mount.txt) |
| native_custody | 1 | PASS 2/2 | [output](step2-4-attempt1-native_custody.txt) |

`native_mount` no longer asserts `EROFS`: its refusal block is now the shared
set of refusals that remain (below), and the unregistered nonroot `touch` that
R2 asserted was denied is now asserted to create an empty file owned by the
command identity. `native_custody` was changed by one Clippy rewrite
(`matches!(.., Ok(_))` to `.is_ok()`) after these runs; the binaries were not
rerun for that or for the rustfmt pass that followed.

What the two mutation cases executed, each checked three ways where it is file
content (cached read, `O_DIRECT` read served by the daemon, reported size):

- create with `O_EXCL` and mode, write, overwrite inside, sparse extension;
  two interleaved `O_APPEND` descriptors; a 300 000 byte write split by the
  kernel into 128 KiB requests; overwrite of a file inherited from the
  committed root with the uncovered bytes still inherited.
- the other name of that inherited inode reads the changed bytes from the
  shared page cache and from the daemon (FP-14 for a write).
- `ftruncate` shrink then regrow with a zero tail, write after regrow,
  `truncate` by path, `O_TRUNC` of a previously cached inherited file followed
  by a write.
- negative name then create, unlink then recreate; `mkdir`, `rmdir` with exact
  `ENOTEMPTY`; replacement rename with the replaced file still readable through
  its descriptor at link count 0; `RENAME_NOREPLACE`; a directory moved across
  parents three times; a move beneath itself refused `EINVAL`; replacing a
  nonempty directory refused; removal of an inherited file and directory.
- hard link with exact link count through both names and unchanged time;
  write through one name read through the other; rename of one name onto the
  inode's other name.
- symlink round trip, a 4095-byte target accepted, a 4096-byte target refused
  `ENAMETOOLONG` with no name created.
- chmod to 000 denies a nonroot reader at once although the pages are cached,
  and re-allows at once; explicit modification time with nanoseconds; chmod and
  rename leave the one stored time; change time is always reported equal to
  modification time.
- a shell script run as the nonroot command identity, never registered with
  anything, doing create, append, rename, link, mkdir, symlink, unlink, chmod
  and truncate through the same mount.

Refusals that remain, each with no effect: name over 255 bytes
`ENAMETOOLONG`; FIFO and socket `mknod` `EPERM`; set-user-id, set-group-id and
a sticky bit on a regular file `EPERM`; `chown` to another owner or group
`EPERM` (naming the projected identity succeeds and changes nothing); hard link
to a symlink `EPERM`; `RENAME_EXCHANGE` `EINVAL`; `setxattr`, `getxattr` and
`fallocate` `EOPNOTSUPP` (the daemon answers `ENOSYS` once); `fsync`,
`fdatasync` and directory `fsync` succeed with no engine work.

Both cases ended with `retained: 0`, `terminal: 0`, `unadmitted: 0` and a
normal unmount with a complete drain.

### Package suites at this identity

Every test binary of the four changed packages, built with `--no-run`, one run
per binary under a 100 s limit.

| Side | Binaries | Exit 0 | Other | Receipt |
| --- | --- | --- | --- | --- |
| Linux | 70 | 68 | 2 | [summary](increment1-linux-suite.txt) |
| Host (macOS, 1.85.1) | 70 | 66 | 4 | [summary](increment1-host-suite.txt) |

| Binary | Side | Result | Disposition |
| --- | --- | --- | --- |
| complete_installed_roots | both | FAILED 3/4 ([Linux](increment1-linux-suite-complete_installed_roots-b8a4f28c1707b846.txt), [host](increment1-host-suite-complete_installed_roots-e76bd78a0eded4e2.txt)) | `huge_native_namespace_is_complete_after_install`: explicit closed preparation not supplied. **NOT_RUN**, not repaired |
| shared_processes | Linux | [FAILED 0/1](increment1-linux-suite-shared_processes-498c7928b7fbb8cc.txt) | explicit named-volume placement not supplied. **NOT_RUN**, not repaired |
| host_handoff | host | [FAILED 0/1](increment1-host-suite-host_handoff-a4c4b3d19848e0be.txt) | explicit build-listed Linux binary not supplied. **NOT_RUN**, not repaired |
| captured_file_edits (workspace) | host | [FAILED 13/14](increment1-host-suite-captured_file_edits-c95644d018d26b22.txt) | `credited_bytes` read 2000 immediately after the last release. PASS 14/14 on Linux at this source. Not rerun, not repaired |
| root_qualification (daemon) | host | [FAILED 0/1](increment1-host-suite-root_qualification-b7ecf092dd1d0f90.txt) | `outstanding` read one higher immediately after the last job. PASS on Linux at this source. Not rerun, not repaired |

The last two are **failures at this identity** and stay failures. Diagnosis
from source: both counters are decremented together in the drop of the owner's
completion credit (`layerfs-daemon/src/overlay/credits.rs`), which the engine
thread can perform after the caller has already received the result; each test
reads diagnostics immediately. That is the race R2 recorded for
`captured_runs` on the host with the same value 2000. Neither test file, nor
`credits.rs`, `owner.rs` or `queue.rs`, is changed by R3. They were not rerun
to look for a pass.

The first host invocation ran no test: the runner's lock path was relative and
every binary exited 2 before starting
([record](increment1-host-suite-runner-error.txt)). The runner was corrected
and the table above is the single execution that followed.

Static checks: `cargo fmt --all --check`; warning-denying Clippy for all
targets on Linux (four changed packages) and on the host (whole core
workspace, pinned 1.85.1); the product boundary guard (834 production files);
the fuser provenance check before the native builds.

## Steps 5–7: removed references, shared mappings and the remaining proofs

Test binary `layerfs-daemon/tests/native_coherence.rs`, same harness. Three
attempts; the first two are failures and stay failures.

| Attempt | Result | Receipt | What changed before it |
| --- | --- | --- | --- |
| 1 | **FAILED 4/6** | [output](step5-7-attempt1-native_coherence.txt) | — |
| 2 | **FAILED 5/6** | [output](step5-7-attempt2-native_coherence.txt) | Product: two corrections below. Test: none |
| 3 | PASS 6/6 | [output](step5-7-attempt3-native_coherence.txt) | Test only: one observation replaced |

Attempt 1 found two product defects, each diagnosed from source before any
further run:

1. `a_lookup_racing_create_unlink_and_rename_never_binds_a_stale_inode`: a
   reader got `ENOENT` for a name that existed throughout. The kernel resolves a
   path to an inode and then sends OPEN for that inode; a replacement rename
   landed between the two, and R2's engine refused to open a file whose last
   name was gone (`eval.existing` in Workspace and an `nlink == 0` check in
   `observe_native_open`, pinned by an Overlay test). With mutation live that
   rule makes an atomic replacement visible as a missing file. **Corrected in
   the product:** a native OPEN of a removed file succeeds while the kernel
   reference that protects the request lives. The Overlay test
   `native_open_refuses_removed_and_nonregular_answers_without_acquisition` was
   rewritten as
   `native_open_refuses_nonregular_and_opens_a_removed_file_under_lookup_custody`
   ([PASS 7/7](step5-7-attempt1-overlay-native_lookup.txt)). This reverses an R2
   decision and is listed for owner review in the completion record.
2. `removed_references_keep_exact_state_without_open_custody`: a removed
   directory held by `O_PATH` reported link count 2. The projection in
   `layerfs-fuse/src/attributes.rs` answered a constant 2 for every directory.
   **Corrected in the product:** 0 once the directory has no namespace
   reference, 2 otherwise (the root always).

Attempt 2 is the corrected product, a new source identity. Its one failure was
in the test: it read `/proc/<pid>/cwd` of the nonroot resident process, which
this container's root may not do without a tracing capability (`EACCES`). The
removed-directory assertions before that line passed. The observation is now
made by the resident process itself (`stat .` after the removal). Attempt 3 is
that new test source.

What attempt 3 executed:

- **Shared mappings (FP-15).** `MAP_SHARED` stores with `msync`; a store past
  the end inside the last page (not published, size unchanged, and zero in both
  cache and daemon after a later extension); a store while another descriptor
  appends, each at its own offset; `munmap` without `msync`; and map, close,
  store, exit in a re-executed unregistered nonroot process. Receipt counters:
  `store_units=5`, exactly one SETATTR on the connection (the explicit
  extension), no retained or terminal request. A store arriving after its
  handle's RELEASE would have been refused and retained; none was.
- **Removed references (FP-28, FP-29).** Open-unlinked content, write and
  truncation through the handle; one of two handles released with the other
  still exact; then every handle released and, after engine maintenance
  settled, a kernel-forced `statx` through `O_PATH` alone: same inode, link
  count 0, size 15, same mode. A removed directory through `O_PATH` and from
  inside it as a nonroot process's working directory: link count 0, same inode,
  while the name was already rebound to a new directory. A replaced file
  through `O_PATH`. A hard-link alias whose count went 2, 1, 0 at once. Log
  rotation by rename away and then by replacement with a writer still
  appending: the nameless inode read back exactly.
- **Lookup races (FP-12).** 240 rounds of replacement rename and
  unlink-then-create on one name against two readers: 1885 reads found a
  generation at or above the newest one installed before the read began, 46
  found the name absent inside an unlink window, 53 found the just-created
  empty file. No stale binding and no absence outside a window.
- **Ordering (FP-16, executable half; FP-10 request count).** Cached size,
  kernel-forced size and read length formed one non-decreasing sequence against
  200 concurrent appends, every byte exact. A read of bytes the kernel already
  held sent zero requests.
- **Refusals stay refused (FP-18).** The whole refusal set twice, then 400
  one-byte appends by root and by a nonroot process: over 633 WRITE requests
  the connection received exactly one GETXATTR, one SETXATTR and one FALLOCATE.
- **Unmount probe (FP-20, mutation half).** Six unmount probes, each answered
  Busy by the kernel, against a thread appending, statting, reading, creating
  and unlinking, with a nonroot process's working directory inside the mount:
  no request failed, every acknowledged append was present in order, control
  admission reopened and the later unmount drained completely.

Observations printed by the last removed-reference case, not proof rows:
reopening a removed file through `/proc/self/fd` succeeded with exact content;
`fchmod` on a removed inode answered `ENOENT`.

### Package suites at the final identity

| Side | Binaries | Exit 0 | Other | Receipt |
| --- | --- | --- | --- | --- |
| Linux | 71 | 69 | 2 | [summary](final-linux-suite.txt) |
| Host (macOS, 1.85.1) | 71 | 66 | 5 | [summary](final-host-suite.txt) |

Linux: the two unsupplied-precondition cases, **NOT_RUN** as before
([complete_installed_roots](final-linux-suite-complete_installed_roots-b8a4f28c1707b846.txt),
[shared_processes](final-linux-suite-shared_processes-498c7928b7fbb8cc.txt)).
All five native mount binaries pass in it: native_mount 3/3, native_mount_routes
2/2, native_custody 2/2, native_mutation 2/2, native_coherence 6/6.

Host: the two unsupplied-precondition cases, **NOT_RUN**
([complete_installed_roots](final-host-suite-complete_installed_roots-e76bd78a0eded4e2.txt),
[host_handoff](final-host-suite-host_handoff-a4c4b3d19848e0be.txt)), and three
**FAILED**:

| Binary | Result | Assertion |
| --- | --- | --- |
| captured_file_edits (workspace) | [FAILED 13/14](final-host-suite-captured_file_edits-c95644d018d26b22.txt) | `credited_bytes` 2000, expected 0 |
| root_qualification (daemon) | [FAILED 0/1](final-host-suite-root_qualification-b7ecf092dd1d0f90.txt) | `outstanding` 0 after, 1 before |
| edit_backing (daemon) | [FAILED 3/4](final-host-suite-edit_backing-2bee8b6565d25697.txt) | `outstanding` 1, expected 0 |

All three read the owner's `credited_bytes`/`outstanding` diagnostics
immediately after a completion. Both counters are decremented in one place, the
drop of the completion's `Credit` (`layerfs-daemon/src/overlay/credits.rs`),
which is held through the executing lifetime and can be dropped by the engine
thread after the caller already has its result. `edit_backing` passed in the
increment-1 host run and failed here, which is what a timing race looks like.
None of the three test files, nor `credits.rs`, `owner.rs` or `queue.rs`, is
changed by R3, and all three pass on Linux at this identity. They are host
failures at this identity, not rerun and not repaired.

## Full topology with mutation

`core/crates/layerfs-api/sdk/examples/sandbox_native_mount.rs`: host Project Init
and seal on macOS, one Sandbox-created container running the actual
`layerfs-daemon` binary built from this source (SHA-256
`f0bcde210ab533e5a729dfe170153cb2934cb513de664bc1b4f2554b66329705`), install
into a named VM volume, SDK `mount`, and ordinary commands launched through the
Sandbox as uid 501 with no capability and no daemon registration.

The example's R2 step that asserted `touch`, append, `mkdir` and `rm` were
refused is replaced: the uid-501 command now creates a directory, writes and
appends a file, reads it back cached and with `O_DIRECT`, renames it, hard-links
it (link count 2, owner 501), removes everything again, and is refused `mkfifo`
and a set-user-id `chmod`. The complete-root oracle, the sustained workload,
the Busy probe, the normal unmount and three fresh mounts follow unchanged.

| Attempt | Limit | Result | Receipt |
| --- | --- | --- | --- |
| 1 | 20 s | **FAILED** before any effect | [output](full-topology-attempt1.txt) |
| 2 | 20 s | PASS in 7.7 s | [output](full-topology-attempt2.txt) |

Attempt 1 was my invocation: the image was named by tag and the Sandbox
validates an immutable digest before it contacts the runtime (`phase:
Validate`, `attempted: false`, `immutable image digest`). No container was
created and nothing was mounted. Attempt 2 names the same image by digest. The
elapsed marks are diagnostic wall-clock values, not a timing claim. Both named
volumes and the host directories I created were removed; the Sandbox deleted
its own container.

## Not run

- FP-16, the half that needs an external fixture holding a GETATTR inside the
  daemon across a WRITE. The product has no hook to hold a request and none was
  added.
- FP-15: the writeback request's header identity is not reported by the
  product and was not observed; Commit inclusion of mapped stores is R5.
- FP-29: a processing read spanning the release of its handle was not staged.
- Files above 4 GiB, any cold, timing, storage or resident-memory measurement.
- Durable: `NOT_RUN — disabled by owner until explicit reauthorization`.
- `complete_installed_roots::huge_native_namespace_is_complete_after_install`,
  `shared_processes` (Linux) and `host_handoff` (host): explicit preconditions
  not supplied.
