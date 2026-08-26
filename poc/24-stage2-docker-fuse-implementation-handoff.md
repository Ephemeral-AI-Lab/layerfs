# Handoff Prompt — Implement Stage 2 Docker/Linux FUSE and Pass `fs-bench`

Use this entire document as the implementation agent's prompt.

---

You own the complete Stage 2 Docker/Linux FUSE implementation and performance
closure for LayerFS in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty
```

## Goal

Implement the real production LayerFS mounted filesystem, expose it through a
thin native Linux FUSE adapter at `/workspace` inside the admitted Docker
environment, close correctness/durability/resource/portability gates, and
continue optimizing until a source-bound unchanged upstream `fs-bench.sh`
campaign reaches `PASS_OPTIMIZED`.

This is an implementation task, not a planning or report-only task. Do not stop
after drafting architecture, adding an evaluator, compiling a toy filesystem,
or producing a slow/correct first run. Terminal success is a real LayerFS FUSE
mount, exact correctness and cleanup, and a very successful complete benchmark
run meeting every gate below.

## Closure record

This handoff is complete. The terminal source is
`88e12ff0268afb380f0f8f44d3ca9d4639be65cc`; the terminal ARM64 image is
`sha256:39d13adfb9f2f1a20313d09f23ea1d3be7fcd5535a12eb1afd3a6698b1800fc1`.
Both exact `--cpus 1` authoritative populations pass all numeric, row,
resource, publication, and cleanup gates. The controlling evidence and raw
custody are [candidate 012](evidence/stage2-freeze-candidate-012/summary.md).
Earlier source/image populations are not promoted by this record.

## Terminal success

Terminal success is all of the following on one frozen source/image/fixture:

```text
real native Linux FUSE at /workspace                 PASS
actual LayerFS Core/Engine/Store product path        PASS
read-only and writable filesystem oracle             PASS
fsync/checkpoint/reopen/fault reconciliation         PASS
history/fork/rollback/old-root correctness           PASS
memory/CPU/Q/spool/FD/connection limits              PASS
materialization/capture in mounted workflows         0 / 0
owned process/mount/temp/SQLite residue              0
unchanged /var/tmp fs-bench population               PASS_OPTIMIZED
unchanged /tmp fs-bench population                   PASS_OPTIMIZED
independent raw/statistics/resource audit             PASS
```

The primary `/var/tmp` performance disposition is:

```text
SL       = sum of 12 LayerFS scenario medians        <= 4,500 ms
Rsum     = SL / same-population control median sum   <= 2.85
G        = geometric mean of 12 row ratios           <= 7.00
Spread   = sum of 12 maxima / SL                     <= 1.15
each LayerFS row ratio                               <= 1.10 * matching
                                                       Cloudflare overlay ratio
```

The separate `/tmp` disposition is:

```text
SL                                                     <= 4,500 ms
Rsum                                                   <= 3.10
G                                                      <= 7.75
Spread                                                 <= 1.15
each LayerFS row ratio                                 <= 1.10 * matching
                                                         Cloudflare tmpfs ratio
```

`REVISE_FIRST_PASS`, `REVISE_PERFORMANCE`, an implementation-caused `NO_GO`,
or a partial benchmark is not terminal success.

## Continuation rule — do not stop on implementation-caused blockers

You are explicitly authorized to make the in-scope production, test,
evaluator, Docker, dependency, documentation, and evidence changes listed in
this prompt. Do not pause to ask for routine authorization before:

- editing the listed production files;
- adding the new production crate/modules/tests;
- selecting and pinning one FUSE dependency;
- updating `Cargo.toml` and `Cargo.lock`;
- building release binaries and Docker images;
- running focused/workspace/Linux-container tests;
- starting/stopping exact owned containers and FUSE mounts;
- running the unchanged benchmark and filtered diagnostics;
- adding source-bound receipts and reports; or
- committing exact in-scope files for source/image identity.

Never treat these as terminal:

```text
compile failure
test failure
Docker build failure
FUSE callback failure
mount failure caused by implementation
correctness mismatch
resource-limit failure
performance miss
REVISE
NO_GO caused by source/configuration
missing counter/evidence field
benchmark timeout caused by the product
```

For each such result:

```text
preserve the first failing command, raw output, counters and equation
  -> identify the shared production owner/root cause
  -> replan the smallest correct repair
  -> change production code, not only the benchmark harness
  -> add one focused regression proof
  -> rerun only the invalidated focused proof
  -> re-freeze changed source/image before a new full campaign
  -> continue
