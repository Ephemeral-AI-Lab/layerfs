# Decision — #226 step 2: a bounded fixture for `pipeline-namespace-100000`

> **Status:** design page. It changes no product line, runs no new measurement of its own, and makes no
> performance claim. It prices four options, recommends one, and **stops at a recommendation** — options
> A and D need an owner ruling and the ruling is asked for in §7, not taken here. Every number is
> sourced to a receipt, a counter or a `file:line`; the two figures that are arithmetic rather than
> measurement are labelled as such.
>
> Commissioned by [`issue226-bounded-fixture-handoff.md`](issue226-bounded-fixture-handoff.md) §2 step 2.
> Step 1 of that handoff — the redundant prefix snapshots — **landed** in this round and its numbers are
> in [§1](#1-where-the-fixture-stood-and-where-step-1-left-it); the round's report is
> [`evidence/issue226-ns22-boundedfixture-20260921T163314Z/report.md`](evidence/issue226-ns22-boundedfixture-20260921T163314Z/report.md).

## 1. Where the fixture stood, and where step 1 left it

Round 21 attributed the row's 1.05 GB lifetime peak and found 96 % of it held **before** the timer
(`evidence/issue219-ns21-100k-20260921T155500Z/report.md` §11):

| component | round 21 | after step 1 | `file:line` (this tree) |
| --- | ---: | ---: | --- |
| the content store — 503 MB of canonical objects held in RAM | ~728,465,408 | **unchanged** | `ops/pipeline.rs:737` (built), `:1124` (accepted) |
| the 25 per-batch prefix snapshots | 164,347,158 | **removed** → 3,786,440 | `ops/pipeline.rs:864`, `:924`; `workload/providers.rs:254`, `:299` |
| the prepared tree and the plan | ~55,394,304 | unchanged | `ops/pipeline.rs:687-720` |

**Step 1 is measured, not projected.** Three locked runs, `--verify full`, one sample, clean tree,
binary `d6c1d363c9f8471f…`, commit `47cf17049`:

| | round 21 (`ns21-E1`) | step 1 (`ns22-D2`) | change |
| --- | ---: | ---: | ---: |
| lifetime peak RSS (`resources.rss.process_peak_bytes`) | 1,050,738,688 | **885,325,824** | **−165,412,864 (−15.7 %)** |
| measured-region baseline (`pipeline.rss_phase_baseline_bytes`) | 1,013,219,328 | **848,396,288** | −164,823,040 |
| measured-region peak | 1,050,738,688 | 885,325,824 | −165,412,864 |
| measured-region **increment** | 37,519,360 | **36,929,536** | −1.6 % |
| `pipeline.prefix_keys_total` | — | 66,824 identities | new counter |
| `pipeline.operation_work_ns` | 3,549,393,833 | 3,640,415,749 | +2.6 % (registered band ±10 %) |
| all fifteen pins, both `pipeline.*` rows | reproduce | **reproduce** | — |
| session anchor `namespace-10000` | 68,951,625 (round 21's own anchor) | 77,814,875 | +12.9 % against it, within the ±20 % bound on the reference 68,514,625 |

**What that leaves.** The row still holds **~503 MB of constructed content inside the process before the
timer**, and that is now the whole of the fixture cost: the 885 MB peak is the ~728 MB content store
(this §'s first row) plus the ~55 MB tree and plan plus the 37 MB measured region. 500 MB of the 885 MB
is a fixture the row does not need to hold, and the reference harness does not hold it: it streams its
own `namespace-100000` scenario to disk through `NamespaceContentStream` over a
`NAMESPACE_SCRATCH_BYTES = 1 MiB` window (`benchmark/fs-bench-pro/workload/main.rs:120`, `:936-948`).

**The one thing that makes this hard is not memory.** It is that the product reads the fixture, so every
byte the row stops holding it must read back — and a read is only honest if its cache state is declared
and enforced. That is the whole of §4.

## 2. What is fixed, and is not a lever

| fixed | source |
| --- | --- |
| the declaration — 100,000 files / 1,000 directories / 500,000,000 B / 2 anchors | the reference harness's own `namespace-100000` scenario, ported field for field; shrinking it is **out of scope** (handoff §3) |
| construction stays **outside** the timer; the C1 tree build and the save stay **inside** it | `test_setup_and_cache_discipline.md` §2.2; the driver's own statement at `ops/pipeline.rs:1011-1048` |
| the measured content — `pipeline.content_bytes` 502,914,928 canonical, `content_objects` 109,414, `inserted` 113,635 | `tests/golden/expected.tsv`, pinned |
| `LAYERFS_CONSTRUCTION_WORKERS=1`, one worker | `AGENTS.md` §3.8 — the namespace **content** is not the `init_namespace` exception, and the harness exports 1 for every run (`runner.py:321-327`) |
| complete command ≤ 15 s; the row measures **6.477 s** | `AGENTS.md` §3.7; `ns22-D2` `run.json` |
| `core/crates/**` and `core/*/sql/**` unchanged | handoff §7 |

**A smaller declaration is not an option and is not priced below.** It would make the row cheap by
making it about a different fixture; the handoff rules it out and this page agrees.

## 3. The four options, priced

The prices below are the surfaces each option costs, not estimates of benefit. `[measured]` marks a
figure this round or round 21 took; `[arithmetic]` marks one computed from measured inputs, with the
inputs named.

### A. Spill the constructed objects, stream them back in a bounded window inside the timer

| | |
| --- | --- |
| **what changes** | the content store moves from heap to a scratch directory before the timer; inside it, a bounded reader streams the objects back |
| **memory** | becomes **O(window)**, not O(content): the ~728 MB row of §1 leaves the process. Peak is expected to land near the ~120 MB the tree, the plan, the store's own page cache and the measured region cost together `[arithmetic, from §1's components]` |
| **time** | the row gains a **disk read of 502,914,928 canonical bytes `[measured: pipeline.content_bytes`]** inside the declared figure, plus a second read for the oracle replay (which is untimed). The oracle already re-reads every object today, so A does not add a third pass — it changes where the two passes read from |
| **new work** | a **bounded** reader. `TreeStore::write_to_dir` and `load_from_dir` exist (`providers.rs:124`, `:143`) but `load_from_dir` loads the whole store, so it is a producer/serializer, not the reader this needs. The reader is new |
| **cache contract** | **required, and it is the condition that makes or breaks A.** See §4 |
| **what it buys** | the row stops being a 885 MB process whose fixture is 500 MB of it, and starts measuring what the reference harness measures: a namespace read from storage while it is built and saved. It is the only option that makes the row's fixture cost *structurally* bounded rather than bounded by constant factor |
| **ruling** | **yes** — a measured phase would read its own fixture inside the timer |

### B. Keep constructing in memory, but consume each object as it is accepted

| | |
| --- | --- |
| **what changes** | the content stream's `cloned_object` copy (`ops/pipeline.rs:1124`; `providers.rs:92`) is replaced by an owned drain, so the store's object moves into `accept` instead of being cloned into it |
| **memory** | halves **the part of the measured region that is the clone**, not the fixture. Round 21 put `heap.peak_incremental_bytes` for the measured region at **27,557,538 B `[measured]`**; the clone is the largest single term in it but the store itself — the 558,516,171 live heap bytes §1's first row is — is untouched, because it lives outside the timer and B does not move it |
| **time** | neutral to slightly better: one copy per object removed |
| **new work** | a draining path into `accept`. `Store::accept` takes `FinalizedObject` **by value** (`cas/store.rs:504`), so an owned drain is possible; `TreeStore` would need an `into_objects`-shaped API so a caller can hand the object over rather than copy it. The driver's content loop is the only caller |
| **what it buys** | roughly half of one component of a 37 MB region. It does **not** bound the fixture and it does not touch the 728 MB |
| **ruling** | **no** |

### C. Build the tree twice — once untimed into a Store that already holds the content, once timed

| | |
| --- | --- |
| **what changes** | the untimed pass saves the content into the prepared Store; the timed pass reads it back from there, so the harness never holds it |
| **memory** | **O(1)** for the fixture. The content lives in the Store, which is what a Store is for |
| **time** | the preparation doubles: the row's `construct_ns` + `construct_noise_ns` are 459,429,478 + 135,016,097 = **594,445,575 ns `[measured: ns22-D2`]**, and the C1 tree build is a further **1,137,483,625 ns `[measured: pipeline.span_build_ns`]**. C pays the C1 half twice, ~1.14 s more preparation. Both are **untimed** and the complete command is 6.477 s against a 15 s ceiling, so it fits |
| **cache contract** | the timed pass reads the content out of the **Store**, which is the thing the row already opens from a de-warmed copy: `PreparedDewarmed` / `OpenedFromCopy`, `g4.residency == 0`. The store's page-cache state is therefore already declared and already gated, and no new contract is needed |
| **new work** | a second tree build and a Save of the content in preparation. The registry row is `prepared = -` (`Preparation::InProcess`), so this happens inside the row's own invocation, twice — and the *first* pass's Store must be the sample the timer opens, which means the row's `c2::create_and_save_untimed` / `prepare_sample` sequence moves to after the first build |
| **what it buys** | the same bounded fixture as A, without a read the row has to declare |
| **price** | ~1.14 s of extra untimed work and the C1 tree build performed twice, to avoid a read A would have to declare anyway |
| **ruling** | **no** — but note that it is the cheapest *legal* option, and §5 recommends A over it for a stated reason rather than for a measured one |

### D. Construct inside the timer and declare the boundary change

| | |
| --- | --- |
| **what changes** | the declared figure starts including construction |
| **memory** | unchanged; the fixture is still held, just held while the clock runs |
| **time** | the declared figure gains ~594,445,575 ns of construction `[measured]` and the boundary no longer matches the one every existing figure for this row was taken under |
| **what it costs** | **every existing figure for this row is invalidated** and the row's bar needs re-reading |
| **ruling** | **yes** — and it is the *only* option that moves the timer's boundary, which `test_setup_and_cache_discipline.md` §2.2 and the handoff §4.1 both make an owner ruling rather than a harness decision |

## 4. The contract A would have to satisfy

A is legal only if all five of these hold. Four are satisfied by instruments that already exist; the
fifth is the one this page is really about.

1. **The cache state of every page the timed phase reads is declared.** The fixture's spilled pages are
   the new input, so the row's declaration gains them. The row's existing declaration is
   `PreparedDewarmed` / `OpenedFromCopy` and its `g4.residency` gate reads 0 resident pages today — but
   that gate is about the **sample Store** (`ops/pipeline.rs:1042`), not about a scratch directory that
   does not exist yet. The declaration has to name both.
2. **The state is enforced, not assumed.** `shared/residency.py:154` already implements the enforcement
   primitive: `msync(MS_INVALIDATE)` over an `mmap`, with `mincore` before and after
   (`de_warm` → `DeWarmReport`, `resident_after == 0`). It is applied to the row's scratch files before
   the timer, and the report is published on the row. A run whose `resident_after` is not 0 is
   `INELIGIBLE`, never quietly fast — `AGENTS.md` §1's fail-closed direction.
3. **The read is attested as a device read, not a cache read.** `mincore` cannot tell a cache-served
   read from a device read — that is the #151/L18 error — so the harness already carries the second
   instrument: `process_usage().disk_read_bytes` (`support/instruments.rs:346`) and
   `gates::device_attestation`, which requires `disk_read_bytes >= 0.9 x requested`
   (`gates.rs:382-405`). This round's row does not gate on it; A's row must, against the spilled
   fixture's own byte count. `ops/history.rs:2291` is the existing precedent for bracketing a chain
   with two `process_usage()` reads.
4. **Both arms are treated identically and neither is pooled with the other's counterpart.** The
   spill-then-read row is a **new** cache state for this family. `AGENTS.md` §1: "cold and warm rows
   MUST NOT be pooled". A's row is comparable with A's row and with `c1.fs.build-scale`'s pack-free
   anchor; it is **not** comparable with `ns22-D2`'s 885 MB / 3.64 s figures, which are a different
   cache state and a different cost structure. That is a reporting rule the round must carry, not a
   measurement.
5. **One question that is not this page's to answer, and is the reason A needs a ruling.** Under A the
   declared figure contains a read the harness performs *on the product's behalf*: `pipeline.accept_span_ns`
   would include 502,914,928 bytes read from disk to feed the operation. `AGENTS.md` §1's first sentence
   is "a measured phase pays for its own work from a **declared** cache state" — the read is paid for and
   the state is declared, so §1 is satisfied. But `operation_work_ns` is published as **this row's work
   figure**, and under A a term in it is harness storage traffic rather than product work. The honest
   form is to publish the split — `pipeline.spill_read_ns` beside `operation_work_ns`, the way
   `establishment_ns` and `teardown_ns` are already published beside it (`ops/pipeline.rs:1483-1488`) —
   and to say in the row's own note that the figure now includes it. **Whether that is an acceptable
   shape for this row's declared figure is the owner's call, and it is question 1 of §7.**

**What A is not.** It is not a product change. The product's `SaveHandoff` path, its `accept` and its
`finish` are untouched; the harness chooses to feed them from disk instead of from a `HashMap`. It is
also not a claim that the product's own memory improved — §1's third row shows the measured-region
increment is 36,929,536 B, 7.3 % of canonical bytes, and it was 7.4 % before step 1. **The product's
measured memory is bounded and is not this page's subject.**

## 5. Recommendation

**Recommend A, on the condition that §4's five points are met — with C named as the fallback if they are
not.**

The reason is not that A is cheaper; it is not. A costs a new bounded reader, a serializer round trip,
and the second declared input this page is asking the owner to accept. C costs none of that and none of
the read. A is preferred because **A is what the reference harness does**: its `namespace-100000`
scenario streams its content through a 1 MiB window (`workload/main.rs:120`, `:936-948`), so a row built
this way measures a namespace read from storage while it is built and saved — the thing the bar's case
is. C's row never reads its fixture at all; it reads a Store the harness filled in a previous pass, and
the fixture's cost moves into a Store the row already declares. Both are legal. **Only A leaves the
harness's fixture cost structurally bounded rather than moved.**

Two things this recommendation does **not** claim, because they were not measured:

* that A's peak lands near ~120 MB. The component arithmetic in §3A is `[arithmetic]`, and the row it
  predicts has never been run. The pre-registration for the round that builds it registers the bound
  before its first locked run, as this round's did;
* that A's read costs less in wall time than a session's own spread. §5.1 of round 21 measured a
  **3.38 %** within-session spread on the declared figure, and 502,914,928 bytes read at an unflagged
  rate is a term the page cannot price without measuring it.

**B is recommended as a follow-up, not as an alternative.** It is legal, it is small, and it makes the
measured region's largest harness term disappear; it bounds nothing. If the owner declines A and
declines C, B is what is left and the page should say plainly that the row then keeps a 500 MB fixture.

## 6. What is not claimed

* **No performance figure of its own.** Every number in §1 is from a locked run named beside it; §3's
  prices are surfaces (`file:line`) and measured inputs, and §3A's ~120 MB is labelled `[arithmetic]`.
* **No claim that A is faster or slower than today's row.** A's read is unpriced in wall time, and §5
  says why.
* **No claim that the reference harness's approach is portable as-is.** Its fixture is *generated* into
  the scratch window and never authenticated as a set of canonical objects; this row's fixture is
  109,414 canonical objects whose identities are pinned. A has to preserve
  `digest:filesystem_root 2412681d…fd954` and the fifteen pinned counters, and a streaming reader that
  changed the object set would move them. That is the acceptance condition for any round that builds A.
* **No claim about v0.1.6's memory on this axis.** The only defensible comparison remains fixture
  residency (~500×) and it remains a harness difference
  (`evidence/issue219-ns21-100k-20260921T155500Z/report.md` §11.5).
* **No claim that B's saving is 37.5 MB.** B removes the clone, not the region; the region's own
  `heap.peak_incremental_bytes` is 27,557,538 B `[measured]` and contains other terms.
* **No recommendation on the row's declared memory gate.** This page does not propose a limit, because
  a limit has to be derived from a paired control the same way the campaign's other limits were, and
  that control does not exist for a row that has never been run.

## 7. The ruling requested

Two questions. Both are the owner's; neither is taken here.

**Question 1 — may this row read its own fixture inside the timer?**
That is option A, and it is the same question the handoff §3 raises: *"whether the row may read its own
fixture inside the timer"*. Concretely: `pipeline-namespace-100000` would open a de-warmed scratch file
and stream 502,914,928 canonical bytes through a bounded window inside the measured closure, under the
contract of §4, publishing `pipeline.spill_read_ns` beside `pipeline.operation_work_ns` and gating the
read with `g4.residency == 0` and `g4.device-attestation >= 0.9 × requested`. The timer's boundary does
not move; **what the declared figure contains does.**

**Question 2 — if question 1 is declined, is option C acceptable?**
C bounds the fixture without a read the row must declare, at the price of building the tree twice untimed
(~1.14 s, inside the 15 s complete-command budget) and of a row whose content is supplied by a Store the
harness filled in a previous pass. **If both are declined, this page recommends B as the only remaining
legal step and records that the row keeps a ~500 MB in-process fixture** — which is a legitimate
outcome, and one a reader should then read the row's 885 MB peak with.

**Question 3 — the cache contract's wording, if question 1 is granted.**
The contract is a new declaration for this family. The name below is a **proposal**, not a decision:
`pipeline-spill-cold-v1`, "the fixture's scratch
pages are invalidated with `msync(MS_INVALIDATE)` and `mincore`-verified non-resident before the timed
sample; the timed phase reads them from the device and the read is gated at ≥ 0.9 × the spilled bytes."
The owner is asked to accept or amend the wording **before** a round builds it, because a contract
declared after the run that needs it is a description rather than a contract.

## 8. What must not be redone

| closed | why |
| --- | --- |
| **the v0.1.6-vs-replacement memory comparison** | the pair cannot be formed — the reference harness publishes no per-region figure for `init_namespace`. The one valid comparison is fixture residency (~500×) and it is a **harness** difference |
| **the product's measured-region memory** | 27.2 MB / 36.9 MB and 7–9 % of canonical bytes, falling per byte; bounded, and not this page's subject |
| **the redundant prefix snapshots** | removed and measured in this round; 164,347,158 B → 3,786,440 B, fifteen pins intact, §1 |
| **round 21's row, declaration, pins, anchor** | registered and measured; the declaration is the reference's and is not a lever |
| **the pack-capacity lever** | refuted on a matched pair and reverted (`8efc798de`, ledger L80) |
| **the row's time boundary** | round 20 measured the exclusion of the save's connection close at 3.5 % of the closure at 10,000 entries and 10.5 % at 100,000, swinging **8.8× between two runs in this round's own session** (§9). Only option D moves it and D is an owner ruling |
| **the registry cardinality defect** | fixed in round 21 (`FROZEN_CARDINALITY`'s `pipeline.*` `4 → 6`); `runner.py self-check` passes |
| **the release-mode `instruments_selfcheck` failure** | found and fixed in this round: the heap-window test's 4 MiB pattern was dead code with optimisations on, so `--release` read a peak of 0. One `std::hint::black_box` line; pre-existing at the parent commit |

## 9. Two measurements this round took that bear on the page

Neither is a claim about A; both are facts a reader of §3 and §4 needs.

**The save's connection close swings 8.8× inside one session, and the declared figure does not.** Three
runs of the same row on the same binary in one lock window (`ns22-C2`, `ns22-D2`, and the pre-edit
`ns22-B2`):

| run | `teardown_ns` | `span_finish_ns` | `span_finish_ns − teardown_ns` | `operation_work_ns` | `accept_span_ns` |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ns22-B2` | 60,190,625 | 72,939,000 | 12,748,375 | 3,642,802,292 | 3,702,992,917 |
| `ns22-C2` | 60,392,959 | 72,153,667 | 11,760,708 | 3,685,116,916 | 3,745,509,875 |
| `ns22-D2` | **528,019,917** | **540,065,458** | 12,045,541 | 3,640,415,749 | 4,168,435,666 |

The teardown moved **467,829,292 ns (8.77×)** while the accept span moved with it and the **declared
figure moved 44,701,457 ns (1.2 %)**. The difference `span_finish_ns − teardown_ns` is stable to 8.4 %
across all three, which is evidence — not proof, and labelled as evidence — that the connection close is
nested inside the finish span. **This is the strongest support the handoff's §3A argument has**: a row
that measures a fixture read inside its accept span is measuring a region whose own terms swing by
hundreds of milliseconds between runs, and A's contract is what keeps that from deciding a comparison.

**A comment-only edit changes the harness binary's hash.** `e386225e9371a374…` (the runs taken before a
six-line `#[allow(clippy::len_without_is_empty)]` doc-comment edit) and `d6c1d363c9f8471f…` (the
acceptance runs after it) are the same code. Nothing in this page depends on that, but every receipt
does: a run's binary hash is part of its identity, and a reader comparing two runs by hash is comparing
builds rather than sources.
