# Handoff: give the 100,000-entry row a bounded fixture

> **Status:** handoff and commission. Filed from
> [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219) as its sub-issue
> [#226](https://github.com/Ephemeral-AI-Lab/layerfs/issues/226), after round 21. **It makes no
> performance claim of its own and pre-authorises no product change.** Every number is sourced to a
> receipt, a counter or a `file:line`; the two that are inferences are labelled as such.

**What this document is for.** Round 21 registered and measured `pipeline-namespace-100000`, and the
row passes — but it peaks at 1.05 GB of resident memory, and 96 % of that is the harness's own fixture
held *before* the timer. This handoff gives the next agent (a) the attribution, already measured, (b)
the two levers with their prices, (c) the contract that decides which lever is legal, and (d) the
options that need an owner ruling before any code depends on them. **It does not authorise the
product-side design; it commissions the investigation and prices the fix.**

---

## 1. Where things stand, all of it measured

**The row.** `pipeline-namespace-100000`, in the core harness's `pipeline.*` group. Registered,
pinned and measured in round 21: **`PASS` 13/13**, one sample, `--verify full`, fresh `--out`,
complete command 6.57 s, no `DECLARED_EXCEPTIONS` entry. Source
`core/benchmark/fs-bench-pro-storage-content/`, declaration
`ops::namespace_content::Declaration::LARGE` (100,000 files / 1,000 directories / 500,000,000 decimal
bytes / two 100,000,000-byte anchors).

**The worktree.** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch `codex/219-ns10000`,
pushed at **`a22e98bb7`**, tree clean. The measurement lock is per worktree
(`core/benchmark/fs-bench-pro-storage-content/.measurement.lock`, created on first use).

**The evidence.** [`evidence/issue219-ns21-100k-20260921T155500Z/report.md`](../../evidence/issue219-ns21-100k-20260921T155500Z/report.md)
§11, with raw receipts under its `raw/`. §11.1 is the instrument, §11.3 the attribution, §11.4 the
priced lever.

**The memory, attributed.** `resources.rss.process_peak_bytes` = 1,050,738,688 is a **lifetime**
high-water (`getrusage(RUSAGE_SELF).ru_maxrss`), so round 21 added a 10 ms `RssSampler` around the
measured closure and re-measured:

| | bytes | share of peak |
| --- | ---: | ---: |
| lifetime peak | 1,050,738,688 | 100 % |
| **measured-region baseline** — held before the timer | **1,013,219,328** | **96 %** |
| **measured-region increment** — the product and its Store | **37,519,360** | **3.6 %** |

and the baseline decomposes as:

| component | bytes | share | `file:line` |
| --- | ---: | ---: | --- |
| the content store — the declared 500 MB held in RAM | ~728,465,408 | 72 % | `src/ops/pipeline.rs:735`, `:1040` |
| the 25 per-batch prefix snapshots | **164,347,158** | 16 % | `src/ops/pipeline.rs:846`, `:885-886` |
| the prepared tree and the plan | ~55,394,304 | 5 % | `src/ops/pipeline.rs:687-720` |
| everything else, incl. the measured region | ~65 MB | 6 % | — |

**The product's measured region is bounded and improves per byte** — 27,197,440 B over 301,171,810
canonical at 10,000 entries and 37,519,360 B over 502,914,928 at 100,000, i.e. **9.03 % → 7.42 %**.
**This handoff is not about the product's measured memory.** Do not go looking for a product leak.

**The reference harness does not have this problem.** It streams each file to disk through
`NamespaceContentStream` over `NAMESPACE_SCRATCH_BYTES = 1 MiB`
(`benchmark/fs-bench-pro/workload/main.rs:120`, `:936-948`), so its fixture is bounded at 1 MiB where
this row's is 500 MB.

## 2. What is commissioned

Three steps, in this order. Step 1 is contained and is expected to land. Step 2 is a design page and
**stops at a recommendation** unless the owner rules on §4. Step 3 happens only if a ruling is given.

### Step 1 — remove the redundant prefix snapshots (164,347,158 bytes, no measurement change)

`prefixes` keeps one `TreeStore` snapshot per batch (`src/ops/pipeline.rs:846`) and
`TreeStore::absorb` **copies** (`src/workload/providers.rs:103`). Each snapshot is a strict subset of
the next, so the driver retains the sum over batches of the chain so far:

```text
batches 25   snapshots retained 25   objects retained 66,824   final chain 4,221
heap with all snapshots 176,799,943 B
heap with the chain alone 12,452,785 B
cost of the snapshots  164,347,158 B = 14.2 x the chain
```

They exist because `prefixes[index]` is the reader for batch `index` in both the timed pass
(`:984`) and the oracle replay (`:1100`), and a batch must be served the chain **as it stood before
that batch** — measured in round 19, where handing a batch the complete chain returned
`cycle check work limit`.

**That is a statement about which objects a reader may serve, not about holding a separate copy.** A
provider holding the one complete chain plus the `ObjectId` set of a batch's prefix serves the identical
object set and fails identically — `PairProvider` returns `ContentError::MissingObject` for anything
absent (`src/workload/providers.rs:302`). Target: **12.5 MB instead of 176.8 MB.**

**Acceptance for step 1:** the row still `PASS` and **all fifteen pins reproduce**, including
`digest:filesystem_root 2412681d335571082c4dfbc1df2117bc015b7f6132fca17cb0d11bbeaaefd954`. A pin that
moves means the change reached the measurement and the change is wrong. The row's declared figure is
expected to move by less than the round's own spread, which round 21 measured at **3.38 %** within one
session (`report.md` §11.6) — register your own tolerance before the run and say what you registered.

### Step 2 — design page for the content store (~728,465,408 bytes)

Write the page; do not implement it. It must price each option in §3 and recommend one, and it must
name which options need an owner ruling. Put it at
`docs/roadmap/0.1/0.1.7/issue226-bounded-fixture-design.md`, in the shape of
[`issue219-ns20-scaling-decision.md`](../issue219-ns20-scaling-decision.md): what is decided, what is
priced, what is *not* claimed, and a "what must not be redone" table.

### Step 3 — only if the owner rules

Re-measure the row (`--verify full`, one sample, fresh `--out`, rebuilt binary) with the session anchor
taken **locked, in the same window, immediately before or after** it. The anchor is
`c1.fs.build-scale`'s `namespace-10000`; the reference value is **68,514,625 ns** and round 21's was
**68,951,625 ns (+0.64 %)**. If your anchor lands outside ±20 % of 68,514,625, the session is not
comparable and you report the run as within-session only.

## 3. The options, each with its price

| option | what it costs | what it changes |
| --- | --- | --- |
| **A. Spill the constructed objects to disk before the timer, stream them back in a bounded window inside it** | the measured region gains a disk read of ~503 MB; the row's `cache_contract` must be declared and enforced for the spilled pages and `g4.residency` re-read | memory becomes O(window). This is what the reference harness does, at 1 MiB. **`TreeStore::write_to_dir` / `load_from_dir` already exist (`providers.rs:124`, `:143`) but `load_from_dir` loads the whole store — a bounded reader is new work** |
| **B. Keep constructing in memory but consume each object as it is accepted** | the peak becomes the content itself (~558 MB) rather than content plus the `cloned_object` copy (`:1040`); needs an owned drain | halves the lever, does not bound it |
| **C. Build the tree twice — once untimed to establish a base Store that already holds the content, once timed** | ~1.1 s more preparation at 100,000 entries (inside budget, untimed); the C1 work is paid twice | memory becomes O(1) |
| **D. Construct inside the timer and declare the boundary change** | the row's declared figure starts including construction; **every existing figure for this row is invalidated and the bar needs re-reading** | changes what the row measures — an owner ruling, not this issue's to take |

**A and D need an owner ruling. B and C do not.** A is preferred over C if the cache contract can be
declared and gated cleanly, because A is what the reference harness does and C pays the C1 work twice to
avoid a read the row would have to declare anyway.

**Explicitly out of scope: shrinking the declaration.** The 500,000,000 bytes are the reference
harness's own `namespace-100000` scenario, ported field for field in round 21. A smaller fixture makes
the row cheap and stops it being about the bar's case.

## 4. The contract that decides which option is legal

1. **The timer's boundary is fixed by `test_setup_and_cache_discipline.md` §2.2**: content construction
   stays outside it; the C1 tree build and the save stay inside it. The driver's own statement of
   that boundary is `src/ops/pipeline.rs:945-967`.
   Option D is the only one that moves it, and moving it is an owner ruling.
2. **`AGENTS.md` §1 is absolute.** A measured phase pays for its own work from a *declared* cache state.
   Any option reading fixture pages inside the timer must declare that state, enforce it equally across
   arms, and never let a warm variant pass where a cold one fails. The row is `PreparedDewarmed` /
   `OpenedFromCopy` today with `g4.residency == 0`. **This is the condition that makes or breaks A.**
3. **The declared content is not negotiable.** `pipeline.content_bytes` 502,914,928 canonical for
   500,000,000 declared, `content_objects` 109,414, `inserted` 113,635 and the pinned root must survive.
4. **The budget.** Complete command ≤ 15 s; the row measured 6.57 s.
5. **Both boundaries stay published**: `operation_work_ns` **and** `accept_span_ns`, with
   `establishment_ns` and `teardown_ns`.

## 5. What the round must produce

1. **A pre-registration before the first locked run**, carrying the predicted bound as a number — e.g.
   *the lifetime peak falls to ≤ X MB and the measured-region increment stays within 20 % of
   37,519,360* — plus refutation conditions. `AGENTS.md` §3 and the round-21 pattern.
2. **Step 1 landed**, row `PASS`, fifteen pins reproducing.
3. **The step 2 design page**, with options priced and a recommendation, and the ruling request stated
   as a question for the owner rather than decided in the page.
4. **If a ruling is given and step 3 runs:** the row re-measured with the anchor, and **both figures
   published** — the lifetime peak *and* the measured-region increment. A row reporting only one cannot
   be read for this question.
5. **A report** with every number sourced, every non-passing line reported as plainly as a passing one,
   and the production LOC comparison for each commit (`tools/production_loc.py --root <tree>`; a
   harness-only commit reports the unchanged total and delta 0 and says so).

## 6. What must not be redone

| closed | why |
| --- | --- |
| **the v0.1.6-vs-replacement memory comparison** | the pair cannot be formed — the reference harness publishes no per-region figure for `init_namespace`. The one valid comparison is fixture residency (~500×), and it is a **harness** difference |
| **the product's measured-region memory** | 27.2 MB / 37.5 MB and 7–9 % of canonical bytes, falling per byte. It is bounded |
| **round 21's row, declaration, pins, anchor** | registered and measured; the declaration is the reference's and is not a lever |
| **the pack-capacity lever** | refuted on a matched pair and reverted (`8efc798de`, L80) |
| **the row's time boundary** | round 20 measured the exclusion of the save's connection close at 3.5 % of the closure at 10,000 entries and 10.5 % at 100,000, swinging 14× between sessions. Option D moves it; nothing else may |
| **the registry cardinality defect** | fixed in round 21 (`FROZEN_CARDINALITY`'s `pipeline.*` `4 → 6`); `runner.py self-check` passes |

## 7. Housekeeping

- Branch `codex/219-ns10000`; `a22e98bb7` is pushed and the tree is **clean**. Push before filing
  anything that cites a hash.
- Harness receipts are **not** tracked by git (`benchmark-results` is in `.git/info/exclude`), so any
  receipt a report depends on must be copied into the evidence directory. Round 21's `raw/` is the
  pattern.
- The measurement lock is **per worktree**. Never interrupt another owner's run.
- `AGENTS.md` §3: one sample per case per arm, fresh `--out` per run, `--verify full` for anything
  called evidence, never retune a receipt, report `FAIL` and `INCOMPLETE` as plainly as `PASS`.
- The harness **rebuilds by default**; `--no-build` is for iteration only, and a rebuilt binary
  invalidates its pair.
- `runner.py self-check` must pass before you call the tree verified. It does today.
- **No change to `core/crates/**` or `core/*/sql/**`.** If the fixture work shows a product problem,
  that is a finding and the next round's subject, not this round's edit.

## 8. The one-line version

The row's 1.05 GB is 96 % a fixture the harness holds in RAM where the reference harness streams the
same bytes through 1 MiB. **164 MB of it is redundant and can go now; 728 MB of it needs a design
decision, and the decision is whether the row may read its own fixture inside the timer.**
