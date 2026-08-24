# Apple/APFS PoC implementation checklist

Status: **execution ledger; no item is complete merely because a design exists**.

This checklist turns the preceding documents into one implementation sequence.
Mark an item complete only with a source link and a passing focused test. Avoid
adding another planning document when a checked item and test are sufficient.

Primary contracts:

- [scope and decisions](00-scope-and-decisions.md)
- [architecture and file structure](01-architecture-and-file-structure.md)
- [data structures and algorithms](02-data-structures-and-algorithms.md)
- [operation workflows](03-operation-workflows.md)
- [Apple/APFS materialization and recovery](04-apple-apfs-materialization-and-recovery.md)
- [minimal implementation plan](05-minimal-implementation-plan.md)
- [correctness and fast verification](06-correctness-and-fast-verification.md)
- [native workspace and Bash verification](08-native-workspace-and-shell-verification.md)
- [portability and Apple completeness](09-portability-and-apple-completeness.md)
- [final handoff freeze](10-handoff-freeze.md)
- [Stage One performance completion](12-stage1-performance-completion.md)
- [Stage One implementation and complexity](13-stage1-implementation-and-complexity.md)
- [100 MiB operation campaign](14-stage1-single-file-benchmark.md)
- [real workspace campaign](15-stage1-workspace-benchmark.md)

Current native disposition (2026-08-24): **PASS for the frozen
AppleWorkspaceV1 PoC scope**. The active host synthesizes exact
`com.apple.provenance` on every
new file/directory and regenerates it after removal. The Apple adapter now
classifies only that exact name as environmental: never canonicalized,
restored, or equality-relevant. Every other exclusion remains fail-closed.
Focused Apple metadata tests and the SDK Bash/capture/reopen workflow pass
without deletion workarounds.

Terminal evidence: legacy v1/v2 writers are test-only, final-reachable mutation
emission is proved, APFS clone/same-offset fallback parity is exercised, the
workspace test/clippy/static gates pass, and the one frozen three-run S0-S12
campaign completed with exact oracles, terminal Q/FD equality, and zero owned
residue. Changed-root incremental materialization remains an explicit PoC v1
exclusion rather than an implemented claim.

That exclusion describes the completed AppleWorkspaceV1 baseline. Stage One
prospectively reopens changed-root refresh, direct canonical SDK operations,
repeatable managed checkpoints, and performance closure under documents
12–15; it does not retroactively relabel the v1 evidence.

## 1. Completion definition

```text
fresh Store
  -> create/import root A
  -> materialize A into an APFS directory
  -> expose the ordinary workspace and run a real /bin/bash child
  -> wait for quiescence and capture direct content/path/mode/symlink changes
  -> managed overwrite/insert/delete/rename
  -> capture root B
  -> close and reopen process/Store
  -> materialize A and B independently
  -> exact native-tree comparison
  -> capture one arbitrary external edit through the full-scan route
  -> fork a retained root and diverge
  -> rollback one ref with expected-head protection
  -> read every retained root exactly
  -> run one compact real-workspace/Bash measurement
```

The PoC is incomplete if this workflow imports a Phase-4 benchmark module,
uses a fixed benchmark root/fixture, or requires APFS clone success.

## 2. Pre-implementation decisions

- [x] Record the fresh profile ID domain and confirm it cannot collide with v1,
  v2, or the provisional/unimplemented CD32–64 v3 proposal.
  Evidence: `content/extent_codec.rs`; literal digest in `tests/extent_codec.rs`.
- [x] Freeze extent leaf/internal tags, entry widths, `64..=128` occupancy,
  split/borrow/merge precedence, root-collapse rule, and maximum level.
  Evidence: `content/{extent,extent_codec,rope}.rs`; focused codec/model tests.
- [x] Accept the identity ADR: `FileStateRoot` is operational/history-shaped;
  optional `ContentDigest` owns semantic complete-byte equality.
  Evidence: `content/extent.rs::FileStateV3`; retained roots differ by operation history.
- [x] Remove mode from FileStateV3/DirectoryStateV1; freeze mode + mtime in one
  PortableMetadataV1 authority reached through InodeRecordV1.
  Evidence: mode-free `content/extent.rs::FileStateV3`; persistent metadata-tree tests.
