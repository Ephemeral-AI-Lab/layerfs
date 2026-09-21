# Source citations — Squad C cadence (#219)

Every claim in `README.md`, `linkage-answer.md`, `commit-decomposition.txt` and
`pre-registration.md` resolves to one of these, to a receipt field, or to a query in
`sqlite-queries-2.sql` / `pack-rewrite-estimate-2.sql`. Quotes are verbatim; line
numbers are from the tree at `9c46930b846600e5f3c6ca4a4c4cbcf44ecdc356` (worktree
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch `codex/219-ns10000`).

## The cadence unit

- `core/crates/layerfs-storage/src/cas/lifecycle.rs:160-178` — `maybe_commit`, the only
  commit site in the step path. `161` guards on `self.transaction_open`; `162`
  `let started = Instant::now()`; `163-167`:
  > "Multi-writer: SQLite admits one writer per store file, and the other writer must
  > not have to wait for this one's whole upload. A write transaction therefore never
  > outlives the step that opened it under the arbitration lock, so every step commits
  > before that lock is released. Batching stays inside a step; it cannot span steps."
  `168` `advance_pack_if_moved`; `169` `write::commit`; `170` charge `commit_ns`;
  `174` clear `transaction_open`; `175` `self.counters.commits += 1`.
- `cas/lifecycle.rs:129-139` — `begin_write`: `130` `write::begin_immediate`; `131`
  `transaction_open = true`; `132` reset `TransactionState`; `133`
  `ownership::next_pack` (the watermark read); `137` `counters.transactions += 1`.
  **No `SaveProfile` charge appears in this function.**
- `cas/placement.rs:111-200` — `seal_group`, the step that frames and places one group.
  `117-119` early return when the lane's group is empty; `120` `mem::take`;
  `121-123` charge `group_ns` around `build_group`; `131-135` take the arbitration lock
  and `begin_write` when the transaction is closed; `136-139` charge `place_ns` around
  `select_many`; `137` `self.placement[index].select_many(lane, vec![group], ...)`
  (**one group per call**); `161` `self.write_pack(write)`; `178-180` charge `sql_ns`
  around `insert_objects`; `199` `self.maybe_commit()` — still inside the guard taken
  at `132`.
- `cas/pool_lane.rs:110-123` — the ordinal reservation sub-step: `112` take the lock;
  `115-117` `begin_write` when closed; `118-121` `reserve_ordinals`; `122`
  `self.maybe_commit()?`.
- `cas/pool_lane.rs:258-356` — `write_value_groups`: `270`
  `fresh.chunks(crate::policy::VALUES_PER_GROUP)`; `288-292` take the lock and
  `begin_write` when closed; `294-296` `select_many(lane, encoded, ...)` (**a vector of
  groups**); `299-323` one `write_pack` per returned write plus one `insert_group` per
  placed group; `355` `self.maybe_commit()`.
- `cas/lifecycle.rs:180-202` — `flush_candidates`, the wave-level step: `182` take the
  lock; `187` `let opened = !self.transaction_open`; `188-190` `begin_write`;
  `191-195` `candidates.flush`; `196-200` roll back and return when
  `written == 0 && opened` (**a step that opens a transaction and places nothing does
  not commit**); `201` `self.maybe_commit()`.
- `cas/lifecycle.rs:226-272` — `finish_inner`: `228-230` seal every lane;
  `233-235` `begin_write` when closed; `239-247` charge `sql_ns` around the content-index
  flush; `253` `ownership::publish`; `254` `advance_pack_if_moved`; `255-257` charge
  `commit_ns` around `write::commit` only; `258` `counters.commits += 1`.
- `cas/lifecycle.rs:95-99` — a fresh owner starts at `transactions: 1, commits: 1`,
  the slot-acquisition transaction taken by `cas/lifecycle.rs:39`
  (`ownership::acquire`).
- `sqlite/ownership.rs:103-136` — `acquire`: `104` `write::begin_immediate`; `133`
  `write::commit` — the transaction behind the owner's initial `commits: 1`.
- `cas/save.rs:112` — `flush_batch` ends with `owner.flush_candidates()`; `cas/save.rs:22-25`
  returns early for an empty wave.
- `cas/store.rs:500-509` — `SaveOperation::accept` pushes into the pending batch and
  calls `self.flush(drained)` when the batch drains; `531-539` `flush` →
  `save::flush_batch`; `542-549` `finish` drains the remainder, flushes it, then
  `owner.finish()`.
