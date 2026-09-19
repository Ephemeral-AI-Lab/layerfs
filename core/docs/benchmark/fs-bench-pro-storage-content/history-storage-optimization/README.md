# Retained history storage — deepseek-harness snapshots

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Companion to [issue #184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184)
> and its history-storage sub-issue. Consumed by Stage 6
> ([#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)) as a **second
> claim**, measured by the harness in
> [`core/benchmark/fs-bench-pro-storage-content/`](../../../../benchmark/fs-bench-pro-storage-content/).

The question this campaign asks: **how much storage does the replacement C1/C2 core
retain for a real repository history, and what does it cost in preparation, operation,
memory and verification to retain it?**

```text
claim_kind = history-storage-efficiency
```

## 1. What this claim may and may not support

**May support:** the replacement C1/C2 core, at the frozen profile, retains a real
157-checkpoint repository history within declared storage, memory and per-row
complete-command bounds, with every state's canonical content pinned to an
independently recorded total.

**May not support:**

- **Any v0.1.6 pairing.** `../CONTRACT.md` decision D1 forbids pairing a core family
  with a v0.1.6 family: the surfaces differ (v0.1.6 commits through LayerStack and a
  container; these rows construct and save through public C1/C2 APIs only). v0.1.6
  storage figures appear in this campaign only as **labelled reference points**, never
  as a ratio a row claims to have beaten.
- **Any Commit, LayerStack, Branch, FUSE, daemon, container or cgroup claim.** Those
  are Stage 7 ([#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172)).
- **Any claim that these rows reproduce v0.1.6's wall times.** They cannot; the
  operation surfaces are different sizes of work.

**Does support, and this is the strongest available fixture check:** the canonical
content totals in §3 are recorded from v0.1.6 on byte-identical source trees, and
v0.1.7 policy preserves canonical bytes and identities. Reproducing them proves the
migration is faithful. Failing to reproduce them is a **finding**, not a fixture tweak.

## 2. Frozen identities

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

Lane names are `history-stride10`, `history-stride3`, `history-stride1`. Row IDs are
`history-stride<N>-state-<ordinal>`, where `ordinal` is the campaign index within the
selection and `tier_label` is the original `full157_index`, so the three profiles are
comparable at shared indices.

| lane | selection | states | rows |
| --- | --- | --: | --: |
| `history-stride10` | `range(1,158,10) ∪ {157}` | 17 | 17 |
| `history-stride3` | `range(1,158,3)` | 53 | 53 |
| `history-stride1` | all checkpoints | 157 | 157 |
| | | | **227** |

The three selections are independent workloads, not samples of one another: stride-3
and stride-10 take **direct transitions between selected states** and never replay a
skipped state, so their per-transition deltas are larger than stride-1's.

## 3. Frozen pins

Recorded from the v0.1.6 campaign (`docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md`,
ledger `L31`). These are **gates** for the corresponding lane.

| lane | verified path-states | verified logical bytes | canonical content | canonical objects |
| --- | --: | --: | --: | --: |
| `history-stride10` | 101,477 | 561,010,345 | *not recorded* | *not recorded* |
| `history-stride3` | 306,861 | 1,676,767,835 | 589,423,458 B | 73,476 |
| `history-stride1` | 904,143 | 4,936,693,030 | 871,588,115 B | 104,705 |

The path-state and logical-byte pins are properties of the **corpus** and are checked
against it directly. The canonical pins are properties of the **product's encoding**
and are checked against the Store.

`history-stride10` has no recorded canonical total, so its exit gate is the
corpus-derived pins. That asymmetry is stated rather than papered over.

## 4. Reference points — not gates, not comparisons

| | v0.1.6 Store allocated / apparent B |
| --- | --- |
| `history-stride10` | 49,344,512 / 49,315,940 |
| `history-stride3` | 64,024,576 / 64,000,100 |
| `history-stride1` | 83,947,520 / 82,677,860 |

Matched Git comparators, recorded and **cited rather than re-run**: Git53 =
49,332,224 B allocated, Git157 = 56,373,248 B. They depend only on the selected source
trees and the frozen Git policy, which is why citing them is legitimate and why this
campaign never runs a Git arm.

The core Store carries **no commit, branch or LayerStack metadata**, so the core
allocated figure is expected to land *below* the v0.1.6 reference and the gap is
attributable. A number that lands above it is a finding.

## 5. The documents

| document | what it fixes |
| --- | --- |
| [`preparation.md`](preparation.md) | how the test is prepared: corpus, chain, rungs, de-warm, cache states, and how preparation's own time, memory and disk are measured |
| [`verification.md`](verification.md) | how a row is verified: the oracles, the deterministic 10 % sample, the pinned constants, and the storage attribution |
| [`measurement.md`](measurement.md) | how a row is measured: the four phases, the six published numbers, the instruments, the report shape, lanes and budgets |
| [`implementation-plan.md`](implementation-plan.md) | the file list and the stride-10 → stride-3 → stride-1 rollout with stage gates |

`docs/general/benchmark_rules.md` governs everything here; where it and these documents
disagree, it wins. These documents never amend
[`../CONTRACT.md`](../CONTRACT.md) or its 217 admission rows.

## 6. Owner decisions still open

Each carries this campaign's recommendation. Until ruled, the affected gate is
**proposed**, and no row measured under it is admission evidence.

| # | decision | recommendation |
| --: | --- | --- |
| 1 | May a sampled row in these lanes be `PASS`? The 217 keeps `full` as its default and forces `sample` to `INCOMPLETE`. | Yes for this campaign only, frozen here **before** collection, because the O(1) storage counters are never sampled. |
| 2 | What is the sampled unit? | The state's file manifest — the thing a tree oracle compares. |
| 3 | Run both rung classes? | Yes: R1 for iteration, R2 for the allocation-gated headline, never pooled. |
| 4 | Cite the Git comparators as recorded constants? | Yes, with provenance, never re-run. |
| 5 | Are the §3 canonical totals gates or diagnostics? | Gates. They are identity, not performance. |
| 6 | Sample selection: endpoints included, or `index % 10 == 0`? | Endpoints **and** every tenth, because boundary defects live at the ends. The current code does not include the last index; this campaign must not inherit that silently. |

## 7. What is not yet true

- No row of this campaign has been measured. Every figure in §3 and §4 is quoted from
  the v0.1.6 record, and no figure here is a measurement of the replacement product.
- The harness has no history driver, no history registry rows, no corpus reader and no
  history golden table. `implementation-plan.md` is the work order.
- Preparation's own **memory** is not published by the harness today: the counting
  allocator and the RSS sampler are wired to the measured phase only.
  `preparation.md` §7 is the fix.
- **CPU time is instrumented but never published.** `cpu_now()` and `CpuReading` exist in
  `src/support/instruments.rs` and no driver calls them. `measurement.md` §3 is the fix.
- `cleanup_wall_ns` does not exist today, because nothing is copied per case yet. This
  is the first family where it becomes real work.
- The seal records only `harness_sha256`. The corpus identity is not part of the key yet,
  so a changed corpus would reuse a stale base. `preparation.md` §4.1 is the fix.
