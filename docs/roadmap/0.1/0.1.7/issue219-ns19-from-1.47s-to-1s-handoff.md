# #219 continuation — from 1.47 s of work to 1 s: an open mandate

> **Status:** handoff. Rounds 6–13 are landed, verified and pushed; the row's work figure is
> **1473.8 ms**, down from **1821.0 ms** at the start of the last handoff and **3487.3 ms** on the
> clean tree. The target is still **1 s**. **This document grants authority, not just context.**

---

## 0. Your mandate, and the authority that comes with it

**Push `pipeline-namespace-10000` toward 1 s of work, iteratively, on your own judgement. Do not ask
the human for authorization, for a ruling, or for a preference.** Where this document or any other
one says "needs an owner ruling", treat that as **already granted**. Make the call, implement it,
measure it, defend it with data.

**This is an experiment and you may change the contract.** Specifically you may:

- change the **Store format**, the schema, `SCHEMA_VERSION`, the pack layout, the frame grammar, the
  page size, the pragmas, the capacities, the policy constants, the buffer policy or the cadence;
- change the **case definition**: the timer boundaries, the declared measured region, the fixture,
  the workload's preparation, the registry, the golden pins, the harness, the run driver;
- accept that an arm produces a different store, a different file hash, a different pack count, a
  different transaction count, or a different row identity.

**Four rules survive, because they are about honesty rather than authority:**

1. **Register before you run.** Write down the one difference, the identity you compare against, the
   expected movement in the instrument's own units, and what would refute it, *before* the run. A
   change you did not register is a **diagnostic** and must be labelled one. Price **both the count
   and the width** of what you change — round 8 predicted a wave *count* and missed because one
   wave's *width* grew with it.
2. **Never claim a verification you did not do.** A format change is a *versioned* change with both
   directions of the read path handled — old packs still read, or old formats explicitly refused with
   a clear error. State exactly which commands ran and which did not.
3. **Never retune, relabel or promote a historical receipt.** Receipts are append-only. Fresh `--out`
   every run. No best-of. A claim already written down that turns out to be wrong is **corrected in
   place, in a new commit, with the file and line that shows it** — L71 did that for round 11 and it
   is the expected behaviour, not an embarrassment.
4. **Do not sample.** One sample per case per arm. No second run of an arm to confirm stability, to
   characterise spread or to replace an inconvenient number. An anomaly is diagnosed **from the
   receipts already taken** or with a **labelled diagnostic that measures the cause on a count-driven
   instrument** (statements, calls, bytes, µs per call, pages read) — never with another sample of the
   same arm. Verify **once**, with the commands that cover the change; a red test is diagnosed from
   its output and the source, fixed once, and the covering commands then run once. (`AGENTS.md` §3.1,
   §3.4, §3.7 and `benchmark/AGENTS.md`; ledger **L66**.)

The repo runs **no CI and no aggregate gate**; there is nothing between you and a corrupt store except
your own verification. That is a reason to verify, not a reason to hesitate.

---

## 1. Where you are