- `cas/batch.rs:48-69` — the wave bounds: `51-52`
  `self.objects.len() >= self.object_limit || self.canonical_bytes.saturating_add(length) > self.byte_limit`.
- `policy.rs:105` `BATCH_OBJECT_LIMIT = 512`; `policy.rs:107`
  `BATCH_CANONICAL_BYTES_LIMIT = 512 * 1024`; `policy.rs:382-385` binds them into
  `StorageCapacities { batch_objects, batch_bytes, transaction_rows, transaction_bytes }`.
- `policy.rs:93` `GROUP_TARGET = 48 * 1024`; `policy.rs:85` `GROUP_LIMIT = 65_536`;
  `policy.rs:95` `PACK_LIMIT = 256 * 1024`; `policy.rs:97` `GROUP_COUNT_LIMIT = 256`;
  `policy.rs:135` `VALUES_PER_GROUP = 165`; `policy.rs:139`
  `POOLED_LEAF_ROWS_LIMIT = 100`; `policy.rs:109/111` `TRANSACTION_ROW_LIMIT = 8_191`,
  `TRANSACTION_CANONICAL_BYTES_LIMIT = 4 * 1024 * 1024 - 1`.
- `cas/selection.rs:56-79` — the seal decision: `57-58` `WholeFile | PooledMetadata |
  Singleton => occupied` (one record per group); `59-69` `Ordinary | Native` project
  `framed_group_length(...) > GROUP_TARGET`; `71-79` the additional
  `capacities.batch_bytes` bound. `cas/selection.rs:97-102` seals immediately for
  `WholeFile | PooledMetadata | Singleton`.
- `cas/placement.rs:154-160` — the only enforced transaction bound in the step:
  `pending.members.len() as u64 + 4 > self.capacities.transaction_rows` →
  `CapacityExceeded`. **`capacities.transaction_bytes` is declared at
  `policy.rs:352` and never read anywhere in the crate** (grep of
  `core/crates/layerfs-storage/src`, 2026-09-21).
- `pack/assemble.rs:21-22` — `frame_group(records) = frame_group_bounded(records, GROUP_LIMIT)`;
  `pack/assemble.rs:111-131` — `build_group`: `117-120` for `WholeFile`
  `if records.len() != 1 { return Err(StorageError::Integrity("compact group record count")) }`
  and `122-124` refuses `bytes.len() <= WHOLE_FILE_COMPACT_DROP`; the body is stored
  `GroupCodec::Raw` with no internal framing. `132-137` `Native` frames
  `frame_group(records)` (multi-record). `144-156` `PooledMetadata` also requires
  `records.len() == 1`.
- `pack/layout.rs:114-117` — `directory_is_starts_only` is true only for `WholeFile`;
  `pack/layout.rs:204-219` `append_fits` (group count < `group_count_limit`, assembled
  total ≤ `pack_limit`); `pack/layout.rs:107-112` `pack_limit`; `119-137` `body_limit`
  and `group_count_limit`.
- `encoding/full.rs:217-225` — `lane_body_limit`: `WholeFile => capacities.pack_limit`,
  `Native | Ordinary => capacities.group_limit`.
- `pack/placement.rs:71-80` — `select_many`, with `73-74`:
  > "Every pack that receives a group in this call produces exactly one write,
  > assembled once, after the last group that landed in it."
  `83-117` the append-or-new decision and `group_number = open.groups.len()`;
  `148-153` the retained open tail.
- `cas/placement.rs:202-245` — `write_pack`, with `203-207`:
  > "Writing a pack moves every body in it (the directory grows), so every cache that
  > holds those bytes is invalidated whenever this save writes"
  `208` `self.pool_reader.release_packs()`; `217` `self.pack_cache.remove(&write.pack_id)`;
  `218-224` `insert_pack`/`append_pack` charged to `sql_ns`; `236-241` the
  `pack_ceiling` update on creation.
- `sqlite/write.rs:68-77` `insert_pack` (`INSERT INTO object_packs`); `80-89`
  `append_pack` (`UPDATE object_packs SET data = ?2 ...`) — the whole body is bound;
  `138-164` `insert_objects` (one statement per chunk, `115-129` the chunk size).
- `encoding/delta/candidates.rs:344-382` — `flush`, with `355-357`
  `if self.stamp == self.flushed { return Ok(0) } `; `313-342` `insert` (the only writer
  of `stamp`); `59` `SLOTS = 8_192`.
- `encoding/delta/select.rs:297-303, 332-337, 358-360, 393-398` — `candidates.insert`
  is called **only** under `if lane == PackLane::WholeFile`.
