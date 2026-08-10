# Ephemeral AI FS implementation plan

| Field | Value |
| --- | --- |
| Status | Planned |
| Target | Version 0.1 integration candidate |
| Delivery style | Milestone exits with objective acceptance evidence |
| Database foundation | SQLite remains authoritative |

## 1. Execution rules

Milestones are ordered by dependency. Work inside one milestone may proceed in
parallel, but the next milestone does not become the integration baseline until
the current milestone's acceptance criteria pass.

Each milestone exit records:

- the exact commit;
- completed checklist items;
- commands and environments used;
- machine-readable correctness and benchmark results;
- known deviations with an owner and follow-up milestone; and
- confirmation that the working tree and generated package artifacts are
  reproducible from a clean checkout.

Correctness, durability, integrity, and resource ceilings are hard gates.
Latency cannot waive them. Tests are finite and iteration-based:

- the mandatory smoke profile is capped at 60 seconds per target;
- the default correctness and benchmark selection should finish within
  10 minutes per target;
- an optional `load-10m` profile is hard-capped at 10 minutes; and
- 10 GiB logical manifests and millions-of-rows jobs are extended, non-gating
  diagnostics.

## 2. Dependency map

```text
M0 repository and test foundation
  -> M1 CAS, CDC, COW, patches, and manifests
      -> M2 SQLite storage and Node driver
          -> M3 filesystem namespace and I/O
              -> M4 branches and publication
                  -> M5 maintenance and recovery
                      -> M6 Cloudflare SQLite parity
                          -> M7 Node VFS
                          -> M8 replication
                              -> M9 release candidate
                                  -> M10 Computer integration
```

Milestones 7 and 8 may proceed in parallel after milestones 0 through 6 pass.

## 3. Milestone 0: Repository and test foundation

### M0 objective

Create a buildable monorepo whose package and dependency boundaries prevent
later implementations from bypassing the architecture.

### M0 checklist

- [ ] Create the package workspace and lockfile.
- [ ] Add `packages/fs` as `@ephemeralai/fs`.
- [ ] Add `packages/sqlite-node` as `@ephemeralai/fs-sqlite-node`.
- [ ] Add `packages/sqlite-cloudflare` as
      `@ephemeralai/fs-sqlite-cloudflare`.
- [ ] Add `packages/node-vfs` as `@ephemeralai/fs-node-vfs`.
- [ ] Add `packages/replication` as `@ephemeralai/fs-replication`.
- [ ] Add `packages/testkit` as `@ephemeralai/fs-testkit`.
- [ ] Add shared TypeScript, lint, formatting, build, and test configuration.
- [ ] Add clean-package and packed-tarball test fixtures.
- [ ] Add architecture checks for cycles and forbidden imports.
- [ ] Add API extraction or an equivalent export-snapshot check.
- [ ] Configure the `@ephemeralai/fs` export map with only the root,
      `sqlite-driver`, and two integration bridge subpaths.
- [ ] Add deterministic fixture generation with recorded seeds and SHA-256
      digests.
- [ ] Add Node and Durable Object testkit factory contracts.
- [ ] Add fault-controller, restart, read-only, and second-connection hooks.
- [ ] Add machine-readable correctness and benchmark result schemas.
- [ ] Add CI jobs for documentation, architecture, unit, package, and testkit
      harness checks.

### M0 acceptance criteria

- [ ] A clean install builds every empty package without unpublished local
      state.
- [ ] The dependency graph has zero cycles and matches the allowed direction.
- [ ] Packed consumers can import only documented export paths.
- [ ] Deep imports of CAS, CDC, COW, manifests, repositories, schema, and
      transactions fail.
- [ ] The testkit can create, label, seed, and dispose a driver fixture through
      a recording implementation.
- [ ] Fixture generation is byte-identical across two clean runs.
- [ ] Documentation lint, link validation, build, and architecture checks pass.

## 4. Milestone 1: Content algorithms and binary formats

### M1 objective

Implement the pure, host-independent CAS, CDC, COW, patch, and segmented
manifest algorithms before adding persistence.

### M1 checklist

- [ ] Implement checked integer and byte-view utilities.
- [ ] Implement branded CAS object and manifest identifiers.
- [ ] Implement incremental SHA-256 hashing and object verification.
- [ ] Implement the exact `fastcdc-v1` Gear table and boundary algorithm.
- [ ] Implement resumable streaming FastCDC state.
- [ ] Implement COW page-size, page-key, dirty-range, and page-overlay math.
- [ ] Implement ordered insertion, deletion, replacement, and truncation
      patches.
