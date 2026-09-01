# LayerFS V3 copy-range and extent-aware prepend specification

Status: binding V3 refinement; not yet implemented

Specification date: 2026-09-01

Repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

Inspected base commit: `07b1fc2ae58c946c2d2a8af7a2caf4aba9949e0d`

Source state: active dirty worktree; preserve unrelated changes

## 1. Purpose and authority

This specification defines the smallest correct V3 implementation of the
standard Linux/FUSE `copy_file_range` operation and its use for an
extent-aware prepend workflow.

It is authoritative for:

- `copy_file_range` behavior across FUSE, proxy, Workspace, content, and
  Commit;
- immutable-source range reuse;
- the bounded fallback for ranges that cannot use the structural fast path;
- copy-range telemetry and proof;
- the `prepend-copy-range-rename` benchmark row.

It refines the count-changing-edit discussion in
`docs/research/history/v3/capture-large-mixed-edit-resilience-audit.md`. It does not change the
one-Store topology, Workspace lifecycle, Commit visibility rules, SQL
placement, SDK lifecycle, or benchmark environment in
`docs/research/history/v2-replacement/spec.md`.

Where this document conflicts with older prepend projections or informal
optimization notes, this document wins for V3 copy-range work. Measured
results always win over projections.

## 2. Required outcome

The registered opaque prepend workload currently creates a temporary file,
writes a ten-byte prefix, copies the complete 32 MiB source through userspace,
fsyncs the temporary file, and renames it over the source.

The copy-range form must instead express:

```text
temporary[0..10]        = new prefix bytes
temporary[10..N + 10]   = immutable source[0..N]
```

For an eligible immutable source, LayerFS must:

- transfer only the ten new prefix bytes across the FUSE data plane;
- represent the copied destination range as a reference to the authenticated
  immutable source root;
- make the copied bytes readable immediately, before Commit;
- reuse unchanged payload objects and intact extent subtrees;
- build only boundary and ancestor metadata required by the new file root;
- admit only newly constructed candidate objects;
- preserve normal Workspace Commit and visibility-last durability;
- keep CPU and memory bounded independently of the copied byte length.

The fast-path cost shape is:

```text
O(tree height + changed boundary metadata + new bytes)
```

It must not be:

```text
O(copied payload bytes)
O(source extent count)
O(total Store objects)
O(repository history)
```

## 3. Non-goals

This refinement does not add:

- a private LayerFS SDK splice operation;
- `FICLONE`, `FICLONERANGE`, reflink ioctls, or clone-range emulation;
- an insert-range or collapse-range POSIX extension;
- inference that recognizes ordinary read/write traffic as a copy;
- a persistent or prewarmed execution shell;
- a prewarmed Workspace pool;
- a second Store, Push, Pull, Reference, Replica, or transfer path;
- per-syscall SQLite persistence;
- crash-recoverable Workspace spools;
- an OverlayFS path;
- a general immutable Workspace segment engine for every dirty-source case.

Those features require separate evidence. The first implementation uses the
standard `copy_file_range` operation already exposed by `fuser` and falls back
correctly when its structural fast path is not eligible.

## 4. Current evidence and defect

The current benchmark workload in
`benchmark/fs-bench-pro/workload.rs` uses:

```rust
writer.write_all(PREPEND)?;
std::io::copy(&mut source, &mut writer)?;
```

This hides the source/destination relationship from LayerFS. The observed
operation therefore moves approximately 32 MiB from FUSE to the application
and approximately 32 MiB back to FUSE before Commit discovers that almost all
durable payload already exists.

The retained seven-sample native-host report at:

```text
benchmark-results/fs-bench-pro/runs/
  v4-native-host-r006-repeat2-20260901/report.md
```

records:

```text
Workspace Create median       11.506667 ms
small-edit Commit median       5.189750 ms
opaque prepend median        229.131292 ms
host peak RSS                 98,254,848 bytes
host swaps                     0
```

These values are baseline evidence, not V3 copy-range results.

The content core already provides the essential persistent-tree behavior:

- structural split, replace, and join;
- zero unchanged-payload reads for retained destination ranges;
- bounded tree-height node work;
- immutable retained roots;
- an approximately 8 MiB deferred structural ceiling.

The missing pieces are:

1. LayerFS does not override `fuser::Filesystem::copy_file_range`, so the
   default is `ENOSYS`.
