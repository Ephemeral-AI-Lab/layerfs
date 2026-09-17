# Parallelism and batching: core against the v0.1.6 reference

> **Status:** Research; informative and not a product contract. This study informs
> [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) and
> [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172); it decides
> nothing and measures nothing.
>
> **Superseded.** This is the dated snapshot. The maintained version, extended with
> the round-trip findings and a regression ledger, lives with the product at
> [`core/docs/architecture/11-optimization-study.md`](../../../../../core/docs/architecture/11-optimization-study.md).
> Retained append-only as the record of what was concluded on this date; read the
> core paper for anything current.

Release: [v0.1.7](../README.md). Parent:
[#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165).

- **Source pin:** `9761d3755`. The reference tree (`crates/`) and the replacement
  (`core/`) were both read at that commit. No file was modified by this study.
- **Method:** source reads only — no builds, tests, benchmarks or code changes.
  Every figure is a **declared constant** read from source, not a measurement. The
  study contains no performance numbers, and none of the differences below is
  asserted to cause a measurable effect.
- **Scope:** the two trees' *structure* for parallelism and batched work. It does
  not evaluate correctness, formats or storage economics; those are
  [`core/docs/architecture/`](../../../../../core/docs/architecture/README.md).

## 0. Summary

| # | Finding | Direction |
| --- | --- | --- |
| F1 | The reference runs a bounded producer/consumer pool for construction; core is single-threaded | **core lacks** |
| F2 | The reference sets `cache_size = 32 MiB` and `cache_spill = OFF`; core sets neither, leaving SQLite's 2 MiB default with spilling ON | **core lacks** |
| F3 | The reference sets `locking_mode = EXCLUSIVE` and `threads = 0`; core sets neither | **core lacks** |
| F4 | ~~read batching~~ **RETRACTED — both page SQL at 128.** See §7 | *corrected* |
| F8 | The reference inserts up to 128 rows per multi-row INSERT; core issues one statement per row | **core lacks** |
| F9 | Core reads a whole pack once per wave and caches it; the reference opens a read-only blob per record | **core better** |
| F5 | The reference fills groups to 65,536 bytes; core seals at a 48 KiB target | **differs, unclear** |
| F6 | Queue slabs, admission batches, pack and group limits, and the journal/synchronous/temp profile are identical | **shared** |
| F7 | Admission itself is serial in *both* trees — the reference parallelises the producer, never the consumer | **shared** |

The headline is F2: **the largest structural difference after parallelism is not
parallelism at all.** It is a SQLite page cache 16× smaller than the reference's,
with cache spilling left on.

---

## 1. Parallelisation

### 1.1 What the reference does

`crates/layerfs-layerstack-store/src/objects.rs` implements one bounded
producer/consumer pipeline, `run_finalized_output`, used for native and Workspace
inputs alike. It is **explicitly documented as the shared implementation**:

> One bounded ownership/drain/join implementation for native and Workspace inputs.
> Keep lifecycle callbacks explicit rather than introducing a configuration wrapper.

```text
   run_finalized_output(worker_limit, task_count, tasks, cancelled, …)

   ┌── worker 0 ─────────────┐
   │  initialize(0)           │   scope.spawn × workers
   │  loop {                  │   workers = worker_limit.min(task_count)
   │    claim task  ←─────────┼──── Mutex<Iterator> + AtomicU64 claimed
   │    step(&mut state, …)   │
   │    writer.push(slab) ────┼──┐
   │  }                       │  │
   │  finish(state)           │  │  sync_channel(QUEUE_SLOTS = 4)
   └──────────────────────────┘  │
   ┌── worker N-1 ────────────┐  │  each slab ≤ 512 objects / 256 KiB
   │  … same …                │  │
   └──────────────────────────┘  │
                                 ▼
                    ┌── ONE consumer ────────────────┐
                    │  receiver.recv() → consume(slab) │
                    │  = ADMISSION (serial)            │
                    └──────────────────────────────────┘
                    metrics.consumer_idle_ns ← time spent waiting
```

Structural properties worth naming:

- **Scoped threads** (`std::thread::scope`), so no `'static` bound and no join
  leaks. Workers join before the function returns.
- **Work-stealing by claim**: a `Mutex<Iterator>` plus an atomic counter, not a
  partition. A worker that finishes early takes the next task.
- **Panic isolation**: `catch_unwind(AssertUnwindSafe(…))` per worker; a panic
  becomes `StoreError::Integrity` and sets the `cancelled` flag.
- **Cancellation is cooperative**: workers check `cancelled` before each claim.
- **Failure drains**: the consumer keeps receiving after a failure, "so every
  sender and retained source owner joins".
- **Coverage is proved, not assumed**: completion asserts
  `claimed == completed == task_count`.

### 1.2 The worker bound

```text
   crates/layerfs-layerstack-store/src/objects.rs:48
        pub(crate) const SMALL_CONTENT_WORKERS: usize = 4;

   objects.rs  : self.source.is_some_and(ObjectSource::small_content_format)
                 ⇒ worker_limit.min(SMALL_CONTENT_WORKERS)
                 else worker_limit

   layerstack.rs: let worker_limit = std::thread::available_parallelism()…
   changes.rs   : let workers = construction_worker_limit();
```

So the effective producer count is `min(available_parallelism(), 4)` for
small-content stores and `available_parallelism()` otherwise, further capped by
the caller's `construction_worker_limit()` and by `task_count`.

### 1.3 What core does

**Nothing.** `core/` contains no `std::thread::spawn`, no `std::thread::scope`, no
tokio, no rayon, no channels and no atomics in its product path. The only
concurrency primitive in the whole workspace is one `Arc<Mutex<PoolIndex>>` in
`layerfs-storage/src/cas/store.rs` — a shared cache handle, not a worker.

Three structural facts make this deliberate rather than unfinished:

| Fact | Where | Consequence |
| --- | --- | --- |
| `TimingScope` is `!Send` + `!Sync` | `layerfs-telemetry`, pinned by `tests/compile_fail/send_scope_across_threads.rs` | an operation **cannot** be moved to another thread; parallel work could not be a child of its span |
| `begin_save` takes exclusive write ownership once | `cas/store.rs` | writes are serialised by construction |
| `FilesystemObjects` holds `&dyn` / `&mut dyn` | `filesystem/objects.rs` | not shareable or cloneable |

And the policy is explicit in [`AGENTS.md`](../../../../../AGENTS.md):

> **A performance drop against v0.1.5 is expected** for the single-worker cases
> and is absorbed by the bounded acceptance rule, **never by adding workers back.**

### 1.4 The serial fraction is shared

This is the finding that most constrains any proposal to restore the pool. **In
both trees, admission is serial.**

```text
   PARALLELISABLE                          SERIAL
   ─────────────                           ──────
   read the source bytes                   SQL location lookups
   BLAKE3 over canonical bytes             delta base acquisition + trial
   FastCdc scan                            pack assembly + group sealing
   zstd encode                             record_number assignment
   attribute-tree construction             SQLite writes + COMMIT
   mapping / tree page encoding
        │                                        │
        └── N producers ──► bounded queue ──► 1 consumer
                                                 (the reference's design)
```

The reference's own instrumentation concedes this: it records
`consumer_idle_ns` — the time the single consumer spent *waiting*. That counter
exists precisely to answer "are the producers keeping up?", and it is the correct
instrument for deciding whether more producers would help.

**Any parallelism proposal is therefore bounded by Amdahl's law at the consumer.**
What fraction that is, this study cannot say: it is a measurement, and it is
exactly what `consumer_idle_ns` plus the per-phase timings in
[`core/docs/architecture/10-counters.md`](../../../../../core/docs/architecture/10-counters.md)
would establish.

### 1.5 What parallelism would cost core

Beyond throughput, two properties would be at risk. Both are structural, not
stylistic.

**(a) The timing tree would lose the parallel portion.** `TimingScope` is `!Send`
by construction and by a compile-fail fixture. Work done on a worker thread cannot
be a labelled child of the operation's span. A receipt would then either omit the
parallel work or measure it by a mechanism that reintroduces test-only clocks — the
pattern `core/AGENTS.md` forbids.

**(b) Storage layout would stop being reproducible.** Verified in
`cas/owner.rs`:

```rust
// the seal decision reads the ACCUMULATED group state
occupied && framed_group_length(
    self.groups[index].records.len() + 1,
    self.groups[index].payload_len + record.record.len(),
)? > GROUP_TARGET

group.records.push(record.record);                      // ARRIVAL order
for (record_number, member) in pending.members.iter().enumerate() { … }
//   ^ record_number IS the arrival position
```

Emission order determines group composition, pack boundaries and **every locator**
`(pack_id, group_number, record_number)`. Single-threaded and deterministic today,
the same input yields the same layout — which is what makes two receipts
comparable. Parallel construction emits in completion order: identical canonical
identities, **different physical layout**.

---

## 2. Batch operations

### 2.1 The write path

| Bound | Reference | Core | |
| --- | --- | --- | --- |
| queue slab objects | `INITIALIZATION_SLAB_OBJECTS` = 512 | `BATCH_OBJECT_LIMIT` = 512 | same |
| queue slab bytes | `INITIALIZATION_SLAB_BYTES` = 256 KiB | `BATCH_CANONICAL_BYTES_LIMIT` = 512 KiB | **core 2× larger** |
| queue slots | `INITIALIZATION_SLAB_QUEUE_SLOTS` = 4 (≤ 1 MiB buffered) | 1 (drain on full) | **reference buffers more** |
| admission batch count | `ADMISSION_BATCH_COUNT` = 8,191 | `TRANSACTION_ROW_LIMIT` = 8,191 | same |
| admission batch bytes | `OBJECT_PAGE_BYTES − 1` = 4 MiB − 1 | `TRANSACTION_CANONICAL_BYTES_LIMIT` = 4 MiB − 1 | same |
| physical admission batch | `PHYSICAL_ADMISSION_BATCH_COUNT` = 512 | 512 (the wave) | same |
| group limit | `GROUP_LIMIT` = 65,536 | `GROUP_LIMIT` = 65,536 | same |
| **group target** | **none declared** — fills to the limit | **`GROUP_TARGET` = 48 KiB** | **F5** |
| pack limit | `PACK_LIMIT` = 256 KiB | `PACK_LIMIT` = 256 KiB | same |
| group count / pack | 256 | 256 | same |
| record count / group | 8,191 | 8,191 | same |
| canonical limit | 16 MiB | 16 MiB | same |

The batching **arithmetic is essentially identical**; the differences are the queue
depth (the reference's 4 slots exist because producers run ahead of the consumer)
and the group target.

**On F5, direction is genuinely unclear, and the arithmetic cuts both ways.**

```text
   PACK_LIMIT = 262,144 = 256 KiB

   reference: fills a group to GROUP_LIMIT = 65,536, then seals
        4 × 65,536 = 262,144  ← EXACTLY the pack limit, leaving no room
                                for the 16-byte header and the directory
        ⇒ 3 groups per pack  = 196,608 bytes of body
        ⇒ each group is large, so each zstd frame has the most context

   core: seals when the NEXT record would pass GROUP_TARGET = 49,152
        5 × 49,152 = 245,760  ← fits, with ~16 KiB left for framing
        ⇒ 5 groups per pack  = 245,760 bytes of body
        ⇒ each group is smaller, so each zstd frame has less context
```

So core fills each **pack** better (245,760 vs 196,608 bytes of body) while giving
each **group** less compression context. Which dominates depends on content
compressibility and on how many records a group holds — a measurement, not a
reading. Core's constant carries a rationale for the *comparison method* (framed
group identity, not a sum of per-record lengths), not for the 49,152 figure.

### 2.2 The SQLite profile — F2 and F3

The most concrete difference in this study. The two trees do **not** share a
connection profile.

```text
   reference — crates/layerfs-layerstack-store/src/schema.rs configure_connection
   ─────────────────────────────────────────────────────────────────────────────
   foreign_keys   = true                journal_mode   = MEMORY
   synchronous    = OFF                 temp_store     = MEMORY
   cache_size     = −SQLITE_PAGE_CACHE_KIB   ← −32,768 KiB  =  32 MiB
   cache_spill    = OFF                 ← keeps dirty pages resident
   mmap_size      = 0                   threads        = 0
   locking_mode   = EXCLUSIVE           ← verified by read-back
   page_size      = NEW_STORE_PAGE_SIZE_BYTES = 4,096

   core — core/crates/layerfs-storage/src/sqlite/connection.rs configure
   ─────────────────────────────────────────────────────────────────────────────
   journal_mode   = MEMORY   (verified by read-back)
   synchronous    = OFF      (verified)      temp_store = MEMORY
   foreign_keys   = ON       (verified)      busy_timeout = 0 (verified)
   cache_size     = NOT SET  ← SQLite default, −2,000 KiB = 2 MiB
   cache_spill    = NOT SET  ← SQLite default, ON
   mmap_size      = NOT SET  ← listed in `Pragma` "read for evidence only"
   locking_mode   = NOT SET  ← default NORMAL
   threads        = NOT SET
```

Core's `Pragma` enum lists `CacheSize`, `MmapSize` and `PageSize` explicitly as
"read for evidence only" — so the omission is visible in the source, and it is a
choice rather than an oversight. But three consequences follow from it:

**(a) The page cache is 16× smaller than the reference's.** 2 MiB against 32 MiB.

**(b) `cache_spill` is left ON, where the reference turns it OFF.** This is the
part that interacts with batching. SQLite spills dirty pages from a transaction
when the page cache is exceeded; with spilling *off*, pages stay resident until
`COMMIT` and are written once. Core's declared transaction ceiling is
`4 MiB − 1` of canonical bytes — **twice its own page cache** — so a full-size
transaction can spill mid-flight, and spilled pages may be written and re-read.
The reference pairs a 32 MiB cache with spilling off, so a 4 MiB transaction fits
entirely in memory and writes once.

**(c) `locking_mode` is left at `NORMAL`.** The reference takes `EXCLUSIVE`, which
removes the per-transaction lock cycle (unlock at commit, re-lock at the next
write). Core commits repeatedly across one save's lifetime, so this is a
per-transaction cost paid once per wave rather than once per save.

Note that (b) and (c) are **not** statements about durability. Both trees keep
`journal_mode = MEMORY` and `synchronous = OFF`; neither adds WAL, retries or
`fsync`. `locking_mode = EXCLUSIVE` is a locking-scope setting, not a durability
change, and core's no-WAL / no-fsync rules are untouched by it.

### 2.3 Read batching — corrected (F4 retracted)

An earlier revision of this study claimed core was ahead on read batching because
`READ_OBJECT_LIMIT = 4,096` against the reference's `OBJECT_PAGE_COUNT = 128`.
**That was wrong**, and the mistake is worth recording because the two constants
measure different things.

```text
   CORE — crates/layerfs-storage/src/sqlite/lookup.rs
        pub fn pages(ids: &[ObjectId]) -> impl Iterator<Item = &[ObjectId]> {
            ids.chunks(LOOKUP_PAGE_IDS)          // LOOKUP_PAGE_IDS = 128
        }
        ⇒ a 4,096-id wave is 32 SQL queries of 128 ids each, not one query

   REFERENCE — crates/layerfs-layerstack-store/src/objects/read.rs
        let count = connection.limit(SQLITE_LIMIT_VARIABLE_NUMBER)?
            .min(OBJECT_PAGE_COUNT);             // = 128
        for page in ids.chunks(count) { … }
        ⇒ also 128 ids per query
```

`READ_OBJECT_LIMIT` is a **demand ceiling** — how many ids C1 may name in one
provider call — not a SQL page size. Core's own comment ("one wave is one grouped
SQL lookup") reads as though it were one query; the implementation pages at 128
exactly as the reference does.

**So SQL round trips are identical on the read path**, and core's real gain is
different and smaller: a 4,096-id demand is one *provider* call instead of up to
32, and the wave shares one decode workspace. That is an API-surface and
memory-sharing improvement, not a round-trip one.

### 2.3a Multi-row INSERT — F8, a core regression

The clearest round-trip difference in either direction, found after the first
revision of this study.

```text
   REFERENCE — objects/admission.rs
        let locator_rows = sql_rows(transaction, 5, 12)?;
        for page in locators.chunks(locator_rows) {
            let sql = format!(
                "INSERT INTO objects(…) VALUES {}",
                vec!["(?,?,?,?,?)"; page.len()].join(","));      // MULTI-ROW
            transaction.prepare_cached(&sql)?.execute(params_from_iter(values))?;
        }

        fn sql_rows(connection, parameters, row_bytes) -> usize {
            OBJECT_PAGE_COUNT                                   // 128  ← binds here
                .min(parameters_limit / parameters)             // 32,766 / 5
                .min(sql_limit.saturating_sub(256) / row_bytes)
        }

   CORE — cas/owner.rs
        for (record_number, member) in pending.members.iter().enumerate() {
            write::insert_object(&self.connection, &ObjectRow { … })?;   // ONE ROW
        }

        sqlite/write.rs:
          "INSERT INTO objects (…) VALUES (?1, …, ?7)"    ← single-row, executed per row
```

| | Rows per statement | Statements for 8,191 rows |
| --- | ---: | ---: |
| Reference | **128** (capped by `OBJECT_PAGE_COUNT`) | **64** |
| Core | 1 | **8,191** |

The reference issues **≈127× fewer statements** for the same transaction. The
`prepare_cached` call is cached in both, so the difference is not SQL parsing — it
is 8,191 separate `execute` cycles against 64, each with its own step/reset over
the same prepared statement.

The same pattern applies to packs: the reference uses
`INSERT INTO object_packs(pack_id,data) VALUES {}` (multi-row, `admission.rs:1547`)
where core uses `insert_pack` — `INSERT INTO object_packs (pack_id, data)
VALUES (?1, ?2)` — one row per statement. Core's only multi-row-looking statement,
`INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)`, is in fact single-row.

### 2.3b Pack-body reads — F9, where core is ahead

```text
   REFERENCE — objects/read.rs, six sites, ALL read-only
        let blob = connection.blob_open("main", "object_packs", "data",
                                        pack_id, /* read_only = */ true)?;
        blob.read_at_exact(&mut buffer[..length], 41 + self.cursor)?;
        ⇒ SQLite INCREMENTAL BLOB I/O: opens a handle per extraction and
          reads only the byte range it needs; NO whole-pack read, NO cache

   CORE — sqlite/lookup.rs + encoding/delta/read.rs
        pub fn pack_bytes(connection, pack_id) -> Vec<u8> {
            query_row("SELECT data FROM object_packs WHERE pack_id = ?1", …) // WHOLE PACK
        }
        Resolver::pack_of(&mut self.packs, connection, pack_id)
        ⇒ one read per pack per wave, retained in BTreeMap<i64, Vec<u8>>
          under DEPENDENCY_PACK_CACHE_BYTES = 4 MiB
```

So for N records drawn from one pack, the reference performs N blob opens (each
followed by range reads) while core performs **one** `SELECT` and serves the rest
from memory.

**But the trade is real and runs the other way on bytes.** Core reads the whole
pack — up to `PACK_LIMIT = 256 KiB` — even when it needs a single record; the
reference reads only the range it needs. Core buys fewer SQLite API calls with
more bytes read. Which wins depends on how many records a wave draws from each
pack, and on whether the pack is already page-cached.

Note that rusqlite's `blob` feature **is** enabled in core's manifest, so the
capability is available and unused.

### 2.4 What is identical (F6)

Queue slab size (512), admission batch (8,191 / 4 MiB − 1), physical admission
batch (512), `GROUP_LIMIT` (65,536), `PACK_LIMIT` (256 KiB), `GROUP_COUNT_LIMIT`
(256), `RECORD_COUNT_LIMIT` (8,191), `CANONICAL_LIMIT` (16 MiB), and the
journal / synchronous / temp_store / foreign_keys profile. Core ported these
deliberately.

---

## 3. Where core is not lacking

Recording this matters, because the differences above are easy to read as a
general regression.

| Dimension | Verdict |
| --- | --- |
| Batch arithmetic (sizes, counts, limits) | equivalent — core ported the reference's numbers |
| Read batching | **equal** — both page SQL at 128; core's 4,096 is a demand ceiling, not a page size |
| Multi-row INSERT | **core is behind** — 64 statements against 8,191 for the same rows |
| Pack-body reads | **core is ahead** — one cached read per pack per wave against a blob open per record |
| Memory bounding | core declares every ceiling; nothing grows without an owner |
| Delta candidate cache | **identical** — both 128 KiB / 1,024 slots (`INDEX_BYTES`, `SLOTS`) |
| Ordering spill | core's filesystem reference reducer spills to a tiered run store under one declared byte owner |
| Layout reproducibility | core is deterministic; a producer pool would end that |
| Measurement coverage | core's timing tree can span the whole operation; parallel work could not be a child of it |

---

## 4. Opportunities, ranked by evidence

Each states what is known, what is not, and what would have to be measured. None is
a recommendation to implement.

### O1 — SQLite page cache and spilling (F2) · strongest

**Known:** the reference uses 32 MiB with `cache_spill = OFF`; core leaves SQLite's
2 MiB default with spilling ON, against a declared 4 MiB − 1 transaction ceiling.
**Unknown:** whether spilling actually occurs at core's transaction sizes, and what
it costs.
**Test:** run one save at the transaction ceiling and compare `cache_size` /
`cache_spill` set versus unset, holding everything else constant. Instruments
already exist: `SaveOutcome.commits`, transaction counters, and the timer.
**Why it ranks first:** it is a connection-level setting, it needs no concurrency,
it does not touch the timing tree or layout determinism, and core's own `Pragma`
enum already names the pragmas as evidence-only — so the change is small and the
observability is present.

### O2 — `locking_mode = EXCLUSIVE` (F3) · cheap, narrow

**Known:** the reference verifies `locking_mode = exclusive` by read-back; core
leaves the default.
**Unknown:** the per-transaction cost at core's commit frequency.
**Test:** same save, `locking_mode` set versus unset, count commits and wall time.
**Caveat:** an exclusive lock is a **scope** change — it must be reconciled with
the "one save owner" contract, and a Store opened read-only should not take it.

### O3 — producer pool (F1) · largest but gated

**Known:** the reference's shape is `min(available_parallelism(), 4)` producers
feeding a 4-slot × 512-object bounded queue, with **one** consumer.
**Unknown:** the serial fraction at the consumer, which is the whole question.
**Test:** measure `consumer_idle_ns`-equivalent behaviour in core first. If the
single-threaded consumer is never idle, producers would only deepen a queue that
already drains instantly, and the pool would buy nothing.
**Blocked by:** three things this study did not resolve — `TimingScope` being
`!Send`, locator determinism, and the `AGENTS.md` rule. Any proposal has to answer
all three, and the third may mean the answer is "in the Stage 7 adapter, not in the
product".

### O5 — multi-row INSERT (F8) · cheap and bounded

**Known:** the reference batches up to 128 rows per `INSERT` and issues 64
statements for an 8,191-row transaction; core issues one statement per row, 8,191
of them. The reference's chunk size is computed from SQLite's own limits
(`sql_rows`) rather than hardcoded.
**Unknown:** the per-statement overhead at core's row sizes.
**Test:** one save at the transaction ceiling, single-row against multi-row
insertion, holding everything else constant. `SaveOutcome.commits` and the timer
are sufficient instruments.
**Why it ranks high:** it needs no concurrency, no format change and no new
constant beyond a chunk size derived from `SQLITE_LIMIT_VARIABLE_NUMBER` and
`SQLITE_LIMIT_SQL_LENGTH` — the same derivation the reference already uses.

### O4 — group target (F5) · unclear direction

**Known:** core seals at 48 KiB, the reference fills to 65,536.
**Unknown:** the compression gain from larger groups against the pack-fit loss.
**Test:** retained bytes and `packs_created` at both targets on representative
content.

---

## 5. What must be measured before anything is changed

Stated as a precondition, not a suggestion:

1. **Is there a gap at all?** No receipt compares core against the v0.1.6
   reference on a matched workload. Core is unqualified; Stage 6 owns this.
2. **Where is it?** Core's counters locate time by phase — construction, admission,
   delta trials, SQL, pack assembly.
3. **Is the consumer ever idle?** This single number decides whether O3 is worth
   any further thought.
4. **Does spilling occur?** This decides whether O1 is real.

Until (1) answers, every item in §4 is a hypothesis about a gap nobody has
demonstrated.

---

## 6a. Corrections to earlier revisions of this study

An earlier revision, committed as `5e45897dd` and superseded here, contained one
substantive error. It is recorded rather than quietly fixed, because the mistake is
instructive.

**F4 retracted.** That revision claimed core was ahead on read batching —
"4,096 against 128, 32× fewer lookups". It conflated two different constants:
`READ_OBJECT_LIMIT` is a *demand ceiling* (how many ids C1 may name in one provider
call), while the SQL page size is `LOOKUP_PAGE_IDS = 128`, identical to the
reference's `OBJECT_PAGE_COUNT`. Core's `pages()` chunks by 128, so a 4,096-id
wave is 32 queries in both trees. §2.3 carries the corrected analysis.

The error came from reading core's own comment — "one wave is one grouped SQL
lookup" — as a description of the implementation rather than of the API surface.
The implementation pages. That comment is imprecise, and the study should have
checked `pages()` before making the claim.

Two findings were added after that revision, from the same re-examination: **F8**
(multi-row INSERT, a core regression) and **F9** (pack-body reads, a core
advantage). Neither was visible in the first pass.

## 6b. Explicit non-findings

- **No measurement was taken.** Every figure is a declared constant.
- **No performance claim.** A configuration difference is not an effect; the
  reference's larger cache is not evidence that core is slower.
- **No recommendation.** §4 lists opportunities with their unknowns; it selects
  none.
- **No durability claim.** Both trees keep `MEMORY` journaling and
  `synchronous = OFF` with no `fsync` anywhere. Nothing here proposes changing
  that, and `locking_mode` is not a durability setting.
- **The reference's parallelism is not asserted to be worth 4×.** Its own
  `SMALL_CONTENT_WORKERS = 4` is a cap, not a speedup.
