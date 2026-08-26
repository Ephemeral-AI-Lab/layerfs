# Stage 2 Specification — LayerFS + Linux FUSE Direct Workspace

Status: **implemented; local correctness/resource, restart durability, and
matched native-FUSE comparison pass**.

Current terminal local-only custody is
[candidate 015](evidence/stage2-freeze-candidate-015/summary.json): source
`7e82abcd7320f6a214be336d82488ba0527b6025`, image
`sha256:f8647b84580c75d4688a18665e4c60cd6dcf5b2d3092cf22bce34dfbd86b59b0`.
Its persistence-inclusive campaign passes 36/36 measured samples plus 12
warmups on fresh Stores: sum of durable medians `8.229 s`, separately selected
live medians `3.898 s`, and durability-residual medians `4.337 s`. Every sample
has two independent SIGKILL/Verified-reopen proofs, exact generation/root/
inventory/bytes, and zero owned residue. Current-source focused proofs also
pass for dirty successful external unmount with one publication, metadata-heavy
post-`fsyncdir` SIGKILL, and 64 MiB high-entropy post-`fsyncdir` SIGKILL.
The candidate-015 unchanged-upstream live rerun also passes under one CPU,
512 MiB, Verified integrity, and real FUSE: `/var/tmp` SL `3.361 s`, Rsum
`2.193`, G `3.372`, Spread `1.058`; `/tmp` SL `3.299 s`, Rsum `2.133`, G
`3.569`, Spread `1.021`. These remain `LIVE_MOUNT` diagnostics rather than
persistence-inclusive evidence. Under the same local native ARM64, one-CPU,
512 MiB envelope, Cloudflare Computer's FUSE median sums are `7.260 s` and
`7.449 s`; LayerFS is `2.160x` and `2.258x` faster respectively and uses lower
whole-cgroup peak memory. Restart diagnostics now separate two Cloudflare
persistence classes: standalone `computerd` predictably loses its process-local
SQLite state, while the shipped pull/reconcile/push path backed by a local
file-SQLite authority survives SIGKILL and rehydrates the exact 64 MiB payload
through fresh native FUSE. LayerFS independently reopens exact bytes from its
production Store. The persistence timings are not ranked because the commands,
clocks, sync endpoints, media, and retention contracts differ. Cloud deployment
and Durable Object sync are outside the user-selected scope. `PASS_LOCAL_ONLY`
is terminal for the user-selected local-only scope; it is not global
`PASS_OPTIMIZED`, deployed Cloudflare, Durable Object durability, or a
persistence-latency comparison. Candidate 014 and candidate 013 are historical;
candidate 012 remains superseded.

Entry sequence:

```text
final Stage 1.1 correctness/durability closure
  -> Stage 2.0 Docker/FUSE admission
  -> Stage 2.1 read-only LayerFS FUSE
  -> Stage 2.2 writable mounted workspace
  -> Stage 2.3 count-changing/locality proof
  -> Stage 2.4 real container workspace
```

This document controls only the first direct **LayerFS + Linux FUSE**
implementation, using Docker as its execution envelope, and its focused
qualification. It does not authorize OCI import/export,
containerd integration, Working/Durable Fetch/Push, Windows, macFUSE, FSKit,
Kubernetes, or production packaging.

## 1. Authority and entry gate

Authority order:

1. [10 — Handoff freeze](10-handoff-freeze.md) remains authoritative for
   canonical bytes, identities, authentication, publication, history,
   durability, metadata, and compaction.
2. [09 — Portability and Apple completeness](09-portability-and-apple-completeness.md)
   remains authoritative for the dependency direction and platform boundary.
3. The final Stage 1.1 source/artifact remains the correctness, algorithm, and
   resource control for direct logical edits, reads, history, and Apple
   projection. Its Verified performance REVISE remains preserved; the explicit
   TrustedLocalDev class is the admitted local developer-loop mode.
4. The explicit user decision on `2026-08-26` skips Stage 1.2. The retained
   [15 — developer-workspace workload](15-stage1-workspace-benchmark.md) is a
   non-gating historical workload reference only.
5. This document controls Stage 2 LayerFS + Linux FUSE scope, files, algorithms,
   tests, measurements, and stop rules after the final Stage 1.1 closure.

Research input:

- [Cloudflare Computer research](../research/cloudflare-computer-architecture.md)
  informs container/FUSE execution and remote-client sequencing. It is not
  implementation authority.

Entry requires:

```text
Stage 1.1 correctness/durability          closed and source-bound
Stage 1.1 Verified performance REVISE     preserved without relabeling
Stage 1.1 TrustedLocalDev class           admitted for the local loop
Stage 1.2                                 skipped; not an entry gate
canonical codecs                         unchanged or explicitly re-frozen
working tree                             committed/clean for Stage 2 ownership
Docker host                              admitted separately
Linux FUSE                               admitted separately
```