2. `FilesystemPort` and the proxy protocol have no copy-range operation.
3. Workspace overlays represent only base, dirty/charged ranges, and spool
   bytes; they cannot represent an immutable borrowed range.
4. `FileMutationBatch::replace` accepts replacement bytes through `Read`.
   It cannot yet splice a range from another `FileStateRoot` without scanning
   that range through CDC.

Adding only the FUSE callback is therefore insufficient. The Commit path
would still turn the copied source back into a byte stream.

## 5. Public operation and semantics

### 5.1 Standard operation

LayerFS implements the standard FUSE `copy_file_range` callback exposed by
`fuser 0.18`:

```text
source inode and handle
source offset
destination inode and handle
destination offset
requested length
flags
```

The operation returns the number of bytes copied. A zero-length request
returns zero. A source offset at or beyond EOF returns zero. A request that
extends beyond source EOF may return the available shorter length.

The initial implementation supports the standard zero-flags form. Unknown or
unsupported flags return the platform-appropriate typed error; they never
silently change semantics.

### 5.2 Byte-copy independence

`copy_file_range` copies bytes. Internal extent sharing must not create a live
relationship between the files.

After the operation:

- later source writes do not change destination bytes;
- later destination writes do not change source bytes;
- source rename or unlink does not invalidate the destination;
- hard links continue to obey shared-inode semantics;
- the destination is readable before Commit;
- Commit and retained history preserve the copied snapshot.

An immutable `FileStateRoot` is a safe snapshot. A pointer to a mutable
Workspace node is not.

### 5.3 Same-inode and overlap rules

Hard links resolve to the same Workspace `NodeId`. Source and destination
ranges on the same inode must follow Linux `copy_file_range` overlap rules.
Overlapping ranges are rejected before mutation. A same-inode case not proven
safe by focused tests uses the bounded fallback or returns the exact typed
unsupported/error result; it never produces memmove-like behavior by accident.

### 5.4 Partial completion

The structural fast path is all-or-error after preflight and normally returns
the complete available length.

The bounded fallback may return a positive partial byte count if a later
chunk fails, matching ordinary copy/write behavior. It returns an error only
when zero bytes completed. Deferred Workspace errors still surface at fsync,
pause, Commit, or another synchronization boundary under the existing rules.

## 6. Frozen end-to-end route

The only copy-range route is:

```text
fresh process in prepared container
  -> Linux copy_file_range syscall
  -> fuser Filesystem::copy_file_range
  -> LayerFs handle and inode validation
  -> FilesystemPort::copy_file_range
  -> bounded proxy Request::CopyFileRange
  -> one serialized Workspace worker operation
  -> immutable-root fast path or bounded host-local fallback
  -> immediate FUSE result
  -> later ordinary public Workspace Commit
```

No copied payload bytes appear in the proxy request. The request contains only
typed node identities, offsets, length, and flags. The response contains only
the completed byte count or typed error.

The container never supplies an `ObjectId` or `FileStateRoot`. The host
resolves the source `NodeId` through the pinned Workspace snapshot and
authenticates canonical objects through the existing Store reader.

## 7. FUSE and proxy contract

### 7.1 Filesystem adapter

`LayerFs` adds the `fuser::Filesystem::copy_file_range` override. The adapter:

1. validates source and destination handles and writable destination state;
2. converts FUSE inode numbers to exact Workspace `NodeId` values;
3. validates integer conversions and range overflow;
4. calls one `FilesystemPort` method;
5. returns the exact completed byte count through `ReplyWrite`;
6. maps the existing typed port errors to errno.

It does not read, buffer, hash, or copy file payload.

### 7.2 Port operation

The port adds one operation equivalent to:

```rust
fn copy_file_range(
    &self,
    source: NodeId,
    source_offset: u64,
    destination: NodeId,
    destination_offset: u64,
    length: u64,
    flags: u32,
) -> PortResult<u64>;
```

Use the project’s exact existing flag and byte-count types where required by
`fuser`; do not introduce a parallel public copy abstraction.

### 7.3 Wire operation

The protocol adds one bounded variant equivalent to:

```rust
Request::CopyFileRange {
    source: NodeId,
    source_offset: u64,
    destination: NodeId,
    destination_offset: u64,
    length: u64,
    flags: u32,
}
```

The decoder must reject malformed frames, invalid enum tags, trailing bytes,
and overflow under the existing protocol rules. The request frame size is
constant and independent of `length`.

## 8. Workspace representation

### 8.1 Borrowed immutable ranges

