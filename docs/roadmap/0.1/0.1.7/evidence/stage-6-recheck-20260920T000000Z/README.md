# Stage 6 re-check at HEAD — the 217 lane is stale, and the 157 row is not closable yet

> **Status: diagnostic re-check.** Append-only; it supersedes nothing and edits no
> earlier receipt. It is **not** a lane run, **not** an admission sample and makes
> **no timing claim**. No product, harness or test source was changed.
> HEAD: `795fb1a2f792742a2b19fdb82d98e6fd8a8b0470` (`feat(core): retained-history
> storage — C1 chunk cursor, C2 persisted similarity index, lean row grammar`).

This note records a read-only re-check of two questions:

1. does Stage 6's closure still hold at HEAD? and
2. can the `history-stride1` (157-checkpoint) dedup row be closed today?

## 1. Stage 6 is closed at `90bbb617d` / `2f8ebc90d`, and HEAD is past both

| receipt | source commit | 217 admission rows |
| --- | --- | --- |
| round 4c | `90bbb617de4a43bfb5443aadd590bc37901eab1b` | 217 PASS / 0 FAIL / 0 NOT_RUN |
| round 5b | `2f8ebc90d751d996bbc84581e1f8129690870c67` | 217 PASS / 0 FAIL / 0 NOT_RUN |

`795fb1a2f` then changed the product (codec `GROUP_LEVEL` 1 -> 19, workspace 2 -> 16
MiB, `SCHEMA_VERSION` 4 -> 6, `objects.base_object_id` and two indexes removed, the
persisted `content_signatures` index added) and the harness (ordering backing always
supplied; `FIXTURE_RECIPE_VERSION` v2 -> v3). The full 217-row lane has **not** been
re-run since; the commit says so itself ("NOT RUN: the full 217-row lane").

### 1.1 The one red row at HEAD, reproduced and bisected

`runner.py perf --lane smoke --verify full` at HEAD content reports **19 PASS / 1 FAIL**;
the failure is `pooled-lane-cold`, `g1.o3-pinned-counters`:
`pool.commits 19 -> 18` (`tests/golden/expected.tsv:1312` pins 19 from round 4c).
`pool.commits` is `SaveOutcome.commits`, the number of write transactions acknowledged
with `COMMIT` (`cas/lifecycle.rs:129-145` bounded commits + `:216-219` the final one).

**Reproduced deterministically** — the two runner receipts at HEAD content, both 18:
`/tmp/perf2/pooled-lane-cold/receipt.json`, `/tmp/perf3/pooled-lane-cold/receipt.json`
(both `source_commit 66bce8378` + dirty tree, i.e. the content of `795fb1a2f`;
`harness_binary_sha256 8da16a29…`).

**Bisected on frozen campaign binaries** (raw binary, one invocation each, sequential,
`LAYERFS_CONSTRUCTION_WORKERS=1`, `--case pooled-lane-cold`, fresh `--out`):

| binary | sha256 | `content_signatures` | `pool.commits` | trace sha256 |
| --- | --- | --- | --- | --- |
| `/tmp/lane-baseline` | `0ae3712111c1fb67…` | absent (pre-W2/W3) | **19** | `e6bf9eea56cde801…` |
| `/tmp/lane-after` | `66aaa0946e6b8274…` | present (W2+W3) | **18** | `aa707bab3656cd70…` |

So the counter moved with the **W2/W3 product change**, not with the codec change.
The mechanism is **not identified**; the accounting that decides a bounded commit is
`cas/placement.rs:168-169` (`member.canonical_length`) and `:199-200`
(`write.bytes.len()`), plus `cas/pool_lane.rs:267-268` (`group.body.len()`).

**A third invocation is reported and discarded as evidence:** the checkout's
`core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content`
(`sha256 2da6f07ceb627e39…`) contains **no `content_signatures` string** — it is a
stale pre-W2 build — and it measures 19. It says nothing about HEAD and a fresh build
is required before any lane re-run.

Reproduction:

```sh
cd /tmp
LAYERFS_CONSTRUCTION_WORKERS=1 /tmp/lane-baseline --case pooled-lane-cold --out /tmp/pool_diag_base
LAYERFS_CONSTRUCTION_WORKERS=1 /tmp/lane-after    --case pooled-lane-cold --out /tmp/pool_diag_after
```

### 1.2 Also owed, and unchanged by this note

- `tests/golden/expected.tsv` is pinned to round 4c; a moved counter is a FAIL, not a
  new baseline, so the pin can only be regenerated after the delta is explained.
- `ops/fs.rs:475-478` still sizes the ordering backing by binding count — the shape
  that failed `history-stride3` — and it drives the 217 lane.
- `core/docs/architecture/13-physical-writing.md:73` still reads
  `GROUP_LEVEL = 1` while the shipped constant is 19 and the same document's prose
  (line 82) says 19.