- [x] Freeze StoreId + transactional next-inode-serial allocation and preserve
  both exactly through abort, reopen and compaction.
- [x] Confirm legacy v1/v2 readers are compatibility-only and no new PoC write
  emits their file mapping.
  Evidence: new writes route through `content/extent_codec.rs`; legacy golden suite passes.
- [x] Freeze the persistent namespace node byte limit/minimum fill, name limit,
  entry kinds, split/borrow/merge precedence, root rules and profile ID.
- [x] Freeze canonical names as exact 1..=255-byte UTF-8 excluding `.`, `..`,
  NUL, slash and backslash; native representability/collisions remain driver
  admission, not canonical normalization. The Apple driver preflights exact
  sibling spellings through APFS before visible mutation.
- [x] Confirm the old complete-`BTreeMap` namespace is legacy/model input only;
  every new product mutation path-copies persistent namespace nodes.
- [x] Set SQLite `busy_timeout=0`; assert exact immediate Busy/Locked and no
  SQLite/internal/application retry. Keep this runtime policy out of canonical
  profile identity.
  Evidence: `layerfs-engine/src/lib.rs`; `sqlite_error_mapping_preserves_busy_and_no_space`.
- [x] Confirm the PoC uses synchronous materialization. Do not extract the G5
  exact/latest mailbox unless an actual asynchronous caller appears.
- [x] Confirm arbitrary external capture walks the complete supported namespace
  and scans every regular file because neither complete paths nor ranges are
  authoritative.
- [x] Confirm hard links and frozen Apple extension metadata are required;
  device/FIFO/socket kinds, online/in-place GC, transparent write interception,
  legacy migration, and production portability remain typed/deferred rather
  than silently flattened.

## 3. Package A — canonical persistent file state

### 3.1 Files

- [x] Add `crates/layerfs-core/src/content/extent.rs`.
- [x] Add `crates/layerfs-core/src/content/extent_codec.rs`.
- [x] Add `crates/layerfs-core/src/content/rope.rs`.
- [x] Add `crates/layerfs-core/tests/extent_codec.rs`.
- [x] Add `crates/layerfs-core/tests/extent_model.rs`.
- [x] Add `crates/layerfs-core/src/namespace.rs`.
- [x] Add `crates/layerfs-core/src/namespace_codec.rs`.
- [x] Add `crates/layerfs-core/src/inode.rs` and `metadata.rs`.
- [x] Add `crates/layerfs-core/tests/namespace_model.rs`.
- [x] Update `content/mod.rs` to expose the persistent rope without
  loading all extents into a `Vec` for durable operations.
  The old flat `LogicalFile` writer is crate-private compatibility/golden code;
  product callers use `content::rope`.
- [x] Update existing modules only where the selected profile requires it; no
  parallel fresh-profile representation or writer remains public.

### 3.2 Codec and validation

- [x] Encode/decode the exact fresh-profile common header.
- [x] Encode/decode 40-byte `ExtentSliceV3` entries.
- [x] Encode/decode 48-byte cumulative child descriptors.
- [x] Encode/decode canonical mode-free `FileStateV3` with profile ID, logical
  length, extent count, tree level, and mapping-root ID; define
  `FileStateRoot` as this record's ObjectId.
- [x] Authenticate every fetched node under the requested role and profile.
- [x] Reject wrong magic/version/tag/level/flags/count/trailing bytes.
- [x] Reject zero-length nonempty extents and checked source-range overflow.
- [x] Decode the referenced payload and prove every slice lies within it.
- [x] Reject noncanonical adjacent coalescible slices.
- [x] Validate leaf occupancy, branch occupancy, cumulative totals, child level,
  maximum height, empty-root form, root expansion, and root collapse.
- [x] Freeze independent canonical bytes/ObjectIds for empty, one-entry, min,
  max, leaf split, branch split, and multi-level roots.

### 3.3 Algorithms

- [x] Implement full streamed construction.
- [x] Implement byte-offset locate with bounded-node binary search.
- [x] Implement point/range/full streaming reads.
- [x] Implement persistent split at start/middle/extent boundary/EOF.
- [x] Implement deterministic join with borrow/merge/root-collapse rules.
- [x] Implement overwrite as split + replacement build + join.
- [x] Implement insert without touching an unaffected suffix subtree.
- [x] Implement delete without copying deleted or suffix payload bytes.
- [x] Implement append and truncate through the same shared operations.
- [x] Preserve old roots after every path-copy operation.
- [x] Emit structural counters from the product algorithms, not literal test
  constants.

