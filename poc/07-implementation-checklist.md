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

- [ ] Record the fresh profile ID domain and confirm it cannot collide with v1,
  v2, or the provisional/unimplemented CD32–64 v3 proposal.
- [ ] Freeze extent leaf/internal tags, entry widths, `64..=128` occupancy,
  split/borrow/merge precedence, root-collapse rule, and maximum level.
- [ ] Accept the identity ADR: `FileStateRoot` is operational/history-shaped;
  optional `ContentDigest` owns semantic complete-byte equality.
- [ ] Remove mode from FileStateV3/DirectoryStateV1; freeze mode + mtime in one
  PortableMetadataV1 authority reached through InodeRecordV1.
- [ ] Freeze StoreId + transactional next-inode-serial allocation and preserve
  both exactly through abort, reopen and compaction.
- [ ] Confirm legacy v1/v2 readers are compatibility-only and no new PoC write
  emits their file mapping.
- [ ] Freeze the persistent namespace node byte limit/minimum fill, name limit,
  entry kinds, split/borrow/merge precedence, root rules and profile ID.
- [ ] Freeze canonical names as exact 1..=255-byte UTF-8 excluding `.`, `..`,
  NUL, slash and backslash; native representability/collisions remain driver
  admission, not canonical normalization.
- [ ] Confirm the old complete-`BTreeMap` namespace is legacy/model input only;
  every new product mutation path-copies persistent namespace nodes.
- [ ] Set SQLite `busy_timeout=0`; assert exact immediate Busy/Locked and no
  SQLite/internal/application retry. Keep this runtime policy out of canonical
  profile identity.
- [ ] Confirm the PoC uses synchronous materialization. Do not extract the G5
  exact/latest mailbox unless an actual asynchronous caller appears.
- [ ] Confirm arbitrary external capture walks the complete supported namespace
  and scans every regular file because neither complete paths nor ranges are
  authoritative.
- [ ] Confirm hard links and frozen Apple extension metadata are required;
  device/FIFO/socket kinds, online/in-place GC, transparent write interception,
  legacy migration, and production portability remain typed/deferred rather
  than silently flattened.

## 3. Package A — canonical persistent file state

### 3.1 Files

- [ ] Add `crates/layerfs-core/src/content/extent.rs`.
- [ ] Add `crates/layerfs-core/src/content/extent_codec.rs`.
- [ ] Add `crates/layerfs-core/src/content/rope.rs`.
- [ ] Add `crates/layerfs-core/tests/extent_codec.rs`.
- [ ] Add `crates/layerfs-core/tests/extent_model.rs`.
- [ ] Add `crates/layerfs-core/src/namespace.rs`.
- [ ] Add `crates/layerfs-core/src/namespace_codec.rs`.
- [ ] Add `crates/layerfs-core/src/inode.rs` and `metadata.rs`.
- [ ] Add `crates/layerfs-core/tests/namespace_model.rs`.
- [ ] Update `content/mod.rs` to expose a small logical file facade without
  loading all extents into a `Vec` for durable operations.
- [ ] Update `limits.rs`, `error.rs`, `object`, `cow`, and `delta` only where the
  selected profile requires it.

### 3.2 Codec and validation

- [ ] Encode/decode the exact fresh-profile common header.
- [ ] Encode/decode 40-byte `ExtentSliceV3` entries.
- [ ] Encode/decode 48-byte cumulative child descriptors.
- [ ] Encode/decode canonical mode-free `FileStateV3` with profile ID, logical
  length, extent count, tree level, and mapping-root ID; define
  `FileStateRoot` as this record's ObjectId.
- [ ] Authenticate every fetched node under the requested role and profile.
- [ ] Reject wrong magic/version/tag/level/flags/count/trailing bytes.
- [ ] Reject zero-length nonempty extents and checked source-range overflow.
- [ ] Decode the referenced payload and prove every slice lies within it.
- [ ] Reject noncanonical adjacent coalescible slices.
- [ ] Validate leaf occupancy, branch occupancy, cumulative totals, child level,
  maximum height, empty-root form, root expansion, and root collapse.