- [ ] Implement the manifest root-envelope codec.
- [ ] Implement leaf and internal manifest-node codecs.
- [ ] Implement deterministic content-defined manifest grouping.
- [ ] Implement bounded root-to-leaf lookup and sequential manifest cursors.
- [ ] Implement canonical manifest building from a CAS-entry stream.
- [ ] Implement local FastCDC and manifest-tree reconnection.
- [ ] Add golden vectors for CAS, FastCDC, root envelopes, leaves, internal
      nodes, grouping, and complete manifest roots.
- [ ] Add property tests for byte coverage, determinism, overflow, malformed
      encodings, and full-rebuild equivalence.

### M1 acceptance criteria

- [ ] All golden vectors match on supported Node and workerd runtimes.
- [ ] FastCDC results are independent of input buffer partitioning.
- [ ] Identical bytes and parameters produce identical manifest roots.
- [ ] Start, middle, end, and EOF lookup scan no more than one leaf after
      traversing the bounded tree path.
- [ ] Corrupt root, node, span, count, or child data is rejected before affected
      bytes are returned.
- [ ] Local insertion, deletion, truncation, and overwrite produce exactly the
      full-rebuild manifest root.
- [ ] Unchanged CAS objects and manifest subtrees are reused after deterministic
      reconnection.
- [ ] COW and patch cases pass at 4, 8, and 16 KiB page sizes.
- [ ] Pure algorithms import no SQLite, branch, host, RPC, or FUSE module.

## 5. Milestone 2: Transactional SQLite storage and Node driver

### M2 objective

Persist the content engine, revisions, staging, quotas, and schema through one
transaction-only SQLite contract and a production-capable Node driver.

### M2 checklist

- [ ] Implement `FilesystemSQLiteDriver` and callback-scoped transaction values.
- [ ] Invalidate transaction values immediately after callback completion.
- [ ] Implement the private SQLite unit of work.
- [ ] Implement bounded statement, binding, BLOB, and result handling.
- [ ] Implement schema identity, version metadata, migrations, and fixtures.
- [ ] Implement CAS object, manifest-root, and manifest-node relations.
- [ ] Implement namespace-head and immutable revision relations.
- [ ] Implement lease, staged-membership, and closure-certificate relations.
- [ ] Implement immutable COW page versions and mutable page heads.
- [ ] Implement structural-patch and insertion-segment relations.
- [ ] Implement exact `efs_usage` count and byte deltas.
- [ ] Implement root-change journal and maintenance reserve accounting.
- [ ] Implement bounded repositories requiring an active transaction.
- [ ] Implement batched CAS insertion and collision verification.
- [ ] Implement staged closure sealing and constant-row final validation.
- [ ] Implement the file-backed Node SQLite driver.
- [ ] Configure foreign keys, acknowledged durability, busy timeout, WAL,
      checkpointing, 16 MiB cache target, and zero-byte mmap default.
- [ ] Enforce and report physical database and journal ceilings.
- [ ] Add initialization, reopen, read-only, second-connection, migration,
      corruption, and statement-fault tests.

### M2 acceptance criteria

- [ ] The driver exposes no connection-level SQL execution.
- [ ] A transaction value used after its callback fails before issuing SQL.
- [ ] Failure after every statement leaves the complete old or complete new
      state after reopen.
- [ ] Schema creation and every migration are deterministic and restart-safe.
- [ ] CAS deduplication retains one verified payload and detects corruption.
- [ ] Manifest roots and nodes round-trip and traverse in bounded batches.
- [ ] A staged file with more than 100,000 CAS entries finalizes from one sealed
      certificate without rescanning every membership row.
- [ ] Concurrent quota races cannot exceed payload, metadata, database, or
      journal ceilings.
- [ ] `efs_usage` matches bounded direct recalculation after commit, rollback,
      replacement, expiry, and collection setup.
- [ ] The Node driver passes the milestone's shared storage suite.

## 6. Milestone 3: Filesystem namespace, revisions, and I/O

### M3 objective

Deliver the public filesystem facade with complete namespace semantics,
revision commits, bounded range I/O, and snapshot streams on Node SQLite.

### M3 checklist

