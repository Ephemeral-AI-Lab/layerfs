# Stage 2P Specification — Portable LayerFS FUSE Performance Optimization

Status: **implemented and closed at source-bound `PASS_OPTIMIZED`**

The terminal result is [candidate 012](evidence/stage2-freeze-candidate-012/summary.md),
using the unchanged upstream benchmark, the exact 12-scenario filter, and the
controlling Docker `--cpus 1` quota envelope. Candidate 011's cpuset numbers
are retained only as non-authoritative diagnostics.

Prepared: `2026-08-26`

Post-Stage-1.1 closure review: `2026-08-26`

## 0. Decision

Stage 2P optimizes the direct mounted LayerFS path defined by
[19 — LayerFS + Linux FUSE](19-stage2-docker-linux-fuse.md). It does not replace
that document's canonical, durability, correctness, metadata, history, or fault
requirements. It narrows the performance design, resource limits, portability
boundary, benchmark targets, and repair order using the completed native
Cloudflare FUSE baseline.

The selected route is:

```text
ordinary POSIX tool
  -> platform mount adapter
  -> one portable layerfs-vfs::mounted session
  -> existing LayerFS Core + Engine
  -> one SQLite/CAS Store
```

For the first qualified adapter:

```text
Linux kernel VFS
  -> one selected FUSE binding
  -> thin layerfs-fuse translation
  -> layerfs-vfs::mounted
```

The performance-critical law is:

```text
write/create/rename/unlink/truncate
  -> bounded mounted dirty state
  -> zero publication
  -> zero COMMIT

flush/release/close
  -> handle and error cleanup only
  -> zero publication
  -> zero COMMIT

fsync/fsyncdir/explicit checkpoint/graceful dirty daemon shutdown
  -> one ordered mount-wide mutation batch
  -> one expected-head Publication
  -> one visibility COMMIT
```

An implementation that publishes on ordinary `write`, `flush`, or `release`
is not admitted to the complete performance campaign.

## 1. Authority and evidence boundary

Authority order:

1. [10 — Handoff freeze](10-handoff-freeze.md) controls canonical identity,
   authentication, publication, history, durability, and compaction.
2. [09 — Portability](09-portability-and-apple-completeness.md) controls the
   platform dependency direction and typed unsupported behavior.
3. [19 — LayerFS + Linux FUSE](19-stage2-docker-linux-fuse.md) controls mounted
   correctness, supported operations, fault behavior, and Stage 2 scope.
4. [Final Stage 1.1 terminal audit](evidence/stage1.1-terminal-audit-20260826/summary.md)
   freezes the current reusable product source, resource/counter behavior, and
   handoff boundary.
5. [Cloudflare Docker/FUSE handoff](../../cloudflare-computer-bench/cloudflare-docker-fuse-layerfs-handoff.md)
   supplies the completed comparison environment, unchanged workload, raw
   medians, evidence rules, and cleanup lessons.
6. This document controls Stage 2 performance architecture, optimization
   sequence, targets, counters, and performance dispositions.

### 1.1 Final Stage 1.1 closure state

The closed task binds:

```text
closure/docs commit
  30597059d563f39113d7b69017146a70a7437e1a

final product source
  d1848200d249915d3f1e35af5556fdf6c1ec05c6

release SHA-256
  b056b535c7d3e0711a120731e414bbff213ca0be9c6a603cc3387da6633af624

release BLAKE3
  8fe897685cda24c850d58c35e27687a02389747232fcc862337bcf7de234ef01

final regression
  attempt-024 independently audited PASS: 47 rows / 51 edits / 34 transitions
```

Its exact disposition is:

```text
Stage 1.1 correctness and durability       PASS / closed
Verified Stage 1.1M performance            REVISE_NO_AUTHORIZED_OWNER
TrustedLocalDev                            PASS_PRIMARY_TRUSTED_CLASS
Stage 1.2                                  SKIPPED / not a gate
Docker/FUSE                                not started
```

The final current-source resource witness is:

```text
RSS / largest buffer             28,442,624 / 1,048,576 bytes
Q high / terminal                8,388,607 / 0 bytes
Store connections high/terminal 2 / 0
FD baseline/terminal             5 / 5
BUSY / LOCKED / residue          0 / 0 / 0
transactions / COMMITs           34 / 34; rollback 0
```

This closes the Stage 1.1 correctness predecessor and confirms the explicit
TrustedLocalDev classification used by Stage 2P. The explicit user decision on
`2026-08-26` skips Stage 1.2 and removes it as a predecessor. Stage 2P is the
direct successor to the final Stage 1.1 closure.

No Stage 2P performance target changes from this closure. TrustedLocalDev
attempt-003 remains a separately frozen APFS-materialization population, not
final-source or FUSE performance evidence; the later final-source changes close
correctness and accounting without authorizing a Trusted performance rerun.

Controlling Cloudflare evidence:

```text
source commit
  de87919a4fd37242e960e13b7b3ba802d1eef0a0

fs-bench SHA-256
  0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef

attempt directory
  /Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/
    evidence/docker-fuse-attempt-2

attempt SHA256SUMS SHA-256
  38497037b0b871c7794b77ee11486c971a0471d0920aeb518367954697551bbf
```

Measured facts and forecasts must remain separate:

```text
Measured
  Cloudflare source, environment, FUSE admission, callback trace,
  scenario medians, controls, ratios, resources and cleanup

Forecast
  every LayerFS first-pass, optimized and stretch number in this document

Accepted LayerFS result
  only a later source/image/fixture-bound real-FUSE campaign
```

No forecast is a LayerFS performance claim.

## 2. Scope and non-goals

Stage 2P specifies only:

- the portable mounted-session boundary;
- the thin Linux FUSE adapter boundary;
- bounded pending namespace, inode, dirty-range and spool state;
- inode-oriented reads and metadata access;
- checkpoint batching and exact durability acknowledgement;
- kernel-cache and directory-enumeration policy;
- the unchanged 12-scenario offline `fs-bench.sh` campaign;
- same-environment overlay and tmpfs controls;
- CPU, memory, disk-scratch and residue gates; and
- the optimization/repair ladder needed to reach the target envelope.

Stage 2P does not authorize:

- a physical workspace mirror, polling shim, watcher, or capture loop;
- OverlayFS as the LayerFS workspace;
- an SDK-only benchmark path;
- a benchmark-only in-memory filesystem;
- per-callback publication;
- a worker pool, async runtime, connection pool, or second writer;
- multiple FUSE libraries or a mount-backend registry;
- a handwritten FUSE kernel protocol;
- a generic cross-platform mount framework before a second adapter exists;
- macFUSE, FSKit, WinFsp, production packaging, or remote synchronization;
- weakening Verified or TrustedLocalDev authentication rules;
- changing canonical object/namespace/inode/metadata codecs; or
- implementing `copy_file_range` before ordinary copy measurements identify it
  as a material owner.

## 3. Minimal repository and runtime shape

Expected source ownership:

```text
Cargo.toml
  add one layerfs-fuse workspace member

crates/layerfs-core/
  reuse canonical namespace/inode/rope primitives
  add one bounded directory_page_after(root, after_name, budgets) primitive
  add one keyed metadata_lookup(root, key) primitive for mode/mtime

crates/layerfs-engine/
  reuse Engine, RefState and Publication
  add no FUSE type, callback or platform lifecycle

crates/layerfs-vfs/src/
  lib.rs
  resolver.rs
  mounted.rs
    MountedWorkspace
    mount-scoped node and handle identities
    pending namespace/inode state
    dirty regular-file ranges and spool references
    inode-oriented read/metadata operations
    checkpoint and invalidation output
    mounted resource counters

crates/layerfs-fuse/
  Cargo.toml
  src/lib.rs
    FUSE callback translation
    semantic error -> errno mapping
    cache invalidation
  src/main.rs
    minimal daemon and mount lifecycle
  tests/mounted_routes.rs
    focused real-FUSE container oracle

tools/layerfs-eval/src/
  main.rs
  stage2_fuse.rs
    fixture/evidence orchestration and validation only

containers/layerfs-fuse/
  Dockerfile
    one sealed native Linux build/runtime environment

crates/layerfs-os/
  unchanged

crates/layerfs-sdk/
  unchanged until a real mounted-control caller exists
```

Do not split `mounted.rs` or the adapter further until file size or ownership
actually makes the split useful.

The daemon opens `MountedWorkspace` directly from Store path, integrity mode,
ref name and runtime directory. It does not route through the current
Apple-oriented SDK or enlarge `ManagedWorkspace` replay into a second mounted
model. Mounted checkpointing reuses `Publication`, namespace/inode primitives,
rope `build`/`replace`, metadata helpers, and existing codecs directly.

The final Stage 1.1 source already supplies guarded authenticated reads, the
admitted StoreId cache, consolidated authenticated scratch ownership,
`ProjectionFacts`, `OperationCounters`, `EngineCounters`, and
`StorageObservation`. Stage 2P reuses those owners and extends them only with
facts unique to mounted/FUSE state. It does not fork their SQL, authentication,
scratch, storage, or projection accounting into the daemon or evaluator.

Runtime layout:

```text
/workspace
  real LayerFS FUSE mount; no bind or Docker volume at this destination

/var/lib/layerfs/store.sqlite
  authoritative Store on Docker-local storage

/var/tmp/layerfs-owned/mount-spool
  LayerFS-owned non-authoritative dirty spool on the same observed storage
  class as the /var/tmp control and outside every benchmark directory

/var/tmp
  observed Docker Linux-VM storage control

/tmp
  explicit 1 GiB tmpfs control
```

The exact spool filename may be implementation-owned. Store and spool must not
be placed below `/workspace`, and measured evidence must not be written into a
benchmark target. If the spool cannot use the `/var/tmp` control storage class,
add and report a separate control on the spool's actual volume; otherwise the
large-I/O ratios are storage-class confounded.

## 4. Portability contract

### 4.1 Portable owner

`layerfs-vfs::mounted` owns filesystem semantics and must compile without:

```text
FUSE request/reply types
Linux errno values
libc calls
host inode numbers
Docker paths
cfg(target_os)
Apple, Linux or Windows metadata syscalls
```

It uses existing LayerFS identities and portable Rust values:

```text
RefState
ObjectId
InodeId
CanonicalName / CanonicalPath
InodeRecordV1
FileStateRoot / ReadPlan
portable mode and mtime metadata
bounded byte slices, readers, writers and ranges
```

The mounted API should be concrete, not a trait with one implementation. A
future FSKit or WinFsp adapter can call the same concrete API when such an
adapter is authorized.

### 4.2 Adapter owner

`layerfs-fuse` owns only:

- FUSE inode and handle tokens;
- credentials and access checks selected for the Linux profile;
- semantic error to errno translation;
- negotiated request sizes and supported kernel capabilities;
- entry/attribute/page-cache policy;
- exact kernel invalidations after accepted root changes;
- mount, signal, cancellation and unmount lifecycle; and
- cheap callback totals.

Platform adapters may contain platform-specific code. Core, Engine, VFS and
SDK may not gain platform branches merely to add an adapter.

### 4.3 Portable semantic errors

Mounted operations require semantic errors such as:

```text
NotFound
AlreadyExists
NotDirectory
IsDirectory
NotEmpty
InvalidName
InvalidRange
PermissionDenied
ReadOnly
NoSpace
TooManyOpenFiles
ResourceExhausted
Busy
StaleHandle
InvalidHandle
Conflict
Unsupported
Corrupt
Indeterminate
```

