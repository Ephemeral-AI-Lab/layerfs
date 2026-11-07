# Telemetry

[Architecture](../../README.md) › [Core](../README.md) › Telemetry

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Environment-independent parent/child timing trees, and how they are printed or written as JSON.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-telemetry` |
| Source | [`core/crates/layerfs-telemetry/src`](../../../../core/crates/layerfs-telemetry/src), [`Cargo.toml`](../../../../core/crates/layerfs-telemetry/Cargo.toml) |
| Uses | — |
| Used by | [Objects](../objects/README.md), [Files](../files/README.md), [Filesystem trees](../filesystem-trees/README.md), [Compare and merge](../compare-and-merge/README.md), [Engine: Store](../../engine/store/README.md), [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Engine: Workspace](../../engine/workspace/README.md), [Engine: Commit](../../engine/commit/README.md), [Engine: Daemon](../../engine/daemon/README.md), [Control: Management](../../control/management/README.md) |
| Flows | [A call, end to end](../../flows/call.md) |

The crate is `std`-only by default, depends on no product crate and opens no
file while recording ([`lib.rs`](../../../../core/crates/layerfs-telemetry/src/lib.rs)).
An optional cargo feature, `platform`, adds a process sampler and bounded
diagnostic output; no product crate enables it at HEAD.

## Topics

| Topic | What it covers |
| --- | --- |
| [Timers](timers.md) | Timing scopes, waits, limits and formats. |
| [Runtime](runtime.md) | Sessions, operations, observations and platform readings. |
| [Output](output.md) | Collecting, encoding and retaining telemetry output. |

## The picture

```text
 always built                         feature "platform" only
 +-----------------------------+      +-------------------------------+
 | Timers                      |      | Runtime (session.rs)          |
 |  Timing::record / disabled  |      |  Configuration --> Runtime    |
 |  TimingScope  child / run   |      |   |            |              |
 |  Recording (slot arena)     |      |   v            v              |
 |   |                         |      |  Monitor     OperationRecorder|
 |   v                         |      |  (sampler     (always built)  |
 |  TimingReport / TimingNode  |<-----|   thread)  -> Diagnostic      |
 |   |          |              |      |   |  Window      |            |
 |   v          v              |      +---|--------------|------------+
 | write_text  write_json      |          | resource     | publish
 | encode_bounded              |          v              v
 | waits (thread-local totals) |      +-------------------------------+
 +-----------------------------+      | Output                        |
                                      |  encode_* --> LFT1 line       |
                                      |  queue --> writer thread      |
                                      |    --> stderr  and/or         |
                                      |    --> local segment files    |
                                      |  Collector / Producer         |
                                      +-------------------------------+
```

The left box is what the product uses. Engine crates wrap it: for example
[`layerfs-store-service/src/telemetry.rs`](../../../../engine/crates/layerfs-store-service/src/telemetry.rs)
records roots only while a request is traced, and flattens them into the
reply.

## What it does

### Timers and timing trees

| Term | Meaning | Source |
| --- | --- | --- |
| Timing tree | One bounded tree per measured operation: a root node and the nodes its scopes created or attached. | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| `TimingScope<Pending>` | A child handed to a component; `run` starts its clock once. | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `TimingScope<Active>` | The handle inside `run`; the only source of further children and attachments. | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `TimingNode` | Owned label, inclusive monotonic elapsed time, `NodeOutcome`, incomplete flag, children in start order. | [`report.rs`](../../../../core/crates/layerfs-telemetry/src/timer/report.rs) |
| `TimingReport` | An optional root: `None` when recording was disabled. | [`report.rs`](../../../../core/crates/layerfs-telemetry/src/timer/report.rs) |

### Where the trees surface

| Surface | How | Source |
| --- | --- | --- |
| Traced call results | A traced request turns recording on for its thread; finished roots (at most 16) are flattened to at most 256 pre-order nodes `[name, parent, microseconds, outcome, incomplete]` plus an `omitted` count. | [`store-service telemetry.rs`](../../../../engine/crates/layerfs-store-service/src/telemetry.rs) |
| Daemon replies | The daemon adds the same `timing` value next to SQL work, delta counters and kernel opcode counts. | [`daemon telemetry.rs`](../../../../engine/crates/layerfs-daemon/src/telemetry.rs) |
| Wait nodes | Store admission and read sessions add queued time to thread-local totals; a timed span reports its unreported share as a `wait.*` node when it is at least 50 µs. | [`waits.rs`](../../../../core/crates/layerfs-telemetry/src/timer/waits.rs), [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |
| Text and JSON | `write_text`, `write_json` and `encode_bounded` on any `std::io::Write`. | [`format.rs`](../../../../core/crates/layerfs-telemetry/src/timer/format.rs), [`json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/json.rs) |
| `LFT1` diagnostic lines | `platform` feature only: operation, resource and run-summary records to stderr or local segments. | [`encode.rs`](../../../../core/crates/layerfs-telemetry/src/output/encode.rs) |

Counters such as SQL work and delta selection are not part of this crate; the
engine reports them beside the timing tree.

## Invariants

