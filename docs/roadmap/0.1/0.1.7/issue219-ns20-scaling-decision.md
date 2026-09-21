# Decision — #219 round 20, step 3: `pipeline-namespace-100000`, and the case mapping that was missing

> **Status:** decision page. It changes no product line, runs nothing, and claims no
> performance figure of its own. Every number is sourced to a receipt, a counter or a
> `file:line`. Commissioned by
> [`issue219-ns19-packlimit-and-scaling-handoff.md`](issue219-ns19-packlimit-and-scaling-handoff.md)
> section 5, and it answers the two things that step left open: whether to build a
> 100,000-entry pipeline row, and the case mapping that correction 4 says has never
> been written down.
>
> **Section 3's decision was taken back by the owner the same day — see §0.** The row
> is commissioned by [`issue219-ns20-pipeline-100k-handoff.md`](issue219-ns20-pipeline-100k-handoff.md);
> §1, §2 and §4 still stand and are what the new row is not evidence for.

## 0. Owner ruling, 2026-09-21: the rung is built anyway

**Section 3's decision has been taken back by the owner.** It registered its own reversal conditions,
and condition 3 was *"an owner ruling that the pipeline family needs a second namespace-scale point for
a reason outside this campaign's gap"* — that ruling is given: the row is commissioned. The commission
is [`issue219-ns20-pipeline-100k-handoff.md`](issue219-ns20-pipeline-100k-handoff.md), which carries the
surfaces the row costs, the two hard constraints that make the obvious declaration wrong, and the
declaration itself: the reference harness registers **100,000 files / 500,000,000 B / 1,000
directories / 2 anchors** with a scaled band mix, so the row is not a total to invent but a port to
make — and the core harness's port currently produces a **different fixture at the same total**
(97,899 tiny files, one 100 MB anchor).

**Conditions 1 and 2 are not met and are not waived.** There is still no session control, so a
100,000-entry row's number cannot yet be compared with the 10,000-entry row it extends; the handoff's
section 5 is the cheap answer and the next agent is asked to do it **before the first timed run** rather
than after. And no measured non-proportionality in the save path has been found — the row is built on
the owner's judgement that a second point is worth having, which is a legitimate reason and is recorded
here as the reason rather than as a measurement.

**Condition 1 was met on 2026-09-21 by round 21.** The row was built, pinned and measured, and the
`c1.fs.build-scale` `namespace-10000` anchor was run **locked, in the same session and the same lock
window**, landing at **68,951,625 ns against the reference 68,514,625 — +0.64 %**, inside the ±20 %
bound the round registered before its first locked run. Condition 1's own wording is *"a pack-free
anchor row measured in the same session as every admission row, published beside it"*; this round
satisfies it **for its own session** and for nothing else — the anchor must be re-run beside any future
row that wants to be compared with these. Condition 2 (a measured non-proportionality in the save path)
was **not** sought and is **not** claimed; the round did find one in the row's C1 half, which is the
driver's handoff rather than the save, and it is filed as the next round's subject.
[`evidence/issue219-ns21-100k-20260921T155500Z/report.md`](evidence/issue219-ns21-100k-20260921T155500Z/report.md)
§4, §5a and §6.1 carry the numbers and the boundary of each claim.

Everything else on this page stands: the case mapping (§2) is what the new row is and is not evidence
for, and the handoff's price for the row corrects two errors in the page that commissioned this round.
§3's decision is superseded by §0's ruling and is left as written, because a decision page records what
was decided and when rather than what happened afterwards.

## 1. First, a correction to the handoff: S0 was written, and it *is* the pairing page

Correction 4 says the plan's S0 was *"a written decision on pairing feasibility"* and that
*"S1–S3 were never executed; the campaign went straight to optimising"*. **Both halves are wrong**, and
the record is on disk:

| stage | deliverable | what it settles |
| --- | --- | --- |
| **S0** | [`evidence/issue219-s0-custody-20260921T031259Z/README.md`](evidence/issue219-s0-custody-20260921T031259Z/README.md), 205 lines, + `inventory.py` / `inventory.tsv` | custody of all eight `namespace-10000` rows, mechanics 2a and 2b confirmed by reading the code, and **§3 "Can a v0.1.6 reference row be produced? — the pairing decision"** |
| **S1** | [`evidence/issue219-s1-reproduction-20260921T031259Z/README.md`](evidence/issue219-s1-reproduction-20260921T031259Z/README.md) | the bar's first part, measured on a clean sealed build: **NOT MET** |
| **S2** | [`evidence/issue219-s2-attribution-20260921T031259Z/README.md`](evidence/issue219-s2-attribution-20260921T031259Z/README.md) | anchor vs small files, phases, host vs container |
| **S3** | — | **blocked by S0 §3**, which is a decision and not an omission |
| **S5** | [`evidence/issue219-s5-review-20260921T031259Z/`](evidence/issue219-s5-review-20260921T031259Z/README.md) | an independent re-derivation of every S0/S1/S2 headline from raw receipts |