### 3.4 Package-A exit

- [x] Deterministic `Vec<u8>` differential model passes overwrite, insert,
  delete, append-equivalent insertion, truncate-equivalent deletion and reads.
- [x] Deterministic randomized sequences pass after every intermediate edit;
  the test also retains and rereads the original root.
- [x] All retained roots reconstruct exactly.
- [x] Corruption/wrong-role/missing-node/overflow tests fail with exact errors.
- [x] Managed local operations satisfy `O(B + log E)` structural work.
- [x] No local operation owns memory proportional to `F` or `E`.
- [x] `cargo fmt`, core check, and core tests pass.

### 3.5 Persistent namespace

- [x] Encode/decode canonical directory state, byte-bounded leaf/branch nodes,
  name-to-InodeId entries, inode-table nodes/records, regular/directory/symlink
  targets, exact symlink targets and typed extension metadata.
- [x] Reject invalid names, duplicate keys, wrong kinds, noncanonical fill,
  trailing bytes, overflow, wrong levels and redundant roots.
- [x] Full directory/inode/metadata visitors authenticate each node once,
  reject repeated/wrong levels before grandchildren, and remain linear; inode
  decoding rejects count 128, level 32, and oversized input before allocation.
- [x] Implement component/path lookup, create, remove, replace, same-directory
  rename and cross-directory rename through persistent path-copy.
- [x] Implement hard link/unlink as shared InodeId plus checked
  `namespace_ref_count`
  updates; preserve one content mutation across every linked path.
  Core removal has bounded path-copy merge/root collapse tests; external native
  link/unlink capture validates the complete alias count and rebuilt closure.
- [x] Preserve unchanged directory subtrees and all old namespace roots.
- [x] Differential-test against a nested ordered map.
- [x] Force multi-level split, borrow, merge and root collapse with at least
  10,000 deterministic synthetic names without adding large file payloads.
- [x] Assert namespace nodes read/created are proportional to changed spines,
  never complete directory entry count.

## 4. Package B — one reusable durable engine

### 4.1 Reconcile, do not copy

- [x] Start from `crates/layerfs-engine/src/lib.rs` and its `layerfs_*` schema.
- [x] Inventory each required G5 behavior from `wp4m_*` benchmark code against a
  source test or accepted frozen vector.
- [x] Add only the minimum missing reusable behavior under the existing engine,
  `integrity.rs`, `refs.rs`, and `publication.rs`.
- [x] Do not import a `src/bin` module from product code.
- [x] Do not create a second SQLite Store/schema/projector implementation.
- [x] Keep payload/node canonical identities independent of SQLite row IDs.
- [x] Remove benchmark-bin product ownership only after new library tests prove
  parity and repository search shows no active caller.

### 4.2 Store and publication

- [x] Configure and assert `DELETE`, `FULL`, `FILE`, and `mmap_size=0`.
- [x] Freeze one writer and at most two query-only readers, no pool,
  `cache_size=1280`, observed 4 KiB page size, and report configured cache
  budget separately from actual RSS/Q.
- [x] Implement authenticated put-if-absent and unequal-incumbent rejection.
- [x] Read-only preflight existing `sqlite_schema`, Store metadata, and authority
  before assigning PRAGMAs or DDL; foreign and missing-meta/authority databases
  remain byte- and row-unchanged on refusal.
  Internal SQLite objects are excluded only by exact `NOT GLOB 'sqlite_*'`;
  `sqliteX` tables/triggers are visible and rejected for stores and candidates.
- [x] Implement authenticated complete and exact-range object reads.
- [x] Replace current redundant object loads with one SELECT, one borrowed-row
  authentication, one strict decode and no separate length query.
- [x] Implement ordered payload batches of at most 64 references, preserving
  duplicate occurrence order and exact missing/wrong-role errors.
- [x] Prepare only bounded operation descriptors/evidence before `BEGIN`; do
  not accumulate a large candidate object set in memory or a durable carrier.