Begin Stage 2 only from a committed clean product baseline while preserving
unrelated user specifications and evidence.

## 2. Decision

The selected platform direction is:

```text
macOS
  ordinary APFS workspace remains the supported native route
  no macFUSE dependency
  no FSKit dependency

Stage 2
  LayerFS is the authoritative filesystem and storage model
  Linux FUSE is the thin interface exposing LayerFS at /workspace
  Docker is only the Linux execution envelope
  tools run inside the container
```

The product is the combination:

```text
LayerFS persistent namespace/inode/extent graph
  + Linux FUSE request translation
  = ordinary mounted workspace without a second physical file tree
```

FUSE is not a second store, cache, overlay, or canonical representation. The
execution host's WorkingStore serves the live mount; the central DurableStore is
the true system of record. The FUSE adapter must translate kernel
requests into universal `layerfs-core::logical` operations and must not
implement a parallel filesystem model. Core logical remains generic over
`ObjectRead`/`ObjectStore` and contains no FUSE, Linux, SQLite, workspace, or
Working/Durable authority policy.

The target product always has both physically distinct Stores. Filesystem
syscalls and WorkingStore-only OperationCommit never synchronize implicitly;
`Fetch` and `Push` run only at explicit durable
Branch/child-Branch/merge boundaries.

Docker's own OverlayFS/containerd snapshotter may implement the container's
operating-system root. It is not the LayerFS workspace representation:

```text
container /
  /bin         Docker image filesystem
  /usr         Docker image filesystem
  /lib         Docker image filesystem
  /workspace   direct LayerFS FUSE mount
```

Rejected primary route:

```text
/workspace
  -> OverlayFS
       lowerdir = LayerFS
       upperdir = ordinary filesystem
```

OverlayFS performs file-level copy-up on first data modification and can copy a
complete large lower file for a small edit. That defeats the selected
count-changing locality objective.

### 2.1 Exact count-changing claim

The LayerFS + FUSE combination removes the mandatory APFS projection round
trip from mounted work:

```text
Stage 1 APFS projection
  LayerFS root -> materialize physical tree -> edit -> capture -> new root

Stage 2 direct mount
  LayerFS root -> FUSE view -> edit LayerFS state -> publish new root
```

It therefore eliminates workspace materialization and capture for mounted
operations; it does not make an explicit export to an ordinary APFS file
incremental. Such an export remains byte-linear.

The persistent extent tree can represent an explicit insertion or deletion by
reusing the unchanged prefix and suffix and replacing only affected payload
chunks and tree spines:

```text
explicit splice work              O(B + log E + path)
unaffected suffix payload reads   0
unaffected suffix payload writes  0
physical suffix shift             0
```

This guarantee requires explicit change intent, such as the LayerFS splice
operation or an admitted kernel range operation. POSIX/FUSE `write(offset,
bytes)` means overwrite, not insert. If an application writes a complete
temporary replacement and renames it over the old file, LayerFS must consume
that submitted stream in `Theta(input bytes)` even when content-defined
chunking later recovers object sharing.

No Stage 2 report may generalize the explicit-splice result to every editor or
opaque file-save pattern.

## 3. Goal and success definition

Stage 2 proves that ordinary Linux tools can consume and modify the real
LayerFS namespace/inode/extent representation through a thin FUSE interface
without an ordinary physical workspace, materialization loop, watcher, or
capture loop.

Success requires:

```text
Linux applications
  -> kernel VFS
  -> FUSE
  -> thin layerfs-mount::fuse adapter
  -> universal OperationWorkspace lifecycle in layerfs-workspace
  -> concrete mounted driver in layerfs-mount
  -> portable semantics in layerfs-core::logical
  -> layerfs-storage ObjectRead/ObjectStore
  -> layerfs-working-store on the container's native volume
  -> explicit layerfs-sync + layerfs-service boundary
  -> layerfs-durable-store system of record
```

The retained Stage 2 mount campaign measured the path through WorkingStore and
therefore qualifies that mechanism only, not the complete durable product.
Product qualification must separately cover explicit Fetch/Push,
DurableStore acknowledgement, conflict/reconciliation, and fresh-host recovery.
A benchmark-only in-memory filesystem, ordinary backing-file shim, or generic
FUSE loopback does not measure LayerFS + FUSE and cannot satisfy a product gate.

The visible `/workspace/path` must not correspond to a separately materialized
native `path` behind the mount.

Terminal Stage 2 PASS requires:

- exact read-only mounted behavior;
- exact writable behavior and checkpoint/reopen;
- direct use of the product namespace, inode, extent, metadata, publication,
  and history paths;