S0's §3 is the pairing decision, and it is stronger than "infeasible": it measured that **v0.1.6 and
`HEAD` compile the same reference product** — 216 production files under `crates/` with zero content
differences, and `git diff --stat v0.1.6..HEAD -- 'crates/*/src/**'` empty — so a v0.1.6 arm would be a
second measurement of the same code, and the pair would report build noise rather than a version
difference. It also confirmed mechanic 2a (`--source-arm baseline` selects nothing on
`init_namespace`'s performance path) and 2b (`namespace-10000` has no cold contract, so its
`cache_contract` is `null` by construction). **S3 was not skipped; it was decided.**

### What genuinely was missing, and is written here

S0 maps the bar's case to its own receipts. What no page records is the relation between **the bar's
case** and **this campaign's row** — and they are not the same case. That mapping is section 2.

## 2. The case mapping, written down

Three different registered rows answer to the name `namespace-10000`, in two harnesses:

| | the approved bar's case | this campaign's row | the core harness's own |
| --- | --- | --- | --- |
| case id | `namespace-10000` | `pipeline-namespace-10000` | `namespace-10000` (and `-text-v1`) |
| harness | `benchmark/fs-bench-pro/` | `core/benchmark/fs-bench-pro-storage-content/` | `core/benchmark/fs-bench-pro-storage-content/` |
| family | `families/init_namespace/` | `families/pipeline.rs`, group `pipeline.*` | `families/c1_fs_build.rs`, `c1.fs.build-scale` |
| route / timer | `namespace` / `layerstack_init_ns` | `pipeline:namespacescale` / `pipeline.operation_work_ns` | `fs-build:binary` / the row's operation |
| setup / store | `fresh-output` | `prepared-dewarmed` / `opened-from-copy` | `warm-in-process-fixture`, **no Store at all** |
| what it does | initialise the namespace and its bytes | build the tree **and** save 300 MB of content | build the tree, save nothing |
| entries / bytes | 10,000 files / 300 MB + a 100 MB anchor | 10,000 files / 100 directories / 300,000,000 B including the anchor | 10,000 files / 100 directories |
| declared cache | `cache_contract: null` (S0 §2b — the harness has no contract for it) | `prepared-dewarmed`, gated by `g4.residency == 0` | `warm-in-process-fixture` |
| state of the row | 402.721 / 407.598 / **578.245** / 928.022 / 1020.422 / 1100.711 ms, all `verification_status: NOT_RUN`, the three fastest `SOURCE_DIRTY=true` and reading **0.0 MB** from storage (S0 §1a, §1b) | `PASS` 13/13, one sample, `--verify full` | `PASS`, one sample |

**Consequences, stated plainly.**

1. **The bar is not evaluated by this campaign's row.** The bar is
   `namespace-10000` **≤ 578.245 ms on `layerstack_init_ns`, with a declared cache contract and a
   passing verification** — and per S0 the row carrying that figure is a dirty tree, serves all 300 MB
   from the page cache, and is `NOT_RUN`. S1 measured the bar's own case on a clean sealed build and
   recorded it **NOT MET**. Nothing this campaign has measured under the name
   `pipeline-namespace-10000` moves that number, in either direction.
2. **The campaign's row is an analogue, not a reproduction.** Its timer excludes the construction that
   the bar's timer includes and includes a Store the bar's case does not write
   (`pipeline.rs:1159-1164` says exactly this). Comparing the two figures is a category error and the
   campaign has not made it.
3. **The one comparison that is defensible is a boundary-matched *work* comparison**, and it is labelled
   as a shape comparison rather than a pairing: round 17's boundary is
   `operation_work_ns + construct_ns + construct_noise_ns`, which is the closest thing the two timers
   can share. That is what round 19's `−6.11 % work / −5.76 % CPU` statement is, and it is bounded by
   **this round's own finding** about session spread (section 4).
4. **`namespace-10000`'s `cache_contract` is `null` in the reference harness** (S0 §2b: `cold.applies()`
   requires `case == "namespace-100000"`). The bar's part 1 asks for a declared cache contract on a
   case the harness cannot declare one for. That is a bar defect, not a measurement defect, and it is
   an owner ruling rather than a harness change.

## 3. Decision: `pipeline-namespace-100000` is not worth building now

