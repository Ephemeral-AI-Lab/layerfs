# Handoff prompt — root-cause the retained-history measured-operation slowdown

> **Status:** Investigation prompt for [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190).
> Nothing in this document is a measurement. It states the issue, hands over what is
> already measured, and authorizes a subagent research campaign. It does **not**
> authorize a product change.

---

## 0. Your mission

The replacement C1/C2 core saves a real 157-checkpoint repository history into one
Store at a **smaller** size than v0.1.6 did, but on the one axis where a v0.1.6
number exists it is **~2.9× slower**: `history-stride10`'s measured operation is
**32.566 s** where v0.1.6 recorded an **11.371 s** Commit sum.

**Find out why, and attribute it to named mechanisms with measurements.**

You are *not* asked to make anything faster, and you may not change product source.
Three outcomes are equally acceptable and all three must be reported plainly:

1. the gap is **composition** — the two timers bracket different work, and the
   "slowdown" is work v0.1.6 never timed;
2. the gap is **real cost**, attributed to named mechanisms (tree update, codec,
   index, worker count, cache state …);
3. the gap is **not resolvable** with the available evidence, with the missing
   evidence named.

A confident wrong answer is the only failure mode. "Not measured" beats "inferred".

---

## 1. The issue, stated exactly

`history-stride10`, 17 states, `range(1,158,10) ∪ {157}`, same corpus on both sides.

| quantity | v0.1.6 (recorded) | ours | ratio |
| --- | --: | --: | --: |
| measured operation — their Commit sum / our Σ of the 17 named state children | **11.371 s** (median 564.3 ms) | **32.566 s** (median 1,564.1 ms) | **2.86×** (median 2.77×) |
| end-to-end wall | 98.0 s (exec 59.7 + per-state install 18.6 + commit 11.4 + overhead) | 45.0 s invocation | **0.46× — we are 2.2× faster** |
| Store allocated / apparent | 49,344,512 / 49,315,940 B | 49,192,960 / 49,053,696 B | we store less |

The lane's time axis is a **one-sided tripwire** (owner ruling 6,
[`retained-history-storage.md`](../retained-history-storage.md) §8):

> Passing the time comparison proves nothing. **Failing it proves a flaw.**

So the wall being better does not excuse the operation being worse, and the
operation being worse is not yet a defect — it is an unexplained 21.2 s.

---

## 2. What is already measured — your head start

### 2.1 The phase receipt of the retained run

