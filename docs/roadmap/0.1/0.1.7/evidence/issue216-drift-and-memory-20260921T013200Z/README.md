# Is it getting slower, and where does 700 MB come from

> **Status: DIAGNOSTIC. Not a gate, not a qualification, not a performance claim.**
> Two questions: has the writer path slowed down over this session, and does it
> decay inside one run; and what explains a ~700 MB resident reading. The answers
> are **no measurable trend**, **no decay at budget 1**, and **the diagnostic's own
> retained payloads, plus ~19 MB per in-flight writer**.

## 1. Has it got slower across the session? No measurable trend

The same case - 64 logical writes x 4 MiB at budget 1, incompressible - sampled in
four sequences over roughly forty minutes:

| Sample | Sequence | work | rate |
| --- | --- | ---: | ---: |
| `long-w1` | `issue216-writer-budget-long` | 2.2024 s | 116.24 MiB/s |
| `cpu-w1` | `issue216-cpu-rate-run3` | 2.223 s | 115.16 MiB/s |
| `drift-w1-a` | `issue216-drift-memory` | 2.185 s | 117.15 MiB/s |
| `drift-w1-b` | `issue216-drift-memory` (immediately after) | 2.430 s | 105.36 MiB/s |

Central value ~115 MiB/s, spread -9%/+2%, **no downward trend**. The two
back-to-back identical arms differ by 11%, so that is this host's noise floor at
this moment; an earlier pair of identical arms in one sequence differed by 1.4%.
The 1-minute load average was 5.8-6.9 on 14 logical cores throughout, so the host
never became quieter or busier in a way that explains a trend - there is none to
explain. Other budget-1 arms across the session agree: 124.23 MiB/s for one
256 MiB write, 111.97 MiB/s for 96 x 4 MiB, 109.72 MiB/s for 192 x 2 MiB.

**Consequence for reading any single number here:** differences below ~10% between
two samples are not signal on this host.

## 2. Does it decay inside one run? Not at budget 1

The receipt now reports bytes completed in each half of the work phase's **wall
time**, which is valid at any budget. A first/second split built from a sum of
per-write durations is *not* a rate once writes overlap: it counts the same wall
time once per writer. One earlier arm was printed that way and its line is void
(§5).

| Arm (192 writes x 2 MiB = 384 MiB) | first half | second half | effective cores | peak RSS |
| --- | ---: | ---: | ---: | ---: |
| `shape2-192-w1` (budget 1) | 108.58 MiB/s (95 writes) | 110.86 MiB/s (97 writes) | 0.98 | 441 MiB |
| `shape2-192-w8` (budget 8) | 120.43 MiB/s (100 writes) | 110.80 MiB/s (92 writes) | 1.24 | 568 MiB |

At budget 1 the second half is marginally *faster* than the first while the Store
grows to 391 MiB, so there is no decay as the Store fills. At budget 8 the second
half is 8% lower, which is inside this host's noise and is reported as one sample,
not as a decay claim.

## 3. Where a ~700 MB resident reading comes from

Peak resident size, sampled every 20 ms on the arm's own pid and cross-checked
against `maxrss`:

| Payload | budget 1 | budget 8 | delta | per extra writer |
| ---: | ---: | ---: | ---: | ---: |
| 64 MiB | 125 MiB | 253 MiB | +128 MiB | **+18.6 MB** |
| 256 MiB | 395 MiB | 522 MiB | +127 MiB | **+18.6 MB** |
| 384 MiB | 441 MiB | 568 MiB | +127 MiB | **+19.0 MB** |
| 512 MiB | 742 MiB | 870 MiB | +128 MiB | **+19.2 MB** |

So a 512 MiB workload reaches **742 MiB** at budget 1 and **870 MiB** at budget 8:
a ~700 MB reading at that workload size is expected, and it decomposes as:

- **~19 MB per in-flight writer** - the product's own marginal cost, the fixed
  16 MiB encode workspace, 1 MiB decode workspace, connection and bounded batch
  state. It is the same at every payload size measured, so it is a per-writer
  property, not a per-byte one.
- **the rest is the harness.** This diagnostic keeps every payload in memory for
  its verification pass (512 MiB of the 742 MiB in the 512 MiB case) and the
  allocator retains freed canonical buffers. The 0.5 GiB `measure_ingest` arm
  peaked at **1161 MiB** for the same reason: it holds the payload *and* every
  canonical object at once.

The product's declared bounds are unchanged by any of this: a fixed 16 MiB
encoder and 1 MiB decoder per save, bounded batches, bounded read waves. The
resident figures above are a *harness* footprint at a chosen payload size plus the
per-writer term - not a product memory bound, and not a leak: they return to
baseline when the process exits.

## 4. Gaps, stated plainly

- One sample per arm; no n3, no best-of, nothing pooled across sequences.
- The host was not idle (load 5.8-6.9 of 14 cores), so every rate carries the
  ~10% noise floor measured in §1.
- RSS is process resident size, not a cgroup or lifetime peak; allocator retention
  is not separated from product ownership.
- The 96-write budget-8 arm's write-shape line is void (tool defect, §5); its rate
  and RSS are used, that line is not.
- No arm above budget 8 was accounted for memory.

## 5. Retained and not used

`shape-96-w8` (`issue216-shape-20260921T012400Z`) printed
`first half 15.64 MiB/s, second half 16.16 MiB/s` - a meaningless figure, because
that build summed overlapping per-write durations and divided payload by it. The
same build's budget-1 arm is valid (the durations do not overlap there). The tool
was corrected to bytes-per-wall-time-half and the corrected arms are §2; the
defective sequence stays on disk and its rate line is not used anywhere.

## 6. Reproduction

```sh
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-service --example measure_admission
/usr/bin/time -l core/target/release/examples/measure_admission --writers 1 --writes 192 \
  --bytes 2097152 --mode queued --commit "$(git rev-parse HEAD)" --output /tmp/shape-w1
/usr/bin/time -l core/target/release/examples/measure_admission --writers 8 --writes 192 \
  --bytes 2097152 --mode queued --commit "$(git rev-parse HEAD)" --output /tmp/shape-w8
# peak RSS: ps -o rss= -p <arm pid> every 20 ms; effective cores: (user + sys) / real
```

Raw outputs: `~/Ephemeral-AI-Lab/layerfs-216-measure/issue216-drift-memory-20260921T012000Z`,
`.../issue216-shape-20260921T012400Z`, `.../issue216-shape2-20260921T013000Z`.
Those directories hold the arms' Store files (about 5.5 GB across the whole
session) and are outside the repository; the receipts and JSON mirrors are
committed here.