- `sqlite/connection.rs:32-48` — the connection profile: `journal_mode = MEMORY`
  (`36-39`), `synchronous = OFF` and `temp_store = MEMORY` (`40`),
  `foreign_keys = ON` (`41`), `busy_timeout(Duration::ZERO)` (`46`).
- `sqlite/ownership.rs:167-176` `advance_pack`; `178-196` `reserve_ordinals`;
  `138-157` `publish`.

## What a cadence change may not break

- `core/crates/layerfs-storage/tests/pack_watermark.rs:42-72` —
  `a_step_never_leaves_the_watermark_behind_a_pack_it_wrote`: for **every** accepted
  object it reads `next_pack_id` through an independent connection and asserts
  `mark > highest` committed pack id (`59-62`), then asserts the published watermark is
  ahead of every pack row (`67-70`). `pack_watermark.rs:74-116` —
  `two_writers_interleaved_between_steps_never_share_a_pack_identifier`.
- `cas/lifecycle.rs:141-150` — the doc that binds the watermark to the step boundary:
  "deferring it to publication is exactly the change that would let a second writer
  collide, and `tests/pack_watermark.rs` fails on it".
- `tests/visibility.rs:109-160` — early bounded commits must not publish
  (`133-136` asserts the save committed packs early, `140-146` asserts the unrelated
  reader gets `Unpublished`); `tests/visibility.rs:342-394` — a pooled value group
  committed above the publication watermark is refused with
  `StorageError::VisibilityCeiling`.
- `tests/cas_reuse.rs:51-54` and `tests/persistence_failure.rs:260-263` —
  `assert_eq!(commits, 2, "slot acquisition and publication, no payload writes")`.
  The two fixed commits are pinned by tests.
- `tests/multi_writer.rs:6-51` — two private saves over one Store overlap, publish in
  reverse order, and both must read back byte-identically.
- `tests/write_admission.rs:1-19, 285-300` and
  `docs/roadmap/0.1/0.1.7/concurrency-controls.md:23-82` — the #216 writer budget:
  default 2 (`30-31`), `1..=64` (`33-34`), "It is not one SQLite transaction and not
  one local FUSE `write()` callback; short database transactions still serialize inside
  the Store" (`36-40`).
- `docs/roadmap/0.1/0.1.7/issue-commit-time-rca-handoff.md:67-70` — the measured
  cadence-widening failure: "`COMMIT_EVERY` = 8, 64 and 100000 all **fail in round 0**
  with `CleanupFailed { original: OwnershipUnavailable, cleanup: OwnershipUnavailable }`";
  `:222-223` — "**Do not serialise writers, widen the step, or batch across steps.**
  Measured: the wider step fails, it does not wait."
- `docs/roadmap/0.1/0.1.7/issue-commit-time-rca-handoff.md:51-58` — the mechanism this
  evidence prices: "`commit_ns` is the pack body's pages at the write syscall's price.
  `SQLITE_DBSTATUS_CACHE_WRITE` equals the payload's page count **exactly** ... 4.18 GiB
  of pack body is 1,095,642 pages, and at the measured 1.65–1.72 µs per 4 KiB `pwrite`
  that is the whole bucket." (`:60-65`: "**Do not re-test the profile.**")

## Provenance of the cadence (correction to a handoff premise)

- `git show -s --format=%H%n%ad%n%s eb319aaa9` →
  `eb319aaa9ef196358035e6af86066e51b2bdf826`, 2026-09-21 02:56:40 +0800,
  "feat(core): land the multi-writer storage model with the bridge upload optimization".
- `git log -S "never outlives the step" -- core/crates/layerfs-storage/src/cas/lifecycle.rs`
  → `eb319aaa9`.
- `git show 7075f338^:core/crates/layerfs-storage/src/cas/lifecycle.rs | grep -n ...`
  already prints `151: fn advance_pack_if_moved`, `160: pub fn maybe_commit`,
  `165: // transaction therefore never outlives the step that opened it under the`,
  `168: self.advance_pack_if_moved()?` — the step-scoped commit **predates** #216.
- `git show --stat 7075f338db36b209b59031e3e55ae11cf87eed57` touches
  `sql/schema.sql`, `cas/store.rs`, `policy.rs`, `sqlite/lookup.rs`,
  `sqlite/ownership.rs`, `sqlite/schema.rs`, `tests/cas_roundtrip.rs`,
  `tests/write_admission.rs` — **not** `cas/lifecycle.rs`, `cas/placement.rs` or
  `cas/pool_lane.rs`.
