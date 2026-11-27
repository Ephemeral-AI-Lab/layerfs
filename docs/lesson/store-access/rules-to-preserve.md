# Rules to preserve when optimizing the Store path

> **Status:** Lesson; distilled from merged work, informative and not a product contract.
>
> **Category:** store-access · **Applies when:** changing a Store read or write path for speed (batching, pipelining, caching, reordering) · **From:** [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810), [#815](https://github.com/Ephemeral-AI-Lab/layerfs/issues/815), [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816), [#821](https://github.com/Ephemeral-AI-Lab/layerfs/pull/821), [#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822)

## Rule

An optimization may change how many messages are sent and when, never what is
stored, what is guaranteed or how much memory it may use.

## What happened

#810 and #816 cut PostgreSQL Commit round trips by over an order of magnitude
(C4s4 26,740 → 203; [Round trips follow depth](round-trips-follow-depth.md)) by
reading earlier, grouping reads by level and folding Save writes into fenced
flights. Each move put a rule at risk: review after #820 found two Save ordering
regressions (#821); the flights left batch-sized buffers on every writer
connection (#822); and count pins went stale.

None of these rules is new; each links its owner. This lesson adds what each
means for a round-trip change, how #810/#816 checked it, and what bending breaks.

## How to spot a bent rule

- A reorder or new cache whose pull request shows trip counts but no root and
  Save comparison against the control arm.
- A batched path that runs on SQLite too, with no gate and no new SQLite pin.
- A new pipelined message with no drop and kill sweep, or a sweep that counts
  fewer server messages than the clean run sends.
- A buffer, cache or statement sized by the whole change or batch.
- A pin edited in a commit whose message does not say what moved it.

## The rules

### 1. Same bytes

Owner: [#810 "Also required"](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810),
[#816 Acceptance](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816); AGENTS.md has no byte rule.

- **Why.** Construction order may change; results may not. #816's plan moved
  file builds after each window's fetch; its stated guard was byte identity.
- **Check.** The canonical root and the Save's `stored_bytes`, `inserted`,
  `reused` and `packs` equal the control arm in every cell, on both backends.
  `commit_batched_reads.rs` pins `[inserted, reused, packs, stored_bytes]` and
  the root per scenario; `commit_batched_base_files.rs` checks roots equal the
  base on both backends. Every #816 result table states the match.
- **If bent.** A reorder can also move a dependency. #821 found a Save that
  published while its referenced file was still deferred (`MissingDependency`),
  and an over-budget request that took a pack before refusing.

### 2. SQLite path unchanged unless the change is meant for it

Owner: [Store invariants](../../architecture/engine/store/README.md#invariants),
[SQLite Commit reads](../../architecture/engine/store/sqlite.md#commit-reads-before-the-publication-810).

- **Why.** On SQLite a lookup is a function call; read-ahead only adds work.
- **Gate.** The provider says a lookup costs a round trip:
  `PackPersistence::batched_chain_reads` (`core/crates/layerfs-storage/src/port/persistence.rs`,
  default false, true for `PostgresStorage`) and `Mechanics::batches_locations`
  (`engine/crates/layerfs-persistence/src/backend/seam.rs`, true on an unfenced
  PostgreSQL transaction). `BoundWorkspace::construct_captured`
  (`layerfs-store-service/src/store/captured.rs`) reads the first.
- **Check.** #816 required SQLite statements unchanged per cell; #822 reports
  them identical. #810 F1/F4 changed SQLite on purpose (C4 6,138 → 1,725
  statements), pinned in `commit_batched_reads.rs` with identical roots.

### 3. No format change, server-side walk, second connection or lane change

Owner: the #810 scope decision (conservative only), repeated in #816
Constraints; [one database, one writer](../../../AGENTS.md#where-calls-run).

- **Why.** Each trades a round trip for a contract. A format change needs an
  upgrade step for every Store
  ([Format upgrade](../../architecture/engine/store/README.md#format-upgrade)).
  A server-side walk moves tree-sized work out of the shared Content code. #816
  kept "the one connection and producer": batching rides the existing writer or
  Workspace reader connection through `Session::pipeline`
  (`.../backend/postgresql/session/pipeline.rs`).
- **If bent.** A binding hint block's `seq` is `max(seq) + 1`, so two Commits
  would collide outside the lane; hints stay a lane write
  ([Commit round trips](../../architecture/engine/store/postgresql.md#commit-round-trips)).

### 4. One writer, short lane holds

Owner: [AGENTS.md, Where calls run](../../../AGENTS.md#where-calls-run).

- **Why.** Batching tempts you to move reads next to the write that needs them.
  Reads and construction stay before the lane; only head check, merge and
  publish run in it. In-lane work that grows with tree size is a defect.
- **Check.** The fused Commit is one message in the lane, with no head read
  before it ([Fused Commit](../../architecture/engine/store/postgresql.md#fused-commit)).
  Overflow binding batches are bounded by `HINT_BATCH_ROWS` per transaction.

### 5. One attempt

Owner: [AGENTS.md §4](../../../AGENTS.md#4-code-build-and-docs) and
[engine failure rules](../../../engine/AGENTS.md#failure-persistence-and-dependencies).

- **Why.** Each server message of a pipelined flight is a new place to lose a
  reply. No retry, refresh or replay; a lost reply quarantines the session.
- **What "nothing published" means.** A failed construction read ends the
  Commit before history publication, with the provider's original error.
  Earlier acknowledged Save batches stay in the Store
  ([construction failures](../../architecture/engine/commit/construction.md#base-directory-reads-810-f3)).
  A v2 C4s4 Commit cut off by the 25 s command cap in #816 step 0 left 53
  packs and 9,949 locations registered and the Branch at its old head.
- **The one sanctioned second path.** A fused Commit refused as a race (nothing
  written) continues once on the separate path, never after an unknown outcome.
- **Check.** Drop and kill every server message of every new or changed
  message: `layerfs-store-service/tests/commit_fault_*` (invariants in
  `support/fault_sweep.rs`; `_wide_{drop,kill}_{0..3}`, `_directories`,
  `_base_files_*`, `_race_*`) and persistence `postgresql_captured_save_fault.rs`.
  Passes: 202 Commit positions at `7e6caa86b2`, 101 + 101 on the grouped
  directory read, 34 + 34 on the captured Save flight.

### 6. Bounded memory

Owner: [AGENTS.md "No artificial caps"](../../../AGENTS.md#layerfs-mental-model)
and §4 ("memory hints are not evidence of resident bounds"); the +10% is #810's.

- **Why.** Fewer trips means bigger messages; no window or cache may be sized
  to the whole Commit.
- **The driver trap.** tokio-postgres keeps its request buffer and the
  connection's write buffer at their largest size for the connection's life.
  Before #822 a 4 MiB captured flight or a 16 MiB fused tail left about two
  batch-sized buffers per writer connection (B4 retained RSS 58,708 KiB on v2,
  72,992 at `db54e9d75f`, 57,556 after the split; one sample each). #822 split
  pack inserts into statements of at most `STATEMENT_PACK_BYTES`, inside the
  same flight and transaction (`pack_statements`, `.../postgresql/history/fused.rs`).
- **The named limits.** `PACK_LIMIT` 256 KiB, `TRANSACTION_ROW_LIMIT` 8,191
  (`core/crates/layerfs-storage/src/store/policy.rs`); `HINT_BATCH_ROWS` 4,096,
  `ARRAY_ROWS` 1,024 (`engine/crates/layerfs-persistence/src/history/hints.rs`).
  A new statement needs a named bound of its own.
- **Check.** Peak RSS at most +10% per cell against the control arm. #816's
  handback reported B1 at +10.36% instead of rounding it away; after #822's split it
  measured +2% (`262ef260be`, one sample).

### 7. Pinned counts

Owner: [AGENTS.md §4](../../../AGENTS.md#4-code-build-and-docs) ("never weaken a
surviving assertion").

- **Why.** Small cases are the common case. `mount_round_trips.rs` pins the
  second Commit of a mounted Workspace at 3 PostgreSQL round trips, the first at
  23, UpToDate at 2, plus mount and unmount; an empty plan must send nothing.
- **When a pin moves.** Update it in the commit that moves it, with the measured
  cause. `db54e9d75f` lowered writer trips in `commit_batched_reads.rs` (edits
  31 → 28, adds 34 → 29, one 18 → 17) and in `commit_similar_tiny_files.rs` but
  left both pins stale; the second surfaced only in the full Linux pass before
  merge. The fixes (`262ef260be`, `207b4f63cc`) name the mechanism and confirm
  the same saved objects and roots at the new counts.

### 8. Measurement rules for claims

Owner: [AGENTS.md Measurements](../../../AGENTS.md#measurements) and the
[measurement workflow §1](../../general/agent-measurement-policy.md#1-a-warm-cache-must-never-credit-a-measured-phase)
and [§3](../../general/agent-measurement-policy.md#3-running-a-measurement).

- One sample per arm (ABBA order in #816), cold state declared and enforced,
  `LAYERFS_CONSTRUCTION_WORKERS=1`, every test command under 120 s. The variable
  selects nothing in engine source (one producer holds by
  [construction](../../architecture/engine/commit/construction.md)); export it
  so the receipt states it.
- Compare counts only under the same cache state
  ([State the cache behind a cap](../measurement/state-the-cache-behind-a-cap.md)),
  by role (writer, readers) and phase.

## Limits

- These rules bound an optimization, not its target. Trip and time caps belong
  to the issue, and the owner may waive them (#816 waived B1, C4s4, C2, B5e, C1).
- Rule 1 covers read-path and ordering changes. A change meant to alter storage
  (#810 F2, F6) reports raw byte and chain-depth deltas instead.
- Not exhaustive: durability, reopen custody and first-winner comparison keep
  their own contracts.

## Pre-merge checklist

1. Root and `stored_bytes`, `inserted`, `reused`, `packs` equal the control in every cell, both backends.
2. SQLite statements unchanged cell for cell, or new SQLite pins with identical roots; batched path behind the provider flag.
3. No format change, server-side walk, extra connection or lane change.
4. Nothing new inside the lane; in-lane work does not grow with tree size.
5. Drop and kill sweeps at every server message of each new or changed message; failed reads publish no history.
6. Every window and statement has a named row or byte bound; peak RSS ≤ +10% per cell.
7. Small-Commit, mount and unmount pins unchanged, or moved with the measured cause in the commit message.
8. One sample per arm, cold state declared, `LAYERFS_CONSTRUCTION_WORKERS=1`, commands under 120 s.

## Evidence

- Scope: [#810](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810) ("Scope decision",
  "Also required"); [#816](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816) (Constraints, Acceptance).
- 202 fault positions after #821: [#810 comment](https://github.com/Ephemeral-AI-Lab/layerfs/issues/810#issuecomment-7185859079).
- #816 comments: [step 0 custody](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7186732914),
  [34 + 34 sweep](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7187186832),
  [101 + 101 sweep](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7188922029),
  [B1 RSS +10.36%](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7198507181).
- Buffers, memory table, SQLite identity, stale pins: [#822](https://github.com/Ephemeral-AI-Lab/layerfs/pull/822),
  [#816 results at merge](https://github.com/Ephemeral-AI-Lab/layerfs/issues/816#issuecomment-7202528418).
- Related: [Round trips follow depth](round-trips-follow-depth.md), [Costly patterns](costly-patterns.md),
  [State the cache behind a cap](../measurement/state-the-cache-behind-a-cap.md).
