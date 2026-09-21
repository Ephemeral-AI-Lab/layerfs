# Squad A — `SaveProfile` for the v0.1.7 row, and the phase matrix (#219)

> **Status:** Squad A deliverable. One **diagnostic** row, produced by a harness-only
> instrumentation change. **No product source changed, no treatment was applied, no
> performance claim is made.** The row exists to publish an instrument the pinned row
> did not publish; it is *not* a re-measurement of the pinned row and the two are not
> pooled.
>
> This directory also records **seven corrections to the handoff's premises**, each
> sourced to a raw field. They are listed in §7 and should be read before the
> campaign's numbers are quoted.

## 1. What was run, and why it is a diagnostic

The pinned v0.1.7 row
(`benchmark-results/issue219/ns17-pinned2-20260921T031259Z`) records `"profile": ""`.
That field is the **case registry's** fixture-profile column, empty for this row — not
`SaveProfile`. The product already accumulates `SaveProfile` and returns it on
`SaveOutcome.profile` (`core/crates/layerfs-storage/src/cas/lifecycle.rs:216`,
`cas/store.rs:73`); the pipeline driver simply never wrote it to the trace.

Changed: **one file, benchmark tree only** —
`core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs`, 55 inserted lines,
in `fn namespace_scale` only. Three additions:

1. the seven buckets and the `reuse_repeat` count, published as `pipeline.profile_*`
   counters, using the same names the `history` driver already publishes as
   `delta.profile_*` (`src/ops/history.rs:2616-2652`);
2. `pipeline.profile_total_ns`, the instrument's own seven-bucket sum;
3. `pipeline.accept_span_ns`, a **harness** `Instant` pair from immediately after
   `begin_save` returns to immediately after `finish` returns. It exists because
   `SaveProfile::total_ns`'s own documentation defines the remainder as `total_ns`
   against the accept span (`cas/owner.rs:145-152`) and nothing in the row measured
   that span. It is deliberately **not** named `storage.accept_loop`: the shipped
   `timing.json` carries no span for this region and this is not the product's node.

None of these counters is pinned in `tests/golden/expected.tsv`; a wall-clock
observation cannot be a frozen constant. The pinned-counter gate is unaffected by
construction (`src/main.rs:627-650` compares only the row's pinned keys), and
`golden_matches()` compares the registry table, not counters.

| | |
| --- | --- |
| command | `python3 runner.py perf --lane smoke --out benchmark-results/issue219/ns17-squadA-profile-20260921T044041Z --case pipeline-namespace-10000 --verify full --no-build` |
| harness binary sha256 | `71d7c40ce1d35cd1997e20a87f8298c98357cb4e5179acbb2c49e5a3457d552c` |
| source commit | `9c46930b846600e5f3c6ca4a4c4cbcf44ecdc356`, `source_dirty: true`, one dirty file (the driver above) |
| product seal | unchanged — no `core/crates` file was touched |
| result | **PASS**, 13 gates, 14 of 14 pinned counters reproduced, replay root reproduced |
| wall | 4.79 s complete command, inside the 15 s budget |
| declared interference | a build/measurement in `layerfs-190-scope` was running concurrently; per `AGENTS.md` §3.5 the lock is per-worktree, so this is recorded as declared interference rather than prevented |
| raw | `diagnostic-receipt.json`, `diagnostic-run.json`, `counters.tsv`, `shares.tsv`, `diagnostic-sha256.txt` beside this file |

## 2. The bucket split

Denominators are both published: `pipeline.accept_span_ns` = 3,536,155,750 and the
row's `operation_ns` = 3,538,935,458 (the span is 99.92 % of the operation). Source:
`shares.tsv`, derived by `shares.tsv`'s generator from `counters.tsv`.

| bucket | ns | share of accept span | share of operation |
| --- | ---: | ---: | ---: |
| `commit_ns` | 1,351,360,518 | **38.2155 %** | 38.1855 % |
| `sql_ns` | 781,237,539 | **22.0928 %** | 22.0755 % |
| `full_ns` | 245,777,362 | 6.9504 % | 6.9450 % |
| `place_ns` | 79,188,910 | 2.2394 % | 2.2376 % |
| `group_ns` | 18,859,112 | 0.5333 % | 0.5329 % |
| `resolve.pooled_ns` | 5,858,368 | 0.1657 % | 0.1655 % |
| `delta_ns` | 82,960 | 0.0023 % | 0.0023 % |
| **seven buckets** | **2,482,364,769** | **70.1995 %** | 70.1444 % |
| **remainder** | **1,053,790,981** | **29.8005 %** | 29.7771 % |

`resolve.eligible_ns`, `resolve.acquire_ns`, `resolve.cost_ns` and
`resolve.reuse_ns` are **all zero**; the whole of `resolve` is `pooled_ns`.
`reuse_repeat` is 0 — the `LAYERFS_STORAGE_REUSE_PROBE` was **not** set, so the
probe was off and the count is structurally zero, not a measurement of no repeats.

**Derived, and labelled derived:**
`sql_ns + commit_ns = 2,132,598,057` = **60.3084 %** of the accept span.
`resolve_ns + delta_ns + group_ns = 24,800,440` = **0.7013 %**.

### 2.1 The mechanism this names

**The database term is 60.3 % of the v0.1.7 operation; decode (resolve + delta +
group) is 0.70 %.**

The commit term alone is 38.2 %, and it is 17,378 commits:
`1,351,360,518 / 17,378 = 77,762.7 ns` per commit, against
`25,245 / 17,378 = 1.4527` object rows and
`301,171,810 / 17,378 = 17,330.6` content bytes per commit.

The comparison arm committed **73** times for 25,158 objects at a maximum
transaction of 4,149,860 bytes. The v0.1.7 row's average transaction is ~17.3 KB.
**The transaction-size ratio is ~239x and the commit count ratio is 238x.**

This is measured, not assumed — the handoff's §3.2 asked that cadence's share be
measured before it is called the gap. It is 38.2 % directly. It is *not* durability:
the connection profile is `journal_mode = MEMORY`, `synchronous = OFF`,
`temp_store = MEMORY`, `busy_timeout = 0`
(`core/crates/layerfs-storage/src/sqlite/connection.rs:1-48`), and no product path
calls `fsync`. The cost is engine and page-flush work.

The handoff's §3.1/§3.2 ranking (database first) is therefore confirmed by
measurement, and §3.4's remaining suspect — decode/scan — is refuted for this row:
**0.70 %**.

## 3. The phase matrix (§4.1), every cell sourced or `NOT_MEASURED`

v0.1.6 = `benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl`
(`kind: "sample"`, `records[0]`). v0.1.7 = the diagnostic row above.

| phase | v0.1.6 `init_namespace` | source | v0.1.7 `pipeline` | source |
| --- | ---: | --- | ---: | --- |
| content/fixture construction | 2,733.5 ms | `preparation.fixture.fixture_generate_ns` 2,733,511,125 | **`NOT_MEASURED`** — constructed before the timer by C2's supplied-object rule; no phase field exists | `src/ops/c2.rs:1-22` |
| fixture plan + manifest | 17.8 ms | `fixture_plan_ns` + `fixture_manifest_ns` | `NOT_MEASURED` | — |
| acquisition | `NOT_MEASURED` — no field | — | 8.1 ms | `acquisition_wall_ns` |
| preparation, total | 4,200.8 ms | `preparation_wall_ns` | 913.2 ms | `preparation_wall_ns` |
| setup / store open | 250.0 ms, **outside** the timer | `setup_ns` | 3.6 ms, **inside** the timer (`store.open` 2.3 + `storage.begin` 1.3) | `timing.json` |
| **timed operation** | **944.9 ms** | `layerstack_init_ns` | **3,538.9 ms** | `operation_ns` |
| — scan of the 300 MB input | inside the timer: `scanned_bytes` 300,000,000, `scanned_files` 10,000, disk read 0.73 MB | `records[0]` | **excluded by design** | `src/ops/c2.rs:1-22` |
| — build / encode | `NOT_MEASURED` | — | 349.9 ms = `full_ns` + `delta_ns` + `group_ns` + `resolve_ns` + `place_ns` | `counters.tsv` |
| — SQL | `NOT_MEASURED` | — | 781.2 ms | `pipeline.profile_sql_ns` |
| — commit / publication | `NOT_MEASURED`; 73 admission transactions | `initialize_admission_transactions` | 1,351.4 ms; 17,378 commits | `pipeline.profile_commit_ns`, `pipeline.commits` |
| — caller per-object remainder | `NOT_MEASURED` | — | 1,053.8 ms | derived: accept span − seven buckets |
| — product timing children | `NOT_MEASURED` | — | 11.2 ms of 3,538.9 ms = **0.32 %**; the other 99.68 % has no product timing node | `timing.json` |
| handoff | `NOT_MEASURED` — internal to the product | — | 41.7 ms | `handoff_ns` |
| teardown | 249.1 ms, outside the timer | `teardown_ns` | `NOT_MEASURED` — no field | — |
| verification | `NOT_RUN` | `verification_status` | 338.7 ms | `verification_wall_ns` |
| cleanup | 319.1 ms | `cleanup.wall_ns` | 459 ns | `cleanup_wall_ns` |
| product command window | 1,457.4 ms | `command_wall_ns` | 4,841.6 ms complete command | `complete_command_ns` |
| sample / run wall | 6,152.5 ms | `wall_ns` | 4,841.6 ms command / 5,045.4 ms run | `phases`, `run.json` |
| cache declaration | `reused-first-sample-uncontrolled`, `cache_contract: null` | `records[0]`, header | `prepared-dewarmed`, 0 resident pages | `trace.jsonl` seq 8, gate `g4.residency` |
| construction workers | 1 (namespace init is the sanctioned multi-worker exception) | `fixture_worker_count` | 1 | `identity.construction_workers` |

**The two timers do not cover the same phases** and the matrix is the evidence for
that. The v0.1.6 timer **includes** scanning 300 MB and excludes setup/teardown; the
v0.1.7 timer **excludes** all construction and includes `store.open`.

Both rows measure a **native macOS host process**: v0.1.6's
`initialization_{user,system}_cpu_ns` are `getrusage(RUSAGE_SELF)` deltas of the
host Store process and the Linux container contributed 12.0 ms of 1,788.8 ms
(`../issue219-s2-attribution-20260921T031259Z` §6). So the container is **not** a
confound; the report records this because it looked like one.

## 4. The target, restated from the measured numbers

| term | v0.1.6 | v0.1.7 diagnostic | ratio |
| --- | ---: | ---: | ---: |
| operation wall | 944,880,958 ns | 3,538,935,458 ns | 3.745x |
| CPU user + system | 1,788,767,834 ns | 3,286,170,000 ns | 1.837x |
| CPU / wall | 1.8934 | 0.9286 | — |
| canonical objects | 25,158 | 25,245 | 1.0035x |
| canonical bytes | 302,182,831 | 302,231,057 | 1.0002x |
| transactions | 73 | 17,378 | 238.05x |

Two notes the campaign should carry:

- **The CPU gap is a lower bound.** Both timers are compared as they are, but the
  v0.1.6 timer *also* scans and hashes 300 MB that the v0.1.7 timer does not. Charging
  that work to v0.1.6 would lower its 1,788.8 ms and *raise* the ratio above 1.837x.
  The scan's CPU share is `NOT_MEASURED` and no attempt is made to estimate it here.
- **Both canonical totals now agree to 0.016 %** (48,226 bytes), once the right
  quantities are compared — see §7.2.

If the database term could be driven to the v0.1.6 transaction geometry, the
arithmetic that matters is `2,132,598,057` ns of `sql_ns + commit_ns` against a
3,538,935,458 ns operation: **60.3 % of the row sits in the term whose geometry is
238x away from the comparison arm's.** Whether that term is reducible at fixed
objects and bytes is **not** measured here; it is Squad C's pre-registration.

## 5. What the instrument does not name

The **remainder is 29.8 %** (1,053,790,981 ns). `SaveProfile`'s own documentation
says the caller's per-object work — presence validation, group assembly outside the
codec, `raw_payload` — is outside all seven buckets and is reported as the remainder
(`cas/owner.rs:29-43`). That is what this is: **named as unreported by the
instrument, not attributed to a cause.**

## 6. Not measured, and named as such

- the remainder's internal composition;
- the product's SQLite `cache_size` / `cache_spill` / `mmap_size` for this row —
  `StoreFacts` reads them (`cas/store.rs:523-526`) but the receipt publishes none;
- the v0.1.6 scan's CPU share, and therefore the like-for-like CPU gap;
- any spread: this is **one sample**, and the diagnostic row's `operation_ns` is
  1.30 % below the pinned row's while its `CPU` is 2.04 % below, which is the only
  variation figure the campaign has;
- the 3-way forced split's cost (`MAXIMUM_WALK_ENTRIES = 4096`,
  `core/crates/layerfs-content/src/filesystem/limits.rs:86`) — `pipeline.batches` is
  3 and `pipeline.largest_batch_bindings` is 4096, but no arm without the ceiling was
  run.

