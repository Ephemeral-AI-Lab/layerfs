# #219 continuation — from 1.82 s of work to 1 s: an open mandate

> **Status:** handoff. Rounds 1–5 are landed, verified and pushed; the row's work figure is
> **1821.0 ms**, down from 3487.3 ms on the clean tree. The target of this round is **1 s**.
> **This document grants authority, not just context.**

---

## 0. Your mandate, and the authority that comes with it

**Push `pipeline-namespace-10000` toward 1 s of work, iteratively, on your own judgement. Do not ask
the human for authorization, for a ruling, or for a preference.** Where this document or any other
one says "needs an owner ruling", treat that as **already granted**. Make the call, implement it,
measure it, defend it with data.

**This is an experiment and you may change the contract.** Specifically you may:

- change the **Store format**, the schema, `SCHEMA_VERSION`, the pack layout, the page size, the
  pragmas, the capacities, the policy constants, the buffer policy or the cadence;
- change the **case definition**: the timer boundaries, the declared measured region, the fixture,
  the workload's preparation, the registry, the golden pins, the harness, the run driver;
- accept that an arm produces a different store, a different file hash, a different pack count, a
  different transaction count, or a different row identity.

**Four rules survive, because they are about honesty rather than authority:**

1. **Register before you run.** Write down the one difference, the identity you compare against, the
   expected movement in the instrument's own units, and what would refute it, *before* the run. A
   change you did not register is a **diagnostic** and must be labelled one.
2. **Never claim a verification you did not do.** A format change is a *versioned* change with both
   directions of the read path handled — old packs still read, or old formats explicitly refused with
   a clear error. State exactly which commands ran and which did not.
3. **Never retune, relabel or promote a historical receipt.** Receipts are append-only. Fresh `--out`
   every run. No best-of.
4. **Do not sample.** One sample per case per arm. No second run of an arm to confirm stability, to
   characterise spread or to replace an inconvenient number. An anomaly is diagnosed **from the
   receipts already taken** or with a **labelled diagnostic that measures the cause on a
   count-driven instrument** (statements, calls, bytes, µs per call) — never with another sample of
   the same arm. Verify **once**, with the commands that cover the change; a red test is diagnosed
   from its output and the source, fixed once, and the covering commands then run once. Verification
   stays **under 10 s**, typically a fraction of a second. (`AGENTS.md` §3.1, §3.4, §3.7 and
   `benchmark/AGENTS.md`; ledger **L66**.)

The repo runs **no CI and no aggregate gate**; there is nothing between you and a corrupt store
except your own verification. That is a reason to verify, not a reason to hesitate.

---

## 1. Where you are