```

Do not wait for user confirmation between these steps. Do not weaken a target,
remove a failing row, hide an unavailable field, switch to a shim, or relabel a
slow/correct run as success.

If the Docker daemon or `/dev/fuse` is temporarily unavailable, continue every
independent portable-VFS, build, fixture, evaluator and evidence task, preserve
the exact admission failure, repair/retry the environment, and resume the real
mount. Only a genuinely external physical/permission impossibility that remains
after all safe alternatives are exhausted may be reported as blocked; it is
not terminal success. Never bypass security controls, use destructive cleanup,
or invent credentials/permissions.

## Scope change — Stage 1.2 is skipped

Stage 1.2 is explicitly skipped by user decision. It is not an entry gate,
execution stage, or required APFS baseline.

The active sequence is:

```text
final Stage 1.1 correctness/durability closure
  -> Stage 2.0 Docker/FUSE admission
  -> Stage 2.1 portable/read-only mounted VFS
  -> Stage 2.2 writable mounted workspace
  -> Stage 2.3 locality, durability and fault proof
  -> Stage 2.4 direct Linux developer workspace
  -> Stage 2P optimization and fs-bench closure
```

Do not implement or run the Stage 1.2 APFS materialize/capture campaign. The
retained `poc/15` file is historical workload input only. You may reuse its
bounded offline npm/build/search command corpus directly through FUSE.

## Current repository and custody

Expected starting state:

```text
cwd       /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty
branch    codex/empty-worktree
HEAD      30597059d563f39113d7b69017146a70a7437e1a
```

Final Stage 1.1 product source:

```text
commit          d1848200d249915d3f1e35af5556fdf6c1ec05c6
release SHA-256 b056b535c7d3e0711a120731e414bbff213ca0be9c6a603cc3387da6633af624
release BLAKE3  8fe897685cda24c850d58c35e27687a02389747232fcc862337bcf7de234ef01
regression      independently audited attempt-024 PASS: 47/51/34
```

The working tree intentionally contains user-owned Stage 2 sequencing/spec
changes and untracked `poc/21`, `poc/23`, and this `poc/24` handoff. Preserve
them. Before editing:

1. freeze `pwd`, branch, HEAD and full tracked/untracked status;
2. save the exact current diff and source manifest;
3. hash every controlling document, `Cargo.toml`, `Cargo.lock`, and current
   product source;
4. identify unrelated concurrent changes and never reset/revert/clean them;
5. stage/commit only exact in-scope paths; and
6. never use broad destructive cleanup.

You may create source-bound commits for Stage 2. Include the intentional Stage
2 document changes only after reviewing them; never sweep unrelated user files
into a commit merely to obtain a clean status.

## Read completely before implementation

Read these files end to end in this order:

1. `poc/10-handoff-freeze.md`
   - SHA-256 `514a76b3ec14c3b5bcef19a491a2f22aaad3404255ec15f51af1f4a002f5c1f2`
2. `poc/09-portability-and-apple-completeness.md`
   - SHA-256 `e8eafc9c3ca9a8006e867e94376c0f635edfe7198240b8edc7f77f4d3aaafa53`
3. `poc/evidence/stage1.1-terminal-audit-20260826/summary.md`
   - SHA-256 `93e76b7496ca9ff4eea1ab0f479fb76b9986ef08cf519913e5a57f5d28746229`
4. `poc/evidence/stage1.1-terminal-audit-20260826/terminal-receipt.json`
   - SHA-256 `018557adffbf03725e03b765628afab97a89adaf91094e75ad6e575e414c8cfe`
5. `poc/20-stage1.1-full-materialization-optimization.md`
6. `poc/22-stage1.1-trusted-localdev-materialization.md`
7. `poc/19-stage2-docker-linux-fuse.md`
   - expected SHA-256 `428bfe5946643fb2e8b248d3ef38bfcc8ca8c57dcdf6cf3ed80578ed9b28cfc7`
8. `poc/23-stage2-fuse-performance-optimization.md`
   - expected SHA-256 `0bbede150fb338d668bef50f38fe686c6fb9255032491c3ed8e62c86b5f5371e`
9. `poc/21-lane-a-cloudflare-computer-baseline.md`
   - SHA-256 `2194d9f49ffc1a61be845b66ca1e44b90720b342e456f9eaa06af603a74a2dd3`
10. `/Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/cloudflare-docker-fuse-layerfs-handoff.md`
    - SHA-256 `7c13fdb7dc7784dead6398eb984a62be39a665e3435b71e9e634054a96b9f4b7`
11. `/Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/evidence/docker-fuse-attempt-2/measurement-verification.json`
    - SHA-256 `c6c6561658eacbbbac6e01b6798658abdf69e8dcb605299bfb802da4c3c9fc26`
12. the unchanged upstream benchmark:
    `/Users/yifanxu/Ephemeral-AI-Lab/cloudflare-computer-bench/upstream/script/fs-bench.sh`
    - SHA-256 `0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef`

If an expected hash differs because this handoff itself or another authorized
change updated the document, do not restore the old bytes. Preserve the current
file, inspect the diff, recompute the hash, record the supersession, and continue.

Then trace the actual current code end to end before designing:

```text
crates/layerfs-core/src/{namespace,inode,metadata,content/rope}.rs
crates/layerfs-engine/src/{lib,refs,publication,scratch,integrity}.rs
crates/layerfs-vfs/src/{lib,resolver,workspace,managed_edit,driver}.rs
crates/layerfs-sdk/src/lib.rs
tools/layerfs-eval/src/
```

Plans are hypotheses; current source, tests, counters, raw rows and receipts are
evidence.

## Production-code authority and expected files

You are authorized to add/edit the smallest correct production tree:

```text
Cargo.toml
Cargo.lock