| Rule | Enforced by |
| --- | --- |
| A measured operation's `Result` is returned unchanged; timing never fails it. | `TimingScope::run`, `Timing::record` in [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| Disabled timing reads no clock and creates no node; the report has no root. | `Timing::disabled` in [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| Turning telemetry off allocates nothing, even with an invalid configuration. | `Runtime::start` and `Runtime::disabled` in [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs); test [`off_allocations.rs`](../../../../core/crates/layerfs-telemetry/tests/off_allocations.rs) |
| A child cannot outlive its parent's measured region or cross threads. | Lifetimes and a `*const ()` marker in [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs); [`tests/compile_fail/`](../../../../core/crates/layerfs-telemetry/tests/compile_fail) |
| A node that never returned reports `Unknown`, never `Ok`. | `Inner::collect` in [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Missing detail marks the node and every ancestor incomplete. | `mark_incomplete`, `graft` in [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| A stalled output never blocks the measured work. | Nonblocking `try_lock` submit and a separate writer thread in [`queue.rs`](../../../../core/crates/layerfs-telemetry/src/output/queue.rs); test [`stalled_output.rs`](../../../../core/crates/layerfs-telemetry/tests/stalled_output.rs) |
| The library reads no environment variable or global configuration. | [`lib.rs`](../../../../core/crates/layerfs-telemetry/src/lib.rs), `Configuration` in [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Nodes per report | `MAX_NODES` = 1,024, root and attached nodes included | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Levels per report | `MAX_DEPTH` = 32 | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Label bytes | `MAX_LABEL_BYTES` = 128, cut on a UTF-8 boundary | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Recording charge | `NODE_CHARGE` = 1,024 bytes per node; `MAX_RECORDING_BYTES` = 1,024 × 1,024 | [`limits.rs`](../../../../core/crates/layerfs-telemetry/src/timer/limits.rs) |
| Over a node or depth limit | Child runs unmeasured; node and ancestors marked incomplete | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Elapsed beyond `u64` nanoseconds | JSON writer returns `InvalidData` | [`json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/json.rs) |
| `encode_bounded` above 1 MiB | `io::Error` "encoded report hard limit" | [`bounded_json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/bounded_json.rs) |
| Full disk or removed output directory | Local destination is dropped, `failed` is counted, product result unchanged | [`queue.rs`](../../../../core/crates/layerfs-telemetry/src/output/queue.rs); test [`full_disk.rs`](../../../../core/crates/layerfs-telemetry/tests/full_disk.rs) |
| Platform aggregate ownership | Over 8 MiB: `io::Error` "aggregate telemetry ownership" | [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |
| Platform build target | `compile_error!` on anything but Linux or macOS | [`platform/mod.rs`](../../../../core/crates/layerfs-telemetry/src/platform/mod.rs) |

## Demonstration

| Test or example | Crate | Shows |
| --- | --- | --- |
| [`timer.rs`](../../../../core/crates/layerfs-telemetry/tests/timer.rs) | `layerfs-telemetry` | Nesting, start order, early return, recovery, inclusive time, disabled vs clipped, panics, clipping, attachment |
| [`timer_json.rs`](../../../../core/crates/layerfs-telemetry/tests/timer_json.rs) | `layerfs-telemetry` | JSON shape, escaping and round trip through an independent parser, writer and flush failures, no-clobber saving |
| [`timer_nested.rs`](../../../../core/crates/layerfs-telemetry/examples/timer_nested.rs) | `layerfs-telemetry` | One component pair run measured, with an early error, and disabled |
| [`off_allocations.rs`](../../../../core/crates/layerfs-telemetry/tests/off_allocations.rs) | `layerfs-telemetry` | Master-off runtime makes zero allocations |
| [`stalled_output.rs`](../../../../core/crates/layerfs-telemetry/tests/stalled_output.rs) | `layerfs-telemetry` | A blocked stderr does not change the result or stop process exit |

- **`timer.rs`** builds trees with `Timing::record` over a local `TestError`
  and checks child names, `NodeOutcome`, `Completeness`, label clipping and the
  node and depth budgets. Any platform, no feature.
  `cargo +1.85.1 test --locked -p layerfs-telemetry --test timer`
- **`timer_json.rs`** renders reports and pipes them to `python3 -c` for an
  independent JSON parse; it writes temporary files under the system temp
  directory. Needs `python3` on `PATH`.
  `cargo +1.85.1 test --locked -p layerfs-telemetry --test timer_json`
- **`timer_nested`** prints a text tree, its JSON, the early-return tree and a
  disabled run (`null`). Any platform.
  `cargo +1.85.1 run --locked -p layerfs-telemetry --example timer_nested`
- **`off_allocations.rs`** installs a counting global allocator, starts a
  `Runtime` with `enabled: false` and out-of-range monitor values, and asserts
  zero allocations. Needs the feature (`#![cfg(feature = "platform")]`), Linux
  or macOS.
  `cargo +1.85.1 test --locked -p layerfs-telemetry --features platform --test off_allocations`
- **`stalled_output.rs`** spawns its own ignored helper with piped, unread
  stderr, submits 256 records of 4 KiB and checks the child exits within 3 s
  with `failed = 1`. Needs the feature, Linux or macOS.
  `cargo +1.85.1 test --locked -p layerfs-telemetry --features platform --test stalled_output`

These commands are written from source and were not run while writing this
page.

## Earlier material

Folded into these pages. Matched by title, so check each one before relying on it.

- [`core/docs/architecture/10-counters.md`](../../../../core/docs/architecture/10-counters.md) — its §15.8 "Timer composition" (outcomes and completeness) is covered in [Timers](timers.md); the counter inventory belongs to the engine and storage pages, and the rest is historical only.
- [`core/docs/architecture/36-operation-cost-observations.md`](../../../../core/docs/architecture/36-operation-cost-observations.md) — overlay statement and startup observations, not this crate; historical only here (see [Engine: Overlay](../../engine/overlay/README.md)).

---

← [History model: Allocation](../history-model/allocation.md) · [Up: Core](../README.md) · [Telemetry: Timers](timers.md) →
