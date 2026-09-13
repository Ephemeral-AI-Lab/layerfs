# Snapshot-Isolated Workspace: Phase 1 contract resolution

Status: Dated planning checkpoint; not release evidence or a product contract.
Phase 1 output for [#124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124),
2026-09-15. V2, V3 and V4 are resolved to implementable contracts against the
current source. **V1 is resolved to a precise platform incompatibility that
requires an owner decision; it is not implemented, not waived, and not silently
worked around.** No product behavior in this document is claimed as implemented or
measured.

Governing documents: [rule](overlay-snapshot-rule.md),
[specification](overlay-snapshot-spec.md),
[adversarial review](overlay-snapshot-spec-review.md),
[architecture](overlay-snapshot-architecture-design.md),
[implementation plan](overlay-snapshot-implementation-plan.md),
[#122 exclusion manifest](benchmark-exclusions-issue122.json).

Frozen source for this resolution: commit `86f6a0a0ff3de2d524b4984ab9c6b5de5272b18b`
(rule/spec/review/architecture/plan/handoff as published) plus the code baseline
`0814cc37f1dafb6041930c74489107f4a5035a26` recorded by the specification; the
inspected live source is that baseline plus the documentation commit.

## 1. V1 — kernel visibility: demonstrated incompatibility (blocking)

### What was tested

A dependency-free probe (`benchmark-results/v016/v1-probe/probe.c`, raw log
`run.log`, reproduction and full result table in that directory's `README.md`)
serves a cached, write-through FUSE session (`init` honours
`FUSE_ASYNC_READ | FUSE_BIG_WRITES`, i.e. `flags=0x21`, and deliberately not
`FUSE_WRITEBACK_CACHE`/`FUSE_DIRECT_IO_ALLOW_MMAP` — the same selection as
`crates/layerfs-fuse/src/filesystem.rs:37-47`), mounts it with `mount(2)` under
`CAP_SYS_ADMIN`, maps a served file `MAP_SHARED` with `PROT_WRITE`, stores bytes,
and reports exactly what the daemon observed. Container kernel
`6.12.76-linuxkit`, image `rust:1.85.1-bookworm`.

### Result

| Observation | Raw evidence |
| --- | --- |
| `mmap(PROT_WRITE, MAP_SHARED)` is accepted on a cached handle, and a store into it is **not** delivered to the daemon (daemon view unchanged, no WRITE request) | `daemon_write_requests=0 daemon_saw_first_byte=0`, waited 1000 ms |
| A later store is again invisible | `daemon_saw_second_byte=0`, waited 500 ms |
| `syncfs()` on the mount root delivers the dirty page | `daemon_first_byte=65` within 10 ms |
| A per-file flush works without any mount-wide barrier: the daemon opened the file through its own mount and called `fsync` | pre-`fsync` `daemon_saw_third_byte=0`; post-`fsync` `daemon_third_byte=67` within 10 ms |
| That per-file flush cannot address an open-unlinked inode: the self-open has no path | `self_open_errno=2` (`ENOENT`) |
| `syncfs` does cover the open-unlinked inode | `daemon_fourth_byte=68` within 10 ms |

### Why this follows from the kernel contract

- `mmap()` of a cached FUSE file sends no request to the daemon
  (`fs/fuse/file.c` `fuse_file_mmap`): the daemon sees the `open` that preceded the
  mapping but never learns that a mapping exists, let alone which pages are dirty.
- In cached write-through mode only `write(2)` is delivered synchronously
  ([FUSE I/O modes](https://docs.kernel.org/filesystems/fuse/fuse-io.html));
  mapped stores are written back later by the VM.
- The kernel exposes no facility to read current page-cache bytes. The only
  triggers are per-file `fsync`/`msync`/`munmap`, filesystem-wide `syncfs`, and
  memory pressure.
- `FUSE_DIRECT_IO_ALLOW_MMAP` does not remove this: the first shared mapping of a
  direct-I/O file enters caching inode I/O mode (`fuse_file_cached_io_open`), after
  which mapped stores are cached and written back lazily.

### Mechanism inventory and why each is not compliant as specified

| # | Mechanism | Preserves mapping semantics | Non-freezing | Bounded acquisition | Verdict |
| --- | --- | --- | --- | --- | --- |
| M1 | `syncfs` on the mount at acquisition | yes | yes (no operation exclusion) | **no** — work is proportional to dirty payload; a store concurrent with the flush can land on either side, and a store that completes before the flush returns can still miss its collection point, so the boundary is not a single observable cut | forbidden by rule §6 ("drain the entire cache", "bounded work independent of … payload bytes") |
| M2 | per-file flush of files that can hold kernel-dirty bytes (daemon self-open + `fsync`), optionally combined with M1 for open-unlinked inodes | yes | yes | **no** — still payload-proportional; the daemon cannot see mappings, so the candidate set is "every cached writable open"; same collection-point boundary caveat as M1; no path exists for open-unlinked inodes | same rule tension as M1 |
| M3 | Refuse/removal of writable shared mappings (`FOPEN_DIRECT_IO` handles only) | **no** | yes | yes | forbidden: "do not silently disable … mmap"; the kernel currently accepts the mapping and `crates/layerfs-sdk/tests/live_fuse.rs:180` asserts the supported surface |
| M4 | Snapshot host-installed state only; treat kernel-dirty mapped bytes as outside the snapshot | **no** (bytes a concurrent reader can see are omitted) | yes | yes | forbidden: "Capturing consistent daemon metadata while omitting bytes promised visible by the filesystem contract is incorrect" |
| M5 | Real-file backing with kernel/file-system snapshot support (`FUSE_PASSTHROUGH`, reflink/APFS clone, hole-based COW) | partial | yes | mixed | changes the canonical storage model (one backing file per inode, unbounded descriptors), still cannot see un-flushed dirty pages of the backing file, and needs kernel ≥ 6.9 plus `CAP_SYS_ADMIN` |
| M6 | Read the dirty pages out of the writer's address space (`process_vm_readv` after enumerating `/proc/<pid>/maps`) | yes | yes | **no** | rejected: requires enumerating every process and mapping in the client's namespace, cannot cover other containers, and produces torn reads that are not a consistent cut |

### The decision required

The rule and the specification jointly require (a) exact snapshot visibility for
writable mappings, (b) no global freeze, (c) no whole-cache drainage, (d) bounded
acquisition work independent of payload bytes, and (e) preservation of the existing
supported mapping surface. The probe shows the platform offers no mechanism
satisfying all five at once. One of the following must therefore be an explicit
owner decision before the full-surface implementation is complete:

1. **Permit a bounded acquisition-time writeback barrier** (M2, plus M1 for
   open-unlinked inodes): the acquisition boundary becomes "after the daemon has
   forced writeback of every file that may hold kernel-dirty bytes"; the rule's
   "bounded acquisition" clause is narrowed to "bounded by kernel-dirty state, not
   by total files/changed files/pieces", and the per-file/completeness limits
   above become declared contract text. Ordinary filesystem operations continue.
   This option is weaker than the current behavior in one respect that must be
   declared rather than assumed: FUSE offers no way to fence a mapping's stores, so
   a store that *completes* before the flush returns can still miss the kernel's
   internal collection point, while a store issued during the flush can be included.
   The exact boundary would therefore be "the kernel's collection point inside the
   flush", not the flush's return, and the contract must say so.
2. **Narrow the supported surface** for writable shared mappings on Workspace
   files (M3): the product would refuse `MAP_SHARED` + `PROT_WRITE` on cached
   handles, and the existing tests that assert that surface
   (`crates/layerfs-sdk/tests/live_docker.rs::live_cut_check`,
   `crates/layerfs-sdk/tests/live_fuse.rs:180`) would have to be re-versioned as a
   declared semantic change.
3. **Keep the current behavior** (freeze + `syncfs` before capture), i.e. treat
   non-pausing Commit as not achievable for the full supported surface and stop
   #124 rather than ship a weaker visibility model.

Until one is chosen, no code may claim full public snapshot support: the plan's
Phase 1 exit condition ("no code claiming full public snapshot support proceeds on
a known weaker visibility model") is satisfied by recording this blocker, and the
implementation issue stays open. The other Phase 2–5 obligations below are
independent of this decision and are specified now so that implementation can be
scheduled either way.

One further consequence of removing the current mechanism must be declared
whichever option is chosen: the `syncfs` in `flush_kernel_cache` is not only a
visibility trigger, it is also where the product observes kernel-side writeback
errors ("syncfs observes the mount's writeback error sequence, including kernel-side
allocation errors", `crates/layerfs-fuse/src/live_owner.rs:2619-2621`). A Commit that
no longer performs it stops surfacing those errors at that boundary, so the
replacement contract must say where writeback errors are observed and delivered to
the caller instead of silently dropping them.

### The second half of V1 is separable and is not blocked

"Acknowledged data previously buffered only in the container" is a product change,
not a kernel boundary: before an ordinary mutation is acknowledged, the host must
own the exact replacement bytes and the installed metadata that references them.
The current code publishes container facts to the host only inside the frozen
capture (`crate::projection::capture` →
`crates/layerfs-workspace/src/live_backing.rs`), so this remains to be implemented
per §2. It does not depend on the mapping decision above.

## 2. V2 — host overlay ownership, atomic installation, bounded reclamation

Selected contract (smallest implementation that satisfies rule §5/§6/§9 and the
specification's authority decision; reuses existing value types so no logical
semantics are rewritten):

1. **Live root bundle.** `Arc<OverlayRoot>` is the single installed authority for
   host-owned inode, binding, change-index and provenance state. It is immutable
   once installed. `OverlayRoot` carries the installation `sequence`, the inode and
   binding indexes, `change_by_key`/`change_by_seq`, stable-inode and lineage
   high-water marks, and the immutable base/backing context. A mutation installs a
   whole new root; it never mutates an installed root in place.
2. **Indexes.** Both indexes are structurally shared persistent maps keyed by typed
   identity (`NodeId` for inodes, `(parent, name)` for bindings, `ChangeKey` for
   changes), not by path strings. Values keep the existing `Node`, `FileData`,
   `PieceTree`, `DirectoryData` types from `layerfs-workspace-core`, so the
   `live/` semantics, resource checks and piece/compact encodings are unchanged. A
   mutation copies only the index paths it touches; the working set remains bounded
   by the admitted node/byte policy, and index residency is charged.
3. **Atomic installation with a leased source root.** Preparation leases the
   current root (`Arc` clone), resolves the affected records and builds privately
   readable replacements; reservation charges worst-case payload, index nodes and
   ownership records before any state change; installation compares the complete
   leased root identity (pointer/sequence equality, not per-inode revisions) and
   swaps the slot only if it is still the leased source, then releases the previous
   root outside the installation lock. A conflict re-prepares against a freshly
   leased root outside that lock, is bounded by an explicit attempt budget with
   fair rescheduling, and every attempt/conflict/abandoned allocation is counted
   (`r` in the complexity contract is never assumed to be 1).
4. **Change tracking replaces the current maps.** `LiveWorkspace.dirty`
   (`BTreeSet<NodeId>`) and `LiveWorkspace.mutation_paths` (`BTreeMap<String, u64>`)
   stop being the Commit input. In their place: one current `ChangeEntry {sequence,
   effect}` per affected key, plus the secondary `(sequence, key)` index, updated in
   the same installation and with superseded secondary entries removed atomically.
   Enumeration is a range scan of the captured `(b0, b1]` interval on the captured
   secondary index. Cleanup of covered entries is conditional on the key's current
   sequence and bounded. `mutation_paths` consumers must be traced before removal;
   today they are `session.rs`, `registry.rs`, `reconcile.rs`, `projection.rs` and
   reporting/diagnostics.
5. **Snapshot acquisition.** `Arc::clone` of the installed root plus retention
   registration; no map clone, no fact export, no enumeration, no payload copy. The
   existing one-word `BackingRef` (`layerfs-workspace-core/src/backing.rs`) already
   gives range ownership that survives live-tree drops, which is the ownership
   primitive the snapshot uses for payload. Acquisition latency and work are
   measured as part of the Commit API (`snapshot_acquire_ns`), never reported as
   the root pin alone.
6. **Payload backing and physical release.** Keep create-then-unlinked backing files
   (their descriptors stay open) but replace one file per segment with a bounded set
   of arena files addressed by (arena, offset, len); segments are ranges, not
   descriptors. Physical block release is declared per host backend and probed:
   Linux `fallocate(FALLOC_FL_PUNCH_HOLE|FALLOC_FL_KEEP_SIZE)` and macOS APFS
   `fcntl(F_PUNCHHOLE)`. If a backend cannot release blocks, the bytes stay charged
   and the storage-efficiency claim is withheld rather than reported as reclaimed.
7. **Reclamation.** Current-state, snapshot, reader and in-flight owners each retain
   backing independently; release is deferred, charged and bounded; no synchronous
   graph-wide destruction on the foreground path; reclamation backlog has a declared
   bound and a measured drain. Reclaiming a mostly-dead source retains only the
   intersecting 4-KiB payload block plus metadata and reader leases, not the whole
   logical source.
8. **Replay and acknowledgment.** Host-readable bytes before ordinary acknowledgment
   (V1 second half), plus a bounded active-session replay window keyed by
   `(transport_session, request_sequence)` with retained unacknowledged results and a
   cumulative client watermark; below-watermark requests are rejected as stale,
   beyond-window requests are refused, and reconnect resolves retained outcomes or
   returns explicit uncertainty. Retention is bounded by count and bytes.
9. **Budgets.** Extend `ResourcePolicy` (`layerfs-workspace-core/src/limits.rs`,
   today `max_spool_bytes` and `max_final_delta_memory_bytes`) with explicit caps for
   index/page cache, prepared transactions, active read leases, transport queue and
   replay results, arenas/descriptors, and reclamation records, with a documented
   aggregate policy across Workspaces. No cap silently removes canonical validation
   or an existing public input limit.
10. **Bounded substitution, not bounded duplication.** After a successful publication,
    live ranges that still reference temporary bytes which are now canonical must be
    interchangeable with canonical backing, otherwise live state pins the temporary
    spool forever and repeated Commits accumulate redundant temporary histories — the
    failure rule §5 forbids. Selected contract: a per-range (not per-inode, not
    per-file) *backing substitution* that requires exact byte/length equivalence
    against the retained canonical object, retains canonical ownership while any
    reader holds the old range, and keeps the old source alive until those readers
    release it. It changes no live content, offset, inode identity, namespace entry or
    generation, and it installs no captured state, so it is not checkpoint
    installation. Until safe substitution is possible for a given range, its physical
    bytes stay explicitly charged rather than being silently assumed reclaimed.

**V2 exit evidence (not yet produced):** root-race (two disjoint writers from one
root), replay/exactly-once, quota and short-I/O failure at transitions,
partial-retention and stale-reference, tombstone/covered-cleanup, and bounded
reclamation tests, plus the counters the specification lists.

## 3. V3 — predecessor correspondence without live checkpoint installation

The current failure this must remove is concrete: `FrozenFile::file_may_differ`
(`crates/layerfs-workspace/src/changes.rs:1843-1854`) can only answer cheaply when
the live file's own base root equals the canonical predecessor root; otherwise it
calls `file_matches`, which reads and compares the whole file. `mutate_existing_file`
and `incremental_file_supported` likewise require the live base root to equal the
predecessor content root. After C1 publishes without installing a checkpoint, that
equality no longer holds, so every later small edit would fall back to full-file
comparison/reconstruction — the exact regression the rule forbids.

Selected contract:

1. **Provenance descriptors.** Canonical construction emits, per published inode, an
   ordered metadata-only descriptor stream
   `(canonical_offset, length, origin_id, origin_offset, kind)` where `kind` is the
   existing live-occurrence kind (`base(root,offset)`, `spool(segment,offset)`,
   `inline`, `zero`). Descriptors describe identity, not payload ownership: bytes
   stay readable at the canonical content root. Unchanged predecessor spans inherit
   the predecessor's origin lineage; replacement/inline/zero spans get fresh lineage
   from a per-Workspace monotonic counter that is never reused; contiguous additions
   may extend an origin into never-used coordinates; duplicated coordinates get
   fresh lineage.
2. **Indexed lookup.** Two disk-backed bounded-cache indexes per inode description:
   the canonical-offset ordered stream and `(origin_id, origin_start)` with interval
   end and canonical offset as values. Nonzero origin intervals are disjoint within a
   file, so an overlap query is one floor/lower-bound seek plus forward iteration to
   the query end — no duplicate-candidate scan.
3. **Matching.** The reviewed two-pass algorithm is retained: first stream current
   nonzero descriptors and match exact predecessor origins, journalling monotonic
   anchors `(old_offset, new_offset, length)` with bounded buffers and never letting a
   zero run advance the cursor; then stream the anchor journal with virtual start/EOF
   anchors and match zero runs only inside each bounded old/new gap. Emitted base
   spans are disjoint and monotonically increasing and are validated. Backward-only
   matches are counted replacement input.
4. **Builder integration.** The transient Commit-only view presents the C1 canonical
   content root as the predecessor plus the plan's unchanged spans and
   snapshot-owned replacement readers. It feeds the *existing* incremental engine
   through the existing `ObjectBuffer::set_physical_predecessor` /
   `PredecessorCursor` correspondence path
   (`crates/layerfs-layerstack-store/src/objects.rs:3311`, `:2812-2960`) so CAS
   hints, CDC, FULL/DELTA selection and packing are unchanged and no second encoder
   exists. `file_may_differ`/`mutate_existing_file` are adapted to consume the plan
   instead of failing on base-root inequality; every remaining full-hash/full-build
   path is counted and reported.
5. **Storage and lifetime.** Descriptors and their indexes are private host runtime
   state under the Workspace runtime root (not the canonical Store schema), bounded
   by declared cache/record budgets, reclaimed when superseded and when no active
   attempt retains them.

**V3 exit evidence (not yet produced):** source-allocation reuse, equal-content new
occurrences, partial overlap, zero-prefix insertion, deletion before a large zero
run, shifted offsets, delete/truncate/append, inode recreation, physical
substitution, and repeated large-C1/small-C2 cycles, each with
full-hash/CDC-scanned/visited-range counters proving no full-file reconstruction.

## 4. V4 — authoritative publication-outcome resolution

The publication transaction already exists and is exact
(`crates/layerfs-layerstack-store/src/workspace.rs:423-604`): candidate objects are
admitted before it; inside one `Immediate` transaction the code validates the exact
stage row, the exact expected head/base, the LayerStack ownership and the expected
canonical root, then either deletes the stage (`UpToDate`) or inserts the Commit and
conditionally advances the branch before deleting the stage. Commit identity is
deterministic: `CommitId::derive(candidate_root, expected_head, new_base_layer)`.
What is missing is a witness for the case where the *outcome* is lost (process
death, disconnect, reply loss) after the transaction.

Selected contract:

1. **Minimal explicit Store receipt.** Add one table, `workspace_publications`,
   written *inside* the same publication transaction:
   `(workspace_id, attempt_key, outcome_kind, root_id, head_after, base_after,
   covered_sequence, published_ns)`. One row per resolved attempt; it is the
   authoritative record for both `Committed` and `UpToDate`. This is the explicit,
   minimal schema change the specification requires; it is not a hidden helper and
   not an outcome log.
   **The key must be derived, not random.** Attempt identity is in-memory, so a random
   attempt id cannot resolve uncertainty after a process restart, and #124's retry
   contract requires exactly that ("retry must use the same candidate and expected
   context instead of rebuilding from newer live state"). `attempt_key` is therefore
   the deterministic tuple
   `(workspace_id, candidate_root, expected_head, new_base_layer)`, which is also
   what makes the `Committed` case independently derivable: the same tuple yields
   `CommitId::derive(root, parent, base)`. A retry of the same candidate recomputes
   the same `attempt_key` and therefore finds its receipt.
2. **Resolution procedure on uncertainty** (all four cases use only authoritative
   Store state):
   - receipt exists for `attempt_key` → the outcome is exactly the recorded one;
     apply the receipt, then delete it as part of the idempotent acknowledgment;
   - no receipt, and the branch head is still the attempt's `expected_head` with the
     attempt's `expected_base` → the transaction cannot have advanced the branch
     (the advance is conditional on exactly those values) and no `UpToDate` receipt
     exists, so the attempt is known **not published**; the same candidate and the
     same expected context may be retried;
   - no receipt, and the branch head equals the derived Commit id with parent
     `expected_head` and base `new_base_layer` → **published**; the deterministic
     derivation makes this an independent witness for the `Committed` case;
   - no receipt, and the branch head is a bounded descendant of that Commit →
     **published**; if the bounded ancestry walk cannot reach the attempt's parent,
     the outcome stays explicitly **unknown** and the attempt slot is retained.
     A missing stage is never treated as evidence in any branch of this procedure.
3. **No speculative work.** While an attempt is unresolved, no new snapshot is
   acquired and no second candidate is built from newer live state; the retained
   candidate is retried with its immutable captured input and expected context.
   Conflicts (head/base/root moved without publishing this attempt) are retained and
   reported as an explicit conflict requiring reconciliation, not silently rebased.
4. **Lifetime.** Receipt rows are retained until the attempt's outcome is applied and
   acknowledged, then deleted; a bounded sweep removes receipts whose attempt the
   Workspace has already resolved. Cleanup failure keeps the receipt and its charge
   and never turns a known successful publication into a failed branch update.

**V4 exit evidence (not yet produced):** fault-injected loss of the publication
reply for both `Committed` and `UpToDate`, branch advance by another authorized
actor after publication, retained-stage conflict, idempotent retry of the exact
candidate, and an unresolvable case that must stay `unknown`.

## 5. Removal/audit inventory against current source

| Element | Current location | Required disposition |
| --- | --- | --- |
| Freeze/pause and writer/execution completion gates | `crates/layerfs-workspace/src/lifecycle.rs:539-580` (`projection::pause`, `worker.wait_for_writers`, `worker.quiesce`) | Remove from ordinary Commit |
| Full fact export as snapshot input | `projection::capture` → `live_backing.rs` facts/incoming groups | Replace with the owned snapshot reader |
| Same-generation checkpoint installation and global reset | `lifecycle.rs:260-328` (`install_checkpoint`), `lifecycle.rs:222-258` (`refresh_reconciled`), `crates/layerfs-workspace-core/src/checkpoint.rs` | Remove the ordinary-Commit obligation; keep only consumers that still need explicit live replacement |
| Stage/publication implying inactivity | `pending_stage`/`pending_publication` in `ensure_active` (`lifecycle.rs:355-364`) | Attempt state must not gate ordinary operations; only real lifecycle validity does |
| Bootstrapping base-root retarget | `base_root`, `base_inodes`, `expected_head` on publication (`lifecycle.rs:308-321`) | Separate live provenance from published comparison context |
| Build-duration live borrow | `Workspace::commit(&mut self)` and `changes::CandidateInputs`/`FrozenWorkspaceChanges` (`layerfs-workspace-core/src/lib.rs:123-152`) | Replace with owned snapshot access and bounded cursors |
| Kernel cache flush path | `live_owner.rs:2580-2630` (`flush_kernel_cache`: `inval_inode` + `syncfs`) | Not reused by Commit; **still required** by its own consumers until V1 is decided |
| Must-audit non-Commit consumers | see §5.1 | Preserve their independent semantics; delete only what becomes unreachable |

### 5.1 Non-Commit consumer audit (required before Phase 5 deletes anything)

| Helper | Actual consumers | Disposition |
| --- | --- | --- |
| Container cut gate + `flush_kernel_cache` (`live_owner.rs:2580-2690`, `2946-2967`, `3196-3210`, `1935`) | **not only Commit**: the ordinary SDK splice path takes the same cut with `freeze_diagnostic(false, …)` before reading live-owner state (`live_owner.rs:2956-2961`), `wire::FREEZE`/`wire::RESUME` are the host-driven Commit path, `prepare_shutdown` takes the cut (`:2568`), and `gate.cache_flush()` is used directly at `:1936` | Keep the primitive. Commit must stop using the *complete-dirty-prefix* flavor (`publish = true`, whole-prefix facts), not delete the cut, its edit-path use, or shutdown drain |
| `syncfs` role beyond visibility | the same call is the product's kernel **writeback-error channel** (`live_owner.rs:2619-2621`: "syncfs observes the mount's writeback error sequence, including kernel-side allocation errors") | A Commit that no longer calls it also stops surfacing kernel-side writeback errors at that boundary; the replacement must state where those errors are observed, or the semantic loss must be declared |
| Host `install_checkpoint` (`live_backing.rs:2080`) | `lifecycle.rs:642` Commit presentation plus `live_backing.rs:1209/1512/1661/1673` tests | Remove only the Commit call; keep the function while remote/explicit install paths exist |
| Container checkpoint installation (`live_owner.rs:363-380`) | `validate_checkpoint_record` / `install_checkpoint_record` / `finish_checkpoint`, driven by `wire::INSTALL_BEGIN`/`INSTALL_END` (`:3224-3235`) | Independent of the host Commit path; must keep working |
| `Workspace::ensure_active` (`lifecycle.rs:355-364`) | `cow_tree.rs:224,585-692`, `execution.rs:175`, `file_io.rs:409,491,523` — i.e. every read, write and namespace operation | Redefine to lifecycle validity only. Note: `objects.rs:2570,4058,4230` define an unrelated admission-session `ensure_active`; it must not be conflated |
| `pending_stage` / `pending_publication` / `pending_checkpoint` | `lifecycle.rs` (commit fast path, `discard`, `end_clean`, `recover_workspace_presentation`, verification state), `cow_tree.rs:96`, `live_backing.rs` (remote install) | Move attempt state into the attempt record; preserve `discard`/`end_clean` and remote-install behavior |
| `mutation_generation` / `mutation_paths` | `session.rs`, `registry.rs`, `changes.rs`, `reconcile.rs`, `file_io.rs`, `projection.rs`, `live_backing.rs`, `layerfs-fuse/src/live_owner.rs` (`wire::OBSERVE`), `file_edit.rs`, `checkpoint.rs` | Replacement must keep: OBSERVE diagnostics, reconciliation invalidation (`invalidate_if_mutated`), capture invalidation, and report/telemetry consumers. The checkpoint generation-equality check disappears with installation and must not be silently repurposed |

## 6. Benchmark catalog recorded for Phase 7

Enumerated with the existing registry entrypoint (no benchmark executed):
`target/release/fs-benchmark-pro infra-list` → 260 scenarios, 21 families, 258
supported, 29 proof-only. Raw output and the machine-readable ledger are in
`benchmark-results/v016/phase1-catalog/` (`infra-list.jsonl`, `scope-ledger.json`).

- The [exclusion manifest](benchmark-exclusions-issue122.json) declares 36
  #122-owned cases (33 regular, 3 extended); 28 of them are present in the current
  active registry and 8 are still planned/unregistered
  (6 `historical_access` `v016-access-*`, 2 `branch_development`
  `v016-branch-convergent-content-v1`/`v016-branch-fork-descendant-v1`).
- `E ∩ U` = 28, all `v016-`prefixed, none in a family excluded wholesale. Included
  registry scenarios: **232**.
- Family-specific entrypoints not returned by the generic registry:
  `historical_access` (11 cases, `historical-access-v2`), `repository_history`
  (3 optional profiles, `NOT_RUN_OPTIONAL` unless selected), and
  `small_file_delta_smoke` (1 exploratory case, no numerical gate; classification
  to be re-confirmed on the sealed candidate).
- Provisional counted inclusion: 232 registry + 11 historical access = 243 rows,
  with every #122 row recorded as `EXCLUDED_ISSUE_122` and the 8 unregistered
  exclusions recorded as planned, not as registered rows.
- `shared/v016_matrix.py` is **not** the Phase 7 driver; it targets the excluded
  matrix.
- These numbers are provisional because they were enumerated from the documentation
  commit's binary (`LAYERFS_SOURCE_COMMIT=536aaf9ded4e09a9ccfcf2b251aa4db995b383e6`,
  identical to `86f6a0a0f` except for documentation). Phase 7 must re-enumerate from
  the sealed candidate before executing anything.

## 7. Phase status

- Phase 1: complete for V2/V3/V4 (resolved above) and for V1's separable
  acknowledged-bytes half (contracted in §2.8); **V1's mapping-visibility half is
  open and requires the owner decision in §1**. Per the implementation plan, the
  documented incompatibility plus an open implementation issue is the specified
  Phase 1 exit for this case; it is not a completion claim for the product.
- Phases 2–5: not started. The plan's exit condition "no code claiming full public
  snapshot support proceeds on a known weaker visibility model" is respected: no
  product code has been written for this feature.
- Phase 6–7: not started; the provisional scope ledger above is preparation, not
  benchmark evidence. No benchmark case was executed, and no #122 case may be.