The Linux adapter maps them to errno. The portable VFS must not return Linux
errno or collapse expected POSIX distinctions into generic `InvalidState`.

### 4.4 Platform matrix

| Layer | Linux Stage 2 | macOS current | Future Apple mount | Future Windows mount |
|---|---|---|---|---|
| Core/Engine | unchanged | unchanged | unchanged | unchanged |
| Mounted VFS | implemented and qualified | compiles; not the APFS projection route | reused | reused |
| Adapter | `layerfs-fuse` | none | separate FSKit bridge/package | separate WinFsp adapter |
| Native projection | not used for `/workspace` | existing APFS driver | remains available | future driver only if required |
| Platform metadata | typed Linux projection | exact admitted Apple projection | typed mapping | typed mapping |

Portability means the filesystem state machine and durable representation are
shared. It does not mean one adapter binary or one syscall set runs everywhere.
The Linux `4.500 s` wall target does not transfer to another platform. A new
adapter reuses semantic, complexity, durability and resource tests, then
publishes new absolute medians and same-host normalized ratios.

The first mounted name/metadata profile is intentionally narrower than all of
Linux:

```text
names                 canonical UTF-8 only
backslash             rejected by the existing canonical profile
component/path/depth  existing LayerFS limits
uid/gid               fixed synthetic mount identity; not canonical metadata
chown                  Unsupported
Linux xattrs           Unsupported in v1
portable metadata      existing mode + mtime
Apple extension data   preserved canonically; typed unrepresentable on Linux
```

The adapter must return typed errors for non-UTF-8, backslash-containing, or
over-limit names. It must not normalize, drop, or silently accept unsupported
names or metadata. Adding Linux xattr domains is separate canonical-format
work and is not required by the frozen benchmark.

## 5. Mounted-session model

One mount owns one state machine:

```text
MountedWorkspace
  accepted RefState
  mounted root
  lifecycle: Live | Checkpointing | Incomplete

  mounted node map
    MountedNodeId -> canonical InodeId or pending identity
    cached inode record and metadata
    optional shared Arc<ReadPlan>
    kernel lookup reference count
    open handle reference count

  open file and directory handles
  per-directory changed-entry maps
  pending inode records
  per-inode dirty regular-file state
  one owned append spool
  cache invalidation generation
  operation and resource counters
```

Mounted inode ownership is bounded by:

```text
forget / batch_forget
  -> decrement kernel lookup references

release / releasedir
  -> decrement open-handle references

lookup references == 0
and open references == 0
and node is not dirty/pending
  -> reclaim mounted node and cached ReadPlan
```

Hard-link aliases share one canonical or pending inode session. Open-unlinked
files remain addressable through their live handles until final release.

Pending nodes use stable mount-local `MountedNodeId` values because canonical
`InodeId` allocation occurs inside `Publication`. A pending node carries
`canonical_inode=None`; the checkpoint allocates IDs for surviving nodes,
rewrites their pending references, and retains the same `MountedNodeId` after
acceptance. FUSE `ino`/`fh` values remain adapter-owned and never truncate or
hash canonical `InodeId` into an assumed collision-free `u64`.

The first mounted profile admits no more than:

```text
live mounted node records             65,536
open file + directory handles          8,192
dirty + pending nodes                 32,768
dirty-range descriptors               65,536 mount-wide
live directory cursors                 4,096
inflight mounted callbacks                 8
FUSE workers in the one-CPU campaign       4
all daemon threads                          8
```

Reject before partial mutation when a limit is exhausted. Use semantic
`TooManyOpenFiles`, `ResourceExhausted`, or `NoSpace` errors for adapter
translation; do not allocate optimistically and repair after failure.

The callback route is inode-oriented:

```text
lookup_child(parent_node, name)
getattr(node)
open(node)
read(node, offset, length)
create(parent_node, name)
write(node, offset, bytes)
unlink(parent_node, name)
checkpoint()
```

After `lookup`, ordinary `read`, `write`, and `getattr` must not rebuild a full
path and resolve it again from the namespace root.

## 6. Dirty data and cancellation

Per dirty regular file:

```text
DirtyFile
  optional base FileStateRoot
  logical length
  ordered/coalesced dirty ranges
  references into one owned disk spool
  mode/mtime changes
  open-handle count
```

Requirements:

- never retain a complete large file in a `Vec`;
- never collect all file extents;
- one callback payload is at most 1 MiB;
- aggregate operation Q is at most `8 * 1024 * 1024 - 1` bytes;
- mount-wide dirty payload resident in RAM is at most 8 MiB, independent of
  operation Q;
- excess dirty payload spills to the accounted mount spool before acknowledgement;
- sequential tail writes coalesce in amortized `O(1)` range work;
- arbitrary overlap is `O(log R + overlapped ranges)` for `R` dirty ranges;
- same-mount reads merge dirty spool ranges and unchanged canonical ranges;
- new pending files need no canonical objects before checkpoint;
- dirty state is shared across hard-link aliases; and
- spool failures enter a typed non-durable failure or `Incomplete`; they never
  produce an acknowledged durable result.

The existing observational `OperationQ` reservation is not enforcement. The
mounted route must reserve the actual bytes before allocation and block on a
condition variable when the byte budget is unavailable; it must never spin.
The initial mount-wide lock may serialize the bounded product operation, but
queued FUSE requests still require explicit inflight and byte accounting.
Account adapter request/response copies, mounted temporary buffers, current
dirty inline bytes, payload-batch staging, and spool-copy buffers. SQLite and
kernel page caches are observed separately rather than hidden inside Q.

The unchanged benchmark creates a fresh `.bench` subtree and removes it after
each run. Cancellation is therefore load-bearing:

```text
create pending node
  -> mutate pending node
  -> remove pending node before checkpoint
  = no persistent inode allocation
  = no canonical object write
  = no SQLite transaction
  = no COMMIT
```

