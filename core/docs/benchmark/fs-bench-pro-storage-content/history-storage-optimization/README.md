# Retained history storage — deepseek-harness snapshots

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Companion to [issue #186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186), a
> sub-issue of [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184). Measured by
> the harness in
> [`core/benchmark/fs-bench-pro-storage-content/`](../../../../benchmark/fs-bench-pro-storage-content/).

The question: **save a real repository history into one Store, and measure what it costs to
retain.** Three rows, one per history selection. Each row creates one Store, saves every
state of its selection into it in order, and is then verified.

```text
claim_kind = history-storage-efficiency
```

## 1. The three rows

| row | selection | states | cumulative logical bytes |
| --- | --- | --: | --: |
| `history-stride10` | `range(1,158,10) ∪ {157}` | 17 | 561,010,345 |
| `history-stride3` | `range(1,158,3)` | 53 | 1,676,767,835 |
| `history-stride1` | all checkpoints | 157 | 4,936,693,030 |

Registry group `history.*`, three admission rows, `tier` fixed, `entries` = the state count,
`bytes` = the cumulative logical bytes above. Each row is also its own lane.

The selections are **independent workloads, not samples of one another**: stride-3 and
stride-10 take direct transitions between selected states and never replay a skipped state,
so their per-transition deltas are larger than stride-1's.

### 1.1 Owner direction: the optimization tiers

| tier | states | role | optimization | bug fix | how it runs |
| --- | --: | --- | --- | --- | --- |
| `history-stride10` | 17 | **smoke** | **P0** — the first place anything is tried | **P0** | the default iteration tier |
| `history-stride3` | 53 | **intermediate** | **P0** — must also pass | **P0** | the second gate |
| `history-stride1` | 157 | **run only** | **never optimized** | run to confirm | explicit, never a default |

**Iteration runs one way:**

```text
iterate on stride10          P0, cheap reject
  → confirm on stride3       P0, matched pairs
    → run stride1 once       confirmation only, never tuned
```

**"Do not optimize stride1" forbids, concretely:** changing any constant, threshold, buffer,
batch size or policy value *because of* a stride1 number; running stride1 more than once per
candidate that has already passed the two lower tiers; and treating a stride1 result as a
target to iterate on. A stride1 failure while both lower tiers pass is a **scaling finding**,
investigated at stride10 and stride3 and reported — never patched at stride1. No n3, no re-run
for a better number, no best-of at stride1.

The tier-spanning falsifiers of [`measurement.md`](measurement.md) §7 are what catch such a
finding early: per-state work time and peak heap must stay flat from 17 to 53 to 157 states,
and under this policy a rise shows up at stride3, where it is P0.

## 2. What this claim may and may not support

**May support:** the replacement C1/C2 core saves a real 157-checkpoint repository history
into one Store, the Store deduplicates it automatically, and every state verifies against
its original oracle.

**May not support:**

- **Any Commit, LayerStack, Branch, FUSE, daemon, container or cgroup claim.** Those are
  Stage 7 ([#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172)). This lane has no
  runtime envelope at all — that is the point of it.
- **Any v0.1.6 pairing as a gate.** `../CONTRACT.md` decision D1 forbids pairing a core
  family with a v0.1.6 family for a claim. §5 states the one comparison that is legitimate
  and why.
- **Any claim that a time improvement demonstrates algorithmic quality.** See §5.

**Does support, and this is the strongest check available:** the canonical totals in §4 are
recorded from v0.1.6 on byte-identical source trees, and v0.1.7 policy preserves canonical
bytes, identities **and the Store format**. Reproducing them proves the migration is
faithful; not reproducing them is a finding, not a fixture tweak.

## 3. Frozen identities

| | value |
| --- | --- |
| Source | `https://github.com/deepseek-ai/deepseek-harness.git` |
| Pinned tip | `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed` |
| Checkpoint manifest SHA256 | `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` |
| Checkpoints | 157 (of 15,632 reachable commits) |
| Total logical bytes | 4,936,693,030 |
| Unique blob bytes | 891,893,320 |
| Largest blob | 1,241,221 B |
| Modes | `100644` 762,840 · `120000` 1,223 · `100755` 991 |

## 4. Frozen pins — gates

| row | verified path-states | verified logical bytes | canonical content | canonical objects |
| --- | --: | --: | --: | --: |
| `history-stride10` | 101,477 | 561,010,345 | *not recorded* | *not recorded* |
| `history-stride3` | 306,861 | 1,676,767,835 | 589,423,458 B | 73,476 |
| `history-stride1` | 904,143 | 4,936,693,030 | 871,588,115 B | 104,705 |

Recorded from the v0.1.6 campaign
(`docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md`, ledger `L31`).
`history-stride10` has no recorded canonical total, so its canonical numbers become
first-run pins; the asymmetry is stated rather than papered over.

## 5. The one legitimate comparison, and what it can prove

v0.1.6's recorded Store allocated / apparent bytes — 49,344,512 / 49,315,940 at 17 states,
64,024,576 / 64,000,100 at 53, 83,947,520 / 82,677,860 at 157 — and its Commit sums
11.371 / 24.815 / 64.108 s.

**Storage is a real falsifier.** The canonical content is pinned byte-identical across the
two generations and the Store format is preserved, so the same content in the same format
should produce a comparable file, differing only by the commit and branch metadata the core
Store does not carry. The core figure should land **below** v0.1.6's, and landing above it
is an algorithmic finding.

**Time is a one-sided tripwire, not a gate.** This lane has no container, no FUSE mount, no
spool and no Commit envelope, so a speed-up is guaranteed by the surface change and proves
nothing about the algorithm:

> **Passing the time comparison proves nothing. Failing it proves a flaw.**

Matched Git comparators, cited rather than re-run: Git53 = 49,332,224 B, Git157 =
56,373,248 B. v0.1.6 measured 1.298× and 1.489× against them.

## 6. The documents

| document | what it fixes |
| --- | --- |
| [`preparation.md`](preparation.md) | what preparation is — corpus authentication, and nothing else |
| [`verification.md`](verification.md) | the final gate over every state, the oracles, the sample |
| [`measurement.md`](measurement.md) | the row shape, the four phases, the instruments, the falsifiers |
| [`implementation-plan.md`](implementation-plan.md) | the file list and the rollout |

`docs/general/benchmark_rules.md` governs everything here; where it and these documents
disagree, it wins. These documents never amend [`../CONTRACT.md`](../CONTRACT.md) or its 217
admission rows.

## 7. Owner decisions — ruled 2026-09-19

All seven were ruled by the owner before the first product run of this family, as
`docs/general/benchmark_rules.md` §1 requires. Until they were, the affected gates were
proposals and no row measured under them was admission evidence. The rulings are recorded
verbatim in
[`docs/roadmap/0.1/0.1.7/retained-history-storage.md`](../../../../../docs/roadmap/0.1/0.1.7/retained-history-storage.md)
§9, which is the roadmap-level specification this document set is subordinate to.

| # | decision | ruling |
| --: | --- | --- |
| 1 | The per-lane and verification budgets, now that the family's budget is lifted | **Ruled as a procedure.** Both are declared at Phase 3 from the stride-10 baseline, recorded with their source, and frozen before any optimization work; the numbers cannot exist before that baseline. They are carried as **per-lane declared ceilings**, not as `DECLARED_EXCEPTIONS` entries — the exception list is capped at 25 s and cannot carry a 157-state run. A ceiling is never inflated after a valid miss. |
| 2 | Is `operation_ns` the sum of the N named per-state children? | **Yes** — a root would include untimed corpus reading. `shared/phases.py`'s "two sources, one number" check becomes **lane-scoped** for this shape: `root >= Σ children`, with the difference published rather than absorbed. |
| 3 | May a sampled row in this lane be `PASS`? | **Yes, this lane only, frozen before collection** — the O(1) counters that decide storage are read in full in every mode. The 217 keeps `full` and keeps forcing `sample` to `INCOMPLETE`. |
| 4 | Sampled unit, and the selection rule | **The state's file manifest**, in corpus order; `max(1, ceil(n/10))` units by `index % 10 == 0`, **plus the first and last entry of every state**. The rule is **lane-scoped**: `sampled_indices` is used by the 217's `c2.delta.*` rows and no 217 `verification_selection` string may move. |
| 5 | Are the §4 canonical totals gates or diagnostics? | **Gates** — identity, not performance. A mismatch is a `FAIL` and stops Phases 4 and 5. stride-10's are first-run pins, thereafter reproduced. |
| 6 | Is the v0.1.6 timing comparison accepted as a labelled one-sided tripwire? | **Yes**, stated as in §5: published, labelled, never gate-deciding. |
| 7 | Is the storage comparison against v0.1.6 accepted as a **gate**? | **Yes, and at all three sizes**: allocated below 49,344,512 B at 17 states, 64,024,576 B at 53 and 83,947,520 B at 157. Format preserved and content pinned, so this is an identity-anchored comparison, not a cross-generation performance pairing. |

### 7.1 Ruled before these, and not re-opened

**Family rulings (owner, 2026-09-19).** The family's budget is lifted and the lane ceilings are
declared instead; the tiers are stride10 (smoke, P0), stride3 (intermediate, P0) and stride1
(run only, **never optimized**); and this lane builds no prepared storage
(`Preparation::InProcess`, nothing under `prepared/`).

### 7.2 The earlier proposal table, for the record

Verbatim from the committed proposal, so the ruling can be compared against what was
proposed rather than restated after the fact. This table is the historical record and is
not edited; the rulings in §7 supersede it.

| # | decision | recommendation |
| --: | --- | --- |
| 1 | The per-lane and verification budgets, now that the family's budget is lifted | fix both **before** collection, from the stride-10 measurement; the lift is a declared ceiling, not an absence of one |
| 2 | Is `operation_ns` the sum of the N named per-state children? | yes — a root would include untimed corpus reading |
| 3 | May a sampled row in this lane be `PASS`? The 217 keeps `full` and forces `sample` to `INCOMPLETE` | yes for this lane only, frozen before collection, because the storage counters are never sampled |
| 4 | Sampled unit, and the selection rule | the state's file manifest; endpoints **and** every tenth |
| 5 | Are the §4 canonical totals gates or diagnostics? | gates — identity, not performance |
| 6 | Is the v0.1.6 timing comparison accepted as a labelled one-sided tripwire? | yes, stated as in §5 |
| 7 | Is the storage comparison against v0.1.6 accepted as a **gate** (allocated below its recorded bytes), given D1 forbids pairing? | yes — the Store format is preserved and the content is pinned identical, so this is an identity-anchored comparison rather than a cross-generation performance pairing |

## 8. What is not yet true

- **No row of this lane has been measured.** Every figure in §4 and §5 is quoted from the
  v0.1.6 record; none is a measurement of the replacement product.
- The harness has no `history.*` group, no corpus reader, no driver and no golden rows.
  `implementation-plan.md` is the work order.
- **CPU and process RSS are published**, since `2f8ebc90d`: `cpu.user_ns` and `cpu.system_ns`
  for the measured region, and `rss.process_peak_bytes` for the child. The RSS figure is a
  *lifetime* number and is named a process peak for that reason; the counting allocator's
  `heap.peak_incremental_bytes` stays the precise phase figure beside it. The 10 ms
  `RssSampler` is deliberately unwired — it cannot cover a phase under ~200 ms and a sampling
  thread inside the measured region perturbs what it measures.
- **The complete-command budget classifies a formula, not the raw wall**, since `2f8ebc90d`:
  `../CONTRACT.md` §4 fixes `declared_ns + LIFECYCLE_ALLOWANCE_NS` and §11 records it as
  erratum **E4**. The wall is still published as `complete_command_ns`.
- **The full Store reading is still published for the six `c2.footprint` rows only.** Since
  `2f8ebc90d` every row carries `resources['artifact.data_bytes']` — the bytes its prepared
  master occupies, or zero when it declares none — but the allocated, apparent, pack-body and
  attribution readings are taken by the runner's `verify` from a retained file, for the rows
  that gate them. This lane's whole claim is storage, so its reading must be taken inside the
  invocation, before and after the chain. `measurement.md` §4 is the fix.

## 9. Errata

Recorded rather than applied in place, per `../CONTRACT.md` §11's convention, because the five
documents were committed before the corpus was read. Each correction below was verified against
the corpus or the harness at `edb80addd` on 2026-09-19. The corrected text is in the document
named in the last column; this table is the record of what changed and why.

| # | The specification said | Verified against the corpus / harness | Corrected in |
| --- | --- | --- | --- |
| **E1** | `path-states` is not defined; `implementation-plan.md` §2.1 has `Corpus::pins()` return "path-states and cumulative logical bytes, for the pins", and `State.paths` is the only nearby counter | **The obvious readings are both wrong.** Summing `checkpoints[].files` gives 86,064 / 259,771 / 765,054 against pins of 101,477 / 306,861 / 904,143 — short by 17.9 / 18.1 / 18.2 %. The pins reproduce **exactly** as the entry count of `oracles/<sha>.json`, which counts files **and** directories (state 1: 276 files, 359 oracle entries, 83 directories). Cumulative logical bytes already match exactly under either reading. | §4 and §7 above; `implementation-plan.md` §2.1 |
| **E2** | `implementation-plan.md` §2.1: `manifest.tsv` and `previous.tsv` parse as `hex path \t mode \t oid \t size` | The actual column order is `mode \t oid \t size \t hex path` (`100644\t6e28c773…\t3481\t2e6167…`). A parser written to the stated order reads the mode as a path and refuses every state. | `implementation-plan.md` §2.1 |
| **E3** | `implementation-plan.md` §3 Phase 1 says "no registry change" but lists `history_declarations` passing among its exit criteria | `tests/history_declarations.rs` asserts the three rows exist, their ids, their `bytes`/`entries`, their `Preparation`/`StoreState` declarations and their lane membership. None of that is testable without the registry rows, which §3 Phase 3 owns ("the registry rows, the driver, the gates, the golden table"). | `implementation-plan.md` §3 Phase 1 and §2.4 |
| **E4** | `implementation-plan.md` §2.1 has `Corpus::open` read `checkpoint-manifest.json` and `Corpus::oracle` read `oracles/<sha>.json`, and §2.8's modified-files table adds no dependency | **The harness has no JSON parser, and none may be added.** `shared/test_lock_parity.py` fails a *harness-only registry package*, so `serde_json` — which is absent from `core/Cargo.lock` — cannot be linked. The harness already hand-rolls SHA-256 for this reason (`workload/digest.rs`). | `implementation-plan.md` §2.1 |
| **E5** | `implementation-plan.md` §2.8's modified-files table lists `shared/space.py` and `shared/analyze.py` | Owner ruling 2 (`operation_ns` = Σ children, not the root) also requires `shared/phases.py`: it currently **fails closed** unless `operation_ns == timing.json root.elapsed_ns`, and `ops::measure()` writes `timing.json` once per call with last-write-wins, publishing that call's root. | `implementation-plan.md` §2.8 |
| **E6** | `measurement.md` §2: "`verify` re-derives all six from the raw artifacts" | True of the runner's own re-derivation. The **child's** `Timing` API is `Timing::record(name, …)` with `scope.child(name)`, so the chain needs one root with N named children, and the row's `operation_ns` is the sum of those children rather than that root. | `measurement.md` §2.1 |

**E1 is the one that could have changed a verdict.** Had `path-states` been implemented as the
manifest line count, all three Phase 1 pin checks would have failed against numbers that are
correct, and the natural next move — adjusting the pins — is exactly the fixture tweak §4 and
`implementation-plan.md` §4 forbid.
