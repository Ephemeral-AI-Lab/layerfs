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
| **memory** | becomes **O(window)**, not O(content): the ~728 MB row of §1 leaves the process. Peak is expected to land near **~160 MB** `[arithmetic: 885,325,824 measured today, less the ~728,465,408 B content store of §1]`, plus a 1 MiB read window. The floor under any spill design is the ~55 MB tree and plan plus the ~37 MB measured region, which is why **10x is not reachable by this option** and ~5.6x is |
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

**The option as the handoff states it is underspecified in the one place that decides whether it works,
and this page corrects it.** "Build the tree twice" describes where the cost moves; it does not say
whether the content is *streamed* into the Store or built into a `TreeStore` and then saved. Those are
different options and only one of them bounds anything:

| | C1 — build the content store, then save it | C2 — stream each constructed object into the Store |
| --- | --- | --- |
| **how** | `content` is built as today (`ops/pipeline.rs:737`) and handed to `create_and_save_untimed` (`c2.rs:103`) instead of an empty store (`ops/pipeline.rs:800`) | the construction loop accepts each object into the save operation as it is constructed; no `TreeStore` ever holds the fixture |
| **peak** | **~728,465,408 B, unchanged** — and that is the whole point: *the fixture is held either way; only its position relative to the timer moves.* A lifetime peak is a high-water mark, so moving 503 MB out of the timer does **not** lower it | **O(1)**: one object plus the Store's own cache at a time. The ~728 MB row of §1 leaves the process |
| **new work** | one argument changed | a streaming accept path: the construction loop's consumer becomes the operation rather than a `TreeStore` |

**C1 is not a fix and must not be recommended as one.** It is recorded here because it is what a
mechanical reading of the handoff's wording produces, and because a round that built it would report a
lower *measured-region* figure while the process still peaked at 885 MB.

**And C2's price is a cold read, not the absence of one.** The page's first draft claimed C "needs no new
contract because the store's page-cache state is already declared and already gated". **That claim is
false as written and is retracted here.** `prepare_sample` makes an independent byte copy of the base and
de-warms **the whole copy** (`c2.rs:132-146`, `instruments::de_warm` at `support/instruments.rs:674`),
and `dirty`'s `g4.residency` gate reads 0 resident pages on the sample. So a timed pass that reads its
content out of that sample reads it **cold** — the same physical work as A, at the same
502,914,928 canonical bytes plus the Store's own framing. What C2 avoids is not the read; it is the
**spill file and its contract**.

**Where the two genuinely differ**, then:

| | A (spill + bounded reader) | C2 (stream into the Store) |
| --- | --- | --- |
| the input the timed phase reads | a harness scratch file, **new** | the sample Store, which every C2 row already declares, copies, de-warms and gates |
| contract surface | a new declaration plus `g4.device-attestation` against the spilled bytes | the row's existing `PreparedDewarmed` / `OpenedFromCopy` and `g4.residency`, already published |
| new mechanism | a bounded reader (**new**), a scratch-directory lifecycle in the row's output | none: an existing writer and an existing reader |
| what the row's declared content is drawn from | the 502,914,928 canonical bytes, exactly | the Store's contents, which carry packs, indexes and framing beyond the canonical bytes |
| fidelity to the reference harness | high — the reference streams its own scenario through a 1 MiB window | lower — the reference reads a prepared Store for its other cases but streams for this one |

| | |
| --- | --- |
| **ruling** | **no** for either — but C2 is a bigger change than "no ruling" suggests, see §5 |

### D. Construct inside the timer and declare the boundary change

| | |
| --- | --- |
| **what changes** | the declared figure starts including construction |
| **memory** | unchanged; the fixture is still held, just held while the clock runs |
| **time** | the declared figure gains ~594,445,575 ns of construction `[measured]` and the boundary no longer matches the one every existing figure for this row was taken under |
| **what it costs** | **every existing figure for this row is invalidated** and the row's bar needs re-reading |
| **ruling** | **yes** — and it is the *only* option that moves the timer's boundary, which `test_setup_and_cache_discipline.md` §2.2 and the handoff §4.1 both make an owner ruling rather than a harness decision |

