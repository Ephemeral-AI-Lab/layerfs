# Measuring a retained-history row

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Governed by [`docs/general/benchmark_rules.md`](../../../../../docs/general/benchmark_rules.md) §5,
> §6, §9, §10 and §11, and by
> [`../memory_cpu_space_support.md`](../memory_cpu_space_support.md).

## 1. The row shape

One row per selection. One invocation. One Store, created inside the sample and grown in
place:

```text
create the Store
for each state k in the selection, in order:
    read state k's changed bytes from the corpus      untimed
      construct the changed content                   TIMED
      build_filesystem against the previous root      TIMED
      save                                            TIMED
close
```

No base is prepared, no copy is taken, nothing is checkpointed. `store_state` is
`CreatedInSample` and `cache_state` is `CreatedInSample`, matching `c2.save.*`'s declaration.
`Store::create` is charged to state 1's child and published as its own counter, so the fixed
cost is visible and subtractable.

## 2. The four phases, and the six published numbers

| | phase | what runs | published as | budgeted |
| --- | --- | --- | --- | --- |
| a | preparation | authenticate the corpus, resolve the selection, preflight disk | `preparation_wall_ns`; `acquisition_wall_ns` is **zero** | no — published only |
| b | **work** | the N per-state children above | `operation_ns` — the **sum of the named children** | **yes — the performance claim** |
| c | verification | the final gate of [`verification.md`](verification.md) §1, in its own invocation | `verification_wall_ns` | its own declared budget |
| d | cleanup | destroy the Store, close | `cleanup_wall_ns` | inside the complete-command wall |
| | | process wall | `complete_command_ns` | the declared lane ceiling |

`handoff_ns` is published **beside** `operation_ns`, not inside it, so harness work that falls
within a timed region is visible rather than absorbed.

`verify` re-derives all six from the raw artifacts and **fails closed**: if
`preparation + operation + verification + cleanup` does not reconcile with the invocation, and
the invocation with the process wall, inside the declared tolerance, the row is `INCOMPLETE`
rather than quietly wrong. The tolerance is the harness's existing one — 250 ms plus 2 % of
the wall.

### 2.1 Why `operation_ns` is the sum of the children

The product's timing tree carries **one named child per state**, and the row's `operation_ns`
is their sum. It is not the root, because the root would include the harness's own corpus
reading between the children — untimed work that must not appear as product time.