- [x] After `BEGIN` and expected-state validation, stream/authenticate/insert
  every SQLite object/root/delta/ref row inside that one writer transaction.
- [x] Check expected ref/root plus generation before publication.
- [x] Dispatch exactly one publication COMMIT for a state change.
- [x] Dispatch zero publication COMMITs for a normalized no-op.
- [x] Reconcile ambiguous COMMIT outcome from a fresh connection as requested,
  prior, different, or indeterminate; never blind-redispatch.
  Fresh reconciliation is read-only, repeats exact schema admission, and binds
  StoreId before ref classification; missing/replaced paths are focused-tested.
- [x] Preserve exact Busy, Locked, no-space, corruption, permission, constraint,
  and I/O error classes.

### 4.3 Integrity and history

- [x] Keep `Verified` as default.
- [x] Make `TrustedLocalDev` explicit and Store-lifetime.
- [x] Keep fetched/new/incumbent identity checks common and unconditional.
- [x] Prevent trusted assumptions from becoming verified receipt authority.
- [x] Perform the required Verified-after-Trusted reopen verification.
  Initial open and live-handle revalidation serialize check/verify/clear in
  one immediate transaction.
- [x] Implement named/internal refs with root + generation expected-state update.
- [x] Implement checkpoint/fork as a new ref to an immutable retained root.
- [x] Implement rollback as an expected-state ref move, not object mutation.
- [x] Require fork/rollback targets to already belong to retained-root authority.
- [x] Implement direct historical root read without replaying later history.
- [x] Implement read-only reachability enumeration from every retained ref/pin.
- [x] Add exclusive-maintenance admission and Store-generation lifetime pins.
  The shared pin is acquired before `CURRENT` is read and is carried by the
  exact opened Engine generation.
- [x] Mark the authenticated union of every retained ref/checkpoint and reject
  compaction while any reader/writer/workspace/recovery pin exists.
  Evidence: role/context-keyed indexed disk work table, named-ref plus retained
  seeding, SDK `Arc::try_unwrap`, and generation maintenance-lock tests.
- [x] Stream the retained union to one same-directory sibling Store, preserving
  exact canonical bytes, schema/profile and ref generations.
- [x] Preflight available space for the new sibling generation allocated upper
  bound + disk-backed mark database + candidate SQLite rollback-journal/temp
  high-water bound + `CURRENT.tmp` and safety margin; fail before copy when it
  cannot fit.
- [x] Report total peak as retained old generation + new generation + mark
  database + candidate journal/temp + selector temporary bytes.
- [x] Install through checksummed `CURRENT` and StoreGenerationDriver; recovery
  never guesses highest generation filename.
  The selected/prior install paths pass; the complete sync/crash fault matrix remains.
- [x] Read selectors with fixed 154+1-byte storage and keep directory-sync
  failure durability-ambiguous without deleting the prior generation. Missing
  `CURRENT` with any generation fails closed; only exact next-candidate/partial
  selector residue is recovered, using read-only schema/StoreId inspection that
  preserves empty, foreign, and unknown generation bytes.
- [x] Verify every retained root before durably swapping Store generation.
- [x] Freshly reopen the installed Store before removing the old backup.
- [x] Recover exactly across sibling-COMMIT/sync/swap/reopen/cleanup faults;
  never remove the only verified Store generation.

### 4.4 Package-B exit

- [x] Object/store parity tests pass.
- [x] Stale head/ref conflicts before visible publication.
- [x] T0–T6 publication fault/restart matrix passes through real child-process
  dependency-inverted driver boundary; product semantics contain no
  `cfg(test)` fault branches.
- [x] One writer/multiple pinned readers return exact roots without switching.
- [x] Fork divergence and rollback preserve every retained root.
- [x] A 1,000-tiny-revision correctness sequence remains directly readable.
- [x] Offline compaction removes only authenticated-unreachable objects and all
  retained/forked/rollback roots remain exact after process reopen.
- [x] Owned writer transaction, connection, descriptor, and Q state returns to
  terminal baseline/zero.
- [x] Engine format/check/tests pass.

## 5. Package C — universal VFS plus Apple/APFS driver

### 5.1 Native OS boundary

- [x] Add the OS-neutral `layerfs-vfs/src/driver.rs` port and an in-memory/fault
  conformance driver; keep VFS free of concrete syscalls and platform cfg.