### What each option does to the harness's own complexity

Priced here because §5's recommendation costs it, and a page that recommends a design without saying what
it does to the code is not a design page. Counted in **mechanism**, not in diff size.

| option | new mechanism | published surface added | mechanism removed |
| --- | --- | --- | --- |
| **A** | **a bounded reader** — `write_to_dir` / `load_from_dir` exist (`providers.rs:124`, `:143`) but `load_from_dir` loads the whole store, so the reader is new work — plus a serializer round trip and a scratch directory the row must create, de-warm and clean up | a new cache declaration, a `g4.device-attestation` gate against the spilled bytes, and `spill_read_ns` beside `operation_work_ns` | the in-memory content store |
| **B** | none — one draining path into `accept`, which already takes the object by value (`cas/store.rs:504`) | none | **the `cloned_object` copy** |
| **C2** | none — the Store's existing writer, the existing `prepare_sample` copy-and-de-warm, the existing `g4.residency` gate | none | the in-memory content store |
| **C1** | none | none | **nothing** — it bounds no memory at all |
| **D** | none | the boundary itself moves, and every existing figure for the row is invalidated | the fixture's exclusion from the timer |

**A makes the harness more complex. C2 does not, and B simplifies it.** A's complexity is the price of
bounding an in-process fixture; a row that declines to pay it keeps 885 MB, correctly attributed. **No
option on this page lowers both the peak and the harness's complexity**, and that is a fact about the
problem rather than about the options: the fixture has to live somewhere, and "somewhere" is either the
heap, a scratch file with a contract, or a Store that already has one.

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

**Recommend A, on the condition that §4's five points are met — with C2, and not C1, named as the
fallback if they are not.**

The reason is not that A is cheaper; it is not. A costs a new bounded reader, a serializer round trip,
and the second declared input this page is asking the owner to accept. C costs none of that and none of
the read. A is preferred because **A is what the reference harness does**: its `namespace-100000`
scenario streams its content through a 1 MiB window (`workload/main.rs:120`, `:936-948`), so a row built
this way measures a namespace read from storage while it is built and saved — the thing the bar's case
is. C's row never reads its fixture at all; it reads a Store the harness filled in a previous pass, and
the fixture's cost moves into a Store the row already declares. Both are legal. **Only A leaves the
harness's fixture cost structurally bounded rather than moved.**

Two things this recommendation does **not** claim, because they were not measured:

* that A's peak lands near ~160 MB. The component arithmetic in §3A is `[arithmetic]`, and the row it
  predicts has never been run. The pre-registration for the round that builds it registers the bound
  before its first locked run, as this round's did;
* that A's read costs less in wall time than a session's own spread. §5.1 of round 21 measured a
  **3.38 %** within-session spread on the declared figure, and 502,914,928 bytes read at an unflagged
  rate is a term the page cannot price without measuring it.

**B is recommended as a follow-up, not as an alternative.** It is legal, it is small, and it makes the
measured region's largest harness term disappear; it bounds nothing. If the owner declines A and
declines C2, B is what is left and the page should say plainly that the row then keeps a 500 MB fixture.

**The honest cost of the recommendation, stated because it is a design decision and not a free one.**
A is the option that bounds the fixture *and* the option that adds the most mechanism: a bounded reader,
a serializer round trip, a new cache declaration, a new gate, a new published term, and a scratch
directory that becomes part of the row's output and its cleanup. **It makes the harness more complex, not
less.** Two things keep that price honest rather than dismissed:

* the row's fixture is **harness** memory, 96 % of it held outside the timer (§1). Nothing about the
  product becomes simpler or more complex under A; what changes is how much the harness holds.
* the option that strictly *simplifies* is **B** — it removes a copy and adds nothing — and it bounds
  nothing. **There is no option on this page that both lowers the peak and lowers the harness's
  complexity**, and a reader should not expect one. A's complexity is the price of bounding an in-process
  fixture; a row that does not pay it keeps 885 MB, correctly attributed, which is what today's row does.

## 5a. A's price, measured after this page was written