The existing file overlay gains the minimum additional state required to
represent immutable copied bytes:

```rust
struct BorrowedRange {
    destination_end: u64,
    source_root: FileStateRoot,
    source_offset: u64,
}
```

Borrowed ranges are stored in an ordered map keyed by destination start. They
must be nonempty, nonoverlapping, range-valid, and coalesced when adjacent
ranges reference adjacent bytes in the same source root.

This is final Workspace file state, not a replay log and not a durable table.

### 8.2 Fast-path eligibility

The first structural fast path requires all of the following:

- source and destination are regular files in the same active Workspace;
- source and destination are distinct inodes;
- flags are supported;
- source range resolves entirely to one authenticated immutable visible
  `FileStateRoot` with a linear source offset;
- source range does not intersect dirty zero, charged spool, or unresolved
  mutable source state;
- destination range does not intersect active charged spool bytes;
- all offsets and computed ends are valid;
- adding the borrowed interval passes the existing Workspace metadata policy.

An unchanged interval of an overlay may resolve to its immutable base root.
The implementation must prove that resolution; filename, benchmark case, and
byte-pattern recognition are forbidden.

### 8.3 Final-range precedence

Workspace reads resolve final bytes with unambiguous precedence:

```text
active spool bytes
borrowed immutable ranges
logical zero/hole ranges
unchanged destination immutable base
implicit zero gaps
```

The interval maps must remain nonconflicting so the read path does not depend
on incidental iteration order.

A successful copy-range operation immediately changes destination length to:

```text
max(old destination length, destination offset + completed length)
```

and marks the destination inode and its paths as mutated.

For overwrite semantics, Commit removes at most the copied bytes already
present at the destination:

```text
destination delete length =
  min(completed length, old destination length - destination offset)
```

An offset beyond the old destination EOF retains the existing logical-zero
gap behavior before the copied range.

### 8.4 Later mutations

A later destination write removes or splits borrowed intervals covered by the
write and makes the spool bytes authoritative. Truncate shrink trims or drops
borrowed ranges beyond the new EOF. Truncate growth does not invent borrowed
bytes. A later copy-range replaces or splits prior borrowed ranges.

Rename and unlink change namespace references only. They do not copy source
payload or invalidate a borrowed immutable root.

### 8.5 Bounded fallback

When the structural fast path is ineligible but the operation is semantically
supported, the Workspace worker performs a host-local bounded copy:

```text
Workspace read plan
  -> fixed buffer no larger than 1 MiB
  -> Workspace destination spool write
```

The fallback must:

- allocate no buffer proportional to the requested length;
- perform no Docker-to-host-to-Docker payload round trip;
- preserve source bytes for the duration of the synchronous operation;
- honor partial-completion behavior;
- use existing spool and resource limits;
- record that fallback occurred.

Fast-path optimization of dirty source ranges is deferred. Correct bounded
fallback is the required behavior.

## 9. Content-layer cross-root splice

### 9.1 Required primitive

`layerfs-content` adds one cross-root range operation to the existing rope
mutation implementation. Its effective contract is:

```rust
FileMutationBatch::replace_from_root(
    destination_start: u64,
    destination_delete_len: u64,
    source_root: FileStateRoot,
    source_start: u64,
    source_len: u64,
) -> CoreResult<()>;
```

The exact Rust signature may differ to fit the existing ownership model, but
there must be one shared implementation. Workspace, tests, and future callers
must not duplicate split/join logic.

### 9.2 Algorithm

The primitive reuses the existing authenticated rope machinery:

```text
source_before, source_tail = split(source, source_start)
source_middle, source_after = split(source_tail, source_len)

destination_left, destination_tail = split(destination, destination_start)
discarded, destination_right = split(destination_tail, delete_len)

result = concat(destination_left, source_middle, destination_right)
```

Only boundary paths and required ancestors may be decoded or emitted. Interior
source subtrees remain referenced by their existing `ObjectId` values.

The operation must not:

- read unchanged payload bytes;
- send source bytes through FastCDC;
- re-encode or remint unchanged payload objects;
- enumerate all source extents;
- materialize the copied range in memory;
- bypass canonical-object authentication;
- create a second extent-tree implementation.

### 9.3 Canonical result

The result must be a valid canonical LayerFS object graph and must pass all
existing file-state, extent-node, occupancy, length, identity, and reachability
validators.

The logical bytes must exactly equal ordinary byte-copy semantics. The
structural root is required to be deterministic for this operation and equal
between direct content tests, Workspace Commit, FUSE, and explicit
materialization of the same resulting root.