- [ ] Freeze independent canonical bytes/ObjectIds for empty, one-entry, min,
  max, leaf split, branch split, and multi-level roots.

### 3.3 Algorithms

- [ ] Implement full streamed construction.
- [ ] Implement byte-offset locate with bounded-node binary search.
- [ ] Implement point/range/full streaming reads.
- [ ] Implement persistent split at start/middle/extent boundary/EOF.
- [ ] Implement deterministic join with borrow/merge/root-collapse rules.
- [ ] Implement overwrite as split + replacement build + join.
- [ ] Implement insert without touching an unaffected suffix subtree.
- [ ] Implement delete without copying deleted or suffix payload bytes.
- [ ] Implement append and truncate through the same shared operations.
- [ ] Preserve old roots after every path-copy operation.
- [ ] Emit structural counters from the product algorithms, not literal test
  constants.

### 3.4 Package-A exit

- [ ] Deterministic `Vec<u8>` differential model passes every operation class.
- [ ] Deterministic randomized sequences pass after every intermediate edit.
- [ ] All retained roots reconstruct exactly.
- [ ] Corruption/wrong-role/missing-node/overflow tests fail with exact errors.
- [ ] Managed local operations satisfy `O(B + log E)` structural work.
- [ ] No local operation owns memory proportional to `F` or `E`.
- [ ] `cargo fmt`, core check, and core tests pass.

### 3.5 Persistent namespace

- [ ] Encode/decode canonical directory state, byte-bounded leaf/branch nodes,
  name-to-InodeId entries, inode-table nodes/records, regular/directory/symlink
  targets, exact symlink targets and typed extension metadata.
- [ ] Reject invalid names, duplicate keys, wrong kinds, noncanonical fill,
  trailing bytes, overflow, wrong levels and redundant roots.
- [ ] Implement component/path lookup, create, remove, replace, same-directory
  rename and cross-directory rename through persistent path-copy.
- [ ] Implement hard link/unlink as shared InodeId plus checked
  `namespace_ref_count`
  updates; preserve one content mutation across every linked path.
- [ ] Preserve unchanged directory subtrees and all old namespace roots.
- [ ] Differential-test against a nested ordered map.
- [ ] Force multi-level split, borrow, merge and root collapse with at least
  10,000 deterministic synthetic names without adding large file payloads.
- [ ] Assert namespace nodes read/created are proportional to changed spines,
  never complete directory entry count.

## 4. Package B — one reusable durable engine

### 4.1 Reconcile, do not copy

- [ ] Start from `crates/layerfs-engine/src/lib.rs` and its `layerfs_*` schema.
- [ ] Inventory each required G5 behavior from `wp4m_*` benchmark code against a
  source test or accepted frozen vector.
- [ ] Add only the minimum missing reusable behavior under `store.rs`,
  `integrity.rs`, `refs.rs`, and `publication.rs`.
- [ ] Do not import a `src/bin` module from product code.
- [ ] Do not create a second SQLite Store/schema/projector implementation.
- [ ] Keep payload/node canonical identities independent of SQLite row IDs.
- [ ] Remove benchmark-bin product ownership only after new library tests prove
  parity and repository search shows no active caller.

### 4.2 Store and publication

- [ ] Configure and assert `DELETE`, `FULL`, `FILE`, and `mmap_size=0`.
- [ ] Freeze one writer and at most two query-only readers, no pool,
  `cache_size=1280`, observed 4 KiB page size, and report configured cache
  budget separately from actual RSS/Q.
- [ ] Implement authenticated put-if-absent and unequal-incumbent rejection.
- [ ] Implement authenticated complete and exact-range object reads.
- [ ] Replace current redundant object loads with one SELECT, one borrowed-row
  authentication, one strict decode and no separate length query.
- [ ] Implement ordered payload batches of at most 64 references, preserving
  duplicate occurrence order and exact missing/wrong-role errors.
- [ ] Prepare only bounded operation descriptors/evidence before `BEGIN`; do
  not accumulate a large candidate object set in memory or a durable carrier.
- [ ] After `BEGIN` and expected-state validation, stream/authenticate/insert
  every SQLite object/root/delta/ref row inside that one writer transaction.
