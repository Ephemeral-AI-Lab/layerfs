# Pre-registration — #219 round 21, `pipeline-namespace-100000` and its session anchor

Written **before the first locked run of this round** (no `runner.py perf` had been invoked from this
worktree when this file was written). One unlocked diagnostic probe of the new binary, run directly
against a scratch `--out` under `/tmp`, had already been taken; it is declared in §4 and every
prediction below says which numbers came from it and which came from the control row.

## 1. What this round builds

One row, `pipeline-namespace-100000`, in the core harness's `pipeline.*` group. It is the
`pipeline-namespace-10000` shape at ten times the entries, and it is commissioned by
[`issue219-ns20-pipeline-100k-handoff.md`](../../issue219-ns20-pipeline-100k-handoff.md) under the
owner ruling in [`issue219-ns20-scaling-decision.md`](../../issue219-ns20-scaling-decision.md) §0.

**No product line changes.** `core/crates/**` and `core/*/sql/**` are byte-identical to `070305949`.

### 1a. The declaration decision, registered

The handoff (§4) put one decision on the row's builder: port the reference harness's own
`namespace-100000` declaration, or keep the core port's fallback and say the row measures entry
scaling at a different band mix.

**Registered: port it.** `namespace_content::Declaration::LARGE` is the reference's scenario field for
field — 100,000 files, 1,000 directories, 500,000,000 decimal bytes, **two** 100,000,000-byte anchors,
and the band mix `1,000 / 78,998 / 15,000 / 5,000 / 2`. The alternative would have produced 97,899
tiny files and one anchor at the same entry count, which is a different fixture and therefore a
different row.

**Consequence, registered because it changes what the row is comparable to:** the reference's large
declaration carries **1.67x** the bytes of its small one, not 10x, so the two rows are two declared
scaling points at **different totals** rather than one shape at two sizes. Every ratio below is
therefore read against the byte ratio as well as the entry ratio.

## 2. The anchor, and the rule that uses it

The C1 row `namespace-10000` in `c1.fs.build-scale` builds a tree and writes **no Store at all**, so no
product lever can reach it and it is this campaign's pack-free session control. Its recorded value is
`phases.operation_ns` **68,514,625 ns**, from `ns20-L1-10000-20260921T113500Z`.

**Registered:** this round runs it **locked, in this session, under the same lock window as the new
row**, and publishes its `operation_ns` beside the new row's counters.

**Registered rule:** if this session's anchor lands outside **±20 %** of 68,514,625 (i.e. outside
54.8–82.2 ms), the session is **not comparable** with the sessions the 10,000-entry figures came from,
and this round reports the new row's numbers as within-session only rather than as a scaling claim.
Round 20 measured a 183.4 ms spread between two sessions of the *same* case with the same product, and
every lever this campaign has priced is 20–90 ms, so a session-control of ±20 % is the loosest bound
that is still tighter than the effect it is meant to permit a comparison of.

## 3. Registered predictions

All ratios are against the 10,000-entry row's own control session
(`ns20-P2a-control-20260921T115000Z`), whose identifiers are the same product binary as this round's
harness for the product half.

| quantity | registered prediction | why |
| --- | --- | --- |
| `pipeline.declared_files` / `declared_directories` | 100,000 / **1,000** | the declaration; 1,000 not 100, because the plan's index space wraps otherwise |
| `pipeline.declared_content_bytes` | 500,000,000 | the reference scenario's total |
| anchors ≥ 100,000,000 bytes | **2** of exactly 100,000,000 | per-anchor bytes, multiplied by the count |
| `pipeline.bindings` | 101,000 | 100,000 files + 1,000 directories + 1 root |
| `pipeline.batches` | 25 (= ⌈101,000 / 4,096⌉) | `MAXIMUM_WALK_ENTRIES` = 4,096 |
| `pipeline.operation_work_ns`    | **within 1.10x of 3.0–3.5 s** | the probe, §4 |
| save cost per canonical byte | **flat within 15 %** of the 10,000-entry row's 3.13 µs/B | the row's point: the save path is proportional to content by construction, and this tests it at 1.67x the bytes |
| `pipeline.construct_ns` + `construct_noise_ns` | **5–10x** the 10,000-entry row's 404,782,924 ns | untimed construction scales with **file count** (4.05 s at 100,000) |
| `pipeline.span_build_ns` (C1 tree build) | **5–10x** the 10,000-entry row's 67,551,417 ns | the C1 cost per binding is flat at −2.8 % from 10k to 100k (L81); this row pays 10x the bindings |
| complete command | **≤ 15 s**, so no `DECLARED_EXCEPTIONS` entry | `preparation` scales with file count, the timer with bytes plus entries |
| status | **PASS**, all gates, one sample, `--verify full` | — |

**Refuted if any of:** the row exceeds the 15 s complete-command budget; a gate fails; the session
anchor lands outside §2's bound (the run is then reported as within-session only, not as refuted data);
the save's cost per canonical byte moves by more than 15 % in the direction of *sub*-linearity while
`content_bytes` and `commits` say the work was performed; or the 10,000-entry row's own pinned run —
which this round also takes, as the check that the generalised plan did not move the old fixture —
fails a pinned counter.

## 4. The declared diagnostic, and what it is not

Before this file was written, one **unlocked** invocation of the new binary was run for a compile-and-
wall-clock check:

```text
fs-bench-storage-content --case pipeline-namespace-100000 --out /tmp/ns100k-probe-<pid>
pipeline-namespace-100000 PASS gates=11 trace_bytes=27390
```

It is a **diagnostic**: it was not run through `runner.py`, holds no lock, wrote no receipt under
`benchmark-results`, is not `--verify full`, and its `--out` was deleted. It is not evidence and it is
not a sample. What it is used for here is the one thing a diagnostic may be used for — telling the
round's author roughly where the wall clock lands before registering a prediction, which is why §3's
operation prediction says "the probe" and §3's structural predictions do not.

Its published counters were: `batches` 25, `bindings` 101,000, `chain_objects` 4,221,
`content_objects` 109,414, `content_bytes` 502,914,928, `commits` 388, `statements` 12,206,
`packs_created` 2,180, `pack_appends` 12,088, `operation_work_ns` 3,410,265,459,
`accept_span_ns` 3,470,198,667, `construct_ns` 446,947,022, `construct_noise_ns` 137,023,701,
`span_build_ns` 1,048,070,042, `pack_bytes_written` 509,752,317.

**The ordering defect is recorded rather than hidden:** §3's structural predictions were derivable from
the source before the probe and were, but the operation-time band was not, and a prediction written
after a probe is a weaker instrument than one written before it. §3 says which is which.

## 5. Pins

**Registered: bootstrap from this round's own `PASS` run**, and pin the same set the 10,000-entry row
pins today (15 counters + 2 identity digests), **not** `pipeline.packs_created` or
`pipeline.pack_appends`.

Reason: those two are published but unpinned on the 10,000-entry row, and they are the only counters in
the row that a *policy constant* can move without moving any work — L80 measured them at 1,270 / 6,603
at 256 KiB packs and 295 / 7,578 at 1 MiB. Pinning them would make the row's pin set a statement about
the pack policy as well as about its work, and a future round that deliberately changes the pack policy
would have to re-pin a row it did not intend to touch. Leaving them unpinned keeps the pins to work no
policy constant can move; the price, stated plainly, is that **the row's pins would not have caught
round 20's arm** — that arm's movement was in exactly these two counters and in wall-clock terms that
are never pinned. §7 of the round report carries this as a finding rather than a defect.