- zero workspace materializations and zero capture scans in mounted workflows;
- bounded memory independent of file/workspace size;
- explicit and measured write/checkpoint durability boundaries;
- honest count-changing classifications;
- explicit-splice proof with zero unaffected-suffix payload reads and writes;
- opaque full-replacement rows kept byte-linear and labeled as such;
- exact old-root reads, fork, rollback, and restart;
- real git/npm/search/build behavior within the frozen budget;
- comparison against a native Linux volume in the same container/VM class;
- resource and mount cleanup; and
- one source-bound terminal artifact.

## 4. Non-goals

Stage 2 does not implement:

- macFUSE, FSKit, File Provider, or an Apple kernel extension;
- a mount projected back into `/Users/...` on macOS;
- Windows WinFsp;
- OCI layer import/export;
- a containerd or Docker volume/snapshotter plugin;
- remote object/ref storage;
- RPC, WebSocket synchronization, or a remote shell service;
- OverlayFS as the LayerFS workspace;
- a filesystem watcher or polling/hash shim;
- multi-container concurrent writers;
- merge, rebase, or conflict resolution above exact expected-head rejection;
- online/background GC;
- a pack format or transport bundle;
- Kubernetes, orchestration, production images, signing, or distribution;
- device nodes, sockets, FIFOs, paging files, or privileged special files;
- a 100 GB campaign;
- a second canonical file/namespace representation; or
- weakening authentication, durability, metadata, or history to meet a timer.

## 5. Target architecture

```text
macOS host
  Docker Desktop
    Linux VM
      developer container
        WorkingStore volume
          /var/lib/layerfs/working/layerfs.sqlite
          /var/lib/layerfs/working/workspaces/<operation-id>-<nonce>/
            owner / recovery / view / spool

        layerfs-workspace
          universal OperationWorkspace lifecycle
          layerfs-mount::fuse concrete driver
            private mount view exposed at /workspace

        tools
          bash
          git
          node/npm
          compiler/build/test/search
```

Request flow:

```text
read("/workspace/src/main.rs", offset, length)
  -> FUSE read
  -> mounted handle/inode
  -> layerfs-core::logical range read through ObjectRead
  -> O(log E + intersecting extents + returned bytes)
  -> return bytes
```

Write flow:

```text
WorkingStore begin_operation
  -> persist OperationId + exact BranchHead/base-version lease + recovery
  -> single-use WorkspaceTicket

layerfs-workspace + layerfs-mount driver
  -> create private 0700 mount view and sibling bounded spool

write / truncate
  -> bounded ordered dirty-range journal
  -> reads in the same mount observe dirty state

workspace finalization
  -> driver quiescence and exact dirty evidence
  -> layerfs-core::logical candidate RootId/RootTransition
  -> layerfs-storage object admission

WorkingStore OperationCommit
  -> bind Operation identity + RootTransition as OperationDelta
  -> one expected-Branch-head publication
  -> one visibility COMMIT
  -> WorkingRecorded OperationVersion or Conflict

explicit Push, outside the retained Stage 2 Working-only campaign
  -> accepted canonical/version records only
  -> DurableStore independently authenticates/verifies
  -> one durable head transaction
  -> DurablyAccepted | Conflict | Indeterminate
```

Restart flow:

```text
daemon/container stops
  -> no unacknowledged durable claim

fresh daemon
  -> open WorkingStore through layerfs-storage
  -> read WorkingRecorded BranchHead/OperationVersion
  -> mount exact root
  -> old accepted roots remain directly readable
```

The retained implementation/evidence may use current `RefState`, `checkpoint`,
`layerfs-engine`, and `layerfs-vfs::mounted` names. Those are honest
current-source labels, not target ownership or proof of DurableStore acceptance.

Target workspace custody defaults to the WorkingStore-owned root adjacent to
working SQLite:

```text
<working-root>/workspaces/<operation-id>-<nonce>/{owner,recovery,view,spool}
```

It is `0700`, marker-validated without following links, safely cleaned by exact
ownership, and host-local. The mounted driver uses `view` as its private
mountpoint and sibling `spool` for bounded dirty bytes. Sync never transfers
these paths, markers, spools, mount/process/descriptor state, or native files;
DurableStore never stores them.

## 6. Dependency law

The controlling target law is:

```text
layerfs-core::logical
  exact-version stat/list/read_range/stream/readlink
  portable mutation, candidate RootId/RootTransition, root diff, merge
  generic only over ObjectRead/ObjectStore
  no SQLite, FUSE, libc, Docker path, workspace, sync, or authority policy

layerfs-storage -> layerfs-core
  one SQLite/object/schema/integrity/transaction/compaction mechanism

layerfs-working-store -> layerfs-storage + layerfs-core
  Operation identity, exact BranchHead/base lease, recovery record
  WorkingRecorded OperationCommit and host-recoverable candidates

layerfs-durable-store -> layerfs-storage + layerfs-core
  independent verification and DurablyAccepted system-of-record policy

layerfs-workspace -> layerfs-core + layerfs-working-store
  universal private runtime lifecycle, admission, quiescence, custody, cleanup

layerfs-mount -> layerfs-core + layerfs-workspace
  logical COW overlay/spool; fuse/ owns Linux translation/session lifecycle

layerfs-materialization -> layerfs-core + layerfs-workspace
  physical materialize/capture/refresh; apfs/ owns Apple mechanics

layerfs-sync -> layerfs-storage + layerfs-working-store + layerfs-durable-store
layerfs-service -> layerfs-sync(server) + layerfs-durable-store
```

