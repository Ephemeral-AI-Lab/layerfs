# #219 — single-thread optimization of `pipeline-namespace-10000`: an open mandate

> **Status:** handoff. Filed after the attribution campaign of 2026-09-21. The attribution is
> **done and verified**; the optimization is **not started**. Three treatments were attempted and
> all three were refuted. **Nothing has landed.**
>
> **This document grants authority, not just context.**

---

## 0. Your mandate, and the authority that comes with it

**Optimize `pipeline-namespace-10000` iteratively, on your own judgement. Do not ask the human
for authorization, for a ruling, or for a preference.** Where this document or any other one says
"needs an owner ruling", treat that as **already granted**. Make the call, implement it, measure
it, and defend it with data.

**Treat this as an experiment. You are allowed to do anything to this tree**, including:

- change the Store format, bump `SCHEMA_VERSION`, change the pack layout, change the schema;
- change capacities, policy constants, pack geometry, buffer policy or cadence;
- restructure the write path, the read path, the commit model or the placement rules;
- change the harness, the run driver or the registry;
- accept that a measurement arm produces a different store, a different file hash or a different
  pack count.

**What is still required is not permission — it is labelling.** Three rules survive because they
are about honesty, not authority:

1. **Register before you run.** Write down the one difference, the identity you compare against,
   the expected movement in the instrument's own units, and what would refute it, *before* the
   run. A change you did not register is a **diagnostic** and must be labelled one.
2. **Never claim a verification you did not do.** A format change must be a *versioned* change
   with both directions of the read path handled — either old packs still read, or old formats
   explicitly refused with a clear error. Run the full suite before quoting any number, and state
   exactly which commands ran and which did not.
3. **Never retune, relabel or promote a historical receipt.** Receipts are append-only. Fresh
   `--out` every run. No best-of, no re-running until a number looks right.

The repo runs **no CI and no aggregate gate**; there is nothing between you and a corrupt store
except your own verification. That is a reason to verify, not a reason to hesitate.

---

## 1. Where you are

