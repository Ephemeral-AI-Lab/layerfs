# Optimization study: parallelism, batching and round trips

> **Status:** Research; informative and not a product contract.
>
> **STUDY — nothing here is measured.** Every figure is a *declared constant* read
> from source, or an arithmetic consequence of one. No benchmark was run, no
> receipt exists, and **no performance claim is made**. A configuration or
> statement-count difference is not an effect. See
> [§16.1](#161-status-and-authority).

Part of the [replacement-core architecture](README.md) set. Source pin
`ce2d738ff`; scope, method, measurement status and upkeep are stated in the
[index](README.md).

Chapter numbers are global to the set: this paper holds **chapter 16**. Unlike
chapters 1–15, it is **not** a description of shipped behaviour — it is a
comparative study with an opportunity register.

Related work, deliberately not duplicated here:

| Where | Covers |
| --- | --- |
| [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176) | complexity inventory, O(1)/O(log n) opportunities, the round-trip register |
| [`08-representations.md`](08-representations.md) | storage economics and transitions |
| [`09-delta-hints.md`](09-delta-hints.md) | the positional-hint proposal |
| [`10-counters.md`](10-counters.md) | what each counter reports |
| `docs/roadmap/0.1/0.1.7/component-decoupling/parallelism-and-batching-study-20260918.md` | the dated snapshot this paper supersedes |

---

## 16. Optimization study: parallelism, batching and round trips

### 16.1 Status and authority

| Part | Kind | Authority |
| --- | --- | --- |
| Every constant and structural claim | **Descriptive** — read from source at the pin, both trees | Same as the set |
| §16.7 ledger | **Descriptive comparison** — which tree has which mechanism | Same as the set |
| §16.8 opportunity register | **Proposed** — nothing implemented | Draft |
| Every "impact" statement | **Not established** — no measurement | Do not cite |

**What "regression" means in this paper.** It means *core lacks a mechanism the
reference has*, or *core performs more declared operations for the same declared
work*. It does **not** mean "core is slower". Establishing that requires Stage 6
([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)) measurement, and
nothing in this paper substitutes for it.

### 16.2 Method

Source reads only, both trees, at `ce2d738ff`. No builds, tests, benchmarks or code
changes. Where a count is given, it is computed from declared constants and the
arithmetic is shown. Where the arithmetic depends on an engine default (for example
SQLite's parameter limit), the default is named so it can be checked.

Three terms are used precisely throughout:

- **declared operation** — one call that crosses a boundary: a SQL statement, a
  `blob_open`, a provider wave, a file read.
- **settled** — read from source and not expected to change without a source change.
- **unverified impact** — the mechanism difference is verified; its cost is not.

---

## 16.3 Parallelism

### 16.3.1 What the reference does

`crates/layerfs-layerstack-store/src/objects.rs` implements one bounded
producer/consumer pipeline, documented as the shared implementation: *"One bounded
ownership/drain/join implementation for native and Workspace inputs."*

```text
   run_finalized_output(worker_limit, task_count, tasks, cancelled, …)

   ┌── worker 0 ─────────────┐
   │  initialize(0)           │   std::thread::scope × workers
   │  loop {                  │   workers = worker_limit.min(task_count)
   │    claim task  ←─────────┼─── Mutex<Iterator> + AtomicU64
   │    step(&mut state, …)   │
   │    writer.push(slab) ────┼──┐
   │  }                       │  │  sync_channel(QUEUE_SLOTS = 4)
   │  finish(state)           │  │  slab ≤ 512 objects / 256 KiB
   └──────────────────────────┘  │  ⇒ ≤ 1 MiB buffered, ≤ 2,048 objects
   ┌── worker N-1 ────────────┐  │
   │  … same …                │  │
   └──────────────────────────┘  │
                                 ▼
                   ┌── ONE consumer ─────────────────┐
                   │  receiver.recv() → consume(slab) │
                   │  = ADMISSION — serial            │
                   │  metrics.consumer_idle_ns        │
                   └──────────────────────────────────┘
```

| Property | Implementation |
| --- | --- |
| Worker bound | `min(available_parallelism(), SMALL_CONTENT_WORKERS = 4)` for small-content stores, else `available_parallelism()` |
| Distribution | work-stealing by claim (`Mutex<Iterator>` + atomic), not partitioned |
| Panic isolation | `catch_unwind(AssertUnwindSafe(…))` per worker → `StoreError::Integrity`, sets `cancelled` |
| Cancellation | cooperative, checked before each claim |
| Failure drain | the consumer keeps receiving "so every sender and retained source owner joins" |
| Coverage proof | asserts `claimed == completed == task_count` |
| Starvation instrument | **`consumer_idle_ns`** — time the single consumer spent waiting |

**The last row is the important one.** The reference already measures whether its
producers are keeping up. That counter is the correct instrument for deciding
whether more producers would help, and it is the thing core does not have.

### 16.3.2 What core does

**Nothing.** Verified by exhaustion across `core/crates/*/src`:

| Probe | Core | Reference |
| --- | ---: | ---: |
| `std::thread::spawn` | **0** | 28 |
| `std::thread::scope` | **0** | present |
| `rayon` | **0** | 0 |
| `tokio` | **0** | 75 references |
| `sync_channel` | **0** | present |
| `AtomicU64` | **0** | present |
| `catch_unwind` | **0** | present |

The only concurrency primitive in the entire core workspace is one
`Arc<Mutex<PoolIndex>>` in `cas/store.rs` — a shared cache handle, not a worker.

### 16.3.3 The serial fraction is shared

**In both trees, admission is serial.** The reference parallelises the producer
only; core does neither.

```text
   PARALLELISABLE (reference)              SERIAL (both trees)
   ──────────────────────────              ───────────────────
   read the source bytes                   SQL location lookups
   BLAKE3 over canonical bytes             delta base acquisition + trial
   FastCdc scan                            pack assembly + group sealing
   zstd encode                             record_number assignment
   attribute-tree construction             SQLite writes + COMMIT
   mapping / tree page encoding
        │                                        │
        └── N producers ──► bounded queue ──► 1 consumer
```

Any proposal to restore the pool is bounded by Amdahl's law at the consumer, and
`consumer_idle_ns` is what would size that bound.

### 16.3.4 What parallelism would cost core

Two structural costs, both verified.

**(a) The timing tree would lose the parallel portion.** `TimingScope` is `!Send`
and `!Sync`, enforced by
`layerfs-telemetry/tests/compile_fail/send_scope_across_threads.rs`. Every C1 entry
point takes a scope, so an operation cannot be moved to another thread, and work on
a worker thread could not be a labelled child of the operation's span. A receipt
would omit the parallel work or measure it by a mechanism `core/AGENTS.md` forbids.

**(b) Storage layout would stop being reproducible.** In `cas/selection.rs`:

```rust
// the seal decision reads the ACCUMULATED group state
occupied && framed_group_length(
    self.groups[index].records.len() + 1,
    self.groups[index].payload_len + record.record.len(),
)? > GROUP_TARGET

group.records.push(record.record);                 // ARRIVAL order
for (record_number, member) in pending.members.iter().enumerate() { … }
//   ^ record_number IS the arrival position
```

Emission order determines group composition, pack boundaries and **every locator**
`(pack_id, group_number, record_number)`. Single-threaded and deterministic today,
the same input yields the same layout. Parallel construction emits in completion
order: identical canonical identities, **different physical layout**, and receipts
that cannot be compared run to run.

### 16.3.5 Policy

[`AGENTS.md`](../../AGENTS.md) states the trade and forbids the obvious remedy:

> **A performance drop against v0.1.5 is expected** for the single-worker cases and
> is absorbed by the bounded acceptance rule, **never by adding workers back.**

---

## 16.4 Batched work

### 16.4.1 Queue and admission bounds

| Bound | Reference | Core | |
| --- | --- | --- | --- |
| queue slab objects | `INITIALIZATION_SLAB_OBJECTS` = 512 | `BATCH_OBJECT_LIMIT` = 512 | same |
| queue slab bytes | `INITIALIZATION_SLAB_BYTES` = 256 KiB | `BATCH_CANONICAL_BYTES_LIMIT` = 512 KiB | core 2× larger |
| queue slots | `INITIALIZATION_SLAB_QUEUE_SLOTS` = 4 (≤ 1 MiB) | 1 (drain on full) | reference buffers more |
| admission batch count | `ADMISSION_BATCH_COUNT` = 8,191 | `TRANSACTION_ROW_LIMIT` = 8,191 | same |
| admission batch bytes | `OBJECT_PAGE_BYTES − 1` = 4 MiB − 1 | `TRANSACTION_CANONICAL_BYTES_LIMIT` = 4 MiB − 1 | same |
| physical admission batch | `PHYSICAL_ADMISSION_BATCH_COUNT` = 512 | the wave, 512 | same |
| group limit | `GROUP_LIMIT` = 65,536 | `GROUP_LIMIT` = 65,536 | same |
| group target | **none declared** — fills to the limit | `GROUP_TARGET` = 49,152 | differs |
| pack limit | `PACK_LIMIT` = 256 KiB | `PACK_LIMIT` = 256 KiB | same |
| group count / pack | 256 | 256 | same |
| record count / group | 8,191 | 8,191 | same |
| canonical limit | 16 MiB | 16 MiB | same |

The batching **arithmetic is essentially identical**; core ported the reference's
numbers. The two differences are the queue depth — the reference's four slots exist
because producers run ahead of a consumer — and the group target.

### 16.4.2 Group target, and why the direction is unclear

```text
   PACK_LIMIT = 262,144 = 256 KiB

   reference: fills a group to GROUP_LIMIT = 65,536 then seals
        4 × 65,536 = 262,144  ← EXACTLY the pack limit, leaving no room
                                for the 16-byte header and directory
        ⇒ 3 groups/pack = 196,608 bytes of body
        ⇒ each group large ⇒ each zstd frame has the most context

   core: seals when the NEXT record would pass GROUP_TARGET = 49,152
        5 × 49,152 = 245,760  ← fits, with ~16 KiB left for framing
        ⇒ 5 groups/pack = 245,760 bytes of body
        ⇒ each group smaller ⇒ each zstd frame has less context
```

Core fills each **pack** better (245,760 against 196,608 bytes of body) while giving
each **group** less compression context. Which dominates depends on content and on
records per group — a measurement, not a reading.

---

## 16.5 Round trips

### 16.5.1 Object-row insertion — core issues one statement per row

```text
   REFERENCE — objects/admission.rs
        let locator_rows = sql_rows(transaction, 5, 12)?;
        for page in locators.chunks(locator_rows) {
            let sql = format!(
                "INSERT INTO objects(object_id,canonical_length,pack_id,group_number,record_number) \
                 VALUES {}",
                vec!["(?,?,?,?,?)"; page.len()].join(","));          // MULTI-ROW
            transaction.prepare_cached(&sql)?.execute(params_from_iter(values))?;
        }

        fn sql_rows(connection, parameters, row_bytes) -> usize {
            OBJECT_PAGE_COUNT                                   // 128  ← binds here
                .min(parameters_limit / parameters)              // SQLITE_LIMIT_VARIABLE_NUMBER / 5
                .min(sql_limit.saturating_sub(256) / row_bytes)  // SQLITE_LIMIT_SQL_LENGTH
        }

   CORE — cas/placement.rs (the owner split moved this loop there at P2-0)
        for (record_number, member) in pending.members.iter().enumerate() {
            write::insert_object(&self.connection, &ObjectRow { … })?;   // ONE ROW
        }
   CORE — sqlite/write.rs
        "INSERT INTO objects (…) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
```

The chunk size is the **minimum of three engine-derived limits**, and at default
SQLite settings `OBJECT_PAGE_COUNT = 128` is the binding term — not the parameter
limit (32,766 / 5 = 6,552).

| | Rows per statement | Statements for 8,191 rows |
| --- | ---: | ---: |
| Reference | **128** | **64** |
| Core | 1 | **8,191** |

That is **≈127× more declared operations** for the same rows. `prepare_cached` is
used in both, so this is not SQL parsing — it is 8,191 execute cycles against 64,
each with its own step/reset over an already-prepared statement.

The same pattern appears for packs: the reference uses
`INSERT INTO object_packs(pack_id,data) VALUES {}` (multi-row,
`admission.rs:1547`), core uses `insert_pack` — `INSERT INTO object_packs (pack_id,
data) VALUES (?1, ?2)` — one row per statement.

**Verified against the reference's own other multi-row sites**, so this is a
deliberate reference pattern rather than an isolated case:

```text
   INSERT OR IGNORE INTO seen(id) VALUES {}              spill.rs:192
   INSERT INTO offsets(id,offset,length) VALUES {}       spill.rs:752
   INSERT INTO object_packs(pack_id,data) VALUES {}      admission.rs:1547
   INSERT INTO objects(…) VALUES {}                      admission.rs:1603
```

### 16.5.2 Pack-body reads — core reads once per pack, per wave

```text
   REFERENCE — objects/read.rs, SIX sites, ALL read_only = true
        let blob = connection.blob_open("main", "object_packs", "data",
                                        pack_id, /* read_only */ true)?;
        blob.read_at_exact(&mut buffer[..length], 41 + self.cursor)?;
        ⇒ SQLite INCREMENTAL BLOB I/O — a handle per record extraction,
          reading only the byte range needed. No whole-pack read. No cache.

   CORE — sqlite/lookup.rs + encoding/delta/read.rs
        pub fn pack_bytes(connection, pack_id) -> Vec<u8> {
            query_row("SELECT data FROM object_packs WHERE pack_id = ?1", …)
        }                                                    // WHOLE PACK
        Resolver::pack_of(&mut self.packs, connection, pack_id)
        ⇒ one read per pack per wave, retained in BTreeMap<i64, Vec<u8>>
          under DEPENDENCY_PACK_CACHE_BYTES = 4 MiB
```

For N records drawn from one pack: the reference performs N blob opens plus range
reads; core performs **one** `SELECT` and serves the rest from memory.

**The trade runs the other way on bytes.** Core reads the whole pack — up to
`PACK_LIMIT = 256 KiB` — even when it needs one record; the reference reads only
the range it needs. Which wins depends on records-per-pack per wave.

rusqlite's `blob` feature **is enabled** in core's manifest
(`features = ["cache", "hooks", "trace", "limits", "blob"]`) and unused.

### 16.5.3 SQL lookup paging — identical

```text
   CORE      sqlite/lookup.rs:  ids.chunks(LOOKUP_PAGE_IDS)      // 128
   REFERENCE objects/read.rs:   ids.chunks(SQLITE_LIMIT_VARIABLE_NUMBER
                                            .min(OBJECT_PAGE_COUNT))  // 128
```

Both page at 128. **`READ_OBJECT_LIMIT = 4,096` is a demand ceiling** — how many
ids C1 may name in one provider call — **not** a SQL page size. Core's own comment,
*"one wave is one grouped SQL lookup"*, describes the API surface; the
implementation pages. A 4,096-id wave is 32 queries in both trees. See
[§16.10](#1610-corrections-to-earlier-revisions).

### 16.5.4 Pack append — identical, and both rewrite the whole BLOB

```text
   CORE      sqlite/write.rs:  "UPDATE object_packs SET data = ?2 WHERE pack_id = ?1"
   REFERENCE objects.rs:2403:  "UPDATE object_packs SET data=?2 WHERE pack_id=?1"
```

Both rewrite the entire pack body on every append. All six reference `blob_open`
sites are `read_only = true`, so the reference does **not** append incrementally
either. This is a shared cost, not a core regression — it is recorded in
[#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176)'s seed findings.

---

## 16.6 SQLite connection profile

The two trees do **not** share a connection profile.

```text
   REFERENCE — schema.rs configure_connection
   ────────────────────────────────────────────────────────────────────────
   foreign_keys = true          journal_mode = MEMORY (verified by read-back)
   synchronous  = OFF           temp_store   = MEMORY
   cache_size   = −SQLITE_PAGE_CACHE_KIB   ← −32,768 KiB = 32 MiB
   cache_spill  = OFF           ← keeps dirty pages resident to COMMIT
   mmap_size    = 0             threads      = 0
   locking_mode = EXCLUSIVE     ← verified by read-back
   page_size    = 4,096

   CORE — sqlite/connection.rs configure
   ────────────────────────────────────────────────────────────────────────
   journal_mode = MEMORY (verified)    synchronous  = OFF (verified)
   temp_store   = MEMORY               foreign_keys = ON  (verified)
   busy_timeout = 0 (verified)
   cache_size   = NOT SET  ← SQLite default −2,000 KiB = 2 MiB
   cache_spill  = NOT SET  ← SQLite default ON
   mmap_size    = NOT SET  ← named in `Pragma` as "read for evidence only"
   locking_mode = NOT SET  ← default NORMAL
   threads      = NOT SET
```

Core's `Pragma` enum lists `CacheSize`, `MmapSize`, `PageSize` and `CacheSpill`
explicitly as read-only, so the omission is visible in source and is a choice
rather than an oversight. `SaveOperation::connection_profile()` (#178 **V7**,
2026-09-18) reads those four back **on the save's own connection** and returns
them as a bounded `SaveConnectionProfile`, so a receipt states the cache profile a
save actually ran under instead of replaying the row shape on a harness-owned
connection. What that accessor deliberately does **not** carry is
`SQLITE_DBSTATUS_CACHE_SPILL`: `rusqlite` 0.40.2 exposes no safe binding for
`sqlite3_db_status`, and this crate's `#![deny(unsafe_code)]` allows exactly one
audited FFI module (`encoding::codec`), enforced by the product boundary guard.
The spill counter is therefore read on a harness-owned connection (a forced
8-page cache is the live control) and never on the product's - a limitation of
the profile item `P2-1`'s write-path claim, recorded rather than worked around by
adding a second FFI site. Three consequences follow:

**(a) The page cache is 16× smaller** — 2 MiB against 32 MiB.

**(b) `cache_spill` is left ON where the reference turns it OFF — and this
consequence does not materialise.** `cache_spill = OFF` keeps dirty pages resident
to `COMMIT`; with it ON, a transaction that overflows the page cache spills
mid-flight and may write and re-read pages.

The reasoning here was that core's declared ceiling of `4 MiB − 1` canonical bytes
is **twice its own 2 MiB page cache**, so a full-size transaction would spill.
**That is wrong, and it has been measured.** The `4 MiB − 1` is a *byte* ceiling;
the binding limit is `TRANSACTION_ROW_LIMIT = 8,191` **rows**, and a product
transaction reaches 8,191 rows without approaching 4 MiB. No overflow, so no spill:

> `SQLITE_DBSTATUS_CACHE_SPILL` … reads **0** at 8,191 rows and **0** at 4× that,
> where the 2 MiB/ON and 32 MiB/OFF profiles agree on every other observable. The
> gate save opens **31** transactions at 8,191 rows, so no product transaction
> approaches 4 MiB. **The write-path half of the O1 premise is unsupported**; the
> read-path half still waits for P1-2.
> — [P0-2 receipt](../../../docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z/receipts/p0-2-spilling.md)

The counter is reachable with **no code change** and proven live by a control
(**1,032** spills at a forced 8-page cache), so the zero is a measurement rather
than an absent instrument. I mistook a declared ceiling for a reached one; the row
limit binds first, and the interaction described above cannot occur at these sizes.

**(c) `locking_mode` stays `NORMAL`.** The reference takes `EXCLUSIVE`, removing the
per-transaction lock cycle. Core commits repeatedly across one save's lifetime, so
this is a per-transaction cost.

**None of this is a durability difference.** Both trees keep MEMORY journaling,
`synchronous = OFF` and no `fsync` anywhere. `locking_mode` is a locking-scope
setting; core's no-WAL / no-fsync rules are untouched by it.

---

## 16.7 The ledger

| # | Area | Reference | Core | Verdict |
| ---: | --- | --- | --- | --- |
| F1 | Construction pool | bounded producer/consumer, ≤ 4 workers | none | **core lacks** |
| F2 | SQLite cache / spilling | 32 MiB, `cache_spill = OFF` | 2 MiB default, spilling ON | **differs — write-path impact DISPROVEN, see §16.6(b)** |
| F3 | Locking / engine threads | `EXCLUSIVE`, `threads = 0` | defaults | **core lacks** |
| F4 | ~~read batching~~ | 128 | 128 | **retracted — equal** |
| F5 | Group target | fills to 65,536 | seals at 49,152 | differs, unclear |
| F6 | Queue, admission, pack and group bounds | — | — | **shared** |
| F7 | Delta candidate cache | 128 KiB / 1,024 slots | identical | **shared** |
| F8 | Object-row insertion | multi-row, 128/statement | one row/statement, 8,191 | **core lacks** |
| F9 | Pack-body reads | blob open per record | one cached read per pack per wave | **core ahead** |
| F10 | SQL lookup paging | 128 | 128 | **shared** |
| F11 | Pack append | rewrites whole BLOB | rewrites whole BLOB | **shared** |
| F12 | Prepared statements | `prepare_cached` | `prepare_cached` | **shared** |

**Tally: four areas where core lacks, one where it is ahead, six shared.** The
removals are not uniform, and two of the four (F2, F8) are not about concurrency at
all.

---

## 16.8 Opportunity register

Nothing here is implemented. Each entry states what is known, what is not, and the
test that would settle it. Ranked by strength of evidence and by how little the
change would cost elsewhere.

**Updated after Phase 0:** O1's write-path premise was disproven by measurement and
it drops from first place. **O2 (multi-row INSERT) now leads** — it is the one item
whose premise is a statement count verified by reading, with no engine interaction
to explain away.

### O1 — SQLite page cache and spilling (F2) · **WRITE-PATH PREMISE DISPROVEN**

**Measured, not inferred:** no spill occurs at any size this product reaches —
`SQLITE_DBSTATUS_CACHE_SPILL` is 0 at 8,191 rows and 0 at four times that, with the
instrument proven live by a control at a forced 8-page cache
([P0-2](../../../docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z/receipts/p0-2-spilling.md)).
The binding limit is the **row** ceiling, not the byte ceiling, and the row ceiling
is reached long before the page cache overflows.

**So the write-path half of this opportunity is withdrawn.** What remains is the
**read-path half**, and it is not measurable yet: with a fresh connection per read
wave (`#176` RT-05) SQLite discards its page cache every wave, so `cache_size`
cannot show an effect on reads until connection pooling lands.

**Revised test:** pool the read connection first, then A/B `cache_size` on the read
path. Tuning the cache before pooling measures a profile that is about to change.
**Revised rank:** no longer first — see O2.

Kept rather than deleted because the error is instructive: I read a **declared
ceiling** as a **reached** one, and that difference is exactly what a measurement is
for.

### O2 — multi-row INSERT (F8)

**Known:** 8,191 statements against 64 for the same rows; the reference's chunk size
is derived from engine limits rather than hardcoded.
**Unverified impact:** per-statement overhead at core's row sizes.
**Test:** one save at the transaction ceiling, single-row against multi-row.
**Why second:** no concurrency, no format change, and the derivation is already
written in the reference.

### O3 — `locking_mode = EXCLUSIVE` (F3)

**Unverified impact:** per-transaction cost at core's commit frequency.
**Test:** same save, locking mode set versus unset.
**Caveat:** an exclusive lock is a *scope* change and must be reconciled with the
one-save-owner contract; a read-only Store should not take it.

### O4 — producer pool (F1)

**Known:** `min(available_parallelism(), 4)` producers, 4-slot bounded queue, one
consumer.
**Unverified impact:** the serial fraction at the consumer — the whole question.
**Test:** measure consumer starvation in core first. If the consumer is never idle,
producers only deepen a queue that already drains instantly.
**Blocked by:** `TimingScope` being `!Send`, locator determinism, and the
`AGENTS.md` rule — all three, not one.

### O5 — group target (F5)

**Unverified impact:** compression gain from larger groups against pack-fit loss.
**Test:** retained bytes and `packs_created` at both targets.

---

## 16.9 Preconditions

Stated as a precondition, not a suggestion. **Three of the four are now answered
by Phase 0**, so the state is recorded here rather than left as an open ask.

1. **Is there a gap at all? — ANSWERED, and the answer is not the one assumed.**
   A matched-workload receipt now exists
   ([P0-1](../../../docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z/receipts/p0-1-matched-workload.md)):
   three component cases, identity `MATCH` on all six pinned keys, one worker,
   cache state declared and equal in both arms. The candidate/reference ratios are
   **0.909 / 0.329 / 0.519** — all **below 1**, i.e. core is faster on those cases,
   not slower.
   **The receipt is diagnostic-only and this study does not treat it as a
   regression signal:** the `small` case's same-tree re-run spread (0.909 → 0.883)
   is as large as the cross-tree delta, and `pipeline.filesystem` / `pipeline.c2`
   are `NOT_RUN` with reasons re-verified on this tree. So the honest statement is:
   **no gap has been demonstrated, and the direction of the measured difference is
   the opposite of what this study's ledger implies.** "Core lacks a mechanism" is
   not the same claim as "core is slower", and §16.7 must not be read as the latter.
2. **Where is it?** Core's counters locate time by phase — see
   [`10-counters.md`](10-counters.md). A per-phase baseline now exists
   ([P0-3](../../../docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z/receipts/p0-3-counter-baseline.md));
   `ValidationWork` and the edit family's `nodes_read` are `NOT_EXPOSED` in the
   frozen vehicles.
3. **Is the consumer ever idle?** Still open — this decides whether O4 deserves
   further thought. No consumer-idle equivalent exists in core.
4. **Does spilling occur? — ANSWERED: no.** See §16.6(b). The row ceiling binds
   before the byte ceiling, so nothing this product writes overflows the 2 MiB page
   cache, and O1's write-path premise is withdrawn.

**The order this study originally proposed is therefore wrong.** It led with O1 on
an unmeasured premise that turned out to be false, and ranked O2 below it on the
assumption that cache tuning would matter first. Measurement inverted that.

And **matching v0.1.6 is not obviously the goal**: core was built to be measurable,
and `AGENTS.md` pre-commits to accepting a drop for that — but on the evidence
available, no drop has been shown either.

---

## 16.10 Corrections to earlier revisions

This study's predecessor — the dated snapshot in
`docs/roadmap/0.1/0.1.7/component-decoupling/` (committed `5e45897dd`, corrected
`ce2d738ff`) — contained one substantive error. It is recorded rather than quietly
fixed, because the mistake is instructive.

**F4 retracted.** The earlier revision claimed core was ahead on read batching:
"4,096 against 128, 32× fewer lookups". It conflated two constants.
`READ_OBJECT_LIMIT` is a *demand ceiling* — how many ids C1 may name in one provider
call. The SQL page size is `LOOKUP_PAGE_IDS = 128`, identical to the reference's
`OBJECT_PAGE_COUNT`, and `pages()` chunks by it. A 4,096-id wave is 32 queries in
both trees.

The error came from reading core's own comment — *"one wave is one grouped SQL
lookup"* — as a description of the implementation rather than of the API surface.
The implementation pages. That comment is imprecise; the study should have checked
`pages()` before making the claim.

Re-examining the read and write paths for that correction surfaced **F8** (multi-row
INSERT — a core regression, §16.5.1) and **F9** (pack-body reads — a core advantage,
§16.5.2). Neither was visible in the first pass.

---

## 16.11 Explicit non-findings

- **No measurement was taken.** Every figure is a declared constant or arithmetic
  over one.
- **No performance claim.** A configuration or statement-count difference is not an
  effect. The reference's larger cache is not evidence that core is slower; core's
  pack cache is not evidence that core reads faster.
- **No recommendation.** §16.8 registers opportunities with their unknowns and
  selects none.
- **No durability claim.** Both trees keep MEMORY journaling and
  `synchronous = OFF` with no `fsync` anywhere. Nothing here proposes changing that.
- **No claim that the reference's parallelism is worth 4×.** `SMALL_CONTENT_WORKERS
  = 4` is a cap, not a speedup.
- **No canonical-compatibility analysis.** Nothing proposed here changes emitted
  bytes; O1–O3 are connection and statement-shape settings, O4 and O5 would need
  their own analysis. Canonical risk is tracked by
  [#176](https://github.com/Ephemeral-AI-Lab/layerfs/issues/176).
