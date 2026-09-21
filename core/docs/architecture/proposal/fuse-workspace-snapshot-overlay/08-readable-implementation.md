# Readable Workspace implementation record

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> Started 2026-09-21 from synchronized main
> `0749180db34d1cdc57f905806a17e3f3f48ec2bc`, on
> `codex/pair1-readable-mount`. The historical source audit remains pinned to
> `152b9c3a2` and the reference remains exact v0.1.6. No historical receipt changed.

This record owns actual R0-R/R1 decisions and round results, in addition to
[04's acceptance matrix](04-implementation-and-verification.md). Source work
and verification below concern the replacement workspace only. The source
checkout was preserved; implementation is isolated in
`/Users/yifanxu/.codex/worktrees/pair1-readable-mount/layerfs`.

## R0-R: selected readable profile

The first shared prerequisite is `Inspect::Attributes { path }`, a profile-1
Inspect query using existing authentication and authorization. Its fixed
106-byte result adds exact logical size to Stat's serial, kind, namespace
references, content/metadata roots, mode and signed timestamp. Service composes
public C1 reads; no canonical-object RPC, new grant or changed input limit.
Existing Stat and List encodings are preserved. Listing remains List followed
by complete Attributes for each returned child, within one callback deadline.
This is bounded correctness coverage; it is not a batching or latency improvement.

`Source` moves from the native client to the portable bridge contract; the old
native path reexports it. Workspace's `OperationDelivery` is one existing
Request/Source/Write/Response/Failure call with a caller-local deadline. The
native daemon binds that capability; Workspace imports no FUSE, service,
storage, history implementation, network endpoint or native socket.

Current native client **and server** close a session on every failed operation,
including a normal missing pathname, and idle sessions expire. R1 selects one
fresh authenticated connection per logical call, with no replay or reconnect
inside a failed call. `connect_until` bounds TCP connect, authentication and
HELLO by the same absolute callback deadline. This deliberately incurs more
handshakes than the persistent successful-call path described in the old audit;
no transport-performance improvement is claimed. A later session-lifetime
change belongs in the existing bridge owner and needs separate proof.

The concrete callback budget is 10 seconds across all dependent queries;
existing transport progress timeout remains 5 seconds. Caller-side timeout does
not prove remote storage work stopped. No immediate FUSE_INTERRUPT support is
claimed. Local kernel mount/unmount calls have deadline observation points,
not a claim of preemptible syscalls. Unmount stops new callbacks, drains owned
results and projection handles, detaches and joins; failed or timed-out cleanup retains ownership.

R1 is immutable: explicit roots use (Store, filesystem root, serial) identity;
Branch attach uses GetBranch's validated actual root serial. All mutations and
retargeting remain unavailable. One mount lease per Workspace is the bounded
projection binding. It closes admission before detach and prevents clean close
while mounted. Local and projection lookups/handles use separate `ReferenceScope` accounting.
`status()` is fallible and reports the projection handle count. There is no mutation event bus or best-effort invalidation; W
must implement the ordered affected-identity/revision binding before SDK success. Local and projection lookup reference counts are separate; unmount
releases only the latter.

Linux aarch64/x86_64 uses published fuser 0.18.0, cached read I/O, two fixed event loops, 128 KiB
maximum read, one kernel background request, no writeback, readdirplus, stateless-open, allow_other or AutoUnmount.
Mount options are read-only, nosuid, nodev, default_permissions, exec and noatime.
The initial supported mount deployment is privileged Linux daemon UID 0 with
CAP_SYS_ADMIN and /dev/fuse; application owner is that same execution identity.
Non-Linux and missing capability fail explicitly. The existing scratch/nonroot
headless Docker image remains its own deployment; the functional mounted proof
runs the built daemon in the supplied Linux runtime. No dependency was patched.

The four process settings follow 01: an explicit absolute existing common root,
8,388,608-byte default aggregate Workspace working budget, explicit positive
MAX_COUNT, and a positive disk quota only for future W. R creates only the
managed `workspace/` directory and its fresh owned child. It allocates no private
backing. Duplicate/stale child paths and symlinks are refused, never adopted or
removed as setup. The native local entry is:

```text
layerfs-daemon --mount-readonly ID INCARNATION_HEX STORE ROOT_HEX UID GID
layerfs-daemon --mount-readonly ID INCARNATION_HEX STORE branch:TAGGED_BRANCH_HEX UID GID
```

No arguments retain headless framed forwarding. The new entry assembles an
in-process API; it is **not** authenticated host-SDK/container-Workspace control.
SIGINT/SIGTERM initiate explicit unmount and clean close. R1-C still requires its
own bounded daemon-targeted profile, operation authorization and controls through
existing bridge owners; these are not claimed by local startup or shell exec.

## Readable resource arithmetic

Workspace reserves one aggregate consumer budget. Each admitted Workspace owns
finite tables: 256 retained nodes (including 4096-byte immutable path locators),
128 semantic handles, and 1024 directory-cookie records. These are demand-used
bounded tables, not a resident mirror of the full namespace. Root and exposed
serials cannot be recycled to another inode. Directory cookies stay bound to the
handle and immutable view; reaching capacity refuses instead of invalidating old
positions. This R profile has a finite namespace/handle compatibility ceiling;
it is not the paged metadata design or npm qualification required by R3b.

The implementation reserves table capacities using Rust `size_of` and reserves
fixed host/Workspace descriptors plus 128 KiB scratch per admitted remote
callback. Read result capacity up to 128 KiB and directory result/name capacity
stay charged until their owned reply guards drop. One consumer-wide remote
callback lease uses immediate refusal, with no waiting client mutex queue.
Open/release/local attribute work uses short state locks; no bridge call holds
registry or Workspace state locks. Count reservation precedes attach; failure
with unresolved cleanup retains the slot. Remaining exact arithmetic and
exercised boundary observations are recorded with the R1 result below.

**Separate domains:** fuser allocates `16 MiB + 4096` receive bytes per event
loop even when max_read is 128 KiB. Two loops therefore reserve **32 MiB + 8192
bytes**, plus their thread/session state. The bridge's authentication/frame
buffers and 2 MiB upload-thread stack, service/C1/C2 state, kernel page/socket
memory and process baseline are also separate. The 8 MiB Workspace number is
not daemon RSS, cgroup memory, FUSE receive capacity or a measured resource pass.
No extra userspace payload cache or prefetch is added.

## Round results

The shared prerequisite was committed as
`646fcd5e5027ca8bea361c4a44ffc491ed8e7b05`. Its exact parent/staged/committed
production count was 96,380 -> 96,557 (+177): reference 65,417 unchanged,
core 30,963 -> 31,140 (+177), using `tools/production_loc.py` on Git archives.

R1 whole-core locked tests, example/bin build, warning-denying Clippy, formatting,
and the boundary guard/six self-tests passed on the macOS host. Later ownership
fixes passed targeted daemon/FUSE/Workspace tests and whole-core Clippy again.
The Linux daemon build, all-target Clippy and seven Workspace tests passed with
Rust1.85.1 and root AEAD flags (including the root-DAC test unrun on the nonroot
macOS host). Attempts `mounted-01` and `mounted-02` retained ordinary-open failure:
GNU64 libc reports O_LARGEFILE=0, while aarch64 Linux sends its raw 0x20000 FUSE
flag. The initial whitelist refused it. The second receipt contains the complete
stderr; the first driver retained the failure but accidentally omitted its
captured stderr. The oracle was corrected to retain captured command failure
output. Attempts 03-06 retain later failures: Linux exec's internal `__FMODE_EXEC` flag;
a shutdown oracle which omitted the selected ENODEV refusal; the PID1 container
lifetime killing its reporting child; and native EBUSY when backend completion
preceded release of the FUSE file handle. The final correction tracks projection
handles and drains them before one detach attempt. A separate executor process
keeps the container alive until all syscall results and cleanup are observed.
No failed attempt was removed or relabelled.

**R1 PASS at the declared read-only scope:**
[final receipt](evidence/r1-20260921/mounted-07/result.json), alongside
[all attempts and check logs](evidence/r1-20260921/).
The full functional command took 32.135557166 seconds within its 60-second hard
budget. This is external verification wall time, not product latency or a
performance sample. The receipt records product-input, binary, image,
oracle/driver/helper hashes and the dirty source context at collection. It
includes new files in the product-input hash; earlier diagnostic receipts which
only hashed Git diff do not establish that complete source identity.

| 04 requirement | Actual proof / limits |
| --- | --- |
| R-01, R-02 | Real Linux FUSE mount from explicit root and GetBranch; actual canonical root serial 2, no guessed serial 1 |
| R-03, R-11 | Wrong root role, missing root, unauthorized Store and unauthorized Inspect all refuse before mounted readiness; malformed profiles/grants also have direct/native shared tests |
| R-04 | Empty and long-name paginated directory; all 108 names exact, ordinary dirent kinds and saved-cookie seek checked |
| R-05 | Complete 577,551-byte file, prefix, boundary, final short read, EOF and dup/open lifetime; no content-size change to fit a timing target |
| R-06, R-07 | Exact symlink/dangling behavior, hard-link inode/nlink equality, independent and duplicated descriptors; external API tests cover scoped forget and retained local handles |
| R-08 | Actual 68,384-byte Linux ELF from pinned runtime image executes through mount; its runtime-image loader/libc remain dependencies; non-executable data denies exec even for UID0 |
| R-09, R-10 | Mutation/synchronous flags and fsync refuse; managed root rename/removal refuses, including attempts by UID/GID65534 |
| R-12 | Open real FD, pause the owned host service, observe the callback's established TCP connection, signal daemon shutdown; read reports ENODEV, closes, then projection drains/detaches/joins/cleans. This is a delayed connection/authentication response, not proof of cancelled C1 I/O or a save overlap |
| Peer loss / cleanup | Unread payload after service shutdown fails; mount child and private-backing absence checked; owned containers and volume removed |

R1 does not observe every VFS reference: cwd/O_PATH references may still yield a
native EBUSY after tracked-handle drain. That failure retains ownership and is
not retried or reported as successful cleanup. x86_64 has explicit kernel flag
mapping but was not mounted here; the actual runtime was Linux aarch64. No
macFUSE/Windows projection, writable syscall, SDK edit, snapshot or Commit route
is qualified by this result.

Reproduction, after locked host examples/bins and the Linux daemon are built:

```sh
LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/crates/layerfs-daemon/tests/mounted_read.py \
  --linux-daemon "$PWD/core/target-linux/debug/layerfs-daemon" \
  --output "$PWD/core/target/pair1-evidence/NEW-UNUSED-OUTPUT"
```

The Linux build uses `rust:1.85.1-bookworm`, the worktree bound at `/work`,
`CARGO_TARGET_DIR=/work/core/target-linux`, the existing registry read-only, and
`cargo build --manifest-path core/Cargo.toml --locked --offline -p layerfs-daemon`.
Clippy is installed as a toolchain component in the disposable check container;
no third-party crate source is modified. `.cargo/config.toml` supplies the AEAD
profile. Store and catalog remain on the host throughout mounted verification.
The next independent operation is R1-C authenticated daemon Status. R2, R0-W/R3a-R3d,
R4, R5a/R5b and R6 remain open. No issue is closed. No timing sample,
performance comparison, durability promise or full writable pipeline is claimed.

## Production source comparison

| Commit round | Combined | Reference | Replacement core |
| --- | --- | --- | --- |
| Shared read prerequisite `646fcd5e5` | 96,380 -> 96,557 (+177) | 65,417 -> 65,417 (+0) | 30,963 -> 31,140 (+177) |
| R1 readable projection, first parent `646fcd5e5` | 96,557 -> 98,831 (+2,274) | 65,417 -> 65,417 (+0) | 31,140 -> 33,414 (+2,274) |

Method: the unchanged `tools/production_loc.py` counter, Git blob
`b5b9617d08204977176302311e0b2c72a811b420`, on exact parent/staged snapshots
exported with `git archive REV crates core/crates`; then
`python3 tools/production_loc.py --root EXPORTED --json`. Counts product Rust
and shipped runtime SQL, excluding inline/external tests, examples, fixtures,
tools, docs, manifests, blanks and comments. Staged product files are checked
against the mounted receipt's product-input seal before commit; commit tree
identity is checked against that final stage. This adds the two selected
libraries and daemon assembly, retaining all reference implementation; it is
neither legacy retirement nor a performance/simplification claim.