When the last dirty operation cancels:

```text
pending entries           0
dirty nodes/ranges        0
spool live bytes          0
spool file length         0
operation Q               0
```

Cleanup is outside each scenario timer, but it is still product work. It must
finish before the next scenario and must not leak deferred state across rows.

## 7. Durability and checkpoint contract

POSIX `flush` and `release` do not imply stable-storage durability. They must
not publish merely because a file descriptor closed.

Acknowledged durability boundaries are:

```text
fsync
fsyncdir
explicit mount checkpoint
graceful daemon shutdown while dirty
```

`O_SYNC`/`O_DSYNC` behavior must be classified and implemented before those
flags are advertised as supported. Until then the adapter returns an honest
typed unsupported/error rather than silently weakening them.

The first profile gives `fsync`/`fdatasync`/`fsyncdir` a stronger but simpler
meaning: checkpoint the entire mounted dirty workspace. It does not attempt a
partial file-local Publication.

One dirty checkpoint performs:

1. freeze the ordered mounted mutation set;
2. stream new/completely dirty files through the existing rope `build` path;
3. apply coalesced partial changes through the existing rope `replace` path;
4. apply all inode and namespace changes to one `Publication`;
5. publish the namespace once;
6. COMMIT once;
7. return the newly accepted `RefState`; and
8. issue exact adapter invalidations.

Required equations:

```text
dirty successful checkpoint:
  transactions_started       = 1
  transactions_committed     = 1
  transactions_rolled_back   = 0
  publication_commits        = 1
  accepted_generation_after  = accepted_generation_before + 1
  expected_root              = accepted_root_before

exact no-op checkpoint:
  transactions_started       = 0
  publication_commits        = 0
  objects_written            = 0
  CDC bytes                  = 0

ordinary callback path:
  write-triggered publications    = 0
  flush-triggered publications    = 0
  release-triggered publications  = 0
```

Ambiguous COMMIT outcomes use the existing fresh independent reconciliation
law. No adapter may infer success from elapsed time or in-memory state.

Mounted reconciliation transitions are exact:

```text
candidate RefState observed exactly
  -> accept candidate
  -> clear the included dirty set
  -> durability request succeeds

old expected RefState observed exactly
  -> retain the dirty set
  -> durability request fails
  -> retry remains possible

different or indeterminate RefState
  -> lifecycle = Incomplete
  -> reject every later mutation
  -> require explicit discard/reopen/remount
```

The receipt records expected, candidate, freshly observed state and the final
lifecycle. Mapping the first error to `EIO` and continuing writable is forbidden.

After acknowledged `fsync`, a fresh daemon/container reopen must expose the
accepted root and exact bytes. Unacknowledged dirty state may be discarded on
crash and must never be called durable.

## 8. Performance design by scenario

The sealed Cloudflare post-smoke trace shows the callback amplification owner:

| Callback | Calls | p50 | Total callback wall |
|---|---:|---:|---:|
| `create` | 4,007 | 0.215 ms | 889.3 ms |
| `release` | 4,017 | 0.071 ms | 395.2 ms |
| `unlink` | 4,008 | 0.086 ms | 368.7 ms |
| `getattr` | 14,090 | 0.026 ms | 363.8 ms |
| `write` | 5,030 | 0.015 ms | 111.3 ms |
| `read` | 1,029 | 0.038 ms | 45.6 ms |

LayerFS does not copy Cloudflare's implementation. The trace only establishes
that tens of thousands of callbacks make small per-callback costs material.

| Scenario | Required LayerFS route | Disallowed hot owner |
|---|---|---|
| create 1,000 | pending inode/name plus small spool write | persistent ID/object/publication per close |
| stat 1,000 | mounted node/attribute cache | root/path/SQLite resolution per stat |
| remove 1,000 | pending create/delete cancellation | canonical removal of never-published nodes |
| mkdir tree | pending directory maps | canonical path-copy per mkdir/touch |
| find tree | bounded resumable merged iterator with file type | whole-directory `Vec`, O(D^2) restart, extra getattr |
| write 64 MiB | bounded sequential spool writes and tail coalescing | whole-file RAM, tiny negotiated writes, one range per fragment |
| copy 64 MiB | ordinary bounded read/write first | speculative extent-sharing implementation |
| read 64 MiB | dirty spool/page-cache read | accidental canonical path/SQL read for a pending file |
| pure read 64 MiB | staged dirty/kernel-cache read | `direct_io` or false persisted-read claim |
| pure copy 64 MiB | staged source to pending destination | premature `copy_file_range` framework |
| overwrite 64 MiB | replace/coalesce live dirty range | unbounded dead spool/range retention |
| Git commit | pending namespace/inodes; honor real Git fsync | per-object transaction or suppressed fsync |

The `pure read` and `pure copy` inputs are staged through the mount immediately
before timing. Those rows are mounted-working-set measurements, not persisted
Store reconstruction measurements.

A separate prerequisite proves the persisted path:

```text
persisted 100 MiB root
  -> fresh daemon/open
  -> sequential extent-tree read
  -> deterministic random ranges
  -> exact byte oracle
  -> sequential throughput >= 250 MiB/s
  -> zero materialization/capture
```

Persisted-path gates inherited from Stage 2:

```text
300 deterministic 64 KiB reads     p50 <= 1.5 ms; p95 <= 3 ms
100 adjacent 1 MiB reads            >= 250 MiB/s
100 MiB sequential read             >= 250 MiB/s
durable 100 MiB create/checkpoint    <= 400 ms
```

## 9. Performance target envelope

### 9.1 Comparison environment

The first authoritative comparison reproduces:

```text
platform              linux/arm64
CPU                   1
memory                3 GiB
PID limit             512
network               none
FUSE                  real /dev/fuse + CAP_SYS_ADMIN
privileged            false
/tmp                  1 GiB tmpfs
workspace             real /workspace FUSE; no bind/volume
integrity             explicit TrustedLocalDev for the local developer loop
trace                  disabled during authoritative rows
```