The page's §5 said *"A's read is unpriced in wall time"* and §6 listed that as a non-claim. It is priced
now, by `tests/spill_read_price.rs` (registered in
[`evidence/issue226-ns22b-spillprice-20260921T171500Z/`](evidence/issue226-ns22b-spillprice-20260921T171500Z/pre-registration.md)),
on the row's own fixture — 109,373 objects, 502,912,427 canonical bytes. Every figure is a share of the
row's declared figure, 3,549,393,833 ns, and every one is a share of the **whole** cold read, hashing
included:

| reader shape | time | share |
| --- | ---: | ---: |
| one file per object (`TreeStore::write_to_dir` as it stands) | 15,326,877,666 ns | **432 %** |
| `mmap`, one fault per object | 1,007,319,334 ns | **28.4 %** |
| positioned read per object into a reused buffer | 445,381,375 ns | **12.6 %** |
| streamed 1 MiB windows, splitting objects out and hashing each | **347,960,667 ns** | **9.8 %** |
| hash only, no read at all | 372,680,083 ns | **10.5 %** |

**Read together, these four facts decide A's price, and it is not the 12 % the page first implied nor the
1–2 % it hoped for.**

1. **The bytes are nearly free; the objects are not.** A cold read of the whole fixture differs from a
   warm read of it by **2.5 ms** (426,732,834 against 429,199,791 ns), and the device reads all 502 MB.
   What costs is per-object work: hashing alone is 10.5 % of the figure, so any reader that hands the
   Store authenticated objects is bounded below by roughly that.
2. **The reader must stream, not seek, and must not `mmap`.** Streaming windows cost 9.8 % against 12.6 %
   for positioned per-object reads and 28.4 % for `mmap` — a cold mapping pays a page fault per object.
3. **`load_from_dir`'s shape is unusable as it stands.** One file per object costs 432 % of the figure to
   read back. **A's spill must be one pack with an entry table**, and that is the new work the page's §3A
   priced as "a bounded reader".
4. **A's honest price is therefore ~10 % of the declared figure, not 1–2 %.** The page's §5 compared A
   against today's row, which reads the fixture from a `HashMap`; the read it removes is 79,174,500 ns of
   deep copy (2.23 %, measured, and removed by option B), while the read it adds is 348–450 ms. **A trades
   roughly 10–12 % of the row's figure for 5.6× less peak memory**, and the trade is real rather than
   marginal.

**What that does not change.** The ruling question in §7 stands as asked and is unchanged by the price:
whether this row may read its own fixture inside the timer, under a declared and gated cache contract.
What changes is the number the owner is accepting — **≈ 350 ms and 10 % of the declared figure, against
885 MB → ~160 MB of peak** — and the page says so rather than leaving the reader with the earlier
estimate. The owner has accepted a *minor* degradation on this basis; the round that builds A must
re-register the bound before its first locked run and is free to find the price higher or lower than
these four readings, which are one fixture on one host.

## 6. What is not claimed

* **No performance figure of its own.** Every number in §1 is from a locked run named beside it; §3's
  prices are surfaces (`file:line`) and measured inputs, and §3A's ~120 MB is labelled `[arithmetic]`.
* **No claim that A is speed-neutral.** §5a measures the read at 9.8–12.6 % of the declared figure
  depending on the reader shape, against the 2.23 % of deep copy it replaces. The page's own earlier
  estimate of 1–2 % was wrong and is corrected there.
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

**Question 2 — if question 1 is declined, is option C2 acceptable?**
C2 streams each constructed object into the Store untimed and reads them back cold inside the timer. It
bounds the fixture at the price of a bigger untimed write and of a row whose 502,914,928 declared
canonical bytes are drawn from a Store that also carries packs, indexes and framing. It needs no new
contract — `prepare_sample` already copies and de-warms, and `g4.residency` already gates — but it is a
larger change than the handoff's one-line description suggests, because "build the tree twice" as written
(C1) bounds nothing at all (§3C). **If both are declined, this page recommends B as the only remaining
legal step and records that the row keeps a ~500 MB in-process fixture** — which is a legitimate outcome,
and one a reader should then read the row's 885 MB peak with.

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
