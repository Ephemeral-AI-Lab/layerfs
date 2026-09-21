# Handoff: the algorithm gap against v0.1.6 — explore first, then choose a direction

> **Status:** handoff and commission. Filed from [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
> at `d7f1d8556`, after rounds 14–16 (ledger L72–L74). **It commissions an exploration, not an
> implementation, and it makes no performance claim of its own.** Every number below is sourced to a
> receipt, a counter or a `file:line`, and the ones that are inferences are labelled.

**The question as posed:** *why has v0.1.7 not reached the speed v0.1.6 had?* The owner wants the gap
explored at the **algorithm** level before another treatment is commissioned.

**What this document is for.** It gives the next agent (a) the state of the comparison as the receipts
now stand, (b) the two structural differences already found by reading, (c) the protocol that turns each
into a measurement rather than a belief, and (d) the ranked directions with the price of each. It does
**not** pre-authorise a change: the exploration runs first, on count-driven instruments, and the
direction is chosen from its result.

---

## 1. Read this first: the premise is now testable, and part of it is already refuted

`docs/roadmap/0.1/0.1.7/issue219-v016-gap-rca-handoff.md` (L69's round) already did the archaeology on
the **700 MB/s** figure and found it is `init_namespace`, not payload-create, that all 75
`init_namespace` receipts are `verification_status: NOT_RUN` with `cache_contract: null`, that the
100k rows span 279 ms to 105.9 s, and that a bare rate is a best-of. Nothing here repeats that.

What has changed since is the **row itself**, and it changes the question.

### 1a. Boundary-matched, the row is now level with the fastest reference row

The reference's timer *includes* reading its fixture and building the objects it admits. This row's
timer *excludes* that construction (C2's supplied-object rule) and includes the C1 tree build. The
excluded half is charged and published (`pipeline.construct_ns`, `pipeline.construct_noise_ns`), so the
comparison can be made on one boundary:

| | ms |
| --- | ---: |
| `operation_work_ns` (the row's formula, `ns19-Q2-repin-20260921T115200Z`) | **1218.9** |
| `+ construct_ns` (the product's `construct_bytes` over every planned file) | 449.3 |
| `+ construct_noise_ns` (the harness's byte generation, standing in for the reference's fixture read) | 113.8 |
| **boundary-matched work** | **1782.0** |
| boundary-matched **CPU** (CPU 1228.3 + the same two) | **1791.5** |

The reference's three same-shape rows are **1789.5 / 2116.7 / 2195.3 ms of CPU** (L69, from the six
`namespace-10000` receipts). **On a matched boundary the row is level with the fastest of the three, to
0.4 %**, and inside their band. L69 recorded the row at 2219.4 ms on the same basis and called it "1.4 %
beyond the worst of the three and 24 % behind the best"; rounds 10–16 removed **437 ms** of
boundary-matched work since.

**So the first deliverable is to test the premise, not to serve it.** Re-derive that table from the
receipts in `benchmark-results/issue219/` and the six reference rows, state the boundary you are using,
and if you reach a different conclusion, say so with the file and line. If the boundary-matched gap is
closed, the remaining question is a **design** question — *what does the reference do differently, and
is any of it still worth taking?* — and that is a different and more useful question than "why are we
slow".

### 1b. Where the row's own 1218.9 ms is, all of it measured

| block | ms | % of work | status |
| --- | ---: | ---: | --- |
| `diag_commit_total_ns` | **458.3** | 37.6 % | **byte-bound, closed as a lever**: 302 MB at 659 MB/s, and 95 COMMITs priced at **0.21 ms each** (L74) leave ~20 ms of transaction overhead in the whole term |
| `diag_insert_objects_ns` | **143.4** | 11.8 % | 7,666 statements for 25,245 rows = **5.68 µs per row**; L73 measured that a *wider* statement costs more per row (5.28 → 5.80 µs) |
| `diag_write_pack_total_ns` | **125.6** | 10.3 % | 7,873 seals; the native lane still groups **2.03 records** per group against a 64 KiB `GROUP_LIMIT` |
| `span_build_ns` (C1) | **90.1–121.8** | 7.4–10.0 % | `validate` is 4.9; the named build phases are 18; **~70–100 ms is uncharted** |
| `profile_full_ns` | 69.1–72.0 | 5.7 % | 42.1 is the stored-frame probe's own 15,977 calls (L72) |
| `diag_wave_ns`, `diag_validate_ns`, `diag_begin_ns` | 115.2 | 9.5 % | |
| group/place/resolve/delta | ~36 | 2.9 % | |
| teardown (excluded from the formula) | 75–175 | — | the OS's price for dirtied pages; measured at 6.7 → 469.9 ms on identical source |

Serial floor (one writer at a time): **842.9 ms**. Cumulative against the clean tree: **−65.0 %**.

---

## 2. The algorithm gap: two structural differences are already visible by reading

The reference is `crates/layerfs-layerstack-store` — the ancestor of `core/`, and it is the **same
physical family**: `object_packs(pack_id INTEGER PRIMARY KEY, data BLOB)` with `objects` as a
`WITHOUT ROWID` locator table, groups and records. It is not a different algorithm at the top level.
The differences that exist are in the **schema's keys** and in what the save path writes beside the
payload. Two of them are concrete and unmeasured:

### H1 — a locator insert maintains **two** B-trees here and **one** in the reference

| | reference (`crates/layerfs-layerstack-store/sql/schema/v7.sql:9-16`) | this Store (`core/crates/layerfs-storage/sql/schema.sql:66-77`) |
| --- | --- | --- |
| `objects` key | `object_id BLOB PRIMARY KEY` — **32 bytes** | `PRIMARY KEY (object_id, save_id)` — 32 + 8 |
| `object_packs` | `pack_id`, `data` | `pack_id`, `data`, **`save_id`** |
| secondary indices in the whole schema | **0** (`grep -c "CREATE INDEX" v7.sql` = 0) | **3**, one on this table: `CREATE INDEX objects_save ON objects(save_id, object_id)` |

The table is `STRICT, WITHOUT ROWID`, so its primary key **is** its B-tree — and this Store declares a
second index on the same table, which every one of the **25,245** locator inserts maintains as well.
The reference maintains one B-tree per row; this Store maintains two, on a key that is also wider by the
8-byte `save_id`. That column is there because the multi-writer work (#216) gave each private save its
own row identity before publication, and `objects_save` serves the save-scoped lookups that model needs
— so this is a **contract**, not an accident, and it is the only structural difference found so far that
sits directly on the **143.4 ms** insert term (25,245 rows at 5.68 µs each).

*(A discrepancy worth a line in the exploration's notes: `REQUIRED_INDEXES` in
`core/crates/layerfs-storage/src/sqlite/schema.rs:95` is documented as "**Empty by owner ruling C**"
while the constant lists three names and `sql/schema.sql` creates them. One of the two is stale. Do not
"fix" either without reading ruling C and #188 first.)*

### H2 — the reference's save path has a spill, and a coalescing batch session

`crates/layerfs-layerstack-store/src/objects/spill.rs` exists, and
`src/objects/admission.rs:1321-1358` drives `session.begin_batch(...)` / `commit_pending(...)` with
`final_batch` and a `coalesce` flag. **Read them before assuming our cadence is worse**: L74 has now
priced a COMMIT at 0.21 ms, so cadence is a small lever, but *where the payload lives* is not small at
all — if the reference's payload bytes go to a spill file rather than into SQLite, then its commit is
cheap by construction and the 458.3 ms is the price of a design it never paid. **That would make the
two numbers non-comparable rather than unfavourable, and it must be settled first.**

### The rest of the schema difference, for completeness — and its price

This Store writes, per save, a `saves` row (private-slot ownership and publication), a
`store_policy` row read and written by every transaction, `content_signatures` (the admitted-FULL
winner cache) and `metadata_value_groups` (pooled ordinals and the retained window). The reference has
none of the four. Each is a *statement* and a *dirty page* on the write path, and the counters for them
already exist on the row: `profile_sql_ns` 261.4 ms contains them. **Count them; do not assume they are
small and do not assume they are large.**

---

## 3. The protocol

The rules of this lane apply unchanged: **one sample per case per arm, one difference per arm,
register before you run, fresh `--out`, never retune a receipt, no best-of.** The exploration below is
mostly *diagnostic*, and a diagnostic must be labelled one and must measure its cause on a
**count-driven instrument** (statements, calls, bytes, µs per call, pages read) rather than on another
sample of the same arm.

1. **Re-derive §1a** from the receipts and state the boundary. Deliverable: a table with one row per
   receipt, `NOT_MEASURED` for any cell without evidence.
2. **Settle H2's spill question by reading**, then by measurement if the answer is "payloads are in the
   database": quote the file and line, and say what bytes the reference hands to the pager per canonical
   byte. This is the fork in the road: if the reference spills, §1a's comparison needs a *third* column
   and the direction changes.
3. **Price H1 on a count-driven instrument.** A labelled diagnostic that inserts the row's own row
   shape (25,245 locator rows, the same columns) into otherwise identical tables and reports **µs per
   row** and **pages dirtied** for three shapes: the reference's (one 32-byte-keyed `WITHOUT ROWID`
   table, no secondary index), this Store's (`(object_id, save_id)` plus `objects_save`), and this
   Store's with the secondary index dropped. That is a measurement of the mechanism, not of an arm, and
   it is what decides whether a schema change is worth its contract.
4. **Only then** choose a direction and register a treatment the established way.

### What must not be redone

| closed | why |
| --- | --- |
| codec level, stored frames, page size, cache-in-pages, journal modes | refuted in L67/L72 and the page size is owner-ruled |
| transaction cadence | L74: a COMMIT costs **0.21 ms**; the term is byte-bound |
| "more workers" | `AGENTS.md` §3.8; and the handoff's arithmetic: the ceiling is the floor, now 842.9 ms |
| whole-file lane granularity, ordinal reservations | landed in L73/L74 |
| the 700 MB/s figure | L69: it is `init_namespace`, `NOT_RUN`, `cache_contract: null` |

---

## 4. The directions, ranked, with what each is worth

| # | direction | price | why this order |
| --- | --- | ---: | --- |
| 1 | **H2 first, because it can invalidate everything else**: find out whether the reference's payload bytes reach the pager at all | decision, not ms | if they do not, the matched-boundary comparison is the wrong frame and the row is already ahead of a design that never paid for its own storage |
| 2 | **H1, the second B-tree and the wider key on `objects`**, if step 3 prices it | ≤ 100 ms, unmeasured | two B-trees per locator row against the reference's one; the contract it touches is #216's private-row identity, which the mandate permits changing with an argument |
| 3 | **the native lane's 2.03 records per group** (`GROUP_LIMIT` 64 KiB, `GROUP_TARGET` 48 KiB, `PACK_LIMIT` 256 KiB raised together) | ~40–60 ms | the same lever that paid −81.6 ms on the whole-file lane in L73, and it is a policy-constant change |
| 4 | **the build span's uncharted ~70–100 ms** | unknown | it is now the largest block with no attribution at all, and rounds 10–13 turned two cheap instruments into a −207.5 ms treatment |
| 5 | the insert **statement shape** (L73's sign problem: the per-row scalar `SELECT save_id FROM temp.layerfs_read_scope`, and a statement text that varies with every chunk width) | ~20–60 ms | cheap, already diagnosed, and it is a *regression* the lane introduced in L73 |

**The honest summary for the owner.** On a boundary-matched basis the row is no longer behind v0.1.6 —
it is level with the fastest of the three comparable rows, and the 437 ms that closed the gap came from
rounds 10–16, not from the harness. What remains of the v0.1.6 question is therefore narrow and
answerable: *does the reference put its payload bytes in the database at all, and why does it maintain
one B-tree per locator row where this Store maintains two?* Both are readable in an afternoon, and the second is measurable on the insert term in
an hour. Everything else on the road to 1 s — the last −219 ms to the floor and the floor itself — is
this product's own, and §1b says where it is.