## 2. `history-stride1` (157 checkpoints) cannot be closed today

### 2.1 What exists

Prose only. The claimed result — **80,273,408 B apparent**, verification
**9,996 paths, 0 mismatches, 64.2 / 90.3 s** — appears in
`../stage-6-history-188d-20260920T000000Z/ALL-THREE-TIERS.md`,
`../stage-6-history-188d-20260920T000000Z/VERIFICATION.md` and the closing comment
retained at `/tmp/i186close.md`.

### 2.2 What does not exist

- **No receipt of any kind for `history-stride1`.** No `trace.jsonl`, `receipt.json`,
  `run.json` or `phases-verify.json` for that row exists on this machine (searched
  `/tmp`, `/private/tmp` and `/Users/yifanxu` for `trace.jsonl` files naming a
  `history-stride*` case, and `benchmark-results/fs-bench-pro-storage-content/`).
  The two surviving raw directories are `/tmp/history-stride10` (stride 10 only) and
  `/tmp/history-stride3`, whose trace ends at
  `g1.o1-chain-complete INCOMPLETE | product error: ResourceUnavailable { what:
  "ordering backing" }` — the **pre-fix failure**, not the claimed stride3 result.
  Neither was produced by `runner.py`, so neither carries an identity, a budget
  classification or a `run.json`.
- **The row is `INCOMPLETE` by construction at HEAD.** `tests/golden/expected.tsv`
  contains **zero** `history.*` rows, so `Expected::counter_gates`
  (`src/workload/expected.rs:157-166`) emits
  `g1.o3-pinned-counters INCOMPLETE | no pinned counters for this case` for every
  history row — visible in `/tmp/history-stride10/trace.jsonl`. The row's status is
  the worst gate, so no history row can be `PASS` regardless of its storage result.
- **Most of the specification's gate table is not implemented for this group.** The
  history driver emits `g1.o1-chain-complete` (a **count** of saved states, not the
  pinned-root comparison the spec names), `g1.o4-readback` (the sampled O2),
  `g6.verify-state-count`, `g6.verify-sample-declared` and `g4.swaps`. Absent:
  `g1.o1-state-root`, `g1.o3-pinned-counters` (canonical `871,588,115 B` / `104,705`
  objects at 157), `g1.o4-state-tree` (full metadata equality over every state),
  `g1.o6-below-v016` (**the storage gate** — `V016_ALLOCATED` exists in
  `shared/history_corpus.py` and no gate reads it), `g1.pack-accounting`,
  `g2.handoff`, `g5.one-file`, `g5.no-sidecars`, `g6.store-exists`,
  `g7.tree-complete`.
- **No per-lane complete-command ceiling exists.** `runner.py` classifies every row
  against 15 s, or 25 s for the eight `DECLARED_EXCEPTIONS` rows, which cannot carry a
  157-state run. Owner decision 1 requires per-lane ceilings "declared at Phase 3 from
  the stride-10 baseline"; that baseline was never recorded, so `--lane history-stride1`
  can only be `NOT_RUN` today.
- **The declared verification ceilings are not wired.** `VERIFICATION_CEILING_NS`
  (10 / 20 / 30 s) and `history_corpus.ceiling()` exist and are self-checked, but
  `runner.py` never calls them: the deferred verify invocation is classified against
  the flat `VERIFICATION_BUDGET_NS = 60 s`. Under the ruled 30 s the measured
  64.2 / 90.3 s is over, and the campaign's own cost model (5.24 ms/unit x 10,048
  units = ~52.6 s) says the current sampler cannot fit — the spec's required
  reduction, **O2 deduplicated by distinct object before sampling**
  (`verification.md` §6), is not implemented in the driver.
- **The lane-scoped `sample` ruling is not implemented.** Owner decision 3 allows a
  sampled history row to be `PASS`; `runner.py` forces `INCOMPLETE` for any mode other
  than `full`, and `full` at 157 states is 904,143 path-states (~79 minutes at the
  measured unit cost).
- **The comparison quantity in the evidence tables is not the one the gate names.**
  Ruling 7 gates **allocated** bytes below `49,344,512 / 64,024,576 / 83,947,520`.
  `ALL-THREE-TIERS.md` compares our apparent `st_size` against `64,000,000` and
  `82,685,952` — the retained `store.sqlite` file sizes, which are neither the recorded
  allocated nor the recorded apparent (`82,686,052`, identity files included; see
  `../stage-6-history-187-20260920T000000Z/squad-a/A4-v016-reconstruction.md` §1).
  For reference, on five `188d` core Stores `st_blocks x 512 == st_size` exactly, so
  the apparent figures are also the allocated ones; that equality is a property of
  this Store format on this volume and is stated rather than assumed.
