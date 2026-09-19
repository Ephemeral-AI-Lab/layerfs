# v0.1.7: retained history storage — specification

> **Status:** Specification, 2026-09-19. Frozen before any product run of this family.
> Tracking: [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186), a sub-issue of
> [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Measurement harness:
> [`core/benchmark/fs-bench-pro-storage-content/`](../../../../core/benchmark/fs-bench-pro-storage-content/).
> Detailed case documents:
> [`core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/`](../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/).

This document is the roadmap-level specification `docs/general/benchmark_rules.md` §1 requires
before benchmark implementation or sample collection begins. It **freezes the claim, the
membership, the gates and the budgets' sources**; the five campaign documents under the harness
carry the file-level design and are subordinate to this one. Neither amends
[`CONTRACT.md`](../../../../core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md) or its
217 admission rows: `history.*` is a **separate claim in a separate lane**.

```text
claim_kind = history-storage-efficiency
```

## 1. The question and the exact claim

**The question:** what does it cost the replacement C1/C2 core to retain a real repository
history in one Store?

**The claim this specification may support:** the replacement C1/C2 core saves a real
157-checkpoint repository history into one Store, the Store deduplicates it automatically, and
every state verifies against its original oracle.

**The claim it is NOT allowed to support:**

- any Commit, LayerStack, Branch, FUSE, daemon, container or cgroup claim — those are Stage 7
  ([#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172)); this lane has no runtime
  envelope at all, and that is the point of it;
- any v0.1.6 pairing as a *performance* gate — `CONTRACT.md` decision D1 forbids pairing a core
  family with a v0.1.6 family for a claim. §8 records the one identity-anchored comparison that
  is legitimate and why;
- any claim that a time improvement demonstrates algorithmic quality — see §8.

**This lane may not be counted in the 217, and the 217 may not be counted in it.** The group is
`history.*`: three admission rows, cardinality 3, outside `FROZEN_CARDINALITY`, outside
`--lane full`, and outside every 217-row tally, golden table and verdict.

## 2. Membership, identities and cardinality

| row ID | selection | states | cumulative logical bytes | lane |
| --- | --- | --: | --: | --- |
| `history-stride10` | `range(1,158,10) ∪ {157}` | 17 | 561,010,345 | `history-stride10` |
| `history-stride3` | `range(1,158,3)` | 53 | 1,676,767,835 | `history-stride3` |
| `history-stride1` | all 157 checkpoints | 157 | 4,936,693,030 | `history-stride1` |

Expected cardinality: **3**. Each row is also its own lane. The selections are **independent
workloads, not samples of one another**: stride-3 and stride-10 take direct transitions between
selected states and never replay a skipped state, so their per-transition deltas are larger than
stride-1's. A stride-1 result may not be inferred from stride-10 or stride-3.

**Frozen source identity.**

| | value |
| --- | --- |
| Source | `https://github.com/deepseek-ai/deepseek-harness.git` |
| Pinned tip | `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed` |
| Checkpoint manifest SHA256 | `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271` |
| Checkpoints | 157 of 15,632 reachable commits |
| Corpus root | `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data` |
| Corpus bytes | 2.3 GB, read-only, never copied into the repository |

The corpus is reached through an explicit `--corpus` path that **fails closed** when absent or
when any identity disagrees. There is no default substitution and no reduced selection: an
absent corpus stops the campaign rather than shrinking it.

## 3. The approved operation, entrypoint and acknowledgement

The measured operation is the **product's own public C1/C2 API**, exactly as the 217 use it:
`layerfs_content::construct_files` (C1) and `layerfs_storage::Store` create/begin_save/accept/
finish (C2). No new product entrypoint is introduced and **no product source file is touched by
this campaign**.

```text
create the Store
for each state k in the selection, in order:
    read state k's changed bytes from the corpus      untimed, harness
      construct the changed content                   TIMED
      build_filesystem against the previous root      TIMED
      save                                            TIMED
close
```

`InodeUpdate.value` is `InodeValue { kind, namespace_ref_count, content_root, metadata_root }` —
it references content **by id** and carries no bytes. The content objects are therefore **what
the measured child produces, not an input**, and no constructed object is supplied to a measured
child. State 1 builds a new filesystem (`base: None`); every later state passes the previous
state's root, read back **from the Store** through `StoreProvider`.

## 4. Fixtures, cache state and source arms

**There is no fixture and nothing is prepared.** `Preparation::InProcess`; `store_state` and
`cache_state` are `CreatedInSample`; nothing is written under `prepared/`. The expensive work —
saving N states — *is* the measurement, so moving any of it into preparation would remove the
thing being measured. A second run costs what the first cost, and that is correct here.

Nothing is pre-loaded: a 157-state selection is 4.94 GB of cumulative logical bytes, and holding
it resident would both defeat the memory claim and warm the pages the saves read.

**One arm.** There is no control arm and no candidate arm in this lane: every gate is absolute
and single-arm, and the only permitted comparison is the identity-anchored one in §8.

**Cache state.** Each state's changed bytes are read from the corpus **between** the timed
children, inside the work phase but outside every timer. No timed region reads the corpus.

## 5. Timing boundary, included and excluded actions

| | phase | what runs | published as | budgeted |
| --- | --- | --- | --- | --- |
| a | preparation | authenticate the corpus, resolve the selection, preflight disk | `preparation_wall_ns`; `acquisition_wall_ns` is **zero** | no — published only |
| b | **work** | the N per-state children | **`operation_ns` = the sum of the N named children** | **yes — the performance claim** |
| c | verification | the final gate of §7, in its own invocation | `verification_wall_ns` | its own declared budget |
| d | cleanup | destroy the Store, close | `cleanup_wall_ns` | inside the complete-command wall |
| | | process wall | `complete_command_ns` | the declared lane ceiling |
| | | harness work inside a timer | `handoff_ns` | published beside `operation_ns`, never inside it |

**`operation_ns` is the sum of the children, not the root** (owner ruling 2, §9). A root would
include the harness's own corpus reading between the children, which is harness work and must not
appear as product time.

**Included in a child:** the state's changed content construction, the tree update against the
previous root, and the save (`begin_save`, `accept`, `finish`). **Excluded:** reading the
corpus, opening the output directory, the storage readings, and the root read-back between
states.

`Store::create` sits **inside state 1's child** and is published as `history.state.1.create_ns`,
so the fixed cost is visible and subtractable rather than buried in the chain.

## 6. Raw metrics, units and formulas

```text
states · cumulative logical bytes          pinned from the corpus
path-states                                the entry count of oracles/<sha>.json (files + directories)
canonical content bytes / objects          from the Store, pinned
Store allocated / apparent                 O6, before and after the chain
pack bodies · canonical bytes by role · non-pack · allocation difference
ratio: cumulative logical ÷ allocated      the dedup claim
ratio: allocated ÷ Git53 or Git157         cited constants, not re-run
preparation · acquisition · operation · verification · cleanup · complete command
peak incremental heap · phase peak RSS · CPU user/system
read amplification in verification
delta.prefix_selected · reused · inserted
```

Units are bytes, nanoseconds (`CLOCK_MONOTONIC_RAW`, id 4, Rust and Python alike) and counts.
The counting `GlobalAlloc` is the precise **phase** memory figure; `rss.process_peak_bytes` is a
**lifetime** figure, named a process peak for that reason. CPU is a diagnostic and never
gate-decides.

## 7. Gates

**Absolute and single-arm.** `elapsed_ns` never decides a gate.

| gate | class | requires | id |
| --- | --- | --- | --- |
| state roots | Correctness | every state's root equals its pin (O1) | `g1.o1-state-root` |
| pinned counters | Correctness | canonical bytes and object counts equal their pins (O3) | `g1.o3-pinned-counters` |
| state trees | Correctness | every state's tree equals `oracles/<sha>.json` (O4) | `g1.o4-state-tree` |
| sampled bytes | Correctness | the sampled files read back byte-exact through the Store (O2) | `g1.o2-sampled-bytes` |
| pack accounting | Correctness | `pack_bodies <= database` | `g1.pack-accounting` *(existing)* |
| handoff | Mechanism | objects C1 emitted equal objects the save acknowledged | `g2.handoff` *(existing)* |
| one file | Cleanup | exactly one Store file | `g5.one-file` *(existing)* |
| no sidecars | Cleanup | no `-wal` / `-shm` / `-journal` | `g5.no-sidecars` *(existing)* |
| store exists | Custody | the Store the run names is on disk | `g6.store-exists` *(existing)* |
| tree complete | TimingPurity | the product's timing tree is complete | `g7.tree-complete` *(existing)* |
| swaps | Resource | zero swaps | `gates::swap_gate` *(existing)* |
| allocated vs v0.1.6 | Storage | allocated **below** the recorded bytes of §8 | `g1.o6-below-v016` |

Existing ids are **reused, not re-spelled**: a second spelling of the same gate would let one of
them drift.

**The final gate is all-or-nothing.** For every state: the root equals its pin, the tree equals
`oracles/<sha>.json`, and a deterministic 10 % of its files reads back byte-exact. A disagreement
fails the row — "156 of 157" is a failure, not a pass — and increments `verify.disagreements`.

**Frozen pins.**

| row | path-states | logical bytes | canonical content | canonical objects |
| --- | --: | --: | --: | --: |
| `history-stride10` | 101,477 | 561,010,345 | *first-run pin* | *first-run pin* |
| `history-stride3` | 306,861 | 1,676,767,835 | 589,423,458 B | 73,476 |
| `history-stride1` | 904,143 | 4,936,693,030 | 871,588,115 B | 104,705 |

`path-states` is the **entry count of the state's oracle**, which is files **plus directories**;
it is not `checkpoints[].files` and not the `manifest.tsv` line count, both of which are short by
about 18 %. All three pairs above reproduce exactly under that definition (erratum E1).

The stride-3 and stride-1 canonical totals are recorded from the v0.1.6 campaign on
byte-identical source trees and are **gates** (owner ruling 5): a mismatch is a finding about the
migration, and it stops the phase. `history-stride10` has no recorded canonical total, so its
canonical numbers become first-run pins and thereafter must be **reproduced**, with a counter
that moves being a `FAIL` rather than a new baseline. The table is embedded with `include_str!`
so the harness identity covers it.

## 8. Baseline, permitted differences, and the one legitimate comparison

v0.1.6's recorded Store allocated / apparent bytes, from
`docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md` (ledger `L31`):

| states | allocated | apparent | Commit sum |
| --: | --: | --: | --: |
| 17 | 49,344,512 | 49,315,940 | 11.371 s |
| 53 | 64,024,576 | 64,000,100 | 24.815 s |
| 157 | 83,947,520 | 82,677,860 | 64.108 s |

Matched Git comparators, **cited rather than re-run**: Git53 = 49,332,224 B, Git157 =
56,373,248 B.

**Storage is a real falsifier and a gate** (owner ruling 7). The canonical content is pinned
byte-identical across the two generations and the Store format is preserved, so the same content
in the same format should produce a comparable file, differing only by the commit and branch
metadata the core Store does not carry. **The core figure must land below v0.1.6's recorded
bytes at all three sizes** — 49,344,512 / 64,024,576 / 83,947,520 B — and landing above any of
them is an algorithmic finding, not a new baseline.

**Time is a one-sided tripwire, not a gate** (owner ruling 6). This lane has no container, no
FUSE mount, no spool and no Commit envelope, so a speed-up is guaranteed by the surface change
and proves nothing about the algorithm:

> **Passing the time comparison proves nothing. Failing it proves a flaw.**

The tripwire is published in every receipt and report, labelled as such, and never decides a
status.

**Permitted difference from v0.1.6:** the absence of the runtime envelope. Nothing else. The
source trees, the canonical content, the Store format and the selections are identical by
construction, and any other difference is a finding.

## 9. Owner rulings (2026-09-19)

Ruled by the owner before the first product run, as `benchmark_rules.md` §1 requires.

**Family rulings.**

1. **The family's budget is lifted.** The ≤ 15 s per-row complete-command rule does not apply to
   these three rows. The lane ceilings are **declared** instead, and a lifted budget is a
   declared ceiling, not an absence of one.
2. **The tiers are stride10 (smoke, P0), stride3 (intermediate, P0), stride1 (run only).**
   stride1 is **never optimized**: no constant, threshold, buffer, batch or policy value is
   changed because of a stride1 number, and a stride1 failure while both lower tiers pass is a
   scaling finding to investigate at stride10/stride3, not to patch at stride1.
3. **This lane builds no prepared storage.** `Preparation::InProcess`; nothing under `prepared/`.

**The seven decisions of the campaign README §7.**

| # | decision | ruling |
| --: | --- | --- |
| 1 | per-lane and verification budgets, the family budget being lifted | **Ruled as a procedure.** The per-tier lane ceilings and the verification budget are declared at Phase 3 from the stride-10 baseline, recorded with their source, and frozen before any optimization work. The numbers cannot exist before that baseline by construction. `runner.py` carries them as **per-lane declared ceilings**, not as `DECLARED_EXCEPTIONS` entries — the exception list is capped at 25 s and cannot carry a 157-state run. A ceiling is never inflated after a valid miss. |
| 2 | is `operation_ns` the sum of the N named per-state children? | **Yes.** A root would include untimed corpus reading. The product's timing tree carries one named child per state and the row's `operation_ns` is their sum. `shared/phases.py`'s "two sources, one number" check is **lane-scoped** for this shape: it requires `root >= Σ children`, and the difference is the harness's own untimed work, published rather than absorbed. |
| 3 | may a sampled row in this lane be `PASS`? | **Yes, for this lane only, frozen before collection**, because the storage counters are never sampled: the O(1) counters that decide storage are read in full in every mode. The 217 keeps `full` as its default and keeps forcing `sample` to `INCOMPLETE`. |
| 4 | sampled unit and selection rule | **The state's file manifest** — the same list O4 compares — in corpus order. `max(1, ceil(n/10))` units selected by `index % 10 == 0`, **plus the first and last manifest entry of every state**, because that is where boundary defects live. The rule is **lane-scoped**: `sampled_indices` is used by the 217's `c2.delta.*` rows, and no 217 `verification_selection` string may move. |
| 5 | are the §7 canonical totals gates or diagnostics? | **Gates** — identity, not performance. A mismatch is a `FAIL` and stops Phases 4 and 5. stride-10's are first-run pins, established once and thereafter reproduced. |
| 6 | is the v0.1.6 timing comparison accepted as a labelled one-sided tripwire? | **Yes**, stated as in §8: published, labelled, never gate-deciding. |
| 7 | is the storage comparison against v0.1.6 a **gate**? | **Yes**, and at **all three sizes**: allocated below 49,344,512 B at 17 states, 64,024,576 B at 53 and 83,947,520 B at 157. The Store format is preserved and the content is pinned identical, so this is an identity-anchored comparison rather than a cross-generation performance pairing. |

## 10. Verifier coverage and the independent oracle

Verification is a **second, unmeasured invocation** with its own declared budget. It reopens the
Store the performance invocation left behind and reads the history back out of it. It never
enters a performance distribution, and a verifier failure preserves the performance sample while
failing the row's admission.

| oracle | what it checks | coverage |
| --- | --- | --- |
| O1 identity | each state's filesystem root `ObjectId` equals its pin | full, N states |
| O2 logical equality | each sampled file's bytes read back **through the Store** and compared to the corpus oracle's sha256 | deterministic 10 %, endpoints included |
| O3 structural count | canonical bytes, object count and pack count pinned | full |
| O4 tree equality | each state's entry manifest equals `oracles/<sha>.json` | full, N states |
| O6 footprint | allocated vs apparent, pack bodies, freelist, sidecar absence, attribution | full |
| O7 SQL invariants | schema identity, 4 tables, 2 indexes, watermark `I1`, `quick_check == ok` | full |

Only O2 is sampled. O2 is deduplicated by **distinct object** before sampling, because
consecutive states share most of their objects. O4 is not a read-back: it compares metadata
against the corpus oracle and is cheap, so it runs over every state.

**A replay that equals the measured result is not a proof.** O1 compares against a **pinned
constant**, not against a replay.

## 11. Budgets

| budget | rule |
| --- | --- |
| per-lane complete command | **declared at Phase 3** from the stride-10 baseline, recorded with its source, frozen before optimization |
| verification | **declared at Phase 3**; the 60 s default does not transfer, because v0.1.6's 157-state read-back was 570.6 s |
| cleanup and lifecycle | still bounded — a row that leaks processes or disk still fails |

`benchmark_rules.md` §11 applies in full: no timeout inflated after a valid miss, no tier shrunk,
one sample per case per arm, budgets frozen before collection. **`history-stride1` is explicitly
selectable and no default invocation launches it** (§15).

## 12. Result layout

```text
benchmark-results/fs-bench-pro-storage-content/   gitignored, development runs
  <run>/<case_id>/timing.json        byte-verbatim product receipt
  <run>/<case_id>/trace.jsonl        the harness trace
  <run>/<case_id>/phases-*.json      the phase spans each invocation observed
  <run>/<case_id>/receipt.json       derived: identity, gates, statuses, counters, phases, budget
  <run>/run.json, manifest.json, verification.json, report.txt
docs/roadmap/0.1/0.1.7/evidence/<stamp>/    admission evidence, append-only
```

Receipts are append-only and are never edited. Failures, `INELIGIBLE` rows and discarded
attempts stay on disk. Every receipt carries the corpus manifest sha256 and pinned tip, the
product/compilation/dependency seals, the harness identity, `construction_workers = 1`,
`cache_state`, `store_state`, and the verification mode with its selection rule and omissions.

## 13. What this specification does not do

- It does not amend `CONTRACT.md` or its 217 admission rows, their registry, cardinality, golden
  table, `--lane full` composition or `full` verification default.
- It does not re-open [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171), which is
  closed.
- It does not edit the historical record: rounds 1–5b and their prompts, plans, handoffs and
  receipts are not rewritten.
- It states no measurement. Every figure in §7 and §8 is quoted from the v0.1.6 record; none is
  a measurement of the replacement product.