| field | value |
| --- | --: |
| `operation_ns` (Σ of the 17 named state children) | 32,566,067,669 ns |
| root span (includes the harness's untimed corpus reads) | 44,832,509,917 ns |
| harness work inside the timer (root − Σ children) | 12,265,442,248 ns |
| preparation / cleanup / handoff | 145,507,209 / 167 / 34,319,846 ns |
| invocation wall | 44,999,165,042 ns |
| CPU user + system | 30,614,677,000 + 9,042,045,000 ns |
| peak RSS (lifetime) | 264,536,064 B |
| heap peak incremental | 75,411,887 B |

Source: `/tmp/history-stride10/` (raw: `timing.json`, `phases-perf.json`,
`trace.jsonl`, `sample.sqlite`), recorded in
[`../evidence/stage-6-recheck-20260920T000000Z/README.md`](../evidence/stage-6-recheck-20260920T000000Z/README.md) §4.
**That directory is volatile**: re-run and retain your own artifacts before you
publish anything from it.

### 2.2 The decomposition that exists — and its 94 % hole

The per-state child has only four named spans in the whole run:

| span | Σ | share of 32.566 s | occurrences |
| --- | --: | --: | --: |
| `content` (C1 construction) | 1,115.2 ms | 3.4 % | 17 |
| `storage.finish` | 843.3 ms | 2.6 % | 17 |
| `storage.begin` | 8.6 ms | 0.0 % | 17 |
| `store.create` | 2.5 ms | 0.0 % | 1 (state 1) |
| **unnamed inside the children** | **~30.6 s** | **94.0 %** | — |

**94 % of the measured operation is inside the per-state child body and outside
every named span.** Until that is split, no attribution is possible and no
hypothesis can be tested. This is the single highest-value piece of work in the
campaign — see Squad S1.

### 2.3 The per-state scaling — the strongest existing clue

| ordinal | ms | oracle entries | files | logical MB |
| --: | --: | --: | --: | --: |
| 1 | 37.0 | 359 | 276 | 1.50 |
| 11 | 194.8 | 1,143 | 875 | 6.67 |
| 21 | 277.6 | 1,701 | 1,306 | 9.64 |
| 31 | 438.8 | 2,148 | 1,656 | 11.88 |
| 41 | 904.0 | 3,437 | 2,762 | 18.79 |
| 51 | 942.3 | 4,208 | 3,462 | 21.99 |
| 61 | 1,333.5 | 5,383 | 4,539 | 26.00 |
| 71 | 1,351.4 | 5,678 | 4,795 | 27.71 |
| 81 | 1,564.1 | 6,369 | 5,395 | 33.74 |
| 91 | 1,694.8 | 6,750 | 5,739 | 36.25 |
| 101 | 2,919.3 | 7,435 | 6,355 | 40.56 |
| 111 | 2,931.4 | 7,992 | 6,828 | 44.39 |
| 121 | 3,243.9 | 8,297 | 7,132 | 46.74 |
| 131 | 3,971.6 | 9,137 | 7,880 | 50.91 |
| 141 | 3,906.7 | 10,123 | 8,715 | 57.79 |
| 151 | 3,284.9 | 10,393 | 8,934 | 61.43 |
| 157 | 3,569.9 | 10,924 | 9,415 | 65.02 |

Correlation of per-state time with **base-tree size**: 0.9586 (oracle entries),
0.9605 (logical bytes); aggregate marginal cost 320.9 µs per oracle entry.

**The per-state cost tracks the size of the base tree, not the size of the state's
change.** Each state changes a small delta, yet costs scale with everything already
saved. That is the shape of a walk over the base tree, and it is where the gap
most plausibly lives. It is a clue, not a finding.

### 2.4 The Store the run produced

52,032 objects / 380,921,328 B canonical · 255 packs / 45,035,732 B pack bodies ·
schema v6 · `quick_check ok` · 49,053,696 apparent / 49,192,960 allocated.

Save counters: `delta.prefix_selected` 38,163 · `delta.full_records` 12,952 ·
`delta.no_candidate` 6,988 · `delta.trials` 38,230 ·
`delta.ineligible_candidates` 683 · `delta.work_exceeded` 21 · `delta.reused` 1,121.

### 2.5 The tiers, and where our side is thin

| tier | states | v0.1.6 Commit sum | ours |
| --- | --: | --: | --- |
| stride10 | 17 | 11.371 s | 32.566 s operation (retained) |
| stride3 | 53 | 24.815 s | **wall only**, 97.9 s — no operation sum retained |
| stride1 | 157 | 64.108 s | **wall only**, 289.5 s — and **no receipt of any kind** |

Verification walls, for a second view of the same work: ours 7.5 / 20.3 / 64.2–90.3 s
against the ruled ceilings 10 / 20 / 30 s; v0.1.6's 67.5 / 199.8 / 570.6 s. A quiet
re-run of the stride1 verification is separately owed by that lane.

### 2.6 The v0.1.6 side

Recorded in [`issue153-retained-history-report.md`](../../0.1.6/evidence/issue153-retained-history-report.md)
§1 (ledger `L31`): source `8308cd8e…` @ `7fab1027a`, product `970964e9…`, image
`layerfs-bench-infra:8308cd8e628a97cd`; columns `Commit sum (median)`, `exec sum`,
`per-state install`, `perf wall`, `verify wall`. It ran **in containers**, with the
per-state fixture install and the exec phase **inside its work wall and outside its
Commit**. A paired v0.1.5 control exists at `276c5970…` @ `6ee1ec94c`.

---

## 3. Confounds you must resolve before attributing anything

1. **Composition.** Our timer includes C1 content construction and the tree update;
   their Commit's composition is not yet reconstructed from source. If most of the
   21.2 s is work v0.1.6 never timed, the "slowdown" is a comparison artefact — and
   saying so requires the reconstruction, not an argument.
2. **Worker count.** This lane runs `LAYERFS_CONSTRUCTION_WORKERS=1` by acceptance
   rule; the v0.1.6 campaign ran under the container runtime and the rule did not
   apply to it. A declared difference, but it must be **quantified** as a labelled
   diagnostic before it is used as an explanation.
3. **Cache state.** Our states read the corpus untimed between the children; their
   per-state install (18.6 s) sat inside their work wall. Both sides must be
   declared, and neither may credit a timed phase from warm pages.
4. **Machine state.** Our run was at load ~5 with a competing `cargo` process; the
   same configuration measured 64.2 s and 90.3 s on repeat in the verification
   phase (a 40 % spread). Their figures are cited, not re-run, and their machine
   state is not described. Per-case noise across five behaviourally identical runs
   I took was median 16.6 % / p90 38.9 % relative range, while the lane total moved
   only 75.4 → 79.2 s: **small rows are noise, multi-second rows are signal**.
5. **Selection and corpus.** Both sides used the same 17 indices over the same
   corpus pins (`03f21acf…`, tip `b0a7d2ce…`). Confirm it yourself from the harness
   pins rather than trusting this paragraph.

---

## 4. Hypotheses, ranked, each with the measurement that settles it

| # | hypothesis | decisive measurement | prior |
| --: | --- | --- | --- |
| H1 | **The tree update walks the base tree per state**, so per-state cost tracks the base size (r = 0.96) | Split the unnamed 94 % of the child body into named spans: `update_filesystem`/`build_filesystem`, the accept loop, and ordering | high |
| H2 | **Composition**: our child brackets work their Commit did not (content construction, tree update, accept loop) | Reconstruct v0.1.6's Commit composition from its source and reconcile the two boundaries item by item | high |
| H3 | **Codec cost**: `GROUP_LEVEL` 1 → 19 and the 16 MiB encode workspace raise encode CPU | Matched pair, one constant: `GROUP_LEVEL = 1` vs `19` on the same selection; report codec CPU separately from lane CPU | medium |
| H4 | **Worker count**: one construction worker versus the reference's runtime default | Same selection, the declared rule vs the reference's worker count, as a **labelled diagnostic**, never as a gate arm | medium |
| H5 | **Persisted similarity index / chunk cursor** add per-object work | The counters already exist (`delta.trials` 38,230, `delta.ineligible_candidates` 683); attribute them against the codec's own instrumented CPU | medium |
| H6 | **Save/transaction cadence**: the whole-pack rewrite accounting commits every ~28 objects on a large lane | Count `SaveOutcome.commits` and charged transaction bytes per state; compare with the boundary the accounting declares | low–medium |

Falsify them. A hypothesis that fails is a result; record it as falsified with the
number that killed it.

---

## 5. Subagent structure — explore and research in parallel

Run this as a **squad campaign**, as [#187](../evidence/stage-6-history-187-20260920T000000Z/)
and `188b` did. Each squad owns one document, one scope, and no other squad's
files. Squads S1–S5 run in parallel; S1 is the critical path.

| squad | scope | must produce | must not do |
| --- | --- | --- | --- |
| **S1 — span attribution** | split the unnamed 94 % of the per-state child body into named spans (tree update, accept loop, ordering, read-back) and publish a per-state, per-span table with the residual | `squad-s1/SPAN-ATTRIBUTION.md`: per-state decomposition, Σ per span, residual stated, plus the harness change (if any) as a diff under `core/benchmark/` only | touch product source; publish a share without naming the span |
| **S2 — tree-update path** | source-read the child body: which code walks the base tree, how often, and with what bounds; link every claim to file:line | `squad-s2/TREE-UPDATE-PATH.md`: call graph, the walk's complexity and bounds, and the counters that would expose it | guess from names; cite a function without a line |
| **S3 — C2 cost** | codec levels, persisted index, transaction cadence, pack rewrite accounting — as matched-pair diagnostics | `squad-s3/C2-COST.md`: each lever with its measured delta and its cost (CPU, RSS, bytes) | change a default; run stride1 |
| **S4 — the v0.1.6 side** | reconstruct exactly what v0.1.6's Commit included and excluded, from its source and its receipt; the exec/install/Commit boundary, worker count, cache state | `squad-s4/V016-COMMIT-COMPOSITION.md`: an item-by-item boundary comparison, with source references | re-run v0.1.6; quote its numbers as ours |
| **S5 — protocol** | the matched-pair protocol everyone else follows: quiet-machine check, lock, cache declaration, worker rule, one sample per case per arm, ceilings; and a list of comparisons that cannot be matched | `squad-s5/PROTOCOL.md`: the protocol, the checklists, and the unmatched-comparison register | relax a rule to make a comparison possible |
| **You — synthesis** | reconcile S1–S5 into one attribution table with **residual 0**, or state what remains unmeasured | `SYNTHESIS.md` + the ledger entry | average, smooth, or "roughly" |
| **Adversarial reviewer** | read-only agent: re-derive your headline numbers from the raw artifacts and try to falsify the attribution | `REVIEW.md`: what reproduced, what did not, what is unsupported | fix anything; it reports only |

Squad rules: each writes only its own document; disagreements between squads are
published, never merged away; the synthesis names which squad's number it used.

---

## 6. Rules of engagement

- **No product change.** The investigation may edit `core/benchmark/**` (harness,
  not product source) and may write documents. A product fix needs an owner ruling
  and is out of scope here.
- **No third-party patching, no vendoring, no `[patch]`.** Builds stay `--locked`.
- **One sample per case per arm. No n3, no best-of, no re-run for a better number.**
  Diagnostics are allowed and must be labelled and reported beside the gate sample.
- **`history-stride1` is never optimized**: no constant, threshold, buffer, batch or
  policy value may be changed because of a stride-1 number. Iterate on stride10,
  confirm on stride3.
- **The measurement lock.** Never overlap resource-sensitive work; never interrupt
  another owner's run. Check the machine is quiet and say whether it was.
- **Receipts are append-only.** Fresh `--out` per run; a run never overwrites
  evidence; failures, `INELIGIBLE` rows and discarded attempts stay on disk. The
  raw binary refuses an existing directory, and `phases-verify.json` is not
  overwritten — a "completed" re-run that verified nothing looks plausible.
- **No timing claim is a gate here.** The storage gate is the gate; time is the
  tripwire. Nothing you measure becomes a release claim.
- **Record every run** in the ledger (exact numbers, limits, arithmetic, identities,
  reproduction command, and every non-passing line).
- **Production LOC:** if you commit, report `Production LOC: before -> after
  (delta …)` with the scope and method. Harness and documentation changes report
  the unchanged product total and delta 0.
- **No CI, no `tools/preflight.sh`** — it is permanently retired. Verify the
  workspace you changed with explicit commands.

---

## 7. Deliverables and acceptance

1. **A per-state, per-span decomposition** of the stride-10 operation with the
   residual stated — the 94 % hole closed.
2. **An attribution table** with one row per named mechanism: measured effect,
   evidence, and the comparison it was measured against.
3. **The v0.1.6 boundary reconstruction**, so the reader can see which parts of the
   21.2 s are like-for-like.
4. **Every falsified hypothesis**, with the number that falsified it.
5. **A disposition recommendation** for the owner: fix (with the ruling it needs),
   accept-and-record, or re-rule the tripwire — with the reason.
6. **An independent read-only review** that reproduces or contradicts the headline
   numbers from the raw artifacts.

Acceptance is the standard this repository holds everywhere else: the arithmetic
closes, the identities are pinned, the residual is zero or the gap is named, and
every non-passing line is reported as plainly as every passing one.

---

## 8. Where everything lives

| what | where |
| --- | --- |
| the harness | `core/benchmark/fs-bench-pro-storage-content/` — driver `src/ops/history.rs`, corpus reader `src/workload/history.rs`, pins/selections/ceilings `shared/history_corpus.py`, runner `runner.py` |
| the lane's specification and rulings | `docs/roadmap/0.1/0.1.7/retained-history-storage.md` §5, §8, §11.1 |
| the case documents | `core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/` |
| our retained numbers | `docs/roadmap/0.1/0.1.7/evidence/stage-6-recheck-20260920T000000Z/README.md` §4 |
| the tier and verification evidence | `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188d-20260920T000000Z/` |
| the v0.1.6 record | `docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md` |
| the corpus | `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, manifest `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`, tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed` |
| write your evidence | `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-190-<stamp>/` |

Environment switches the driver reads, which you must record as set or unset:
`LAYERFS_HISTORY_ADVISORY`, `LAYERFS_HISTORY_ORDERED_PREDECESSORS`,
`LAYERFS_HISTORY_DECLARED_ONLY`, `LAYERFS_HISTORY_SIMILARITY_CANDIDATES`,
`LAYERFS_HISTORY_FALLBACK_CANDIDATES`, `LAYERFS_HISTORY_FULL_PRODUCER`,
`LAYERFS_HISTORY_CHUNK_PREDECESSORS`, `LAYERFS_HISTORY_DEPTH_LIMIT`.

---

## 9. Commands to start from

```sh
H=core/benchmark/fs-bench-pro-storage-content
C=/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked

# the measured phase, raw driver, fresh output every time (never reuse a path)
LAYERFS_CONSTRUCTION_WORKERS=1 $H/target/release/fs-bench-storage-content \
    --case history-stride10 --corpus $C --out /tmp/h190-$(date +%s)

# the runner path exists but is NOT yet usable as an admission row:
#   * the per-lane complete-command ceilings are not wired (15 s / 25 s apply),
#   * no history.* row is pinned in tests/golden/expected.tsv, so
#     g1.o3-pinned-counters is INCOMPLETE by construction,
#   * the ruled verification ceilings (10/20/30 s) are not wired either.
# Use the raw driver for instrumentation; do not report its output as an admission row.
```

---

## 10. If you cannot find the cause

Say so, and name the missing evidence. "The 94 % was split, and the split shows X
but X could not be compared to v0.1.6 because Y was never recorded" is a complete
answer. Marking a hypothesis `NOT_RUN` with the reason is required behaviour, not
a failure — inventing a mechanism to close the issue is the failure.