- **The spec's justification for the cross-generation gate has drifted.**
  `retained-history-storage.md` §8 rests on "the Store format is preserved". At HEAD it
  is not (`SCHEMA_VERSION` 4 -> 6, column and indexes removed, `content_signatures`
  added — owner-ruled as C and D, recorded in `crates/layerfs-storage/sql/schema.sql`).
  The comparison may still be legitimate, but the ruling's stated premise no longer
  holds and needs re-affirmation or amendment before a row is closed on it.
- **T1's own target is missed at the shipped configuration.**
  `../retained-history-t1/README.md` registers `47,048,435 B` apparent for
  `history-stride10`; the shipped stride-10 result is `49,053,696` — **2,005,261 B
  above the target**, while clearing the v0.1.6 gate by 262,144 B apparent
  (290,816 B allocated, using the equality above). The target was reached only by the
  W1+W2+W3 arm at payload level 9 (`45,432,832`), and payload 9 was then rejected on
  CPU grounds in favour of level 3. The T1 document also still reads "Nothing here is
  implemented".

## 3. Not claimed

- No lane was re-run; no gate sample was collected; no number here is admission
  evidence.
- No timing claim of any kind.
- The `pooled-lane-cold` invocations are **diagnostics** of one structural counter on
  frozen binaries, reported beside the receipts they describe.
- No product, harness, test or document outside this directory was modified.

## 4. Addendum — what the surviving artifacts actually measure, and one correction

Read-only inspection of the one surviving `history-stride10` run directory
(`/tmp/history-stride10/`, `sample.sqlite` + `timing.json` + `phases-perf.json` +
`trace.jsonl`; **no runner receipt**). This is the Store behind the committed
`49,053,696` figure.

| quantity | value | source |
| --- | ---: | --- |
| apparent (`st_size`) | 49,053,696 B | `stat` |
| **allocated (`st_blocks x 512`)** | **49,192,960 B** | `stat` |
| gate (v0.1.6 stride-10 allocated) | 49,344,512 B | `issue153-retained-history-report.md:26` |
| **margin** | **151,552 B below** | derived |
| objects / canonical bytes | 52,032 / 380,921,328 B | `SELECT` |
| packs / pack bodies | 255 / 45,035,732 B | `SELECT` |
| `PRAGMA user_version` · `quick_check` | 6 · `ok` | `PRAGMA` |
| `operation_ns` (sum of 17 named children) | 32,566,067,669 ns | `phases-perf.json`, `timing.json` |
| root span (includes untimed corpus reads) | 44,832,509,917 ns | `timing.json` |
| harness work inside the timer | 12,265,442,248 ns | derived |
| preparation / cleanup / handoff | 145,507,209 / 167 / 34,319,846 ns | `phases-perf.json` |
| CPU user + system | 39,662,931,000 ns | `phases-perf.json` |
| peak RSS (lifetime) | 264,536,064 B | `phases-perf.json` |
| per-state min / median / max | 37.0 / 1,564.1 / 3,971.6 ms | `timing.json` |
| `delta.prefix_selected` / `full_records` / `no_candidate` | 38,163 / 12,952 / 6,988 | trace |
| `delta.trials` / `ineligible_candidates` / `work_exceeded` / `reused` | 38,230 / 683 / 21 / 1,121 | trace |
| gates | `g1.o1-chain-complete` PASS · `g4.swaps` PASS · `g1.o3-pinned-counters` INCOMPLETE | trace |

**Correction to §2.2.** The claim that `st_blocks x 512 == st_size` on core Stores is
true of the five `188d` lane Stores but **not** of this one: allocated exceeds apparent
by 139,264 B here. The gate margin for stride-10 is therefore **151,552 B**, not the
290,816 B an apparent-only reading gives. The equality must be measured per artifact,
never assumed — which is exactly what the missing `g1.o6-below-v016` gate exists to do.

**No counterpart artifacts exist.** `benchmark-results/repository-history/` retains
v0.1.6's three Stores (49,315,840 / 64,000,000 / 82,685,952 B) but there is **no
v0.1.7 stride-3 or stride-1 Store anywhere** (searched `/tmp`, `/private/tmp`,
`/Users/yifanxu/Ephemeral-AI-Lab` for `*.sqlite` over 50 MB): the claimed
`61,767,680` and `80,273,408` figures have neither a Store, a trace, nor a receipt.

## 5. The fix — `pooled-lane-cold` attributed, then re-baselined

### 5.1 Attribution: a matched pair that differs by one constant

Both binaries are built from the same tree; the only difference is
`encoding/codec.rs`'s `GROUP_LEVEL`. The case is `pooled-lane-cold`, one raw
invocation each, `LAYERFS_CONSTRUCTION_WORKERS=1`, fresh `--out`.

