# Stage One — 100 MiB Operation Campaign

Authority: [12-stage1-performance-completion.md](12-stage1-performance-completion.md)
Purpose: isolate read, write, edit, reconstruction, materialization, refresh,
reopen, history, and resource ownership through product SDK/VFS paths.

## 1. Budget

```text
file maximum                104,857,600 bytes exactly
preferred campaign wall     < 60 seconds
hard diagnostic stop        <= 120 seconds
one release executable      yes
network                     forbidden
large source in repository  no
adaptive reruns             no
```

Expected fixed schedule: 3 samples for byte-linear/heavy cases, 300 random
ranges, 11 reopens, one 4-revision sequence, three locality sentinels.

## 2. Prepared master

```text
target/layerfs-stage1-fixtures/single-100m-v1/
├── bases/
│   ├── read-reconstruct/
│   ├── import-genesis/
│   ├── replace-existing/
│   ├── overwrite/
│   ├── insert/
│   ├── delete/
│   ├── append/
│   ├── truncate/
│   ├── refresh-a-b/
│   └── history/
├── input/S1-100.bin
├── input/S1-replace-100.bin
└── master.json
```

Reuse the preserved Phase-4 `S1-100` byte generator, copied as a minimal
evaluator fixture helper rather than importing its historical harness:

```text
source helper  implementation-detail/phase-4/preserved-benchmark-sources/
               phase4_create_edit_benchmark.rs::fill_retained_buffer
size           104,857,600
seed           0x51
raw BLAKE3     bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7
CDC references 5,284
CDC sequence   5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994
```

This preserves direct G4/G5 population comparability without reusing their
runner, schema, database, thresholds, or benchmark-private product code. The
generator streams 1 MiB buffers. `master.json` binds:

```text
generator version + seeds
exact size and digest
StoreId/profile
root IDs
extent count + CDC sequence fingerprint
file mode/mtime
complete inventory digest
```

`S1-replace-100.bin` is the bytewise `S1-100 XOR 0xA5` streaming transform,
also exactly 100 MiB and independently sealed. It is not admitted to the
`replace-existing` Store before A03b.

Prepared input roots:

| Root | Size | Use |
|---|---:|---|
| `R100` | `104,857,600` | read/reconstruct/materialize/delete/truncate |
| `Rinsert_base` | `104,849,408` | 8 KiB insert ends exactly at 100 MiB |
| `Rappend_base` | `104,853,504` | 4 KiB append ends exactly at 100 MiB |
| `Roverwrite` | `104,857,600` | 4 KiB middle child for refresh |
| `Rempty` | `0` | streamed import/write |
| `Rhistory_0` | `104,857,600` | base for four measured retained revisions |

No input, intermediate user-data, or native output regular file may exceed
100 MiB. SQLite Store authority files are excluded from that user-file ceiling
and are reported separately. The preserved campaign observed a maximum Store
database above 100 MiB; this is Store amplification evidence, not a user-file
ceiling PASS.

Each operation base contains only the authority required before that
operation. A measured output is not preinserted in its base Store. The sole
exception is `refresh-a-b`: both retained A and B must already exist because
refresh projects an accepted immutable target rather than publishing it.
Per-case bases are prepared as APFS clones so their physical fixture storage
shares unchanged blocks.

The insert/append bases are imported in separate Stores from their shorter
input streams; those Stores do not retain `R100`. Thus an insert/append result
cannot take a preexisting-CAS favorable path merely because its bytes equal
the full fixture.

## 3. Ultra-fast preparation/reset

One-time `layerfs-eval stage1 prepare single-file`:

```text
generate input streams once -> prepare each case base once -> close all
-> verify complete inventory/root/digest -> seal 0444/0555
```

Repeated campaign admission:

```text
preferred <= 2 s
verify sealed manifest + selector + StoreId + profile + root IDs
full master digest at campaign start/end only
```

Per operation sample reset:

```text
exclusive new attempt directory
-> /bin/cp -cR sealed APFS master
-> prove clone return + distinct destination inode
-> make attempt writable
-> validate CURRENT/StoreId/root/sizes
```

Targets:

```text
preferred reset <= 2 s
hard reset      <= 5 s
```

No per-reset full 100 MiB rehash; it would prewarm and dominate the sample.
Evidence label:

```text
APFSCloneReturnPlusSealedMasterCustodyNotPerResetFullRehash
```

If APFS clone reset is unavailable, fail readiness. Do not silently copy and
call it fast reset.

Exact attempt topology:

```text
fixed Store resets              54
A02 random reads                3 resets × 100 ranges inside each reset
A13 reopen                      1 reset × 11 close/open cycles
A10/A11/A12                     3 resets; cold materialize -> no-op
                                -> move main to exact B RefState -> refresh B
A17 managed reuse               1 reset; one materialization + 100 checkpoints
all other heavy/arm observations one operation sample per reset
```