crates/layerfs-core/src/
  namespace.rs       bounded directory_page_after query if still required
  metadata.rs        keyed metadata_lookup for portable mode/mtime

crates/layerfs-vfs/src/
  lib.rs             export mounted types/counters
  resolver.rs        reuse explicit path operations; do not make it FUSE-hot
  mounted.rs         one portable mounted state machine

crates/layerfs-fuse/
  Cargo.toml         one pinned maintained Linux FUSE binding
  src/lib.rs         callback/error/cache/invalidation translation
  src/main.rs        daemon, CLI, mount/unmount, signals, terminal receipt
  tests/mounted_routes.rs

containers/layerfs-fuse/
  Dockerfile

tools/layerfs-eval/src/
  main.rs
  stage2_fuse.rs

poc/
  Stage 2 readiness/results/evidence manifests and exact warranted updates
```

Modify `layerfs-engine` only when the actual mounted path proves one shared
primitive is missing. Reuse final Stage 1.1 guarded reads, cached StoreId,
authenticated scratch, `Publication`, `RefState`, `EngineCounters`,
`OperationCounters`, `ProjectionFacts`, and `StorageObservation`. Do not fork
their logic into the daemon or evaluator.

Leave `layerfs-os` and `layerfs-sdk` unchanged unless a concrete production
caller requires a minimal neutral constructor or operation. Do not route the
timed mount through the Apple-oriented SDK.

Do not change canonical codecs, object identity, FastCDC profile, schema,
metadata domains, inode allocation, trust rules, or durability merely for
performance. If a real correctness contradiction requires such a change,
preserve the contradiction, prove it with a focused test, make the smallest
authority-consistent change, re-freeze fixtures and continue.

## Dependency and portability law

Select exactly one maintained Rust FUSE binding after proving:

- native Linux ARM64 and x64 support;
- required low-level inode/handle callbacks;
- read/write/readdir/getattr/open/release/flush/fsync;
- rename/unlink/link/symlink/invalidation support;
- kernel page-cache and `mmap` compatibility;
- bounded request buffers;
- clean cancellation/unmount; and
- acceptable license/build/runtime requirements.

Prefer the simplest binding that passes. `fuser` is a reasonable first
candidate, but admission evidence decides. Pin one exact version. Do not
handwrite the FUSE protocol, support multiple bindings, add a registry, or add
an async runtime unless the selected binding makes it unavoidable.

Dependency direction is mandatory:

```text
layerfs-core
  no FUSE/libc/errno/Linux/Docker/platform cfg

layerfs-engine
  no FUSE callback or mount lifecycle

layerfs-vfs::mounted
  portable node/handle/namespace/dirty/checkpoint semantics
  no FUSE request/reply, errno, uid_t/gid_t, Linux flags or kernel cookies

layerfs-fuse
  Linux FUSE types, errno, credentials, cache, request negotiation,
  invalidation and mount lifecycle only
