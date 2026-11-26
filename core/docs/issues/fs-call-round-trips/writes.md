# fs call round trips: Writes track

> **Status:** Proposal; target LayerFS 2.0.0; not a released contract. Part of [SPEC.md](SPEC.md). Source pinned at `056e920166`; all after-values are estimates until measured. The corrections at the end override earlier text.

**Scope:** the SDK write calls `fs.write`, `fs.edit`, `fs.mkdir`, `fs.cp`, `fs.mv`, `fs.rm` (including recursive) and `fs.apply`. Each one commits a single change set to a Branch with no mounted Workspace.

- Source: `056e920166`.
- Paths are relative to `engine/crates/layerfs-store-service/src/` unless they name another crate.
- Every citation below was read at that commit; findings marked *inferred* are not yet confirmed by a measurement.

## Verified facts

1. **Publication is not the cost.**
   - `files::commit` calls `commit_captured_via` (`write/files.rs:123-130`), which is eligible for the fused one-message Commit (`store/commit_fused.rs:149-161`, `store/commit.rs:459-474`), because the SDK passes `expected: None` (`write/files.rs:128`).
   - A warm Commit over an unmoved head costs the membership locate plus the message: 2 trips (`tests/mount_round_trips.rs:268-276`).
2. **Every call binds a fresh private Workspace and closes it.**
   - Bind: `files.bind` → `StoreService::bind` (`store_service.rs:195`) → `bind_private` (`store/bind.rs:82-90`).
   - On PostgreSQL the bind reads the snapshot on the writer (`store/bind.rs:129`; enabled by `bootstrap.rs:327`) and registers the mount intent in one message that also returns the node's markers (`store/bind.rs:177`).
   - The Workspace reader comes from the Store's idle readers and its `KeptReads` are cleared on release (`store/read_handle.rs:250`), so every call starts with a cold reader cache.
   - Release: `StoreService::release` (`store_service.rs:230`).
3. **The fixed part.** Trips are counted on PostgreSQL; how they are sent is verified, and the sum is inferred.

   | Step | Trips | Where |
   |---|---:|---|
   | Project lookup (`args::project`) | 1 | `write/files.rs:80` |
   | Branch by name (`args::branch` → `branch_named`, a `self.read` read transaction: BEGIN+fence, statement, guarded COMMIT) | 3 | `write/files.rs:89`; persistence `history/catalog.rs:102` |
   | Bind: writer snapshot + admission message (markers included) | 2 | `store/bind.rs:129,177` |
   | Hidden-path setup, read eagerly before any change: `Visibility::for_stack` → `path_removals` (`self.read`, 3) and `node_serial` (1) | 4 | `write/files.rs:243-248`; persistence `history/catalog.rs:344-350` |
   | `TreeTable::open` of the base root on the uncached control reader | 1–2 | `write/files.rs:249-250` |
   | `logical.known`: Commit-row read; the private Workspace's memo is always empty | 1 | `store/statistics.rs:22-34` (read at `:31`), called from `store/commit.rs:350` |
   | Membership locate + fused message | 2 | `tests/mount_round_trips.rs:268-276` |
   | Close: reader close + intent release | 1 | `store_service.rs:230` |
   | **Total** | **15–16** | |

   *Correction to the earlier analysis:* `files.resolve` costs 4 trips, not 2, because `branch_named` is a 3-trip read transaction. The hidden-path setup costs 5–6, not 4.
4. **The markers are already in memory.**
   - The private bind keeps the bound node's markers when it holds a mount admission (`gc_op`): see `materialize_removals_with` → `keep_removals` (`removal.rs:476-487`, `:531-541`) and `KnownRemovals` (`removal.rs:125`, bound `KEPT_REMOVAL_BYTES` = 256 KiB at `removal.rs:143`).
   - Their serial is the node's `history_serial` (`NodeRemovals = Option<(u64, Vec<PathRemovalRecord>)>`, `removal.rs:108`).
   - So `write/files.rs:243-250` re-reads facts the bind already holds. They are needed only on a `NoSuchPath` from `edit` or the `cp` source (`write/files.rs:251-257`, used at `:267-270` and `:277-282`).
