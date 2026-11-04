# Runtime

[Architecture](../../README.md) › [Core](../README.md) › [Telemetry](README.md) › Runtime

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Sessions, operations, observations and platform readings.

| | |
| --- | --- |
| Component | [Telemetry](README.md) |
| Source | [`core/crates/layerfs-telemetry/src/runtime`](../../../../core/crates/layerfs-telemetry/src/runtime), [`core/crates/layerfs-telemetry/src/platform`](../../../../core/crates/layerfs-telemetry/src/platform) |

`operation.rs` and `observation.rs` are always built. `session.rs`,
`monitor.rs` and `platform/` exist only with the `platform` cargo feature
([`runtime/mod.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/mod.rs),
[`Cargo.toml`](../../../../core/crates/layerfs-telemetry/Cargo.toml)). No
product crate enables that feature at HEAD.

## Sessions and operations

```text
 Configuration { enabled, timing, monitor, output, identity }
   |
   v
 Runtime::start
   enabled == false ----------------> Runtime::disabled()  (no allocation)
   | validate output, monitor, aggregate charge <= 8 MiB
   v
   Output::start -----------------+
   Monitor::start_with_output ----+ (stops output again on failure)
   OperationRecorder::new(256 nodes, 32 levels, 256 KiB; 8 slots, 4 MiB)
     .with_timing(timing) .with_windows(monitor.windows())
   |
   v
 runtime.recorder().run(key, label, op)
   no pool ............ Timing::disabled  --> Diagnostic::Disabled
   no free slot ....... Timing::disabled  --> Diagnostic::Omitted
   slot leased ........ window begin
                        Timing::record_with_limits or disabled
                        window finish, release
                        --> Diagnostic::Report(OperationReport)
   |
   v
 runtime.publish(diagnostic) --> encode_operation --> Output::submit
                                 (too big or omitted: Output::omit)
```

| Type | Role | Source |
| --- | --- | --- |
| `Configuration` | Caller-built settings; the library reads no environment | [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |
| `Runtime` | Lifetime owner of monitor, output and recorder; `Clone` shares them | [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |
| `OperationRecorder` | Pool of at most 32 recording slots, charged twice per slot, at most 4 MiB | [`operation.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/operation.rs) |
| `Diagnostic` | `Disabled`, `Omitted` or `Report`; never replaces the product result | [`operation.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/operation.rs) |
| `OperationReport` | Key, success, `TimingReport`, optional `Window`; holds its slot until dropped | [`operation.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/operation.rs) |

`OperationReport::resource_status` is `disabled` (no window source),
`omitted` (no window slot), `unavailable` (zero samples) or `sampled`. When the
runtime is dropped it stops the monitor, submits one `run-summary` line with
the loss counters and shuts output down with a 2 s allowance.

## Observations

```text
 Observation (one sample)           Window (one operation, constant size)
 +---------------------------+      +-------------------------------+
 | selected: bit0 CPU, bit1  |      | opened_ns, closed_ns          |
 |   RSS                     | ---> | first, last                   |
 | source: Supplied, Macos,  |      | sampled_max_rss               |
 |   Linux                   |      | samples, gaps, largest_gap_ns |
 | probe_ns, at_ns           |      | concurrency                   |
 | incarnation               |      | discontinuity                 |
 | user_ns, system_ns, rss   |      +-------------------------------+
 +---------------------------+                  | cpu_delta()
                                                v
                          Some((user, system)) only with no gaps,
                          no discontinuity and at least 2 samples
```

| Rule | Source |
| --- | --- |
| A `None` field with its selected bit set means unavailable; an unset bit means not requested. | [`observation.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/observation.rs) |
| Time going back, a changed incarnation, source or selection, or a falling CPU counter sets `discontinuity`. | `Window::observe` |
| `sampled_max_rss` is the largest sampled RSS, not an exact peak. | `Window` |
| `concurrency` is the most windows open at once; process samples include sibling work. | `Window`, [`monitor.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/monitor.rs) |
| `WindowSource` acquires without blocking and releases on return, error and unwind. | `WindowSource`, `WindowLease` in [`operation.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/operation.rs) |

## Monitor

```text
 thread "layerfs-observe" (256 KiB stack), every interval_ms
   sample = platform::sample(cpu, memory)     none --> failures += 1
   once per second: Output::submit(resource or resource-unavailable)
   for each active window slot (try_lock):  observe(sample), add gaps
   history ring (try_lock): ring[i] = sample
   park until next tick
```

| `MonitorConfig` field | Accepted | Source |
| --- | --- | --- |
| `cpu`, `memory` | both false: `Monitor::start` returns `Ok(None)`, no thread | [`monitor.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/monitor.rs) |
| `interval_ms` | 10..=1000 | same |
| `history` | 1..=600 ring slots | same |
| `windows` | 1..=32 window slots | same |

A window token is `generation × 64 + slot`. `finish` copies the summary only if
the generation still matches; `release` clears the slot. The sampler never
waits for a lock: a slot or ring it cannot lock misses that sample. Missed
ticks count as gaps. `latest()` returns the newest ring entry without waiting.
Dropping the monitor sets `stop`, unparks the thread and joins it only if it
has already finished.

## Platform readings

| Platform | `Source::as_str` | CPU | RSS | Incarnation | Source |
| --- | --- | --- | --- | --- | --- |
| Linux | `procfs+sysconf` | `/proc/self/stat` utime, stime ÷ `CLK_TCK` | rss × `PAGE_SIZE` | process start time | [`linux.rs`](../../../../core/crates/layerfs-telemetry/src/platform/linux.rs) |
| macOS | `getrusage+libproc` | `getrusage(RUSAGE_SELF)` µs × 1,000 | `ri_resident_size` | `ri_proc_start_abstime` | [`macos.rs`](../../../../core/crates/layerfs-telemetry/src/platform/macos.rs) |
| Other | — | — | — | — | `compile_error!` in [`platform/mod.rs`](../../../../core/crates/layerfs-telemetry/src/platform/mod.rs) |

The Linux reader reads at most 4,096 bytes of `/proc/self/stat` and parses
fields after the last `)`. On macOS, libproc's raw CPU fields are Mach ticks,
so CPU comes from `getrusage` instead. `probe_ns` is the wall time the sample
took; `at_ns` is time since the monitor started.

## Related

- [Timers](timers.md): what `OperationRecorder` records.
- [Output](output.md): where `publish` and the monitor send lines.
- [Glossary](../../glossary.md)

---

← [Telemetry: Timers](timers.md) · [Up: Telemetry](README.md) · [Telemetry: Output](output.md) →