This forces the operation-window ruling that [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184)
§7.2 left open for a multi-operation row. It is owner decision 2 in the
[README](README.md#7-owner-decisions--ruled-2026-09-19).

### 2.2 What is inside a child

```text
construct the changed content      C1: the state's changed files
build_filesystem                   C1: the tree update against the previous root
save                               C2: begin_save, accept, finish
```

`InodeUpdate.value` is `InodeValue { kind, namespace_ref_count, content_root, metadata_root }`
— it references content **by id** and carries no bytes. So the content objects are not an
input; they are what the measured child produces. Pre-supplying constructed objects would move
measured work into preparation, which is why this lane has no prepared artifact at all.

## 3. Memory, CPU and disk — what is tracked, and the two gaps

| axis | instrument | status |
| --- | --- | --- |
| heap | counting `GlobalAlloc` → `heap.peak_incremental_bytes`, `heap_charged_bytes`, `heap_allocations` | exists — the **precise phase** figure |
| CPU | `getrusage(RUSAGE_SELF)` read at both phase boundaries → `cpu_user_ns`, `cpu_system_ns` in `phases-<invocation>.json`, composed into the receipt as `cpu.user_ns` / `cpu.system_ns` | **published**, since `2f8ebc90d` |
| process RSS | the child's lifetime peak → `process_peak_rss_bytes`, composed as `rss.process_peak_bytes` | **published**, since `2f8ebc90d` |
| sampled RSS | 10 ms `RssSampler` → `phase_peak_bytes`, `incremental_peak_bytes` | implemented and self-checked, **deliberately unwired** |
| swaps | `swaps()` plus `gates::swap_gate` in every C1/C2 driver | exists |
| disk | `st_blocks × 512`, `st_size`, `page_count`, `freelist_count`, `pack_bodies`, `object_rows`, `catalogue`, `schema_shape`, `quick_check`, `sidecars` | exists |
| disk I/O | `disk_read_bytes` against `requested` | exists |

**Why the sampler stays unwired.** Its 10 ms interval cannot cover a phase under ~200 ms, which
is most of the lane, and a sampling thread inside the measured region perturbs the thing it
measures. The harness README's earlier claim that an un-sampled row is `INELIGIBLE` was
corrected to say so rather than left standing.

**The RSS reading is a lifetime value**, named a *process* peak for exactly that reason —
`AGENTS.md` §5 forbids quoting a lifetime counter as a phase reading. The counted allocator's
`heap.peak_incremental_bytes` is the phase number beside it, and a claim about a phase's memory
uses that one.

**What is still missing for this lane** is the storage reading of §4, not the resource axes.

**CPU is a diagnostic, like `operation_ns`.** `getrusage(RUSAGE_SELF)` is process-wide and
cumulative, so a phase's CPU is a difference of two readings, and it is only meaningful
because `AGENTS.md` §3.8 mandates a single construction worker. It never gate-decides;
counters, heap and disk do.

## 4. The storage readings

This lane's claim is storage, so the reading is part of the measurement and not an
afterthought. It is taken **before and after the chain**, outside every timer, and published
per row:

```text
before:  the Store file as created            (allocated, apparent, page_count, freelist)
after:   the same axes over the retained history
         plus canonical bytes and objects by object_role, pack bodies, non-pack bytes
```

A read-only SQLite open still faults pages in, so ordering matters — and here it is
unconstrained, because nothing is de-warmed and nothing is copied. The readings are O(1)
`stat` and `PRAGMA` work.

`space.py` already provides the primitives. What this lane adds is the **delta** shape — a
before/after pair — and the `object_role` split, because the current reader is called once
from `verify` against a retained file and reports only the end state.

## 5. What the row reports

```text
states · cumulative logical bytes                 pinned from the corpus
canonical content bytes / objects                 from the Store, pinned
Store allocated / apparent                        O6
pack bodies · canonical bytes by role · non-pack · allocation difference
ratio: cumulative logical ÷ allocated             the dedup claim
ratio: allocated ÷ Git53 or Git157                cited constants
preparation · acquisition · operation · verification · cleanup · complete command
peak incremental heap · phase peak RSS · CPU user/system
read amplification in verification
```

"Auto-deduped" is one comparison: 4,936,693,030 logical bytes across 157 states retained in a
single Store of *X* bytes. v0.1.6's recorded reference is 83,947,520 B — a **58.8×** ratio at
157 states, 26.2× at 53.

## 6. Budgets

The family's budget is **lifted by owner decision**: the ≤ 15 s per-row complete-command rule
does not apply to these three rows, because the workload is a whole history and shrinking it
is not an option.

| budget | rule |
| --- | --- |
| per-lane complete command | **declared before collection**, fixed from the stride-10 measurement and recorded with its source |
| verification | **declared before collection**; a full 157-state read-back was 570.6 s in v0.1.6, so the 60 s default does not transfer |
| cleanup and lifecycle | still bounded — a row that leaks processes or disk still fails |

A lifted budget is a **declared ceiling, not an absence of one**. `benchmark_rules.md` §11
still applies in full: no timeout inflated after a valid miss, no tier shrunk, one sample per
case per arm, budgets frozen before collection.

**What the harness now classifies.** Since `2f8ebc90d`, `../CONTRACT.md` §4 fixes the
complete-command budget as a formula rather than the raw wall:

```text
budgeted = declared_ns + LIFECYCLE_ALLOWANCE_NS
declared_ns = preparation + operation + verification + cleanup
LIFECYCLE_ALLOWANCE_NS = 250 ms   fork/exec/dyld, the trace header, gate assembly, teardown
```

recorded as erratum **E4** in §11 because the contract was frozen against the wall. The wall is
still published as `complete_command_ns`, and the allowance cannot hide work: reconciliation
independently requires `declared <= invocation <= wall` inside the declared tolerance, so a row
whose unaccounted span exceeds it fails reconciliation first and is `INCOMPLETE`.

For these three rows the family budget is lifted above that formula, and the lane ceiling is
declared per lane.

`benchmark_rules.md` §15 also governs what may run by default: `history-stride1` is explicitly
selectable and **no default invocation launches it**.

## 7. The falsifiers

Time against v0.1.6 is a **one-sided tripwire**: this lane has no container, FUSE mount, spool
or Commit envelope, so a speed-up is guaranteed by the surface change and proves nothing.
Passing proves nothing; failing proves a flaw.

The gates that can actually detect an algorithmic defect:

| falsifier | a defect looks like |
| --- | --- |
| **allocated ÷ cumulative logical** | worse than v0.1.6's 26.2× (53 states) or 58.8× (157) — dedup or compression regressed |
| **allocated vs Git53 / Git157** | worse than 1.298× / 1.489× — the gap to Git widened |
| **allocated vs v0.1.6's recorded bytes** | above 64,024,576 B at 53 states or 83,947,520 B at 157, given the core Store carries strictly less metadata |
| **per-state work time across the three rows** | cost per state rises with history length — each save is rescanning history |
| **peak heap across the three rows** | heap grows with the number of states — the chain is not streaming |
| **`delta.prefix_selected`, `reused`, `inserted`** | zero prefix selection — the delta path is not exercised. Round 5 found `delta.prefix_selected = 0` across all 20 `c2.delta.*` rows |
| **read amplification** | decoded bytes far above requested — the read path decodes more than it serves |
| **canonical content total** | anything other than the §4 pins — the migration is not faithful |

## 8. Sampling policy

One sample per case per arm, fresh `--output` per run, receipts append-only, measurement lock
held for the whole invocation. No n3 and no best-of selection; diagnostics are labelled as
diagnostics and reported beside the gate sample.

For an optimization campaign the #118 regression rule applies prospectively: three fresh
alternating pairs, median paired slowdown greater than `max(15 % of the control median, 3 ms)`
and at least two of three pairs slower. Every attempt is retained; no arm-only retry and no
outlier deletion.

## 9. Identity on every receipt

| field | why |
| --- | --- |
| source commit and tree seal | a rebuilt artifact needs a rebuilt matched arm |
| product seal, compilation seal, dependency seal | the product under test |
| harness binary sha256 and harness Python sha256 | the harness is a measured input |
| harness lock and product lock sha256 | dependency parity |
| registry TSV sha256 | which rows ran |
| **corpus manifest sha256 and pinned tip** | this lane's external input |
| `construction_workers` | must be `1`; no run raises it |
| `cache_state`, `store_state` | declared, never pooled |