- [ ] Check expected ref/root plus generation before publication.
- [ ] Dispatch exactly one publication COMMIT for a state change.
- [ ] Dispatch zero publication COMMITs for a normalized no-op.
- [ ] Reconcile ambiguous COMMIT outcome from a fresh connection as requested,
  prior, different, or indeterminate; never blind-redispatch.
- [ ] Preserve exact Busy, Locked, no-space, corruption, permission, constraint,
  and I/O error classes.

### 4.3 Integrity and history

- [ ] Keep `Verified` as default.
- [ ] Make `TrustedLocalDev` explicit and Store-lifetime.
- [ ] Keep fetched/new/incumbent identity checks common and unconditional.
- [ ] Prevent trusted assumptions from becoming verified receipt authority.
- [ ] Perform the required Verified-after-Trusted reopen verification.
- [ ] Implement named/internal refs with root + generation expected-state update.
- [ ] Implement checkpoint/fork as a new ref to an immutable retained root.
- [ ] Implement rollback as an expected-state ref move, not object mutation.
- [ ] Implement direct historical root read without replaying later history.
- [ ] Implement read-only reachability enumeration from every retained ref/pin.
- [ ] Add `compaction.rs` with an exclusive-maintenance admission check.
- [ ] Mark the authenticated union of every retained ref/checkpoint and reject
  compaction while any reader/writer/workspace/recovery pin exists.
- [ ] Stream the retained union to one same-directory sibling Store, preserving
  exact canonical bytes, schema/profile and ref generations.
- [ ] Preflight available space for the new sibling generation allocated upper
  bound + disk-backed mark database + candidate SQLite rollback-journal/temp
  high-water bound + `CURRENT.tmp` and safety margin; fail before copy when it
  cannot fit.
- [ ] Report total peak as retained old generation + new generation + mark
  database + candidate journal/temp + selector temporary bytes.
- [ ] Install through checksummed `CURRENT` and StoreGenerationDriver; recovery
  never guesses highest generation filename.
- [ ] Verify every retained root before durably swapping Store generation.
- [ ] Freshly reopen the installed Store before removing the old backup.
- [ ] Recover exactly across sibling-COMMIT/sync/swap/reopen/cleanup faults;
  never remove the only verified Store generation.

### 4.4 Package-B exit

- [ ] Object/store parity tests pass.
- [ ] Stale head/ref conflicts before visible publication.
- [ ] T0–T6 publication fault/restart matrix passes.
- [ ] One writer/multiple pinned readers return exact roots without switching.
- [ ] Fork divergence and rollback preserve every retained root.
- [ ] A 1,000-tiny-revision correctness sequence remains directly readable.
- [ ] Offline compaction removes only authenticated-unreachable objects and all
  retained/forked/rollback roots remain exact after process reopen.
- [ ] Owned writer transaction, connection, descriptor, and Q state returns to
  terminal baseline/zero.
- [ ] Engine format/check/tests pass.

## 5. Package C — universal VFS plus Apple/APFS driver

### 5.1 Native OS boundary

- [ ] Add the OS-neutral `layerfs-vfs/src/driver.rs` port and an in-memory/fault
  conformance driver; keep VFS free of concrete syscalls and platform cfg.
- [ ] Add `layerfs-os/src/apple/{mod,workspace,apfs,metadata,ffi}.rs`; move
  Apple-only `libc` ownership from the benchmark crate if still required.
- [ ] Replace the OS crate's non-overridable `forbid(unsafe_code)` with
  deny-by-default and allow unsafe only in one reviewed `apple::ffi` submodule;
  expose safe wrappers for every required syscall and test exact errno/partial
  I/O behavior.
- [ ] Implement no-follow destination/directory admission.
- [ ] Preflight every canonical sibling set in private same-volume staging and
  reject unrepresentable/case-/normalization-colliding APFS names with typed
  errors before visible mutation or Complete live authority.
- [ ] Record pinned directory and file identity needed to detect substitution.
- [ ] Create private same-directory temporary files with unique ownership.
- [ ] Use one mode-0700 same-volume staging directory; do not extract the G5
  ownership-xattr helper, and assert no private LayerFS xattr reaches output.