```

Do not add a generic frontend trait with one implementation. Future FSKit and
WinFsp adapters will call the same concrete portable `MountedWorkspace` API.

The Linux 4.5-second wall target does not transfer to other platforms. Portable
complexity, durability, memory and zero-materialization gates do.

## Architecture to implement

```text
Linux application
  -> kernel VFS
  -> real FUSE at /workspace
  -> thin layerfs-fuse adapter
  -> layerfs-vfs::MountedWorkspace
  -> existing LayerFS Core + Engine
  -> one SQLite/CAS Store
```

There is no separately materialized workspace tree, backing-file loop, watcher,
polling mirror, capture scan, OverlayFS workspace, or benchmark-only model.

One `MountedWorkspace` owns:

```text
accepted RefState and namespace root
lifecycle: Live | Checkpointing | Incomplete
mount-scoped MountedNodeId map
optional canonical InodeId per accepted node
kernel lookup references
open file and directory handles
pending namespace/inode state
per-directory entry deltas
per-inode dirty ranges
one owned disk spool
bounded in-flight byte budget
portable invalidation events
resource/performance counters
```

Pending nodes use stable mount-local IDs. Allocate canonical `InodeId`s only
inside the one expected-head `Publication` for nodes surviving checkpoint.
Retain the same mounted identity after acceptance. Never truncate/hash a
canonical ID into an assumed collision-free FUSE `u64`.

Hot operations are inode-relative:

```text
lookup(parent_node, basename)
getattr(node)
open(node)
read(node, offset, length)
create(parent_node, basename)
write(node, offset, bytes)
unlink(parent_node, basename)
```

After lookup, read/write/getattr/open/release must not reconstruct a path and
walk from the root.

## Mounted profile

Required:

- regular files and nested directories;
- exact symlink target bytes;
- regular-file hard links sharing one mounted/canonical inode state;
- read/write/executable mode and canonical mtime;
- exact rename replacement and directory-cycle refusal;
- unlink-open lifetime;
- read-your-writes across handles/aliases;
- ordinary kernel page cache and `mmap` after invalidation proof;
- fork, rollback, retained historical roots and exact reopen;
- expected-head conflict and ambiguous-publication handling; and
- typed errors for unsupported/unrepresentable operations.

Initial canonical Linux name/metadata boundary:

```text
names                  exact canonical UTF-8 only
backslash              rejected
component/path/depth   existing LayerFS limits
uid/gid                fixed synthetic mount values; not canonical
chown                  Unsupported
Linux xattr mutation   Unsupported
portable metadata      existing mode + mtime
Apple extension data   preserved; typed unrepresentable where required
```

Never use lossy conversion or silently discard metadata.

## Durability contract

```text
write/create/truncate/rename/unlink
  -> bounded dirty mounted state
  -> zero Publication
  -> zero COMMIT

flush/release/ordinary close
  -> report prior errors and release references
  -> zero Publication
  -> zero COMMIT

fsync/fdatasync/fsyncdir
  -> checkpoint the entire mounted dirty workspace
  -> one expected-head Publication
  -> one visibility COMMIT

explicit checkpoint
  -> same whole-workspace checkpoint

graceful dirty shutdown
  -> one checkpoint before successful unmount

forced death before acknowledged durability
  -> unacknowledged dirty state may be discarded
```

Dirty checkpoint equations:

```text
transactions_started       = 1
transactions_committed     = 1
transactions_rolled_back   = 0
publication_commits        = 1
generation_after           = generation_before + 1
expected_root              = accepted_root_before
```

Exact clean checkpoint equations:

```text
transactions_started       = 0
publication_commits        = 0
objects_written            = 0
CDC bytes                  = 0
root/generation            unchanged
```

Ambiguous outcome:

```text
fresh reconciliation observes candidate exactly
  -> accept candidate, clear included dirty state, fsync succeeds

fresh reconciliation observes old expected exactly
  -> retain dirty state, fsync fails, retry allowed

fresh reconciliation observes different/indeterminate state
  -> lifecycle = Incomplete
  -> reject every later mutation
  -> require discard/reopen/remount
```

Never infer durability from elapsed time, kernel cache, a process-local root,
or a generic `EIO` followed by continued writes.

## Dirty-state and memory design

Use one mount-wide bounded dirty working set:

```text
DirtyFile
  optional base FileStateRoot
  logical length
  ordered/coalesced dirty ranges
  references into one owned spool
  mode/mtime changes
  open-handle count