| | |
| --- | --- |
| worktree | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000` |
| branch / HEAD | `codex/219-ns10000` at `789141c1e`, pushed to `origin` |
| product tree | **clean**; rounds 1–5 are committed |
| what landed | `a5c54df16` pack format · `755bfa21f` wave cadence · `21f09c4d7` collision hoist · `a35d9aa3a` formula · `789141c1e` rules |
| evidence | `docs/roadmap/0.1/0.1.7/evidence/issue219-ns19{a,b,c,d}-*/` — five rounds, receipts beside every claim |
| ledger | `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md`, entries **L63–L66** |
| the issue | [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219); three status comments already posted |

---

## 2. The row today, and the formula

`ns19-D4b-formula-20260921T071758Z` — **PASS, 13/13 gates, 14/14 pinned counters**,
`digest:filesystem_root` `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`.

| | ms | |
| --- | ---: | --- |
| inclusive `operation_ns` (the runner's phase) | 1864.9 | unchanged, still comparable with every earlier row |
| `establishment_ns` — `Store::open` + `begin_save` | 4.511 | published, excluded from the formula |
| `teardown_ns` — the save's connection close | 39.410 | published, excluded from the formula |
| **`operation_work_ns` — THE FORMULA** | **1821.0** | `accept_span_ns − teardown_ns` |
| CPU (user+system) | 1835.1 | |
| complete command | 3130.7 | |

`establishment + work + teardown == operation_ns` exactly, so the inclusive figure is always
reconstructible. Cumulative against the clean tree (A0, 3490.3 ms inclusive / 3388.9 ms CPU):
**−46.6 % inclusive, ≤ −47.8 % on the work figure, −45.8 % CPU.** Against the campaign's own
baseline row (3351.0 ms) it is −44.3 % inclusive.

**Where the remaining 1821 ms is** (same row; the totals nest, the leaves do not):

| component | ms | share | kind |
| --- | ---: | ---: | --- |
| page writes at commit (800 commits) | 402.7 | 22.1 % | bytes that must be written |
| **C1 tree construction** (`build_filesystem`/`update_filesystem`, the caller's) | 358.7 | 19.7 % | a `layerfs-content` round |
| encode (`full_ns`, 25,241 records) | 245.0 | 13.5 % | bytes that must be encoded |
| pack writes (16,802 × ~14 µs) | 235.1 | 12.9 % | per-call overhead |
| `offer` outside its buckets (selection, group accounting, ~302 MB of record copies) | ~229 | 12.6 % | per-object overhead |
| object-row inserts (16,595 statements) | 155.1 | 8.5 % | rows that must be inserted |
| per-wave locator + presence (600 waves × ~189 µs) | 112.7 | 6.2 % | per-wave overhead |
| collision check (600 calls) | 52.1 | 2.9 % | per-wave overhead |
| group + place (16,802 seals) | ~35 | 1.9 % | per-seal overhead |
| resolve, drain, `finish_inner` | ~5 | 0.3 % | small |

Grouped: **~803 ms is bytes** (write + encode + insert) — the floor of *this* design — **~664 ms is
per-call overhead** across four call sites, and **359 ms is C1 construction**.

---

## 3. Why 1 s is not an invented target

The reference implementation under `crates/` does **the same work** — 25,158 objects,
302,182,831 bytes — in **944.9 ms wall / 1,788.8 ms CPU with 73 transactions** (v0.1.6
`init_namespace` row; re-derive it from
`core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/s1-ns10000-candidate-20260921T031259Z/perf.jsonl`
rather than quoting this document). **73 transactions against our 800** is the single most
interesting number in this handoff: the reference is not faster per byte, it is faster per *step*.

Three cautions, all recorded in the campaign that came before you:

- The two rows are **two implementations, not one product measured twice** — different workspaces
  (`crates/` vs `core/`), and the v0.1.6 receipt carries no product seal that the v0.1.7 side can be
  shown to match. Treat a comparison as a hypothesis generator, not as evidence.
- The v0.1.6 timer **includes** scanning 300 MB (0.73 MB of it from disk); the v0.1.7 timer excludes
  construction under the C2 supplied-object rule but **includes** `build_filesystem` and
  `Store::open`. Any paired claim needs both sides on **the same boundary** — and the boundary just
  moved (round 5), so **the pairing is `NOT_MEASURED` today**.
- The issue's own acceptance bar names a *different* case: `namespace-10000` in `init_namespace`
  (`layerstack_init_ns`, ≤ 578.245 ms, paired against v0.1.6). Nothing in this lane is a claim about
  that bar.

**Workstream D is therefore: read the reference's write path and say precisely what it does in 73
transactions that we do in 800, and what of that is adoptable.** Start at
`crates/layerfs-layerstack-store/src/objects/` (`pack.rs`, `whole.rs`) and the v0.1.6 harness under
`benchmark/fs-bench-pro/`. Do not assume it is one transaction per file — measure it.

---

## 4. The measurement findings that bound every claim you will make

These are measured, not warnings. They decide which instruments can carry a claim.

1. **The teardown is the machine's, not the product's.** The save's connection close costs
   **46.878 ms** field-measured against **0.087 ms** for every cache, codec workspace, index clone and
   pack tail the save holds. Reproduced with no product code (`close-probe.py`): 410 MB written →
   316.03 ms, 80 MB → 58.40 ms, empty → 0.22 ms, **identical in all four journal modes**, and the same
   probe → **44.20 ms minutes later**. It swings **6.7 → 469.9 ms on identical source**.
2. **The machine drifts ±226 ms in 13 minutes** on the non-teardown part of the row.
3. **Therefore: a movement below ~250 ms on `operation_ns` is not testable at one sample per arm.**
   Round 2's 27 ms query fix was **reverted rather than landed** for exactly this reason, and round
   4's collision hoist was claimed on `diag_validate_ns` (count-driven) rather than on `operation_ns`.
   **Choose an instrument that matches the size of the effect you expect**, and say which one carried
   the claim.
4. **A warm cache must never credit a timed phase.** Declare the cache state; never pool cold and
   warm rows.

---

## 5. Workstreams, in the order I would take them

### A. The database itself — the largest untried structural lever

Measured on the row's own store: **`page_size` 4096, `page_count` 82,129, `freelist_count` 0,
`cache_size` −2000 (the SQLite default — the product never sets it)**, and `dbstat` says
**`object_packs` = 333,053,952 bytes of the 336,400,384-byte store** (99 %), with `objects` 1,404,928,
`objects_save` 1,101,824, `content_signatures` 651,264, `signatures_save` 114,688, `packs_save` 20,480.

**A 4 KiB page means ~100,000 page writes for ~400 MB, and every one of them pays a journal image
copy, a pager bookkeeping pass and a write.** A 16 KiB or 64 KiB page cuts that count 4× or 16× for
the same bytes. **`PRAGMA page_size` is a Store-creation decision** (it only takes effect before the
first table exists, or on `VACUUM`), so this is a fixture and case change — allowed, and it must be
declared, because the store stops being byte-comparable with the current baseline. The campaign
refuted **`cache_size`** (flat 2 MiB→128 MiB) and **blob size** (4 KiB→4 MiB, larger was faster); it
**never tried the page size**, and issue #218's own title names it. Note that a larger page interacts
with `cache_size` (a 2 MB cache holds 32 pages of 64 KiB), with the pack `zeroblob` capacity, and
with the `content_signatures` ring — measure, do not assume.

Also worth asking of the DB: why is `object_packs` 333 MB when only 302 MB is bodies? The difference
is the per-pack capacity padding (1,268 packs × 256 KiB). Is a **growth schedule** (L62's
`max(32 KiB, pack_limit/8)`) better than full capacity now that the rewrite is gone? Price it in
bytes written, not in bytes resident.

### B. Algorithmic complexity in the save path

Nobody has audited this. Known shapes to check, each with its call count on this row:

- `MutationOwner::pending_member` is a **linear scan of every lane's open group members**, called once
  per offered object (25,245 times) and again inside `seal_pending`;
- `Availability` (a `BTreeMap`-backed set) is consulted per object and per reference;
- `lookup::candidates`/`present` **build their SQL text with `format!` + a per-placeholder `String`
  join on every page**, and the text width varies with the page, so the prepared-statement cache
  thrashes across 600 waves (round 2 refuted *fixing the width*, not *fixing the building*);
- `Candidates` and `PoolIndex` are bounded rings — check the bound is doing the work it claims;
- `wave_rows` (round 4) grows per wave; `sealed_rows` is scanned per wave.

**The rule of thumb: anything called once per object, per seal or per row that walks something else is
a candidate, and the row gives you the counts to price it.**

### C. Batching, and what is left of it

Commits are done (800; the step is the wave). What is *not* batched:

- **object rows**: 25,245 rows in **16,595 statements — 1.5 rows per statement**, 155 ms. The
  locator row cannot be deferred (invariant 2 below), but the *statement* could be if a same-save
  reader consulted an in-memory wave map first. That is the PR-C1b wall; price it before climbing it.
- **pack writes**: 16,802 calls at ~14 µs. Most of that is the ownership guard `SELECT` and the
  `blob_open`/`blob_close` pair rather than the bytes. **One blob handle per lane per wave plus a
  per-pack guard cache is worth an estimated 60–90 ms** and is the cheapest thing on this list.
- **placements**: one group per `select_many` call. The lane already has the machinery for many.

### D. The v0.1.6 comparison (§3).

### E. C1 construction — 358.7 ms, 19.7 %, inside the declared region

`build_filesystem`/`update_filesystem` over 10,100 bindings, plus 382 metadata objects through the
product's accept path. Split it before optimizing it: how much is C1's tree build and how much is the
product's accept path? If it is C1, this is a `layerfs-content` round.

---

## 6. Two invariants that are walls — do not walk into them twice

1. **Pack sharing.** `tests/cas_reuse.rs` asserts that two whole-file objects saved in one operation
   **share a pack** and that reading both reads **one** pack. Any scheme that gives a group its own
   pack is a contract change. *(This refuted PR-A1.)*
2. **Intra-save delta candidacy.** `tests/delta_payload.rs` asserts that an object admitted **earlier
   in the same save** must be usable as a delta base by a later one, with no predecessor supplied by
   the caller. Only a *placed* row can be a base, so **the locator row cannot be deferred**. *(This
   refuted the full batching treatment, PR-C1b.)*

Both are green today, and both are green **because the landed changes alter what is written, never
when**. If your treatment changes *when*, you own those two tests.

---

## 7. Already closed — do not spend time here

| candidate | status |
| --- | --- |
| transaction cadence | **done**: one transaction per wave (800), `begin_ns` 257.5 → 10.8 ms |
| the pack-append rewrite | **done**: amplification 7.5917× → 1.0013×, 2,292,865,337 → 302,406,480 bytes |
| pack geometry (lower `PACK_LIMIT`) | **dead**: its entire value was the rewrite |
| `journal_mode = MEMORY` → `OFF` | refuted earlier (0.927×), and the teardown is identical in all four modes |
| `cache_size` | refuted: flat 2 MiB → 128 MiB, and the product never sets it |
| blob / pack size | refuted: larger blobs are faster |
| locator / presence queries as a *guard* | refuted at product level: `sql_ns` moved 12 µs of 796 ms |
| positional delta encoding | unused: `prefix_records` = 4 of 25,245 |
| chunking | cleared byte-exact: the `1-8 / 32-256 / 1024-8192` bands are weights, not byte sizes |
| compression / codec as a *bucket* | not implicated: `full_ns` 13.5 %, `group_ns` 0.9 % — but see workstream A/B before dismissing the codec's *level* |
| fixed-width `IN` lists | refuted with numbers: 128-term `IN` is an ephemeral-index build, `collision_query_ns` 81.4 → 511.1 ms |

---

## 8. Deliverables

- **A measured movement toward 1 s on a pre-registered treatment**, with the row PASS, the root
  digest accounted for, and the invariant tests green — or a refutation recorded as carefully as the
  ones already in the evidence directories.
- **Every claim sourced** to a receipt field, a raw file or a `file:line`, or marked `NOT_MEASURED`.
- **Evidence in the repo's own style**: `docs/roadmap/0.1/0.1.7/evidence/<topic>-<UTC>/` with
  `README.md`, the pre-registration and raw receipts beside it. Never overwrite an existing evidence
  file.
- **A ledger entry** per round, and **a status comment on #219**. Close nothing on this handoff alone.
- **Production LOC** in every commit message: `Production LOC: <before> -> <after> (delta <signed>)`,
  with the counting method, computed from the exact parent and the committed tree —
  `python3 tools/production_loc.py --root <tree>`. Test, docs and tooling changes report the
  unchanged total and delta 0.

### Mechanics

```sh
WT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000
HB=$WT/core/benchmark/fs-bench-pro-storage-content