It is not required to equal the root produced by rebuilding the complete byte
stream through FastCDC. LayerFS canonical encoding authenticates exact object
representations; persistent structural edits already permit a valid retained
extent mapping without rereading and rechunking the complete file.

### 9.4 Reuse guarantee

The normative guarantee is:

> All unchanged payload objects and intact interior extent subtrees are reused
> without reading unchanged payload bytes. Only source/destination boundary
> nodes and required result ancestors may be rebuilt.

The implementation must not promise that the complete former mapping root is
always retained as one child. Tree occupancy and boundary rebalancing may
replace a small number of structural nodes.

## 10. Commit and Store behavior

Copy-range remains an ephemeral Workspace mutation. It performs no SQLite
write and makes no durable head visible.

At public Workspace Commit:

1. the candidate planner combines spool-backed replacement bytes and borrowed
   root ranges into the final file root;
2. replacement bytes use existing streaming CDC;
3. borrowed ranges use `replace_from_root`;
4. only newly emitted canonical objects enter the candidate;
5. local candidate membership checks only candidate IDs;
6. only membership-missing candidate objects are admitted;
7. the immutable Commit and Branch head CAS remain visibility-last.

The copied source root is already part of the pinned complete visible Store
state. Its payload and structural objects are not copied into the candidate.
Completeness remains inductive under the one-Store proof in
`docs/research/history/v2-replacement/spec.md`.

Normal Commit must not perform a full-root closure scan merely because the new
root references an existing source subtree. Commit SQL work remains
proportional to newly emitted candidate IDs and independent of copied file
size.

## 11. Resource and safety contract

### 11.1 Memory

- A copy-range request allocates no memory proportional to `length`.
- The structural fast path uses existing bounded rope node state.
- The fallback uses one fixed buffer no larger than 1 MiB.
- Every borrowed interval is charged to the existing Workspace metadata/final
  delta policy before mutation.
- Borrowed interval splitting must fail safely before exceeding the policy.
- Adjacent compatible borrowed intervals are coalesced.
- No complete extent list or source object list is materialized.

### 11.2 CPU

For an eligible immutable range, source work is bounded by tree height and
boundary nodes. There is no CDC over copied source bytes and no hash over
copied source payload.

Fallback work may be linear in copied bytes but remains bounded and explicit
in telemetry.

### 11.3 Storage

The fast path charges and writes:

```text
new prefix/replacement payload
new boundary extent nodes
new ancestor nodes
changed inode/directory/root metadata
```

It does not charge copied logical length as Workspace spool bytes and does not
insert the existing source payload set again. Separate logical file-size and
metadata limits still apply.

### 11.4 Transactions

No FUSE read/write, copy loop, hashing, CDC, tree traversal, or range-map work
occurs inside a SQLite writer transaction. Existing bounded candidate
membership remains before writer transactions. Existing object-admission and
final visibility transaction limits remain unchanged.

## 12. Errors and integrity

The operation must return a typed error without visible partial structural
mutation for:

- invalid source or destination handle;
- non-file source or destination;
- read-only or closed destination;
- arithmetic overflow;
- invalid or unsupported flags;
- prohibited same-inode overlap;
- malformed proxy request;
- missing or corrupt visible source canonical object;
- source summary or extent validation failure;
- Workspace state or lease failure;
- metadata or spool resource-limit rejection before work begins.

Missing or corrupt immutable source data is `Integrity`; it never silently
falls back to unauthenticated bytes. A content-address collision or unexpected
canonical bytes for an existing `ObjectId` is `Integrity` under existing Store
rules.

## 13. Monitoring and receipts

Receipts must make the optimization observable without logging paths, payload,
or unbounded per-range detail.

At minimum record bounded operation totals for:

```text
copy_file_range_calls
copy_file_range_requested_bytes
copy_file_range_completed_bytes
copy_file_range_fast_path_calls
copy_file_range_fast_path_bytes
copy_file_range_fallback_calls
copy_file_range_fallback_bytes
copy_file_range_source_payload_bytes_read
copy_file_range_spool_bytes_written
copy_file_range_borrowed_ranges_created
copy_file_range_borrowed_ranges_peak
copy_file_range_elapsed_ns
```

Content/Commit evidence for the benchmark case must also expose:

```text
rope_source_nodes_read
rope_payload_bytes_read
rope_nodes_created
cdc_bytes_scanned
candidate_objects
candidate_bytes
inserted_objects
inserted_bytes
reused_objects
reused_bytes
membership_pages
writer_transactions
writer_transaction_max_ns
```

The immutable fast path hard proof is:

```text
completed bytes             == source logical length
fallback bytes              == 0
source payload bytes read   == 0
CDC bytes scanned           == new literal bytes only
spool bytes written         == new literal bytes only
candidate                   == inserted + reused
```

## 14. Benchmark contract

### 14.1 Existing opaque row remains

The existing row remains unchanged:

```text
prepend-temp-copy-rename
```

It continues to use `BufReader`, `BufWriter`, and `std::io::copy`. It measures
unmodified applications that communicate only ordinary reads and writes.

LayerFS must not recognize this workload, filename, ten-byte marker, size, or
read/write sequence and secretly convert it into a range clone.

### 14.2 New standard-operation row

Add a separate row:

```text
prepend-copy-range-rename
```

The workload executable performs:

```text
open source A
create temporary B
write exact ten-byte prefix to B
loop copy_file_range(A, source_offset, B, 10 + destination_offset, remaining)
fsync B
rename B over A
```

The loop handles short copies and fails on zero progress before EOF. It does
not call an internal LayerFS API.

### 14.3 Public lifecycle

The timed LayerFS path is exactly:

```text
T0 public SDK Workspace Create
   real FUSE mount readiness
T1 public SDK Exec
   one fresh workload process
   prefix write + copy_file_range + fsync + rename
T2 public SDK Workspace Commit
T3 public SDK Workspace End
T4 complete
```

The standard shell/runtime is not prewarmed. The container and image may be
prepared before T0 under the existing benchmark contract. Store, Workspace,
fixture result, candidate, and execution output caches may not be prepared or
reused across timed samples.

### 14.4 Fair comparison with Computer

LayerFS and Computer receive the same prepared source bytes, workload binary,
fresh-process boundary, copy-range request sequence, fsync, rename, and
acknowledgement boundary.

Each sample reports one of:

```text
copy_range_path=syscall
copy_range_path=userspace_fallback
copy_range_path=unsupported
```

Unsupported behavior or fallback must not be hidden. Syscall success alone
does not prove that Computer performed a zero-copy or extent-aware operation.
LayerFS receipts separately prove
`copy_file_range_fast_path_calls` versus
`copy_file_range_fallback_calls`. The table reports raw times and both visible
path classifications. The row is a standard-operation feature comparison,
not a claim about opaque applications.

A future LayerFS-only SDK splice, if implemented, must appear as a separately
labeled capability row and must not enter the cross-product registered total.

### 14.5 Correctness oracle

After T4, an untimed public-SDK proof remounts the committed root and verifies:

- exact size `32 MiB + 10 bytes`;
- exact SHA-256 against an independently built byte-copy oracle;
- prefix bytes at offset zero;
- exact former source bytes at offset ten;
- source history remains readable;
- no temporary file remains visible.

## 15. Performance expectations and gates

These are targets, not results.

For the 32 MiB immutable-source case on the established native-host prepared
container environment:

| Phase | Initial target | Mature target |
| --- | ---: | ---: |
| Workspace Create | 8-13 ms | 7-10 ms |
| Fresh process, prefix, copy-range, fsync, rename | 7-18 ms | 5-10 ms |
| Commit | 6-12 ms | 4-8 ms |
| Workspace End | 3-5 ms | 3-4 ms |
| Complete lifecycle | **30-55 ms** | **22-40 ms** |

Initial hard gates after seven valid paired samples are:

```text
median complete                         <= 55 ms
p95 complete                            <= 80 ms
median Commit                           <= 12 ms
fast-path samples                       == all valid LayerFS samples
fallback bytes                          == 0
source payload bytes read               == 0
spool bytes written                     == 10 bytes
host peak RSS                           < 128 MiB
aggregate lifecycle RSS                 < 256 MiB
swap/OOM                                == 0
```

Size-scaling proof uses the same operation at 32 MiB, 512 MiB, 1 GiB, and a
synthetic or real 10 GiB immutable source permitted by the environment. It
must show no payload-proportional FUSE traffic, spool bytes, CDC bytes, or
candidate bytes. Tree-node work may grow logarithmically.

The benchmark report must not invent a Computer target or require Computer to
be slower. It reports the measured paired speedup after both paths are proven.

## 16. Required focused proof

### 16.1 Content tests