- [ ] Implement bounded full-stream file construction.
- [ ] Implement optional APFS clone attempt.
- [ ] Implement same-size `pwrite` patch ranges only after seed authority.
- [ ] Implement file sync, atomic one-file rename, and directory sync.
- [ ] Reconcile clone metadata as an exact set: remove seed-only xattrs, replace
  ACL/mode/xattrs, apply restrictive flags last, read back and verify.
- [ ] Use one final file sync after content and exact metadata, then rename and
  parent-directory sync; record achieved durability class.
- [ ] Implement owned-temp cleanup that never unlinks a substitute.
- [ ] Return typed unsupported/fallback/error outcomes; clone failure is not a
  correctness failure when full stream is available.

### 5.2 Materialization

- [ ] Add the VFS workspace (including live authority/provenance/spool),
  materialize, and capture modules named in the architecture document.
- [ ] Implement cold/full tree materialization in canonical directory order.
- [ ] Implement exact-root no-op with zero content rewrites only under
  uninterrupted exclusive live-managed Store/workspace/generation/mutation
  authority; inode/size/mtime or an old record never suffices.
- [ ] Implement verified parent same-size clone/patch.
- [ ] Route missing/replaced/unverified seed to complete stream.
- [ ] Route length-changing ordinary native projection to complete stream.
- [ ] After every file rename, record per-file progress without claiming a
  complete tree.
- [ ] Install process-lifetime `Complete` live authority only after the entire
  native tree is freshly exact and required directory sync has completed; do
  not persist projection intent/receipt in PoC v1.
- [ ] On interruption, classify a mixed directory as Incomplete derived state;
  rebuild or resume only from exact intent/progress authority.

### 5.3 Capture

- [ ] Implement bounded managed change descriptors bound to Store/workspace/
  generation/base root.
- [ ] Define managed coordinates relative to the current pending state; preserve
  exact call order and never sort count-changing operations across calls.
- [ ] Mutate the private native workspace and spool exact replacement bytes in
  a LayerFS-owned process/workspace-lifetime file with digest/offset binding.
- [ ] Cap descriptors at 64; require capture/discard at the bound and keep spool
  bytes out of RAM/Q except for bounded streaming windows.
- [ ] Freeze and revalidate managed edit evidence before capture.
- [ ] Replay descriptors in call order against the base root, stream managed
  bytes from the spool, compare the result with native state, and use the one
  engine publication path.
- [ ] For arbitrary external capture, walk the complete supported native
  namespace; do not use advisory event candidates as completeness authority.
- [ ] Full-scan every supported regular file with bounded buffers, detecting
  additions, removals, renames, kind changes, metadata changes, and content.
- [ ] Capture symbolic links with `lstat`/`readlink` and never follow them.
- [ ] Group native hard links, preserve/reuse one canonical `InodeId`, and
  verify stable native link count equals aliases inside workspace; use a
  disk-backed scratch table and return `ExternalHardLinkBoundary` for external
  aliases; reject device/FIFO/socket kinds with typed errors.
- [ ] Capture/materialize xattrs, resource forks, supported ACLs and BSD flags
  through typed canonical Apple extension metadata.
- [ ] Reject ambiguous/replaced/symlink native identity according to the frozen
  policy.
- [ ] Make capture/discard mutually exclusive terminal successes.
- [ ] After native managed mutation, an expected-head conflict transitions to
  `ExternalDirtyConflict`; allow inspect/discard/rebuild or explicit full scan,
  never replay silently against the new head.
- [ ] Make explicit cleanup mandatory; `discard` removes the private workspace
  and spool, crash/reopen removes owned residue and selects Unknown/full scan,
  and `Drop` remains best effort only.

### 5.4 Package-C exit

- [ ] Cold, exact no-op, same-size patch, missing seed, replaced destination,
  and length-changing fallback tests pass.
- [ ] Native N0–N5 fault/restart matrix passes.
- [ ] Every renamed file is old/new; interrupted trees never gain Complete
  authority.
- [ ] Managed capture rematerializes exact bytes after process reopen.
- [ ] External capture rematerializes the exact tree and reports full-workspace
  scan class.