# product tests (33 binaries)
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml -p layerfs-storage
# the invariants that matter most for any write-path change
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml -p layerfs-storage \
  --test cas_reuse --test delta_payload --test pack_watermark --test multi_writer \
  --test visibility --test persistence_failure --test pack_locator
# whole core workspace, when the change is large
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml --no-fail-fast

# harness build (the repo-root .cargo/config.toml supplies the aarch64 AEAD flags: run from the repo)
cargo +1.85.1 build --release --locked --manifest-path $HB/Cargo.toml

# one measured row — fresh --out every time, never an existing path
cd $HB && python3 runner.py perf --lane smoke \
  --out benchmark-results/issue219/<your-name>-<UTC> \
  --case pipeline-namespace-10000 --verify full --no-build
```

The row is PASS when 13 gates pass, all 14 pinned counters reproduce and
`digest:filesystem_root` is `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`.
**`tests/golden/expected.tsv` is compiled into the harness binary** — edit it and you must rebuild,
or `--no-build` will silently grade you against the old pin. `pipeline.commits` is pinned at **800**
and `pipeline.pack_bytes_written` is the instrument the format change moves.

Three harness tests (`registry_negative`) fail on a **pre-existing** registry/cardinality drift
(registry 221 rows / `[…,2,5]` against a frozen 220 / `[…,2,4]`); `src/registry.rs`,
`tests/registry_negative.rs` and `tests/golden/registry.tsv` are byte-identical to `HEAD` and the base
commit's own `run.json` records the same mismatch. Repair it if it is in your way, and label the
repair.

---

## 9. The shortest honest summary

The store's write path is fixed: the whole-pack rewrite is gone (7.5917× → 1.0013×), the cadence is
one transaction per wave (17,378 → 800), and the collision check runs once per wave. The row's work is
**1821 ms** where the clean tree was 3487 ms. What is left is **~803 ms of bytes that must be written,
encoded and inserted**, **~664 ms of per-call overhead across four call sites**, and **359 ms of C1
construction the caller performs inside the declared region**. To reach 1 s you must attack the bytes
themselves — and the page size, the codec level and the row/statement shape are where the bytes are
still being paid for more than once. The reference implementation does the same work in 944.9 ms with
73 transactions; find out how, and find out what of it is real.

**Go and find out what it is worth.**
