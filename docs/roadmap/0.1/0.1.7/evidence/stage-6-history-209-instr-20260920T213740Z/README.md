# #209 — the step's transaction, read with SQLite's own instruments: the commit path is closed, and the round withdraws

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-21, continuing [#209](https://github.com/Ephemeral-AI-Lab/layerfs/issues/209)
> from `b65d09a81`. **No product line is kept and no treatment is pre-registered**:
> the tree is byte-identical to `704580673` over `core/crates` and `crates`
> (`git diff --stat 704580673 HEAD -- core/crates crates` prints nothing).
> Admission `INELIGIBLE`, every budget class `NOT_RUN`. Nothing here closes #190,
> #205, #208 or #209.

## The answer, in one paragraph

**Every question the handoff asked is answered by the engine's own instruments, and
the answer is that the commit path is closed.** A step's transaction writes
**27.839 pages per commit** on the product's own connection — **24.433** of them the
pack body it just grew and **4.74** the structural overhead the growth forces (page
1's change counter, the `objects` primary-key btree, the `objects_save` index, the
free-list trunk and the pack's own leaf). **Zero pages spill.** No statement in the
step scans, sorts, builds an autoindex or re-prepares: `SQLITE_STMTSTATUS` reads
**0 for `FULLSCAN_STEP`, `SORT`, `AUTOINDEX` and `REPREPARE` in every one of the
21 buckets**, and every plan is a seek. `COMMIT` executes **three VDBE opcodes** and
costs 56.1 µs, so its price is the page writes and nothing else. `BEGIN IMMEDIATE`
is **9.74 µs of the step and 0.473 s of the run**, of which a deferred `BEGIN` arm
prices **3.8 µs as the eager RESERVED file lock** — and it is not removable, because
the watermark read that opens the step must happen under the write lock. The one
hypothesis this round's split did *not* inherit — that the 4 MiB pack cache is
thrashing — is **refuted at a 97.33 % hit rate**. What is left in the step that is
neither the format nor the contract is **0.49 s of re-parsing the pack `UPDATE` on
every append and 0.17 s of copying the body into the engine at bind**, and the
re-parse is the mechanism L59 already tested and withdrew. **So the round
withdraws**, and hands the owner a format proposal priced at **≈3.07 s** rather than
the ≈1.5 s in circulation.

## 1. The instruments, and what each one actually measures

Two instruments, both declared in `extra_environment` of every receipt and both
removed before this commit; their sources are retained here and the exact diff that
carried them is [`scratch/instrument.diff`](scratch/instrument.diff).

**I1 — `SQLITE_TRACE_PROFILE` on the save's own connection**
([`scratch/step_probe.rs.txt`](scratch/step_probe.rs.txt), 433 lines with its call
sites). One observer gives both §2.2 and §2.5: SQLite reports, once per completed
statement execution, the statement's runtime **and the live `sqlite3_stmt_status`
counters of that very handle**, so the per-statement charge split needs no call site
in the SQL layer to know about it.

> **What its clock is.** SQLite computes the profile duration as
> `(sqlite3OsCurrentTimeInt64() - startTime) * 1000000`
> (`sqlite3.c`, `invokeProfileCallback`), and `sqlite3OsCurrentTimeInt64` is the
> **millisecond** clock. The reported value is therefore a whole number of
> milliseconds: for a statement that takes 20 µs it is 0 almost always and 1 ms with
> probability ≈ t/1 ms. The figures below are read as a **straddle estimator of total
> execution time** — unbiased in the mean, with a standard error of
> `sqrt(observed ms) × 1 ms / calls` — and never as a per-call time. The in-product
> and micro-probe figures agree to within that error, which is the check that the
> estimator is being read correctly.

**I2 — `sqlite3_db_status` / `sqlite3_status` around every `COMMIT` and every
save**, on the product's own connection: `CACHE_WRITE`, `CACHE_SPILL`,
`CACHE_HIT`/`CACHE_MISS`, `CACHE_USED`, `STMT_USED`, `DEFERRED_FKS`, the process
page-cache counters, and the Store file's own `page_count`/`freelist_count`.
**These had never been read for the product's connection**; the "15.30 pages per
commit" in circulation came from a harness-owned replay.

**I3 — `EXPLAIN QUERY PLAN` and the bytecode of every statement one step issues**
([`scratch/plan_probe.py`](scratch/plan_probe.py),
[`scratch/plans.txt`](scratch/plans.txt)), against a copy of the run's own Store,
with the declared pragma profile, the product's temp read-scope table and
`foreign_keys = ON` (which the CLI does not set and the product does).

**I4 — a micro-probe that prices the parts one statement cannot separate**
([`scratch/step_cost_probe.c`](scratch/step_cost_probe.c),
[`scratch/step-cost-arms.txt`](scratch/step-cost-arms.txt)): `Connection::execute`
gives rusqlite a SQL text and a parameter list, and parse, bind and step happen
inside one call, so the decomposition is measured by replaying the same SQL with the
same parameter shapes over a copy of the run's own Store, arms interleaved
round-robin in one process.

## 2. Q1 — what one step's transaction writes, page for page

Measured on the product's own connection, whole run, stride10:

| quantity | value |
| --- | ---: |
| `SQLITE_DBSTATUS_CACHE_WRITE` | **1,348,771 pages** |
| pages attributed to `COMMIT` windows | 1,348,686 |
| commits | 48,446 |
| **pages per commit** | **27.839** |
| pack-body bytes handed to `insert_pack`/`append_pack` | 4,487,746,188 (4.18 GiB) |
| `ceil(body / 4096)` summed over those calls | **1,118,898 pages** |
| **body pages per append** | **24.433** |
| **excess** | **229,788 pages (4.74 per commit, +20.5 %)** |
| `SQLITE_DBSTATUS_CACHE_SPILL` | **0** |
| `CACHE_HIT` / `CACHE_MISS` | 6,382,657 / 5,989 |
| `CACHE_USED` at end of save | 8,332,288 B |
| `page_count` start → end | 11,687 → 12,663 |
| `freelist_count` start → end | 46 → 38 |

**The excess is not waste, and it is accounted for.** Every one of those pages is
genuinely dirtied by the transaction: page 1 carries the change counter that SQLite
rewrites on every commit that modified the file (48,446 pages); the `objects`
primary-key btree and the `objects_save` index take one leaf each per commit
(≈97,000); the free-list trunk is rewritten when the freed overflow chain is handed
back and again when the new one is allocated (≈45,800); and the pack's own leaf page
holds the cell header and the local part of the row (≈45,800). Those five terms sum
to ≈245,000 pages against 229,788 measured. **Nothing in the excess is a policy that
could be changed**; three of the five are the schema's own btrees and two are
SQLite's per-commit bookkeeping.

**The file does not grow with the writes**: 4.18 GiB were written and the Store grew
by 976 pages, with the free list ending *lower* than it started (46 → 38). The
allocation is reusing freed pages, which is why the append re-dirties the same
region of the file rather than extending it.

**And the commit is nothing but those pages.** `COMMIT` executes **3 VDBE opcodes**
per call and costs 56.124 µs of engine time; 27.839 pages at the 1.65–1.72 µs per
4 KiB `pwrite` this lane measured is 46–48 µs, and the per-commit excess above adds
its own. **`COMMIT` is closed for good**, exactly as the handoff said it would be if
a real run wrote `ceil(pack body / 4096)` plus a structural remainder.

## 3. Q2 — does any statement do more work than its rows and indexes require? No.

`SQLITE_STMTSTATUS` over the whole run, per SQL-text bucket (`delta.rca.*` in the
raw trace). **`FULLSCAN_STEP`, `SORT`, `AUTOINDEX` and `REPREPARE` are 0 in every
bucket**; the columns below are the calls, the engine time, and the VDBE opcodes each
call executes:

| bucket | calls | µs/call | VDBE opcodes/call | scan | sort | auto | repr |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `begin` | 48,582 | 9.036 | 5.0 | 0 | 0 | 0 | 0 |
| `commit` | 48,446 | 56.124 | **3.0** | 0 | 0 | 0 | 0 |
| `next_pack` | 48,565 | 0.412 | 9.0 | 0 | 0 | 0 | 0 |
| `insert_pack` | 255 | 3.922 | 51.0 | 0 | 0 | 0 | 0 |
| `append_pack` | 45,794 | 20.680 | 55.0 | 0 | 0 | 0 | 0 |
| `insert_objects` | 44,334 | 8.481 | 96.8 | 0 | 0 | 0 | 0 |
| `advance_pack` | 255 | 0.000 | 34.0 | 0 | 0 | 0 | 0 |
| `ordinal_update` | 1,737 | 2.879 | 48.0 | 0 | 0 | 0 | 0 |
| `value_group` | 1,737 | 2.879 | 54.0 | 0 | 0 | 0 | 0 |
| `locator` | 297,082 | 8.479 | 42.4 | 0 | 0 | 0 | 0 |
| `pack_bytes` | 19,771 | 56.092 | 28.8 | 0 | 0 | 0 | 0 |
| `candidates_write` | 6,605 | 3.482 | 60.0 | 0 | 0 | 0 | 0 |
| `other` | 54,554 | 1.980 | 15.0 | 0 | 0 | 0 | 0 |

**Every plan is a seek** ([`scratch/plans.txt`](scratch/plans.txt) prints all
nineteen, with the bytecode). The four the handoff named specifically:

- **the `EXISTS` subquery is a coroutine, evaluated once for the one row the
  `UPDATE` matches** — `|--SEARCH object_packs USING INTEGER PRIMARY KEY (rowid=?)`
  then `CORRELATED SCALAR SUBQUERY 2 / SEARCH saves USING INTEGER PRIMARY KEY`, and
  the read-scope subquery is `Once`-guarded (`OP_Once` + a 1-row `SCAN` of the temp
  table), so it is evaluated once per statement, not once per row;
- **no plan uses `packs_save` or `objects_save`**, and none needs to: every
  statement reaches its table by integer primary key or by the `WITHOUT ROWID`
  primary key;
- **the pack `UPDATE` compiles to `OP_Delete` + `OP_Insert`** — the whole-row
  rewrite this lane already knew about, now visible in the plan rather than inferred
  from timing;
- **the object `INSERT` costs 2 btree insertions (the `WITHOUT ROWID` primary key
  and `objects_save`) and 2 foreign-key parent existence checks
  (`OP_NotExists` on `object_packs` and on `saves`)** — 96.8 opcodes per statement
  for the ≈1.17 rows a group seal carries, which is what its rows and its two indexes
  and its two declared constraints require.

**The pack cache is not thrashing, and that hypothesis dies here.** 740,000 pack-body
lookups were served **719,722 from memory against 19,771 fetches — a 97.33 % hit
rate** over 227 distinct packs, with 560 bounded wholesale clears. The
4 MiB `DEPENDENCY_PACK_CACHE_BYTES` bound is doing its job; the 19,771 misses are
the per-save cold start plus the clears, and no eviction policy can remove them
without removing the bound.

## 4. Q4 — what `BEGIN IMMEDIATE` is made of, and what is not removable

On the product's own connection, per step: **9.738 µs of wall time and 9.036 µs of
engine time over 48,582 calls — 0.473 s of the run** — for a statement whose VDBE
work is **5 opcodes**. The price is therefore in the pager's transaction open, and
the micro-probe separates it:

| micro-probe arm | µs per call |
| --- | ---: |
| `BEGIN IMMEDIATE` + `ROLLBACK` (open and abandon) | 4.035 |
| `BEGIN IMMEDIATE` + `COMMIT` (open and close, nothing written) | 4.321 |
| **`BEGIN` deferred + `COMMIT`** (no eager lock) | **0.504** |

**≈3.8 µs of the 4.3 µs is the eager RESERVED file lock** `BEGIN IMMEDIATE` takes,
and ≈0.5 µs is the pager and VDBE bookkeeping. **None of it is removable without
holding a transaction across steps**: the step's first statement is the
`SELECT next_pack_id` that must read the watermark *under the write lock* before
`LanePlacement` may allocate a pack identifier from it, so a deferred `BEGIN` is
not available — it would read the watermark as a reader and could hand out a pack id
another writer has already used. The lock is the contract's price, not a defect.

## 5. Q3 — why L59's `commit_ns` rose, answered by mechanism rather than by a second window

**The second A B B A window the handoff asked for was not run.** Owner direction
during the round was to keep the round cheap — quick tests, few samples — so the
four-row window was started and stopped after one child had written a partial Store;
it is retained, marked and consumes no sample
([`runs/q3-1a-history-stride10/INTERRUPTED.md`](runs/q3-1a-history-stride10/INTERRUPTED.md)),
and the window is recorded **`NOT_RUN`**. What is offered instead is the mechanism,
measured three ways:

1. **The commit's content cannot change with a parse.** The micro-probe's six
   full-step arms — four of which differ only in whether the pack text is prepared
   once or parsed per call, and two of which are byte-identical code at different
   positions in the round — all report **46.14 pages written per commit** in the
   aggregate. In the product's own run, `commit_ns` is **99.8 % the `COMMIT`
   statement** (`commit_statement_ns` 2.904 s against a 2.912 s bucket) and that
   statement executes 3 opcodes. **Removing a parse cannot change how many pages a
   transaction writes**, so L59's +0.087 s cannot be an arm effect on the commit's
   content.
2. **The parse's own price, with the probe's own control.** In the micro-probe the
   two arms that parse the pack text per call read **121.556 µs** (TRANSIENT bind) and
   **120.468 µs** (STATIC bind) against **112.938 / 109.919 / 109.467 µs** for three
   arms of *identical code* that use a prepared handle at three different positions.
   The identical arms span 3.5 µs (3.2 %) purely by position, and the parse arms sit
   **10.5–11.0 µs** above them — against a parse measured alone at 4.782 µs, so
   parse **and finalize** together are what the step pays. Because the round-robin
   drifts *downward* across positions (112.9 → 109.5), that 10.5–11.0 µs is, if
   anything, an understatement.
3. **The split's own arithmetic closes.** `sql_ns` is 2.005 s of wall time while the
   statements charged inside it execute 1.340 s of engine time: a **0.665 s** gap. The
   pack `UPDATE`'s parse is 4.782 µs × 45,794 = 0.219 s and the body's transient copy
   at bind is 3.678 µs × 45,794 = 0.168 s; the remainder is the per-call
   prepare/finalize the micro-probe prices at ≈6 µs. **The gap and the mechanism agree
   to within 0.1 s**, which is what L59 could not show.

**So the round's reading of Q3 is: a time effect within L59's window, not an arm
effect.** L59's own rows support it — its `commit_ns` was a U in time (2.485, 2.377,
2.429, 2.494) while `sql_ns` was a hump (1.279, 1.590, 1.581, 1.343), and the
window's operation drifted −0.514 s from its first row to its last. **This is an
inference from mechanism and is labelled as one**; it is not the second window the
handoff asked for, and it does not re-open L59's treatment.

## 6. What is left in the step that is neither the format nor the contract

| term | per append | over the run | removable? |
| --- | ---: | ---: | --- |
| the pack body's pages at the write syscall's price | 24.43 pages × 1.9 µs = 46.4 µs | **2.13 s** | format (owner) |
| the same pages' btree delete + insert | 20.7 µs | **0.95 s** | format (owner) |
| the structural pages the growth forces | 4.74 pages × 1.9 µs = 9.0 µs | **0.44 s** | no — page 1, two btrees, the free-list trunk |
| the eager RESERVED lock in `BEGIN IMMEDIATE` | 3.8 µs | **0.19 s** | no — the watermark must be read under the lock |
| re-parsing + finalizing the pack `UPDATE` | 10.7 µs | **0.49 s** | **yes — and L59 tested exactly this and withdrew** |
| copying the body into the engine at bind (TRANSIENT) | 3.7 µs | **0.17 s** | not through rusqlite: `bind_parameter` hardcodes `SQLITE_TRANSIENT`, and this lane does not patch a dependency |
| the object rows (2 btree inserts + 2 FK checks) | 7.2 µs/row | **0.38 s** | no — the schema and the declared constraints |

**No treatment is pre-registered.** The only removable term is the re-parse, its
mechanism is L59's, and this round's own instrument says its saving belongs to
`sql_ns` and the uncharged remainder — never to `commit_ns`, which is the term the
handoff owns. Re-shipping a withdrawn treatment on the strength of a split that
agrees with the withdrawal would be re-deriving a closed round. **The round
withdraws**, and the deliverable is the RCA plus the priced owner decision below.

## 7. The owner decision this round hands over: the format's price is ≈3.07 s, not ≈1.5 s

`§3.4` of the handoff prices the whole-chain rewrite at ≈1.5 s of `commit_ns` from
L59's 17.8 µs per-append growth penalty. **Measured on the product's own connection,
the body's rewrite costs twice that**: 24.433 body pages per append at the measured
page price is **2.13 s inside `commit_ns`**, and the btree delete-and-insert of those
same pages is **0.95 s inside `append_pack`** — **≈3.07 s of a 25.99 s operation
(11.8 %)**, and 73 % of the 2.912 s `commit_ns` bucket. A chunked pack, a
pre-allocated row or an incremental-blob write would each end it and would each move
the Store hash. **Per this lane's rule that is a different operation and an owner's
decision, not an agent's optimisation**; it is recorded here with its numbers and is
not shipped.

## 8. Custody, equivalence and the rows

Two diagnostic rows, both from the instrumented binary, both with
`LAYERFS_STORAGE_RCA_PROBE=1` declared in `extra_environment`, one sample each,
fresh `--output`, both global flocks held for every resource command:

| row | binary sha256 | operation | `commit_ns` | `resolve_ns` | `sql_ns` | Store |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| `rca-instrument` | `844a95ee2cb3f2b9…` | 25.986 s | 2.912 s | 6.906 s | 2.005 s | `7ea2fe6c…` |
| `rca-packcache` | `bb1966025019cb2a…` | 17.697 s | 1.924 s | 4.765 s | 1.319 s | `7ea2fe6c…` |

**The two rows are from two different binaries and are not comparable for time** —
the pack-cache counters were added to the probe between them — and the handoff's own
rule forbids a cross-binary effect size. What they *are* is the round's clearest
statement of the window problem the handoff opens with: **the same work, the same
Store hash and the same engine accounting, at a 1.47× difference in wall clock.**

- **The saved Store is byte-identical in both rows** —
  `7ea2fe6ccf13bc5aee7ba59bddef2d60d61d50d6b90329ee1488b1f096228358`, the constant
  every #209 row has carried — so the instrument changed no stored byte.
- **The engine accounting is identical in both rows**, to the page: `cache_write`
  1,348,771, `CACHE_SPILL` 0, `CACHE_HIT`/`CACHE_MISS` 6,382,657/5,989,
  `page_count` 11,687 → 12,663, `pages_at_commit` 1,348,686. **The work did not
  move; the machine did.**
- Both rows are **diagnostics**: admission `INELIGIBLE`, every budget class
  `NOT_RUN`, and no row's time is offered as an effect.
- The instrumented harness seal is `6a922e39…` / `3f1e5896…` (the probe's trace
  rows are a harness change) and the **shipped tree's harness is unchanged at
  `04bcfab5…`**; the instrumented binary is archived at
  [`binary-archive/instrumented`](binary-archive/instrumented) with its sha256.
- One interrupted attempt is retained and marked, not deleted
  ([`runs/q3-1a-history-stride10/INTERRUPTED.md`](runs/q3-1a-history-stride10/INTERRUPTED.md)).

## 9. The second writer

**No product line changed this round**, so the shipped step is the step L57 and L60
measured, and the capability that was bought is still bought by construction as well
as by test: `multi_writer.rs` is **5/5** and `pack_watermark.rs` **2/2** on the
shipped tree in this round's checks. The retained measurement of the step a second
writer can be locked out of is L57's: three rounds of `solo`/`pair` over a 24 MiB
payload, **both writers finishing in every round with zero `OwnershipUnavailable`**,
pair p99 6.7–7.1 ms against a 6.4–7.1 ms control and a worst observed 9.8 ms against
36.5 ms. The round did not re-run the step-width probe; it is recorded **`NOT_RUN`**
and cited rather than re-derived, because the tree it measured is the tree this round
leaves behind.

## 10. Checks as run

On the shipped tree, which is byte-identical to `704580673` over `core/crates` and
`crates`:

- `core/tools/check_product_boundary.py` — **PASS**, 175 production Rust/SQL files
  ([`checks/boundary.log`](checks/boundary.log));
- `core/tools/test_check_product_boundary.py` — **OK**
  ([`checks/boundary-selftest.log`](checks/boundary-selftest.log));
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` — exit 0
  ([`checks/fmt.log`](checks/fmt.log), empty);
- `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage
  --test multi_writer --test pack_watermark --test memory_bounds --test visibility
  --test persistence_failure` — **34 passed, 0 failed** (5 / 2 / 10 / 9 / 8)
  ([`checks/storage-tests.log`](checks/storage-tests.log)).
- **Not run, and said so**: the full `--workspace` test and clippy runs, and the
  117-test harness suite. **No product line and no harness line changed**, so the
  tree under test is the tree L57 checked (537 passed / 0 failed, clippy exit 0,
  harness 117 passed / 0 failed) and this round ran the checks that the changed
  surface — none — and the round's own decision depend on. No CI, and
  `tools/preflight.sh` is permanently retired and was not used.

## 11. Reproduce

```sh
# the plans and the bytecode of every statement one step issues (seconds)
python3 scratch/plan_probe.py <a measured run's sample.sqlite> scratch/plans.txt

# the parts of one statement the engine will not separate (seconds)
cc -O2 -o scratch/step_cost_probe scratch/step_cost_probe.c -lsqlite3
./scratch/step_cost_probe <a copy of the same Store> 3000 96000

# every derived number of this report
python3 analyze.py
```

The two diagnostic rows need the instrumented binary
([`binary-archive/instrumented`](binary-archive/instrumented), sha256
`844a95ee…`, and the pack-cache build `bb196602…`), which the shipped tree no
longer builds; the diff that carried the instrument is
[`scratch/instrument.diff`](scratch/instrument.diff) and the probe module is
retained whole at [`scratch/step_probe.rs.txt`](scratch/step_probe.rs.txt).