```

Required behavior:

- no complete large-file `Vec`;
- no all-extents or all-directory `Vec`;
- sequential tail writes coalesce in amortized `O(1)` range work;
- arbitrary overlap is `O(log R + overlaps)`;
- reads merge dirty/spool data and unchanged canonical ranges;
- hard-link aliases share one dirty inode state;
- pending create/write/unlink before checkpoint cancels to zero persistent work;
- exact clean state truncates the spool to zero; and
- quota failure occurs before partial mutation and maps to semantic `NoSpace`.

The existing Stage 1 operation-Q counter is observational, not concurrent
admission control. Implement an enforcing standard-library byte budget:

```text
reserve actual bytes before allocation
block waiters on Mutex + Condvar
shutdown wakes/cancels waiters
never spin or retry-poll
```

Account adapter request/response copies, mounted temporaries, current dirty
inline bytes, payload-batch staging and spool-copy buffers. Observe SQLite and
kernel page caches separately.

## Resource and CPU hard gates

```text
largest FUSE/product payload buffer             <= 1,048,576 bytes
enforced operation Q high-water                 <= 8,388,607 bytes
operation Q terminal                            = 0
mount-wide resident dirty payload               <= 8,388,608 bytes
live mounted nodes                              <= 65,536
open file + directory handles                   <= 8,192
dirty + pending nodes                           <= 32,768
dirty-range descriptors                         <= 65,536
directory cursors                               <= 4,096
inflight mounted callbacks                      <= 8
FUSE workers in one-CPU campaign                <= 4
all daemon threads                              <= 8

authoritative fs-bench spool quota              <= 512 MiB
profile live spool                              <= 320 MiB
steady spool physical                           <= 2 * live + 64 MiB
absolute spool physical                         <= 640 MiB
spool compaction transient                      <= 960 MiB

daemon RSS above settled baseline               <= 64 MiB
campaign cgroup memory peak                     <= 512 MiB
daemon FD high-water                            <= settled baseline + 64
Store connections high-water                    <= 2
Store connections terminal                      = 0
OOM / oom_kill                                  = 0 / 0