There is no target `layerfs-fs`, shared-role Engine, or runtime Store/workspace
mode. Extract Workspace/FUSE/APFS ownership first, then move the remaining
portable `layerfs-vfs` semantics directly into `layerfs-core::logical` and
delete the old VFS implementation. Current `layerfs-engine`, `layerfs-vfs`,
`layerfs-fuse`, and `layerfs-os` paths remain legitimate evidence names until
that one-way migration completes.

## 7. Expected repository shape

The expected new and edited tree is:

```text
Cargo.toml

crates/
  layerfs-core/
    src/
      logical/
        mod.rs               universal exact-version filesystem operations
        resolver.rs          canonical path/inode resolution
        read.rs              stat/list/range/stream/readlink
        mutate.rs            portable logical mutations/candidates
        diff.rs              Merkle/root diff
        merge.rs             three-root merge candidates

  layerfs-storage/
    src/                      SQLite/object/schema/transaction mechanisms

  layerfs-working-store/
    src/                      host-recoverable operation/Branch policy

  layerfs-durable-store/
    src/                      independent durable admission/retention policy

  layerfs-workspace/
    src/
      operation.rs           universal OperationWorkspace lifecycle
      workspace.rs           private runtime/custody
      direct.rs              no-path logical driver
      driver.rs              concrete-driver contract
      quiescence.rs          writer/process/mapping barrier
      receipt.rs             terminal runtime receipt

  layerfs-mount/
    Cargo.toml               target-specific Linux FUSE dependency only
    src/
      lib.rs                 mounted capability exports
      driver.rs              layerfs-workspace concrete mounted driver
      session.rs             mounted logical COW overlay state
      fuse/
        mod.rs               callback translation and errno mapping
        daemon.rs            mount/unmount and request lifecycle
      bin/
        layerfs-mount-fuse.rs
    tests/
      mounted_routes.rs      platform-neutral mounted-session tests
      fuse_routes.rs         focused real-FUSE container tests

  layerfs-materialization/
    Cargo.toml               target-specific Apple dependencies only
    src/
      lib.rs                 materialization capability exports
      driver.rs              layerfs-workspace concrete physical driver
      materialize.rs         canonical root -> native workspace
      capture.rs             native workspace -> canonical root
      refresh.rs             related-root native reconciliation
      workspace.rs           managed/external workspace authority
      apfs/
        mod.rs               Apple adapter export
        workspace.rs         parent-handle APFS workspace
        apfs.rs              clone/patch/native helpers
        metadata.rs          supported Apple metadata
        ffi.rs               Apple-only syscall boundary
        store.rs             Apple Store/open integration

  layerfs-sync/
    src/                      explicit bounded Fetch/Push bridge

  layerfs-sdk/
    src/                      thin snapshot/version/workspace facade

  layerfs-service/
    src/                      durable network/auth boundary

containers/
  layerfs-mount/
    fuse/
      Dockerfile             Linux arm64/x64 build/runtime image

tools/
  layerfs-eval/
    src/
      main.rs                tiny Stage 2 command dispatch
      stage2_fuse.rs         fixed schedules, rows, receipts, reports

poc/
  19-stage2-docker-linux-fuse.md
```

Expected existing product edits are limited to universal missing primitives in:

```text
crates/layerfs-core/src/logical/{mod,resolver,read,mutate,diff,merge}.rs
crates/layerfs-workspace/src/{operation,workspace,direct,driver,quiescence,receipt}.rs
crates/layerfs-mount/src/{lib,session}.rs
crates/layerfs-mount/src/fuse/{mod,daemon}.rs
crates/layerfs-materialization/src/{lib,driver,materialize,capture,refresh,workspace}.rs
crates/layerfs-materialization/src/apfs/*
crates/layerfs-storage/*                     migrated once from current Engine
crates/layerfs-working-store/*               working Operation/Branch policy
crates/layerfs-durable-store/*               independent durable policy
crates/layerfs-sync/*                        explicit accepted-state transfer
crates/layerfs-service/*                     durable network boundary
crates/layerfs-sdk/src/lib.rs                 only after a concrete public caller
```

The Stage 2 source move is capability ownership, not an algorithm rewrite:

