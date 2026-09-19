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

## 7. Owner decisions still open

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