| | |
| --- | --- |
| worktree | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000` |
| branch / HEAD | `codex/219-ns10000` at `9c46930b8` |
| product tree | **clean** — `git diff --stat -- core/crates/` is empty |
| tree change | one file: `core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs`, **+78 uncommitted lines** of instrumentation (see §4) |
| evidence | `docs/roadmap/0.1/0.1.7/evidence/issue219-squad{A,B,C,D,E}-*/` — five directories, ~3,000 lines |
| measurement lock | per worktree: `core/benchmark/fs-bench-pro-storage-content/.measurement.lock`. Two runs in one worktree never overlap; other worktrees are independent and may run concurrently (record overlap as declared interference) |

**The single most complete baseline is**
`core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-squadA-packcounters-20260921T044814Z/pipeline-namespace-10000/receipt.json`.
It carries the profile, the pack counters and the pack counters' own instrumentation. Compare
against **it**, not against the older `ns17-pinned2` row, unless you need the pinned row's store.

---

## 2. What is already established — **do not re-derive any of this**

### 2.1 The attribution

`pipeline-namespace-10000`, one sample, on the baseline row:

| term | ns | share of accept span |
| --- | ---: | ---: |
| `operation_ns` | 3,351,043,917 | — |
| `accept_span_ns` | 3,347,582,625 | 100 % |
| `commit_ns` | 1,145,315,268 | **34.2 %** |
| `sql_ns` | 796,535,871 | **23.8 %** |
| `full_ns` | 245,777,362 | 6.95 % |
| `place_ns` | 79,188,910 | 2.24 % |
| `group_ns` | 18,859,112 | 0.53 % |
| `resolve` | 5,858,368 | 0.17 % |
| `delta_ns` | 82,960 | 0.002 % |
| **remainder** | **1,050,422,310** | **31.4 %** |
| CPU (user+system) | 3,307,937,000 | CPU/wall 0.987 |

Counters: `commits` 17,378 · `inserted` 25,245 · `packs_created` 1,250 · `pack_appends` 15,552 ·
`statements` 16,595 · `presence_queries` 398 · `full_records` 25,241 · `prefix_records` **4**.

**Two statements are ~60 % of the row:** `COMMIT` (17,378 × 63.6 µs) and
`UPDATE object_packs SET data = ?2` (the pack rewrite). Everything else is small.
**Decode is 0.70 %.** The workload writes FULL records — delta is unused.

### 2.2 The mechanism, measured

`append_pack` (`core/crates/layerfs-storage/src/sqlite/write.rs:79-89`) binds the **entire** new
pack body, because the pack directory is at the front and grows, so every append moves every body
behind it (`cas/placement.rs:203-205` states this in the product's own words).
`pack/layout.rs:115` — `directory_is_starts_only()` is true for `PackLane::WholeFile` **alone**,
and that is the lane with the worst amplification.

**Measured by parsing all 1,250 pack directories out of the pinned row's own `sample.sqlite`:**

| lane | packs | writes | writes/pack | bytes written | bytes persisted | factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| **WholeFile** | 102 | 9,444 | 92.59 | 1,260,350,294 | 24,613,232 | **51.21×** |
| **PooledMetadata** | 3 | 207 | 69.00 | 25,584,656 | 726,096 | **35.24×** |
| Native | 1,142 | 7,130 | 6.24 | 1,004,612,191 | 276,058,548 | 3.64× |
| Ordinary | 3 | 21 | 7.00 | 2,318,196 | 625,356 | 3.71× |
| **all** | **1,250** | **16,802** | 13.44 | **2,292,865,337** | **302,023,232** | **7.5917×** |

**2.0 GB of the 2.29 GB is pure rewrite** — bytes handed to the pager that the Store does not keep.
Two lanes are 8 % of the packs and 53 % of the rewritten bytes.

### 2.3 The two invariants that are the wall

Both were found by implementing a treatment and watching a declared property break. **They are the
reason nothing has landed.**

1. **Pack sharing.** `core/crates/layerfs-storage/tests/cas_reuse.rs:232-261` asserts that two
   whole-file objects saved in one operation **share a pack** and that reading both reads **one**
   pack. Any scheme that gives a group its own pack is a contract change. *(This refuted PR-A1.)*
2. **Intra-save delta candidacy.** `tests/delta_payload.rs:74-98` asserts that an object admitted
   **earlier in the same save** must be usable as a delta base by a later one, with no predecessor
   supplied by the caller. Only a *placed* row can be a base, so **the locator row cannot be
   deferred**. *(This refuted the full batching treatment, PR-C1b.)*

**Together: the pack write and its locator rows are not merely I/O — they are the act that makes
an object part of the save.** You cannot defer them and you cannot split them. Any treatment that
reduces the rewrite must therefore change **what is written**, not **when**. That is why the format
is the lever.

### 2.4 Already eliminated — **do not spend time here**

| candidate | status | how it was settled |
| --- | --- | --- |
| transaction cadence alone | bounded at **3.9–6.4 %** | synthetic replica at the product's pragma profile; the bound prices only the fixed per-transaction cost |
| `journal_mode = MEMORY` | **refuted** — 0.927× `OFF` | synthetic; the memory journal costs nothing at this shape |
| page-cache size | **refuted** — flat 2 MiB→128 MiB | synthetic, and the product **never sets `cache_size`** at all |
| pack / blob size | **refuted** — larger blobs are *faster* | synthetic at 4 KiB→4 MiB |
| locator / presence queries | **refuted at product level** | a guard skipped all 25,245 probes; `sql_ns` moved **12 µs of 796 ms**; store came out byte-identical |
| `AUTOINCREMENT` / `sqlite_sequence` | not on the commit path | `saves` went 1→2 across 17,378 commits |
| the historical 15.5 µs `ORDER BY … LIMIT` clause | **absent from this tree** | `ORDER BY o.object_id,o.save_id LIMIT ?` exists nowhere |
| positional delta encoding | unused | `prefix_records` = 4 of 25,245 |
| chunking | **cleared, byte-exact** | the `1-8 / 32-256 / 1024-8192` bands are **weights, not byte sizes**; the realized ladder is 79–627 / 2,505–20,035 / 80,684–640,537 B in **both** arms |
| compression / codec | not implicated | `full_ns` 7 %, `group_ns` 0.5 % |

### 2.5 The comparison arm is not the same product

The v0.1.6 row (`init_namespace`, 944.9 ms wall / 1,788.8 ms CPU / 73 transactions) lives in a
**different workspace** (`crates/` vs `core/`) and its receipt records **no product seal or image
on the v0.1.7 side at all** — the two cannot be shown to be the same product from any raw field.
Both rows do the same work (25,158 vs 25,245 objects; 302,182,831 vs 302,231,057 bytes); treat the
comparison as **two implementations**, not one product measured twice. The v0.1.6 timer also
**includes** scanning 300 MB (of which only 0.73 MB came from disk) while the v0.1.7 timer
excludes all construction under the C2 supplied-object rule.

---

## 3. The levers, ranked by expected yield

### L1 — the pack format. ~2.0 GB. **This is the one.**

The rewrite exists because the directory is at the front and grows. Make an append write only the
new bytes. Three shapes, in increasing invasiveness:

- **reserved directory space** — allocate a fixed directory region sized for
  `group_count_limit()`, so bodies never move and an append writes one directory entry plus one
  body;
- **directory-last with a fixed trailer** — bodies first, directory at the end, trailer pointing
  at it;
- **one row per group** — replace the single pack blob with per-group rows so an append is an
  `INSERT` and never a rewrite.

Whichever you choose: bump `SCHEMA_VERSION` (`policy.rs:43`), handle old packs (read them, or
refuse them explicitly), and remember that SQLite stores the pack as **one BLOB in one row** —
partial writes need `zeroblob` + incremental blob I/O, and reads of `SELECT data` would then
materialise the whole capacity unless the read path also uses blob I/O. Work that through before
you start.

### L2 — the 31 % remainder, which no bucket names. Unattributed, and the largest unexamined mass.

Known contents, all **uncharged** by `SaveProfile`:

- `validate_candidates` (`cas/collision.rs`) runs **after** the `insert_objects` charge closes, so
  it lands in the remainder whatever it costs;
- the `ObjectRow` vector construction in `seal_group`;
- the caller's per-object work.

**Look here first, it is cheap.** In particular: the pipeline driver calls
`content.cloned_object(*id)` **once per content object, 24,863 times, inside the timed region**
(`ops/pipeline.rs:822-830`). If that is a deep copy of the canonical bytes it is **~300 MB of
memcpy plus 24,863 allocations inside the timer** — harness bookkeeping, not product work. Verify
it; if so, move the object out of the content store instead of cloning it, and say plainly that
you improved the *measurement* rather than the product. Also instrument the uncharged regions so
the remainder stops being a mystery.

### L3 — batch the placements (PR-C1). Registered, not run.

A lane frames groups as records arrive, holds them bounded, and places the whole run in one
`select_many` + one pack write + one `COMMIT`. **Format-neutral.** Registered in
`docs/roadmap/0.1/0.1.7/evidence/issue219-squadC-cadence-20260921T044258Z/pre-registration.md`.

Two things the registration gets wrong, both found by implementing it:

- it says "no new constant", but a bound is mandatory and **no existing bound is safe**:
  `capacities.transaction_rows` is 8,191 and `batch_objects` is 512, against a current step of
  1–2 objects;
- its upper bound of 2,800 commits does not follow from its own mechanism (5 lane-tail steps per
  wave × 577 waves + 783 fixed = **3,668**).

**And it collides with §2.3 invariant 2** unless you also solve intra-save delta candidacy — the
writers who implemented it watched `delta_payload.rs` go red for exactly that reason. If you
take this route, you must either keep the winner cache able to propose a *unplaced* object (and
make it readable), or flush before every selection. Neither is trivial.

### L4 — pack geometry, via policy. ~12 %, with a trap.

Lowering the fill target keeps packs shared while shortening each rewrite. At 128 KiB the
arithmetic is ~930 MB removed (~12 % of the row). **But**:

```
policy.rs:95     PACK_LIMIT = 256 * 1024          // lane_body_limit(WholeFile) = pack_limit
encoding/full.rs:219   PackLane::WholeFile => capacities.pack_limit
schema.sql       small_file_threshold_bytes CHECK (BETWEEN 131072 AND 1048576)
cas/placement.rs if body > body_limit && lane != Singleton -> CapacityExceeded
```

Whole-file records are bounded by `small_file_threshold_bytes`, which is **configurable up to
1 MiB**. Lowering `PACK_LIMIT` below a store's threshold makes that store **refuse its own
whole-file objects**. Handle it — clamp the threshold to the pack limit, or route oversized
whole-file records to `Singleton` — before you touch the constant. And expect roughly double the
pack rows.

### L5 — anything you find. You are not limited to this list.

---

## 4. The instrument you inherit

The +78 uncommitted lines in `core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs`
publish what the product already computed and the driver never wrote:

- `pipeline.profile_*` — the seven `SaveProfile` buckets, their `resolve` parts, the total and
  `profile_reuse_repeat`, using the same names `history` publishes as `delta.profile_*`;
- `pipeline.accept_span_ns` — a harness `Instant` pair from just after `begin_save` to just after
  `finish`, the denominator the remainder is defined against;
- `pipeline.packs_created`, `pack_appends`, `statements`, `presence_queries`, `full_records`,
  `prefix_records`.

None is pinned in `tests/golden/expected.tsv` and none affects the pinned-counter gate. **It is
uncommitted and it is yours to commit, extend or rewrite.** Note that `SaveProfile`'s own doc at
`cas/owner.rs:60-62` **overstates**: `BEGIN IMMEDIATE` and `ROLLBACK` are charged nowhere
(93.4 ms of `BEGIN IMMEDIATE` sits outside every bucket), and `validate_candidates` is charged to
nothing at all.

---

## 5. Method that still binds

- **Single thread is the owner's direction for this product.** Do not raise
  `LAYERFS_CONSTRUCTION_WORKERS`, add a second lane or add helper threads to move a number. The
  parallelism term is *intended* and out of scope; optimization is inside one thread.
- **Do not relax a limit to turn a miss into a pass** — no inflating timeouts, no enlarging a
  cache or a spool window so a phase stops paying for its own work. *Changing a limit as a
  deliberate engineering trade, pre-registered and measured, is different and is allowed.*
- **A warm cache must never credit a timed phase.** Declare the cache state; never pool cold and
  warm rows.
- **Budgets:** a performance selection's complete command is **≤ 15 s**, with a small declared
  exception list to 25 s; verification is typically under 15 s in a 60 s hard budget. The baseline
  row is **4.8 s**. A selection that cannot fit is recorded `NOT_RUN` with its wall time — never
  made to fit by moving work outside the timer or shrinking the workload.
- **One sample per case per arm.** No n3, no best-of selection. Diagnostics are welcome and must be
  labelled.
- **Production LOC:** every commit reports `Production LOC: <before> -> <after> (delta <signed>)`
  with the counting method, computed from the exact parent and committed tree. Test, docs and
  tooling changes do not contribute; a docs-only commit reports the unchanged total and delta 0.
- **Never patch, vendor or fork a third-party crate.** No `[patch]`/`[replace]`, no edits under
  `~/.cargo`. Builds stay `--locked`.
- **Never claim durability the contract does not provide.** The connection profile is
  `journal_mode = MEMORY`, `synchronous = OFF`, `busy_timeout = 0` and it is a deliberate
  no-fsync design (`sqlite/connection.rs:1-7`). You may experiment with it — but if you change it,
  report it as a **durability change**, not an optimization.
- **Report FAIL, INCOMPLETE and unrun work as plainly as PASS.** No push may claim "CI green".

---

## 6. Mechanics — exact commands

```sh
WT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000
HB=$WT/core/benchmark/fs-bench-pro-storage-content

