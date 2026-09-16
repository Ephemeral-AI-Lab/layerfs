# Handoff prompt: close Stages 3-4 (#168 / #169) after the independent review

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Supersedes the *execution order* in `stages-3-4-continuation-prompt.md` and the
> checkpoint split in `stages-3-4-completion-handoff.md`. It removes **no** requirement from
> `stages-3-4-handoff.md`, `stages-3-4-file-plan.md`, `stages-3-4-verification.md` or either
> issue body.

## Copy/paste assignment

You are the implementation agent closing Stages 3-4 in:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`

An independent review has been completed against the pinned snapshot
`91c3a0741fff64e8161d5c1b6e759f347ffbf757`. It found the implementation **substantially
complete but not complete**: two gate defects, two contract-level deviations, a set of
bounded-owner defects, several oracles that do not test what they claim, and one large
evidence gap (no admission-grade measurement exists at all). Read the review before touching
code; it is the authoritative list of what is missing and why, with file:line citations:

* `docs/roadmap/0.1/0.1.7/component-decoupling/stages-3-4-review-20260916T233008Z.md`
* `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-review-20260916T233008Z/README.md` (index) and
  its companions `criteria-stage3.md`, `criteria-stage4.md`, `limits.md`,
  `simplification.md`, `evidence-audit.md`, `memory-safety.md`, `loc-structure.md`.

Your assignment is to work the packets in §3 in order and to **continue until the completion
gate table in §7 is fully PASS, or an owner waiver is recorded for a named row**. Fixes,
proof and measurement are all in scope. Do not stop at a proposal, a partial packet, or a
green smoke run.

---

## 0. The continuity contract (read this first)

**You do not stop until completion.** Concretely:

1. Work one packet at a time, in the order given. After each packet: run that packet's
   checks, write its evidence, commit it, and update the tracking table in
   `stages-3-4-closeout-report.md` (§11). Then start the next packet in the same session.
2. **Never end a turn with a packet half-done and uncommitted.** If you must stop mid-packet
   for a genuine reason, commit what passes and record the exact continuation (commit/tree,
   remaining files, the failing case, the next action) in the closeout report.
3. You may stop **only** for one of these, and only after recording it in the escalation
   format of §9:
   * **B1 - owner decision required.** The n-sample measurement campaign (W8) and the two
     format/design deviations (W6.3, W6.4) need an owner yes/no. Ask once, in writing, then
     keep working on every packet that does not depend on the answer.
   * **B2 - third-party or dependency blocker.** Something needs a patch, fork, vendoring or
     a new dependency. That is forbidden; report it and route around it.
   * **B3 - measurement lock conflict.** Another agent is running resource-sensitive work.
     Wait, keep doing non-resource work, and resume. This is a schedule event, never a
     completion.
   An external run limit, a long task, a hard bug or an unanswered question is **not** a
   stopping condition. A run limit is not a technical blocker and is not completion.
4. **Never declare completion without §7.** "Implementation complete" is a claim about the
   gate table, not about how the code feels. Every row is PASS with a named artifact, or
   owner-waived, or it is not done.
5. **Never fabricate evidence.** No number without a receipt, no receipt without its
   identity, no PASS without an oracle you actually read. Missing metrics are `null` with a
   reason, never zero.
6. When the gate table is fully PASS, close #168 and #169 with the evidence summary.

---

## 1. Read before you touch anything (checklist)

* [ ] Repository `AGENTS.md` and `core/AGENTS.md` (production-source purity, file caps,
       one-attempt rule, no preflight, per-commit production LOC).
* [ ] `docs/general/benchmark_rules.md`, `benchmark/AGENTS.md`,
       `benchmark/fs-bench-pro/QUICKSTART.md`, `docs/general/release-policy.md`,
       `docs/general/documentation-policy.md`.
* [ ] `stages-3-4-handoff.md` (all acceptance text and hard invariants),
       `stages-3-4-file-plan.md` (file/LOC plan and the required actual-size table),
       `stages-3-4-completion-handoff.md` (D/E algorithms and proof gates),
       `stages-3-4-verification.md` (the frozen measurement contract),
       `stages-3-4-measurement-addendum.md` (round declarations),
       `stages-3-4-report.md` and `stages-3-4-final-review.md` (note their known defects,
       §3 W9).
* [ ] **The independent review and all eight companions** listed in the assignment.
* [ ] Current #168 and #169 bodies (`gh issue view 168`, `gh issue view 169`): they carry the
       E1-E3 and D1-D4 gates and the qualification gate.
* [ ] Design inputs: `file-content.md`, `physical-encoding-and-packing.md` (especially the
       format table at lines 270-284), `content-storage-policy-and-tables.md`,
       `finalized-object-handoff.md`, `content-io.md`, `admission-and-persistence.md`,
       `content-io-memory-audit.md`, `telemetry.md`.
* [ ] `git status`; `git log --oneline -20`; confirm the reviewed snapshot is an ancestor of
       your starting point, and record your actual start commit/tree.

**Later owner instructions that override stale text in any document.** `tools/preflight.sh`
is permanently retired: do not run it, restore it, or add an aggregate gate, workflow or
wrapper. The Stage 2 publication watermark and pending-group fixes must be retained. The
amended schema baseline is four tables / twenty-one columns.

---

## 2. Hard invariants (checklist - a violation is a stop-and-fix, not a tradeoff)

* [ ] **One attempted operation.** No retry, no busy handler, no error-driven fallback, no
      alternate algorithm on failure. Unsupported required capabilities fail explicitly.
* [ ] **No durability claims.** No `fsync`, `fdatasync`, `sync_all`, `sync_data`, no WAL, no
      crash-durability machinery. Keep MEMORY journal, `synchronous = OFF`, zero busy
      timeout, runtime transaction atomicity. Unknown acknowledgement outcome fails without
      replay or guessed deletion.
* [ ] **No third-party patch, fork, vendor or registry edit.** Builds stay `--locked`. No
      `[patch]`/`[replace]`. No new dependency when std or an existing crate already provides
      the capability.
* [ ] **No new architecture.** No registry, plugin framework, generic buffer manager,
      transport abstraction, monitoring subsystem or interface for a hypothetical future
      deployment.
* [ ] **Product source is product code only.** `core/crates/*/src` has no `#[test]`,
      `#[cfg(test)]`, `cfg!(test)`, fixtures, fault injection, test counters or test-only
      hooks. Instrumentation for measurement lives in `tests/` or `examples/`.
* [ ] **Caps.** Every production file at most 999 physical lines; every `lib.rs`/`mod.rs` at
      most 200, declarations/reexports/direct delegation only. Runtime SQL counts as
      production.
* [ ] **Isolation.** C1 `layerfs-content` must not gain a SQLite, C2, Workspace, FUSE,
      daemon, history or checkpoint dependency, and must not reference `crates/`. C2 must not
      construct files.
* [ ] **One construction worker.** `construction_worker_limit()` and the canonical
      construction stay single-producer; every run exports `LAYERFS_CONSTRUCTION_WORKERS=1`;
      no second lane or helper worker is added, and no worker count is raised to pass a gate.
      The only exception remains namespace init, which no case here uses.
* [ ] **Preserve the retained correctness fixes:** exact CAS comparison, canonical identity,
      frozen CDC, extent partition, dependency/chronology checks, publication watermark,
      pending-group same-save reads, private early commits, one-owned-cleanup-attempt,
      quarantine on unknown outcome.
* [ ] **Charged capacities.** Any new state must state its owner, byte/count bound, live
      multiplicity, lifetime and release event. No size-proportional hidden collector, no
      whole-file reconstruction fallback, no whole-input/edit collector.
* [ ] **Evidence is append-only.** Never overwrite, re-label or promote a historical receipt.
      New rounds get new directories. Failures and INELIGIBLE rows stay on disk.
* [ ] **Measurement discipline.** Declared cache state, one sample per case per arm unless an
      owner-approved campaign says otherwise, no best-of, no warm-cache credit, no raised
      timeouts, no dropped failing cell, fresh `--output` per run.
* [ ] **The measurement lock.** Never overlap resource-sensitive work and never interrupt
      another owner's run.
* [ ] **No aggregate gate.** Run the explicit per-workspace commands in §6 individually.

---

## 3. Work packets

Each packet lists: the defect, the exact change, the proof, and the "done when". Packets
W1-W4 are the **gates**; W7-W8 are the **evidence**; the rest are required but smaller.
Complete them in order.

### W1 - CHUNK delta candidates must obey the eligibility rule (gate, ~10 lines)

* **Defect.** `core/crates/layerfs-storage/src/encoding/delta/select.rs:209-210` takes
  `advisory.first()` for `ObjectRole::Chunk` with no eligibility probe, so `:243`
  `acquire(input, base_id)?` on an id with no committed row returns
  `StorageError::ObjectMissing` and **fails the save**. The `WHOLE_FILE` path
  (`:295-311`, `:313-331`) probes through `eligible()` and counts `absent_candidates`, so the
  same situation selects FULL. Handoff `stages-3-4-handoff.md:119-121` and the required
  `delta_payload` "absent/ineligible optional candidates" case both require FULL by policy;
  v0.1.6 does the same
  (`crates/layerfs-layerstack-store/src/objects/admission.rs:676-680` maps
  `NativePriorOutcome::Unavailable` to `NativeFallback::Unavailable`, then stores FULL).
* **Change.** Route the CHUNK candidate through `eligible()` exactly as WHOLE_FILE does,
  counting `absent_candidates` / `ineligible_candidates`, and return the prepared FULL record
  when it is absent or ineligible. Do not add a retry over other candidates: CHUNK keeps its
  first-candidate rule; it only gains the eligibility decision.
* **Proof.** New `delta_payload` cases, using real stores and real malformed/absent inputs:
  (a) the first supplied CHUNK candidate is absent -> FULL stored, readback exact,
  `trials == 0`, `absent_candidates == 1`; (b) the first candidate is present but ineligible
  (wrong role or depth at/over the cap) -> FULL, `ineligible_candidates >= 1`; (c) the
  same-save unsealed predecessor case from a real C1 multi-edit stream -> FULL, no error.
* **Done when.** All three cases pass, and a real integrated multi-edit chunked pipeline case
  (see W4.5) exits 0.

### W2 - the pooled lane must honour the publication watermark (gate, ~20 lines)

* **Defect.** `core/crates/layerfs-storage/src/cas/owner.rs:459`, `:544`, `:658`, `:685` pass
  `i64::MAX` as the read ceiling. `stages-3-4-report.md:112` claims the watermark is applied
  to every dependency and cache read. Today the only safeguard is the
  `retained_pack_ceiling != highest_pack_id` precondition in `MutationOwner::acquire`
  (`:162-171`), which lives in a different module.
* **Change.** Thread the owner's own `self.ceiling` (which already includes packs this save
  created) into those four call sites. `encoding/pool/read.rs:75-80` already returns
  `VisibilityCeiling`; keep that behaviour and make it reachable.
* **Proof.** A test that writes a value group, leaves its pack above the captured ceiling
  (an unfinished save's early commit), and asserts the pooled read refuses it rather than
  returning bytes; plus a re-run of `metadata_pool`, `metadata_chain`, `metadata_window`,
  `metadata_pool_index`, `edit_pipeline`.
* **Done when.** No pooled read path passes `i64::MAX`, and the refusal test passes.

### W3 - checkpoint D: own the decoded frontier and prove finality (gate)

* **Defect.** `core/crates/layerfs-content/src/file/edit/tree.rs:52-58` holds
  `deferred: BTreeMap<ObjectId, Vec<u8>>` - **encoded** drafts. `hold_node` (`:105-139`)
  encodes and hashes every draft at creation; `:112-132` only ever grows the charge, so a
  superseded draft is never released; `:82-101` clones and re-decodes on each visit; and
  `commit_node` (`:185`) decodes the canonical bytes again before encoding them into a
  `FinalizedObject`. `stages-3-4-handoff.md:235-256` and `:417` require one **owned decoded
  unfinished representation** with "drop overwritten drafts and encode/hash only final
  nodes"; `stages-3-4-completion-handoff.md:134-145` names the two states explicitly.
* **Change.**
  1. Hold decoded entries plus a checked summary (a stored subtree stays
     `ObjectId + NodeSummary` until a split or join actually needs its boundary).
  2. Encode and hash exactly once per node, inside `commit_node`, after it is proven final.
  3. Release a draft's charge when `split`/`concat` supersedes it, so the live bound is a
     function of the two boundary paths, the builder levels and the page capacity - not of
     the edit count.
  4. Keep the emission rule: children before parents, and a node a later edit or join could
     still change is never emitted.
  5. Do not add a prune pass, a second representation, a runtime switch or a fallback route.
* **Proof.**
  * All nine `edit_reference` cases still match root, partition and survivors exactly
    (the reference oracle and a re-execution recipe are in
    `evidence/stages-3-4-review-20260916T233008Z/oracle-replay/`).
  * `edit_localized` still shows path/boundary work rather than a full mapping walk.
  * New assertions on `EditCounters::nodes_created` (equals the number of emitted mapping
    objects) and `peak_deferred_bytes` (does **not** grow linearly with edit count on a
    fixed-shape file), replacing the vacuous test in W4.1.
  * A written finality argument per emission rule (why no later admitted edit or either side
    of a join can change that node), in the closeout report.
* **Done when.** The frontier is decoded-and-owned, encodes/hashes are final-only, the three
  new assertions pass, and the finality argument is committed.

### W4 - repair the oracles and close the unexercised cases (gate)

* **W4.1** `core/crates/layerfs-content/tests/edit_bounds.rs:240-273`
  (`the_retained_frontier_does_not_grow_with_the_file`) applies `Edit::delete(0, 0)`, which
  `file/edit/apply.rs:62-74` short-circuits to the base root, so the builder never runs and
  `peaks` is the base's extent count. Replace it with a real builder-stressing edit and assert
  the producer counters (`peak_deferred_bytes`, `nodes_created`), not the base's extents.
* **W4.2** `core/crates/layerfs-storage/tests/policy_capacity.rs:146-158` is named for
  depth-versus-budget independence but compares cutoffs at identical depths. Add a real
  depth-4 versus depth-50 budget comparison (and, if depth 50 remains accepted and
  usable after W6.1, prove its boundary).
* **W4.3** `core/crates/layerfs-content/tests/object_identity.rs:250-251` asserts constants
  equal themselves. Exercise plus/minus one at the 8 MiB field and 16 MiB envelope bounds
  (encode, store and reject), or record explicitly why the bound cannot be exercised.
* **W4.4** `core/crates/layerfs-content/tests/streaming.rs:80-84` discards `peak` and
  `:30-37` builds an unused `CountingSource`. Assert both, or delete the dead scaffolding.
* **W4.5** Add an integrated multi-edit chunked case to `edit_pipeline` (the current
  `:151-192` test loops over three single-edit vectors despite its name), covering current
  result coordinates through a real Store.
* **W4.6** Call `support::assert_children_precede_parents`
  (`core/crates/layerfs-content/tests/support/mod.rs:175-195`) from an `edit_*` target, so
  child-first emission is asserted for edits and not only for complete construction.
* **W4.7** Remove or fix every test name that overclaims its oracle
  (`metadata_pool.rs:136`, `delta_chains.rs:166`, `metadata_pool_index.rs:160`), and add the
  missing slow-bounded-sink case.
* **W4.8** Reach the value-group compression branch in a pooled test (today every fixture is
  BLAKE3-derived and deduplicated, so `pool/value_group.rs:50-53` and its reader never
  execute) and cover the pooled tamper / instruction / "pooled chain work" paths.
* **Done when.** Each item has a named test that fails without the fix and passes with it,
  and the review's §2.4 list is empty.

### W5 - bounded-owner fixes (required)

Each of these is a real unbounded or duplicated owner. Fix them together so the memory audit
in W7 can report a closed ledger.

* **W5.1 - cold-start catalogue collector.** `encoding/pool/index.rs:224-246` calls
  `pool::catalogue(connection, None)`, which materialises every catalogue row into a `Vec`
  (`sqlite/pool.rs:123-151`), on a cold Store with more than `1 + METADATA_INDEX_VALUES`
  ordinals. Stream the statement (or page it) exactly as the reference does
  (`crates/layerfs-layerstack-store/src/objects/metadata.rs:252-281`). Add an accounting
  assertion on the transient row count.
* **W5.2 - per-object canonical clone.** `cas/save.rs:50` clones the whole
  `FinalizedObject` for every wave member. Change `owner.offer` (`cas/owner.rs:324`) to take
  `&FinalizedObject` (it reads only id, role, canonical and length) and delete the clone. The
  file plan already promised "moved input; no per-object clone".
* **W5.3 - placement deep clone.** `pack/placement.rs:84-86` clones the open pack's groups
  and the incoming group to measure a fit. Compute the fit from
  `assembled_length(lane, &open.groups)` plus the incoming group's framed length. While you
  are there, use `lane.group_count_limit()`/`lane.pack_limit()` instead of the
  `GROUP_COUNT_LIMIT`/`PACK_LIMIT` constants (`:83,86`), and either use or delete the dead
  `pack/layout.rs:210 fits`.
* **W5.4 - builder level growth.** `file/mapping/build.rs:187-199` flushes one fixed level and
  pushes only into `level + 1`; its sole call site is `flush_streaming(consumer, 0)`
  (`:165`). A file with many chunks therefore accumulates level-1 entries until `finish`.
  Make every level flush under the same rule, keep the canonical half-partition, and assert
  `MappingBuild::peak_pending()` stays a function of height and page capacity - the doc at
  `:60-62` currently claims a bound the code does not deliver.
* **W5.5 - operation-lifetime pack cache.** `cas/owner.rs:111` never evicts and has no byte
  cap; `encoding/pool/read.rs:32` is a second copy. Give the cache a declared byte bound with
  the same live-multiplicity accounting as the other lanes, or clear it at the declared
  release event. Correct the "inside this wave" comment at `select.rs:162-163` either way.
* **W5.6 - cleanup transaction.** `sqlite/cleanup.rs:31-75` runs one transaction with a
  budget of up to about 128 M row deletes under a MEMORY journal. Apply the existing
  `TRANSACTION_ROW_LIMIT`/`TRANSACTION_CANONICAL_BYTES_LIMIT` discipline (the owner enforces
  them at `owner.rs:774-777`; this path never calls it), keeping exactly one cleanup attempt
  and the newest-first deletion order.
* **W5.7 - dead budgets and miscounted counters.** `METADATA_INDEX_BYTES` (32 MiB) has no
  readers - either enforce it or delete it and state the real bound (entry count times the
  charge). `candidates.rs:104-106` reports the fat-pointer size; report the declared index
  bytes. `delta/read.rs:103` resets the shared `ChainCounters` per resolution, so
  `SaveOutcome.chain` reports the last chain rather than the save total - make the reported
  number mean what its name says.
* **W5.8 - pooled read row loop.** `encoding/pool/read.rs:266` issues one SQL point query per
  pooled row (up to 100 per leaf) and `:72-74` clones the whole cached group per row. Cache
  the covering group row and add an into-style accessor.
* **Done when.** Every owner in the W7 ledger has a stated bound and a release event, and
  `memory_bounds`/`edit_bounds`/`edit_localized` still pass.

### W6 - latent traps, dead code and design deviations (required)

* **W6.1 - metadata depth above 15 is accepted but unusable.** `cas/owner.rs:673-674` charges
  `(depth + 2) * 8192` against the fixed 139 281-byte encoded limit, so any accepted depth
  above 15 can never admit a chain. Either derive the charge from the actual record width, or
  narrow the accepted range to what the budgets can honour - and then make the policy, the
  schema CHECK, the persisted row and the tests agree. Do not leave "accepting a field never
  used" in place (`stages-3-4-handoff.md:157`).
* **W6.2 - constant and dead-dispatch cleanups.** `storage/policy.rs:245,247`
  (`chunk_canonical_limit`, `chunk_frame_limit`) have no readers, and the former is computed
  with a hard-coded 21 while the canonical envelope is 13
  (`content/object/codec.rs:18,23`). `encoding/delta/select.rs:186-194` is a dead
  `InodeLeaf` branch that also hides the whole-file-depth trap in
  `StorageCapacities::delta_depth_for_role` (`storage/policy.rs:315-320`). Rename the
  23-byte envelope constants or correct them. Remove the ~200 lines of dead code listed in
  the review's §5 (all of it absent from both v0.1.6 and Stage 2), including
  `file/view.rs:113-171` and the uncalled `depth_cache_entries` pair.
* **W6.3 - format/design deviation 1 (needs the owner's yes/no).** The design table
  (`physical-encoding-and-packing.md:277`) names v6 as "Active pooled physical inode
  metadata", but the implementation writes pooled **value groups** in v6 and pooled **leaf**
  records in the v1 Ordinary lane, distinguished by the `objects.object_role` column. Decide
  with the owner: either keep the current lane assignment and record the deviation in the
  design document, or move pooled leaf records to v6. Whichever is chosen must be covered by
  `physical_formats` and stated in the report.
* **W6.4 - format/design deviation 2 (needs the owner's yes/no).** The same table names v5 as
  a "Supported older unpooled metadata reader"; the candidate rejects it. Because the
  candidate's schema identity is deliberately not the reference's, v5 has no Store to act on
  in this batch. Record that scope decision explicitly in `physical-encoding-and-packing.md`
  and the report; do not silently leave "unknown version rejected" standing for a named
  supported reader.
* **W6.5 - remaining simplification findings.** Work the ranked list in the review's §5 that
  is not already covered: triplicated `emit_file_state`; the `apply.rs` scaffolding; the
  triplicated FULL-fallback block and the dead `policy` parameter in `owner.rs`; the double
  value-group body copy; `coalesce` duplication; the per-edit `FileState` rebuild. Keep the
  §5.2 "not removable" list intact - in particular the `tree.rs` commit walk, the exact CAS
  comparison, backpressure, atomicity, visibility and cleanup.
* **Done when.** Each item is either fixed or has a committed, owner-visible decision, and
  the review's §2.8-§2.10 lists are closed.

### W7 - memory instrumentation and the allocation ledger (gate for #168's resource item)

**No heap or RSS measurement exists anywhere in the seven retained evidence generations.**
That is why #168's "simultaneous index/codec/SQL memory" gate is unmeasured. Fix that with
real instrumentation, kept out of product source.

* **W7.1 - external counting allocator.** Add a counting `#[global_allocator]` in an
  **external target** (a `tests/` target or an `examples/` tool, never `src/`), recording
  phase-local current bytes, peak bytes and allocation counts around each timed scope. This
  is measurement code, not a product hook.
* **W7.2 - process scope.** Record RSS separately and label it honestly: `getrusage`
  `ru_maxrss` is a **lifetime high-water**, not a phase number; a phase-local figure needs
  sampled residency with the sampling window and coverage stated. Never use a lifetime
  counter as a phase number, and never infer heap from file size.
* **W7.3 - the ledger.** For every significant allocation, state owner, byte/count bound,
  live multiplicity, capacity/transient overlap, lifetime and release event - covering the
  COW frontier, the immutable bases, the FULL and PREFIX alternatives, all five pack lanes,
  the SQLite MEMORY journal and BLOB copies, the ordered-set nodes, the pooled value cache and
  the telemetry report. `Store::pool_index_entries`/`pool_index_bytes` are reported live
  capacity, not a test hook; keep them.
* **W7.4 - SQLite honesty.** `cache_size` is not total RSS and must not be presented as a
  memory cap. State what the profile actually sets (journal, synchronous, temp_store,
  foreign_keys, busy_timeout) and that the engine maxima are the host library's because the
  build links a system `libsqlite3`. If the connection count or the journal can grow
  unbounded, either bound them (W5) or report them as an unqualified limit.
* **Done when.** A committed instrumentation target produces a phase-local heap ledger plus a
  labelled RSS scope, the ledger is closed (no unowned owner), and the numbers appear in the
  closeout report with their coverage and their nulls.

### W8 - the matched performance, storage and memory campaign (gate; needs one owner yes/no)

This is the largest remaining block and the one that cannot be replaced by tests.

* **W8.1 - ask once, in writing.** The verification contract allows one sample per case per
  arm unless an **owner-approved** campaign says otherwise. Ask the owner for an n-sample
  campaign on the existing matched C1 fixture. Keep working on other packets while you wait.
* **W8.2 - declare before collecting.** Commit a versioned addendum that freezes: exact
  cases and sizes, seeds, reference revision `44cf748486863ab7c21ca47e731bd88e2b9a7b4a`,
  candidate commit/tree, harness and tool identities, profile, single-worker setting, cache
  state per case, the acknowledgement boundary, every numerical limit, the sample count and
  the allowed claim. No receipt may exist before its addendum is committed.
* **W8.3 - align the byte boundary.** The current pair is not like-for-like: the reference
  counts the delta inserted into a deduplicating map
  (`crates/layerfs-content/examples/rope_edit_timing.rs:66-67,80-85`) while the candidate
  counts canonical bytes accepted by its consumer
  (`core/crates/layerfs-content/examples/edit_timing_c1.rs:143-151`). Fix the accounting rule
  on both sides (newly inserted store bytes is the natural rule), and replace the fixture
  replacement with **non-duplicating** content: today both tools build the replacement from
  the same seed as the base, so dedup can mask payload writes either way.
* **W8.4 - cover what is missing.** The verification contract's registry must actually be
  run or explicitly NOT_RUN with a reason: whole-file delta win and lose, chunk delta win,
  empty and equal-replacement no-ops, the singleton-1m case, and a real `store-bytes` row
  (the pipeline receipts currently report neither pack bytes nor database bytes).
* **W8.5 - required counters per case.** Raw operation elapsed with units and sample count,
  whole-command wall, candidate/usable/FULL/DELTA/reuse counts, chain work, source/base/pack
  bytes read, query and transaction counts, copies/hashes/encodes/assemblies where observable,
  total retained database, pack, value-group and index bytes with occupancy, allocation
  ledger, and on/off timer equivalence. Missing counters are `null` with a reason.
* **W8.6 - the time/space gates.** Resolve "existing-or-better" for the operations the
  reference can actually expose. Where the products share no public surface, publish a
  non-comparative diagnostic and say the speedup is unproven - do **not** compare a Workspace
  pipeline with a core edit, and do not present new cutoff support as an old-profile win.
* **W8.7 - disclose the clipping.** The retained `e1c-pooled-512` receipt is telemetry-clipped
  (`[incomplete]`, `"incomplete": true`, 57.5 % of the scope unattributed) and the ledger,
  `stages-3-4-verification.md:74-80` and `stages-3-4-final-review.md:281-292` quote it
  without saying so. Either re-collect that arm inside its budget with the detail retained, or
  annotate every quotation of it. Never re-label the existing receipt.
* **W8.8 - identities after every change.** A rebuilt artifact needs a rebuilt matched arm.
  Re-verify source, binary and harness identities after the campaign and record the recheck.
* **Done when.** Either the gates are PASS with receipts that satisfy §8's evidence checklist,
  or the owner has recorded a waiver naming the unmet row and the smallest missing case.
  `NOT_RUN` with a measured wall time and a reason is acceptable; a fabricated zero is not.

### W9 - documents, counter and the missing plan table (required)

* **W9.1** Fix `stages-3-4-report.md`: its §1 still says pooling is NOT IMPLEMENTED and the
  frontier is PARTIAL while its own §5/§6 claim the opposite; its HEAD is five commits behind;
  and its per-target table lists `timing` (C1) as 11 tests where the target runs 7 (the global
  total of 245 is correct).
* **W9.2** Fix the "Small -> large and large -> small still rebuild the mapping" paragraph: the
  code implements exactly what handoff §3.C prescribes (`apply.rs:106-109`), with no fallback
  route. Replace the stale self-criticism with the real residual gap (transition read
  amplification is unmeasured) or drop it.
* **W9.3** Fix the `edit_reference.rs` header ("this target fails today, on purpose") and the
  oracle README's `git diff` claim (true for `src`, false for the whole path).
* **W9.4** Add the per-file actual-size table the file plan requires
  (`stages-3-4-file-plan.md:283-299`) - path, action, before, after, delta, recommended range,
  below/within/above, physical lines, responsibility. The review's `loc-structure.md` and CSVs
  are a starting point, not a substitute.
* **W9.5** Counter correction. `tools/production_loc.py:135` removes code whose
  `#[cfg(...)]` merely contains "test" (1 027 reference lines over-removed) and counts 3 737
  lines of `src/`-resident reference test modules. Fix the counter, apply the **same**
  corrected counter to both snapshots, add a focused tool test for each defect, and restate
  the reference and combined totals. Until then no combined total may be quoted.
* **W9.6** Update `core/README.md`, the roadmap index, `implementation-issues.md`/
  `implementation-plan.md` and the ledger with the closeout entry.
* **Done when.** No acceptance document contradicts the source, the counter is corrected and
  re-applied to both snapshots, and the plan's actual-size table exists.

### W10 - limits boundary coverage (required where reachable, honest where not)

Run the boundaries the review found unqualified, through ordinary public APIs with bounded
fixtures - never by allocating a theoretical maximum, exhausting disk or starting an
endurance campaign:

* [ ] CHUNK depth at a changed cap, and depth 50 (or the narrowed range from W6.1).
* [ ] The 4 096-edit ceiling: at, just below and just above, with the rejection happening
      before mutation.
* [ ] The 8 MiB - 1 deferred-state ceiling: induce it, or record the exact source proof and
      the coverage gap.
* [ ] The 8 MiB field / 16 MiB envelope bounds at plus or minus one (ties into W4.3).
* [ ] The 131 072-entry pooled window filled with **distinct** rows, plus the reset and the
      cold-start replay at that scale.
* [ ] The read-wave byte ceiling and the `PoolReader` 512 KiB cache reset.
* [ ] A file larger than 24 MiB (C1) and larger than 5 MiB through a real Store (C2), inside
      the command budgets.
* [ ] The SQLite engine maxima: state them as environment-dependent and unqualified unless you
      pin and verify them.
* [ ] Directory/workspace dimensions: record as outside Stages 3-4 (Stages 5/7) - do not
      invent a value.

For every boundary record: the value tested, the API used, the observed behaviour at and
over the limit, whether validation happened before mutation, and the exact command. Impossible
or unrun coverage is written as unrun with the reason.

---

## 4. Evidence checklist (applies to every packet)

For each completed item, the closeout evidence directory must contain:

* [ ] The exact command line and its exit code.
* [ ] Raw stdout/stderr, not a summary.
* [ ] Source/tree identity, toolchain identity, binary sha256 where a binary ran.
* [ ] Fixture identity (seed, size, generator) and where the fixture came from.
* [ ] Declared cache state, profile, worker count, sample count.
* [ ] Whole-command wall time against the 15 s budget (declared exceptions up to 25 s;
      verification up to 60 s).
* [ ] The numbers, with units, and their provenance (measured / source-derived / unproven).
* [ ] Every failure, INELIGIBLE and NOT_RUN row, kept on disk.
* [ ] A statement of what the artifact does **not** prove.

Use fresh directories per round; the review round
`evidence/stages-3-4-review-20260916T233008Z/` and every earlier generation stay untouched.

---

## 5. Per-commit production LOC (mandatory on every commit)

Count with one audited counter applied identically to the commit's **first parent** and its
**committed tree**, prepared from the final staged tree. Count nonblank, non-comment
first-party product implementation including imports, declarations and shipped runtime SQL;
exclude tests, examples, fixtures, tooling, docs, manifests, generated and third-party code.
Put the result in the commit message as:

```text
Production LOC: <before> -> <after> (delta <signed difference>)
```

Report C1, C2 and telemetry subtotals separately, and the reference total separately while it
coexists. A docs/test-only commit reports the unchanged total and delta 0. Do not use
insertions/deletions, raw file lines or an uncorrected counter.

---

## 6. Verification commands (run individually; no aggregate gate)

From the repository root:

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
python3 -m unittest discover -s tools -p 'test_production_loc.py'
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
python3 tools/production_loc.py --files
git diff --check
```

Plus the focused targets for whichever packet you touched, for example:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content \
  --test edit_single --test edit_batch --test edit_transitions --test edit_noop \
  --test edit_model --test edit_bounds --test edit_localized --test edit_reference \
  --test edit_timing --test inode_leaf
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage \
  --test delta_payload --test delta_chains --test metadata_pool --test metadata_pool_index \
  --test metadata_chain --test metadata_window --test metadata_fingerprint_collision \
  --test physical_formats --test policy_capacity --test edit_pipeline
```

Do **not** run `tools/preflight.sh` and do not add an equivalent wrapper, workflow or CI job.
Report exactly which commands ran, which did not, and why. A zero-test discovery or an absent
target is not a PASS. Do not re-run unchanged resource-sensitive cases without a stated
reason, and do not run resource-sensitive work concurrently with another agent's measurement.
When clippy or a test run is fully cached, say so in the evidence rather than presenting a
cached result as a fresh check.

---

## 7. Completion gate table (the definition of done)

Copy this table into `stages-3-4-closeout-report.md` and fill the Status and Evidence columns.
**No row may be marked PASS without a named artifact.**

| # | Gate | Source | Status | Evidence |
| --- | --- | --- | --- | --- |
| G1 | CHUNK absent/ineligible candidate selects FULL; save never fails for an advisory candidate | W1 | | |
| G2 | Every pooled read applies the owner's captured pack ceiling | W2 | | |
| G3 | Emitted edit pages are encoded and hashed once, at final emission; superseded drafts released | W3 | | |
| G4 | Frontier memory is a function of height/fanout, asserted by a real oracle | W3, W4.1 | | |
| G5 | Finality argument committed; children-before-parents asserted for edits | W3, W4.6 | | |
| G6 | All nine sealed reference cases still match root, partition and survivors | W3 | | |
| G7 | Every overclaiming or vacuous oracle replaced; each new case fails without its fix | W4 | | |
| G8 | Integrated multi-edit chunked pipeline case exists and passes | W4.5 | | |
| G9 | Every significant allocation has owner, bound, multiplicity, lifetime and release event | W5, W7.3 | | |
| G10 | No size-proportional hidden collector remains on a real path | W5 | | |
| G11 | Phase-local heap ledger and a labelled RSS scope exist | W7 | | |
| G12 | #168's simultaneous index/codec/SQL memory gate has an input and a result | W7 | | |
| G13 | Matched reference campaign collected under a pre-committed addendum with aligned byte accounting | W8 | | |
| G14 | Every declared verification case is RUN or NOT_RUN with a reason | W8.4 | | |
| G15 | "Existing-or-better" for latency/storage/memory is resolved, or waived in writing | W8.6 | | |
| G16 | The clipped `e1c` receipt is re-collected or annotated wherever quoted | W8.7 | | |
| G17 | Acceptance documents match the source; per-file actual-size table exists | W9 | | |
| G18 | Counter defects fixed, same counter applied to both snapshots, combined total restated | W9.5 | | |
| G19 | Limits boundary list run or explicitly unrun with reasons | W10 | | |
| G20 | Every commit carries a first-parent `Production LOC:` line and this review's S1/S2 findings are closed | all | | |

**Stage verdicts are separate.** Stage 3 may only be marked complete when G1, G2, G9-G14,
G17-G19 are PASS (or waived). Stage 4 may only be marked complete when G3-G8, G13-G15,
G17-G19 are PASS (or waived). Close #168/#169 only when their own rows are PASS; keep #165
and Stages 5-7 open.

---

## 8. What you must not do

* Do not weaken validation, exact CAS comparison, candidate quality, backpressure,
  atomicity, visibility or cleanup to make a number smaller or a test greener.
* Do not add a retry, a fallback route, a second algorithm mode, a trial decoder, a RAW
  switch after an error, or an error-driven representation change.
* Do not add `fsync`/WAL/crash-durability work, or change the SQLite profile.
* Do not patch, vendor, fork or locally modify a third-party crate; do not add
  `[patch]`/`[replace]`; keep `--locked`.
* Do not add a dependency when std or an existing crate suffices.
* Do not add test-only hooks, counters, features or visibility to product `src`.
* Do not exceed 999 physical lines per production file or 200 per `lib.rs`/`mod.rs`; split by
  responsibility.
* Do not raise worker counts, timeouts, cache/buffer limits or budgets to pass a gate; do not
  drop a failing case, an INELIGIBLE row or a registered selection from a report.
* Do not claim a speedup from a smoke run, a matched root or an n=1 observation.
* Do not overwrite or re-label retained receipts, including superseded generations.
* Do not run `tools/preflight.sh` or add an aggregate gate.
* Do not close #168/#169 (or any issue) before §7's rows for it are PASS, and do not claim
  Workspace, FUSE, cloud, release or tag readiness.

---

## 9. Escalation format (use when, and only when, §0.3 applies)

```text
BLOCKER: <B1 owner decision | B2 dependency | B3 measurement lock>
Packet: <W-id>
Exact question or condition: <one sentence>
Evidence: <artifact path>
What I did instead while waiting: <packets completed>
What remains blocked: <specific rows of the gate table>
Next action on unblock: <one sentence>
```

Then continue with every packet that does not depend on the answer. A blocker entry is never a
completion and never closes a gate row.

---

## 10. Final deliverables

1. `stages-3-4-closeout-report.md` beside the handoff: the completed gate table, per-packet
   changes with file:line, commands and exits, test counts, evidence paths, every FAIL /
   INCOMPLETE / NOT_RUN, and the memory ledger with its coverage.
2. Fresh evidence directories under `docs/roadmap/0.1/0.1.7/evidence/` - never inside an
   existing generation.
3. The corrected acceptance documents of W9 and the corrected counter.
4. The per-commit production LOC lines in every commit.
5. Issue-ready summaries for #168 and #169 that state exactly which rows are PASS, which are
   waived and which are unrun.

When the table is fully PASS (or waived), say so plainly and close the two issues with the
evidence summary. Until then, keep working.