- [ ] APFS clone and complete-stream routes produce identical logical output.
- [ ] No private temp, descriptor, stale spool, or stale live authority remains.
- [ ] OS/VFS format/check/tests pass.
- [ ] ManagedWorkspace exposes no path; materialize/convert to ExternalWorkspace
  before a native child can observe or mutate the tree.
- [ ] A real `/bin/bash` child reads/executes ordinary workspace files and
  performs the frozen direct mutation script.
- [ ] A tiny Apple helper performs writable mmap, flushes/unmaps/exits, and the
  subsequent cooperative capture records the exact mutation.
- [ ] Capture while a controlled child/writer is live returns `WorkspaceBusy`;
  after wait/reap, full-scan capture and fresh rematerialization are exact.

## 6. Package D — minimal SDK and runnable PoC

- [ ] Expose `LayerFs::open -> OpenedLayerFs { fs, head }`; fresh genesis is
  initialized exactly once and reopen returns the existing exact head.
- [ ] Expose `LayerFs::materialize`.
- [ ] Expose `materialize_managed`, `materialize_external`, ManagedWorkspace
  capture/into_external/discard, and ExternalWorkspace
  path/capture_quiescent/discard without engine/descriptor internals.
- [ ] Add the smallest PoC-only managed edit API needed for exact-range proof;
  do not expose engine/SQLite/APFS internals.
- [ ] Keep SDK errors small while preserving conflict, integrity, unsupported,
  incomplete, ambiguous, and resource distinctions.
- [ ] Add `layerfs-sdk/examples/apple_poc.rs` with no test hooks.
- [ ] Extend `layerfs-eval` with one `apple-poc` command; do not create a new
  benchmark framework or semantic implementation.
- [ ] Run the end-to-end completion workflow from section 1 through SDK/product
  APIs only.

## 7. Cross-operation correctness matrix

| Operation | Implemented | Model/unit | Durable/reopen | Native/SDK |
|---|:---:|:---:|:---:|:---:|
| Empty/tiny/full create | [ ] | [ ] | [ ] | [ ] |
| Point/range/full read | [ ] | [ ] | [ ] | [ ] |
| Same-size overwrite | [ ] | [ ] | [ ] | [ ] |
| Shorter/longer replace | [ ] | [ ] | [ ] | [ ] |
| Insert start/middle/EOF | [ ] | [ ] | [ ] | [ ] |
| Delete start/middle/all | [ ] | [ ] | [ ] | [ ] |
| Append/truncate | [ ] | [ ] | [ ] | [ ] |
| Namespace add/remove/rename | [ ] | [ ] | [ ] | [ ] |
| Namespace multi-level split/merge | [ ] | [ ] | [ ] | [ ] |
| Symbolic-link create/read/capture | [ ] | [ ] | [ ] | [ ] |
| Hard-link create/update/unlink | [ ] | [ ] | [ ] | [ ] |
| xattr/resource-fork/ACL/flags round trip | [ ] | [ ] | [ ] | [ ] |
| Real Bash read/execute/update | [ ] | N/A | [ ] | [ ] |
| Cold/warm/incremental materialize | [ ] | [ ] | [ ] | [ ] |
| Managed capture | [ ] | [ ] | [ ] | [ ] |
| External full-workspace capture | [ ] | [ ] | [ ] | [ ] |
| Reopen/reconstruction | [ ] | [ ] | [ ] | [ ] |
| Long historical direct read | [ ] | [ ] | [ ] | [ ] |
| Checkpoint/fork/diverge | [ ] | [ ] | [ ] | [ ] |
| Rollback/stale conflict | [ ] | [ ] | [ ] | [ ] |
| Reachability report | [ ] | [ ] | [ ] | N/A |
| Offline exclusive compaction | [ ] | [ ] | [ ] | N/A |

## 8. Complexity and resource gate

- [ ] Point read meets `O(log E + R)` structural counters.
- [ ] Range read meets `O(log E + C_R + R)` structural counters.
- [ ] Managed overwrite/insert/delete reads only boundary-path `O(H)` mapping
  nodes and creates at most `O(H) + replacement_tree_nodes` plus bounded
  split/merge allowance.