# product tests (30 binaries; the full suite was green on the clean tree)
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml -p layerfs-storage

# the invariants that matter most for any write-path change
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml -p layerfs-storage \
  --test cas_reuse --test delta_payload --test pack_watermark --test multi_writer \
  --test visibility --test persistence_failure --test pack_locator

# harness build (repo-root .cargo/config.toml supplies the aarch64 AEAD flags; run from inside the repo)
cargo +1.85.1 build --release --locked --manifest-path $HB/Cargo.toml

# one measured row — fresh --out every time, never an existing path
cd $HB && python3 runner.py perf --lane smoke \
  --out benchmark-results/issue219/<your-name>-<UTC> \
  --case pipeline-namespace-10000 --verify full --no-build
```

The row is **PASS** when 11+ gates pass, all 14 pinned counters reproduce and
`digest:filesystem_root` is `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`.
Reference hash: the baseline row's `sample.sqlite` is
`03918d61a9f004b292deafeadbea6992f870b8bdd96b3a631d4bc3309408db29`, **byte-identical to the pinned
row's** — a useful check that a change stored nothing differently.

---

## 7. What "done" looks like

- **A measured movement on `operation_ns` or CPU**, on a pre-registered treatment, with the row
  still PASS, the filesystem root unchanged, and the invariant tests green — or a refutation
  recorded as carefully as the three that already exist.
- **Either the 7.5917× amplification is materially reduced, or the reason it cannot be is
  established** with the invariant or the measurement that says so.
- **The 31 % remainder is attributed** by name, or every attempt to attribute it is recorded with
  what it cost.
- **Every claim is sourced** to a raw file, a receipt field, a `file:line`, or marked
  `NOT_MEASURED`. The prior campaign's reviewer found 17 arithmetic errors and two bad rows; assume
  yours will too, and make its job possible by keeping raw output beside every claim.
- **Evidence in the repo's own style:** `docs/roadmap/0.1/0.1.7/evidence/<topic>-<UTC timestamp>/`
  with `README.md` and raw files beside it. Never overwrite an existing evidence file.
- **A status comment on #219** with the numbers and the gaps. Close nothing on this handoff alone.

---

## 8. Where everything already is

| what | path |
| --- | --- |
| baseline row (use this) | `core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/ns17-squadA-packcounters-20260921T044814Z/` |
| the pinned row and its byte-identical store | `.../ns17-pinned2-20260921T031259Z/` |
| v0.1.6 comparison arm | `benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl` |
| bucket split, phase matrix, four refuted levers, the mechanism, errata | `docs/roadmap/0.1/0.1.7/evidence/issue219-squadA-profile-20260921T044041Z/README.md` (20 sections, 1,114 lines) |
| ranked statement table, `EXPLAIN QUERY PLAN`, plan defects | `.../issue219-squadB-sql-20260921T043911Z/README.md` |
| cadence unit, exact commit decomposition, test constraints, PR-C1 | `.../issue219-squadC-cadence-20260921T044258Z/README.md` |
| chunking cleared, byte-exact closures, timer boundaries | `.../issue219-squadD-chunk-20260921T044229Z/README.md` |
| independent re-derivation and every arithmetic failure found | `.../issue219-squadE-review-20260921T045622Z/README.md` + `rederive.py` |
| the three refuted treatments | `.../issue219-squadA-profile-20260921T044041Z/preregistration-PR-A{1,2}*.json` and README §20 |
| prior art: the same amplification on another lane, and why the cadence is a contract | `.../stage-6-history-209-rca-20260920T191016Z/` |

---

## 9. The shortest honest summary

The attribution is finished and it is not in doubt: **60 % of this row is the database write path,
and 2.0 GB of the 2.29 GB it hands the pager is a whole-pack rewrite caused by a directory that
grows at the front of the blob.** Everything else measured this campaign is noise or already
eliminated. Two declared invariants block every placement- and cadence-level workaround, which is
why the only lever left is **what gets written**, and that means the format.

You have the authority to change it. Go and find out what it is worth.