- whole-root cross-file splice with zero payload reads;
- unaligned source start and end;
- source range at file start, middle, end, and complete file;
- empty and zero-length ranges;
- destination replace, append, and extension;
- multi-level extent trees;
- boundary rebalance and root-height changes;
- retained source root remains readable;
- byte equality against an ordinary-copy oracle;
- deterministic root for repeated identical structural operations;
- malformed, missing, and identity-mismatched source objects;
- memory ceiling and node-read counters;
- no source extent enumeration proportional to copied size.

### 16.2 Workspace tests

- clean immutable source fast path;
- unchanged source interval inside an overlay;
- dirty source bounded fallback;
- charged destination overlap bounded fallback;
- read-after-copy before Commit;
- write over the start, middle, and end of a borrowed range;
- truncate through and beyond borrowed ranges;
- adjacent borrowed-range coalescing;
- later copy replacing a borrowed range;
- source write after copy does not affect destination;
- source rename and unlink after copy;
- hard-link identity and overlap rejection;
- resource rejection leaves destination unchanged;
- fallback buffer and interval metadata remain bounded.

### 16.3 FUSE and proxy tests

- protocol round trip and malformed-frame rejection;
- exact node/offset/length/flags transport;
- handle and write-permission validation;
- short-copy and zero-progress behavior;
- errno mapping;
- real FUSE read-after-copy;
- fsync and deferred-error boundary;
- one request carries zero payload bytes;
- reconnect/failure never replays a mutating request ambiguously.

### 16.4 Commit and Store tests

- copied payload objects are not candidate objects;
- missing-only candidate membership;
- zero full-root closure traversal;
- zero copied payload insertion;
- final visibility CAS remains last;
- failure injection never exposes an incomplete head;
- post-Commit materialization and FUSE bytes agree;
- retained source and destination history remain independently readable.

### 16.5 Live benchmark proof

- at least one diagnostic run with full timers;
- seven valid LayerFS samples;
- seven paired Computer samples when Computer supports the registered row;
- opaque prepend row retained unchanged in the same report;
- exact raw JSONL and environment metadata retained;
- no earlier result substituted;
- no setup or warmup sample included in the distribution.

## 17. Implementation order

Implement and verify in this order:

1. Add and prove the content-layer cross-root splice using existing rope
   split/join machinery.
2. Add borrowed immutable ranges and bounded fallback to Workspace.
3. Add the port, wire protocol, proxy host/client, and FUSE callback.
4. Add bounded receipts and counter assertions.
5. Add the standard copy-range workload and separate benchmark row.
6. Run focused tests, directly dependent crate tests, workspace tests, full
   gates, real FUSE proof, and the seven-sample benchmark.

After each stage, fix root causes before proceeding. A compile failure, failed
proof, fallback in an eligible case, payload scan, unbounded allocation,
incorrect bytes, or benchmark regression is `REVISE`, not completion.

## 18. Terminal acceptance

This refinement is complete only when all of the following pass together:

- standard FUSE `copy_file_range` is implemented end to end;
- eligible immutable ranges use the structural fast path;
- ineligible supported cases use the bounded correct fallback;
- copied bytes are visible before Commit;
- source and destination independence is proven;
- cross-root rope splice reads zero unchanged payload bytes;
- copied source payload and subtrees are not rebuilt or readmitted;
- Workspace and Commit memory remain within frozen bounds;
- no copy-range work occurs inside SQLite writer transactions;
- opaque prepend remains unchanged and honestly measured;
- copy-range prepend uses public SDK lifecycle plus a fresh process and real
  FUSE;
- LayerFS and Computer receive the same standard syscall workload;
- raw receipts prove the selected fast/fallback path;
- exact post-Commit bytes and retained history pass;
- focused, dependent, workspace, and full verification gates pass;
- measured performance and scaling evidence are saved under
  `benchmark-results/fs-bench-pro`.

## 19. Explicitly deferred work

The following remain separate V3 decisions:

- dirty-source symbolic cloning instead of bounded host-local fallback;
- immutable append-only Workspace spool segments;
- general insert-range and collapse-range APIs;
- clone/reflink ioctls;
- a public LayerFS SDK splice capability;
- operation-aware incremental namespace planning for unrelated large-repository
  costs;
- generic Workspace interval compaction beyond the bounds required here;
- OverlayFS support.

They must not block the standard immutable-source copy-range path, and they
must not be scaffolded speculatively in its implementation.
