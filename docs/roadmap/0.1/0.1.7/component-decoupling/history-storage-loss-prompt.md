# Retained-history storage loss — investigation handoff

> **Status:** Paste-ready entry point for the investigation squad.
> Tracked as [#187](https://github.com/Ephemeral-AI-Lab/layerfs/issues/187), a sub-issue of
> [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186), itself a sub-issue of
> [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Related: [#185](https://github.com/Ephemeral-AI-Lab/layerfs/issues/185) (deferred, a *different*
> axis), [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) (Stage 7, the LayerStack
> layer).
> Working directory: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.

**Read §3 before §2.** The root cause reported on #186 is **unconfirmed** and may dissolve under
Step 0. Do not build on it.

---

## 1. Your mandate

`history-stride10` retains the deepseek-harness history in a Store **2.65× larger than v0.1.6's for
byte-identical content**. Find **every** root cause — algorithmically and arithmetically — and
propose a solution **better than v0.1.6's**, not merely equal to it.

You are explicitly encouraged to:

- **run small experiments** rather than reason in the abstract. A 30-second measurement beats a
  paragraph of inference, and this investigation has already been wrong twice by inference;
- **use subagents heavily and in parallel**, in the squad structure of §4;
- **search broadly**, including places this document does not name. This document is a starting
  point, not a boundary.

Every claim you make must carry **an arithmetic account that sums**: canonical bytes in, stored
bytes out, per bucket, with the ratios and the residual stated. A claim without arithmetic is a
hypothesis, and must be labelled as one.

## 2. The claim to settle

| | this lane | v0.1.6, 17 states |
| --- | --: | --: |
| Store allocated | **130,863,104 B** | **49,344,512 B** |
| Store apparent | 128,864,256 B | 49,315,940 B |
| pack bodies | 119,894,291 B | — |
| canonical content | 380,921,300 B / 52,032 objects | *not recorded for stride-10* |

Owner ruling 7 on #186 makes "allocated below v0.1.6's recorded bytes" a **gate**. #187's exit
criteria are either that the number lands below 49,344,512 B, or that the two generations are shown
**not comparable** — and that second conclusion needs an owner ruling before it is written down.

## 3. What is established, and what is not

### 3.1 Established — measured, reproducible, do not re-derive

**The content is the same as v0.1.6's.** The union of content the 17 selected states' trees
reference is **371,937,306 B over 44,240 oids**; the Store's canonical total is 380,921,300 B =
**1.024×** for canonical encoding overhead. The same calculation on v0.1.6's *own recorded* stride-3
pin: union 583,508,923 B vs canonical **589,423,458 B** = **1.010×**. This is not a workload
difference.

**The union, per selection**, computed from the corpus:

| selection | states | union oids | union bytes |
| --- | --: | --: | --: |
| stride10 | 17 | 44,240 | 371,937,306 |
| stride3 | 53 | 60,000 | 583,508,923 |
| stride1 | 157 | 75,929 | 891,893,320 |

**Where the bytes go**, from decoding the pack directory (see §6.2 for the technique). The
whole-file lane is one record per group and is therefore attributable per object; `Native` and
`Ordinary` hold **multi-record** groups and must **not** be attributed per object — a naive join
multiply-counts, which is the hazard `history-storage-optimization/README.md` §9 warns about.

| bucket | objects | canonical | stored | ratio |
| --- | --: | --: | --: | --: |
| whole-file, **with** a delta base | 18,344 | 82,033,173 | 17,196,162 | **4.77×** |
| whole-file, **without** one | 25,804 | 266,427,536 | 93,744,892 | **2.84×** |
| other lanes (aggregate only) | 7,884 | 32,460,591 | 8,953,237 | 3.63× |
| **total** | 52,032 | 380,921,300 | 119,894,291 | **3.18×** |

**Delta coverage is 41.5 % by object count but 28.4 % by bytes** — the objects that get a base are
disproportionately the small ones. Delta-able whole-file versions (the path had an earlier version in
the selection): **288,847,711 B**. *Caveat:* the delta-able and fresh classes can overlap when
identical content reappears for a new path, so 28.4 % is a **lower bound** on coverage.

**The mechanism, verified in the code.** There are exactly **two** routes to a delta base:

```
cas/save.rs:100                advisory = object.predecessors().ids().collect()
encoding/delta/select.rs:342   acquisition(input, role, advisory, depth_cap)
                                   for id in advisory { if probe(...) { return Some(id) } }
encoding/delta/candidates.rs   "The cache is owned by one save operation"
                               SLOTS = 1024, REFERENCES = 8192, INDEX_BYTES = 128 KiB
```

The per-save cache cannot reach a previous state's version, because that lives in an earlier save.
Only the advisory route can.

**Three isolated compression measurements**, taken with the system `zstd` CLI on real corpus bytes:

| experiment | result |
| --- | --- |
| 3,000 whole-file objects, compressed **per record** (what the `WholeFile` lane does) | 2.87× |
| the same objects, compressed **grouped** | 3.64× — grouped is worth **1.25×** |
| 197 real changed versions, compressed **alone** | 3.08× |
| the same, compressed **with the previous version as a zstd prefix dictionary** | **10.20×** — the dictionary is worth **3.31×** |

### 3.2 NOT established — the retraction, and the leading hypothesis

Comment 5739744053 on #186 reported the delta path as the root cause. **It is unconfirmed.** Three
corrections, all of which matter to you:

1. It repeated #185's statement that `PredecessorProvenance::ReusedRange` is never constructed by
   product code as though **no producer exists**. That is **false**:
   `file/edit/apply.rs:121`, `file/mapping/build.rs:127` and `filesystem/sorted/page.rs:383` all
   construct `AdvisoryPredecessors`. One provenance variant is unused, not the mechanism.
2. Its counterfactual waterfall mixed canonical-input bytes with stored-output bytes. The one clean
   subtraction is the coverage fix alone: the 206,814,538 canonical bytes that are delta-able but not
   delta'd sit in the 2.84× bucket, and at the measured delta ratio (4.77×) they would be 43,357,765 B
   instead of 72,822,021 B — **≈29 MB of the ~131 MB**.
3. Its claim that this is "a concrete mechanism for why v0.1.7 stores more than v0.1.6" is
   **unsupported**.

**The leading hypothesis is a gap in the harness driver, not the product.**
`src/ops/history.rs` calls `construct_bytes(...)` for every changed file in every state, and that
path builds `FinalizedObject::new(role, canonical)` — **no predecessors**. The Store is therefore
asked to hold every version as though it had never existed. A faithful history save reads the
previous version and produces the new one as an **edit**, and `file/edit/apply.rs:121` *does* declare
the base. **So the 2.65× may be a modelling error, and the storage falsifier may not be firing.**

**And the producer may belong to a layer that does not exist yet.** The reference tree's producer is
`set_physical_predecessor` in **`crates/layerfs-layerstack-store/src/objects.rs:3311`** — the
**LayerStack** layer. `core/crates/` contains only `layerfs-content`, `layerfs-storage` and
`layerfs-telemetry`. The layer that would declare cross-commit bases is **Stage 7 and unbuilt**.

**Scope note on #185.** #185 is scoped to **representation churn** across the 128 KiB cutoff; its
"otherwise we are fine" is about churn. Delta **base availability** is a different axis and is not
limited to 128 KiB. Do not conflate them, in either direction.

## 4. Squad structure — two halves, then a synthesis

The owner's direction: **half the squads compare v0.1.7 against v0.1.6; half propose good approaches
without looking at v0.1.6 code at all; then combine.** The two halves must not read each other's
findings until both report — the point of the blind half is that it is blind.

### Step 0 — settle whether there is a defect at all (do this first, it is small)

Make the driver model a history save faithfully: declare the previous version's content root as an
advisory predecessor for each constructed whole-file object. The harness's own `TreeStore::accept`
sees every `FinalizedObject` before it reaches the save, so this is a wrapper in `ops/history.rs` —
**not a product change**. Alternatively model the state as an edit of the previous version through
`apply_edits`.

Re-run and read the same two buckets.

- Coverage moves toward 100 % of delta-able bytes and allocated lands near 49 MB ⇒ the gap was the
  harness. **Say so, record it, and the investigation's remaining value is the "better than v0.1.6"
  question in §4C.**
- It barely moves ⇒ the harness is excluded and squads A and B both start from a real defect.

**Do not skip this step to get to the interesting work.** A deep search on top of an unconfirmed
premise is how this investigation already went wrong once.

### Step 1 — launch both halves in parallel

**Squad A — comparative (`v0.1.7` vs `v0.1.6`).** Read both trees. Suggested split, but add your own:

- **A1 — policy and codec.** `whole_file_delta_max_depth`, `chunk_delta_max_depth`,
  `metadata_delta_max_depth`, chain canonical/encoded limits, zstd levels and window logs, group
  limits, the `WholeFile`/`Native`/`Ordinary`/`PooledMetadata`/`Singleton` lane rules and their
  framing. Build a **field-by-field table** of both generations and mark every difference. Then say,
  arithmetically, which differences can account for bytes.
- **A2 — the producer.** Where does each tree declare a delta base, on which path, from which layer?
  Trace `set_physical_predecessor` and its callers in `crates/`, and every `AdvisoryPredecessors`
  construction in `core/`. Produce a **call-graph diff**, not a list.
- **A3 — the LayerStack layer.** What would Stage 7 supply that core cannot? Read
  `crates/layerfs-layerstack-store/` and the Stage 7 specification. Is the producer's absence an
  expected migration-stage consequence, or a genuine omission in core?
- **A4 — empirical reconstruction.** v0.1.6's stride-10 Store is not retained, but its stride-3 and
  stride-1 records are. Can you reconstruct what its *per-bucket* numbers must have been, from its
  recorded canonical totals and allocated bytes, tightly enough to falsify a hypothesis? State the
  bounds explicitly.

**Squad B — independent design, blind to v0.1.6.** **Do not read `crates/`.** Do not read Squad A's
output. Answer from first principles, against the corpus and the core as they are:

- **B1 — the floor.** What is the information-theoretic and practical floor for retaining this
  corpus? Compute it: compress the whole union as one stream; compress per-path version chains;
  measure the entropy of the delta stream. Give a **number** for the best achievable size, with the
  method.
- **B2 — base selection as an algorithm.** What base choice maximises bytes saved, given the corpus's
  actual edit distances? Design the selection rule, state its inputs and its cost model, and measure
  what it would achieve on the real corpus. Consider: same-path previous version, same-path *any*
  version, cross-path similarity, depth, and chain-length limits.
- **B3 — framing and codec.** Grouped versus per-record, dictionary scope, level, window log, and
  whether the current lane split (`WholeFile` and `Native` = `GroupCodec::Raw`, one record per group)
  is right for a history workload. Measure on real corpus bytes.
- **B4 — a reference implementation.** Write a small Python (or Rust) model that reads the corpus and
  produces the **smallest** Store it can, with the arithmetic to prove it. It does not have to be
  shippable; it has to be a number this campaign can aim at.

### Step 2 — synthesis (Squad C, after both halves report)

Combine. Where A and B agree, that is a finding. Where they disagree, that is the interesting part —
say which is right and **measure it**. Then propose a solution that beats **both** v0.1.6 and Squad
B's floor model, and state by how much, arithmetically.

## 5. Method requirements

- **Arithmetic or it did not happen.** Every root cause carries: the bytes it accounts for, the
  measurement that produced them, and the residual it leaves. The residuals across all causes must
  sum to the gap, or the unexplained remainder must be stated as a number.
- **Attribute only what is attributable.** The `Native` and `Ordinary` lanes hold multi-record
  groups; per-object attribution there multiply-counts. Either decode the group grammar or report
  those lanes as an aggregate — never a naive join.
- **One variable per experiment.** A change that moves two things proves neither.
- **Label diagnostics.** This is an investigation, not a measurement campaign. Nothing you produce is
  admission evidence, and every number is `diagnostic` until it is re-run under #186's contract.
- **Retain negative results.** A ruled-out hypothesis is a deliverable. Say what you ruled out, how,
  and with what number.
- **Read the specification before you optimise.** `history-storage-optimization/README.md` §5 and §7,
  and `docs/roadmap/0.1/0.1.7/retained-history-storage.md` §8. The one legitimate comparison is
  identity-anchored; a performance pairing is forbidden by `CONTRACT.md` D1.

## 6. Instruments you already have

### 6.1 The lane as it stands

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
export LAYERFS_CONSTRUCTION_WORKERS=1
$H/target/release/fs-bench-storage-content \
    --case history-stride10 --out /tmp/run --corpus /Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
```

`timing.json` is the product's own tree: one root, **17 named children**, and the row's
`operation_ns` is the **sum of those children**, not the root. The root is ~49.7 s against a 32.7 s
sum, because the harness's own untimed corpus reading sits inside the root and outside every child.

### 6.2 Decoding the pack directory — the instrument that produced §3.1

The `objects` table gives `canonical_length` and `base_object_id`; it does not give stored size. The
pack directory does. Layout, from `layerfs-storage/src/pack/layout.rs` and `pack/assemble.rs`:

```
HEADER_LEN = 16            PACK_MAGIC[8] + version u32 LE + group count u32 LE
WholeFile directory        one u32 offset per group (WHOLE_FILE_ENTRY_LEN = 4)
other lanes                start u32, encoded u32, decoded u32, codec u8 + 3 pad (16 B)
                           codec: 0 = Raw, 1 = Zstandard
VERSION_LANE = {1: Ordinary, 2: Native, 4: WholeFile, 6: PooledMetadata, 7: Singleton}
```

Join `objects(pack_id, group_number)` to the group's byte range. **Only the `WholeFile` lane is one
record per group.** Move this into `shared/space.py` so it becomes a receipt field rather than a
one-off script — #187 step 2 asks for exactly that.

### 6.3 The corpus reader

`src/workload/history.rs` authenticates the corpus and serves changed bytes; `--history-corpus <row>
--corpus <path> [--history-walk]` exercises it without running a product row. Three corpus facts are
verified on **all 157 checkpoints** and documented in that module — read them before writing your own
reader, and do not re-derive them:

1. `previous.tsv` of checkpoint *k* is the manifest of *k−1*, **not** the previous *selected* state's
   tree (they coincide only for stride1).
2. `blobs/` of checkpoint *k* is exactly the oids of the paths *k* added or whose oid changed — a
   **superset** of `oids(manifest) − oids(previous)` by 1–11 entries per state.
3. A state's path-states is its **oracle entry count** — files **and** directories. The manifest's
   `files` field is about 18 % smaller and is a different quantity.

### 6.4 The selection counters are already there

`select.rs` counts `prepared_full`, `no_candidate`, `absent_candidates`, `ineligible_candidates`,
`trials` and `work_exceeded`. **This row publishes none of them**, which is why "cannot see it" and
"sees it and declines it" are currently indistinguishable. Publishing them is the cheapest way to
turn a hypothesis into a gate.

## 7. Guardrails — not negotiable

- **The 217-row lane is not yours to change.** It keeps its registry, cardinality, golden table,
  `--lane full` composition and `full` verification default. `history.*` is outside it by
  construction: `--lane full` is 220 rows, `--smoke` is 20, both unchanged.
- **Never optimise `history-stride1`.** No constant, threshold, buffer, batch or policy value may be
  changed because of a stride1 number. A stride1 failure while both lower tiers pass is a scaling
  finding to investigate at stride10/stride3.
- **Iterate on stride10; confirm on stride3.** No n3, no best-of, no re-run for a nicer number.
- **A canonical pin that does not match stops the phase.** It is a finding, not a fixture tweak.
- **No product source change without an owner ruling.** Steps 0 and 1 are harness-side and analysis.
  If your conclusion requires changing `core/crates/`, **stop and report** — that is a different
  issue with a different owner.
- **`Production LOC: 84936 -> 84936 (delta 0)`** in every commit, with harness lines stated
  separately. No lockfile move, no new dependency: `shared/test_lock_parity.py` fails a harness-only
  registry package.
- **No CI claim.** This repository runs no CI, `tools/preflight.sh` is permanently retired, and
  `cargo fmt --check` is not clean on this tree — do not reformat files outside your change.
- **Do not edit the historical record.** Rounds 1–5b and their prompts, plans, handoffs and receipts
  are not rewritten. `#171` is closed; do not reopen it.

## 8. Deliverables

1. **A root-cause register**: every cause, its byte account, its measurement, and the residual. The
   residuals must sum to the gap or the remainder must be a number.
2. **The Step 0 result**, reported whichever way it falls. If it dissolves the gap, that is the
   headline and the rest is the "better than v0.1.6" question.
3. **Squad A's call-graph and policy diffs**, and **Squad B's floor number** with its method.
4. **A proposal that beats v0.1.6**, with the arithmetic for how much and the experiments that
   support it.
5. **Everything ruled out**, with the number that ruled it out.
6. **Evidence under `docs/roadmap/0.1/0.1.7/evidence/<stamp>/`**, append-only, with the commands run
   and every check *not* run named with its reason.

## 9. Where everything is

| | |
| --- | --- |
| Specification | [`retained-history-storage.md`](../retained-history-storage.md) |
| Case documents | `core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/` — README §7 rulings, §9 errata |
| Phase 0/1 evidence | `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-phase1-20260919T000000Z/` |
| Driver | `core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs` |
| Corpus reader | `core/benchmark/fs-bench-pro-storage-content/src/workload/history.rs` |
| Storage readings | `core/benchmark/fs-bench-pro-storage-content/shared/space.py` |
| v0.1.6 record | `docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md` (ledger `L31`) |
| Deferred, other axis | [`core/docs/architecture/deferred/01-size-transition-delta-hints.md`](../../../../core/docs/architecture/deferred/01-size-transition-delta-hints.md) |

**Known harness gaps that will bite you:** the verify phase, the golden table,
`tests/history_declarations.rs`, the runner's lane wiring and the per-lane complete-command ceilings
are **not built**, so the row cannot yet be run through `runner.py perf`. And two readings of
`st_blocks × 512` on the same closed Store differ by 4 MB (134,889,472 B inside the child,
130,863,104 B later) — resolve that before any gate decides on allocated bytes.
