# Timers

[Architecture](../../README.md) › [Core](../README.md) › [Telemetry](README.md) › Timers

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Timing scopes, waits, limits and formats.

| | |
| --- | --- |
| Component | [Telemetry](README.md) |
| Source | [`core/crates/layerfs-telemetry/src/timer`](../../../../core/crates/layerfs-telemetry/src/timer) |

## Scopes and recording

```text
 Timing::record("object.create", |root| ...)          (one thread)
   |
   | Recording::start: root slot 0, clock starts
   v
 root: TimingScope<Active>
   |  root.child("canonical.construct")  --> TimingScope<Pending>
   |                                          (nothing recorded yet)
   |  pending.run(|c| ...)
   |     begin_child: fits? --no--> run closure unmeasured,
   |        |                       mark parent + ancestors incomplete
   |       yes
   |        v
   |     push slot, Instant::now()
   |     c: TimingScope<Active> --> c.child(..) / c.attach(report)
   |     closure returns Result<T, E>
   |     finish: elapsed, NodeOutcome::from_result
   |     return the same Result
   v
 Recording::complete --> TimingReport { root: Some(TimingNode) }
```

A recording is a flat slot arena behind a `RefCell`. Scopes hold node indexes,
so no borrow is held while a closure runs
([`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs)).

| Call | Effect | Source |
| --- | --- | --- |
| `Timing::record(name, op)` | Starts the root before `op`, returns `(Result<T, E>, TimingReport)` | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `Timing::record_with_limits(limits, ..)` | Same, under a validated `RecordingLimits` | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `Timing::disabled(name, op)` | Runs `op` with a disabled handle; no clock, no node, report without root | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `active.child(name)` | Pending child; on a disabled handle, a disabled pending scope | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `pending.run(op)` | Consumes the scope, times `op`, returns its `Result` unchanged | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `scope.is_recording()` | False when disabled or the child would not fit | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `active.attach(Option<TimingReport>)` | Grafts a completed report below this node | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |

### Node outcome and report completeness

```text
  NodeOutcome                          Completeness (TimingReport)
  Ok       closure returned Ok         Disabled  no root, nothing measured
  Error    closure returned Err        Complete  root, every requested node
  Unknown  never returned: panic or    Clipped   root, some detail missing
           scope dropped while running
```

| Case | Recorded as | Source |
| --- | --- | --- |
| Early return through `?` | Completed children kept; the failing node is `Error` | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| Panic in a closure | Slot still running at collection: `Unknown`, elapsed zero, incomplete | `Inner::collect` in [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Label over 128 bytes | Cut on a UTF-8 boundary; node incomplete | `Label::bounded` |
| `attach(None)` or a disabled report | Node and ancestors incomplete | `Recording::attach` |
| Attached report too large | Clipped to the remaining budget; affected nodes incomplete | `Inner::graft` |
| Attach to a finished node | Ignored | `Recording::attach` |

Elapsed time is inclusive and monotonic (`Instant`): a parent's time includes
its children. `TimingReport::is_incomplete` is false for a disabled report;
callers that must tell disabled from clipped use `completeness()`
([`report.rs`](../../../../core/crates/layerfs-telemetry/src/timer/report.rs)).

`TimingNode::new`, `push_child`, `with_children`, `with_outcome` and
`with_incomplete` build trees from decoded or synthetic data under the same
node and depth caps; `retained_bytes` reports owned capacity.

## Waits

```text
 thread T                         timer::waits (thread-local)
 --------                         TOTALS: [writer, reader, read_session] ns
 store-service timed("x")
   WaitMark::now() -------------> read totals
   ... admission queue wait ----> waits::add(Wait::Writer, d)
   ... read session lock wait --> waits::add(Wait::ReadSession, d)
   WaitMark::attach() ----------> totals now - totals then - inner reported
     own >= 50 us ? --> child node "wait.writer" / "wait.read_session"
```

| `Wait` | Node name | Added by |
| --- | --- | --- |
| `Writer` | `wait.writer` | Store writer admission queue, [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |
| `Reader` | `wait.reader` | Read-only Store connection admission queue, same file |
| `ReadSession` | `wait.read_session` | Daemon read session lock, [`read_handle.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_handle.rs) |

The crate only keeps saturating per-thread totals
([`waits.rs`](../../../../core/crates/layerfs-telemetry/src/timer/waits.rs)).
The 50 µs threshold and the subtraction of inner spans live in the engine
([`store-service telemetry.rs`](../../../../engine/crates/layerfs-store-service/src/telemetry.rs)).

## Limits

| Limit | Value | When exceeded | Source |
| --- | --- | --- | --- |
| `MAX_NODES` | 1,024 per report, root and attached nodes included | Child runs unmeasured; parent chain incomplete | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| `MAX_DEPTH` | 32 levels, root is level 1 | Same | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| `MAX_LABEL_BYTES` | 128 UTF-8 bytes | Label cut; node incomplete | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| `NODE_CHARGE` | 1,024 bytes per admitted node | — | [`limits.rs`](../../../../core/crates/layerfs-telemetry/src/timer/limits.rs) |
| `MAX_RECORDING_BYTES` | `MAX_NODES × NODE_CHARGE` | — | [`limits.rs`](../../../../core/crates/layerfs-telemetry/src/timer/limits.rs) |

`RecordingLimits::new(nodes, depth, bytes)` returns `None` unless
`1 ≤ nodes ≤ 1,024`, `1 ≤ depth ≤ 32` and
`NODE_CHARGE ≤ bytes ≤ MAX_RECORDING_BYTES`. Admitted nodes are
`min(nodes, bytes / NODE_CHARGE)`. The byte figure is a conservative
ownership charge, not a resident-memory bound. `Default` is 1,024 nodes and 32
levels.

## Formats and JSON

A timing tree example, as `write_text` prints it for
[`timer_nested.rs`](../../../../core/crates/layerfs-telemetry/examples/timer_nested.rs)
with an empty input (`<t>` stands for an elapsed time):

```text
object.create  <t> [error]
  canonical.construct  <t> [error]
    encode  <t> [error]
```

The same operation with valid input has `hash` below `canonical.construct`,
and `storage.save` with `pack` and `write` below the root.

| Writer | Output | Source |
| --- | --- | --- |
| `write_text` | Two spaces per level, label, two spaces, elapsed; ` [error]`, ` [unknown]`, ` [incomplete]` markers. Disabled: `disabled: no timing recorded`. | [`format.rs`](../../../../core/crates/layerfs-telemetry/src/timer/format.rs) |
| `write_json` | `{"name", "elapsed_ns", "children"}`; `"outcome"` only when not `ok`; `"incomplete": true` only when set. Disabled: `null`. | [`json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/json.rs) |
| `encode_bounded(max)` | Full JSON if it fits `max`; else the root alone, marked incomplete; else `None`. `max` over 1 MiB is an error. | [`bounded_json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/bounded_json.rs) |

| Text unit | Range |
| --- | --- |
| `ns` | below 1 µs |
| `us` | below 1 ms, three decimals |
| `ms` | below 1 s, three decimals |
| `s` | otherwise, three decimals |

Text labels escape `\`, newline, return, tab and other control characters as
`\u{..}`. JSON escapes per RFC 8259 control rules. `elapsed_ns` beyond `u64`
fails with `InvalidData` instead of saturating. Every writer error propagates.
`encode_bounded` first counts bytes into a sink that writes nothing, so it
never holds a partial encoding.

## Related

- [Runtime](runtime.md): `OperationRecorder` runs `Timing::record_with_limits` from a slot pool.
- [Output](output.md): how a report is embedded in an `LFT1` operation line.
- [Engine: Store calls](../../engine/store-calls/README.md): traced requests and the reply `timing` value.
- [Glossary](../../glossary.md)

---

← [Core: Telemetry](README.md) · [Up: Telemetry](README.md) · [Telemetry: Runtime](runtime.md) →