busy-wait/polling/background checkpoint loops   = 0
SQLite BUSY / LOCKED                            = 0 / 0
10-second idle daemon CPU                       <= 25 ms
daemon CPU per scenario                         <= 1.05 * wall + 5 ms
cgroup throttled_usec / population wall         <= 5%
connection/mount-lock wait                      <= 10% product callback wall
```

The first implementation may use one mount-wide lock on the one-CPU campaign.
Move to per-inode locks only after path/SQL amplification is fixed and measured
lock wait exceeds 10% of a material row or correctness requires it.

At clean benchmark boundaries and terminal shutdown:

```text
pending nodes/entries                0
dirty nodes/ranges                   0
spool live/dead/physical             0 / 0 / 0
operation Q                          0
open handles                         0
owned scratch/temp                   0
Store connections                    0
children                             0
mounts                               baseline
SQLite journal/WAL/SHM residue       0
processes holding /dev/fuse          0
```

## Minimal implementation sequence

### P0 — Freeze and admission

- freeze source/spec/environment/fixture hashes and exact dirty status;
- choose/pin one FUSE binding;
- prove native ARM64/x64 dependency architecture;
- prove Docker `/dev/fuse` with `SYS_ADMIN`, not `--privileged`;
- mount/unmount a tiny <=8 MiB probe;
- prove no host bind, shim, emulation or residue.

### P1 — Minimal Core queries

Add only if still missing:

```text
directory_page_after(root, exclusive_after_name, entry/byte budgets)
metadata_lookup(root, key)
```

Prove bounded memory, malformed-tree refusal and canonical ordering. Do not
reimplement B+ tree navigation in mounted/FUSE code.

### P2 — Portable read-only `MountedWorkspace`

Implement and test without FUSE types:

```text
root, lookup, forget, getattr
opendir, readdir, releasedir
open, read, readlink, release
historical-root open and exact range reads
```

Use bounded directory continuation; arbitrary old-cookie `seekdir` may use a
bounded reset policy in v1, never an unbounded cookie map.

Persisted read gates:

```text
300 deterministic 64 KiB reads     p50 <= 1.5 ms; p95 <= 3 ms
100 adjacent 1 MiB reads            >= 250 MiB/s
100 MiB sequential read             >= 250 MiB/s
exact bytes                          required
materialization/capture              0 / 0
```

### P3 — Real Linux read-only FUSE

Implement:

```text
init/destroy, lookup, forget/batch_forget
getattr, access, statfs
opendir/readdir/releasedir
open/read/readlink/release
```

Prove real kernel mount, exact nested/hard-link/symlink/mode/mtime behavior,
random/sequential reads, concurrent handles, `mmap`, corruption refusal,
restart/reopen and clean unmount.

Start correctness with zero/short cache TTLs. Enable nonzero TTL, normal page
cache, `keep_cache`, readahead and request sizes only after exact rename,
unlink, truncate, hard-link, splice and root-switch invalidation tests pass.

### P4 — Writable portable overlay and FUSE callbacks

Implement:

```text
create, write, truncate/ftruncate
mkdir/rmdir, rename, unlink
link, symlink, chmod, utimens
flush, fsync/fdatasync/fsyncdir, release
```

Prove read-your-writes, pending identity, hard-link aliases, rename replacement,
directory-cycle rejection, sparse/zero extension, truncate, symlink, unlink-open,
quota rejection before mutation and cleanup.

### P5 — Checkpoint, faults and lifecycle

Implement one whole-workspace Publication. Prove dirty and exact-no-op
equations, confirmed pre-visibility failure, conflict, lost acknowledgement,
true ambiguity, graceful shutdown, forced death before/after acknowledgement,
orphan-spool ownership validation and exact reopen.

Owned runtime cleanup uses an authenticated marker containing StoreId,
mount-session ID and source/executable identity. Never delete by broad glob or
prefix alone.

### P6 — Filtered performance diagnostics

Run one source-bound diagnostic per family with detailed tracing allowed only
there:

```text
create/stat/remove
mkdir/find
write/read/overwrite
copy/pure-copy
Git
persisted 100 MiB reads and durable create/checkpoint
```

Repair the largest measured positive row-budget gap. Do not run full campaigns
for every edit.

### P7 — Freeze and authoritative campaigns

After focused correctness/resource/performance gates pass:

```text
one workspace fmt/check/test/clippy closure
one native Linux release build
one immutable Docker image
one zero-row readiness receipt
one 6-row smoke
one complete /var/tmp population
one separate complete /tmp population
one independent audit
one exact unmount/reopen/cleanup seal
```

## Docker comparison environment

Reproduce the admitted Cloudflare envelope:

```text
--platform linux/arm64
--init
--cpus 1
--memory 3g
--pids-limit 512
--device /dev/fuse
--cap-add SYS_ADMIN
--network none
--tmpfs /tmp:rw,nosuid,nodev,size=1g,mode=1777
```

Do not use:

```text
--privileged for accepted evidence
linux/amd64 emulation on Apple Silicon
host bind or named volume at /workspace
network benchmark rows
runtime package installation
FUSE shim/auto fallback
tracing asymmetry
```

Runtime placement:

```text
/workspace                         real LayerFS FUSE mount
/var/lib/layerfs/store.sqlite      authoritative Store on Docker-local storage
/var/tmp/layerfs-owned/spool       non-authoritative spool, outside benchmarks
/var/tmp                           observed primary control storage class
/tmp                               explicit tmpfs control
```

Large benchmark rows exercise the spool. Put it on the same filesystem/storage
class as `/var/tmp` or add and report a separate native control on the actual
spool volume. Do not compare unlike storage classes silently.

## Unchanged `fs-bench` contract

Use the pinned script byte-for-byte:

```text
SHA-256 0e2004339a9269b88c2de451d56575957477bde16b96a10cf1837519e37230ef

REPS=3
WARMUP=1
RANDOMIZE_TARGETS=1
MOUNT=/workspace
BASE=/var/tmp
OUTPUT_JSON=<unmeasured receipt path>
SCENARIOS=create 1000 files,stat 1000 files,rm 1000 files,
          mkdir tree (10x10x10),find tree,write 64 MiB,copy 64 MiB,
          read 64 MiB,pure read 64 MiB,pure copy 64 MiB,
          overwrite 64 MiB,git init + commit 100 files