5. **Recursive `rm` and `cp` stat each child one at a time** (`write/recursive.rs:86`, `:178`, `:208`).
   - `view.stat` reads portable metadata, plus the file length for a regular file (`layerfs-workspace/src/base/view.rs:243-257`, length at `:251`).
   - `remove` needs only the kind.
   - Measured: `rm` of a 100-file dir sends 106 single-object locates.
6. **Serials and prepares.**
   - A create takes serials with `reserve_pooled(64)` (`write/mutation.rs:141`). When the Store's pool is empty, that becomes one catalog reservation of `POOL_BLOCK` = 4,096 (`store/ports.rs:253-268`), in a write transaction (persistence `history/allocation.rs:30`).
   - The SDK Store service never calls `prepare_mounts` (`bootstrap.rs:115-127`), so each statement is prepared at its first use.
   - In the measured run, `write.new_file` was the first create in its process: 1 connect, 12 prepares, 7 transactions. Overwrites have 0 prepares and 4 transactions.
7. **Not attributed:** the overwrite remainder (24 − ~15 ≈ 8–9 trips: path walk, base inode values, chain reads) and the create cost (mkdir 45 / cp 44 vs overwrite 24: +20, with 6 transactions vs 4). W0 must attribute these before W4, W6 and W7 are sized.

## Targets

> SPEC §4 recomputes these rows with each case's own measured cost per trip and the cold-pin rules; where the two differ, SPEC §4 governs.

Columns are trips / local ms / +10 ms time.
- "Now" is measured: `056e920166`, one sample, local PostgreSQL 17 through Docker's forwarder.
- The later columns are **estimates**. Projected time = projected SQLite ms + trips × 0.69 ms locally, plus trips × 11.65 ms at +10 ms (both medians measured on this run).
- "R1 + W1–W3" assumes the Reads track's R1 (Branch-by-name as one statement) has landed.
- "All" adds W4–W7. Its values depend on W0 and are marked ~.

| Case (PG trips / local ms / +10 ms) | Now | R1 + W1–W3 | All (estimate) | SQLite stmts (ms) now → after |
|---|---|---|---|---|
| `write` overwrite (depth 3) | 24 / 22 / 298 ms | 14 / 13 / 176 ms | ~11 / ~11 / ~139 ms | 54 → ~47 (3.9 → 3.4) |
| `edit` (depth 2) | 23 / 23 / 284 ms | 13 / 13 / 164 ms | ~11 / ~11 / ~139 ms | 52 → ~45 (4.3 → 3.8) |
| `write` new file (first create per Store open) | 66 / 49 / 809 ms | 56 / 43 / 696 ms | ~39 / ~31 / ~486 ms | 77 → ~70 (5.0 → 4.5) |
| `mkdir /newdir` | 45 / 30 / 548 ms | 35 / 28 / 436 ms | ~25 / ~21 / ~313 ms | 69 → ~62 (4.5 → 4.1) |
| `cp` file | 44 / 29 / 548 ms | 34 / 28 / 424 ms | ~24 / ~21 / ~301 ms | 64 → ~57 (5.0 → 4.4) |
| `mv` file | 40 / 23 / 487 ms | 30 / 25 / 374 ms | ~22 / ~19 / ~275 ms | 60 → ~53 (4.4 → 3.9) |
| `rm` file | 37 / 23 / 453 ms | 27 / 23 / 337 ms | ~20 / ~18 / ~251 ms | 58 → ~51 (4.6 → 4.1) |
| `apply` 10 new files | 46 / 38 / 562 ms | 36 / 32 / 451 ms | ~26 / ~25 / ~328 ms | 74 → ~67 (7.9 → 7.2) |
| `rm` 100-file dir, recursive | 151 / 106 / 1.87 s | 141 / 105 / 1.75 s | ~35 / ~32 / ~440 ms | 179 → ~80 (18.2 → 8.1) |

**How "R1 + W1–W3" is computed:**
- R1: −2. `branch_named` goes from 3 trips to 1; verified mechanism.
- W1: −5. −4 is verified (`path_removals` 3 + `node_serial` 1); −1 is the inferred `TreeTable::open`.
- W2: −2.
- W3: −1, on sequential writes to one Branch.