## 7. Corrections to the handoff's premises

Each is sourced; none is a reinterpretation.

1. **Identities are not matched.** §1 states "Both arms: source `b0260df3a`
   (`dirty=false`)". The v0.1.6 row is `b0260df3a2ffc371773cd062feafd4b5e435bf1e`,
   `LAYERFS_SOURCE_DIRTY: "false"`. The v0.1.7 row is
   **`2a63aff0d0a24743deebb9129ab14e098d9e24ed` with `source_dirty: true`** and five
   dirty files (`run.json`, `identity.source_dirty_files`). `b0260df3a` is an
   ancestor of the campaign branch, but it is not the row's source commit. The
   product seal `b3e3cb7453…` and the image `sha256:d152ced8…` do match.
2. **The byte comparison is apples-to-oranges.** §1 lists v0.1.7 "canonical bytes
   301,171,810". That is `pipeline.content_bytes` — the content stream only
   (`trace.jsonl` seq 24, basis "canonical bytes of the content objects accepted by
   the save"). The row's own canonical total is
   `resources.space.canonical_bytes_total: 302,231,057`, which is **48,226 bytes
   (0.016 %) above** v0.1.6's `store_canonical_bytes: 302,182,831` — sixteen times
   closer than the "within 0.33 %" the handoff claims, and in the opposite direction.
3. **The SQLite page-cache evidence is on the other row.** §3.1 says
   `sqlite_t1_page_cache_overflow_peak_bytes` and
   `sqlite_t1_connection_cache_used_bytes` "are recorded per row". They are recorded
   on the **v0.1.6** row (543,040 B and 34,604,032 B against a 33,554,432 B target);
   the v0.1.7 receipt publishes neither.
4. **`profile: ""` is not an empty SaveProfile.** It is the case registry's
   fixture-profile column (`src/main.rs:504`), empty for this row. §1 and §3.4 read it
   as the instrument's absence, which happens to be true but for a different reason.
5. **The pinned row carries a non-passing line the handoff omits.**
   `ns17-pinned2-20260921T031259Z/run.json` records
   `registry_self_check.status: "FAIL"`, `exit_code: 1`, with the frozen cardinality
   array expecting `[…, 2, 4]` and the registry rendering `[…, 2, 5]`. The row itself
   PASSed; this is runner bookkeeping, and it is reported here because §6 of the
   handoff binds this work to report every non-passing line plainly.
6. **The handoff's target arithmetic is right to within rounding.**
   `1,788.8 / 0.94 = 1,902.9 ms`; at the diagnostic row's measured 0.9286 the same
   arithmetic gives `1,926.5 ms`. The 1.88x claim holds; the divisor should be the
   row's own ratio.
7. **"The two arms live in different workspaces" understates it.** They also use
   different harnesses (`fs-benchmark-pro` vs `fs-bench-storage-content`), different
   receipt schemas (`layerfs-perf-v1` vs `layerfs-core-receipt-v1`), different cache
   contracts and different timer sets. The matrix in §3 is the only like-for-like
   surface, and it is not complete.

## 8. Gate

> *Gate:* every bucket and the remainder attributed, or a bucket the product does not
> report is named as unreported; the phase matrix carries a source or
> `NOT_MEASURED` in every cell.

Met. All seven buckets and `reuse_repeat` are published and sourced. The remainder is
**named as the instrument's declared unreported region**, not attributed. Every matrix
cell in §3 carries a field name or `NOT_MEASURED`. §6 lists what is not measured, and
§7 lists the seven premise corrections.

## 9. Bounding the cadence lever (labelled synthetic diagnostic)

`commit_ns` is 38.2 % and is 17,378 commits, so the obvious treatment is fewer
transactions. Before registering one, the fixed cost of a transaction was priced at
the product's own pragma profile on a synthetic replica of the same schema, holding
bytes constant (256 MiB written in every arm, only the `BEGIN IMMEDIATE`/`COMMIT`
count varied). Source: [`cadence-calibration.json`](cadence-calibration.json).

| transactions | wall ns | ns / transaction |
| ---: | ---: | ---: |
| 8,192 | 550,620,792 | 67,214 |
| 1,366 | 454,763,917 | 332,916 |
| 74 | 444,408,875 | 6,005,525 |

**Derived bound.** The fixed per-transaction cost is
`(550,620,792 − 444,408,875) / (8,192 − 74) = 13,084 ns` from the extremes, and
`(454,763,917 − 444,408,875) / (1,366 − 74) = 7,959 ns` from the middle segment. The
two disagree because the relationship is not linear over this range and every arm is
one sample, so both are reported. Applied to this row's 17,378 commits that is
**138.3–227.4 ms, i.e. 3.9–6.4 % of the 3,538,935,458 ns operation.**

**What this says.** The product's measured `77,762.7 ns` per commit is ~5.9x the
~13.1 µs fixed cost. **The majority of `commit_ns` is page-flush work proportional to
the bytes written, which fewer transactions do not remove** — a large transaction
still flushes the same dirty pages. Cadence is real but is *not* the 38.2 % lever its
share suggests, and this is exactly the "measure its share before proposing a change"
the handoff's §3.2 asked for.

**Caveat, stated plainly.** One sample per arm; a synthetic replica with one table,
no secondary index, no foreign key exercised on the written table, and only 8,192
rows against the product's 25,245 plus three indexes. It **bounds** the cadence term.
It does **not** decompose the product's `commit_ns`, and it is not a product row.

### 9.1 What this leaves open

`sql_ns + commit_ns = 2,132,598,057 ns` (60.3 %) is then mostly *the price of
putting 302 MB plus 25,245 indexed rows through the pager*, not the price of the
transaction boundary. The open question is therefore not "fewer commits" but **why
that write costs ~142 MB/s at the product's profile**. Candidates the campaign has
not yet separated, all `NOT_MEASURED`:

- `journal_mode = MEMORY` keeps an in-memory copy of every original page for
  rollback, so each dirty page is touched twice;
- `foreign_keys = ON` makes every `objects` insert do FK lookups against `saves` and
  `object_packs` (`sqlite/schema.rs`), 2 per row over 25,245 rows;
- index maintenance on `objects_save`, `signatures_save` and `packs_save`;
- the page cache: `StoreFacts` reads `cache_size`/`cache_spill`/`mmap_size`
  (`cas/store.rs:523-526`) but **this row's receipt publishes none of them**, so the
  campaign cannot currently say what the cache was.


## 10. Work identity: the diagnostic row does the same work as the pinned row

The pre-registration rule asks for one difference per arm and the identity it is
compared against. This diagnostic changes **only** the harness instrumentation, so the
comparison is that no *work* counter may move. Script and raw output:
[`work-identity-check.py`](work-identity-check.py),
[`work-identity-check.txt`](work-identity-check.txt).

| check | result |
| --- | --- |
| counters published by both rows | 15 |
| of those, **moved** | **0** |
| counters only in the diagnostic | 15 — all `pipeline.profile_*` / `pipeline.accept_span_ns`, i.e. the instrumentation itself |
| counters only in the pinned row | 0 |
| `resources.space.canonical_bytes_total` | 302,231,057 in both |
| `resources.space.canonical_objects_total` | 25,245 in both |
| `resources.space.canonical_objects` by role | identical |
| gates | 13 PASS / 13 PASS; both rows `status: PASS` |
| identity differences | exactly four: `harness_binary_sha256` (`cb21593d…` → `71d7c40c…`), `source_commit` (`2a63aff0…` → `9c46930b…`), `started_utc`, and the dirty-file list (5 files → 1) |

This is the campaign's one clean controlled contrast: **same product seal, same image,
same workload, same work counters, one harness difference.** The pinned2 dirty-file
list also confirms §7.1 — the pinned row was taken with five modified files present,
three of them golden tables.

### 10.1 The two rows' wall and CPU, read as one pair

| | pinned2 | diagnostic | delta |
| --- | ---: | ---: | ---: |
| `operation_ns` | 3,575,832,667 | 3,538,935,458 | −1.03 % |
| `cpu_user_ns` + `cpu_system_ns` | 3,354,412,000 | 3,286,170,000 | −2.04 % |
| `process_peak_rss_bytes` | 524,845,056 | 524,222,464 | −0.12 % |

Two identical-work rows differ by 1.03 % of wall and 2.04 % of CPU. **This is the
only spread figure the campaign has for this case**, and it is a lower bound on the
real spread — the two rows are 14 minutes apart on the same machine. Any treatment
claim smaller than ~2 % of `operation_ns` is inside that gap and cannot be resolved
by one sample per arm.


## 11. The two other named candidates, priced and eliminated

§3.1 of the handoff named database operations first, and §9 left four candidates open.
Two of them are now priced on the same synthetic replica and **both are eliminated**.

### 11.1 The MEMORY rollback journal — refuted

[`journal-calibration.json`](journal-calibration.json). Same replica, same preamble, only
`PRAGMA journal_mode` varied:

| journal_mode | wall ns | MB/s |
| --- | ---: | ---: |
| MEMORY (the product's) | 554,603,625 | 484.0 |
| OFF | 598,162,833 | 448.8 |
| DELETE | 2,226,541,709 | 120.6 |

**MEMORY is 0.927x OFF** — measured slightly *faster* than having no journal at all, so the
in-memory rollback journal costs nothing at this shape. DELETE's 4.01x is a statement about
on-disk journaling; the product does not use DELETE, and moving it to OFF would remove the
runtime rollback atomicity `sqlite/connection.rs:3-7` deliberately retains. **Do not read
this row as a licence to change the pragma profile.**

### 11.2 Page-cache size — refuted, and the product never set one

[`cache-calibration.json`](cache-calibration.json). Throughput is **flat** from 2 MiB to
128 MiB of cache:

| `cache_size` KiB | dense (8,192 txns) MB/s | batched (64 txns) MB/s |
| ---: | ---: | ---: |
| −2,000 | 482.6 | 485.9 |
| −8,192 | 482.5 | — |
| −32,768 | 473.4 | 598.7 |
| −131,072 | 476.7 | — |

A 64x cache increase buys nothing. The handoff's §3.1 instruction to read the page-cache
counters before any page-size treatment is therefore discharged: **there is no page-size
treatment to make on this workload.**

The calibration also produced a product fact the campaign needs. **The v0.1.7 Store never
sets `cache_size`.** `sqlite/connection.rs:32-48` sets journal_mode, synchronous,
temp_store, foreign_keys and busy_timeout only; `cache_size` appears in the crate as
`Pragma::CacheSize`, *read for evidence* (`connection.rs:94,114`) and published onto
`StoreFacts` (`cas/store.rs:182,524`). The row runs SQLite's default page cache, and the
33,554,432-byte connection-cache target quoted from the v0.1.6 family **is that harness's
own field and does not describe this product** — a further reason the two rows must not be
pooled.

### 11.3 What survives

| candidate | status | how it was settled |
| --- | --- | --- |
| transaction cadence | **bounded at 3.9–6.4 %** | §9, synthetic |
| MEMORY rollback journal | **refuted** | §11.1, synthetic |
| page-cache size | **refuted** | §11.2, synthetic + source |
| foreign keys / index maintenance on `objects` | **NOT_MEASURED** | §9.1 |
| the 29.8 % remainder (caller per-object work) | **NOT_MEASURED** | §5 |

The synthetic floor for the pack-body write alone at the product's profile is ~480 MB/s.
The product's `sql_ns + commit_ns = 2,132,598,057 ns` moves 302 MB plus 25,245 indexed rows
at ~142 MB/s. **The residual is therefore ~3.4x above the measured floor for the bytes
alone**, and it is not the journal, not the cache and mostly not the cadence. That is the
campaign's open question, stated as a number rather than a suspicion.

### 11.4 A premise correction from Squad D

`../issue219-squadD-chunk-20260921T044229Z/` shows the handoff's §3.3 fixture description is
wrong in kind, not merely in detail: `1-8 / 32-256 / 1,024-8,192` are **relative weights**
(`benchmark/fs-bench-pro/workload/main.rs:424-430`), not byte bands. The realized ladder in
**both** arms is tiny 79–627 B, small 2,505–20,035 B, medium 80,684–640,537 B and one
100,000,000 B anchor. Squad D's own arithmetic closes the v0.1.7 role byte totals **to the
byte**, which is stronger than the handoff asked for. Its "chunking is cleared" verdict was
reached without Squad A's profile and is unchanged by it; only its "all SaveProfile values
`NOT_MEASURED`" line is superseded, by §2 of this report.


## 12. Pack size — refuted, and the residual stated as a factor

[`blob-calibration.json`](blob-calibration.json). Same replica, 268,435,456 bytes in every
arm, transaction count held at 1,024, only the blob size varied:

| blob bytes | rows | txns | insert ns | commit ns | total ns | MB/s | commit share |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 4,096 | 65,536 | 1,024 | 167,406,197 | 427,690,472 | 595,096,669 | 451.1 | 0.719 |
| 32,768 | 8,192 | 1,024 | 71,359,606 | 386,769,747 | 458,129,353 | 585.9 | 0.844 |
| 262,144 | 1,024 | 1,024 | 39,529,091 | 356,697,966 | 396,227,057 | **677.5** | 0.900 |
| 1,048,576 | 256 | 256 | 34,454,666 | 356,822,231 | 391,276,897 | **686.0** | 0.912 |
| 4,194,304 | 64 | 64 | 246,311,538 | 147,970,501 | 394,282,039 | 680.8 | 0.375 |

**Throughput rises with blob size.** At the product's own average pack of 243 KB the floor
is 677.5 MB/s. Shrinking packs would make the row slower, not faster, and pack size is not
the explanation.

### 12.1 The residual, as a number

Scaling the 243 KB arm to the row's 302,231,057 bytes:
`396,227,057 × 302,231,057 / 268,435,456 = 446,106,000 ns` (derived).
The product's measured `sql_ns + commit_ns = 2,132,598,057 ns` is
**`2,132,598,057 / 446,106,000 = 4.78x`** the measured floor for the same bytes at the same
pragma profile. The residual is **1,686,492,057 ns**.

### 12.2 Attribution ledger for the 60.3 % database term

| candidate | verdict | evidence |
| --- | --- | --- |
| transaction cadence | bounded at **3.9–6.4 %** | §9 |
| MEMORY rollback journal | **refuted** (0.927x OFF) | §11.1 |
| page-cache size | **refuted** (flat 2→128 MiB) | §11.2 |
| pack / blob size | **refuted** (larger is faster) | §12 |
| per-commit engine cost | **not anomalous** — synthetic 51,279 ns/txn vs product 77,763 ns/txn, 1.52x, and the product's commit also runs `advance_pack_if_moved` | §12, §9 |
| per-row metadata work — `objects` WITHOUT ROWID on a 32-byte random key + `objects_save`, `content_signatures` + `signatures_save`, `metadata_value_groups` + UNIQUE, two foreign keys | **NOT_MEASURED — the surviving candidate** | §9.1 |
| the 29.8 % caller remainder | **NOT_MEASURED** | §5 |

**Read this as the campaign's answer to the handoff's §3.1.** The database term is first —
60.3 % — but the four cheapest explanations the handoff ranked under it are all now
measured and three are refuted while the fourth is bounded at a sixteenth of its apparent
size. What survives is the **per-row metadata structure**, which is the one candidate no
synthetic replica in this report models, and which the campaign should attack next.


## 13. The mechanism: the pack-append rewrite, and a correction to §9

Squad B's statement table
([`../issue219-squadB-sql-20260921T043911Z/ranked_table.txt`](../issue219-squadB-sql-20260921T043911Z/ranked_table.txt))
puts a statement this campaign had not examined second in total cost:

| rank | statement | calls | ns/call | total ms |
| ---: | --- | ---: | ---: | ---: |
| 1 | `COMMIT` (`sqlite/write.rs:52`) | 17,378 | 63,583 | 1,104.9 |
| 2 | **`UPDATE object_packs SET data = ?2 …`** (`sqlite/write.rs:82`) | **16,802** | 27,208 | **457.1** |
| 3 | `SELECT o.object_id,… WHERE o.object_id IN (?) …` | 25,245 | 7,083 | 178.8 |
| 4 | `BEGIN IMMEDIATE` | 17,378 | 5,375 | 93.4 |
| 5 | `INSERT INTO objects (…)` | 16,595 | 4,250 | 70.5 |

### 13.1 What row 2 is

`append_pack` binds **the entire new pack body** —
`sqlite/write.rs:79-89`. The product says why, in its own words:

> *"Writing a pack moves every body in it (the directory grows), so every cache that
> holds those bytes is invalidated whenever this save writes"* —
> `cas/placement.rs:203-205`. The same lifetime constraint is documented at
> `cas/owner.rs:365`.

The store has **1,250 pack rows** and Squad B measured **16,802** `UPDATE` calls
against them: **14.44 writes per pack**, each rewriting the pack's whole current body.

**Derived amplification.** For a pack of final length `L` rewritten `k` times with equal
increments the bytes written are `L·(k+1)/2`, so the amplification is `(k+1)/2`. At
`k = 14.44` that is **7.72x**: approximately **2.34 GB rewritten to persist 302 MB**. This
is *derived* — the store keeps only final pack lengths, so the intermediate lengths cannot
be read back.

### 13.2 Correction to §9

§9 bounded the cadence lever at 3.9–6.4 % by pricing the fixed per-transaction cost on a
replica that **inserted each blob exactly once**. That replica did not model
`append_pack`, so the bound is correct only for the transaction overhead itself and **is
not the total cadence lever**. The number is not withdrawn — it is the right price of the
thing it measured — but it must not be quoted as the cadence lever.

`pack_appends` (16,802) and `commits` (17,378) sit within 3.4 % of each other. **If an
append is issued once per open lane tail per transaction, the commit cadence and the pack
write amplification are the same lever**, and the cadence hypothesis is far stronger than
§9 could see. That linkage is a **hypothesis, not established**, and
[`pack-append-synthesis.json`](pack-append-synthesis.json) records how to refute it: read
`packs_created` / `pack_appends` and `profile_sql_ns` together under a cadence treatment.

### 13.3 Does it reconcile §12?

§12 left **1,686,486,638 ns** unexplained against a 677.5 MB/s floor for 302 MB. At the
full 7.72x amplification the rewritten bytes would be ~2.34 GB, implying ~1,385 MB/s —
*faster* than the floor, so the full amplification cannot be present. But a substantial
fraction of it can be, and this is the first candidate that puts the residual in the right
order of magnitude. **The arithmetic is a bound on plausibility, not an attribution.**

### 13.4 Why this is the handoff's own prediction

The handoff's §3.1 named exactly two targets under database operations: *"the locator/
presence queries and the pack-append rewrite"*, with the precedent that `63fa15c49` fixed a
single dominant clause on another lane. The pack-append rewrite is real, it is second by
total statement cost, it is structural rather than tunable, and it is the only candidate
this campaign has found that can carry the residual. **The locator/presence query (rank 3,
25,245 calls, 7,083 ns/call, 178.8 ms) is real but third.**

### 13.5 What a treatment must respect

`cas/placement.rs:203-225` and `cas/owner.rs:365` make the invalidation constraint
load-bearing: a stale pack cache is read through bytes that are not the pack, and
`pack::layout::group_view` refuses it as `Integrity("group ordinal")`. Any coalescing
treatment must keep every cache that holds pack bytes invalidated whenever the pack is
written. The natural candidate — hold the tail and issue one `UPDATE` per pack per
transaction, or write the body once at seal — is **not** implemented, **not** measured, and
**not** pre-registered here; it is the campaign's next step. A pack format whose directory
does not move would be a **Store-format change and needs an explicit owner ruling**.


## 14. The amplification, on the row's own counters

A second diagnostic row (`ns17-squadA-packcounters-20260921T044814Z`) publishes the
counters `SaveOutcome` already carried and this driver never wrote. Same one-file harness
change, same command, fresh `--out`. Raw:
[`counters-packcounters.tsv`](counters-packcounters.tsv),
[`packcounters-receipt.json`](packcounters-receipt.json),
[`packcounters-analysis.txt`](packcounters-analysis.txt),
[`packcounters-sha256.txt`](packcounters-sha256.txt).

### 14.1 The counters

| counter | value | what it settles |
| --- | ---: | --- |
| `pipeline.packs_created` | 1,250 | pack rows the save created |
| `pipeline.pack_appends` | **15,552** | whole-body rewrites of an existing pack |
| `pipeline.statements` | 16,595 | object-row `INSERT` statements |
| `pipeline.presence_queries` | **398** | presence queries for offered objects' direct references |
| `pipeline.full_records` | 25,241 | FULL representation |
| `pipeline.prefix_records` | **4** | delta representation |

### 14.2 Two independent methods agree to the unit

| quantity | Squad B, derived from source + microbenchmark | this row, from the product |
| --- | ---: | ---: |
| pack `INSERT` + `UPDATE` calls | 16,802 | `1,250 + 15,552 = ` **16,802** |
| `INSERT INTO objects` calls | 16,595 | `statements = ` **16,595** |

Two methods that share no input agree exactly on both. **Squad B's statement table is
confirmed by the product's own counters**, and the campaign may quote it as measured
rather than derived.

### 14.3 The write amplification, now measured input and derived factor

`15,552 / 1,250 = 12.4416` appends per pack. For a pack of final length `L` written
`1 + k` times with equal increments, the bytes written are `L·(k+2)/2`; the amplification
is therefore **6.72x (using `(k+1)/2`) to 7.22x (`(k+2)/2`)**, i.e.
**2,031,234,488 to 2,182,350,016 bytes rewritten to persist
302,231,057 bytes.** The increment sizes are not known to be equal — packs are sealed at a
target — so the factor is **derived under an equal-increment assumption**; the counts
`1,250` and `15,552` are measured.

### 14.4 Two handoff suspicions refuted by the same row

- **Presence queries are not the cost.** §3.1 ranked "the locator/presence queries" first
  among the SQL suspects. This row answers **398 presence queries** for 24,863 offered
  content objects — one per 62 objects. Whatever the locator costs, it is not a
  per-object presence lookup.
- **Delta is not the cost.** `prefix_records = 4` of 25,245, against `full_records =
  25,241`, and `profile_delta_ns` measured 82,960 ns. The handoff's §3.3 chunking
  question and this both land the same way: the workload writes FULL records.

### 14.5 Spread, and which term is noisy

| | row 1 (`…T044041Z`) | row 2 (`…T044814Z`) | delta |
| --- | ---: | ---: | ---: |
| `operation_ns` | 3,538,935,458 | 3,351,043,917 | −5.31 % |
| CPU | 3,286,170,000 | 3,307,937,000 | +0.66 % |
| `profile_sql_ns` | 781,237,539 | 796,535,871 | +1.96 % |
| **`profile_commit_ns`** | **1,351,360,518** | **1,145,315,268** | **−15.25 %** |
| `profile_total_ns` | 2,482,364,769 | 2,297,160,315 | −7.46 % |

Both rows are work-identical (15 shared counters, 0 moved). **`commit_ns` is by far the
noisiest term**, swinging 15.25 % — seven times the operation's own swing. Any treatment
claim about `commit_ns` smaller than ~15 % cannot be resolved by one sample per arm, and
a first review of this campaign's §2 commit share should treat "38.2 %" as
"34–38 % over two samples".



## 15. Reconciliation: the amplification carries the database term

### 15.0 A correction to this report's own first version

This section was first written claiming the amplification inflates `sql_ns` but **not**
`commit_ns`, on the reasoning that rewriting a pack row in place re-dirties the same pages
and SQLite flushes only the pages dirty at `COMMIT`. **That reasoning was wrong**, and Squad
B's §7.6 is what falsifies it:

> *"one whole-pack-body rewrite per 1.52 inserted objects ... the consequence is measured,
> not inferred: **2,292,865,337 bytes of pack BLOB written for 302,023,232 bytes of final
> pack data (7.59×)**, i.e. ≈132 KB of dirty pages per commit ... which is what makes a
> `COMMIT` cost 63.6 µs on a profile with no fsync in it."* —
> [`../issue219-squadB-sql-20260921T043911Z/README.md`](../issue219-squadB-sql-20260921T043911Z/README.md) §7.6

The flaw was mine: the rewrites are spread **across 17,378 separate transactions**, so each
transaction dirties ≈132 KB of pack BLOB and flushes it at *its own* `COMMIT`. The pages are
re-dirtied and re-flushed per transaction rather than accumulating, so the flush work scales
with the rewritten bytes, not with the final size. The first version of this section is
superseded in full and is recorded here rather than deleted.

### 15.1 The arithmetic, against the rewritten bytes

| quantity | value | source |
| --- | ---: | --- |
| final pack data | 302,023,232 B | `receipt.resources.space.pack_bodies_bytes` |
| pack BLOB written | **2,292,865,337 B** | Squad B §7.6, derived from group sizes |
| amplification | **7.59x** | derived |
| dirty pages per commit | ≈132 KB | 2,292,865,337 / 17,378 |

| | row 2 |
| --- | ---: |
| `profile_sql_ns` | 796,535,871 |
| `profile_commit_ns` | 1,145,315,268 |
| **database term** | **1,941,851,139** |
| implied rate over 2,292,865,337 rewritten bytes | **1.18 GB/s** |
| implied rate over 302,231,057 final bytes | 156 MB/s |

**1.18 GB/s of blob construction plus pwrite is a plausible rate** for a connection with
`journal_mode = MEMORY` and `synchronous = OFF` (no fsync), and it is consistent with the
§9 fixed cost of 13.1 µs per transaction (17,378 × 13.1 µs = 228 ms of the 1,941.9 ms, the
rest being bytes). **The whole 60.3 % database term now has one mechanism that covers it:**
the pack-append rewrite moves ~7.6x the payload, and the row pays for every one of those
bytes twice — once in `sql_ns` when the blob is built and bound, and once in `commit_ns`
when the dirty pages are flushed.

This also reconciles §12. §12's residual of 1,686,486,638 ns was unexplained against a floor
for 302 MB; against 2.29 GB the term is no longer anomalous. §12's synthetic inserted each
blob exactly once, which is precisely the behaviour this row does not have.

### 15.2 Why the amplification is where the campaign should aim

| property | status |
| --- | --- |
| inside one thread | yes — no worker, lane or helper thread is added |
| no durability change | yes — `synchronous = OFF` and `MEMORY` journal are untouched |
| no cache or buffer-policy change | yes — §11.2 shows cache size is irrelevant here anyway |
| no workload change | yes — same objects, same bytes, same counters required |
| structural, not tunable | yes — Squad B §7.6: **10,081 of 16,595 groups (60.7 %) hold exactly one record**, because the WholeFile lane seals after every member (`cas/selection.rs:57-58`, `:97-102`) and every seal ends in `maybe_commit()` (`cas/placement.rs:199`) |
| a Store-format change? | **not necessarily** — coalescing seals *within* the existing format is a cadence question; a pack directory that does not move would be a format change and needs an owner ruling |

**The lever is therefore not the transaction count but *how many objects a seal carries***
— Squad B's own phrasing — and the campaign's next step is a pre-registered treatment on
that, not on batching across steps.

### 15.3 The net attribution

| term | ns (row 2) | attributed to |
| --- | ---: | --- |
| `resolve` + `delta` + `group` | 24.9 ms (row 1) | 0.70 % — decode is not the cost (§2) |
| `full_ns` + `place_ns` | 325 ms (row 1) | encode and lane placement, untouched by this campaign |
| **`sql_ns` + `commit_ns`** | **1,941.9 ms** | **the pack-append rewrite: 7.59x amplification, 2.29 GB written and flushed for 302 MB persisted — one seal per 1.52 objects** |
| remainder | 1,050.4 ms | the caller's per-object work the instrument declares out of scope (§5) |

**`full_ns`, `place_ns` and the remainder are the only terms this campaign has not attacked,**
and none of them is a database term. The handoff's §2 target — the 1.88x CPU term — is
therefore attributed with a mechanism, and the mechanism is one the owner's single-thread
constraint does not forbid.

## 16. Corrections owed to Squad B, and the prior art this campaign did not discover

Squad B's row-2 defence
([`../issue219-squadB-sql-20260921T043911Z/README.md`](../issue219-squadB-sql-20260921T043911Z/README.md)
§12-§13, raw `row2_defence.*`, `db_state.txt`) corrects four things in §12-§15 of this
report. [`pack-append-synthesis.json`](pack-append-synthesis.json) has been rewritten; the
four corrections are recorded here rather than silently applied.

### 16.1 Four corrections

1. **The pack data is 302,023,232 bytes, not 304,427,008.** The larger figure came from
   SQLite `dbstat`, which reports **page** bytes and not payload bytes. The receipt
   publishes `resources.space.pack_bodies_bytes = 302023232`, and
   `SELECT SUM(length(data)) FROM object_packs` equals it exactly. §12 and §13 quoted the
   dbstat number and were wrong by 0.79 %.
2. **The amplification is measured, not derived.** §13.1 said *"the store keeps only final
   pack lengths, so the intermediate lengths cannot be read back"*. **That is false.** The
   pack directory stores `body_start`/`encoded`/`decoded` per group
   (`core/crates/layerfs-storage/src/pack/layout.rs:331-393`) plus a 4-byte start offset for
   the compact whole-file lane (`:397-454`). Squad B read all 1,250 directories, reproduced
   every intermediate length and asserted `Σ(entry+body) == length(data)` per pack with **0
   failures**. The figure is **2,292,865,337 bytes written / 302,023,232 persisted =
   **7.5917x*** — measured. §13's `(k+1)/2 = 7.72x` agreed to 1.7 % by luck as much as by
   method.
3. **The average pack rewrite is ≈134 KiB, not 243 KB** (median 137,544 B). §12's blob-size
   calibration assumed 243 KB, which sits between p90 and the maximum, near
   `PACK_LIMIT = 262,144`. §12's conclusion (larger blobs are faster) is unaffected; its
   *stated geometry* is corrected.
4. **The candidate in §13.5 is already the behaviour.** "Hold the tail and issue one `UPDATE`
   per pack per transaction" **can reduce nothing**: appends are *already* one per affected
   pack per transaction, and no code path writes the same pack twice inside one transaction
   (`cas/placement.rs:161` takes `writes.first()` and there is exactly one write to take).
   §13.5's "natural candidate" is withdrawn.

### 16.2 The amplification is not uniform, and that localises it

| lane | packs | writes | writes/pack | written B | final B | factor |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| **WholeFile** | 102 | 9,444 | 92.59 | 1,260,350,294 | 24,613,232 | **51.21x** |
| **PooledMeta** | 3 | 207 | 69.00 | 25,584,656 | 726,096 | **35.24x** |
| Native | 1,142 | 7,130 | 6.24 | 1,004,612,191 | 276,058,548 | 3.64x |
| Ordinary | 3 | 21 | 7.00 | 2,318,196 | 625,356 | 3.71x |
| **all** | **1,250** | **16,802** | **13.44** | **2,292,865,337** | **302,023,232** | **7.5917x** |

**WholeFile + PooledMetadata rewrite 56.1 % of all rewritten bytes to persist 8.4 % of the
final bytes.** The cause is named and verified in source: `cas/selection.rs:57-58` sets
`must_seal = occupied` for `WholeFile | PooledMetadata | Singleton`, and `:97-102` seals
again immediately after pushing the record — so 9,444 one-record seals land in 102 packs, and
~2.6 KB of new data drags a ~241 KB body through SQLite again. **The global 7.59x is a poor
summary of a distribution that runs from 3.64x to 51.21x.**

### 16.3 Prior art: this is not a discovery, and the handoff's §3.2 warning was right

`../stage-6-history-209-rca-20260920T191016Z/` **already established the same phenomenon on a
different lane** — its own words, quoted by Squad B §0: *"every pack append rewrites a whole
pack body (255 packs, 45,298,203 bytes, 177,640 average, 262,112 maximum → ≈8.1 GB of BLOB
rewritten across 45,794 appends)"*, with a `COMMIT` at 58.0 µs on the same fsync-free
profile. **§13's framing of the amplification as this campaign's finding is wrong and is
corrected here:** what is new is only the per-lane breakdown that localises it to two lanes,
and the `selection.rs` seal rule that causes it.

The same RCA also answers the handoff's §3.2 directly. **The cadence is a contract, not a
tuning choice:** widening the step with a `LAYERFS_STORAGE_COMMIT_EVERY` knob made the second
writer **fail** — `CleanupFailed { original: OwnershipUnavailable, .. }` at every value above
1. That knob **does not exist anywhere in `core/` or `crates/` at HEAD `9c46930b8`**; the
contract is now structural. The handoff said *"do not assume this is the gap ... it may be
intended"*. **It is intended, and it has already been tested and found load-bearing.**

### 16.4 What this leaves as the lever, stated honestly

Reducing the amplification means reducing **seals**, because appends are already one per pack
per transaction and each seal ends in `maybe_commit()` (`cas/placement.rs:199`). #209 shows
that widening the commit step is forbidden by the multi-writer contract. What remains
untested is whether the **WholeFile and PooledMetadata lanes need to seal after every
member** at all — a grouping policy for those two lanes, not a cadence change. That is
**not implemented, not measured and not pre-registered** here, and it is the campaign's next
step. A pack format whose directory does not move would remove the amplification outright
but is a **Store-format change and needs an explicit owner ruling**.

### 16.5 Provenance of the 7.5917x — it is the pinned row's own artifact

Two provenance points matter and both resolve favourably:

1. **The 2,292,865,337 bytes were read out of the *pinned* row's store.** Squad B re-verified
   its two originals byte-identical at the end of its work as
   `base.sqlite 135af7c8…` and `sample.sqlite 03918d61…`, and `03918d61a9f004b292deafeadbea6992f870b8bdd96b3a631d4bc3309408db29`
   is exactly the `sample.sqlite` sha256 that `ns17-pinned2-20260921T031259Z/manifest.json`
   records. **The amplification is therefore measured on the sealed row's own bytes**, not on
   a diagnostic row's and not on a replica.
2. **The call count is now published as well as derived, and the two agree exactly.** Squad B
   warned that the *pinned* receipt publishes neither `packs_created` nor `pack_appends` —
   correct: its whole counters block is 15 keys. But the second diagnostic row
   (`ns17-squadA-packcounters-20260921T044814Z`, §14) publishes them from the product as
   **1,250 + 15,552 = 16,802**, against Squad B's independently derived **16,802**, and its
   `statements = 16,595` against Squad B's derived **16,595**. **A derivation from source plus
   a database count, and a product counter, agree to the unit on both quantities.** Neither
   the pinned row's 15-key counters block nor the diagnostic binary's provenance is a
   weakness in the figure; the two methods are independent and they match.

Squad B's label for row 2 — **DERIVED-EXACT**, a database count times a `select_many`
invariant, rather than a receipt counter — is the right label for its own table and is
preserved. The separate product-counter confirmation is this report's, not Squad B's.

## 17. Squad C: the pre-registration, and two more corrections to this report

Authoritative record: `../issue219-squadC-cadence-20260921T044258Z/README.md` plus
`pre-registration.md` and 11 raw files. Summary of what it settles and what it corrects here.

### 17.1 The cadence unit, established

A **step** is one uninterrupted hold of the per-Store arbitration lock, and it **always ends in
`COMMIT` before that lock is released** - the rule `cas/lifecycle.rs:163-167` states in the
product's own words. Four sites open one: `seal_group` frames and places **one** lane group
(`cas/placement.rs:132-200`, `select_many(vec![group])` at `:137`), `write_value_groups`
(`cas/pool_lane.rs:288-355`), the ordinal reservation (`cas/pool_lane.rs:112-122`) and the
per-wave candidate flush (`cas/lifecycle.rs:180-202`). Batching happens *inside* a step
(`sqlite/write.rs:101-164`), never across the commit.

**Exact decomposition of 17,378** (cross-checked three ways): 1 slot acquisition + **16,595**
object-group seals + 207 pooled value-group writes + 207 ordinal reservations + **367**
candidate-signature flushes (by subtraction) + 1 publication. `16,595 + 207 = 16,802`, which is
Squad B's independently parsed pack-write count, and 17,377 equals Squad B's
`SELECT next_pack_id` call count.

### 17.2 `commit_ns` does not charge what its own doc says

Two charge sites only: `cas/lifecycle.rs:169` (step path, after `advance_pack_if_moved` at
`:168`) and `:256` (publication path). **`BEGIN IMMEDIATE` is charged nowhere**
(`cas/lifecycle.rs:129-139` has no `SaveProfile::charge`) - Squad B prices it at 17,378 x
5,375 ns = **93.4 ms sitting outside every bucket** - and **`ROLLBACK` is charged nowhere**
either. `cas/owner.rs:60-62` therefore *overstates twice and omits once*, and Squad C proposes a
replacement. A third instrument defect belongs beside it:

| | ns |
| --- | ---: |
| `pipeline.profile_commit_ns` (row 1) | 1,351,360,518 |
| Squad B's `COMMIT` statement total | 1,104,900,000 |
| **difference, attributable only to `advance_pack_if_moved` (1,250 statements)** | **246,460,518, NOT_MEASURED** |

### 17.3 Correction to section 15.1's rate

Section 15.1 priced the database term over row 2's `sql_ns + commit_ns = 1,941,851,139 ns` and
got **1.18 GB/s**. Squad C prices it over the full **2,132,598,057 ns** database term and gets
**1.03-1.07 GB/s**. Both divide the same rewritten volume by different denominators, and the
two rows' `commit_ns` differ by 15.25 % (section 14.5). **Quote 1.0-1.2 GB/s as a range, not a
point value.** The conclusion is unchanged, and Squad C states it more sharply than this report
did: *"302 MB is what the Store KEEPS; the pager is HANDED 2.20-2.29 GB"* - so **142 MB/s was
never the right rate to compare against a floor.**

### 17.4 Pre-registered treatment PR-C1 (registered, not implemented, not run)

**One difference:** the step boundary moves from the group to the **lane tail** - a lane frames
groups as records arrive, holds them under the existing wave bound, and places the whole tail in
one `select_many` + one pack write at the step boundary. No new constant, no format, pragma,
worker or workload change; still one transaction per step, still committing under the lock.

| instrument | against | predicted |
| --- | ---: | --- |
| `pipeline.commits` | 17,378 | 1,600-2,800 |
| pack writes | 16,802 | 1,300-2,600 |
| rewritten bytes | 2.20-2.29 GB | 0.30-0.50 GB (amplification ~1-2x) |
| `profile_sql_ns` | 781,237,539 | 330-470 ms |
| `profile_commit_ns` | 1,351,360,518 | 250-560 ms |
| `operation_ns` | 3,538,935,458 | 2.0-2.5 s |
| CPU | 3,286,170,000 | 2.4-2.9 s |

**Refuted if** commits stay above 6,000; appends stay above 6,000 (linkage refuted); rewritten
bytes stay above 1.2 GB; the two DB buckets fall by less than 300 ms while the counters fall
(page-flush model refuted for this row); `operation_ns` falls less than 15 %; the second writer
is refused, waits or loses (red regardless of timing); any pinned identity moves; or
`pack_watermark` / `visibility` / `cas_reuse` / `persistence_failure` go red. The file hash is
**deliberately not** predicted identical - transaction boundaries move page and freelist layout -
and must be reported, not required.

The cap is stated and accepted: the whole-file lane's group body is **one unframed `Raw`
record** (`pack/assemble.rs:117-120`, `pack/layout.rs:114-117`), so **102 packs (8 %) carry 53 %
of the rewritten bytes** and that share cannot be coarsened without a Store-format change, which
Squad C does not propose.

### 17.5 An eighth correction to the handoff

Section 3.2 of the handoff attributes the per-step commit to commit
`7075f338db36b209b59031e3e55ae11cf87eed57`. **It is not that commit.** `git show 7075f338`
touches `schema.sql`, `store.rs`, `policy.rs`, `sqlite/{lookup,ownership,schema}.rs` and tests -
**not** `lifecycle.rs`, `placement.rs` or `pool_lane.rs` - and `git show
7075f338^:lifecycle.rs` **already contains** `advance_pack_if_moved`, `maybe_commit` and the
"never outlives the step" comment. `git log -S` places the step-scoped transaction in
**`eb319aaa9`** (the multi-writer model, six hours before #216). #216 configured the budget; the
step-scoped transaction came from the multi-writer model. **The handoff's causal attribution is
wrong and the owner decision it points at is a different one.**

### 17.6 Where the required deliverables stand

| handoff section 5 gate | status |
| --- | --- |
| A - buckets + remainder, phase matrix | **met** (sections 2, 3, 11-16) |
| B - ranked statement table | **met**, and independently confirmed by the product's own counters (14.2) |
| C - measured cadence share, not assumed | **met** (2, 9, 17.1-17.3) plus **PR-C1 registered** (17.4) |
| D - chunking cleared or implicated | **met**; chunking cleared with byte-exact closures |
| E - independent re-derivation | **in flight** when this section was written; not yet a verdict |
| ledger entry (next free L61) and #209/#219 status comments | **NOT_RUN** - outside the squads' remit and not attempted this session |

## 18. ERRATA — Squad E's review, and this report's corrections

Authoritative review: `../issue219-squadE-review-20260921T045621Z`-adjacent directory
[`../issue219-squadE-review-20260921T045622Z/README.md`](../issue219-squadE-review-20260921T045622Z/README.md)
with `rederive.py` and its verbatim `rederive-output.txt` (exit non-zero on any mismatch).
**Where this section and an earlier section of this report disagree, this section wins.**

Squad E's verdict on the fifteen rows it was given: **1, 2, 3, 5, 7, 8, 10, 11, 13 and 14 PASS on
substance**; **row 9 FAILs with 18 arithmetic items**; **row 12 FAILs outright**; rows 4 and 6
PASS except for one number each. The failures that touch *this* report are corrected below. None
of them changes a verdict or the campaign's attribution; several change a quoted number, and one
changes a source attribution.

### 18.1 Row 12 — the phase matrix is mis-sourced (material)

§3's preamble says *"v0.1.7 = the diagnostic row above"*, but **nine of the v0.1.7 cells are the
`ns17-pinned2` row's values**, and the *"product timing children"* cell divides the diagnostic
row's numerator by the pinned row's denominator. Corrected cell sources:

| cell | §3 value | diagnostic row | pinned2 row | which §3 actually used |
| --- | ---: | ---: | ---: | --- |
| acquisition | 8.1 ms | 5.2 ms | 8.1 ms | **pinned2** |
| preparation | 913.2 ms | 898.8 ms | 913.2 ms | **pinned2** |
| setup (`store.open` + `storage.begin`) | 3.6 ms | 2.7 ms | 3.6 ms | **pinned2** |
| verification | 338.7 ms | 338.1 ms | 338.7 ms | **pinned2** |
| cleanup | 459 ns | 541 ns | 459 ns | **pinned2** |
| complete command | 4,841.6 ms | 4,791.6 ms | 4,841.6 ms | **pinned2** |
| run wall | 5,045.4 ms | 4,986.1 ms | 5,045.4 ms | **pinned2** |
| handoff | 41.7 ms | 44.3 ms | 41.7 ms | **pinned2** |
| product timing children | 11.2 ms of 3,538.9 ms | 11.7 ms | 11.2 ms | **mixed: pinned2 numerator, diagnostic denominator** |

Only `operation_ns` (3,538,935,458), the SaveProfile buckets and `accept_span_ns` are the
diagnostic row's. **The matrix is still complete and every cell is still sourced — but the
column is a mix of two rows, not one.** A reader must not treat the v0.1.7 column as a single
run. Corrected statement: *the v0.1.7 column is the pinned row except for `operation_ns` and the
profile cells, which are the diagnostic row; the two rows differ by 1.03 % of `operation_ns`.*

### 18.2 Row 14 / §17.2 — a withdrawn attribution (material)

§17.2 attributed the **246,460,518 ns** gap between `profile_commit_ns` and Squad B's `COMMIT`
statement total to `advance_pack_if_moved` (1,250 statements). **That attribution does not
survive arithmetic:** 1,250 statements at the ~1 µs the same statement measures at can carry at
most ~1.25 ms, i.e. **at most 0.5 % of the 246.5 ms**. The gap is real and the only *other*
charged work in that region is `advance_pack_if_moved`, but that is a statement about which
candidates exist, not an attribution. **The 246.5 ms is `NOT_MEASURED`** and is recorded as such.

### 18.3 Row 5 / §13.1 and §14.3 — superseded pack arithmetic

| § | what it said | corrected |
| --- | --- | --- |
| 13.1 | "14.44 writes per pack" | **13.4416** (`16,802 / 1,250`) |
| 13.1 | "16,802 `UPDATE` calls" | **15,552 `UPDATE` + 1,250 `INSERT`**; `ranked_table.tsv` row 2 is the pack-write total, not the `UPDATE` alone |
| 13.1 | "7.72x / ~2.34 GB" | **7.5917x / 2,292,865,337 B** — measured (§16.1); the `(k+1)/2` model also wrongly counts the 1,250 creates as rewrites |
| 14.3 | amplification band "6.72–7.22x" | **superseded: the measured 7.5917x lies outside the band**, because the equal-increment assumption is false |

### 18.4 Rows 11 and 9 — stale framings and smaller numbers

- **`reconciliation.json` still carries the withdrawn verdict** ("NOT EXPLAINED BY THE
  AMPLIFICATION") that §15.0 retracted. It is marked superseded in place below.
- **§12.2 still names the per-row metadata structure as "the surviving candidate"** and **§13.4
  still calls the amplification "the only candidate this campaign has found"**. Both are stale
  after §16.3: the phenomenon is prior art (the #209 RCA), and the per-row metadata candidate was
  superseded by Squad C's per-commit split.
- **§12's `7,959 ns`** should be **8,014.74 ns**; the `cadence-calibration.json` derived block is
  internally inconsistent and Squad E recomputed it.
- **CPU/wall for v0.1.6** is **1.8931**, not 1.8934.
- The §3 cell "build/encode 349.9 ms" does not sum to its own parts; the components are right and
  the total is wrong.
- Squad E also corrected Squad B's own report: `576 = 367 + 207 + 1 + 1` (not `207 + 207 + 1 + 1 +
  367`, which is 783), its `operation_ns` citation (3,575.8 ms, not 3,585.8), and 1,956.7 →
  1,956.6 ms. **Squad B's `pack-append` parser was re-implemented independently by Squad E and the
  7.5917x reproduces from the pinned store's own bytes** — the mechanism stands.
- Squad E also notes that §14.2's "two methods that share no input" is too strong: Squad B's row-2
  count was itself parsed out of `sample.sqlite`, so the two methods share that input even though
  they do not share a derivation. The *equality* stands; the phrase is overstated.

### 18.5 What the review did not change

The seven bucket values and their shares; the 60.3084 % / 0.7013 % split; the work-identity
result (15 shared counters, 0 moved, both canonical totals equal); the 7.5917x amplification
measured on the pinned row's own bytes; the 16,802 / 16,595 cross-agreements; Squad D's chunking
verdict and its byte-exact closures; Squad C's commit decomposition and its #216 provenance
correction; the eight handoff corrections; and the direction of every conclusion. **The campaign's
attribution is unchanged; one number, one source attribution and two stale framings were wrong
and are corrected here.**

### 18.6 The claim Squad E could not verify

See §4 of its review. It names the single most load-bearing claim it could not check, and this
report does not restate it — read it there.

## 19. ERRATA II - the claim the reviewer could not verify, and section 18's own defects

Squad E's completed review (157 PASS / 41 FAIL across the fifteen rows it reviewed,
`rederive.py` exiting non-zero on any mismatch) carries four items section 18 did not cover.
**Section 19 wins over section 18 and over every earlier section where they disagree.**

### 19.1 The campaign's most load-bearing unverifiable claim, and an error of mine (material)

Squad E names the single claim it could not verify: **that the two arms are the same product.**
**It is right, and this report asserted the opposite.**

- **No v0.1.7 artifact records a product seal or an image at all.** The row's `identity` block
  carries `product_lock_sha256` (which is `core/Cargo.lock`) and `harness_binary_sha256` -
  different fields. There is no `LAYERFS_PRODUCT_SEAL` and no `image` anywhere in the
  `ns17-pinned2` receipt, `run.json` or `trace.jsonl`.
- `b3e3cb745340675fdc9cda843b2506e5bcafa912cde32d17f32e098ee886402e` and
  `sha256:d152ced8d21cd4ea4b6ca0e73f90aea274ba515bb44dac641a84b1c465d54296` appear **only** in the
  v0.1.6 (root `crates/`) family and in the handoff's prose.
- Therefore **section 7.1's closing sentence** (that the product seal and the image "do match")
  **is unsupported and is withdrawn**, and **section 10's table row** ("same product seal, same
  image, same workload, same work counters, one harness difference") **is wrong on two of its four
  items.** The corrected statement: *same workload, same work counters, one harness difference;
  the product seal and the image are `NOT_MEASURED` on the v0.1.7 side, and the handoff's
  assertion that the two arms share them cannot be checked from any raw field this campaign
  holds.*

This matters beyond bookkeeping. The campaign's premise is a comparison between two *products*,
and section 2 of the handoff already concedes the two arms live in different workspaces (`crates/`
vs `core/`) whose seals differ by construction. **The 1.88x CPU target therefore rests on an
identity the evidence does not carry.** The comparison is still the right one to make - both rows
do the same work at the same object and byte counts, which section 10 proves - but it is a
comparison of **two implementations**, not of **one product measured twice**, and the target
should be quoted with that qualification.

### 19.2 A number in a verdict with no artifact, now archived

Squad E found that **`51,279 ns/txn`** - the figure section 12.2 uses to conclude that the
per-commit engine cost is not anomalous - **appeared in no raw artifact of this corpus**, only in
this report's prose. That is correct: the calibration was run in the session and never written to
disk. [`split-calibration.json`](split-calibration.json) now carries it, with the process failure
recorded rather than hidden. **The verdict it supports is unchanged** (the synthetic's 51,279 ns
per commit against the product's 77,763 ns is 1.52x, and the product's commit does more work), and
section 12.1's `446,106,000` should read **`446,111,419`**.

### 19.3 Section 18's own defects

| section 18 said | corrected |
| --- | --- |
| "18 arithmetic items" (row 9) | **17** |
| "the fifteen rows it was given" | the review's brief defines **rows 1-14**; section 18 was itself reviewed, as a fifteenth |
| section 18 did not correct section 7.1's seal/image sentence | **now corrected, 19.1 above** |

Section 18's substantive corrections all PASS. Its own statement that it wins on disagreement is
what makes the stale numbers left in the body text (`14.44`, `7.72x`, `6.72-7.22x`, `446,106,000`,
`51,279`, `1.8934`, `1,926.5`, `349.9`) survivable - but they are stale, and a reader who quotes
the body rather than sections 18 and 19 will quote them wrong.

### 19.4 Two more defects the campaign should not leave behind

- **Squad B's archived `pack_parse.py` cannot reproduce its own `pack_stats.json`** - its
  whole-file branch uses `start - 8` and its own assertion fires on the first whole-file pack.
  **The 7.5917x result is not affected**: Squad E re-implemented the parse independently against
  the pinned row's `sample.sqlite` (`03918d61...`, manifest-matched) and reproduced it with **0
  format violations**. Do not re-run Squad B's script expecting its output.
- **Squad C's PR-C1 upper bound of 2,800 commits does not follow from its own stated mechanism** -
  five lane-tail steps per wave across 577 waves plus the 783 fixed steps is **3,668**. The
  registration's lower bound, its identity, its refutation list and its "not run" status are
  unaffected; one bound is wrong.

### 19.5 The campaign's verification state, stated plainly

| | |
| --- | --- |
| Squad E `rederive.py` | **157 PASS / 41 FAIL / 0 INCOMPLETE**, exits 1 on mismatch |
| rows PASSing on substance | 1, 2, 3, 5, 7, 8, 10, 11, 13, 14 |
| rows FAILing | 9 (17 arithmetic items) and 12 (the phase matrix's row mixing) |
| failures that change a **conclusion** | **none** |
| failures that change a **number or a source attribution** | corrected in sections 18 and 19 |
| the one claim that cannot be checked at all | **the two arms' product identity, 19.1** |

`rederive.py` must be re-run after any further edit to this directory; the review's verdicts are
pinned to the corpus digests recorded in its section 1.

## 20. Three treatments attempted, three refuted - and what that establishes

After the campaign closed, three optimizations were registered, implemented and tested. **None
is kept.** Each refutation is recorded in full beside this file; this section states what the
three together establish.

| id | treatment | refuted by | recorded in |
| --- | --- | --- | --- |
| PR-A1 | a lane that seals per record stops retaining an open pack, so each group is written once instead of rewriting a shared pack | `cas_reuse.rs:232-261` - two whole-file objects must **share** a pack and a read of both must touch **one** pack | `preregistration-PR-A1.json`, `preregistration-PR-A1-outcome.json` |
| PR-C1b | `seal_group` frames and queues; `flush_tail` places a bounded run of groups in one pack write and one `COMMIT` | `delta_payload.rs:74-98` - an object admitted earlier **in the same save** must be usable as a delta base by a later one, and only a placed row can be | this section |
| PR-A2 | `validate_candidates` returns early when no foreign published save holds a pack | measurement: the guard fired, 25,245 lookups were skipped, and `profile_sql_ns` moved by **12,393 ns of 796,535,871** | `preregistration-PR-A2.json`, `preregistration-PR-A2-outcome.json` |

### 20.1 The two structural refutations are the same refutation

PR-A1 and PR-C1b both fail for one reason: **the pack write and its locator rows are not merely
I/O. They are the act that makes an object part of the save.**

- a row is what makes an object readable as a delta base for a later object *in the same save*
  (`delta_payload.rs`), and
- a shared pack is what makes a batch read of several objects one pack read (`cas_reuse.rs`).

So the write path cannot be deferred (PR-C1b) and cannot be split into one pack per group
(PR-A1). Both would trade a declared product property for write bytes. **Within one thread, and
without changing the pack format, the 7.59x pack-append amplification is not reachable by any
placement, lifetime or cadence rule.** That is now established by two independent invariants
rather than argued.

### 20.2 The measurement refutation closes the handoff's first lead

PR-A2 is the interesting one because it *worked* and still bought nothing. The guard fired; the
25,245 single-id locator probes were genuinely skipped; **the store came out byte-identical to
the pinned row's** (`03918d61…`); every work counter was unchanged; the row passed. And
`profile_sql_ns` moved by 12,393 ns.

**Squad B's 7,083 ns/call for that statement was a Python-harness artifact**, exactly as its own
§6 caveat warned. Together with `presence_queries = 398` for 24,863 offered objects, this closes
the handoff's §3.1 lead - *"the locator/presence queries"* - **at product level, not in a
replica.**

It also found an instrument gap: `validate_candidates` is charged to **no** `SaveProfile`
bucket (`cas/placement.rs` runs it after the `insert_objects` charge closes), so whatever it
costs lands in the unattributed remainder.

### 20.3 What is left, and who owns it

Every route measured this session ends in one of two places:

1. **The pack format.** `pack/layout.rs::directory_is_starts_only()` is true for `WholeFile`
   alone, and it is the lane with 51.21x amplification. A directory-last layout, or reserved
   directory space, makes an append write only the new bytes and removes ~2.0 GB of the 2.29 GB
   at the root, for every lane. It is a **Store-format change and needs an explicit owner
   ruling**; the campaign did not propose or implement one.
2. **The 30 % remainder**, which no bucket names. `validate_candidates` is one known
   uncharged region; the rest is unmeasured. Instrumenting it is cheap and is the only
   remaining unexamined term of any size.

**Tree state:** `git diff --stat -- core/crates/` is empty. All three treatments are reverted;
the only modification in the worktree is the benchmark harness instrumentation, and the harness
binary has been rebuilt from the reverted product so the two agree.
