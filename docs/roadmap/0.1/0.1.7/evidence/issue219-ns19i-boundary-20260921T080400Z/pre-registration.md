# Pre-registration — #219 round 9: publish the work the row's timer excludes

Written **before** the first run of this arm and before any edit. **No product line changes**: this
round adds a harness instrument, and the treatment under test is the *row's boundary*, not the store.

Control: **H2** (`benchmark-results/issue219/ns19-H2-pinned-20260921T075340Z`, `5c2858b40`):
`operation_work_ns` 1656.2 ms, CPU user+system 1682.5 ms, `span_content_ns` 1321.1 ms,
`span_build_ns` 311.0 ms, waves 73, commits 284, 13/13 gates, 14/14 pinned counters.

## Why this instrument, stated as the question it decides

The v0.1.6 `init_namespace` comparison cannot be made on the numbers both sides publish, because the
two timers contain different work:

- the reference's `layerstack_init_ns` **includes** reading its 300,000,000-byte fixture and building
  the ~25,158 canonical objects it then admits (`scanned_bytes` 300,000,000; `candidate_objects`
  25,158; `initialization_disk_read_bytes` 0–0.73 MB, so the scan is cache-served);
- this row's `operation_work_ns` **excludes** that construction — C2's supplied-object rule puts it in
  untimed setup (`Timing::disabled("setup.construct", …)`) — while **including** the C1 tree build
  (`span_build_ns`) and `Store::open`, which the reference also pays.

So the two published figures are not the same measurement, and today only a *sum* is published for
the excluded half: `preparation_wall_ns` (919.1 ms on H2), which also contains the fixture recipe,
the base Store, the sample clone and the de-warm. **A reader cannot do the subtraction**, and the
attribution in `docs/roadmap/0.1/0.1.7/issue219-v016-gap-rca-handoff.md` §1b turns on exactly that
subtraction.

## The one difference

**The row publishes the excluded construction in its two named parts**, each an `Instant` pair around
existing code, neither inside the row's formula:

- `pipeline.construct_ns` — the product's `construct_bytes` call for every planned file;
- `pipeline.construct_noise_ns` — the harness's `fixture::noise` byte generation that precedes it.

Both are diagnostics in the sense the row already publishes `establishment_ns` and `teardown_ns`:
they are excluded from `operation_work_ns`, they are published beside it, and
`construct_ns + construct_noise_ns + operation_work_ns` is the figure that can be laid against the
reference's timer. Nothing else moves — no product line, no bound, no pin, no timer boundary.

## Prediction, in the instrument's own units

The harness has already measured C1 canonical construction on this host at **617 MiB/s**
(`evidence/issue216-store-path-attribution-20260921T034000Z/`, quoted in the RCA handoff). The loop
constructs **24,863 content objects totalling 301,171,810 canonical bytes**, so:

| instrument | predicted | derivation |
| --- | ---: | --- |
| `pipeline.construct_ns` | **380–560 ms** | 301,171,810 bytes at 617 MiB/s = 466 ms, ±20 % |
| `pipeline.construct_noise_ns` | **100–260 ms** | 302 MB of pseudorandom generation, the analogue of the reference's 300 MB cache-served scan |
| their sum | **480–820 ms** | and it must be ≤ `preparation_wall_ns` 919.1 ms minus the acquisition (8.2 ms) and the fixture/base-store/clone work |

And the number the round exists to produce, on the reference's boundary:

| figure | value |
| --- | --- |
| our CPU inside the formula | 1682.5 ms |
| + the excluded construction | predicted **2160–2500 ms** |
| the reference's own CPU, same 25,158-object shape | **1789.5 / 2116.7 / 2195.3 ms** (three rows, 1.93–2.07 cores) |

so the standing hypothesis under test is: **our CPU lead disappears once the boundary is matched,
and the single-row comparison lands inside the reference's own spread.**

## What would refute it

1. `construct_ns + construct_noise_ns` **< 150 ms** — then the excluded work is negligible, the
   boundary correction is nil, and the CPU-lead statement in L68 stands unchanged. That is a
   refutation of the *correction*, not of the row.
2. The sum **exceeds `preparation_wall_ns`**, which would mean the instrument is charging something
   the preparation window does not contain — an instrument defect to diagnose from the receipt.
3. Any of the 14 pinned counters moves, or `pipeline.commits` != 284, or the root digest changes, or
   the row is not PASS 13/13.
4. `operation_work_ns` moves outside 1656.2 ± 250 ms, or CPU outside 1682.5 ± 250 ms: the instrument
   must be work-neutral and this row must remain comparable with H2.
5. The harness's own unit tests gain a failure beyond the three pre-existing `registry_negative`
   cases (registry 221 rows against the frozen 220).

## What is not claimed

No v0.1.6 pairing. `layerstack_init_ns` and `operation_work_ns` still differ after this correction —
the reference reads a fixture from a workspace over FUSE in a Linux container while this row
generates its bytes in-process, and the reference's receipts carry no seal this side can be matched
against. The corrected figure is a **boundary-matched hypothesis**, and it is recorded as one.