- [ ] Implement absolute POSIX path parsing and UTF-8 validation.
- [ ] Implement inode and directory-entry resolution.
- [ ] Implement `readFile`, `readRange`, and `readStream`.
- [ ] Implement `writeFile`, `writeRange`, `replaceRange`, and `truncate`.
- [ ] Implement `mkdir`, `readdir`, `stat`, `lstat`, and `chmod`.
- [ ] Implement symbolic links, hard links, link counts, and final-link rules.
- [ ] Implement atomic rename, unlink, and recursive removal.
- [ ] Implement timestamps using one nondecreasing clock sample per mutation.
- [ ] Implement immutable revision deltas, checkpoints, and head projection.
- [ ] Implement snapshot leases and bounded stream backpressure.
- [ ] Implement shared resident-memory admission and byte-weighted caches.
- [ ] Implement stable error codes, precedence, lifecycle, and idempotent close.
- [ ] Implement capabilities, limits, observations, and operation counters.
- [ ] Add filesystem conformance and fault-injection suites.

### M3 acceptance criteria

- [ ] All portable namespace and I/O conformance cases pass on Node SQLite.
- [ ] Links, rename, timestamps, revision history, and UTF-8 ordering survive
      reopen.
- [ ] Every mutation is atomic under statement-level fault injection.
- [ ] Reads return exact selected bytes and create no durable content state
      except a required bounded lease.
- [ ] A cold range lookup does not enumerate the complete manifest.
- [ ] Increasing a streamed fixture from 100 MiB to 1 GiB adds no more than one
      output chunk, one maximum CAS object, and one manifest node to managed
      memory high-water.
- [ ] Cancellation, failure, and close release every lease and reservation.
- [ ] The 60-second Node SQLite smoke profile passes.

## 7. Milestone 4: Branches and publication

### M4 objective

Deliver durable private branches, deterministic conflicts, atomic publication,
and exact replay without copying whole workspaces.

### M4 checklist

- [ ] Implement branch creation, open, info, discard, and handle close.
- [ ] Implement branch namespace overlays and durable base expectations.
- [ ] Implement immutable page versions with atomic page-head replacement.
- [ ] Implement ordered structural patches and branch materialization.
- [ ] Implement branch generation and write-set tracking.
- [ ] Implement entry, inode, subtree, and ancestor conflict tokens.
- [ ] Implement hard-link alias and parent-timestamp conflict behavior.
- [ ] Implement deterministic exact changed paths and conflict records.
- [ ] Implement operation-ID reservation and branch-generation binding.
- [ ] Implement publication preparation and sealed staging.
- [ ] Implement the constant-bounded final publication transaction.
- [ ] Implement merged, conflict, replay, expired-result, and mismatch outcomes.
- [ ] Implement terminal metadata and result retention.
- [ ] Add branch lifecycle, conflict, replay, restart, and fault suites.

### M4 acceptance criteria

- [ ] Fifty independent writers publish into one valid parent chain.
- [ ] Fifty same-inode writers produce one merge and 49 explicit conflicts.
- [ ] A conflict changes neither main nor the active branch overlay.
- [ ] Lost-response replay returns the original result after physical restart.
- [ ] One operation ID can never publish another branch or generation.
- [ ] Independent sibling publications succeed in either order with exact
      parent timestamps.
- [ ] Hard-link aliases preserve inode identity and conflict as one node.
- [ ] A branch stream retains its original bytes across later edit,
      materialization, publication or discard, collection, and restart.
- [ ] Repeated same-page writes retain one current page plus only explicitly
      leased predecessor versions.

## 8. Milestone 5: Maintenance, recovery, and bounded scale

### M5 objective

Make long-lived databases self-verifying and reclaimable without unbounded
transactions, memory, WAL retention, or process-local indexes.

### M5 checklist

- [ ] Implement bounded storage snapshots with high-water capture and
      reconciliation.
- [ ] Implement bounded integrity verification with resumable cursors.
- [ ] Implement root enumeration for main, revisions, branches, results,
      leases, staging, checkpoints, and holds.
- [ ] Implement durable manifest-tree and CAS mark traversal.
- [ ] Implement root-change reconciliation without restarting completed work.
- [ ] Implement bounded sweep, overlay pruning, and revision pruning.
- [ ] Implement interrupted-run resume and abandoned-run cleanup.
- [ ] Implement lease renewal, expiry, release, and cleanup races.
- [ ] Implement root-journal compaction and emergency maintenance reserve.
- [ ] Implement metadata, database-page, and WAL-pressure behavior.
- [ ] Add 100,000-row cursor, accounting, verification, and collection cases.
- [ ] Add optional 10 GiB logical-manifest and millions-of-rows diagnostics.

### M5 acceptance criteria

- [ ] Collection never deletes any value reachable from a required root.
- [ ] Reachable corruption stops the sweep and reports integrity failure.
- [ ] Mark and sweep resume after every injected interruption.
- [ ] Root additions reconcile without discarding completed mark work.
- [ ] Bounded storage accounting holds no database-wide read transaction.
- [ ] Managed memory does not grow with the mandatory 100,000-row fixture.
- [ ] Metadata-only and blocked-checkpoint workloads enforce finite ceilings.
- [ ] Quota failure leaves usage counters exact and maintenance able to progress.
- [ ] All maintenance operations expose bounded progress and stable metrics.