```

Then run a separate unchanged population with `BASE=/tmp`.

Every population requires:

```text
12 scenarios * 2 targets       = 24 unique rows
samples per row                = 3
0 < min <= median <= max
p95                            = max for n=3
mean                           = floor((min + median + max) / 3)
missing/duplicate/extra rows   = 0
```

The script uses `set -u`, not `set -e`; exit zero is insufficient. Independently
validate the matrix and recompute median ratios. Never use its printed
mean-based ratio.

`RANDOMIZE_TARGETS=1` uses unseeded `shuf`; bind the policy and coreutils
version and mark exact target order unavailable. Do not modify the authoritative
script to expose it.

The unchanged script has no per-scenario daemon-counter hook. Authoritative
counters are campaign-level before/after snapshots. Per-scenario counters come
from separate filtered non-authoritative diagnostics. Never recognize `.bench`
paths in production or fabricate per-row attribution.

## Scenario budgets

| Scenario | Cloudflare median | LayerFS optimized budget |
|---|---:|---:|
| create 1,000 | 858.3 ms | 300 ms |
| stat 1,000 | 1,463.6 ms | 900 ms |
| remove 1,000 | 1,085.2 ms | 400 ms |
| mkdir tree | 1,350.0 ms | 850 ms |
| find tree | 1,401.9 ms | 950 ms |
| write 64 MiB | 83.8 ms | 80 ms |
| copy 64 MiB | 309.6 ms | 250 ms |
| read 64 MiB | 163.5 ms | 130 ms |
| pure read 64 MiB | 74.1 ms | 55 ms |
| pure copy 64 MiB | 231.9 ms | 180 ms |
| overwrite 64 MiB | 144.1 ms | 100 ms |
| Git commit 100 | 451.9 ms | 300 ms |
| **sum** | **7,617.9 ms** | **4,495 ms** |

These are row-budget allocations. The aggregate normalized gates control
terminal acceptance; preserve every row miss exactly.

Workload classification:

- namespace scenarios do not request durability;
- write/copy/read create data through the dirty mount;
- pure read/copy/overwrite use out-of-timer dirty staging;
- no large-I/O row proves persisted Store reconstruction;
- Git durability is classified by observed callbacks, not assumption.

For non-durable pending rows require:

```text
CDC bytes                       0
canonical objects written       0
Store publication COMMITs       0
write/flush/release COMMITs      0
```

## Optimization priority

The forecast Cloudflare-to-LayerFS saving is `3,122.910 ms`; namespace and Git
rows own `2,910.904 ms` or `93.21%`. Optimize in this order:

```text
1. no publication on write/flush/release
2. pending nodes and create/delete cancellation
3. inode/metadata/ReadPlan cache bounded by forget/release
4. inode-relative hot callbacks; remove path/SQL amplification
5. 1 MiB request admission and sequential dirty-range coalescing
6. bounded directory paging with returned file type
7. normal kernel page cache plus exact invalidation
8. measure and repair the actual largest owner
9. optional same-mount copy specialization only if copy is then dominant
```

Start `copy_file_range` as `ENOSYS` and allow ordinary `cp` fallback. Do not add
extent sharing, a connection pool, second user-space object cache, per-inode
locks, compression, background checkpoint, worker pool, async runtime, multiple
bindings, or a generic mount framework without measured proof that it owns the
largest remaining accepted gap.

Use one mount-wide lock first. This is an intentional ceiling:

```text
ponytail: one mount-wide lock; move to per-inode locks only if measured lock
wait exceeds 10% of a material row or correctness requires it.
```

## Counter and evidence law

Reuse existing `OperationCounters`, `EngineCounters`, `ProjectionFacts`, and
`StorageObservation`. Mounted/FUSE counters extend them only for missing facts.

```text
disjoint cumulative work                   add
sequential high-water observations         maximum
simultaneously live distinct peaks         add
terminal observations                      final actual state
storage-observation SQL                    separate diagnostic phase
```

Keep storage-observation queries outside product-operation timers and out of
per-callback authoritative paths. Do not reconstruct failed-operation facts
from later cumulative totals. Return an observed error envelope only where the
fault gate genuinely needs both error and partial observation; otherwise use
`null` plus the exact unavailable reason.

Required campaign totals include:

```text
callbacks by opcode
flush/release/fsync/fsyncdir
nodes, lookup refs, handles, pending/dirty/ranges
spool appended/live/dead/physical/reset/rotation
request and Q high-water/terminal
path/inode/extent/object and payload batches
SQL, fetched/authenticated/decoded rows and query time
mutex/mount-lock wait
objects created/reused
transactions/COMMITs/rollbacks
invalidation requested/succeeded/unsupported/failed
materialization/capture
RSS, CPU, cgroup memory/throttle
FD, connections, children, mounts and owned residue
```

Unavailable is `null` plus reason, never zero.

## Test and source-freeze policy

During iteration:

```text
one shared root cause
  -> one focused test
  -> rustfmt/touched-crate check
  -> continue
