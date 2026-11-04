# Output

[Architecture](../../README.md) › [Core](../README.md) › [Telemetry](README.md) › Output

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Collecting, encoding and retaining telemetry output.

| | |
| --- | --- |
| Component | [Telemetry](README.md) |
| Source | [`core/crates/layerfs-telemetry/src/output`](../../../../core/crates/layerfs-telemetry/src/output) |

The whole `output` module exists only with the `platform` cargo feature
([`lib.rs`](../../../../core/crates/layerfs-telemetry/src/lib.rs)); no product
crate enables it at HEAD. Delivery is best effort: a record may be dropped, and
a failure never reaches the product result.

## Collector and queue

```text
 producers (any thread)                    writer thread "layerfs-output"
 ----------------------                    ------------------------------
 Runtime::publish --+
 Monitor (1/s) -----+--> Output::submit    loop:
 Producer::submit --+     closed? too big?    pop front (Mutex)
   (via Collector)        --> dropped += 1    token bucket: rate, burst
                          try_lock fails      cost = len x destinations
                          --> dropped += 1    no tokens --> dropped += 1
                          full: drop oldest   Forward --> stderr
                          --> dropped += 1    Local ----> segment file
                          push back, notify   a failed route is turned off
                                              for good, failed += 1
```

| `OutputConfig` field | Accepted | `forward()` default | Source |
| --- | --- | --- | --- |
| `mode` | `Forward`, `Local`, `Both` | `Forward` | [`queue.rs`](../../../../core/crates/layerfs-telemetry/src/output/queue.rs) |
| `directory` | required for `Local`/`Both`; path capacity ≤ 4,096 | none | same |
| `record_bytes` | 1,024..=16,384 | 16,384 | same |
| `queue_bytes` | `record_bytes`..=8 MiB | 2 MiB | same |
| `queue_count` | 1..=256 | 128 | same |
| `rate` | 1..=1 MiB per second, all destinations | 65,536 | same |
| `burst` | `record_bytes` (× 2 for `Both`)..=1 MiB | 262,144 | same |
| `segments` | 1..=16 | 4 | same |
| `segment_bytes` | `record_bytes`..=16 MiB | 16 MiB | same |
| `expiry` | nonzero | 86,400 s | same |

`submit` charges a record's `Vec` capacity, not its length, against
`queue_bytes`. The record being written still counts against `queue_count`.
Forwarding writes 512-byte chunks to stderr after `poll` for `POLLOUT` in
50 ms steps, with a 1 s deadline per record. `shutdown(allowance)` closes the
queue, waits at most `min(allowance, 2 s)` for the writer, then sets `stop`;
records still queued are counted as dropped.

`Collector::new(producers, output)` admits 1..=16 producers. `connect` returns
a `Producer` or `None` at once. `Producer::submit` accepts only an envelope of
at most 16,384 bytes that starts with `LFT1 {` and ends with `}\n`; it is kept
as opaque bytes. A queued record holds its producer's slot until written
([`collector.rs`](../../../../core/crates/layerfs-telemetry/src/output/collector.rs)).

## Encoding

Every record is one line: the prefix `LFT1 `, one JSON object, `\n`.

```text
 LFT1 {"v":1,"namespace":N,"run":"<32 hex>","pid":P,"role":R,
       "kind":"operation","key":K,"timing":<encode_bounded tree>,
       "success":B,"resource_status":"...",[window fields]}\n
```

| `kind` | Written by | Fields beyond identity | Source |
| --- | --- | --- | --- |
| `operation` | `encode_operation` | `key`, `timing`, `success`, `resource_status`; with a window: `samples`, `gaps`, `concurrency`, `cpu_shared_ns`, `scope`, `clock`, `first_ns`, `opened_ns`, `closed_ns`, `last_ns`, `incarnation`, `largest_gap_ns`, `source`, `last_probe_ns`, `selected`, `sampled_max_rss` | [`encode.rs`](../../../../core/crates/layerfs-telemetry/src/output/encode.rs) |
| `resource` | `encode_resource`, monitor once per second | `scope`, `clock`, `at_ns`, `incarnation`, `user_ns`, `system_ns`, `rss`, `source`, `probe_ns`, `selected` | same |
| `resource-unavailable` | monitor, when a sample failed | `source`, `selected`, `failures`, `overflow` | [`monitor.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/monitor.rs) |
| `run-summary` | `Runtime` drop | `dropped`, `failed`, `overflow`, `"delivery":"best-effort"` | [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |

`Identity` is `run` (u128), `pid`, `role` (1 service, 2 daemon, 3 collector)
and `namespace`; a PID alone is never an incarnation. `encode_operation`
requires a cap of 1,024..=16,384 bytes. It writes the fixed fields first, gives
the timing tree the remaining room through `encode_bounded`, and strips its
newlines. If even the root does not fit, it returns `None` and the runtime
counts the record as dropped.

## Retention

```text
 <directory>/                      mode 0700, no group/other bits
   .layerfs-operational-v1         "layerfs operational telemetry v1\n
                                    <segments> <segment_bytes>\n"
   .writer-lock                    flock, exclusive, nonblocking
   segment-00.jsonl                mode 0600, <= segment_bytes each
   segment-01.jsonl
   ...                             ring of `segments` files
```

| Step | Behaviour | Source |
| --- | --- | --- |
| Open | Create the directory or reuse it only if the marker matches this profile; take the writer lock; refuse a non-file or oversized segment | [`retention.rs`](../../../../core/crates/layerfs-telemetry/src/output/retention.rs) |
| Start slot | First empty slot, else the oldest | same |
| Write | Rotate to the next slot when the record does not fit; delete what that slot held; create the file with `create_new` | same |
| Expire | Before each write and on each idle wake of the writer (at most 1 s apart): delete segments older than `expiry` | same |

A failed segment write or maintenance step turns the local destination off for
the rest of the output's life and counts one failure. There is no fallback
spool. No `fsync` is made.

## Health

| Counter | Counts | Source |
| --- | --- | --- |
| `Loss::dropped` | Records refused, evicted, rate-limited, unwritten at stop, or omitted | [`queue.rs`](../../../../core/crates/layerfs-telemetry/src/output/queue.rs) |
| `Loss::failed` | Destination write or maintenance failures | same |
| `Loss::overflow` | At least one counter saturated at `u64::MAX` | [`health.rs`](../../../../core/crates/layerfs-telemetry/src/output/health.rs) |
| `Monitor::failures` | Samples the platform could not read; never reported as zero CPU or memory | [`monitor.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/monitor.rs) |

`Output::loss` is a snapshot of these counters, not a promise that a record
arrived. On a full disk the local write fails once, the destination is turned
off and the measured operation keeps its result
([`full_disk.rs`](../../../../core/crates/layerfs-telemetry/tests/full_disk.rs),
`#[ignore]`, needs `LAYERFS_TEST_DISK_ROOT` on a dedicated 8–64 MiB
filesystem).

## Related

- [Timers](timers.md): `encode_bounded`, which sizes the timing field.
- [Runtime](runtime.md): `Runtime::publish` and the monitor, the two internal producers.
- [Glossary](../../glossary.md)

---

← [Telemetry: Runtime](runtime.md) · [Up: Telemetry](README.md) · [Engine](../../engine/README.md) →
