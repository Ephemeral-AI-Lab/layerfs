# Writer budget at different settings: correctness and throughput

> **Status: DIAGNOSTIC. Not a gate, not a qualification, not a performance claim.**
> Nothing here decides release admission, and no row is promoted to a gate sample.
> The numbers answer two questions about `max_concurrent_writes_per_store` (#216):
> does the product stay correct at every setting, and does raising the setting buy
> throughput. The second answer is **no**, and this page reports that as plainly as
> the correctness result.

Tracking: [#216](https://github.com/Ephemeral-AI-Lab/layerfs/issues/216). The
operator-facing definition lives in
[concurrency controls](../../concurrency-controls.md).

## 1. What was asked, and what would count as an answer

1. **Correctness at different settings.** Every supported setting must admit
   exactly its budget, refuse the next writer with an explicit bounded refusal,
   keep duplicate-ownership, publication, read-back and reuse correct, and keep
   retained ownership unbypassable. Answered by a deterministic matrix over
   settings 1, 2, 3, 5, 8, 16 and 64 (§5) plus a real-service-route matrix over
   1, 2, 3 and 8 (§6), and by the admission arms in §4.
2. **Throughput at different settings.** The same total logical write work, with
   at most `W` writes in flight, at `W` = 1, 2, 4 and 8, through the real service
   entry point. Answered in §3. A larger number from a larger setting is **not**
   by itself a claim: the comparison is reported with its variability control and
   its measured cause.

## 2. Identity, cache state and interference

| | |
| --- | --- |
| Commit / tree | `results.json` → `commit`, `tree` (this evidence directory's own commit is recorded there; the arms ran on the commit that added the measurement example) |
| Product lockfile | `core/Cargo.lock` sha256 in `identity.json` |
| Binaries | `core/target/release/examples/measure_admission` and `measure_ingest`, sha256 in `identity.json`, release profile |
| Host | macOS 26.4.1, aarch64, 14 logical cores, 36 GiB, `uname` in `identity.json` |
| Construction workers | one producer per operation, as always; the arms add concurrent **operations**, never a second producer inside one |
| Cache state | declared and identical per arm: payloads are built in-process immediately before the timed phase, each arm gets a **fresh** Store inside its own output directory, no invalidation is applied to one arm only, and **no cold-cache reading is claimed** |
| Samples | one sample per case per arm; nothing was retried, dropped or selected |
| Interference | **the host was not idle.** Load average 6.79–7.05 (1 min) during the long case, 3.93–5.69 during the short case, on 14 logical cores, with desktop processes (WindowServer 53%, Chrome 48%, Codex 30%, WeChat 26%) using CPU throughout. Every arm ran under the same declared interference, none was given a quieter window, and none was repeated to look for a better number |

Raw outputs stay outside the repository at
`~/Ephemeral-AI-Lab/layerfs-216-measure/issue216-writer-budget-run2-20260921T004651Z`
(short case and admission case) and
`~/Ephemeral-AI-Lab/layerfs-216-measure/issue216-writer-budget-long-20260921T004651Z`
(long case and split diagnostic); each arm's Store file is identified by its
sha256 in `arms/<arm>/store-sha256.txt` and its persisted state by
`arms/<arm>/store-accounting.txt`. The Stores themselves (65–260 MiB each) are not
committed; the receipts, the accounting and the hashes are.

## 3. Throughput at different settings

Two cases, one sample each, release build. `short` = 16 logical writes × 4 MiB
(64 MiB per arm); `long` = 64 logical writes × 4 MiB (256 MiB per arm). Every
write carries distinct bytes, so every write inserts its own objects and the work
is identical in every arm.

| Arm | W | work (s) | MiB/s | writes/s | verified read-back | refused | peak in flight | RSS after (KiB) | speedup vs W=1 |
| --- | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: |
| `queued-w1` | 1 | 0.5557 | 115.17 | 28.79 | 16/16 | 0 | 1 | 125,248 | 1.000× |
| `queued-w2` | 2 | 0.4906 | 130.45 | 32.61 | 16/16 | 0 | 2 | 140,256 | **1.133×** |
| `queued-w4` | 4 | 0.5001 | 127.96 | 31.99 | 16/16 | 0 | 4 | 179,632 | 1.111× |
| `queued-w8` | 8 | 0.5776 | 110.81 | 27.70 | 16/16 | 0 | 8 | 252,624 | 0.962× |
| `queued-w1-control` | 1 | 0.5637 | 113.54 | 28.38 | 16/16 | 0 | 1 | 125,072 | 0.986× |
| `long-w1` | 1 | 2.2024 | 116.24 | 29.06 | 64/64 | 0 | 1 | 395,488 | 1.000× |
| `long-w2` | 2 | 1.7666 | 144.91 | 36.23 | 64/64 | 0 | 2 | 416,848 | **1.247×** |
| `long-w4` | 4 | 1.8507 | 138.33 | 34.58 | 64/64 | 0 | 4 | 451,200 | 1.190× |
| `long-w8` | 8 | 2.2114 | 115.76 | 28.94 | 64/64 | 0 | 8 | 522,384 | 0.996× |

**The variability control.** `queued-w1-control` is the same configuration as
`queued-w1`, run last in the same sequence: 0.5637 s against 0.5557 s, a 1.4%
spread. The W=2 gain (13–25%) is larger than that spread, so the gain is a real
observation on this host; the W=4 and W=8 rows are inside or below it, so they
show **no** gain. The control is reported as its own row and is never pooled with
the first W=1 sample.

**Why the gain stops at two.** A supporting single-sample diagnostic
(`diagnostics/measure-ingest-64mib/stdout.txt`, the existing `measure_ingest`
example, 64 MiB of the same incompressible payload through the same path) splits
one logical write:

```text
content construct     0.104 s     617.40 MiB/s     ~18% of the write
save total            0.459 s     139.47 MiB/s     ~82% of the write
END TO END            0.563 s     113.77 MiB/s
```

Only the construction half can overlap between writers. The save half runs its
short write transactions under the Store's in-process arbitration, so it
serializes by design. Overlapping more than two writes therefore has almost
nothing left to overlap, and beyond that the extra writers add memory and
contention rather than throughput. The host being busy (load ≈ 7 of 14 cores)
can only depress the overlap further; separating "work mix" from "host
contention" needs per-arm CPU accounting on an idle host, which was not taken and
is listed as a gap in §7.

**What this does not say.** It is not a statement that the budget is useless: the
budget is an admission and isolation control (§4), and the throughput question was
asked because a larger number must not be inferred from a larger setting. On this
host and this workload, the honest answer is that `max_concurrent_writes_per_store`
above 2 buys no throughput, and the memory cost per extra writer is real.

## 4. Admission at different settings (deterministic, not a rate)

`oversubscribed` releases 16 callers at once (1 MiB each) against the configured
budget. An admitted caller holds its permit inside its input read until every
caller has had its decision, so the counts are not a race. The `work` column here
is **decision time, not a throughput**: the MiB/s figure in the JSON mirror is
meaningless for this case and is not used.

| Arm | W | admitted at once | refused (`Capacity`) | failed | decisions (s) | saved + verified | store: private owners / published saves |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `oversub-w1` | 1 | 1 | 15 | 0 | 0.0159 | 1/1 byte-identical | 0 / 1 |
| `oversub-w2` | 2 | 2 | 14 | 0 | 0.0222 | 2/2 byte-identical | 0 / 2 |
| `oversub-w4` | 4 | 4 | 12 | 0 | 0.0382 | 4/4 byte-identical | 0 / 4 |
| `oversub-w8` | 8 | 8 | 8 | 0 | 0.0785 | 8/8 byte-identical | 0 / 8 |

Every arm refused exactly `16 − W` callers with the bounded `Capacity` class, in
tens of milliseconds, with no waiting and no queue; every admitted write saved and
read back byte-identically; every arm's Store records
`max_concurrent_writes` = the configured `W`, `saves_private = 0` (the refused
callers left no ownership behind) and `saves_published` = the admitted count.
That is the acceptance row "excess writes get an explicit bounded refusal" and
"unresolved owners cannot be bypassed", measured at each setting.

## 5. Correctness matrix: the whole ladder, end to end (tests)

`core/crates/layerfs-storage/tests/write_admission.rs` →
`every_supported_setting_admits_its_budget_and_publishes_correctly` runs settings
**1, 2, 3, 5, 8, 16, 64**, each on a fresh Store, and for every one of them
asserts: exactly that many writers are admitted and the next is refused with
`OwnershipUnavailable`; the live slots are exactly `1..=W`; all `W` writers hold a
private copy of the same identity simultaneously and then publish; publication
releases every slot; the locator rows for that identity are exactly `W` (at 64
this is the supported slot space itself); the content reads back byte-identical;
a later save reuses instead of inserting a ninth/17th/65th copy; and lowering the
setting to 1 afterwards leaves the content readable while refusing the second
writer. The same file also covers the default budget, budget 8 with a released
and re-taken slot, retained owners under a lowered budget, a retained owner in a
slot above the new budget, content written high and read low, unsupported values
(0, 65, 255), the schema constraint on the persisted value, and schema-7 refusal.

## 6. Correctness matrix through the real service route (tests)

`core/crates/layerfs-service/tests/admission.rs` →
`every_supported_budget_admits_its_writers_and_refuses_the_next` runs settings
**1, 2, 3, 8** through `Service::handle` (the same body the native transport
calls): `W` writers are admitted and held inside their input read, the next write
gets `Capacity`, a read is admitted while all `W` hold writer permits, and after
release the next write succeeds. The same file covers four writers held
simultaneously with reads never queueing behind them, two sandboxes sharing one
Store's budget, a second Store with its own budget, and a saturated read bound
that neither blocks writers nor exceeds itself.

## 7. Gaps, stated plainly

- **The Linux Docker daemon/host routes were not run** (no image or identity
  manifest exists for this source and none was built). The transport-level route
  is unqualified here; the in-process service route above is the same
  `Service::handle` body the transport calls.
- **The host was not idle** (load ≈ 7 of 14 cores, desktop processes busy). All
  arms share that declared interference; no per-arm CPU accounting was taken, so
  the split between work mix and host contention is measured for the work mix
  (§3) and only declared for contention.
- **One sample per arm.** The only variability evidence is the W=1 control
  (1.4%). No n3, no best-of, no pooling.
- **No timed arm above W=8.** Settings 16 and 64 are covered functionally (§5),
  not by a rate.
- **A discarded sequence is retained on disk, not used.**
  `~/Ephemeral-AI-Lab/layerfs-216-measure/issue216-writer-budget-20260921T004344Z`
  ran the same nine arms before the measurement example's JSON mirror was fixed:
  it wrote `Some(125248)` where JSON needs a number. Its **text** receipts were
  valid, but a repaired sequence is not mixed with a defective one, so the whole
  sequence is discarded and reported here. It also carries one method warning: in
  that sequence the first arm took 2.50 s against 0.72 s for the rest (cold
  binary page-in and first-touch filesystem work after a fresh release build),
  which is why every sequence below reports a control arm instead of trusting its
  first row. In this evidence's sequences the first arm was not an outlier
  (`queued-w1` 0.5557 s against the control 0.5637 s).
- **No resource qualification.** The RSS column is `ps -o rss=` after the work
  phase, an observation of this process, not a cgroup or lifetime peak.

## 8. Reproduction

```sh
H=core/crates/layerfs-service
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-service --example measure_admission
cargo +1.85.1 build --release --locked --manifest-path core/Cargo.toml -p layerfs-storage --example measure_ingest
B=core/target/release/examples/measure_admission
$B --writers 2 --writes 16 --bytes 4194304 --mode queued --commit "$(git rev-parse HEAD)" --output /tmp/arm-w2
$B --writers 8 --writes 16 --bytes 1048576 --mode oversubscribed --commit "$(git rev-parse HEAD)" --output /tmp/arm-oversub-w8
core/target/release/examples/measure_ingest --bytes 67108864 --pattern noise --output /tmp/split
```

Every arm refuses an existing `--output` path, writes its receipt and its JSON
mirror once, and is run under this worktree's measurement lock
(`core/benchmark/fs-bench-pro-storage-content/.measurement.lock`); the exact
command list, including the load average before and after each arm, is in
`arms/*/` and in the raw roots' `commands.json`.
