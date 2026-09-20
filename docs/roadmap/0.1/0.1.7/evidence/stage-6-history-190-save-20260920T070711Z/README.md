# What `storage.accept_loop` is made of: the save's own counters, published

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-20 continuing [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190)
> after L49. **Harness-only change** (no product line), two arms, one sample per case
> per arm, identical harness source in both. Admission `INELIGIBLE`, every budget
> class `NOT_RUN`, and both stride10 rows' reconciliation is `INCOMPLETE` — with the
> cause now measured, below.

## Why this round exists

L49 removed the read path's pack-acquisition term and left the save path untouched
(`storage.accept_loop` +135 ms / −26 ms). That made the save the largest single
sub-phase — 9.13 s of stride10's 16.7 s operation, 18.4 s of stride3's 37.1 s — and
it is the one sub-phase with **no attribution at all**: L42 refused to treat it
because "compression, indexing and transaction work are inside it and were not
separated". The product already computes those figures (`SaveOutcome` carries
`statements`, `presence_queries`, `packs_created`, `pack_appends`, `delta.*`,
`chain.*`, `pool.*`); the harness published one of them (`save.commits`). This round
publishes the rest, **per state**, and re-samples.

## 1. The change: counters that already existed, now visible

`ops/history.rs` (+152 lines, harness only, excluded from production LOC): the
existing `SaveTotals` accumulator gains `pack_appends`, `commits`, `statements`,
`presence_queries`, the `chain.*` and `pool.*` sub-counters, and a `rows()` accessor
that renders them as trace rows. Each state now publishes 29 `history.state.<n>.save.*`
figures, and the chain totals gain the same fields. Nothing new is measured: every
field is read from the `SaveOutcome` the product already returned, at the same place
the existing `delta.*` totals used. `inserted` and `save.commits` keep their existing
keys, so no key is written twice.

Format deviation, recorded rather than swept in: the added code carries 9 rustfmt
hunks (the file already carried 20 at `c4f757514`, and the harness 157 across it).
Reformatting changes the built binary — Rust embeds `panic!` locations, so the
executable hash moves — which would leave the receipts' `binary_sha256` and
`harness_source_seal` describing a source no longer in the tree. The measured source
is therefore retained as measured and the deviation is stated here.

## 2. The runs reproduce L49, and the save path is untouched

Both arms carry the **same** harness source (seal `264e88c5…`); only the product
crates differ (baseline `c4f757514`, clean; candidate `43f06aef7`, clean).

| | baseline | candidate | delta |
|---|---:|---:|---:|
| stride10 operation (sum of children) | 19.638 s | 16.908 s | **−2.730 s** |
| stride10 `storage.accept_loop` | 9.134 s | 9.164 s | **+0.030 s** |
| stride10 `filesystem` | 8.077 s | 5.323 s | −2.754 s |
| stride3 operation | 48.638 s | 36.810 s | **−11.828 s** |
| stride3 `storage.accept_loop` | 18.526 s | 18.441 s | **−0.085 s** |
| stride3 `filesystem` | 24.679 s | 13.013 s | −11.666 s |

L49 measured −2.754 s on stride10 and −11.747 s on stride3; this round reproduces
them as −2.730 s and −11.828 s under a different harness source, with the save path
still flat. Equivalence holds the same way it did: every state root and content
counter equal between arms, both saved Stores hashing to the **recorded constants**
(stride10 `4af37932aa3391b1…`, stride3 `f5c7ff5a6b4f0821…`), and — the new check
this round can make — **every `save.*` counter identical between the arms**, which is
what "the same work, acquired differently" means.

## 3. What the save actually does (chain totals, identical in both arms)

| per inserted object | early (states 2–6) | late (last 5) | ratio |
|---|---:|---:|---:|
| `accept_loop` (whole run, stride10) | 93.6 µs | 212.5 µs | **2.27×** |
| FULL alternatives compressed | 0.829 | 0.877 | 1.06× |
| prefix trials | 0.600 | 0.778 | 1.30× |
| **chain objects read for trials** | **1.039** | **2.409** | **2.32×** |
| **chain edges walked** | **0.433** | **1.603** | **3.70×** |
| encoded bytes read for trials | 1.8 KB | 2.8 KB | 1.56× |
| values given a new ordinal | 0.952 | 0.941 | 0.99× |
| values reusing an ordinal | 0.534 | 1.039 | 1.94× |
| pack BLOB rewrites | 0.834 | 0.889 | 1.07× |
| INSERT statements | 0.814 | 0.861 | 1.06× |
| commits | 0.0252 | 0.0210 | 0.83× |