```text
current layerfs-vfs/src/mounted.rs
  -> layerfs-workspace contract + layerfs-mount driver/session

current layerfs-vfs/src/{driver,materialize,capture,refresh,workspace}.rs
  -> layerfs-materialization/src/

current layerfs-os/src/apple/*
  -> layerfs-materialization/src/apfs/

remaining current layerfs-vfs resolver/read/mutate/diff/merge semantics
  -> layerfs-core/src/logical/
  -> delete old layerfs-vfs after caller/root/counter conformance

current layerfs-engine storage/version mechanisms
  -> layerfs-storage once
  -> distinct working/durable policy crates compose Storage
```

Core canonical formats/codecs should not change while logical semantic code
moves beside them:

```text
content/extent.rs
content/extent_codec.rs
namespace codecs
inode codecs
metadata codecs
object identity
```

Do not add target `layerfs-fs`, peer `layerfs-fuse`, peer `layerfs-os`,
`layerfs-overlayfs`, a mount framework, backend registry, or separate read/write
representations. FUSE is a submodule of Mount; APFS is a submodule of
Materialization. Current crates with those names remain source evidence until
their migration, not target ownership.

## 8. FUSE dependency selection

Linux FUSE requires one external binding or a handwritten kernel protocol.
Handwriting the protocol is out of scope.

Before source implementation, compare only currently maintained Rust bindings
that provide:

- Linux arm64 and x64 support;
- low-level inode/handle callbacks;
- read/write/readdir/getattr/open/release/flush/fsync;
- rename, unlink, link, symlink, xattrs, and invalidation capability;
- `mmap`/kernel page-cache compatibility;
- bounded request buffers;
- clean unmount/cancellation; and
- no async runtime requirement unless the binding itself requires it.

Select exactly one dependency and record its version/license/API reasons in the
Stage 2 readiness receipt. Keep it target-specific and private to
`layerfs-mount::fuse`. Do not add a generic adapter around multiple FUSE
libraries.

## 9. Stage 2.0 — Docker/FUSE admission

Run before product edits.

Host/container admission:

```text
Docker daemon running
Linux containers available
native linux/arm64 selected on Apple Silicon
/dev/fuse exposed
mount succeeds with the minimum capability set
named volume/native baseline available
clean unmount and container removal succeed
```

Try the narrow capability set first:

```text
--device /dev/fuse
--cap-add SYS_ADMIN
```

Add `MKNOD` only if a concrete device admission requires it. Use
`--privileged` only as a diagnostic control after the narrow route fails; a
privileged-only result is not the preferred product route.

Admission probe:

- one trivial read/write filesystem;
- fixture <=8 MiB;
- one file and one directory;
- enumerate, stat, read, write, truncate, `mmap`, unmount, remount;
- each probe <=60 seconds;
- complete admission <=120 seconds;
- no repository source edit;
- no residual mount, process, container, or volume; and
- exact kernel, architecture, Docker, filesystem, FUSE, and capability receipt.

Do not use `linux/amd64` emulation for accepted Apple-Silicon performance
evidence. It may be retained only as compatibility evidence.

Exit:

```text
PASS
  real Linux FUSE admitted with native architecture and clean lifecycle

NO-GO
  exact external Docker/kernel/device capability absence proven
```

An implementation error is intermediate REVISE, not an external NO-GO.

## 10. Stage 2.1 — read-only LayerFS FUSE

Implement only:

```text
lookup
getattr/fgetattr
readdir
open/opendir
read
readlink
release/releasedir
access
statfs
init/destroy
```

The callback adapter owns:

- FUSE request/response types;
- inode/handle tokens scoped to the mount;
- Linux credential/error translation;
- kernel cache and invalidation calls;
- mount/unmount/cancellation lifecycle; and
- request counters.

`layerfs-core::logical` owns:

- path resolution;
- inode lookup;
- namespace semantics;
- metadata semantics;
- range planning;
- portable logical mutation and Merkle/merge candidate construction; and
- portable candidate construction through generic `ObjectRead`/`ObjectStore`.

`layerfs-workspace` owns the universal private runtime lifecycle and the
`layerfs-mount` driver owns mounted handles/overlay/spool/quiescence.
`layerfs-working-store` owns WorkingRecorded Branch/Operation authority and
delegates exact transactions to `layerfs-storage`. `layerfs-durable-store`
independently owns DurablyAccepted authority after explicit Sync; Sync itself
never moves a head.

Read-only correctness corpus:

- empty and nested directories;
- small and multi-level regular files;
- one 100 MiB file with no intermediate/output above 100 MiB;
- deterministic random ranges;
- sequential reads;
- symlinks without follow confusion;
- regular-file hard links with stable mounted inode identity;
- modes and mtime;
- selected retained historical roots;
- forked roots with divergent content;
- `mmap` exactness;
- concurrent read handles;
- daemon restart and exact reopen; and
- malformed/corrupt object refusal through the existing integrity path.

Hard read gates:

```text
workspace materializations       0
capture scans                    0
ordinary backing user files      0
range complexity                 path + O(log E + X + returned bytes)
payload batch maximum            <=64
largest product buffer           <=1 MiB
operation Q high-water           <=8 MiB
operation Q terminal             0
```