TrustedLocalDev must remain explicitly labeled. New objects and incumbent
reuse remain authenticated. Promotion to Verified still requires close,
Verified reopen, and a complete retained-union scrub.

The `4.495 s` row-budget sum and `4.500 s` acceptance target apply only to the
explicitly opened, immutable `TrustedLocalDev` developer-loop class. A separate
Verified mounted population
must pass correctness and resource qualification, but its performance is
reported rather than compared with this target until a Verified target is
preregistered. TrustedLocalDev and Verified rows are never pooled.

### 9.2 Measured Cloudflare control and LayerFS budgets

All LayerFS numbers below are preregistered forecasts.

| Scenario | Cloudflare FUSE median | Overlay median | First-pass midpoint | Optimized budget | Optimized ratio budget | Stretch budget |
|---|---:|---:|---:|---:|---:|---:|
| create 1,000 | 858.3 ms | 17.9 ms | 650 ms | 300 ms | 16.8x | 180 ms |
| stat 1,000 | 1,463.6 ms | 516.1 ms | 1,050 ms | 900 ms | 1.74x | 775 ms |
| remove 1,000 | 1,085.2 ms | 23.6 ms | 850 ms | 400 ms | 16.9x | 235 ms |
| mkdir tree | 1,350.0 ms | 459.4 ms | 1,000 ms | 850 ms | 1.85x | 690 ms |
| find tree | 1,401.9 ms | 467.2 ms | 1,100 ms | 950 ms | 2.03x | 700 ms |
| write 64 MiB | 83.8 ms | 14.9 ms | 120 ms | 80 ms | 5.37x | 60 ms |
| copy 64 MiB | 309.6 ms | 27.5 ms | 350 ms | 250 ms | 9.09x | 140 ms |
| read 64 MiB | 163.5 ms | 22.1 ms | 180 ms | 130 ms | 5.88x | 90 ms |
| pure read 64 MiB | 74.1 ms | 6.7 ms | 70 ms | 55 ms | 8.21x | 35 ms |
| pure copy 64 MiB | 231.9 ms | 13.7 ms | 240 ms | 180 ms | 13.14x | 90 ms |
| overwrite 64 MiB | 144.1 ms | 10.3 ms | 120 ms | 100 ms | 9.71x | 65 ms |
| Git commit 100 | 451.9 ms | 13.3 ms | 350 ms | 300 ms | 22.6x | 200 ms |

Aggregate equations:

```text
Cloudflare measured median sum
  = 7.617910211 s

Cloudflare overlay control median sum
  = 1.592609711 s

Cloudflare summed-median ratio
  = 7.617910211 / 1.592609711
  = 4.7833x

Cloudflare geometric mean of row ratios
  = exp(mean(ln(FUSE median / control median)))
  = 10.7182x

LayerFS first-pass midpoint sum
  = 6.080 s

LayerFS first-pass ratio / geometric-mean forecast
  = 3.8176x / 9.6332x

LayerFS optimized median budget sum
  = 4.495 s

LayerFS optimized summed-median ratio budget
  = 4.495 / 1.592609711
  = 2.8224x

LayerFS optimized geometric-mean ratio budget
  = 6.9470x

LayerFS stretch median budget sum
  = 3.260 s

LayerFS stretch summed-median ratio budget
  = 3.260 / 1.592609711
  = 2.047x
```

Normative aggregation for scenario `i`:

```text
mL[i]   = median LayerFS FUSE sample
mB[i]   = median same-invocation control sample
xL[i]   = maximum LayerFS FUSE sample
r[i]    = mL[i] / mB[i]

SL      = sum(mL[i])
SB      = sum(mB[i])
Rsum    = SL / SB
G       = exp(mean(ln(r[i])))
Spread  = sum(xL[i]) / SL
```

Never aggregate with the mean of row ratios, sum of row ratios, the upstream
mean-based printed ratio, or numerators/denominators from different populations.

The optimized forecast is approximately 41% lower wall than the measured
Cloudflare median sum, or about `1.69x` faster. It is not accepted until the
LayerFS campaign exists.

Create/stat/remove/mkdir/find/Git account for `2,910.904 ms`, or `93.21%`, of
the forecast `3,122.910 ms` savings. Namespace/handle work is therefore
load-bearing; a copy fast path is not.

### 9.3 Performance disposition

`PASS_OPTIMIZED` requires:

- every correctness, durability, memory, CPU and residue gate passes;
- the unchanged overlay population contains exactly 24 valid rows;
- the unchanged tmpfs population separately contains exactly 24 valid rows;
- LayerFS overlay-control scenario median sum is `<=4.500 s`;
- LayerFS summed overlay median ratio is `<=2.85x`;
- the geometric mean of LayerFS per-row overlay ratios is `<=7.0x`;
- overlay `Spread` is `<=1.15x`;
- every LayerFS overlay row ratio is `<=1.10x` the matching Cloudflare overlay
  row ratio;
- the separate tmpfs population has `SL<=4.500 s`, `Rsum<=3.10x`, `G<=7.75x`,
  and `Spread<=1.15x`;
- every LayerFS tmpfs row ratio is `<=1.10x` the matching Cloudflare tmpfs row
  ratio; and
- every per-row budget miss is retained and attributed even when the aggregate
  passes.

Per-row optimized budgets guide repair priority; they are not silently changed
after measurement. A miss below 3 ms is retained but does not justify new code
unless the aggregate gate also misses.

The first safe bounded implementation is expected near `6.080 s`. A result
with median sum `<=6.10 s`, summed overlay ratio `<=3.85x`, and geometric-mean
ratio `<=9.70x` is retained as the expected first-pass band but remains
`REVISE_PERFORMANCE` until the optimized gate passes.