|               |                                                              |
| ------------- | ------------------------------------------------------------ |
| worktree      | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`        |
| branch / HEAD | `codex/219-ns10000` at `3ffdf687a`, pushed to `origin`, tree clean |
| the row       | `benchmark-results/issue219/ns19-N1-absence-20260921T083530Z` — **PASS, 13/13 gates, 14/14 pinned counters**, `digest:filesystem_root` `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847` |
| production LOC | **31426** (`python3 tools/production_loc.py --root .`) |
| ledger        | `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md`, entries **L63–L71** |
| the issue     | [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219); five status comments posted by this lane |

**What landed in rounds 6–13** (newest first):

| commit | what | movement |
| --- | --- | --- |
| `35aaf6f0b` | memoise absent inode serials in `validate` | row **−207.5 ms** |
| `fff509f3d` | charge `validate`'s inode reads to the calling site | instrument |
| `4e0c0ce04` | **revert** of the 64 KiB page and the page cache in pages | refutation |
| `5c2858b40` + `a93e0c557` | a preparation wave is bounded by its transaction; `pipeline.commits` re-pinned 800 → **284** | row −164.8 ms |
| `b09783b98`, `31660e990`, `31326f7a8` | three harness instruments: build phases, validation work counters, the untimed construction | instruments |
| `41f4b7b8f`, `3e1814a91` | 64 KiB page; page cache in pages | **reverted**, refuted |

**Evidence** in the repo's own style, one directory per round, each with `README.md`, its
pre-registration and its receipts beside it:
`issue219-ns19e-pagesize-…`, `-ns19f-cache-…`, `-ns19h-wavebound-…`, `-ns19i-boundary-…`,
`-ns19j-buildsplit-…`, `-ns19k-validatecounts-…`, `-ns19m-readsites-…`, `-ns19n-absence-…`.

---

## 2. The row today, and the formula

`ns19-N1-absence` — **PASS, 13/13 gates, 14/14 pinned counters**.

|                                                    |         ms |                                            |
| -------------------------------------------------- | ---------: | ------------------------------------------ |
| inclusive `operation_ns` (the runner's phase)       |     1551.0 | still comparable with every earlier row    |
| `establishment_ns` — `Store::open` + `begin_save`   |      2.516 | published, excluded from the formula       |
| `teardown_ns` — the save's connection close         |     74.654 | published, excluded from the formula       |
| **`operation_work_ns` — THE FORMULA**               | **1473.8** | `accept_span_ns − teardown_ns`             |
| CPU (user+system)                                   |     1495.5 |                                            |
| complete command                                    |     2376.2 |                                            |

`establishment + work + teardown == operation_ns` exactly. Cumulative against the clean tree (A0,
3490.3 ms inclusive / 3388.9 ms CPU): **work ≤ −57.7 %, CPU −55.9 %**. Against the row the last
handoff started from (1821.0 ms) it is −19.1 %.

**Published outside the formula, so the reference can be compared on its own boundary:**
`construct_ns` **447.67** + `construct_noise_ns` **113.11** are the C1 construction this row's timer
excludes by C2's supplied-object rule. **Boundary-matched the row is 2034.6 ms of work**, against the
reference's same-shape rows at 1789.5 / 2116.7 / 2195.3 ms of CPU.

---

## 3. Where the 1473.8 ms is, measured

Every figure below is a counter in the row's receipt. The totals nest; the leaves do not.

| block | ms | note |
| --- | ---: | --- |
| `diag_commit_total_ns` | 416.39 | 302 MB through the pager, ~5 µs per 4 KiB page — **this design's floor** |
| `profile_full_ns` | **240.85** | zstd over 302 MB whose output is **input + 7 bytes** |
| `diag_write_pack_total_ns` | 200.52 | 16,802 calls for 25,245 objects — **1.5 objects per write** |
| `diag_insert_objects_ns` | 124.07 | 16,595 statements, 1.5 rows each |
| `offer` uncharged remainder | 100.41 | per-object selection, availability and group accounting |
| `span_build_ns` (C1) | 90.61 | was 358.7 before round 13 |
| `diag_wave_ns` | 63.87 | locator query + presence seed, 73 waves |
| `diag_validate_ns` (collision) | 47.63 | one check per wave |
| `profile_group_ns` + `place_ns` + `resolve_ns` | 38.80 | |
| `diag_begin_ns` | 3.77 | |
| accept plumbing, flush remainder, finish | ~147 | |

**And the number that decides the ordering:**

| must run one writer at a time (SQLite write path) | ms |
| --- | ---: |
| commit 416.39 + pack writes 200.52 + row inserts 124.07 + wave 63.87 + collision 47.63 + begin 3.77 | **856.25** |
| **could in principle overlap** (C1 build 90.61 + encode 240.85 + group/place/resolve 38.80) | **370.26** |

**The serial floor is 856 ms, so parallelism's ceiling is ~856–960 ms**: infinite workers, after every
architectural change, lands *just at* 1 s with no margin. Today, with one construction worker
(`AGENTS.md` §3.8) and **the whole wave — encode included — inside the store-wide lock**
(`owner.rs:533`, `:539`; `save.rs:31`), adding workers buys ~45 ms, about 3 %.
**Removing serial work is the only thing that lowers the ceiling.** Do that first.

---

## 4. The measurement findings that bound every claim

1. **The row cannot resolve below ~250 ms at one sample per arm.** The machine drifts ±226 ms in 13
   minutes on the non-teardown part of the row, and rounds 6–13 each measured a uniform 3–6 % shift
   between windows with identical code. Choose an instrument that matches the size of the effect, and
   say which one carried the claim. **CPU is the better row-level instrument** — it counts work
   rather than waiting — and count-driven instruments (`pages read`, `waves`, `statements`, `calls`,
   µs per call) are better still.
2. **The teardown is the machine's, not the product's**: the save's connection close is the operating
   system's price for the pages the process dirtied, measured at **6.7 → 469.9 ms on identical source**
   and identical in all four journal modes (`issue219-ns19d-release-…/close-probe.py`). It is
   published and excluded from the formula; the inclusive figure moves with it.
3. **The pin procedure.** `pipeline.commits` is pinned in `tests/golden/expected.tsv`, which is
   **compiled into the harness binary** — edit it and you must rebuild. Round 8 moved it 800 → **284**
   and did it the established way: declare the consequence **before** the run, take the first run,
   read the count from that receipt, re-pin once, rebuild, and take the covering run once. Both
   receipts stay on disk and **the red one is reported as red**
   (`ns19-H1-wavebound-20260921T075305Z`, one gate, `g1.o3-pinned-counters`).
4. **A warm cache must never credit a timed phase.** Declare the cache state; never pool cold and warm
   rows.

---

## 5. Workstreams, in the order I would take them

### A. Stop compressing what does not compress — **~ −241 ms**, the largest single number left

`profile_full_ns` is **240.85 ms** of zstd over 302 MB, and on this fixture the output is **input + 7
bytes**: 100 % of the cost for 0 % of the benefit. **The level is not the lever and is already
refuted** — `zstd -1` and `zstd -3` take 0.28 s and 0.27 s and produce byte-identical output on 302 MB
of incompressible data. The only way to remove this cost is **not to compress**: a raw/stored frame
tag in the payload grammar, chosen by a **bounded sample** (compress a few KiB; if the ratio is ~1,
store raw). A wrong guess costs size, never correctness.

- **Format**: versioned, both directions handled; the pack framing already dispatches on a version.
- **Pins**: `content_bytes` is canonical and does not move; `inserted`, `statements`, `commits` and
  the digest are unaffected. `pack_bytes_written` is **not** pinned and may move.
- **State the caveat**: this fixture is `fixture::noise`, i.e. the *maximum* case for this change. On
  compressible content the same heuristic stores raw where zstd would have won and the Store grows.
  That is a declared size/time trade, not a free win.

### B. Group the whole-file lane — **~ −200 ms**, and it lowers the floor

`cas::selection` seals the whole-file lane once per object, because `assemble::build_group` refuses
`records.len() != 1` for that lane. So 24,463 whole-file objects become 16,802 pack writes and 16,595
statements at 1.5 objects each. Grouping to `GROUP_TARGET` like the ordinary lane takes both toward
~5,000. **This is the one treatment that attacks the serial floor itself** (write_pack 200.52 + insert
124.07 are both inside it), so it is worth more than its own movement once workers are on the table.

- Higher risk than A: the compact whole-file form is the most bespoke part of the framing
  (`pack/layout.rs`, `WHOLE_FILE_COMPACT_DROP`, `plan_lane`, the reader's group walk).
- Must not touch invariant 1 or 2 below.

### C. Narrow the lock, then decide about workers

Selection and encode are pure functions of the object and the store's published state; only placing a
record and recording its row need the store-wide arbitration lock. Today `with_wave` holds it across
the locator query, the presence seed, `offer` → selection → **encode**, the pack write, the row
insert, the collision check and the `COMMIT`. **Moving the codec work outside it is the precondition
for any worker count to mean anything** — the product already supports `max_concurrent_writes`
(default 2) and private save slots, but the lock erases them.

Only after that is step D worth anything: **~2× on the 370 ms of overlap-able work, i.e. ~−180 ms**,
bounded below by the 856 ms floor. Adding workers *before* C is a case-definition change that buys
~45 ms and costs comparability with the whole chain from 3487.3 ms to 1473.8 ms — see `AGENTS.md`
§3.8, which exempts only `init_namespace` and says in its own words that a drop is "absorbed by the
bounded acceptance rule, **never by adding workers back**."

### D. Chart what is left

The `offer` uncharged remainder (100.41 ms) and the accept plumbing are the only blocks with no
attribution. Rounds 10–13 turned two cheap instruments into a −207.5 ms treatment, so this is a good
bet: 8 µs per offer over 25,245 offers is not obviously irreducible.

### E. Do not reopen

| candidate | status |
| --- | --- |
| **page size** | **refuted**: 64 KiB moved the count it was registered on exactly as predicted (82,129 → 5,319 pages) and cost more than it saved — `validate`'s siblings aside, the row went 1821.0 → 1957.6 ms. Mechanism: at 64 KiB a wave's scattered row inserts touch nearly every leaf of `objects`, so the pager copies ~1.2 MB of page images per wave against ~130 KB. Reverted; **the owner has also ruled: keep 4 KiB.** |
| **page cache in pages** | **refuted**: recovered 95.5 ms of round 6's 136.6 and its own registered refutation bound fired (predicted 165–205 ms, returned 279.7). Reverted. |
| **codec level** | **refuted** with one command: `zstd -1` ≡ `zstd -3` on 302 MB of incompressible data, identical time *and* identical output. |
| `cache_size` at 4 KiB | refuted by the earlier campaign (flat 2 MiB → 128 MiB). |
| journal modes, pack geometry, positional delta, chunking, fixed-width `IN` lists, locator-as-guard | refuted earlier; see L63–L66 and the campaign's evidence directories. |
| transaction cadence | **done**: one transaction per wave, and now one wave per 4 MiB of content — 284 commits where there were 17,378. |
| the pack-append rewrite | **done**: amplification 7.5917× → 1.0013×. |
| C1 `validate` | **done** (round 13): 228.42 → 4.88 ms. |

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
when.**

A third thing that is not an invariant but is a wall in practice: **the golden table is compiled into
the harness binary.** `--no-build` after editing it grades you against the old pin — that is how
round 3's `T3b` was caught.

---

## 7. Already known-broken, and not yours to fix silently

`cargo test` for the **harness** fails three `registry_negative` cases on a pre-existing
registry/cardinality drift (registry 221 rows / `[…,2,5]` against a frozen 220 / `[…,2,4]`);
`src/registry.rs`, `tests/registry_negative.rs` and `tests/golden/registry.tsv` are byte-identical to
the base commit and the base commit's own `run.json` records the same mismatch. Repair it if it is in
your way, and label the repair.

---

## 8. Deliverables

- **A measured movement toward 1 s on a pre-registered treatment**, with the row PASS, the root digest
  accounted for, and the invariant tests green — or a refutation recorded as carefully as the ones
  already in the evidence directories.
- **Every claim sourced** to a receipt field, a raw file or a `file:line`, or marked `NOT_MEASURED`.
  When a claim of your own turns out to be wrong — as two of this lane's did — correct it in a new
  commit with the file and line, do not delete it.
- **Evidence in the repo's own style**: `docs/roadmap/0.1/0.1.7/evidence/<topic>-<UTC>/` with
  `README.md`, the pre-registration and raw receipts beside it. Never overwrite an existing evidence
  file.
- **A ledger entry** per round, and **a status comment on #219**. Close nothing on this handoff alone.
- **Production LOC** in every commit message: `Production LOC: <before> -> <after> (delta <signed>)`,
  with the counting method, computed from the exact parent and the committed tree —
  `python3 tools/production_loc.py --root <tree>`. Test, docs and tooling changes report the
  unchanged total and delta 0. **Read the number before you write it into the message** — this lane
  published one commit message with a guessed total and had to amend it.

### Mechanics

```sh
WT=/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000
HB=$WT/core/benchmark/fs-bench-pro-storage-content