## 9. Milestone 6: Cloudflare Durable Object SQLite parity

### M6 objective

Run the same filesystem engine and portable conformance outcomes on Durable
Object SQLite without importing DOFS or reimplementing filesystem logic.

### M6 checklist

- [ ] Implement the Cloudflare SQLite driver over Durable Object SQLite.
- [ ] Map callback-scoped transactions to the runtime transaction facility.
- [ ] Normalize rows, BLOBs, safe integers, constraints, busy errors, and
      corruption errors.
- [ ] Report conservative BLOB, binding, physical quota, journal, durability,
      and runtime-memory capabilities.
- [ ] Implement runtime restart and eviction test hooks.
- [ ] Add production-like preview deployment fixtures.
- [ ] Run storage, filesystem, branch, maintenance, recovery, and resource
      suites through the shared testkit.
- [ ] Add the 60-second Durable Object SQLite smoke profile.

### M6 acceptance criteria

- [ ] Every mandatory portable test from milestones 1 through 5 passes on both
      Node and Durable Object SQLite.
- [ ] Adapter-specific setup does not change public results or error codes.
- [ ] Runtime restart reconstructs all state from committed SQLite data.
- [ ] The driver never uses an in-memory SQLite mirror or filesystem index.
- [ ] The driver reports finite conservative resource capabilities.
- [ ] The 60-second production-like Durable Object smoke profile passes.

## 10. Milestone 7: Node VFS and real FUSE readiness

### M7 objective

Provide the synchronous Node filesystem surface needed by Computer while
keeping FUSE and process ownership outside Ephemeral AI FS.

### M7 checklist

- [ ] Implement the supported Node VFS integration bridge in the core.
- [ ] Implement pinned read sessions and bounded manifest cursors.
- [ ] Implement `readIntoSync` without an equal-sized intermediate allocation.
- [ ] Implement writable file sessions and read-after-write visibility.
- [ ] Implement provider-wide per-inode monotonic write admission.
- [ ] Implement bounded pooled slab ownership and transfer.
- [ ] Implement `stagePrefixSync` for hidden durable staging.
- [ ] Implement `commitVisibleSync` for flush and fsync durability.
- [ ] Implement provider sync, close, retry, abort, and error translation.
- [ ] Implement shared backpressure across 1, 16, and 64 sessions.
- [ ] Add real-FUSE test fixtures without adding FUSE to the core package.
- [ ] Add the 60-second real-FUSE smoke profile.

### M7 acceptance criteria

- [ ] Repeated reads on one handle reuse a pinned selection and return exact
      bytes.
- [ ] Three sessions on one inode pass every commit order without lost updates.
- [ ] Hidden staging never satisfies fsync or advances visible state.
- [ ] Successful commit, close, restart, unmount, and remount preserve digest.
- [ ] Large reads and writes allocate no whole-file buffer.
- [ ] Sixty-four sessions remain inside pending-write and aggregate memory
      limits with backpressure.
- [ ] The real-FUSE smoke profile completes within 60 seconds.
- [ ] Computer needs only handle forwarding and no filesystem semantics.

## 11. Milestone 8: Replication

### M8 objective

Replicate revisions, branches, manifests, CAS objects, and results through a
bounded host-neutral protocol without exposing tables or raw content mutation.

### M8 checklist

- [ ] Implement the schema-free replication integration bridge in the core.
- [ ] Implement protocol capabilities, roles, and compatibility handshake.
- [ ] Implement authenticated session, cursor, nonce, receipt, and retry state.
- [ ] Implement bounded manifest-root, manifest-node, and CAS negotiation.
- [ ] Implement incremental envelopes with no complete duplicate buffer.
- [ ] Implement checkpoint bootstrap and main catch-up.
- [ ] Implement branch push and pull.
- [ ] Implement durable export and staging leases.
- [ ] Implement staging-certificate updates and constant-row final activation.
- [ ] Implement dropped-response replay and retry exhaustion.
- [ ] Implement policy checks before durable session creation.
- [ ] Implement bounded abandoned-session cleanup.
- [ ] Add Node-to-Node, Node-to-Durable-Object, and restart suites.

### M8 acceptance criteria

- [ ] A one-byte edit transfers only its root envelope, changed manifest nodes,
      missing CAS objects, revision metadata, and protocol overhead.