Initial performance targets:

| Operation | Target |
|---|---:|
| 300 deterministic 64 KiB reads | p50 <=1.5 ms; p95 <=3 ms |
| 100 adjacent 1 MiB reads | >=250 MiB/s |
| 100 MiB sequential read | >=250 MiB/s |
| warm exact no-op/open authority | p50 <=5 ms |

An absolute sub-1 ms miss is report-only unless it creates a material aggregate
workspace regression. Preserve the raw result; do not weaken the target after
observation.

## 11. Stage 2.2 — writable mounted workspace

Add only after Stage 2.1 passes:

```text
create
write
truncate/ftruncate
mkdir/rmdir
rename
unlink
link
symlink
chmod
utimens
setxattr/getxattr/listxattr/removexattr where admitted
flush
fsync/fsyncdir
release
```

### 11.1 One universal workspace contract, one mounted driver state

The current `layerfs-vfs::mounted` source owns one mount-wide session pending
its move to `layerfs-mount`:

```text
MountedWorkspace
  accepted RefState
  mounted root
  open inode handles
  dirty inode sessions
  namespace mutations pending for checkpoint
  cache invalidation generation
  lifecycle: Live | Checkpointing | Incomplete
```

Target ownership splits this current structure without copying it:

```text
layerfs-workspace        lifecycle, quiescence, finalization, cleanup receipt
layerfs-mount            mounted inode/handle/dirty overlay/spool driver state
layerfs-core::logical    portable reads/mutations/candidate construction
layerfs-working-store    exact begin pin/recovery + WorkingRecorded publication
layerfs-storage          object rows and transaction mechanics
```

Do not implement state separately inside evaluator, callbacks, SDK,
WorkingStore, or Storage.

### 11.2 Dirty regular-file state

Never keep the complete file in memory. A dirty inode contains:

```text
base FileStateRoot
logical length
ordered/coalesced dirty ranges
bounded payload buffer
disk-backed owned scratch spool after the buffer bound
mode/mtime changes
open-handle count
```

Requirements:

- memory payload buffer <=1 MiB per active operation;
- aggregate Q <=8 MiB;
- no source-sized `Vec`/`Buffer`;
- no all-extents collection;
- scratch files are LayerFS-owned, bounded/accounted, and removed at terminal;
- same-mount readers observe dirty state;
- write order and truncate semantics are exact;
- hard-linked aliases observe one dirty inode state; and
- failure after possibly visible native/kernel state enters `Incomplete` and
  permits only discard/reopen according to the mounted contract.

### 11.3 Working OperationCommit boundary

Ordinary `write`, `flush`, `release`, and tool-issued `fsync` change or persist
only private OperationWorkspace state; they do not advance the Branch per
syscall.

Target version boundary:

```text
explicit workspace finalization
  -> layerfs-core::logical candidate
  -> WorkingStore OperationCommit
```

OperationCommit applies the ordered dirty file and namespace changes through
one expected-Branch-head batch and returns the WorkingRecorded
OperationVersion/BranchHead.

Required equations per state-changing WorkingStore OperationCommit:

```text
transactions_started      1
transactions_committed    1
publication_commits       1
accepted generation       previous + 1
expected root             exact previous accepted root
```

Crash before WorkingRecorded OperationCommit may discard unacknowledged dirty
state; operation-private fsync recovery is governed by the Workspace recovery
record and does not imply a Branch version. After WorkingRecorded
acknowledgement, fresh WorkingStore reopen must expose the accepted root and
exact bytes.

Retained Stage 2 source/evidence maps mounted `fsync` to its internal
`checkpoint`/RefState publication. Keep those historical row labels and timer
boundaries unchanged. The target universal Workspace migration routes private
fsync behind OperationCommit rather than reinterpreting old evidence.

## 12. Stage 2.3 — count-changing and locality proof

FUSE/POSIX `write` means overwrite, not insert. Stage 2 must not infer an
insertion from an ordinary same-offset write.

Test three distinct classes.

### 12.1 Explicit logical splice

A companion LayerFS control/SDK operation supplies:

```text
path
offset
delete length
replacement bytes
```

The mounted view must invalidate the affected inode/ranges and immediately
show the accepted new root.

Hard gates:

```text
logical edit work                 O(B + log E + path)
CDC bytes                         replacement/boundary bytes only
unaffected suffix payload reads   0
unaffected suffix payload writes  0
content directory nodes emitted   0
physical workspace shift          0
materialization                   0
capture                           0
```

Initial target:

```text
durable 4 KiB explicit insert/delete p50 <=10 ms
```

### 12.2 Ordinary overwrite/append/truncate

These FUSE semantics are explicit and must remain local to the changed ranges
or affected tail.

Targets:

| Operation | Target |
|---|---:|
| mounted 4 KiB write acceptance | p50 <=2 ms |
| durable 4 KiB checkpoint | p50 <=8 ms |
| append/truncate checkpoint | p50 <=8 ms |

### 12.3 Opaque temp-file/full replacement

When an editor submits a complete replacement stream:

```text
input processing = Theta(submitted bytes)
```

LayerFS may recover chunk sharing, but it must not claim local input work. The
result row reports submitted bytes, CDC bytes, new/reused objects, and whether
the application supplied ancestry or change intent.

## 13. Stage 2.4 — real container workspace

Use the bounded offline developer-workspace workload retained in document 15
as fixture input only, adapted directly to Linux and the mount. Stage 1.2 is
skipped: no APFS execution or accepted APFS baseline is required. Do not import
its APFS reset/capture mechanism into the mounted route.

Budgets:

```text
complete workspace logical bytes   <=300 MiB
largest regular file               <=100 MiB
network during measured run        0
prepared image/fixture             reusable and sealed
complete focused campaign          preferred <=60 s; hard <=120 s
```

Targets in the same Linux VM/container class:

```text
A  native volume workspace control
B  LayerFS FUSE workspace
```

Do not use a macOS bind mount for either target. The control and LayerFS Store
must reside inside the same Linux VM/storage class.

Workload must include:

- git status/init/commit and history reads;
- offline npm install from the sealed cache/fixture;
- directory enumeration and search;
- build and focused tests;
- create/remove/rename many small files;
- modes, mtime, symlink, and hard-link operations;
- one 64 MiB sequential read/write/copy control;
- one 100 MiB direct range/read control;
- same-size edit, append, truncate, and explicit splice;
- multi-write save burst followed by one checkpoint;
- exact no-op checkpoint;
- branch/fork, divergence, rollback, and historical read;
- daemon/container restart and exact reopen; and
- final resource/mount/residue closure.

Initial aggregate target:

```text
LayerFS FUSE developer-workspace wall <=1.5x native Linux control
```

Cloudflare Computer's published full npm result is approximately 2x ext4 in a
different environment. It is a research comparison, not a LayerFS acceptance
baseline. Stage 2 must publish its own same-host raw rows.

## 14. Linux mounted profile

Stage 2 qualifies a narrow `LinuxContainerWorkspaceV1` profile:

Required:

- regular files;
- nested directories;
- symlinks;
- regular-file hard links;
- read/write/executable mode bits;
- canonical mtime;
- exact rename/unlink-open behavior within the selected FUSE/kernel support;
- `mmap` for regular files;
- advisory/byte-range locks needed by the frozen workload;
- exact checkpoint/reopen; and
- typed errors for unsupported required metadata.

Deferred:

- device nodes, sockets, FIFOs, and paging files;
- setuid/setgid/capability semantics;
- complete Linux ACL/security-label qualification;
- NFS export;
- hostile multi-user mount security;
- multiple writer containers; and
- production UID/GID remapping.

Canonical Apple extension metadata remains readable as canonical data but may
return typed `UnrepresentableMetadata` when exact Linux projection is required.
No platform metadata is silently discarded.

## 15. Correctness and fault suite

Focused tests must cover:

### Read faults

- missing/corrupt payload;
- malformed extent/namespace/inode/metadata object;
- wrong canonical role;
- stale handle after accepted root switch;
- invalid range and overflow;
- symlink loop and no-follow boundaries; and
- deleted/open inode behavior.

### Write faults

- partial dirty-spool write;
- dirty-range overflow/bound violation;
- failed checkpoint before COMMIT;
- ambiguous COMMIT reconciliation;
- stale expected head;
- hard-link alias mutation;
- rename/unlink conflict;
- `fsync` failure;
- daemon death before and after accepted publication; and
- scratch cleanup failure without destructive cleanup.

### Lifecycle faults

- unmount with clean handles;
- refused unmount with admitted dirty policy where required;
- forced daemon exit;
- container stop/restart;
- Store reopen;
- old root readability;
- exact ref generation; and
- no leaked mount, process, FD, connection, scratch file, journal, WAL, or SHM.

## 16. Counters and evidence

Every measured row must include:

- exact source/executable/container image identity;
- Docker, kernel, architecture, FUSE, backing filesystem, SQLite, and Rust
  environment;
- callback counts by opcode;
- path/inode/extent/object counters;
- SQL statements, fetched rows, authentication, role decode, new/reused object
  counters;
- payload batches/references;
- dirty-buffer/spool bytes and high-water;
- accepted `RefState` before/after;
- transactions and COMMITs;
- cache invalidations;
- materialization and capture counters, both required literal zero in mounted
  workflows;
- native control bytes/timing;
- RSS/Q/FD/connections/children/mounts/temp residue;
- raw ordered observations;
- correctness oracle; and
- complete timer equation.

Unavailable is `null` plus an exact reason, never zero.

