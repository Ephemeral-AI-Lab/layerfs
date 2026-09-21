# Report — #226 step 1b: the content stream's copy, and a refuted prediction

Pre-registration: [`pre-registration-step-b.md`](pre-registration-step-b.md), written before the first
locked run of this step. Raw receipts under [`raw/`](raw/). One sample per case, one lock window,
`--verify full`, fresh `--out` per run, clean tree (`source_dirty: false`), binary `5653d931a38e…`,
commit `2f9f102fb`.

## 1. The mechanism is confirmed; the declared figure refutes the prediction

`cloned_object` is `HashMap::get(...).cloned()` — a deep copy of every canonical object **inside the
timer**. `Store::accept` takes the object by value, so the copy bought nothing. `TreeStore::drain` moves
the objects out once, `g6.content-complete` requires the offered count to equal the constructed count,
and the copy is gone.

| | `ns22-D2` (before) | `ns22-E2` (after) | change |
| --- | ---: | ---: | ---: |
| **`pipeline.accept_span_ns`** — the region the copy lived in | 4,168,435,666 | **4,070,293,292** | **−98,142,374 (−2.35 %)** |
| `pipeline.operation_work_ns` — the **declared** figure | 3,640,415,749 | 3,662,759,417 | **+22,343,668 (+0.61 %)** |
| `pipeline.teardown_ns` — excluded from the declared figure | 528,019,917 | 407,533,875 | −120,486,042 (−22.8 %) |
| `pipeline.span_content_ns` — the content accept loop | 2,490,886,583 | 2,494,001,208 | +0.13 % |
| all fifteen pins, both rows | reproduce | **reproduce** | — |
| `pipeline.content_objects_offered` | — | 109,414 = `content_objects` | gated by `g6.content-complete` |

**The independent measurement said the copy cost 79,174,500 ns (2.23 % of the declared figure).** The
accept span fell by 98,142,374 ns (2.35 % of it). **Those two agree to 0.12 points, and they were
measured by different instruments in different processes** — a microbenchmark of the driver's own two
calls, and the row's own span. That is the mechanism confirmed.

**And the declared figure still rose, so prediction 3 — "1.5–4 % below `ns22-D2`" — is refuted.** The
reason is arithmetic, not mystery: the declared figure is `accept_span_ns − teardown_ns`, and the
teardown fell by more than the span did. A term the row *excludes* moved 5.4× further than the work the
change removed, and `operation_work_ns` is a difference of two large noisy numbers rather than a
measurement of either. **This is the round-22 report §8.2 finding arriving on a change of our own**: in
one lock window the teardown has been observed at 60,190,625 / 60,392,959 / 407,533,875 / 528,019,917 ns.

Three registered predictions fired, one was refuted, and the refuted one is the headline number:

| # | registered | measured | verdict |
| --- | --- | --- | --- |
| 1 | fifteen pins, both rows | 15/15 each, both roots | **fired** |
| 2 | `content_objects_offered` = 109,414 / 24,863 | 109,414 / 24,863 | **fired** |
| 3 | declared figure 1.5–4 % **below** `ns22-D2` | **+0.61 %** | **REFUTED** |
| 4 | increment within ±20 % of 36,929,536 | 35,127,296 (−4.9 %) | **fired** |
| 5 | lifetime peak within ±2 % of 885,325,824 | 882,180,096 (−0.36 %) | **fired** |
| 6 | complete command ≤ 15 s | 6.5 s, no exception | **fired** |
| 7 | anchor inside ±20 % of 68,514,625 | 70,004,291 (+2.17 %) | **fired** |

## 2. What this step buys, stated without over-reading it

* **Time: 98,142,374 ns off the accept span, confirmed twice.** Whether it reaches the *declared* figure
  in any given session depends on the teardown, which is not under this change's control.
* **Memory: nothing, and that was registered in advance.** The copy is transient — one object at a time —
  so it never accumulates. The lifetime peak moved −0.36 % (inside the registered ±2 %) and the
  measured-region increment −4.9 % (inside the registered ±20 %). **Option B does not bound the fixture,
  and no reading here says it does.**
* **Mechanism: one copy removed, one gate added, no new declaration.** It is the one option of the four
  that makes the harness simpler.

## 3. The diagnostic the design page said it could not price

`tests/spill_read_price.rs`, unrunnable claims now measured on the 100,000-entry fixture
(502,912,427 canonical bytes, 109,373 objects):

| shape | time | share of the declared figure |
| --- | ---: | ---: |
| one file per object (`TreeStore::write_to_dir` as it stands) | 15,326,877,666 ns | **432 %** |
| packed, positioned reads into a reused 4 MiB buffer | 426,732,834 ns | **12.0 %** |
| packed, `mmap` | 1,059,499,916 ns | **29.9 %** |
| hash only, no read | 373,944,292 ns | **10.5 %** |
| streamed `read` + hash, windowed | 3,049,375 ns over 4.1 MB | **0.09 %** |

**Read as a whole, these say three things.**

1. **The per-file shape was the defect, not the option.** Writing 109,373 files costs 432 % of the row's
   figure to read back; one pack of the same bytes with the same per-object hashing costs 12 %. The
   design page's option A must not be built on `write_to_dir` as it stands.
2. **`mmap` is worse than positioned reads** — 29.9 % against 12.0 %, because a cold mapping pays a page
   fault per object. A bounded reader should `read`, not map.
3. **The floor is the product's own re-identification, not the I/O.** Hashing alone is 10.5 % of the
   figure; the read is a small part of the rest. And **that hash is work today's row also pays** — it is
   how the provider authenticates. So a bounded reader's *added* cost is smaller than 12 % by an amount
   this file does not yet separate, and the honest statement of A's price is: **the read is on the order
   of 1–2 % of the declared figure, and separating it from the hash is the first measurement the round
   that builds A must take.**

## 4. What must not be redone

| closed | why |
| --- | --- |
| the per-file spill shape | measured at 432 % of the figure; do not build option A on `write_to_dir` |
| `mmap` as the reader | measured worse than positioned reads, 29.9 % against 12.0 % |
| the timed deep copy | removed here, 79,174,500 ns measured independently and 98,142,374 ns in the span |
| "option B bounds memory" | it does not; the copy is transient, registered in advance and measured flat |
| the declared figure as a lever detector at this scale | a 98 ms effect is invisible in it while the teardown swings 468 ms |