Before measured rows, a zero-row forecast uses the observed clone/reset
receipt and all fixed counts:

```text
forecast_reset_wall = 54 × reset_upper_ns
forecast_campaign_wall = reset + operation estimates + postchecks + cleanup
preferred reset reserve <= 15 s total
hard campaign forecast  <= 120 s
```

If infeasible, repair reset/preparation before measurement; never reduce the
population after rows exist.

## 4. Frozen operations

| ID | Operation | Population | Product route | Primary metric |
|---|---|---:|---|---|
| A01 | Sequential ranges | `3 × 100 calls × 1 MiB` | SDK canonical `read_range` | MiB/s |
| A02 | Random read | `300 × 64 KiB` | SDK canonical range | p50/p95 latency |
| A03a | Streamed new-file import | `3 × 100 MiB` | genesis/new path | MiB/s |
| A03b | Streamed full-file replacement | `3 × 100 MiB` | replace existing `R100` path | MiB/s |
| A04 | Same-size edit | `3 × 4 KiB × 2 arms` | direct logical; retained managed native | split timers |
| A05 | Insert | `3 × 8 KiB × 2 arms` | direct logical; managed native fallback | split timers |
| A06 | Delete | `3 × 4 KiB × 2 arms` | direct logical; managed native fallback | split timers |
| A07 | Append | `3 × 4 KiB × 2 arms` | direct logical; managed native append | split timers |
| A08 | Truncate | `3 × 4 KiB × 2 arms` | direct logical; managed native truncate | split timers |
| A09 | Logical reconstruction | `3 × 100 MiB` | SDK canonical stream to digest | MiB/s |
| A10 | Cold managed materialization | `3 × 100 MiB` | `materialize_managed(R100)` | MiB/s |
| A11 | Managed exact no-op | 3 paired after A10 | retained managed authority | latency/zero work |
| A12 | Changed-root refresh | `3 × 4 KiB` changed | `R100 -> Roverwrite` | latency/route bytes |
| A13 | Reopen/head | 11 | SDK open to exact head | p50/p95 |
| A14 | History | 4 durable revisions | direct root reads | growth/read exactness |
| A15 | Locality sentinels | early/middle/late 4 KiB | direct logical edit | structural counters |
| A16 | Terminal resources | 1 | process/Store cleanup | bounds/zero |
| A17 | Managed reuse | `100 edit→checkpoint` | one retained managed workspace | lifecycle/resources |

Edit operands:

| Shape | Offset | Delete | Replacement | Result |
|---|---:|---:|---:|---:|
| overwrite | `F/2 - 2048` | `4096` | `4096` | `F` |
| insert | `F/2 - 4096` | `0` | `8192` | `F` from `Rinsert_base` |
| delete | aligned `2F/3` | `4096` | `0` | `F-4096` |
| append | `F-4096` | `0` | `4096` | `F` from `Rappend_base` |
| truncate | `F-4096` | `4096` | `0` | `F-4096` |

Expected bytes come from an evaluator-owned splice of the deterministic byte
function. LayerFS never produces its own oracle.

For A04–A08, the two arms start from equivalent sealed roots and use identical
operands:

```text
logical arm  SDK direct canonical edit; native work must be zero
native arm   one retained managed workspace; native edit + checkpoint
```

Never subtract the native arm from the logical arm or merge their counters.

Native-arm schedule for each operand/sample:

```text
select exact prepared base root and RefState
-> first case: materialize managed once
-> later case: move main to exact base RefState, then refresh to that base
-> time refresh separately from edit
-> native edit + durable checkpoint
-> exact postcheck
```

Any refresh/rematerialization remains in complete wall and counters. The edit
timer starts only after the managed workspace and named ref are aligned.

A17 performs 100 sequential bounded edits. Every edit is followed by an
acknowledged or freshly reconciled durable checkpoint, descriptor/spool reset,
and returned `Live(new_ref_state)`; it requires exactly 100 state-changing
transactions/COMMITs, one initial materialization, zero rematerializations,
selected old-root reads, exact terminal root, and terminal Q/temp zero.

## 5. Timing boundaries

Every sample:

```text
complete_sample_wall
  = reset
  + open/select
  + managed_prepare
  + operation
  + checkpoint
  + exact_postcheck
  + cleanup
  + timer_residual
```

Every managed edit additionally reports:

```text
logical_edit_wall
native_edit_wall
durable_checkpoint_wall
edit_plus_checkpoint_wall
managed_prepare_wall
```

Do not call `edit_plus_checkpoint` an edit-call latency. Do not hide workspace
preparation outside complete wall.

Cache labels:

```text
cold-destination
reopened-cache-unknown
same-open-warm-or-unknown
```