| binary | `GROUP_LEVEL` | `pool.commits` | Store apparent | groups | values |
| --- | ---: | ---: | ---: | ---: | ---: |
| `8da16a29…` | **19** (shipped) | **18** | 3,883,008 B | 512 | 51,200 |
| `88842db5…` | 1 (temporary) | **19** | 4,116,480 B | 512 | 51,200 |

The logical work is identical — the same 512 value groups, the same 51,200 values,
and the same 512 object identities. Only the encoded pack bytes differ, and the
commit count follows them. **The codec change is the cause; the movement is not a
regression in the operation.**

**The mechanism.** `write_pack` charges the transaction with
`write.bytes.len()`, and `SelectedWrite.bytes` is the **whole assembled pack**
(`pack/placement.rs:30-39`), not the appended delta, while
`sqlite/write.rs::append_pack` is `UPDATE object_packs SET data = ?2` — the pack row
is rewritten in full on every append. The counter therefore measures *physical
rewrite volume*, which is quadratic in the appends a pack receives, and it crosses
the `TRANSACTION_CANONICAL_BYTES_LIMIT` (4 MiB - 1) boundary fewer times when the
same content compresses better. One fewer boundary is one fewer `COMMIT`
(`cas/lifecycle.rs:129-145`).

**Flagged, not changed:** `policy.rs:314-316` names that field *"Canonical bytes per
open transaction"*. It is not canonical bytes; it is rewrite volume, and it counts
the same bytes once per append. Renaming it or charging the appended delta instead
would move the commit cadence of many rows and needs an owner ruling — it is not
part of this fix.

### 5.2 The re-baseline, through the pin tool

| run | tree | rows |
| --- | --- | --- |
| `run-20260920T-fix1` | HEAD, unchanged pins | **216 PASS / 1 FAIL / 3 NOT_RUN**, 150.2 s |
| `run-20260920T-fix2` | stale assertion removed | **217 PASS / 0 FAIL / 3 NOT_RUN**, 149.1 s |
| `run-20260920T-fix3` | regenerated pins | **217 PASS / 0 FAIL / 3 NOT_RUN**, 152.0 s |
| `run-20260920T-fix4` | final tree (pins + doctest fix) | **217 PASS / 0 FAIL / 3 NOT_RUN**, 152.2 s |

All four are retained under `benchmark-results/fs-bench-pro-storage-content/`
(gitignored). `run-20260920T-fix4` is the closure run: `verify` reports **0
disagreements** over 220 cases, call-graph PASS over 121 files, tripwires PASS over
141 stores, 3.95 s; phases preparation 28.238 s / operation 79.944 s / verification
33.722 s / handoff 1.361 s.

**Exactly one of the 1,969 pinned keys moved.** `run-20260920T-fix1` failed on
`pooled-lane-cold` alone, and a key-by-key comparison of the round-4c table against
the counters `run-20260920T-fix2` published found **one** differing value:
`pooled-lane-cold counter:pool.commits 19 -> 18`.

The value was **not hand-written**. The stale assertion was deleted (no value
written, no constant invented), the lane re-ran to `217 PASS`, and
`shared/pin_expected.py counters --run run-20260920T-fix2` re-derived it from that
passing run; `pin_expected.py merge` applied it to the table. The table's diff is
that one value plus a provenance header. The **50 counters the harness now publishes
that the table never pinned** (`verify.sampled`, `verify.units`) were deliberately
**not** added: widening the oracle is a separate decision, not part of a re-baseline.

### 5.3 Also fixed: the harness's own suite was red

`src/ops/history.rs:186` opened a doc-comment block with a bare ```` ```` ```` fence, so
rustdoc compiled the measured-loss table as Rust and `cargo test` failed with
`expected one of ... found 'OFF'`. Fenced as ````text````. `cargo +1.85.1 test
--manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` is
now green (8 + 2 + 9 + 4 + 12 tests, 0 failed) and `runner.py self-check` is PASS.

### 5.4 Open for the owner

1. **Is `pool.commits` a pin at all?** It is a batching *cadence*, not a structural
   count, and it moves whenever compression improves — which is the campaign's own
   goal. Keeping it means a re-baseline on every storage-format change; dropping it
   means the row's O3 oracle loses a key. This note does not decide it.
2. The transaction-byte accounting name/semantics mismatch in §5.1.
3. **The 217 lane is now green at `795fb1a2f` + this change**, but that is a
   harness-side re-baseline: the closure of record for Stage 6 is still round 4c /
   round 5b at their own commits, and a *new* closure receipt at this identity is
   what would supersede them.

### 5.5 Not claimed

- No timing claim. The wall and CPU figures are published, not gated.
- The three diagnostic `component.primitives` rows remain `NOT_RUN`, as in every
  round.
- Another workstream's files were dirty in the tree during every run above
  (`core/docs/architecture/proposal/*`). They are recorded in each receipt's
  `source_dirty_files`; they are not this change's and were not staged by it.
