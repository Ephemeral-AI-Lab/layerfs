# 64 x 8 MiB against 1 x 512 MiB, one operation in flight at a time

> **Status: DIAGNOSTIC. Not a gate, not a qualification, not a performance claim.**
> The test set the owner asked for: **concurrency fixed at 1**, the same 512 MiB of
> distinct payload either as **64 x 8 MiB** or as **1 x 512 MiB**, over the real
> authenticated route. Answer: the batch is **+9.0%** slower, not "much slower".

## 1. The measurement

Six arms in alternating order (`single, batch, batch, single, single, batch`), so
drift over the sequence lands on both shapes. Every arm: a fresh Store copy from
the closed prepared master, a fresh `layerfs-service` process on 127.0.0.1, one
`layerfs-daemon` process per operation carrying the plaintext `LFB1` frames, and
the same pre-generated 512 MiB of random payload sliced so each operation carries
distinct content. **The next operation starts only after the previous response has
been read** - concurrency is exactly 1.

| Arm | shape | wall | rate | per operation | cores |
| --- | --- | ---: | ---: | ---: | ---: |
| `single-1x512mib-1` | 1 x 512 MiB | 6.9493 s | 73.68 MiB/s | 6949 ms | 1.295 |
| `batch-64x8mib-1` | 64 x 8 MiB | 7.3236 s | 69.91 MiB/s | 114.4 ms | 1.136 |
| `batch-64x8mib-2` | 64 x 8 MiB | 7.2271 s | 70.84 MiB/s | 112.9 ms | 1.151 |
| `single-1x512mib-2` | 1 x 512 MiB | 6.6230 s | 77.31 MiB/s | 6623 ms | 1.333 |
| `single-1x512mib-3` | 1 x 512 MiB | 6.6497 s | 77.00 MiB/s | 6650 ms | 1.313 |
| `batch-64x8mib-3` | 64 x 8 MiB | 7.4912 s | 68.35 MiB/s | 117.1 ms | 1.140 |

| | 1 x 512 MiB | 64 x 8 MiB |
| --- | ---: | ---: |
| mean wall | **6.7407 s** | **7.3473 s** |
| spread across the three samples | 4.93% | 3.65% |
| rate range | 73.68-77.31 MiB/s | 68.35-70.84 MiB/s |
| cores | 1.295-1.333 | 1.136-1.151 |

- **Difference: +0.6066 s = +9.0%** for the batch of 64.
- **Per extra operation: 9.63 ms.** Of that, **5.45 ms is this harness spawning a
  process per operation** (measured separately: 64 spawns of the same binary
  against a refused endpoint), leaving **about 4.18 ms** for the product's own
  per-operation work: connect, handshake, session thread, Store open, save slot
  and publication.
- An earlier single pair of the same two arms measured **+6.75%** (7.302 s against
  7.795 s), the same order of magnitude, so the penalty is a few percent to ten
  percent - not a multiple.
- The batch also uses **fewer cores for the same bytes** (1.14 against 1.31): one
  long operation streams with one store session, while 64 operations pay 64
  session setups, 64 publications and 64 connection handshakes.

## 2. Where the time goes instead

The absolute rate on this route (70-77 MiB/s) is far below either component's own
rate, and that is the larger effect:

- transport alone (`transport_probe`, 1 GiB in one stream, same host): **212 MiB/s**;
- construct + save alone (in-process, 64 x 4 MiB): **115 MiB/s**;
- the route with one 512 MiB operation: **74-77 MiB/s**, at ~1.3 cores.

So inside one operation the wire and the store do not overlap: the route is
roughly the sum of its parts, not their maximum. Concurrency across operations is
where the gain is - eight concurrent 64 MiB operations at a writer budget of 8
finish in 3.916 s against 7.057 s sequentially, **1.80x** (see the
[bridge serialization page](../issue216-bridge-serialization-20260921T015800Z/README.md)).

## 3. Gaps

- Diagnostic, not a gate: three samples per shape, all reported, nothing selected
  and nothing pooled with the earlier pair.
- Host-to-host loopback, not the frozen `--cpus=1` container topology. A
  **container** per operation (which the frozen tdx1/issue192 routes do with
  `docker run`) costs hundreds of milliseconds rather than 5.45 ms, and would turn
  this 9% into a much larger penalty - that is a harness topology cost, and it is
  the first thing to check in any measurement that shows a big penalty.
- Host not idle (load ~6-7 of 14 cores); the observed 4-5% spread within each
  shape bounds how small a difference this design can resolve.
- The harness's own summary step crashed twice on list filters after all six arms
  had completed; the arms are intact on disk and `results.json` records that its
  summary was recomputed from their receipts.