- [ ] No unaffected suffix payload is read, rewritten, or rehashed by a managed
  rope splice.
- [ ] Full import/read, full-workspace external capture, and materialization are
  labeled linear in their actual input/output population.
- [ ] Path lookup counters meet `sum[O(log D_i)+O(log I)]`; a file-content edit
  changes zero directory nodes and one inode-table spine; namespace mutations
  change only direct parent tree(s) plus bounded inode paths.
- [ ] Individual owned buffers are `<=1 MiB`.
- [ ] Operation-owned Q is `<=8 MiB` and terminal exactly zero.
- [ ] Local-operation RSS does not grow with `F`; `<=32 MiB` is a prospective
  small-PoC diagnostic, not inherited evidence.
- [ ] One state-changing capture records one transaction and one COMMIT.
- [ ] File mapping is `<1%` only for files `>=1 MiB` in the frozen deterministic
  CDC population; report absolute overhead below 1 MiB.
- [ ] Per-revision durable growth is unique payload plus local mapping/directory
  path work, never a full file/mapping copy.
- [ ] Terminal temp files, pending work, owned buffers, connections, and file
  descriptors return to baseline/zero.

## 9. Small correctness-first benchmark

Run only after every preceding correctness gate is green.

- [ ] Generate the deterministic roughly 3 MiB directory in the evaluator.
- [ ] Run S0 fresh import/open.
- [ ] Run S1 cold materialization and exact tree oracle.
- [ ] Run S2 exact warm materialization with zero content rewrites.
- [ ] Run S3 4 KiB managed same-size overwrite/capture.
- [ ] Run S4 8 KiB managed middle insert/capture.
- [ ] Run S5 4 KiB managed middle delete/capture.
- [ ] Run S6 mixed add/remove/rename.
- [ ] Run S7 real Bash read/execute against ordinary files.
- [ ] Run S8 Bash redirect/dd/append/truncate/mkdir/mv/rm/chmod/symlink and
  hard-link/xattr operations and prove live-writer capture rejection.
- [ ] Run S9 external full-workspace capture and semantic reuse checks.
- [ ] Run S10 process reopen, fresh rematerialization, Bash assertions and
  historical 4 KiB range.
- [ ] Run S11 fork, divergent edits, rollback, and exact retained-root reads.
- [ ] Run S12 offline compaction and exact retained-root reopen.
- [ ] Execute exactly one three-repetition campaign on frozen source.
- [ ] Include preparation, exact post-check, and cleanup in the `<=30 s` gross
  diagnostic wall; an exceedance triggers owner diagnosis but does not relabel
  exact canonical checks as failed or create a product SLO.
- [ ] Report median/range only; do not publish p95/SLO claims from three samples.
- [ ] Retain one compact artifact directory: environment, test receipt, rows,
  summary, and failure stderr only when applicable.
- [ ] Do not rerun unchanged source for favorable noise.

## 10. Final static and product closure

- [ ] `cargo fmt --all -- --check` passes.
- [ ] Touched-crate clippy with warnings denied passes.
- [ ] `cargo test --workspace` passes.
- [ ] `git diff --check` passes outside immutable historical evidence.
- [ ] Product crates import no Phase-4 benchmark module or fixture path.
- [ ] Core, engine, VFS and SDK contain no concrete Apple syscall, `libc`,
  native inode identity or platform-behavior `cfg`; repository search proves it.
- [ ] Universal ProjectionDriver conformance passes with the in-memory/fault
  driver and the AppleDriver on APFS.
- [ ] Only one fresh-profile writer, one reusable engine schema, and one native
  projector remain authoritative.
- [ ] Active Phase-4 benchmark binaries are removed from the Cargo build after
  extraction/parity; historical evidence remains untouched.
- [ ] Limitations name: external full scan, ordinary length-changing native full
  fallback, unsupported device/FIFO/socket kinds, APFS-profile qualification,
  rollback freshness, cooperative quiescence, and no online/in-place GC.
- [ ] The example and small campaign succeed after a clean process reopen.

## 11. PoC result disposition

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