The `3.260 s` / `2.05x` stretch envelope is report-only until real profiling
shows that its owners are attainable without added complexity or weaker
resource behavior.

A future normalized `fs-bench` product stretch of `<=1.5x` its contemporaneous
control would equal about `2.389 s` against the frozen `1.592610 s` overlay
sum. It is a separate label and is not claimed by the `4.500 s` target.

The existing Stage 2 developer-workspace goal of `<=1.5x` native remains a
separate workload target. It is not weakened or inferred from `fs-bench`.

Any safe/correct result above the optimized gate is
`REVISE_PERFORMANCE`, not failure of the canonical architecture. Any identity,
durability, bounded-memory, exact-cleanup, matrix, or product-path violation is
`FAIL_REVISE` regardless of speed.

## 10. Memory and scratch safety

Hard gates:

```text
largest callback/product buffer       <= 1,048,576 bytes
operation Q high-water                <= 8,388,607 bytes
operation Q terminal                  = 0
dirty payload resident high-water     <= 8,388,608 bytes
dirty spool live bytes                <= 335,544,320 bytes
authoritative fs-bench spool quota    <= 536,870,912 bytes
dirty spool physical bytes            <= 671,088,640 bytes
spool compaction transient peak       <= 1,006,632,960 bytes
payload references per SQL batch      <= 64
RSS above settled daemon baseline     <= 67,108,864 bytes
campaign cgroup memory peak           <= 536,870,912 bytes
whole-file in-memory buffer           0
whole-directory entry collection      0
Store connections high-water          <= 2
Store connections terminal            = 0
owned scratch live at clean boundary  0
spool file length at clean boundary   0
container OOM / oom_kill events        0 / 0
```

Mounted-cache accounting must satisfy:

```text
mounted cache entries
  <= kernel-referenced nodes
   + open-handle-only nodes
   + dirty/pending nodes
   + one root entry
```

Allowed memory is
`O(kernel-referenced nodes + open handles + pending entries + dirty ranges)`.
The mount may not preload or retain `O(total stored workspace)` state, and
`forget`, `release`, and pending deletion must reclaim their ownership.

At final cleanup:

```text
kernel lookup references     0 except admitted root lifecycle
open file/dir handles        0
pending inode/entries        0
dirty inode/ranges           0
spool live/dead/physical     0 / 0 / 0
owned temp files             0
```

Spool growth is disk-backed and may scale with submitted dirty bytes; RSS may
not. During the filtered 64 MiB overwrite diagnostic:

```text
dead spool bytes <= live spool bytes + 1 MiB
steady spool physical bytes <= 2 * live bytes + 64 MiB
```

Monotonic physical spool growth across identical create/delete or overwrite
repetitions is a pre-campaign REVISE. The first implementation uses one
append-only spool and exact reset. Add reuse/rotation only if this diagnostic
fails; do not build an allocator in advance. Reserve spool bytes before
accepting a write and return the portable `NoSpace` error before the 512 MiB
campaign quota or the 640 MiB profile cap can be crossed. Compact only when
dead bytes exceed live bytes and physical bytes exceed 64 MiB; compaction runs
outside authoritative scenario timing and stays below the transient cap.

FD accounting:

```text
FD high-water
  <= settled baseline + 64

FD terminal
  = settled baseline
```

The cap prevents one spool FD per dirty inode. It covers Store, one shared
spool, FUSE session, receipts and process plumbing; any increase requires an
itemized receipt rather than a raised limit.

Node, handle, range, resident-byte and spool quotas are runtime admission
values bound into the receipt. The values in this specification are the
`LinuxContainerWorkspaceV1` defaults; they are not canonical-format constants.

## 11. CPU and algorithmic safety

The comparison container has one CPU. The first implementation therefore uses
one mount-wide lock and no worker pool. This is a deliberate ceiling:

```text
ponytail: one mount-wide lock; move to per-inode locks only when measured
lock wait exceeds 10% of a material row or correctness requires it.
```

Hard CPU/complexity gates:

```text
busy-wait or polling loop                         0
background publication worker                    0
SQLite BUSY / LOCKED events                       0 / 0
pending create/delete SQL and COMMITs             0
pending getattr path-to-root resolution           0 after lookup
non-durable pending-row CDC/hash/publication work  0
sequential tail-range insertion                   amortized O(1)
dirty overlap update                              O(log R + overlaps)
persisted range read                              O(log E + X + returned bytes)
directory enumeration                            O(P log D + D), not O(D^2)
one dirty checkpoint                             one transaction / one COMMIT
materialization/capture in mounted route          0 / 0
```

Symbols:

```text
R  dirty ranges for one inode
E  persisted extent count
X  extents intersecting the requested range
D  returned directory entries
P  bounded directory pages returned
```

Record process user/system CPU, cgroup CPU usage/throttling, callback totals,
connection-mutex wait, and mount-lock wait. Mount-lock wait above 10% of a
material row is a measured owner. Do not add concurrency merely because the
counter exists.

CPU admission also requires:

```text
10-second idle daemon CPU delta                 <= 25 ms
daemon CPU per scenario                         <= 1.05 * wall + 5 ms
cgroup throttled_usec / population wall         <= 5%
OOM and oom_kill events                         0
```

A population exceeding the throttling ratio is environment-invalid, not a
performance result. Git may issue real durability callbacks; classify its
CDC/hash/publication work from observed `fsync`/`fsyncdir` receipts. Other
unchanged non-durable `fs-bench` rows require zero such durable work.

Authoritative rows use totals only. Per-callback timing/tracing is allowed in
filtered diagnostics and must be disabled for final FUSE/control comparisons.

## 12. Read and directory optimization

For a persisted regular file:

1. resolve parent/name during `lookup`;
2. cache the canonical inode record and metadata on the mounted node;
3. build one shared `ReadPlan` while the inode is referenced;
4. reuse it across handles and adjacent reads;
5. retain the existing payload batch maximum of 64; and
6. drop it when lookup/open/dirty ownership reaches zero.

The existing one-entry `(root,path) -> ReadPlan` cache remains useful for SDK
calls but is not the mounted-node cache.

Filtered SQL/path gates:

```text
known-node getattr/access/open       <= 2 primary SQL statements; target 0
cold lookup plus returned attrs      <= 16 primary SQL statements
flush/release                         0 primary SQL statements
path components resolved by handle ops = 0
```

Crossing a gate repairs the shared mounted resolution path before adding a
broader cache.

`getattr` reads only keyed portable mode and mtime metadata through the new
bounded `metadata_lookup`; it must not load every Apple xattr value. Checkpoint
reuse of the existing full metadata loader/writer preserves untouched extension
metadata exactly during `chmod` or `utimens`.

Do not add an Engine read-session or connection pool before counters prove
that non-payload queries or repeated connection-lock acquisition dominate a
persisted-read diagnostic. If proved, add one minimal callback-scoped read
session using the existing connection; do not add a pool.

The smallest missing Core primitive is:

```text
directory_page_after(root, exclusive_after_name, max_entries, max_bytes)
  -> bounded ordered entries + continuation
  -> O(log D + returned page)
```

The mounted directory handle retains only its bounded continuation. It must not
call the existing collecting `directory_entries` route, and it must not restart
at the root and skip `offset` entries on every callback.

```text
decoded canonical page       <= 256 KiB
one readdir response          <= 1 MiB
entries emitted in full pass = exactly D
sequential rescans            0
```

Arbitrary `seekdir` to an old cookie is excluded from the first mounted
profile unless a bounded reset policy is implemented; never retain an
unbounded cookie map. Return known file type with each entry so `find -type f`
does not force a separate `getattr`. Add a live Core cursor or `readdirplus`
only if node-read/callback counters prove a material remaining owner.

## 13. Kernel/FUSE policy

The first correctness mount uses zero or very short entry/attribute TTLs.
Enable the optimized policy only after rename, unlink, hard-link, truncate,
external-splice and root-switch invalidation tests pass.

Initial optimized mount:

```text
normal kernel page cache
direct_io disabled
keep_cache only where exact invalidation is implemented
nonzero entry and attribute TTL
exact invalidation after external accepted-root/splice changes
default_permissions where the admitted Linux profile permits it
max_write/read request negotiated up to the 1 MiB product bound
readahead negotiated and recorded
FUSE tracing disabled in authoritative rows
```

TTL values are performance parameters, not correctness exceptions. External
root changes must either invalidate exact nodes/ranges or require a clean
remount before visibility. A stale-cache observation fails correctness.

## 14. Benchmark and evidence contract

Use the unchanged pinned script with:

```text
REPS=3
WARMUP=1
RANDOMIZE_TARGETS=1
MOUNT=/workspace
BASE=/var/tmp
SCENARIOS=<exact 12-scenario offline filter>
OUTPUT_JSON=<unmeasured receipt path>
```

Then run a separate population with `BASE=/tmp`. Do not combine the two
controls.

Required exact filter:

```text
create 1000 files
stat 1000 files
rm 1000 files
mkdir tree (10x10x10)
find tree
write 64 MiB
copy 64 MiB
read 64 MiB
pure read 64 MiB
pure copy 64 MiB
overwrite 64 MiB
git init + commit 100 files
```

For each population require:

```text
12 scenarios x 2 targets = 24 rows
samples per row = 3
unique (scenario,target)
no missing, duplicate or extra row
0 < min <= median <= max
p95 = max for n=3
mean = floor((min + median + max) / 3)
```

The upstream script uses `set -u`, not `set -e`; exit zero is insufficient.
Validate the complete matrix independently. Recompute median ratios; do not use
the script's mean-based printed ratio.

Campaign sequence:

```text
P0  source/environment/fixture/script freeze
P1  native architecture and real-FUSE admission
P2  portable mounted-VFS unit oracle
P3  read-only real-FUSE oracle and persisted 100 MiB proof
P4  writable/fsync/restart/fault oracle
P5  one filtered diagnostic per scenario family
P6  repair only the dominant measured owner
P7  release build and immutable image freeze
P8  6-row smoke
P9  one 24-row /var/tmp population
P10 one 24-row /tmp population
P11 independent matrix/resource/correctness audit
P12 exact unmount/reopen/cleanup and evidence seal
```

Preferred population wall is below 60 seconds. At 120 seconds, stop, preserve
completed rows and the first failing owner. Do not rerun unchanged source for
favorable noise.

Graceful shutdown stops admission, drains started callbacks, performs one
explicit dirty checkpoint, unmounts, closes Store/spool, and removes only exact
owned scratch. Forced restart validates a spool marker containing StoreId and
mount/session identity before discarding unacknowledged scratch; broad cleanup
globs are forbidden. Keep the container namespace alive until `findmnt` and
mountinfo both prove `/workspace` absent, then remove the container.

The unchanged script does not expose a per-scenario daemon-counter hook.
Authoritative counters are therefore campaign-level before/after snapshots.
Per-scenario counters come only from separate filtered, non-authoritative
diagnostics. Missing per-row attribution is `null` with this reason, never zero.

`RANDOMIZE_TARGETS=1` uses unseeded `shuf` and does not emit the target order.
Bind the policy, script hash, coreutils version, and raw results, while marking
the exact order unavailable. A deterministic-order run may be retained only as
a separate diagnostic; do not modify the authoritative comparison.

## 15. Required counters and equations

Cheap authoritative totals:

```text
callbacks by opcode
flush/release/fsync/fsyncdir counts

mounted nodes high/terminal
kernel lookup references high/terminal
open file/dir handles high/terminal
pending inode/entry high/terminal
dirty inode/range high/terminal
created-then-deleted cancellations
dirty-set normalization count

spool appended/live/dead/physical bytes
spool reset/rotation counts
largest request buffer
operation Q high/terminal

canonical namespace/inode/extent nodes read
payload batch queries/references/maximum
SQL statements/fetched rows
authentication and role-decode passes
non-payload/payload query time
connection-mutex and mount-lock wait

objects created/reused
transactions started/committed/rolled back
publication COMMITs
cache invalidations
invalidation requested/succeeded/unsupported/failed
stale-handle rejections
accepted-generation switches

workspace materializations
capture scans

RSS baseline/high/terminal
user/system and cgroup CPU
FD baseline/high/terminal
Store connections high/terminal
children/mounts/scratch/journal/WAL/SHM residue
```

Critical cross-check:

```text
publication_commits
  <= dirty fsync callbacks
   + dirty fsyncdir callbacks
   + explicit dirty checkpoints
   + dirty graceful-shutdown checkpoints
```

The inequality is not sufficient by itself: every successful acknowledged
durability request must map to an accepted or explicitly coalesced checkpoint
receipt, and every failed/ambiguous request must retain its exact outcome.

Unavailable observations are `null` plus a source-specific reason, never zero.

### 15.1 Reuse and merge law from the final Stage 1.1 closure

Mounted receipts extend the existing product observations rather than creating
parallel Engine/VFS counters:

```text
disjoint cumulative Store/scratch/callback work      add
sequential high-water observations                   maximum
simultaneously live distinct resource peaks          add
terminal observations                                final actual state
storage-observation SQL                              separate diagnostic phase
```

The final Stage 1.1 source currently performs three SQL statements for one
Store storage observation. Stage 2P records the actual count separately and
keeps observation reads outside product-operation timers; it does not query
storage status per callback in an authoritative campaign.

Do not reconstruct failed-operation facts from later cumulative totals. When a
mounted fault gate requires both an error and partial observation, add one
narrow mounted observed-result envelope at that boundary; otherwise report the
field unavailable with the exact current-result-type reason. Never fabricate a
subfamily split from an aggregate counter.

## 16. Smallest required tests

Portable mounted-VFS tests:

- pending create/write/read/delete normalizes to exact clean with zero Store
  transaction;
- overlapping and truncating dirty ranges preserve exact bytes within Q;
- hard-link aliases share dirty inode state;
- rename/unlink-open preserves handle behavior;
- bounded directory cursor resumes without duplicates, skips or restart;
- mounted-node cache reclaims on `forget` plus final `release`;
- 1,000 to 2,000 directory entries keep CPU and node reads `<=2.25x`;
- 16x persisted extent count with equal returned bytes adds only expected tree
  levels and never collects all extents;
- exact no-op checkpoint performs zero SQL/COMMIT;
- dirty checkpoint performs one Publication and one COMMIT;
- conflict and ambiguous COMMIT reconciliation preserve the accepted-head law;
- spool failure and cleanup failure never produce a durable acknowledgement;
- external accepted-root change produces exact invalidations or typed remount;
  and
- every terminal resource counter returns to its required value.

Real-FUSE tests:

- Cloudflare's deterministic filesystem oracle;
- hard links, symlinks, modes, mtime, `mmap`, and rename/unlink-open;
- real `fsync` followed by independent daemon/container reopen;
- persisted sequential and random 100 MiB reads;
- explicit splice plus invalidation;
- narrow `/dev/fuse` and `SYS_ADMIN` lifecycle;
- exact PID-directed daemon stop and mount disappearance; and
- zero owned process, mount, volume, scratch and SQLite sidecar residue.

One small test owns each non-trivial branch. Do not add a benchmark-specific
filesystem model or duplicate semantic suite.

## 17. Optimization and repair order

Stop at the first route that meets the target:

```text
1. zero publication on write/flush/release
2. pending nodes and create/delete cancellation
3. inode/metadata/ReadPlan cache bounded by forget/release
4. 1 MiB request admission and sequential range coalescing
5. bounded resumable readdir with returned file type
6. normal kernel page cache and exact invalidation
7. measure filtered scenario families
8. repair path/SQL, spool, request, or lock owner actually observed
9. optional same-mount copy_file_range only if copy remains dominant
```

For the first copy diagnostic, return `ENOSYS` from `copy_file_range` and allow
ordinary read/write fallback. If pure-copy remains a dominant aggregate miss,
the only admitted fast path is same-mount immutable-extent or owned-spool
reference sharing with exact lifetime handling. No generic copy framework is
authorized.

Do not begin a full population if a filtered diagnostic shows:

```text
publication on write/flush/release
SQL/fetched-row growth proportional to pending callbacks
dirty/spool growth after create/delete cancellation
whole-file RSS growth
O(D^2) readdir restart
materialization or capture nonzero
missing result rows
resource counters growing monotonically
```

## 18. Terminal disposition

Stage 2P can report `PASS_OPTIMIZED` only when:

- the real product mount and unchanged workload are used;
- the portable dependency law holds;
- correctness, durability, history, metadata and fault gates pass;
- pending operations and checkpoint equations pass;
- the optimized aggregate targets pass;
- CPU/memory/spool/FD/connection limits pass;
- materialization and capture are literal zero;
- every raw row, environment, source, executable, image and equation is sealed;
  and
- terminal unmount/reopen/cleanup proves zero owned residue.

`REVISE_PERFORMANCE` preserves a safe/correct target miss and the smallest
measured-owner repair list. `FAIL_REVISE` preserves a correctness, durability,
identity, resource, product-path, evidence or cleanup failure.

The next action after this specification is not a complete optimized daemon.
It is the smallest portable pending-node/dirty-range unit proof, followed by
the smallest read-only real-FUSE mount. Performance work advances only after
each correctness and resource gate closes.