**There is no `pipeline-namespace-100000`.** The pipeline family registers five rows and exactly one is
namespace-scale. Adding a 100,000-entry row is a harness round, and the commission's own condition for
it — *"decide it after step 2 says whether the scaling is benign; if the C1 ladder is flat, a pipeline
rung buys less"* — is met: **the ladder is flat.**

### What the ladder already answered, so the new row would not buy it

The C1 build's cost per binding is **6,784 → 6,591 ns, −2.8 %** from 10,000 to 100,000 entries
([ladder report](evidence/issue219-ns20-ladder-20260921T113500Z/report.md), four `PASS` rungs). The C1
half of this row is `span_build_ns` = 68,377,625 ns of a 1,126,731,417 ns row, and the ladder says that
half is proportional to the entry count with a single structural step where the tree stops fitting
`WALK_CEILING` — a step the 100,000-entry pipeline row would pay once, at 25 operations, not per entry.

### What it would still buy, and what that is worth

The half the ladder does **not** cover is the save: at 100,000 entries the same declaration would carry
1.67× the bytes, and the save path's terms are proportional to content by construction (`pack_bodies`
is `packs × PACK_LIMIT`, `page_count` is the reserve over 4,096, `diag_commit_total_ns` prices the pack
pages). A second point would test that, and it would test the packet's combined behaviour at 10× the
entries. That is real but it is **linearity of a structurally proportional path**, and it is not where
this campaign's problem is.

### Why not now: the round's own measurement says a bigger row buys an uncomparable number

The matched pair in this round measured that **the same product, the same harness driver and the same
case, run five hours apart on this machine, differ by 183,412,999 ns on the row's declared figure**
(1,126,731,417 against 943,318,416), with the work no lever can touch moving 26.8 %
([pair report](evidence/issue219-ns20-pair-20260921T115000Z/report.md)). Every lever this campaign has
priced is 20–90 ms. **A new, larger row would produce one more number that cannot be compared with any
number taken in another session**, which is a worse return than the round it would cost. The
measurement problem comes first.

### The registered conditions under which this reverses

The decision reverses if **any** of these holds:

1. a session-control is in place — a pack-free anchor row measured in the same session as every
   admission row, published beside it, so two rows can be compared at all;
2. the save path's cost per canonical byte is shown to move at another scale, on any row, by
   measurement rather than by the proportionality argument above;
3. an owner rules that the pipeline family needs a second namespace-scale point for a reason outside
   this campaign's gap, such as release evidence for a claim about 100,000-entry namespaces.

### A correction to the handoff's price for it

The handoff costs the row as *"a new case declaration, a 600 MB fixture (500 MB + the 100 MB anchor), a
new prepared artifact and new pins"*. **"A new prepared artifact" is wrong for this family**: the
registry row is `prepared = -` — `Preparation::InProcess`, the default `CaseSpec::new` sets
(`families/mod.rs:134`) and this family never overrides (`tests/golden/registry.tsv:219`) — so the
pipeline case builds its fixture inside its own invocation (`pipeline.rs:631-637`) and `runner.py`
acquires no master for it. The `600 MB` is also an inference rather than a fact: this family declares a **total**
(`NAMESPACE_SCALE_BYTES = 300_000_000`, `pipeline.rs:86`) with the anchor inside it
(`namespace_content.rs:48-51`, `:195`), so a 100,000-entry row's total is a declaration to be made, not
one to be copied from the reference harness's `300 MB + 100 MB anchor`. The real price is a
`PipelineOp` variant, its `Configuration`, a family entry, a fresh pin set, and the run — **cheaper
than the handoff says, and still not worth it for the reason above.**

## 4. What this page hands on

1. **A session control is the next measurement need, ahead of any new row or lever.** The campaign's
   rows are comparable within a session and not across one, and no receipt says so.
2. **The row's declared boundary excludes the save's connection close**, and round 20 measured a change
   that moved 85.61 ms of work into it. Either the close comes inside the boundary or a change that
   moves work there will keep reading as a win.
3. **The bar's case and this campaign's row are different cases**, per section 2, and the bar's own
   case is `NOT MET` on a clean build (S1) with a `cache_contract` the reference harness cannot declare
   (S0 §2b). Any future statement that the bar has been met must name which of the three rows it means.

## What is not claimed

No performance figure of its own. No claim that a 100,000-entry pipeline row would be slow, fast,
linear or otherwise: it was not built and it was not run, and the proportionality argument above is an
argument, not a measurement. No claim that S1's `NOT MET` is final for the bar's case — it is one
session's clean sealed build and, per section 4.1, this campaign cannot currently compare it to any
other session's.
