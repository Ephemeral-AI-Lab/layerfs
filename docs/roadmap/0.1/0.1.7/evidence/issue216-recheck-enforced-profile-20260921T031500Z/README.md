# The original #216 task, re-run under the enforced build profile

> **Status: DIAGNOSTIC re-check. Not a gate, not a release claim.** It answers one
> question: with the new build configuration in place (repository-root
> `.cargo/config.toml`, the aarch64 refusal, moved flags), is the task we were
> originally assigned still implemented and still measured the same way? Yes - and
> one claim from the earlier rounds does not survive the third sequence, which is
> corrected here.

## 1. Correctness and admission are unchanged

`cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
--no-fail-fast`: **621 passed, 0 failed**, including the whole-budget ladder test
(settings 1, 2, 3, 5, 8, 16, 64 end to end) and the service-route ladder
(1, 2, 3, 8 through `Service::handle`). Clippy `-D warnings`, `fmt --check` and the
boundary guard over 194 files are clean.

Measured admission at every budget, 16 callers released at once (group B):

| Budget | admitted at once | refused (`Capacity`) | saved + verified |
| ---: | ---: | ---: | --- |
| 1 | 1 | 15 | 1/1 byte-identical |
| 2 | 2 | 14 | 2/2 |
| 4 | 4 | 12 | 4/4 |
| 8 | 8 | 8 | 8/8 |

Exactly the earlier counts, so the enforced profile changed no admission
behaviour - as expected, since this path never touches the AEAD.

## 2. The writer-budget ladder, re-measured on the new compilation

64 logical writes x 4 MiB, one sample per arm, in-process service route:

| Budget | work | rate | verified | refused |
| ---: | ---: | ---: | --- | ---: |
| 1 | 2.043 s | 125.29 MiB/s | 64/64 | 0 |
| 2 | 1.794 s | **142.68 MiB/s** | 64/64 | 0 |
| 4 | 1.816 s | 140.99 MiB/s | 64/64 | 0 |
| 8 | 1.928 s | 132.76 MiB/s | 64/64 | 0 |

Against the earlier unconfigured-build sequence (116.24 / 144.91 / 138.33 /
115.76 MiB/s) the shape is the same - best at a budget of 2-4 - while the budget-1
and budget-8 arms moved up 8% and 15%. Both are inside the ~10% window noise this
host shows, and the earlier conclusion stands: **the budget is an admission
control, not a throughput knob.**

## 3. Correction: the "+9% batch penalty" is not resolvable

The owner-requested pair (64 x 8 MiB against 1 x 512 MiB, concurrency 1) re-run
through the authenticated transport on this build:

| Shape | this run | flagged build (earlier) | unconfigured build (earlier) |
| --- | ---: | ---: | ---: |
| 1 x 512 MiB | 5.210 s / 98.27 MiB/s | 5.342, 4.892 s | 6.949, 6.623, 6.650 s |
| 64 x 8 MiB | 4.859 s / 105.37 MiB/s | 5.550, 5.617 s | 7.324, 7.227, 7.491 s |
| batch vs single | **-6.74%** | +9.1% | +9.0% |

Three sequences now disagree in sign. Pooling the two configured-profile
sequences (the only comparable ones): singles mean 5.148 s, batches mean 5.342 s,
**+3.8%**, with a per-sequence range of **-6.7% to +9.1%**. So the honest statement
is not "the batch costs +9%" but **"the batch is within this host's noise of one
big operation; a few milliseconds of fixed cost per operation exist (5.45 ms of it
is this harness spawning a process per operation), and no penalty above that is
resolvable at three samples per shape."** The earlier pages' "+9% stands" line is
superseded by this paragraph; their receipts are untouched.

## 4. What is still true from the earlier rounds

- The transport is 3.7x faster with the profile (214 -> 802 MiB/s one stream,
  425 -> 1556 MiB/s two) and the profile is now the default and enforced.
- Admission, refusal, isolation, duplicate ownership, publication, retained
  ownership and the memory law hold at every setting (621 tests plus these arms).
- The route runs at ~100 MiB/s because the store half is the bottleneck, not the
  wire (transport alone 802 MiB/s, construct+save alone ~115 MiB/s in-process).

## 5. Gaps

- One sample per arm in groups A and C; group B is deterministic.
- Host not idle (load ~6-7 of 14 cores); the ±10% spread is the resolution limit
  for the pair question, and §3 shows what happens when a claim is made below it.
- `max_workspaces_per_sandbox` remains unimplemented and blocked on pair 1
  ([#179](https://github.com/Ephemeral-AI-Lab/layerfs/issues/179)); nothing here
  changes that.