## 17. Resource gates

In the target ownership, `layerfs-workspace` charges userspace `Q` and terminal
lifecycle state; `layerfs-mount` charges dirty spool/handles/mount resources;
`layerfs-storage` charges SQLite connections/journals. The retained numeric
gates and measurements below do not change.

```text
largest product payload buffer       <=1 MiB
operation Q high-water               <=8 MiB
operation Q terminal                 0
RSS above settled baseline           <=64 MiB
payload batch references             <=64
Store connections high-water         <=2
Store connections terminal           0
FD terminal                          baseline
child processes terminal             0
mounted filesystems terminal         baseline
owned scratch/temp terminal          0
SQLite journal/WAL/SHM terminal      0
network operations in measurement    0
workspace materializations           0
workspace capture scans              0
```

Memory-growth tests must vary both file extent count and workspace file count.
Local O(E), O(F), or whole-file-buffer growth fails.

## 18. Fast iteration and measurement policy

Implementation loop:

```text
one touched root cause
  -> one focused reduced test
  -> touched-crate format/check
  -> continue
```

Do not repeatedly:

- build the full Docker image;
- run the 300 MiB workspace;
- run all workspace tests;
- pull npm/network fixtures;
- recreate the Store fixture; or
- rerun unchanged source for timing noise.

Source-freeze sequence:

1. focused product and real-FUSE tests pass;
2. one workspace fmt/check/test/clippy closure;
3. one release Linux build;
4. one sealed container image;
5. zero-row readiness and fixed schedule;
6. one admitted measured campaign;
7. one independent correctness/performance/resource audit.

No measured file exceeds 100 MiB. No measured workspace exceeds 300 MiB.

## 19. Continuation policy

Compile, FUSE callback, container, correctness, performance, and resource
failures caused by implementation are intermediate.

For each failure:

1. preserve the failing command, row, counters, and first failed equation;
2. identify the shared product owner;
3. make the smallest clean repair;
4. add one focused regression test;
5. rerun only the focused proof;
6. re-freeze changed source before a new campaign; and
7. continue until PASS or a concrete external Docker/kernel impossibility is
   proved.

Never:

- weaken a threshold after observation;
- replace failed rows;
- relabel an opaque replacement as local splice;
- report unavailable as zero;
- bypass authentication/durability;
- use `--privileged` performance as if it proved the narrow production route;
- substitute the physical shim for FUSE;
- move the workspace onto OverlayFS to avoid a LayerFS defect; or
- stop at an implementation-caused REVISE.

## 20. Terminal disposition

Stage 2 may claim PASS only when:

- Docker/FUSE admission passes on native architecture;
- read-only and writable mounted routes use actual LayerFS product paths;
- exact regular/directory/symlink/hard-link/metadata behavior passes;
- `mmap`, reopen, history, fork, rollback, and conflicts pass;
- explicit splice proves zero unaffected suffix payload work;
- mounted workflows perform zero materialization and zero capture;
- performance and aggregate native-control targets pass or retain an honest
  user-accepted exception without relabeling;
- every resource gate passes;
- the final container unmounts cleanly with no residue;
- raw results and timer equations close; and
- OCI, remote, Windows, macFUSE, and FSKit remain unstarted.

Terminal REVISE preserves the smallest source-backed repair list and does not
authorize a scope expansion.

## 21. Post-Stage-2 sequence

Only after this direct Linux mount passes:

```text
Ownership migration
  extract layerfs-workspace + mount/FUSE + materialization/APFS first
  move residual portable VFS semantics directly into layerfs-core::logical
  delete old VFS; never create layerfs-fs
  extract layerfs-storage and layerfs-working-store

Mandatory durability path
  layerfs-durable-store independent admission
  explicit layerfs-sync Fetch/Push
  layerfs-service durable network boundary
  fresh-host DurablyAccepted recovery

Then, when requested
  OCI import/export
  containerd snapshotter and Docker workflow

Later, only with demand
  Windows WinFsp
  direct Apple FSKit
```

Sync transfers accepted canonical/version state and exact head requests only;
it does not copy Cloudflare Computer's mutable path-revision model or any live
workspace state.

## 22. Final planning statement

The next mounted LayerFS implementation is not a macOS mount and not an
OverlayFS workspace. It is:

```text
Docker/Linux
  + direct LayerFS FUSE at /workspace
  + layerfs-workspace + layerfs-mount concrete driver
  + WorkingStore/layerfs-storage on the Linux volume
  + portable semantics in layerfs-core::logical
  + ordinary tools inside the container
  + one WorkingRecorded OperationCommit boundary
```

The final Stage 1.1 closure is the immediate predecessor. This document defines
the first direct-mounted WorkingStore proof; Stage 1.2 is not an entry gate or
required baseline. Product readiness additionally requires explicit Sync and
independent DurableStore acceptance under the controlling architecture.