- [x] Add `layerfs-os/src/apple/{mod,workspace,apfs,metadata,ffi}.rs`; move
  Apple-only `libc` ownership from the benchmark crate if still required.
- [x] Replace the OS crate's non-overridable `forbid(unsafe_code)` with
  deny-by-default and allow unsafe only in one reviewed `apple::ffi` submodule;
  expose safe wrappers for every required syscall and test exact errno/partial
  I/O behavior. The reviewed boundary exists; the complete errno/partial-I/O matrix remains.
- [x] Implement no-follow destination/directory admission.
- [x] Walk and pin every top-level parent component with no-follow opens; create
  managed roots exclusively and preserve a colliding caller-owned tree.
- [x] Preflight every canonical sibling set in private same-volume staging and
  reject unrepresentable/case-/normalization-colliding APFS names with typed
  errors before visible mutation or Complete live authority. Focused APFS tests
  cover case-only and NFC/NFD collisions.
- [x] Record pinned directory and file identity needed to detect substitution.
- [x] Create private same-directory temporary files with unique ownership.
- [x] Use one mode-0700 same-volume staging directory; do not extract the G5
  ownership-xattr helper, and assert no private LayerFS xattr reaches output.
- [x] Implement bounded full-stream file construction.
- [x] Implement optional APFS clone attempt.
- [x] Implement same-size `pwrite` patch ranges only after seed authority.
- [x] Implement file sync, atomic one-file rename, and directory sync.
- [x] Reconcile selector install and directory-sync lost acknowledgements as
  requested/prior/different and verify the selected generation before success.
- [x] Reconcile clone metadata as an exact set: remove seed-only xattrs, replace
  ACL/mode/xattrs, apply restrictive flags last, read back and verify.
- [x] Use one final file sync after content and exact metadata, then rename and
  parent-directory sync; record achieved durability class.
- [x] Implement owned-temp cleanup that never unlinks a substitute.
- [x] Return typed unsupported/fallback/error outcomes; clone failure is not a
  correctness failure when full stream is available.

### 5.2 Materialization

- [x] Add the VFS workspace, materialize, and capture modules named in the
  architecture document. Live no-op authority and managed spool remain open.
- [x] Implement cold/full tree materialization in canonical directory order.
  Authenticated directory and metadata leaves stream through bounded visitors;
  one APFS preflight session covers the complete sibling set without a name Vec.
  Public root binding is checked at entry and after final root sync.
- [x] Implement exact-root no-op with zero content rewrites only under
  uninterrupted exclusive live-managed Store/workspace/generation/mutation
  authority; inode/size/mtime or an old record never suffices.
- [x] Implement managed verified-parent same-size clone/patch.
- [x] Route a missing/replaced/unverified managed seed to complete stream.
- [x] Route a length-changing managed native edit to complete stream.
- [x] Scope changed-root incremental installation out of AppleWorkspaceV1 PoC
  rather than overclaim it. PoC v1 accepts cold empty construction or an
  exact-root no-op; it does not persist projection provenance and does not
  claim an arbitrary changed-root refresh.
- [x] After every file rename, reconcile and verify that file before advancing,
  without claiming a
  complete tree.
- [x] Install process-lifetime `Complete` live authority only after the entire
  native tree is freshly exact and required directory sync has completed; do
  not persist projection intent/receipt in PoC v1.
- [x] On interruption, return no live wrapper/authority; a subsequent nonempty
  projection must verify the exact root or returns `ExternalDirtyConflict`;
  rebuild or resume only from exact intent/progress authority.

### 5.3 Capture

- [x] Implement bounded managed change descriptors bound to Store/workspace/
  generation/base root.
- [x] Define managed coordinates relative to the current pending state; preserve
  exact call order and never sort count-changing operations across calls.
- [x] Mutate the private native workspace and spool exact replacement bytes in
  a LayerFS-owned process/workspace-lifetime file with digest/offset binding.
- [x] Cap descriptors at 64; require capture/discard at the bound and keep spool
  bytes out of RAM/Q except for bounded streaming windows.
- [x] Freeze and revalidate managed edit evidence before capture.
- [x] Replay descriptors in call order against the base root, stream managed
  bytes from the spool, compare the result with native state, and use the one
  engine publication path.