```

Do not repeatedly run full workspace tests, rebuild the complete image, or run
the full benchmark for timing noise. After focused closure, run exactly one
final workspace format/check/test/clippy closure and one release/image freeze
before authoritative timing.

Final source closure must include, as applicable:

```text
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
portable mounted-VFS focused/model/fault tests
native Linux Docker build and tests
real-FUSE functional/lifecycle tests
tracked/untracked diff and source manifest checks
```

Adjust host commands only where the Linux-target FUSE crate correctly requires
the container; do not weaken the proof by silently omitting it.

## Stop and repair policy

Do not start a complete population if a filtered diagnostic shows:

```text
publication on write/flush/release
SQL/fetched-row growth proportional to pending callbacks
dirty/spool growth after create/delete cancellation
whole-file or whole-workspace RSS growth
O(D^2) directory enumeration
stale kernel cache
materialization/capture nonzero
missing rows
resource counters growing monotonically
```

If a filtered scenario exceeds 10 seconds or resources grow monotonically,
stop that row, preserve evidence, repair the shared owner and rerun only that
proof. If a full population approaches 120 seconds, stop, preserve completed
rows, repair, re-freeze and continue. Do not rerun unchanged source for luck.

After any valid complete campaign:

```text
gap[i] = max(0, LayerFS median[i] - optimized budget[i])
```

Work on the largest positive aggregate owner. Stop optimization only at
`PASS_OPTIMIZED`; do not spend first-pass time pursuing the report-only 2.05x
stretch after terminal success.

## Independent audits

Use independent read-only agents at major gates when available:

1. portable mounted semantics/correctness and source-boundary audit;
2. Linux FUSE/Docker/resource/lifecycle audit; and
3. raw `fs-bench` matrix/statistics/performance audit.

The main agent owns the integrated implementation. Do not let parallel editors
modify the same mounted/adapter files. Read-only audits may not reset, clean,
delete, or overwrite concurrent work.

## Required evidence artifacts

Create a fresh append-only exact-owned attempt directory outside measured
targets containing at least:

```text
environment.json
source-manifest.json
executables-images.json
docker-admission.json
architecture-admission.json
fuse-admission.json
commands.jsonl
schedule.json
functional-oracle.json
checkpoint-reopen.json
splice-locality.json
persisted-read.json
fs-bench-smoke.json/stdout/stderr
fs-bench-deterministic.json/stdout/stderr
fs-bench-tmpfs.json/stdout/stderr
measurement-verification.json
resources.json
cleanup.json
failure-ledger.json
summary.json
summary.md
campaign-time.txt
SHA256SUMS
```

Raw upstream JSON is byte-exact and separate from LayerFS annotations. Bind
source commit/tree/dirty state, executable hashes, image ID/digest, FUSE
dependency, kernel/Docker/tool versions, fixtures, script hash, Store/spool
storage classes, mount options, integrity mode, counters, equations and cleanup.

Cleanup exact owned containers, mounts, volumes and scratch only after evidence
is extracted. Keep the container namespace alive long enough to prove
`/workspace` absent from both `findmnt` and mountinfo before removal.

## Final response

Do not return a terminal final response until `PASS_OPTIMIZED` is honestly
achieved. Then report:

- exact disposition and every acceptance equation;
- source commits/tree/dirty state and all executable/image hashes;
- production files and architecture implemented;
- focused/workspace/Linux/FUSE test results;
- functional, checkpoint, fault, restart and history results;
- all 12 `/var/tmp` and `/tmp` scenario medians/maxima/ratios;
- aggregate `SL`, `Rsum`, `G`, `Spread`, and Cloudflare comparison;
- persisted-root read/create results;
- callback/SQL/object/transaction/invalidation counters;
- CPU/RSS/Q/spool/FD/connections/cgroup/mount/residue results;
- raw artifact paths and `SHA256SUMS` identity;
- preserved failures and the repairs that superseded them;
- portability qualifications and exact unsupported profile; and
- confirmation that Stage 1.2 was skipped and no APFS Stage 1.2 path was used.

Terminal success means the implementation is correct, durable, portable above
the Linux adapter, bounded under the resource gates, cleanly restartable, and
materially better than the reproduced Cloudflare baseline on the unchanged
`fs-bench.sh` campaign.
