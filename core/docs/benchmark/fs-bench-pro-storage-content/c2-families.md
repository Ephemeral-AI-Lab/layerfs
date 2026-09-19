# Stage 6 · C2 benchmark families (`layerfs-storage`)

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Issue: [#183](https://github.com/Ephemeral-AI-Lab/layerfs/issues/183), under
> Stage 6 [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
> Sibling documents: [`c1-families.md`](c1-families.md),
> [`memory_cpu_space_support.md`](memory_cpu_space_support.md),
> [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md),
> [`gates_and_oracles.md`](gates_and_oracles.md).
>
> This is the case specification that must exist **before** benchmark
> implementation or sample collection.
>
> **No figure in this document is a measurement.** Every number is either a
> *declared constant read from source* or a *case configuration*. The only
> measured values quoted anywhere below are cited to an existing receipt and
> labelled diagnostic.

## 1. What "needs a database" does and does not mean

C2 owns one filesystem path, one persisted policy row, pack BLOBs and the SQLite
schema. Every family in this document therefore reaches SQLite — but **the DB is
only one of three cost centres**, and conflating them is how a campaign ends up
tuning the wrong one:

| Cost centre | Where it lives | Evidence |
| --- | --- | --- |
| **encode / hash / frame / assemble** | BLAKE3 identity re-verify, zstd level 3 (payload) / 19 (group), group framing, pack assembly — CPU + malloc | `SaveOutcome.full_records`, `packs_created`, `pack_appends` |
| **SQL statement + transaction shape** | one **multi-row `INSERT` per bound chunk** — the chunk is derived from the linked engine's own limits and capped at 128 rows (`sqlite/write.rs:109,119-133`), so the counter is charged where the statement is issued, not inferred from the row count; plus a transaction per 8,191 rows / 4 MiB−1 | `SaveOutcome.statements` (the counter an INSERT-batching change moves), `SaveOutcome.presence_queries` (batched to one query per wave), `SaveOutcome.commits`; **8,191 rows opens 31 transactions** |
| **read path** | ceiling read, paged locator `SELECT`, chain resolve, windowed decode | `StoreReadCounters{objects, packs_read, pages, ceiling, edges, max_depth, canonical_bytes, group_decodes, opens}` (`cas/store.rs:88-119`) |

Two facts constrain every family below:

- The persistence profile is `journal_mode = MEMORY`, `synchronous = OFF`,
  `temp_store = MEMORY`, zero busy timeout. **A save is not disk-bound**; the
  commit is page cache + malloc. Phase 0's P0-2 determined that
  **no cache spilling occurs** at 8,191 rows or at 4× that (a forced 8-page
  cache control proves the instrument live at 1,032 spills).
- `Store::create` is fixed-cost and large. Phase 0's diagnostic single sample for
  `measure_edits --mode c2 --case small` reports **`store.create` 4,606 µs of a
  6,952 µs save** (with `begin` 406 µs and `finish` 1,154 µs). Store creation
  must be its **own** case, or small saves measure SQLite's DDL.
  Source: `docs/roadmap/0.1/0.1.7/evidence/phase0-baseline-20260917T221759Z/receipts/p0-3-counter-baseline.md` §4 — diagnostic-grade, one sample.

**Excluded from every C2 family:** Workspace, Branch, Commit, LayerStack, FUSE,
POSIX, daemon, SDK, container, Monitor. C2 families accept canonical objects from
the harness directly and never run C1 file construction.

## 2. Why these families, and what was dropped

| Dropped | Reason |
| --- | --- |
| `dedup_branch_history` | Branch and history semantics (26 IDs — the largest single drop) |
| `branch_development`, `multi_workspace_development` | Branch forks / multiple Workspaces |
| `historical_access` | sealed v2 Store, mounted retained state (already `NOT_RUN`) |
| `repository_history` | history replay |
| `workspace_reliability` | fault injection — forbidden by `core/AGENTS.md` |
| `git_tool_workflow`, SDK-edit lineage | FUSE/POSIX/daemon/SDK surfaces |

Reason codes: **R1** Workspace/lifecycle · **R2** Branch/history/LayerStack ·
**R3** FUSE/POSIX/daemon/SDK · **R5** sealed artifact unavailable ·
**R6** fault injection.

## 3. The families

| # | Core family | Source family | Cases | Tier / config | DB touchpoint |
| --- | --- | --- | ---: | --- | --- |
| C2-1 | `c2.lifecycle` | *(none — build)* | 5 | fixed | `Store::create`, `Store::open`, `begin_save`, `finish`, `abort`; DDL + validate |
| C2-2 | `c2.reuse.cross-file` | `dedup_cross_file` | 10 | anchor + 3 profiles × {10,100,500} | paged membership `SELECT` (128 ids/page) + byte compare |
| C2-3 | `c2.delta.cdc-locality` | `dedup_cdc_locality` (delta half) | 20 + boundary set | 5 kinds × 4 tiers; boundaries `[0,1,8191,8192,16384,32768,32769]` | base-chain walk = pack BLOB reads (4 MiB cache) |
| C2-4 | `c2.reuse.workspace` | `dedup_workspace_reuse` (dedup half) | 14 | 3 kinds × 4 tiers + 2 `base128` controls | exact-hit lookup, no pack read |
| C2-5 | `c2.footprint` | `store_footprint` | 6 | 3 controls @100k f/500 MB, 3 low-v1 @100 f/5 MB and 10 f/10 MB | `object_packs.data`, `objects` rows, DB file size |
| C2-6 | `c2.delta.small-file` | `small_file_delta_smoke` | 4 | 1024 / 16384 / 65536 / **131071** bytes | policy outcome only; no pack read claim |
| C2-7 | `c2.read.waves` | `payload_create_read` (read half) | 4 | 1 / 10 / 100 / 500 MiB | ceiling + paged locators + chain resolve |
| C2-8 | `c2.pool.cold-warm` | `pooled lane` | 2 | 24 / 128 / 512 leaves × 100 rows | `metadata_value_groups` catalogue; `Store.pool_index` |
| C2-9 | `c2.pipeline.*` | `measure_edits`/`measure_filesystem --mode pipeline` | 4 | fixed cases | C1 construction + handoff + save ack |

### 3.1 Case IDs

```text
C2-2  dedup-cross-file-anchor-1,
      dedup-cross-file-{unique,identical,mixed}-{10,100,500}
C2-3  dedup-cdc-{overwrite,insert,delete,common-body,scattered}-{1,10,100,500}
      + boundaries(): lengths {0,1,8191,8192,16384,32768,32769} × seeds 1..3
C2-4  dedup-workspace-{exact,local,unique}-{1,10,100,500}[-compact-v2],
      dedup-workspace-unique-{1,10}-base128-v3        (base_file_count = 128)
C2-6  small-file-delta-{1024,16384,65536,131071}
C2-5  store-footprint-unique-100000,
      store-footprint-metadata-cardinality-100000,
      store-footprint-large-object-500m,
      store-footprint-unique-100-low-v1,
      store-footprint-metadata-cardinality-100-low-v1,
      store-footprint-large-object-10m-low-v1
C2-7  payload-random-read-{1,10}m-compact-v2, payload-random-read-{100,500}
C2-9  <mode> <case> ∈ {c1,c2,pipeline} × {small,chunked,small-to-large,large-to-small,batch}
```

### 3.2 Two corrections that land on `c2.footprint`

**`st_blocks` on a clone is a fabricated number.** A COW clone's allocated blocks are
**shared with the master**, so `store_allocated_bytes = st_blocks * 512` double-counts
and the O6 gate becomes meaningless. The repo says so twice: *"Copies/APFS clones are
not allocation controls"* (`0.1.4/issue88-delivery/contract-v1.md:174`) and *"APFS
clones/copies preserve content, not allocation equivalence"*
(`0.1.4/issue87-analysis:138`). Therefore the reflink rung is **forbidden for
`c2.footprint`** and for every row that gates allocated bytes; those rows must use the
byte-copy rung and carry `allocation_attribution: exclusive`. Logical fields —
`page_count`, `freelist_count`, `pack_bodies_bytes`, `store_apparent_bytes` —
remain valid on a clone.

**The footprint SQL can silently become a zero.** `SELECT COALESCE(SUM(length(data)), 0)
FROM object_packs` (lifted from `tests/memory_bounds.rs:280-282`) returns `0` after a
table rename — and `0 <= database` **passes** the O6 gate `pack_bodies <= database`.
`benchmark_rules.md:388` forbids exactly that. `space.py` must assert the table exists
(`SELECT 1 FROM sqlite_master WHERE type='table' AND name='object_packs'`) and must
**not** use `COALESCE`: a missing table is `INCOMPLETE`, never a zero.

## 4. Boundary ladder (each row two-sided)

| Surface | Constant | Value |
| --- | --- | --- |
| Format / schema | `FORMAT_PROFILE` / `SCHEMA_VERSION` / `APPLICATION_ID` | 1 / 4 / 1,279,677,261 |
| Write batch | `BATCH_OBJECT_LIMIT` / `BATCH_CANONICAL_BYTES_LIMIT` | **512 objects / 512 KiB** (bytes usually bind first) |
| Write transaction | `TRANSACTION_ROW_LIMIT` / `TRANSACTION_CANONICAL_BYTES_LIMIT` | **8,191 rows / 4 MiB − 1** |
| Read wave | `READ_OBJECT_LIMIT` | 4,096 (4,097 refused) |
| Lookup page | `LOOKUP_PAGE_IDS` | 128 |
| Group body | `GROUP_LIMIT` / `METADATA_GROUP_LIMIT` | 65,536 (65,537 refused) / 16 KiB |
| Group target | `GROUP_TARGET` | 48 KiB framed |
| Records / groups | `RECORD_COUNT_LIMIT` / `GROUP_COUNT_LIMIT` | 8,191 / 256 (singleton lane 1) |
| Pack | `PACK_LIMIT` / `SINGLETON_PACK_LIMIT` | 256 KiB / 16 MiB + 4,096 |
| Canonical object | `CANONICAL_LIMIT` | 16 MiB |
| Chains | `CHAIN_CANONICAL_LIMIT` / `CHAIN_ENCODED_LIMIT` | 512 KiB / 256 KiB |
| Metadata chains | `METADATA_CHAIN_CANONICAL_LIMIT` / `_ENCODED_LIMIT` | 65,536 / 139,281 |
| Delta depth | `DEFAULT_{WHOLE_FILE,CHUNK,METADATA}_DELTA_MAX_DEPTH` | 8 / 4 / 8 (accepted 0..=50) |
| Pooling | `VALUES_PER_GROUP` / `POOLED_LEAF_ROWS_LIMIT` | 165 / 100 |
| Pool index | `METADATA_INDEX_VALUES` | **131,072** + wholesale reset |
| Caches | `DEPENDENCY_PACK_CACHE_BYTES` / `POOLED_VALUE_CACHE_BYTES` / `DECODED_GROUP_CACHE_BYTES` / `METADATA_DECODED_WORK_LIMIT` | 4 MiB / 512 KiB / 512 KiB / 32 MiB |
| Match budget | `METADATA_MATCH_BUDGET_BYTES` | 128 KiB |
| Cleanup page | `CLEANUP_PAGE_ROWS` | 128 |

**Semantic edges that must be cases, not footnotes:** reading before `finish`
→ `VisibilityCeiling` (never silently visible); `begin_save` after a killed run
→ `UninspectedState`; a lost `COMMIT` → unknown outcome, quarantine, **never
resent**; cleanup mutates the artifact, so **arms cannot share a Store**.

## 5. Inherited measurement contract

As [`c1-families.md`](c1-families.md) §5, with two C2-specific additions:

1. **The Store is opened from a prepared copy — except for the fresh-run save
   family, where it is created inside the timed region.** Phase 0 declared
   creation-inside-the-region for the C2-only *supplied-object* family. It does
   **not** generalise: every edit, reuse, read and pool case measures against a
   base that must already be stored — a delta trial can only read a base that is
   stored — so those cases run `Store::open` over a per-sample byte copy of a
   prepared master. The case row carries the distinction as
   `store_state: created-in-sample | opened-from-copy`; without it, two cases that
   both say "save" measure different work.
   Preparation is therefore mandatory, not an optimisation: an unprepared edit
   case measures base construction rather than the edit. See
   [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md) §2.
2. **Store-owned state must be declared on every row:** the pooled index
   (`Store.pool_index`, `Arc<Mutex<PoolIndex>>`, mutated by every save and
   re-synchronized from the catalogue on the first pooled save after a reopen)
   and the publication watermark (`store_policy.retained_pack_ceiling`, advanced
   only in a save's final transaction). Cold and warm index rows **must not be
   pooled**.
3. **Custody of the prepared Store (review S2).** `journal_mode = MEMORY` is applied
   **per connection** (`src/sqlite/connection.rs:30-37`) and is not persisted in the
   file header, so a clone or copy cannot inherit a journal mode and no
   `-wal`/`-shm`/`-journal` sidecar can exist for it. The existing sidecar refusal is
   therefore necessary but **not sufficient**: it cannot detect a Store that is
   currently *open*. With `synchronous = OFF` a mid-transaction Store has partially
   written pages, and a copy taken then captures a torn page. Enforce quiescence
   structurally: build in `staging/`, close, `fsync`, atomic `rename` into
   `prepared/<digest>/`, `chmod` off `0o222`, and **never release the master path to a
   sample process** (`master_path_released_to_sample: false`).
4. **`prepare` has no producer today (review S1).** The artifact table in
   [`test_setup_and_cache_discipline.md`](test_setup_and_cache_discipline.md) §2 defines
   `objects/` and `store.sqlite`, but nothing in the design *builds* them — and
   construction is a product operation, so Python cannot. The child needs an
   `--emit-objects DIR` mode and a fresh-`--store` mode. No new file (they are the
   `construct` and `save` ops), but the `prepare` verb is unimplementable without them.

**Axis discipline:** report process heap, RSS, SQLite page cache and `.sqlite`
file bytes as **separate fields**. `synchronous = OFF` + `journal_mode = MEMORY`
makes a save page-cache/malloc work, so a growing file and a fast save are not
contradictory — pooling them reproduces the #151 B2 error (19 GB/s
cache-credited vs 2.1 GiB/s from storage).

### Selection lanes (new)

A full C2 run is **90 cases × one sample each** (86 before owner decision R5 added the
four `c2.delta.small-file` rows). Two lanes:

| Lane | Selection | Size | Use |
| --- | --- | ---: | --- |
| `--smoke` | the smallest legal tier of each family | ~9 | the ordinary development loop |
| full | every registered case | ~90 | admission |

**Pipeline is five cases, not a layer.** Integrated timing is required by #171's
acceptance, but it only means something where the C1→C2 handoff *is* the question —
the five `measure_edits` shapes plus one filesystem case. It is not applied to
`c2.read.*`, `c2.pool.*` or `c2.footprint`.

**Tier policy (decided).** The 100k-file / 500 MB `store_footprint` controls and the
500 MiB `c2.read.waves` tier are **declared on the ≤ 25 s exception list with their
measured wall times**, not cut and never shrunk to fit (owner decision R4). Cutting
them would remove the tier where the O(1) footprint claim is most convincing.

**Cardinality is frozen at 90** by [`CONTRACT.md`](CONTRACT.md) §3, including the four
`c2.delta.small-file` rows added by decision R5. A change needs a new contract stamp.

## 6. Vehicles: what exists, what must be built

| Vehicle | Status | Gap |
| --- | --- | --- |
| `measure_components --mode c2` | exists | fixture built outside the timed region; `--store` must be fresh |
| `measure_edits --mode c2` | exists | **a wiring probe, not a per-case benchmark** — all five cases truncate to the whole-file limit and apply one fixed 256-byte patch, so four of five arms write byte-identical stores |
| `measure_pooled` | exists | `--leaves` 1..4,096, `--rows` 2..200; one save per leaf |
| `memory_ledger` | exists | the only memory-instrumented C2 target |
| `phase0client c2 <rows> <profile>` | exists (in the Phase 0 evidence dir) | reaches the 8,191-row ceiling; 31 transactions |
| read-path vehicle with `opens` | **missing** | `StoreProvider::connection_opens()` exists; no vehicle prints it as a gate |
| `Store::create` / `open` isolation | **missing** | must be its own case |
| pooled cold vs warm | **missing** as cases | `memory_ledger` splits the phases but publishes no gate |

## 7. Optimization items this document deliberately does not decide

Ordering belongs to [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178),
whose argument is explicit: *"Tune the engine first and the tuning receipts
describe a profile that is about to change."* The families above exist so that
Phase 1 (algorithm/call pattern) and Phase 2 (DB/engine) can each be measured
against a stable profile. Read-path engine tuning (`cache_size`) **waits for
connection pooling** (P1-2) — the `opens` counter is its prerequisite.

**One measurable cost with no item against it.** Reading the whole #178 register
(P1-1..P1-16, P2-1..P2-8 and the parked list), `Store::create` has **no entry**.
Phase 0 recorded it as a fixed cost — a diagnostic single sample puts
`store.create` at **4,606 µs of a 6,952 µs small save** — covering DDL for four
tables and two indexes, `sqlite_master` scans, `PRAGMA table_info`, and watermark
validation. Any workload creating Stores at a moderate rate pays it in full, and it
is bounded, single-variable and measurable. Worth a Phase 2 item.

Two smaller findings these families are likely to surface, also absent from the
register. **No `VACUUM` exists anywhere in `core/`**, so the `.sqlite` never shrinks
after `abandon` — a space finding rather than a speed one. And the two
counter-attribution caveats an earlier revision listed here are **both resolved or
unreproducible** at the verification commit (see the errata pin in
[`CONTRACT.md`](CONTRACT.md)): `SortedWork.pages_read` no longer undercounts
batched merges (`sorted/page.rs:277`), and the "inner engine inside
`Engine::apply_root` returns its work to nobody" claim does not reproduce. A wrong
counter would still invalidate every row citing it, so a *new* Stage 6 counter must
be attributable at its charge site — but nothing here blocks a row today.

## 8. Explicit non-claims

- No absolute C2 latency, throughput or memory target exists today.
- No v0.1.7-vs-v0.1.6 ratio: only `component.primitives` is a legitimate matched pair; `pipeline.c2` is `NOT_RUN` because the reference's `WorkspaceAdmission` has no public method.
- `evidence for memory is declared-limits and live-ownership accounting, not RSS` — quoting the crate README; a total-RSS cap is not established.
- `work_exceeded` is a policy refusal, **not** a failure or a fallback.
- No worker-count, timeout or cache-policy relaxation to turn a miss green.

## 9. Dependencies and open decisions

- Where the harness lives, and whether it forks `core/benchmark/`.
- Whether the top tiers (100k files / 500 MB) are cut or declared ≤ 25 s exceptions.
- Whether a cold contract is needed at all for C2, or a declared warm in-process fixture state suffices.
- The two known harness defects shared with C1: `collect.py` overwrites its log path; command lists drift from the real arg parsers.