Stride10 totals: 45,239 FULL preparations, 38,230 trials (38,163 choosing PREFIX),
39,080 PREFIX records, **107,628 chain objects read** and **133.9 MB of encoded bytes
read for trials**, 48,925 new values, 53,355 reused, 1,737 value groups, 44,331
statements, **45,791 pack BLOB rewrites**, 1,149 commits, 255 packs.

**The save path's depth term is delta-base acquisition.** It is the only per-object
quantity that grows like the per-object cost (chain objects 2.32×, chain edges
3.70×, against 2.27× for the time), and the only one that reads anything: a trial must
reconstruct the base it compresses against. Everything else per object is flat —
compression 1.06×, appends 1.07×, statements 1.06×, commits 0.83×.

## 4. The write pattern: 97.8× amplification, sized at ~1.03 s

The Store the run wrote shows why `pack_appends` is 45,791 for 255 packs: **44,141 of
the 52,032 objects sit in groups of exactly one** — the `WholeFile` lane seals on
every record by construction (`selection.rs::offer`: `WholeFile | PooledMetadata |
Singleton => occupied`), so each payload object is its own group, and each group
placement rewrites the whole pack BLOB (`sqlite/write.rs::append_pack`:
`UPDATE object_packs SET data = ?2`). Median pack: 245 groups. The store's own shape
therefore implies 45,791 appends and **4.43 GB of BLOB bytes rewritten to store
45.3 MB — 97.8×** — and it also explains the commit count: `write_pack` charges the
*whole rewritten pack* to the transaction's 4 MiB byte budget, so a commit is forced
every ~40 appends, which is the measured 1,149.

[`append_cost.py`](append_cost.py) replays that exact pattern — same pack ids, same
group counts, same intermediate sizes, same pragmas (`journal_mode = MEMORY`,
`synchronous = OFF`), same commit cadence — against a scratch copy of the retained
Store: **46,046 updates and 1,181 commits in 1.03 s**, of which 0.15 s is the
in-memory pack rebuild (a Python concatenation, an *upper* bound on the Rust copy it
stands for) and 0.88 s is SQLite. **This is a synthetic bound on one pattern, not the
product's cost**: the real save interleaves five lanes, compression, reads and commits
between these updates. It says the pattern is worth at most about a second of
stride10's 9.13 s — credible against the owner's one-second bar, not clearly above it.

## 5. The reconciliation gap is explained, and it was never the product

L49's `candidate-history-stride10` reconciled `INCOMPLETE` with 1.883 s outside the
child's own clock, and its start-up probe (35.8–42.2 ms) appeared to rule out
first-execution cost. **That probe was wrong: it ran after the first execution.** A
freshly created executable's *first* run on this machine costs far more than its
second:

| binary | execution 1 | execution 2 | execution 3 |
|---|---:|---:|---:|
| fresh copy A | **1,959.5 ms** | 38.8 ms | 38.9 ms |
| fresh copy B | **669.4 ms** | 34.8 ms | — |

All of it is dynamic-loading/code-validation before `main`, i.e. outside
`phases::begin`. That explains every observation in the family: L47's retained
stride10 (old binary) 0.066 s PASS; L49's baseline (old binary) 0.072 s PASS; L49's
candidate (freshly built) 1.883 s; this round's baseline (freshly built) 2.018 s and
candidate (freshly built) 1.823 s; and every stride3 row ~0.04 s, because by then
both binaries have already run once. Both stride10 rows here are therefore reported
`INCOMPLETE` with a **measured, non-product cause**, and the operation figures they
carry are unaffected (the cost is outside the child's clock, which is why the
operation comparison reproduces L49 to within 24 ms). A future round that wants a
reconciling stride10 wall should execute each freshly built binary once before the
measured sample and declare it; this round did not re-run a case to obtain one.

## 6. Decision: no treatment this round, and what the next one should do

The two candidate targets inside the save path are now sized as far as counters can
size them:

* the **write pattern** (whole-pack BLOB rewrite per group, plus the commits its byte
  charge forces) is bounded at **~1.03 s** by the synthetic replay — real, but not a
  comfortable one-second target, and its fix (keep the open tail in memory and write
  the pack once per seal) touches the visibility and atomicity of the open tail;
* the **depth term** (delta-base acquisition: 107,628 chain objects, 133.9 MB read)
  has no time attribution at all, because `storage.accept_loop` is one span.

So the next step is **one more attribution layer, with time rather than work**:
bounded aggregate spans inside the accept path (selection and chain acquisition /
compression / placement and write / SQL), identical in both arms, kept separate from
unprofiled samples. That is what turns this round's work profile into a time profile,
and it is the evidence the owner's confirm-then-treat rule requires before a write-path
treatment is written. Writing a treatment now would be choosing between a ~1 s bound
and an unsized term on the strength of a synthetic replay — which is the mistake L42
declined to make.

## 7. Checks

Harness-only change, so the harness set ran and the core set was not required:

* `cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml
  --locked`: **117 passed, 0 failed** across 14 test binaries (`checks/harness-test.txt`).
* `cargo +1.85.1 build --release … --locked`: **PASS**, with the inherited
  `unused_mut` warning at `src/ops/history.rs:1632` recorded, not fixed
  (`checks/harness-build.json`, `checks/build-baseline.json`).
* Harness format: **not clean and not claimed** — `history.rs` carries its 20
  pre-existing hunks plus 9 in the added code; the harness carries 157 across it
  (`checks/harness-fmt.txt`). §1 states why the measured source was kept.
* Harness Clippy was **not** re-run: the added code is data plumbing in one file whose
  inherited Clippy set (21 warnings, 1 denied `never_loop` at `src/ops/history.rs`) is
  recorded as inherited.
* Core checks (`cargo test/clippy/fmt --manifest-path core/Cargo.toml`,
  `check_product_boundary.py`): **not run** — no product line changed, and the L49
  treatment's own run of them is recorded at `d57d6f9aa`.

Every build and performance command ran under both global flocks after the quiet
preflight. Nothing was refused, deferred or repeated: the four performance
invocations all exited 0 on their first attempt.

## 8. Identities and reproduction

Baseline arm `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-baseline` (detached at
`c4f757514`, harness patch applied), candidate arm
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope` (`43f06aef7`, same patch).
Harness source seal `264e88c5d379dac36193b5a2182ec479ead061c0db38e5b9b5d718a337deb566`
in both. Binaries: baseline `079ae5e0a5b59d4918d7861aa1227608686d87c6053e8badeeb84403d29dc6db`,
candidate `1822ec21a9d2a362698a5ca2f6dac1c2b68b517f1a8e108bc4a261845c7e9f99` (rebuilt
after the format question and confirmed identical, so the measured executable is the
tree's source). Rust 1.85.1, `--locked`, Python 3.14.3, corpus manifest SHA256
`03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`. All eight
behavioural history switches unset, `LAYERFS_HISTORY_PHASES=1`,
`LAYERFS_CONSTRUCTION_WORKERS=1`.

```
python3 with_locks.py perf-baseline-stride10  python3 collect.py baseline  history-stride10
python3 with_locks.py perf-candidate-stride10 python3 collect.py candidate history-stride10
python3 with_locks.py perf-baseline-stride3   python3 collect.py baseline  history-stride3
python3 with_locks.py perf-candidate-stride3  python3 collect.py candidate history-stride3
python3 analyze.py        # phases.compose / receipt.budget are the runner's own
python3 with_locks.py synthetic-append python3 append_cost.py
```

## 9. Limits

* One sample per case per arm; both stride10 rows are `INCOMPLETE` with the cause in
  §5; both rows are diagnostics with admission `INELIGIBLE` and budget `NOT_RUN`.
* The counters localise **work**, never time. Nothing here splits `accept_loop`'s
  9.13 s into compression, chain reads, placement and SQL; §6 is the plan for that.
* The 1.03 s in §4 is a **synthetic upper bound** on one write pattern, not the
  product's measured cost, and it was measured on a scratch copy of a retained Store.
* `stride1` was not sampled; the scaling claim is not asserted.
* The v0.1.6 comparison is untouched, no pin was written, no budget class changed, no
  cap enlarged, no selection shrunk, no cold claim invented, and `#190` stays open.