Never label an OS cache state cold without controlling it.

## 6. G4/G5 structural gates

```text
campaign_complete_wall == sum(all phases) + timer_residual
operation_wall         == attributed + unattributed

fetched_rows
  == fetched_row_authentication_passes
  == fetched_row_role_decode_passes
new_object_authentication_passes reported separately
incumbent_authentication_passes reported separately
payload_batch_maximum  <= 64

state change: writer_transactions = 1, publication_commits = 1
normalized no-op: writer_transactions = 0, publication_commits = 0

cdc_bytes_scanned <= replacement_input_bytes
unaffected_suffix_payload_reads  = 0
unaffected_suffix_payload_writes = 0
content edit directory_nodes_emitted = 0

managed no-op:
  payload bytes = native bytes = CDC bytes = COMMITs = 0

Q_high_water <= 8 MiB
largest_buffer <= 1 MiB
Q_terminal = 0
FD/connection/temp terminal = baseline/zero
```

Native middle insert/delete:

```text
S = F - P - delete
shift route application transfer = 2S + replacement
full fallback is labeled separately
```

No native suffix cost may be charged to the canonical extent-tree algorithm.

## 7. Correctness gates

| Case | PASS |
|---|---|
| Read | exact length and independent digest/range bytes |
| New import | streamed input; exact new path/root; one transaction/COMMIT |
| Full replace | streamed input; exact reused path/inode policy; one transaction/COMMIT |
| Logical edit | exact child; old root unchanged; local structural work |
| Reconstruction | exactly 100 MiB; exact digest; no native file |
| Cold materialize | exact native bytes/metadata; honest linear route |
| No-op | exact root and literal zero payload/native/CDC/write work |
| Refresh | main aligned to target RefState; exact target; changed ranges/fallback explicit |
| Reopen | exact head; no duplicate clean engine open |
| History | revisions 0/1/2/4 directly readable; no replay |
| Managed reuse | 100 durable checkpoints; one materialization; zero rematerializations |
| Resources | terminal zero/baseline and sealed master unchanged |

## 8. Statistics

```text
for one-based ordered values x1 <= ... <= xn:
p50 odd n  = x[(n+1)/2]
p50 even n = floor((x[n/2] + x[n/2+1]) / 2)
p95 rank   = max(1, ceil(0.95*n)); p95 = x[rank]
```

| Population | n | p50 | p95 |
|---|---:|---:|---:|
| Heavy operation | 3 | `x2` | `x3` (diagnostic maximum) |
| Reopen | 11 | `x6` | `x11` |
| Random ranges | 300 | mean-floor `x150,x151` | `x285` |

Always retain raw sorted values, minimum, maximum, range, p50, p95, and
throughput. Three-sample p95 is not an SLO or significance claim.

## 9. Target table

| Case | Target |
|---|---:|
| Sequential/range read | `>=250 MiB/s` at 1 MiB sequential request class |
| Random 64 KiB | p50 `<=0.5 ms`, p95 `<=1.0 ms` planning target |
| Streamed import and full replace | each `>=150 MiB/s` |
| Logical reconstruct | `>=200 MiB/s` |
| Trusted 4 KiB logical edit | p50 `<=15 ms` |
| Reopen | p50 `<=4 ms` |
| Cold native 100 MiB | `>=150 MiB/s` |
| Managed exact no-op | p50 `<=5 ms`, zero payload/native work |
| Same-size A→B 4 KiB refresh | p50 `<=25 ms` |
| Complete campaign | preferred `<60 s`, hard `<=120 s` |

The random-read limits are first-run planning targets. If they fail while the
structural complexity passes, preserve the failure and attribute the owner;
do not weaken them after observation.

## 10. Complete-wall forecast

| Component | Prospective range |
|---|---:|
| 54 APFS resets | 5–15 s |
| sequential/random/reconstruct | 3–7 s |
| import + full replace | 4–8 s |
| ten edit arms | 8–18 s |
| cold/no-op/refresh | 4–10 s |
| reopen/history/locality/100 checkpoints | 4–9 s |
| oracles/cleanup/artifacts | 4–8 s |
| Expected total | **32–75 s** |
| Preferred target | **<60 s** |
| Hard stop | **120 s** |

The range is not readiness proof. The zero-row receipt must replace its reset
component with observed fixed-count arithmetic and demonstrate adequate
reserve below 120 seconds before any product row.

## 11. Artifacts

```text
<run>/
├── environment.json
├── master.json
├── schedule.json
├── rows.jsonl
├── summary.json
├── summary.md
├── campaign-time.txt
└── stderr.txt              only when nonempty/failure
```

One evaluator process writes one row schema and one summary. No Python runner,
Criterion suite, second analyzer, global benchmark lock, or historical G4/G5
harness copy.