- [ ] Already-present content adds no duplicate CAS payload.
- [ ] Dropping a response in every phase resumes without duplicate activation.
- [ ] Peak buffers remain within the negotiated and shared runtime limits.
- [ ] The receiver never retains the complete missing-object or manifest graph.
- [ ] Retry exhaustion releases or expires all leases and reservations.
- [ ] The bridge exposes no SQL, schema, repository, standalone CAS insertion,
      or standalone COW mutation.
- [ ] Replicated bytes pass digest verification through Node VFS.

## 12. Milestone 9: Version 0.1 integration candidate

### M9 objective

Turn the implementation into reproducible packages with complete correctness,
resource, performance, migration, and compatibility evidence.

### M9 checklist

- [ ] Run the complete mandatory correctness matrix on both SQLite drivers.
- [ ] Run architecture, packed-export, migration, corruption, and fault suites.
- [ ] Run the 60-second smoke profile on Node, Durable Object, and real FUSE.
- [ ] Run B01 through B09 from the release benchmark plan.
- [ ] Compare common workloads with explicitly selected isolated DOFS.
- [ ] Verify the 80% bounded-range throughput comparison gate.
- [ ] Verify the 1.10-times materialization comparison gate.
- [ ] Verify the 10% accepted-baseline regression policy.
- [ ] Verify the 128 MiB managed-memory default and smaller-budget behavior.
- [ ] Run the optional `load-10m` profile when rollout resources permit.
- [ ] Produce package provenance, changelog, migration guide, and known limits.
- [ ] Pack and install every package in clean consumer fixtures.
- [ ] Publish a versioned unstable or release-candidate package set.

### M9 acceptance criteria

- [ ] Every normative `MUST` and `MUST NOT` maps to passing evidence.
- [ ] There are zero digest mismatches, partial commits, lost updates, unsafe
      collections, leaked reservations, or usage-counter mismatches.
- [ ] All mandatory gates complete without an elapsed-time soak.
- [ ] Default correctness and benchmarks finish within the documented bounded
      profile on the reference targets.
- [ ] Performance and resource result artifacts are checked in and reproducible.
- [ ] Public exports match the approved API snapshot.
- [ ] A clean consumer can open, use, close, reopen, and verify both drivers.

## 13. Milestone 10: Ephemeral AI Computer integration

### M10 objective

Make Ephemeral AI FS the default Computer filesystem while retaining DOFS only
as an explicitly selected, isolated benchmark comparison.

### M10 checklist

- [ ] Publish the exact package versions selected for Computer integration.
- [ ] Provide the engine factory and capability-selection example.
- [ ] Wire authoritative Durable Object SQLite through the Cloudflare driver.
- [ ] Wire the local Computer database through the Node driver and Node VFS.
- [ ] Wire authenticated sync transport to the replication endpoint.
- [ ] Map Computer-owned FUSE handles to Node file sessions.
- [ ] Route `workspace.fs`, shell, Git, tools, push, and pull through the common
      filesystem contract.
- [ ] Make omitted engine configuration select Ephemeral AI FS.
- [ ] Keep DOFS behind an explicit comparison selector and isolated database.
- [ ] Replace table-inspecting tests with public behavior and maintenance
      results.
- [ ] Run the complete Computer integration path and smoke profile.
- [ ] Document new-workspace, preview, rollback, and legacy-data policy.

### M10 acceptance criteria

- [ ] The full path passes:

  ```text
  workspace.fs
    -> authenticated replication
    -> computerd
    -> real FUSE
    -> shell and Git
    -> pull
    -> branch publication
    -> restart and reconnect
    -> collection and verification
  ```

- [ ] Computer's default path contains transport, execution, workspace, and
      user-facing integration logic but no filesystem persistence semantics.
- [ ] The Ephemeral AI FS wiring remains within 100 net-new Computer production
      lines, excluding tests, benchmark harnesses, generated code, and the
      optional DOFS comparison adapter.
- [ ] Omitted configuration selects Ephemeral AI FS; DOFS never becomes an
      automatic fallback.
- [ ] Ephemeral AI FS and DOFS schemas, diagnostics, and databases remain
      isolated.
- [ ] Durable Object restart, container restart, FUSE remount, branch reconnect,
      and final integrity verification pass.
- [ ] The 60-second end-to-end smoke profile passes.

## 14. Milestone status template

Use this block when recording a milestone exit:

```md
### Milestone N exit

- Commit:
- Date:
- Checklist complete: yes/no
- Correctness artifact:
- Benchmark artifact:
- Smoke duration and operation counts:
- Resource high-water:
- Known deviations:
- Approved to begin next milestone: yes/no
```