- [x] For arbitrary external capture, walk the complete supported native
  namespace; do not use advisory event candidates as completeness authority.
- [x] Full-scan every supported regular file with bounded buffers, detecting
  additions, removals, renames, kind changes, metadata changes, and content.
- [x] Capture symbolic links with `lstat`/`readlink` and never follow them.
- [x] Group native hard links, preserve/reuse one canonical `InodeId`, and
  verify stable native link count equals aliases inside workspace; use a
  disk-backed scratch table and return `ExternalHardLinkBoundary` for external
  aliases; reject device/FIFO/socket kinds with typed errors.
- [x] Capture/materialize xattrs, resource forks, supported ACLs and BSD flags
  through typed canonical Apple extension metadata.
- [x] Encode zero BSD flags only by metadata absence and reject a present
  canonical zero-flags value during Verified publication.
- [x] Reject setuid/setgid input before applying the portable mode mask.
- [x] Reject ambiguous/replaced/symlink native identity according to the frozen
  policy.
- [x] Make capture/discard mutually exclusive terminal successes.
- [x] Bind managed projection and replay to one pre-projection `main` RefState;
  reject historical roots, make every failed native rename dirty, and consume
  the wrapper on successful discard.
- [x] Share writer leases by pinned workspace identity across wrappers and
  atomically exclude writer acquisition from capture; reject registration after
  capture/discard. Successful replace descriptors carry changed-file metadata
  and rename descriptors carry both affected parents; replay updates only those
  metadata roots with the existing content/directory path copies. Dirty capture
  fails closed and requires discard or explicit cooperative conversion.
- [x] Retain the originally pinned native workspace for capture, mutation,
  rename, and owned cleanup; no later operation reopens the retained pathname.
- [x] Revalidate the public parent/basename binding before and after capture and
  immediately before publication; detach owned roots to an exclusive private
  tombstone and reverify identity before recursive deletion. Each descendant is
  likewise quarantined exclusively and post-verified before unlink.
- [x] Store managed spool bytes in a driver-owned pinned private temp handle;
  VFS owns no predictable spool pathname. Dirty capture records its committed
  root before cleanup, making retries cleanup-only.
  Metadata evidence is serialized into that spool and descriptors retain only
  bounded offset/length pairs; replay decodes one item at a time.
- [x] Bound the xattr name list and aggregate native xattr name/value memory to
  1 MiB; top-level workspace admission pins the parent and opens/creates only
  the basename with no-follow `*at` operations.
- [x] After native managed mutation, an expected-head conflict transitions to
  `ExternalDirtyConflict`; allow inspect/discard/rebuild or explicit full scan,
  never replay silently against the new head.
- [x] Make explicit cleanup mandatory; `discard` removes the private workspace
  and spool, crash/reopen removes owned residue and selects Unknown/full scan,
  and `Drop` remains best effort only.
  One StoreId/root-identity-bound marker stays inside the private staging
  directory under a live native lock; child-exit recovery removes only an
  unlocked exact root/staging pair, while identity mismatch remains fatal.

### 5.4 Package-C exit

- [x] Cold, exact no-op, same-size patch, missing seed, replaced destination,
  and length-changing fallback tests pass.
- [x] Native N0–N5 fault/restart matrix passes.
- [x] Every renamed file is old/new; interrupted trees never gain Complete
  authority.
- [x] Managed capture rematerializes exact bytes after process reopen.
- [x] External capture rematerializes the exact tree and reports full-workspace
  scan class.
- [x] APFS clone and complete-stream routes produce identical logical output.
- [x] No private temp, descriptor, stale spool, or stale live authority remains.
- [x] OS/VFS format/check/non-native tests pass.
- [x] ManagedWorkspace exposes no path; materialize/convert to ExternalWorkspace
  before a native child can observe or mutate the tree.
- [x] A real `/bin/bash` child reads/executes ordinary workspace files and
  performs the frozen direct mutation script.
- [x] A tiny Apple helper performs writable mmap, flushes/unmaps/exits, and the
  subsequent cooperative capture records the exact mutation.
- [x] Capture while a controlled child/writer is live returns `WorkspaceBusy`;
  after wait/reap, full-scan capture and fresh rematerialization are exact.

## 6. Package D — minimal SDK and runnable PoC

