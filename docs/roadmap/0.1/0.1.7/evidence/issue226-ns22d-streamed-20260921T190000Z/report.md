# Report — #226 S1: the fixture streamed, and 5.54× off the peak

Pre-registration: [`pre-registration.md`](pre-registration.md), written before the first locked run.
Raw receipts under [`raw/`](raw/). One sample per case, one lock window, `--verify full`, fresh `--out`
per run, clean tree (`source_dirty: false`), binary `9b6dca6e4340…`, commit `7feee8d90`.

## 1. The headline

| | `ns22-E2` (store-held) | **`ns22-F2` (streamed)** | change |
| --- | ---: | ---: | ---: |
| **lifetime peak RSS** | 882,180,096 | **159,236,096** | **−722,944,000 (−81.95 %)** |
| **peak, as a multiple** | — | — | **5.54× lower** |
| measured-region baseline | 847,052,800 | **103,088,128** | −87.83 % |
| `heap.peak_incremental_bytes` | 36,307,857 | 29,833,774 | −17.83 % |
| measured-region increment | 36,929,536 | 56,147,968 | +52.04 % |
| `pipeline.operation_work_ns` | 3,662,759,417 | 4,707,842,708 | +28.53 % |
| complete command | 6,477,000,000-scale | **6.459 s** | inside the 15 s budget |
| fifteen pins, both rows | reproduce | **reproduce** | both filesystem roots |
| status | `PASS` 15/15 | **`PASS` 15/15** | — |

**And against the reference product, which is the point of the exercise:**

| peak, same declared workload | bytes | vs v0.1.6 |
| --- | ---: | ---: |
| v0.1.6 `namespace-100000` | 83,148,800 | 1.00× |
| v0.1.7 before this change | 882,180,096 | **10.61×** |
| **v0.1.7 after this change** | **159,236,096** | **1.92×** |

The 10,000-entry row moves the same way: peak **528,007,168 → 65,601,536 (−87.6 %)**, fifteen pins intact.

## 2. What was changed

One switch, `LAYERFS_PIPELINE_STREAM_FIXTURE=1`, and one new reader. The row built every file's bytes with
`fixture::noise(file.size, …)` and handed the slice to `construct_bytes`, so 502,914,928 canonical bytes
were live before the timer. It now builds each file from `NoiseReader` through **`construct_stream<R: Read>`**
(`core/crates/layerfs-content/src/file/content.rs:279`), which buffers at most
`small_file_threshold_bytes` = **131,072 B** for its threshold probe and hands the reader to the chunker.
**No product line changed**: `core/crates/**` is byte-identical to `5be4b7ae0`.

`NoiseReader` (`workload/stream.rs`) reproduces `fixture::noise` exactly —
`Rng::fill` consumes one 64-bit value per eight output bytes, so the stream is a function of position
alone. `a_streamed_file_is_byte_identical_to_a_materialised_one` holds that at the row's own sizes
(0, 1, 7, 8, 9, 4,598, 131,071, 131,072, 131,073, 1,000,000 and the **100,000,000-byte anchor**) against
window sizes from 1 byte to 1 MiB.

## 3. The boundary moved, by ruling, and it is published

The row's declared figure has excluded content construction (`test_setup_and_cache_discipline.md` §2.2).
Under this switch the construction is **inside** the region the figure is read from — which is the
**reference product's own boundary**: v0.1.6's `init_namespace` read **887,242,752 bytes** off disk inside
`layerstack_init_ns`. So this is not a new licence; it is the predecessor's shape restored.

**Figures taken under this switch are not comparable with figures taken without it.** The row's notes
must say which produced them, and `pipeline.operation_work_ns` now contains the construction the figure
used to exclude — which is exactly what the +28.53 % on it is.

## 4. Pre-registration, scored

| # | registered | measured | verdict |
| --- | --- | --- | --- |
| 1 | lifetime peak 140–175 MB | **159,236,096** | **fired** |
| 2 | increment 40–70 MB | 56,147,968 | **fired** |
| 3 | declared figure 4.5–5.3 s | **4.708 s** | **fired** (just inside) |
| 4 | fifteen pins, both rows | 15/15 each, both roots | **fired** |
| 5 | `content_objects` 109,414 / `content_bytes` 502,914,928 | exact | **fired** |
| 6 | `heap.peak_incremental_bytes` falls | 36,307,857 → 29,833,774 | **fired** |
| 7 | 10,000 row under 100 MB | **65,601,536** | **fired** |
| 8 | complete command ≤ 15 s | 6.459 s | **fired** |
| 9 | anchor inside ±20 % of 68,514,625 | **70,874,500** (+3.44 %) | **fired** |

**No refutation condition fired.** All nine predictions landed.

## 5. Two corrections the implementation forced, recorded rather than smoothed over

1. **`content_bytes` had to be charged on the offer path, not summed from logical lengths.** Logical is
   500,000,000 and the pinned canonical figure is **502,914,928**; the envelope is the difference, and the
   first version of this change published the logical number and failed `g1.o3-pinned-counters`. A
   `ContentMeter` now charges `canonical_len()` where the objects pass — the same quantity the store-held
   path published.
2. **`content_objects` is objects *produced*, not files.** The first version counted non-empty files
   (99,000) against a pin of 109,414, which is 98,998 whole-file members **plus 10,330 chunk objects**
   from the two 100 MB anchors. The streamed path publishes what it offered, and `g6.content-complete`
   now holds the shape instead of a tautology: every non-empty file contributed at least one object, and
   the canonical bytes stay inside the envelope the logical bytes allow.

## 6. What this does not claim

* **Not parity with v0.1.6.** 1.92× remains. The row still holds the prepared tree and plan
  (~55 MB measured separately) and the platform differs — v0.1.6's figure is Linux/arm64 in a 2 GiB
  container, this one is macOS/arm64.
* **Not a faster row.** The declared figure rose 28.53 % because it now contains construction. Under the
  old boundary the comparable figure would be the old one plus `construct_ns` + `construct_noise_ns`,
  which the row still publishes.
* **Not a product claim.** No product line changed, and the product's own bounded region is unchanged in
  character: `heap.peak_incremental_bytes` fell 17.83 % only because the fixture's 503 MB slice is gone.
* **Not yet a registered row variant.** The switch is an environment variable, not a registry entry. A
  registered variant with its own identity, pins and boundary declaration is the right long-term shape
  and is deliberately not taken here — it moves the frozen cardinality, the golden table and the pins.
