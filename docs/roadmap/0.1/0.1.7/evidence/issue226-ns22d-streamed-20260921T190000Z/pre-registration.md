# Pre-registration — #226 S1: the fixture streamed from a reader, inside the timer

Written **before the first locked run of this step**. It registers one harness switch and the boundary
change that comes with it, both on the owner's ruling of 2026-09-21: *"s1 first"*, taken after the design
page priced S1 against S2 and the change was described as the way back to the reference product's own
shape.

## 1. What changes, and what it costs

`LAYERFS_PIPELINE_STREAM_FIXTURE=1` replaces the untimed construction pass and the in-memory content
store with a per-file `NoiseReader` (`workload/stream.rs`) feeding **`construct_stream`** inside the
measured closure. `construct_stream` already exists and is exported
(`core/crates/layerfs-content/src/file/content.rs:279`, `lib.rs:27`); it buffers at most
`small_file_threshold_bytes` = 131,072 B for its threshold probe and then hands the reader to the
chunker. **No product line changes.**

**The boundary moves, by ruling.** The row's declared figure has excluded content construction
(`test_setup_and_cache_discipline.md` §2.2). Under S1 the construction is *inside* the region the figure
is read from — which is the reference product's own boundary: v0.1.6's `init_namespace` read
**887,242,752 bytes** off disk inside `layerstack_init_ns`
(`benchmark-results/issue152/g1/namespace-100000-r4/perf.jsonl`). **Figures taken under S1 are not
comparable with figures taken without it**, and the row's notes must say which one produced them.

## 2. Registered predictions

| # | quantity | prediction | basis |
| --- | --- | --- | --- |
| 1 | **lifetime** peak RSS, 100,000 row | **falls from 882,180,096 to 140–175 MB**, i.e. **≤ 20 %** of the present peak | the 728,465,408 B content store leaves the process; the plan (~55 MB) and the measured region stay |
| 2 | measured-region **increment** | **rises**, to **40–70 MB** | construction is inside the region now; the round-21 probe put the driver's own construction at ~0.59 s and 558,516,171 B of *live heap*, so the region's cost changes meaning, not just size |
| 3 | declared figure `operation_work_ns` | **rises by 30–40 %** to **4.5–5.3 s**, and is declared to move | it now contains the construction the figure used to exclude |
| 4 | all **fifteen pins**, both `pipeline.*` rows | **reproduce exactly**, both filesystem roots | the fixture is byte-identical — `a_streamed_file_is_byte_identical_to_a_materialised_one` holds `NoiseReader` to `fixture::noise` at the row's own sizes, including the 100,000,000-byte anchor and window sizes from 1 to 1 MiB |
| 5 | `pipeline.content_objects` / `content_bytes` | **109,414 / 502,914,928** | the stream offers the same objects; `content_bytes` is charged by a consumer on the offer path because logical length is not canonical |
| 6 | `heap.peak_incremental_bytes`, 100,000 row | **falls** from 36,307,857 — the `noise` buffers are gone | the 503 MB slice is replaced by a 1 MiB window per file |
| 7 | 10,000 row | same direction: peak from 528,007,168 to **< 100 MB** | same mechanism at one tenth the size |
| 8 | complete command, 100,000 row | **≤ 15 s** | `AGENTS.md` §3.7 |
| 9 | session anchor `namespace-10000` | inside ±20 % of 68,514,625 | round 21 §2's rule |

**Refuted if** a pin moves, if the peak falls by less than 3× (the fixture was not what was retained), if
`heap.peak_incremental_bytes` **rises** (a buffer replaced by a bigger one), or if the declared figure
does not move — the last would mean the construction cost nothing, which contradicts the round-21 probe.

## 3. What was measured before this file

Two **unlocked** diagnostics of the rebuilt binary, declared here as the round's pattern requires:

```text
pipeline-namespace-100000, LAYERFS_PIPELINE_STREAM_FIXTURE=1, /tmp/ns22-s1-m
  PASS, 15/15 pins reproduce, digest 2412681d335571082c4dfbc1df2117bc015b7f6132fca17cb0d11bbeaaefd954
  content_objects 109,414   content_objects_offered 109,414   content_bytes 502,914,928
  peak 156,811,264   baseline 97,632,256   increment 59,179,008   heap 29,833,599

pipeline-namespace-10000, same switch, /tmp/ns22-s1-10k
  PASS, 15/15 pins reproduce, digest 1d6fba29aba105aefbb668c34b026f8b89d20773fc36d548b991034dc3857847
  peak 65,142,784   content_objects_offered 24,863
```

They are **not evidence and not samples**; they are what the predictions above are anchored on, which is
why prediction 1 carries a band around the 156.8 MB already observed rather than a direction.