- [x] Expose `LayerFs::open -> OpenedLayerFs { fs, head }`; fresh genesis is
  initialized exactly once and reopen returns the existing exact head.
- [x] Expose the two ownership-explicit materialization operations rather than
  an ambiguous third `materialize` alias.
- [x] Expose `materialize_managed`, `materialize_external`, ManagedWorkspace
  capture/into_external/discard, and ExternalWorkspace
  path/capture_quiescent/discard without engine/descriptor internals.
- [x] Add the smallest PoC-only managed edit API needed for exact-range proof;
  do not expose engine/SQLite/APFS internals.
- [x] Keep SDK errors small while preserving conflict, integrity, unsupported,
  incomplete, ambiguous, and resource distinctions.
- [x] Keep workflow/evaluator implementation out of the thin SDK; the single
  runnable PoC lives under `tools/layerfs-eval` and uses public SDK calls.
- [x] Extend `layerfs-eval` with one `apple-poc` command; do not create a new
  benchmark framework or semantic implementation.
- [x] Run the end-to-end completion workflow from section 1 through SDK/product
  APIs only.

## 7. Cross-operation correctness matrix

| Operation | Implemented | Model/unit | Durable/reopen | Native/SDK |
|---|:---:|:---:|:---:|:---:|
| Empty/tiny/full create | [x] | [x] | [x] | [x] |
| Point/range/full read | [x] | [x] | [x] | [x] |
| Same-size overwrite | [x] | [x] | [x] | [x] |
| Shorter/longer replace | [x] | [x] | [x] | [x] |
| Insert start/middle/EOF | [x] | [x] | [x] | [x] |
| Delete start/middle/all | [x] | [x] | [x] | [x] |
| Append/truncate | [x] | [x] | [x] | [x] |
| Namespace add/remove/rename | [x] | [x] | [x] | [x] |
| Namespace multi-level split/merge | [x] | [x] | [x] | [x] |
| Symbolic-link create/read/capture | [x] | [x] | [x] | [x] |
| Hard-link create/update/unlink | [x] | [x] | [x] | [x] |
| xattr/resource-fork/ACL/flags round trip | [x] | [x] | [x] | [x] |
| Real Bash read/execute/update | [x] | N/A | [x] | [x] |
| Cold/exact-live no-op materialize | [x] | [x] | [x] | [x] |
| Changed-root incremental materialize | N/A (explicit PoC v1 exclusion) | N/A | N/A | N/A |
| Managed capture | [x] | [x] | [x] | [x] |
| External full-workspace capture | [x] | [x] | [x] | [x] |
| Reopen/reconstruction | [x] | [x] | [x] | [x] |
| Long historical direct read | [x] | [x] | [x] | [x] |
| Checkpoint/fork/diverge | [x] | [x] | [x] | [x] |
| Rollback/stale conflict | [x] | [x] | [x] | [x] |
| Reachability report | [x] | [x] | [x] | N/A |
| Offline exclusive compaction | [x] | [x] | [x] | N/A |

## 8. Complexity and resource gate

- [x] Point read meets `O(log E + R)` structural counters.
- [x] Range read meets `O(log E + C_R + R)` structural counters.
- [x] Managed overwrite/insert/delete reads only boundary-path `O(H)` mapping
  nodes and creates at most `O(H) + replacement_tree_nodes` plus bounded
  split/merge allowance.
- [x] No unaffected suffix payload is read, rewritten, or rehashed by a managed
  rope splice.
- [x] Full import/read, full-workspace external capture, and materialization are
  labeled linear in their actual input/output population.
- [x] Path lookup counters meet `sum[O(log D_i)+O(log I)]`; a file-content edit
  changes zero directory nodes and one inode-table spine; namespace mutations
  change only direct parent tree(s) plus bounded inode paths.
- [x] Individual owned buffers are `<=1 MiB`.
- [x] Operation-owned Q has a 4 MiB structural reservation and a real
  current/high-water lifecycle gauge; focused SDK work observes the reservation
  and terminal zero. The configured SQLite page caches and caller-owned oracle
  buffers are reported separately.
- [x] The frozen three-run campaign reports local-operation RSS honestly:
  18,235,392–20,774,912 bytes with 20,480,000-byte median. This is diagnostic,
  not a production SLO; the structural Q reservation remains independent of
  `F`.