The per-trip times above use the local PostgreSQL of this run (0.34 ms `SELECT 1` through Docker's forwarder). The +10 ms column assumes the measured 11.65 ms per trip.

**Floor per write call, cold reader: 5 + d trips.**
- Named snapshot: 1.
- Admission message: 1. It must precede any root demand (`store/bind.rs:146-149`).
- One planned demand per level of the parent path: d. A level's ids are known only after the level above is read.
- Membership locate: 1. It needs the constructed ids.
- Fused message: 1. It asserts the bound head inside its own transaction, so no head read is needed before the lane.
- Close: 1. It runs after the outcome is known.

3 + d would need either of these owner calls:
- pipeline the snapshot with an admission insert that asserts the head on the server;
- delete the mount intent inside the fused transaction (this changes intent custody for an unknown outcome).

## Phases

Every phase must hold these invariants:
- the publication lane and acknowledgement order unchanged;
- custody for unknown outcomes: `Unknown` keeps the Workspace (`write/files.rs:116-118`, `:170`);
- one attempt and no retry; `one_call` reopens a quarantined session at most once;
- refusal codes and order unchanged: `ReadOnly` (main) before any Branch read; `NoSuchProject`, `NoSuchBranch`, `HeadMoved` (`expected`), `Conflict` (moved head with overlapping paths, `write/files.rs:176-203`), `Removed` vs `NoSuchPath` (`write/files.rs:229-236`);
- every new or changed message gets a drop sweep and a kill sweep, as in `tests/commit_fault_wide_*`;
- the batched paths (W4, W6, W7) only where the provider reports a round-trip cost (`batches_locations`, `batched_chain_reads`). W1–W3 and W5 are logic changes on both backends; their SQLite pins are updated in the same commit, subject to an owner decision.

**W0 — Attribute the write trips (no product change).**
- Add per-phase `SqlWork` to the probe through `telemetry::phase_sql` (`telemetry.rs:80`), split by writer, control reader and Workspace reader.
- Phases: `files.resolve`, `files.bind`, `files.change`, `commit`/construct/membership/publish, `files.close`.
- Run `write.overwrite`, `write.new_file`, `mkdir` and `cp` once each: cold (fresh process, first create) and warm (after one create).
- Exit criterion: every trip of overwrite, new file and mkdir is named by phase and statement template. The figures for W4, W6 and W7 are re-estimated from that run before any of them starts.

**W1 — Build the hidden-path check only when it is needed.**
- Change: replace the eager `Visibility::for_stack` + `node_serial` + `TreeTable::open` (`write/files.rs:243-250`) with a lazy builder, called only on the refusal path (`write/files.rs:251-257`). It builds `Visibility` from the bound Workspace's `KnownRemovals` (new accessor in `removal.rs`).
- Fallback: when the markers were not kept (`charge > KEPT_REMOVAL_BYTES`, no `gc_op`, or a poisoned slot), use today's reads.
- Saves 4 trips verified (+1 inferred) on every write; 0 extra on the refusal path.
- SQLite: −4 to −5 statements.
- Memory: none new; it reuses the kept ≤256 KiB markers.
- Invariant: the `KnownRemovals` doc (`removal.rs:110-124`) guarantees the bound node's markers cannot change while its admission exists.
- Tests:
  - `tests/sdk_path_removal.rs` still answers `Removed` vs `NoSuchPath`;
  - new case: markers over 256 KiB still refuse correctly through the fallback.
- Exit criterion: `write.overwrite` −5 on PostgreSQL, with the refusal tests unchanged.

**W2 — Bind by name.**
- Change: `write/files.rs:79-100` calls `bind_named` with `pooled = true` (a private named variant of `store/bind.rs:48-67`). The project, Branch and snapshot come back in one writer statement (`branch_snapshot_named`).
- Refusal order: keep today's `main` handling exactly. For `branch == "main"`, run the project lookup and then `ReadOnly`, as now, so `NoSuchProject` still comes before `ReadOnly`.
- `expected` needs the `LayerStackRecord` (`write/files.rs:101-114`). Use the stack the named snapshot returns, or keep today's path when `expected` is given.
- Saves 2 after R1 (from 3 trips to 1).
- SQLite: −2 statements; the SQLite named bind already exists.
- Message changes: none new. It reuses the existing named-snapshot statement, which already has drop and kill sweeps from #768. Add one sweep for the private variant.
- Tests: `tests/method_refusal_order.rs` (write cases); `ReadOnly`, `NoSuchProject` and `NoSuchBranch` messages unchanged.

**W3 — Remember the logical size of the last published node.**
- Change: a Store-level slot `(NodeId, u64)` in `store/open.rs`. Set it when a fused Commit publishes, alongside `remember_logical`. `known_logical` (`store/statistics.rs:22-34`) checks it before reading.
- Safe because a node's recorded `logical` never changes (`store/statistics.rs:17-21`), and the slot is checked against the exact bound node.
- Saves 1 on sequential writes to one Branch; 0 on the first write.
- Alternative (option b): carry `logical` in the bind snapshot. That changes the `BranchSnapshot` contract (`layerfs-history`) and its SQL, so it is an owner decision.
- Memory: 16 bytes.
- SQLite: −1 statement.

**W4 — Prepare the create path once per Store open (PostgreSQL only).**
- Change: an SDK-side prepare flight, like `prepare_mounts` (`bootstrap.rs:115-127`, persistence `postgresql/history/mount.rs:156-170`), holding the statements W0 names as first-used by a create (12 measured). Send the serial reservation (`history/allocation.rs:30`) as one pipelined message: BEGIN, lane row, `UPDATE … RETURNING`, COMMIT.
- Saves an estimated ~12 + ~3 on the first create per Store open, pending W0.
- Cost: the prepares move to Store open, about 1 trip. That trade is right for a persistent SDK process. A process-per-call front end (#819) pays it every call either way, so pair this with #819.
- Exit criterion: the second `write.new_file` in a process equals mkdir ±2.

**W5 — Read the kinds of a listed page in one demand (both backends).**
- Change: in `write/recursive.rs:86` (remove) and `:178`/`:208` (copy), replace the per-child `view.stat` with a page-level kinds lookup. Add `BaseView::values(&[serial])` (wrapping `layerfs_content` `lookup_many`) to `layerfs-workspace/src/base/view.rs`, consult overlay-local inodes first, and read no metadata or lengths for `remove`. `copy_one` keeps its own value read, from the same batch.
- Window: one listed page (the existing `list` page bound).
- Saves an estimated ~100 on a 100-file dir (106 locates → about 2–6 demands).
- SQLite: about −100 statements; less work, same result.
- Memory: one page of values, about 100 × `InodeValue`.
- Tests: recursive `rm`/`cp` with mixed files, directories and symlinks, a whiteout inside, and a page boundary of over 1,000 entries.

**W6 — Plan the parent-path reads (PostgreSQL batched path).**
- Change: the change set already names every path (`write/files.rs:33-61`, `:260-299`). Before applying it, collect the distinct parent prefixes and read each level's directory pages and child inode values in one demand per level (`compare/tree.rs` `values`/`lookup_many`), in windows of at most 128 directories / 1 MiB. Hand the results to the mutation's `resolve` (`write/mutation.rs:180-203`) and to the Save's base reads through the call's `ReadCache`.
- `fs.apply` has no cap on the number of changes, so plan in windows of `PLAN_PATHS` = 1,024 paths (a new named bound) and process change by change after that.
- Saves an estimated 2–6 per cold call (more for `apply` across directories), pending W0.
- `resolve`'s per-component `view.stat` (metadata + length, `layerfs-workspace/src/workspace/view.rs:92-141`) becomes a kind-only lookup for intermediate components.

**W7 — Join the membership locate with the base chain (only if W0 shows sequential waits).**
- Use `locate_for_save` with packs (costly pattern 8) for the membership and base-chain demands.
- The lesson records this as a measured miss in Commit construction, so build it only if W0 shows a locate immediately followed by a dependent read of what it returned.
- Saves an estimated 0–2.

## Layout

Production LOC from `tools/production_loc.py --files` at `056e920166`. Only the files this track changes are listed, so the bracketed directory totals cover those files alone. Tests live in `engine/crates/layerfs-store-service/tests/`: the new pins in `sdk_write_round_trips.rs` (the probe `fs_call_round_trips.rs` stays a measurement, not a gate), `sdk_path_removal.rs` and `method_refusal_order.rs`, plus new fault sweeps for W2 and W4.

```diff
  engine/crates/layerfs-store-service/src/      [2458]  (files touched by this track only)
    store_service.rs                               203   (W2: private named bind entry; lane W owns it)
    bootstrap.rs                                   291   (W4: SDK prepare flight)
    removal.rs                                     400   (W1: kept-markers accessor)
    store/                                       [683]
      bind.rs                                      326   (W2: pooled named variant)
      open.rs                                      323   (W3: last-published logical slot)
      statistics.rs                                 34   (W3: check the Store slot)
    write/                                       [881]
      files.rs                                     274   (W1 lazy check; W2 named bind; W6 plan)
      mutation.rs                                  311   (W6: planned resolve)
      recursive.rs                                 296   (W5: page kinds)
+     plan.rs                                        —   cond. (W6 path plan, if files.rs passes 999)
```

```diff
  engine/crates/layerfs-workspace/src/          [529]   (files touched by this track only)
    base/
      view.rs                                      335   (W5: values for a page)
    workspace/
      view.rs                                      194   (W6: kind-only lookup)
```

```diff
  engine/crates/layerfs-persistence/src/        [673]   (files touched by this track only)
    backend/postgresql/history/
      mount.rs                                     278   (W4: prepare list)
    backend/postgresql/storage/
      provider.rs                                  332   (W4: prepare_sdk_writes)
    history/
      allocation.rs                                 63   (W4: reservation as one message)
```

## Owner decisions

1. Do W1, W2, W3 and W5 apply to SQLite too? They are logic changes with fewer statements and the same results, and the SQLite pins would be updated. The lesson's rule keeps the *batched* paths PostgreSQL-only, which W4, W6 and W7 follow.
2. W3: a Store memo (no contract change) or `logical` in the bind snapshot (changes the `BranchSnapshot` contract).
3. W4: move the create statements' prepares to Store open. This adds about 1 trip to every SDK Store open.
4. The 3 + d floor: pipelined snapshot+admission, and/or intent release inside the fused transaction. Both change messages or custody, and neither is proposed by default.
5. `PLAN_PATHS` (1,024) as a new named bound for W6.

## Corrections from the code review (2026-11-27)

These override the text above where they differ.

- **W1 refusal change (SPEC D16).** Today a failed marker read (`read/visibility.rs:90`) refuses every write. After W1 it refuses only on the `NoSuchPath` path. This needs owner approval as a named order change; otherwise keep the eager read when the kept markers are absent. W1 uses the Phase 0 seam `Visibility::from_markers`, plus a `removal.rs` accessor that checks `gc_op`, matches the bound node and maps `None` to (0, no markers).
- **W2 refusal order.** The `branch` argument is an `Arg` refused after the project lookup (`write/files.rs:80-81`). Keep the project lookup whenever that argument is an error, not only for `main`. Map `BindError::NoSuchProject`/`NoSuchBranch` back to today's refusal text (`history/lookup.rs:287-300`); today `store_service.rs:221-224` turns them into strings.
- **W4** runs in M3, after lane RH's H4 merge, because:
  - the reservation is dispatched from persistence `history/catalog.rs:317-318`;
  - the new `allocation_reserve.sql` sits in `sql/postgresql/queries/history/`;
  - re-preparing after a reopen follows the `mount_writes` flag pattern (`backend/postgresql/history/mount.rs:156-160`) in `backend/postgresql/session/` (lane W).

  The one-message form still checks scope authority and `Capacity` on the server.
- **W5** replaces `SourceView::stat` (`write/recursive.rs:84-86`, `layerfs-workspace/src/workspace/view.rs:92`). It wraps the existing `lookup_many`, whose signature lane C keeps.
- **W6/W7** hand their reads to the Save's base reads through `store/commit.rs` and `store/pipeline.rs` (lane W). `store/commit.rs` is at 919 physical lines, so new code goes into `write/plan.rs`.
- **Pin file:** `sdk_write_round_trips.rs`. The 999 ceiling counts physical lines.