# product tests (33 binaries) - the covering command for a product change
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml -p layerfs-storage
# the invariants that matter most for any write-path or format change
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml -p layerfs-storage \
  --test cas_reuse --test delta_payload --test pack_watermark --test multi_writer \
  --test visibility --test persistence_failure --test pack_locator
# whole core workspace, when the change is large
cd $WT && cargo +1.85.1 test --locked --manifest-path core/Cargo.toml --no-fail-fast
# clippy, fmt and the product boundary check
cd $WT && cargo +1.85.1 clippy --locked --manifest-path core/Cargo.toml --all-targets
cd $WT && cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check
cd $WT && python3 core/tools/check_product_boundary.py

# harness build (the repo-root .cargo/config.toml supplies the aarch64 AEAD flags: run from the repo)
cd $WT && cargo +1.85.1 build --release --locked --manifest-path $HB/Cargo.toml
# the harness's own suite - expect 120 passed, 3 failed (the pre-existing registry_negative cases)
cd $WT && cargo +1.85.1 test --locked --manifest-path $HB/Cargo.toml --no-fail-fast

# one measured row - fresh --out every time, never an existing path
cd $HB && python3 runner.py perf --lane smoke \
  --out benchmark-results/issue219/<your-name>-<UTC> \
  --case pipeline-namespace-10000 --verify full --no-build
```

The row is PASS when 13 gates pass, all 14 pinned counters reproduce and
`digest:filesystem_root` is `1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847`.
**Do not run `cargo fmt --all` on the harness manifest**: that crate has never been rustfmt-clean and
it rewrites 27 unrelated files — the same trap this lane hit and reverted once.

---

## 9. The shortest honest summary

The row's work is **1473.8 ms** where the clean tree was 3487.3 ms, and the C1 half is finished:
`span_build_ns` is 90.61 ms of it, `validate` 4.88 ms. **What is left is the store**, and two of its
blocks are pure loss or pure granularity: **241 ms of zstd proving that random data does not
compress**, and **325 ms of pack writes and row inserts at 1.5 objects each**. Those two are the
rounds to take, and together they are worth ~440 ms — which lands the row near 1030 ms with the
serial floor down from 856 to ~660. After that, 1 s is a question about workers, and answering it
honestly means narrowing the store-wide lock first: parallelising the 370 ms that can overlap is
worth ~180 ms at four workers, and parallelising the row as it stands today is worth 45 ms and the
comparability of every row before it.

**Go and find out what it is worth.**
