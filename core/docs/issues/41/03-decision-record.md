# Decision Record — R1, Q1, C3

**Scope**: the three reconciliation decisions of Issue #41 that change what the
implementation must do. **Authority**: [#511](https://github.com/Ephemeral-AI-Lab/layerfs/issues/511)
(release contract) and [#41](https://github.com/Ephemeral-AI-Lab/layerfs-project/issues/41).
**Baseline**: `layerfs_v2_0_0`.

| Id | Decision | Result (recommended default) | Where it lives |
|---|---|---|---|
| R1 | How PostgreSQL replaces the Store during `store.compact` | **E** — two `ALTER SCHEMA RENAME` in one transaction (default); **A'** only if a read-available SLO is required | §1.3 R1, §7.3, S11 |
| Q1 | May a Branch name be reused after `branch.remove` | **A** — no reuse: terminal tombstone, `NameRetired`, no migration | §1.2 Q1, C6 |
| C3 | Signature of a per-object delete | **`delete(id, generation)`** — the intent is bound to `(ObjectId, G)` | §5 C3 |

---

## 0. How these decisions were made

Same four steps for all three. The step that actually settles the decision is
named for each one in §4.

1. **Fix the requirement ladder.** Every requirement relevant to the decision is
   placed on one level; a lower level never overrides a higher one.
2. **Hard-gate eliminate on L1.** Any option that can violate a correctness or
   safety property is removed, no matter how cheap or simple it is. This step is
   decided by one question, not by a score.
3. **Find the single binding requirement.** Among the survivors, the level that
   actually discriminates is identified. Most design arguments are wasted on
   levels where every candidate scores the same.
4. **Cost against benefit, and keep an exit.** The cheapest option that clears
   the gate wins; the rejected option is recorded with the condition under which
   it becomes the right one.

**Requirement levels**

| Level | Name | Question it answers | Decides? |
|---|---|---|---|
| **L1** | Correctness / safety | Can this lose or corrupt retained data? | Hard gate — always decides |
| **L2** | Resource / performance | Time, lock window, storage overhead, migration cost | Decides when L1 ties |
| **L3** | Availability | Are reads/writes usable during the operation? | Decides when L1 ties and a SLO exists |
| **L4** | Maintainability & usability | Code complexity, debuggability, user-visible semantics | Tiebreaker only |

---

## 1. R1 — PostgreSQL Store replacement

### 1.1 Problem scenarios

- **Not atomic → two schemas answering at once.** If the new tables are created
  next to the old ones without a single switch, a client that opens between the
  two statements reads a half-migrated Store. Two clients can then write to
  different tables and both report success.
- **Rename sequence interrupted → `layerfs` does not exist.** With
  `ALTER SCHEMA layerfs RENAME TO layerfs_c_<n>_old` committed and the second
  rename not, every connection fails on schema resolution.
- **Reads blocked for the whole window.** Any exclusive lock held across the copy
  makes unrelated readers wait for the copy, not for the switch.
- **Writes landing in the retired schema.** A writer holding a session on the old
  schema after the switch writes into a schema nobody reads. This is the
  PostgreSQL analogue of C8b's pre-swap writer, and it is why the reader/session
  fence (C8 / R2) is required whatever swap is chosen.

### 1.2 Alternatives (§7.3)

| Option | Mechanism | Why it can be rejected |
|---|---|---|
| **E** (default) | `BEGIN; ALTER SCHEMA layerfs RENAME TO layerfs_c_<n>_old; ALTER SCHEMA layerfs_c_<n> RENAME TO layerfs; COMMIT;` then `DROP SCHEMA …_old` | Reads unavailable for the switch window |
| **A'** | Data in versioned schema `layerfs_c_<n>`; a pointer row in the fixed schema; `self.schema` resolved from the pointer at open; swap = one `UPDATE` + later `DROP SCHEMA` | Costs a second placeholder at open, a pointer that must be recovered, and multiple live schemas |
| **B** | Paired `ALTER TABLE RENAME` in one transaction | Every table takes `ACCESS EXCLUSIVE`; needs a measured `lock_timeout`; N tables, N lock points |
| **C** | View indirection | Writability needs triggers; last resort |

### 1.3 Requirement ladder

| Level | Requirement | E | A' | B | C |
|---|---|---|---|---|---|
| L1 | Atomic switch; retained data complete | ✓ transaction | ✓ one `UPDATE` | ✓ transaction | ✓ |
| L1 | Reader/session fence (C8) present | required | required | required | required |
| L2 | Lock held only for the switch, not the copy | ✓ | ✓ | per-table | ✓ |
| L3 | Reads usable during compact | ✗ window | ✓ | ✗ | ✓ |
| L4 | Recoverable without a new state machine | ✓ | pointer must be recovered | ✓ | triggers to maintain |
| L4 | Understandable and maintainable | ✓ | 3 moving parts | N tables | triggers |

### 1.3a Mechanism in the code at `layerfs_v2_0_0`

What a PostgreSQL Store actually depends on, and therefore what a swap must not break:

| Fact | Where | Consequence for R1 |
|---|---|---|
| A Store is one named schema, `[a-z_][a-z0-9_]*`, at most 63 bytes, **required per call** (no default) | `control/crates/layerfs-store/src/postgresql/open.rs:37` `schema_of` | The name is supplied twice over: by the call and by the Store record |
| The record keeps the name: `Kind::Postgresql { schema, synchronous_commit }` | `control/crates/layerfs-store/src/identity.rs:29,136,161` | A swap that *changes* the Store's name rewrites every Store record |
| Every transaction pins the name in text: read `BEGIN … REPEATABLE READ READ ONLY; SET LOCAL search_path TO <schema>`, write `BEGIN … READ COMMITTED; SET LOCAL search_path TO <schema>; SET LOCAL synchronous_commit TO …` | `.../session/profile.rs:40,50` | An open session resolves table names through `search_path`, and its `search_path` is the schema *name* |
| Six history statements additionally qualify it: `sql.replace("{s}", self.schema)` | `.../mechanics.rs:19`, `Transaction.schema` at `.../session/transaction.rs:47` | Those statements follow `self.schema`, not `search_path`; both must keep resolving |
| A schema is created by one script: `CREATE SCHEMA {name}; SET LOCAL search_path TO {name}; {SCHEMA}{ACQUISITION}` — 19 tables, `SCHEMA_VERSION = 2` written to `store_policy` and `history_meta` | `.../schema.rs:101,116`, `TABLES`/`SCHEMA_VERSION` at `.../schema.rs:40,52` | The replacement schema is built by the same path, so it carries the same `schema_source` and `schema_definition` |
| Writers serialize on one row: `store_lane … FOR UPDATE` on its own connection, `lock_timeout_ms` (default 30 s) reaching it fails `Busy`, never retried | `.../lane/lane.rs:22` `hold_lane`, `.../store/config.rs:62` | The swap has a single place to wait for in-flight writers, and a bounded wait |
| Open verifies `catalog_incarnation` against the admitted authority | `.../schema.rs:129` `Checked`, `diagnostic/postgresql.rs:291` | The generation, not the name, is what tells a session that the bytes behind the name changed |

**Correction to the spec text.** §7.3 says `self.schema` comes from `schema_of` "passed by the caller or defaulting to `layerfs`". At this revision there is no default: `schema_of(None)` refuses `InvalidInput` ("postgresql.schema is required"). The conclusion is unchanged and in fact stronger — the name is always explicit, always recorded, and always the same text after a swap.

**Why the name must end where it started.** R1-E's two renames leave `layerfs` occupied by the new tables. Everything that remembered the name (the Store record, the call's `postgresql.schema`, each open session's `search_path`, the six `{s}` statements) keeps resolving, and nothing has to be rewritten. R1-A' changes that: the fixed schema would hold a pointer row and `self.schema` would be resolved at open, so the *recorded* name no longer identifies the data.

**Why the fence is on the generation, not the name.** Re-pointing a name is exactly what MVCC does not protect: a reader that opened before the swap holds a snapshot, and `SET LOCAL search_path TO layerfs` resolves to *different relations* after the commit. That is why C8 bumps `catalog_incarnation` at install and why every session checks `(catalog_id, incarnation)` — the name is allowed to stay stable precisely because the generation is what refuses stale sessions (`StaleStore`). E is therefore only sound together with the exclusive window (C10/D3) and the reader fence; A' needs the same fence for a different reason, which is why §7.3 says it is required whatever swap is chosen.

**What E costs, in the same terms.** Any session still open when the swap commits is reading through a name that now designates new relations; it must be drained or refused, and it cannot be allowed to complete a write. That is the read-unavailable window S11 prices, and it is a property of the *name*, not of the transaction.

### 1.4 Reasoning, and the basis for the decision

The old design chose "versioned table names + pointer" because *"the schema name
is fixed, so it cannot be renamed"*. **That reason is void.** `schema_of`
(`open.rs:37`) accepts the schema from the caller, and after the two renames the
name is `layerfs` again — every Store record and every `{s}` placeholder stays
valid. So the name is not a constraint at all.

With the false constraint removed, L1 ties between E and A', and L2 ties as well
once the lock is held only across the switch. **The only discriminating level is
L3**, and it is governed by one variable, S11:

- **S11 = exclusive window (default)** → **E**. Reads are unavailable for a
  window measured in the switch, not the copy. Cheapest correct design.
- **S11 = read-available SLO (p99 ≤ 1.2× baseline)** → **A'**. Only A' keeps reads
  usable; E cannot be tuned into it.

**Decision basis**: a single gating requirement variable (S11 read-availability),
not a technical impossibility. E is chosen *because S11 is exclusive*, and the
record names A' as the option to take if S11 flips. B and C never compete: B
multiplies lock points per table, C moves writability into triggers.

**Cost accepted**: a read-unavailable window per compact, in exchange for one
transaction, no pointer state to recover, and no second placeholder.

---

## 2. Q1 — Branch name reuse

### 2.1 Problem scenarios

- **Two Branches, one `BranchId`.** `BranchId::named = blake3(stack ‖ name)`
  (`contract/identity.rs:266-278`). A recreated same-name Branch reproduces the
  id, and `CommitId::derive` (`contract/identity.rs:287-304`) hashes
  `meta.branch`, so old and new history collide on both ids.
- **Old Commits re-attributed.** Every historical Commit that carries that
  `BranchId` now resolves to the new Branch — history silently changes owner.
- **External references become ambiguous.** Anything that stored the id (audit,
  monitoring, a published Layer's `source_branch_id`) points at whichever Branch
  holds the name now.

### 2.2 Alternatives (§1.2 Q1, C6)

| Option | Mechanism | Cost |
|---|---|---|
| **A** (chosen) | No reuse: terminal tombstone, same-name create returns `NameRetired` | One terminal marker + a check in create; **zero migration** |
| **B** | `BranchId::named_v2(stack, name, generation)`; `generation` in the derivation | Identity version bump, a `generation` column, migration of existing rows, every `BranchId` computation touched |

### 2.3 Requirement ladder

| Level | Requirement | A | B |
|---|---|---|---|
| L1 | One `BranchId` identifies one Branch, forever | ✓ the name can never be handed out twice | ✓ generation disambiguates |
| L1 | Historical `BranchId` keeps its owner | ✓ | ✓ |
| L2 | No migration of existing Stores | ✓ zero | ✗ format change |
| L2 | Name available again for a new Branch | ✗ | ✓ |
| L4 | User-visible semantics | ✓ "removed names stay retired" | needs explaining |
| L4 | Extension cost for future features | none | every new identity consumer inherits `generation` |

### 2.4 Reasoning, and the basis for the decision

L1 does not discriminate: A protects the identity invariant by construction, B by
adding a dimension. **The discriminating level is L2, and it is settled by a hard
constraint, not a preference**: selecting B changes the persisted identity format
and migrates every existing Store, while the benefit is a rare convenience.

Two further facts point the same way:

- The code already asserts the premise: `contract/identity.rs:266-268` states that
  Branch names are never reused. A keeps the existing premise; B rewrites it.
- With A, the guard is local — a terminal marker and one check at create. With B,
  every consumer of `BranchId` must handle generations.

**Decision basis**: constraint dominance on L2 (zero migration) on top of a tied
L1, and consistency with an invariant already documented in the code.

**Cost accepted**: a removed name is permanently unavailable; a user picks another
name. **Upgrade path**: if name reuse ever becomes a requirement, B is the
recorded option, and it is a format change taken deliberately — not a default.

**Cross-reference — C3 (§3.6).** Option B is C3's mechanism applied to a name.
Q1 and C3 are the same family of failure (an identifier outliving its referent)
resolved in opposite directions, and each constrains the other: Q1 = A is what
keeps `branch.remove(name)` safely name-only, and Q1 = B would drag C3's
signature into the resource calls. Sign-off for C3 is requested on #42
(comment 6818138560); read the two requests together.

---

## 3. C3 — Per-object delete signature

### 3.1 What the generation in `(ObjectId, G)` is, and is not

`G` is **the Store generation** the intent was formed at — `rootGeneration` from
C1, the same `catalog_incarnation` C8 bumps at install. It is **not** a
per-object version column: C8 forbids a second generation field, and the
contract says the *intent* carries `G` (§5 C3). Nothing is added to an object
row.

Why a version is needed at all is a property of how object ids are made:
`ObjectId::for_bytes = blake3(OBJECT_DOMAIN ‖ canonical)`
(`core/crates/layerfs-content/src/object/id.rs:22-29`). Identical bytes therefore
produce the identical `ObjectId`, always. Recurrence of an id is not an allocator
accident that could be switched off — it follows from content addressing.

### 3.2 Problem scenarios

- **ABA deletion.** Garbage is found unreachable at generation `G`; the intent is
  queued. Before it runs, a Commit references those same bytes again, so the same
  `ObjectId` is reachable once more. An intent keyed on the id alone deletes a
  now-reachable object: retained data disappears with no error.
- **Resumed intent after a restart.** The op record is replayed after a crash. If
  it does not carry the generation it was formed at, it cannot tell "still the
  same Store state" from "the Store has moved on and this object is live again".
- **Unattributable loss.** When a delete is reported as a no-op, nothing in the
  record says which generation the intent was formed on, so the loss cannot be
  reconstructed afterwards.

### 3.3 Alternatives (§5 C3)

| Option | Mechanism | Verdict |
|---|---|---|
| **`delete(id, G)`** (chosen) | The intent carries the generation it was formed at; applied only while the Store is still at `G` | ✓ |
| `delete(id)` | Id only | ✗ violates L1 |
| Soft-mark only (`set_deleted`) | Flag instead of removal | ✗ does not reclaim; reclamation is GC's purpose |
| Content-hash predicate | Re-hash the payload | Same protection, bought with a payload scan instead of a stored scalar |
| Predicate/condition delete | Arbitrary caller condition | Pushes the ABA reasoning into every caller |
| Batch delete | List of ids | Amortizes calls; the per-intent staleness problem is unchanged |
| Per-object `generation` column | A version on each row | ✗ forbidden by C8 (no second generation field); unnecessary, since `G` is one Store scalar |

### 3.4 Requirement ladder

| Level | Requirement | `delete(id)` | `delete(id, G)` |
|---|---|---|---|
| L1 | A stale intent never acts on a now-reachable object (ABA) | ✗ | ✓ |
| L1 | A resumed intent can prove which state it was formed in | ✗ | ✓ |
| L1 | The record attributes a deletion to an intent | ✗ | ✓ |
| L2 | Applies to every backend and to a future online sweep | ✗ | ✓ |
| L2 | Storage overhead | — | **none**: `G` is already `catalog_incarnation` |
| L4 | Signature stays comprehensible | ✓ | ✓ with the generation documented |

### 3.5 Reasoning, and the basis for the decision

This decision has **no trade-off step**: `delete(id)` fails L1 the first time
content recurs after a sweep, and one failure is retained data vanishing. An L1
failure ends the comparison regardless of how much simpler the option is, so the
decision is settled at the gate (step 2), not by weighing options.

What "bound to `G`" buys in practice: the sweep runs inside the exclusive window
(C10) at one generation, so there are no concurrent sweeps to confuse; what the
generation protects is the intent that outlives the window — a queued or resumed
delete. C1 already says an instalment re-reads the generation and abandons the
round if it moved; C3 is the same discipline applied to a single deletion, so
that a replayed intent is dropped rather than applied to a Store that has moved
on.

Scope: SQLite's compact does not delete rows at all — it replaces the file, and
ordering is covered by C1's install-before-generation re-check. The signature is
still used uniformly so PostgreSQL and any future online sweep cannot drift to
the unsafe form.

**Decision basis**: must-have correctness gate (ABA safety). The extra argument
is accepted without being weighed; there is no storage cost to weigh.

### 3.6 Relation to Q1 — same family, opposite mechanisms

Sign-off for Q1 is requested on #44 (comment 6818137920); this section and that
request should be read together.

Both decisions exist because an identifier may outlive the thing it named, and
both are the reason C6/I2 ("an immutable id never resolves to different
content") is stated as an invariant. They are not the same problem:

| | **Q1** (Branch name) | **C3** (object delete) |
|---|---|---|
| Id derived from | `blake3(stack ‖ name)` — a **user-chosen, mutable** input | `blake3(OBJECT_DOMAIN ‖ canonical)` — **the bytes themselves** |
| How identity goes wrong | **aliasing**: two different Branches, one id | **reachability flip**: one content id, unreachable → reachable again |
| Consequence | history re-attributed; stale `BranchId` resolves to a different Branch | a stale delete removes a live object |
| Fix chosen | **forbid reuse** — terminal tombstone, `NameRetired` | **permit recurrence, bind the intent to `G`** |
| Why not the other fix | a version on `BranchId` costs a format change and a migration (Q1-B) | forbidding recurrence is impossible: content addressing reproduces the id for the same bytes, and a per-row version is forbidden by C8 |

Two consequences worth recording:

1. **One generation scalar, reused.** With Q1 = A the system has a single
   generation concept — `catalog_incarnation` — serving the reader fence (C8) and
   delete intents (C3). Q1 = B would add a second one to reason about, migrate
   and validate.
2. **Q1 = A is what keeps `branch.remove(name)` safely name-only.** #41 requires
   the three resource operations to take only `name`. Under Q1 = B a stale
   `remove(name)` could act on a recreated Branch — the C3 failure inside the
   Branch namespace — and a generation argument would have to be pushed into the
   resource calls, against #41. Under Q1 = A the name is never live twice, so
   `remove(name)` has one possible target.

## 4. Summary — what each decision rests on

**Recommended defaults**: **R1 = E** (exclusive window), **Q1 = A** (no name
reuse), **C3 = `delete(id, G)`**. Each sign-off request marks its recommended
box `[x]` so approval is one tick; the alternative box is separate and says
what accepting it costs.

| Decision | Settling level | Settling requirement | Options that fell out before the settling step |
|---|---|---|---|
| **R1** | L3 (availability) | Gate variable **S11**: is a read-available SLO required? — no → **E**; yes → **A'** | B (lock points per table), C (writability via triggers) |
| **Q1** | L2 (migration cost) | **Zero migration** + the invariant already asserted at `contract/identity.rs:266-268` → **A** | B is not rejected as wrong, only as too expensive for the benefit |
| **C3** | L1 (correctness) | **ABA safety** — a stale intent must not act on a now-reachable object; `G` is the existing Store generation → **`delete(id, G)`** | `delete(id)`, soft-mark, hash predicate, condition delete, batch, per-object version (forbidden by C8) |

Two properties hold for all three, and they are the reason the results are stable
rather than a matter of taste:

1. **The deciding step is named.** R1 is decided by a requirement variable, Q1 by a
   cost constraint, C3 by a correctness gate. Whenever a future discussion
   reopens one of them, the step to re-run is known.
2. **The rejected option is kept with its condition.** A' is the answer if S11
   becomes a read-available SLO; B is the answer if name reuse becomes a
   requirement. Neither is adopted now, and neither is lost.

---

## 5. Open items this record surfaces

Deciding R1, Q1 and C3 exposed places where the spec text still says two things
at once. They are listed here rather than resolved silently, because #50 cannot
implement around them without a choice.

### 5.1 PostgreSQL: rebuild-and-swap vs in-place row deletion

The spec's §7.3 (`LayerFS-GC-spec.md`) describes row deletion order —

> "Deletion order: delete references first (`object_location` etc.), then payload; the
> intermediate state is `backend_reusable`, reported separately." — §7.3

— which is **in-place deletion**, while R1-E builds a replacement schema, copies
retained rows and swaps names. The swap path never deletes an object row, so
there is no "references first, payload second" intermediate state, and
`backend_reusable` on PostgreSQL would be unobservable (it would have to report
0, with `physical_shrink` carrying the whole story).

Two coherent readings, one of which must be chosen:

- **(a) PostgreSQL compacts by replacement + rename** (consistent with R1-E).
  §7.3's delete-order bullet is removed, `backend_reusable` reports 0 for
  PostgreSQL with the reason stated, and physical reclamation is measured as the
  `pg_relation_size` / `pg_total_relation_size` delta after `DROP SCHEMA`.
- **(b) PostgreSQL keeps an in-place deletion path** in addition. §7.3 stands,
  but it is a second compact entry point with its own admission, its own
  interruption semantics and its own report, distinct from the R1-E swap.

Consequence for C3: §5 C3 names "(PostgreSQL, future online GC)", i.e. it assumes
PostgreSQL deletes rows. Under (a) C3 binds only a future online sweep and is a
forward-looking contract; under (b) it becomes a v2 deliverable. The C3 sign-off
request carries this as its "scope question" box.

### 5.1a Why in-place deletion was ruled out (restored)

The superseded 1,604-line draft of the specification carried a section that the
318-line version dropped when it kept the decision but not the argument. Restored
here verbatim (its `§6.1` / `§6.3` are *that draft's* numbering, not this
directory's):

> **§2.5 Why in-place row deletion is not the plan (either backend)**
>
> A reachability-driven `DELETE` of unreachable rows, then `VACUUM FULL` (PG) or
> `incremental_vacuum` (SQLite), does not survive the schema: `pack` refuses
> `DELETE` and `UPDATE` by trigger on both backends (§6.1), so the biggest bytes
> cannot be freed without disabling the trigger, which needs table ownership and
> opens a window in which a concurrent writer can mutate an immutable row. It also
> breaks the lock-free read invariant (§6.3). Rebuild-and-swap replaces it — one
> algorithm, two swap primitives.

Where those two old anchors now sit:

| Old anchor | Current anchor | What it says |
|---|---|---|
| §6.1 (pack immutability) | §1.1 **S5**, §5 **C2**, §7.2 | candidates are `object_location` and `content_signature` rows; **`pack` is never a candidate** because it has immutability triggers on both ends |
| §6.3 (lock-free reads) | code: `engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs:181-197` | `read_immutable` runs with no BEGIN/COMMIT, which is "safe only because every row such a body reads never changes once visible: `object_location` is insert-only … `pack` and `pack_unit` refuse UPDATE and DELETE by trigger" |

Why the argument is decisive, in current terms:

- `sql/postgresql/schema.sql:25-28` — `pack_immutable()` is `BEFORE UPDATE OR DELETE ON pack`. An in-place sweep cannot delete a pack row at all, and a pack is where the bytes are. Disabling the trigger needs table ownership and re-opens the window the trigger exists to close.
- The lock-free read path is sound *because* those rows never change. An in-place `DELETE` of `object_location` rows would make two such reads able to observe an inconsistent pair — the reason `read_immutable` documents its own premise.

Consequence for §5.1: if this argument is accepted, reading **(a) replacement + rename** is the only one that does not reopen pack immutability; reading **(b) an additional in-place path** is not a cheap option, it is a change to the immutability and lockdown contracts. Owner decides; the sign-off box on #42 already asks the question in that form.

### 5.2 What is *not* open

- `pack` immutability on PostgreSQL is settled: the trigger refuses `UPDATE` and
  `DELETE`, so reclamation cannot delete pack rows and "new tables + new
  triggers" is the only legal path (`sql/postgresql/schema.sql:25-28`).
- Physical reclamation is measured the same way under both readings —
  `pg_relation_size` / `pg_total_relation_size` before and after — so the metric
  does not depend on 5.1.

### 5.3 Recording the answer

Whichever way 5.1 goes, it lands in two places: this section (the decision), and
the affected issue — #50 for the compact path, #42 for C3's scope.