- [x] One state-changing capture records one transaction and one COMMIT.
- [x] File mapping is `<1%` only for files `>=1 MiB` in the frozen deterministic
  CDC population; report absolute overhead below 1 MiB.
- [x] Per-revision durable growth is unique payload plus local mapping/directory
  path work, never a full file/mapping copy.
- [x] Terminal temp files, pending work, connections, and file
  descriptors return to baseline/zero.

## 9. Small correctness-first benchmark

Run only after every preceding correctness gate is green.

- [x] Generate the deterministic roughly 3 MiB directory in the evaluator.
- [x] Run S0 fresh import/open.
- [x] Run S1 cold materialization and exact tree oracle.
- [x] Run S2 exact-live no-op verification against the same authority-bearing
  destination and prove canonical equality with zero native rewrite;
  mismatches fail closed.
- [x] Run S3 4 KiB managed same-size overwrite/capture.
- [x] Run S4 8 KiB managed middle insert/capture.
- [x] Run S5 4 KiB managed middle delete/capture.
- [x] Run S6 mixed add/remove/rename.
- [x] Run S7 real Bash read/execute against ordinary files.
- [x] Run S8 Bash redirect/dd/append/truncate/mkdir/mv/rm/chmod/symlink and
  hard-link/xattr operations and prove live-writer capture rejection.
- [x] Run S9 external full-workspace capture and semantic reuse checks.
- [x] Run S10 process reopen, fresh rematerialization, Bash assertions and
  historical 4 KiB range.
- [x] Run S11 fork divergent retained refs, rollback, and exact reads of every
  managed historical root.
- [x] Run S12 offline compaction, fresh Store reopen, and exact reads of every
  retained root.
- [x] Execute exactly one three-repetition campaign on frozen source.
- [x] Include preparation, exact post-check, and cleanup in the `<=30 s` gross
  diagnostic wall; an exceedance triggers owner diagnosis but does not relabel
  exact canonical checks as failed or create a product SLO.
- [x] Report median/range only; do not publish p95/SLO claims from three samples.
- [x] Retain one compact artifact directory: environment, test receipt, rows,
  summary, and failure stderr only when applicable.
- [x] Do not rerun unchanged source for favorable noise. Earlier runs were
  invalidated by source/receipt-schema corrections; only the final frozen
  exact-oracle run is evidence.

## 10. Final static and product closure

- [x] `cargo fmt --all -- --check` passes.
- [x] Touched-crate clippy with warnings denied passes.
- [x] `cargo test --workspace` passes.
- [x] `git diff --check` passes outside immutable historical evidence.
- [x] Product crates import no Phase-4 benchmark module or fixture path.
- [x] Core, engine, VFS and SDK contain no concrete Apple syscall, `libc`,
  native inode identity or platform-behavior `cfg`; repository search proves it.
- [x] Universal ProjectionDriver conformance passes with the in-memory/fault
  driver and the AppleDriver on APFS.
- [x] Only one fresh-profile writer, one reusable engine schema, and one native
  projector remain authoritative.
- [x] Active Phase-4 benchmark binaries are removed from the Cargo build after
  extraction/parity; historical evidence remains untouched.
- [x] Limitations name: external full scan, ordinary length-changing native full
  fallback, unsupported device/FIFO/socket kinds, APFS-profile qualification,
  rollback freshness, cooperative quiescence, and no online/in-place GC.
- [x] The evaluator and small campaign succeed after a clean process reopen.

## 11. PoC result disposition

Selected outcome: **PASS**.

Use exactly one of these final outcomes:

```text
PASS
  exact end-to-end workflow, structural/resource gates, restart, and small
  campaign all pass on the frozen Apple/APFS source.

REVISE
  a bounded product/code/test defect has a clear owner; preserve the failure,
  repair the shared path, and rerun only affected checks before one new final
  campaign if measured source changed.

NO-GO
  the selected operational-root identity, persistent rope, durable publication,
  or native workspace boundary cannot satisfy a hard correctness/resource
  invariant without changing the architecture decision.
```

Do not create a new version for syntax, formatting, test fixture, report, or
cleanup plumbing defects before measured rows. Preserve material failures, but
do not turn bookkeeping into the implementation program.
