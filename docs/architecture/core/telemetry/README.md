# Telemetry

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Bounded parent/child timing trees that never change a result, how components
record into them, and the optional platform sampler and diagnostic output.

| | |
| --- | --- |
| Layer | [Core](../README.md) |
| Crates | `layerfs-telemetry` |
| Source | [`timer/`](../../../../core/crates/layerfs-telemetry/src/timer), [`runtime/`](../../../../core/crates/layerfs-telemetry/src/runtime), [`output/`](../../../../core/crates/layerfs-telemetry/src/output), [`platform/`](../../../../core/crates/layerfs-telemetry/src/platform), [`Cargo.toml`](../../../../core/crates/layerfs-telemetry/Cargo.toml) |
| Uses | — (std only by default) |
| Used by | [Files](../files/README.md), [Filesystem trees](../filesystem-trees/README.md), [Physical storage](../physical-storage/README.md), [Engine: Store](../../engine/store/README.md), [Engine: Store calls](../../engine/store-calls/README.md), [Engine: Project Init](../../engine/project-init/README.md), [Engine: Commit](../../engine/commit/README.md), [Engine: Daemon](../../engine/daemon/README.md), [Engine: Kernel routes](../../engine/kernel-routes/README.md), [Control: Stores and projects](../../control/stores-and-projects/README.md), [API: Calls](../../api/calls/README.md) |
| Flows | [A call, end to end](../../flows/call.md) |

## Picture

```text
 always built                          feature "platform" only
 +------------------------------+      +--------------------------------+
 | timer/                       |      | runtime/session.rs, monitor.rs |
 |  Timing::record / disabled   |      |  Configuration --> Runtime     |
 |  TimingScope child / run     |      |  Monitor (sampler thread)      |
 |  Recording (slot arena)      |      |  OperationRecorder --> Window  |
 |  TimingReport / TimingNode   |<-----|  --> Diagnostic                |
 |  write_text / write_json /   |      +---------------|----------------+
 |  encode_bounded              |                      v publish
 |  waits (thread-local totals) |      +--------------------------------+
 +------------------------------+      | output/: LFT1 line --> queue   |
 runtime/operation.rs and              |  --> writer thread --> stderr  |
 observation.rs are always built       |      and/or local segments     |
                                       +--------------------------------+
```

No product crate enables the `platform` feature; the product uses the left
box. The library reads no environment variable and opens no file while
recording ([`lib.rs`](../../../../core/crates/layerfs-telemetry/src/lib.rs)).

## What it does

### Timing trees

```text
 let (result, report) = Timing::record("object.create", |root| {
     let object = construct(input, root.child("canonical.construct"))?;
     save(object, root.child("storage.save"))
 });
 fn construct(input: &[u8], scope: TimingScope<'_>) -> Result<Object, E> {
     scope.run(|c| {
         let bytes = c.child("encode").run(|_| encode(input))?;
         let id = c.child("hash").run(|_| identify(&bytes))?;
         Ok(Object { id, bytes })
     })
 }
```

| Call | Effect | Source |
| --- | --- | --- |
| `Timing::record(name, op)` | starts the root before `op`; returns `(Result<T, E>, TimingReport)` with the caller's own `Result` | [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| `Timing::record_with_limits` | the same under validated `RecordingLimits` | same |
| `Timing::disabled(name, op)` | runs `op` with no clock read and no node; the report has no root (JSON `null`) | same |
| `active.child(name)` → `pending.run(op)` | a pending child; `run` consumes it, starts its clock once, returns `op`'s `Result` unchanged | same |
| `scope.is_recording()` | false when disabled or the child would not fit | same |
| `active.attach(Option<TimingReport>)` | grafts a completed report below this node; on a disabled scope a no-op | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |

A recording is a flat slot arena behind a `RefCell`; scopes hold indexes, so
no borrow is held while a closure runs and a caught panic leaves it usable.
Scope handles are neither `Send` nor `Sync`, and lifetimes keep a child inside
its parent's measured region (checked by `tests/compile_fail/`).

| Case | Recorded as |
| --- | --- |
| Success / closure returned `Err` | `NodeOutcome::Ok` / `Error`; completed children kept after an early `?` |
| Panic, or scope dropped while running | `Unknown`, elapsed zero, incomplete; no report is fabricated |
| Repeated label | distinct invocations in start order |
| Label over 128 bytes | cut on a UTF-8 boundary; node incomplete |
| `attach(None)`, a disabled or clipped report | node and ancestors incomplete; the result is unchanged |
| Over a node or depth limit | the child runs unmeasured; node and ancestors incomplete |

Elapsed time is inclusive and monotonic (`Instant`): a parent includes its
children; no self time or CPU time is derived. `Completeness` is `Disabled`
(no root), `Complete` or `Clipped`; outcome and incompleteness are independent.

### Instrumenting a component

| Role | Owns | Uses |
| --- | --- | --- |
| Operation owner (SDK entry, daemon request handler, test runner) | one root per semantic operation, whether recording is on, where the report goes | `Timing::record` / `disabled`, `write_text`, `write_json` |
| Component (content, storage, workspace …) | its own labelled steps only; never starts a recording, picks a path or reads configuration | a `TimingScope<'_>` parameter, `run`, `child`, `is_recording`, `attach` |
| Adapter or peer | carrying an independently recorded report back with the ordinary result | returns `Option<TimingReport>`; the caller attaches it |

Rules: labels are stable, dotted and low-cardinality (`object.create`, never
per-file or per-request); ask a peer for timing only when `is_recording()`;
attach before propagating its result; add no telemetry-only round trip;
start one root per operation, never one per Workspace lifetime; never store a
scope or hold one across an `await` or thread; tests assert names, order,
outcomes and flags, not wall-clock thresholds. A disabled run does all product
work and changes no result.

### Waits

| `Wait` | Node | Added by |
| --- | --- | --- |
| `Writer` | `wait.writer` | Store writer admission, [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |
| `Reader` | `wait.reader` | read-only Store connection admission, same file |
| `ReadSession` | `wait.read_session` | daemon read-session lock, [`read_handle.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_handle.rs) |

The crate keeps only saturating per-thread totals
([`waits.rs`](../../../../core/crates/layerfs-telemetry/src/timer/waits.rs)).
The engine subtracts what inner spans reported and adds a `wait.*` child when
the remainder is at least 50 µs.

### Who records

| Root | Records | Source |
| --- | --- | --- |
| Store-call phases | a traced request turns recording on for its thread (`Recording` guard, off again on every exit); product roots call `timed`, which is `Timing::disabled` when untraced; at most 16 finished roots flattened to 256 pre-order nodes `[name, parent, µs, outcome, incomplete]`, plus SQL work per phase and delta counters | [`layerfs-store-service/src/telemetry.rs`](../../../../engine/crates/layerfs-store-service/src/telemetry.rs) |
| Daemon reply | the daemon's half of a traced reply: Commit storage and construction receipts and kernel opcode counts, beside the Store-call `timing` | [`layerfs-daemon/src/telemetry.rs`](../../../../engine/crates/layerfs-daemon/src/telemetry.rs) |
| Daemon diagnostics | per-control numeric observations of existing counters; starts `Recording` for a traced control and attaches SQL and delta work | [`application/diagnostics.rs`](../../../../engine/crates/layerfs-daemon/src/application/diagnostics.rs), [`diagnostics/`](../../../../engine/crates/layerfs-daemon/src/application/diagnostics) |
| Kernel callback phases | steps inside a FUSE or NFS callback on its receive worker (owner visit jobs, base supply reads, release), as counters, not timing trees; off Linux charging does nothing | [`layerfs-mount-ports/src/phase.rs`](../../../../engine/crates/layerfs-mount-ports/src/phase.rs) |
| Overlay statements | bounded aggregate observations of real overlay SQLite executions by statement family, startup batch included | [`layerfs-overlay/src/diagnostics/`](../../../../engine/crates/layerfs-overlay/src/diagnostics) |

### Formats

| Writer | Output | Source |
| --- | --- | --- |
| `write_text` | two spaces per level, label, elapsed (`ns`, `us`, `ms`, `s` with three decimals); ` [error]`, ` [unknown]`, ` [incomplete]`; disabled: `disabled: no timing recorded` | [`format.rs`](../../../../core/crates/layerfs-telemetry/src/timer/format.rs) |
| `write_json` | `{"name", "elapsed_ns", "children"}`; `"outcome"` only when not ok, `"incomplete": true` only when set; disabled: `null` | [`json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/json.rs) |
| `encode_bounded(max)` | full JSON if it fits, else the root alone marked incomplete, else `None`; counts bytes first, never holds a partial encoding | [`bounded_json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/bounded_json.rs) |

Labels are escaped in both formats. Every writer error propagates. Saving a
report is the caller's job: choose the path, create it without clobbering,
flush, and keep the save result separate from the product result.

### Platform runtime and output (feature `platform`)

| Part | Behaviour | Source |
| --- | --- | --- |
| `Runtime::start` | `enabled: false` returns a disabled runtime with no allocation; otherwise validates output, monitor and an aggregate charge ≤ 8 MiB | [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |
| `OperationRecorder` | ≤ 32 recording slots (≤ 4 MiB); no free slot gives `Diagnostic::Omitted`, never a changed result | [`operation.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/operation.rs) |
| `Window` | per-operation CPU/RSS summary; `cpu_delta` only with no gaps, no discontinuity and ≥ 2 samples; `sampled_max_rss` is not an exact peak; process samples include sibling work | [`observation.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/observation.rs) |
| Monitor | thread `layerfs-observe`, interval 10–1,000 ms, history 1–600, windows 1–32; never waits for a lock (a missed sample is a gap) | [`monitor.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/monitor.rs) |
| Readings | Linux `/proc/self/stat` and `sysconf`; macOS `getrusage` and libproc; other targets `compile_error!` | [`platform/`](../../../../core/crates/layerfs-telemetry/src/platform) |
| Output | one `LFT1 {json}\n` line per record (`operation`, `resource`, `resource-unavailable`, `run-summary`); bounded queue (≤ 256 records, ≤ 8 MiB) and token bucket; a writer thread to stderr and/or a ring of local segment files (mode 0600, `flock`, expiry); best effort, no `fsync` | [`queue.rs`](../../../../core/crates/layerfs-telemetry/src/output/queue.rs), [`encode.rs`](../../../../core/crates/layerfs-telemetry/src/output/encode.rs), [`retention.rs`](../../../../core/crates/layerfs-telemetry/src/output/retention.rs) |
| Loss | `dropped`, `failed`, `overflow` counters; a failed destination is turned off for good | [`health.rs`](../../../../core/crates/layerfs-telemetry/src/output/health.rs) |

## Invariants

| Rule | Enforced by |
| --- | --- |
| A measured operation's `Result` is returned unchanged; timing never fails it | `TimingScope::run`, `Timing::record` in [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| Disabled timing reads no clock and creates no node | `Timing::disabled` in [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs) |
| A disabled runtime allocates nothing, even with invalid settings | `Runtime::start` in [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |
| A child cannot outlive its parent's region or cross threads | lifetimes and a `*const ()` marker in [`scope.rs`](../../../../core/crates/layerfs-telemetry/src/timer/scope.rs); [`tests/compile_fail/`](../../../../core/crates/layerfs-telemetry/tests/compile_fail) |
| A node that never returned is `Unknown`, never `Ok` | `Inner::collect` in [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Missing detail marks the node and every ancestor incomplete | `mark_incomplete`, `graft` in [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| A stalled output never blocks measured work | nonblocking submit and a separate writer thread in [`queue.rs`](../../../../core/crates/layerfs-telemetry/src/output/queue.rs) |
| The library reads no environment variable or global configuration | [`lib.rs`](../../../../core/crates/layerfs-telemetry/src/lib.rs), `Configuration` in [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Nodes, levels per report | `MAX_NODES` 1,024 (attached nodes included), `MAX_DEPTH` 32 | [`recording.rs`](../../../../core/crates/layerfs-telemetry/src/timer/recording.rs) |
| Label bytes | `MAX_LABEL_BYTES` 128 | same |
| Recording charge | `NODE_CHARGE` 1,024 bytes per node, `MAX_RECORDING_BYTES` 1 MiB; an ownership charge, not a resident bound | [`limits.rs`](../../../../core/crates/layerfs-telemetry/src/timer/limits.rs) |
| Elapsed beyond `u64` nanoseconds | JSON writer `InvalidData`, never saturated | [`json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/json.rs) |
| `encode_bounded` above 1 MiB | `io::Error` "encoded report hard limit" | [`bounded_json.rs`](../../../../core/crates/layerfs-telemetry/src/timer/bounded_json.rs) |
| Full disk or removed output directory | local destination dropped, `failed` counted, product result unchanged | [`queue.rs`](../../../../core/crates/layerfs-telemetry/src/output/queue.rs) |
| Platform aggregate | over 8 MiB: `io::Error` "aggregate telemetry ownership" | [`session.rs`](../../../../core/crates/layerfs-telemetry/src/runtime/session.rs) |

## Demonstration

Each test command gets a wall timeout of at most 120 s.

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`timer.rs`](../../../../core/crates/layerfs-telemetry/tests/timer.rs) | `cargo +1.85.1 test --locked -p layerfs-telemetry --test timer` | any | Nesting, start order, early return, recovery, panics, clipping, attachment |
| [`timer_json.rs`](../../../../core/crates/layerfs-telemetry/tests/timer_json.rs) | `cargo +1.85.1 test --locked -p layerfs-telemetry --test timer_json` | any, needs `python3` | JSON shape and escaping checked by an independent parser; no-clobber saving |
| [`timer_nested.rs`](../../../../core/crates/layerfs-telemetry/examples/timer_nested.rs) | `cargo +1.85.1 run --locked -p layerfs-telemetry --example timer_nested` | any | Text and JSON trees, an error branch and a disabled run |
| [`timer_composition.rs`](../../../../core/crates/layerfs-telemetry/examples/timer_composition.rs) | `cargo +1.85.1 run --locked -p layerfs-telemetry --example timer_composition` | any | Independent recordings composed by attachment, then saved outside the measured operation |
| [`off_allocations.rs`](../../../../core/crates/layerfs-telemetry/tests/off_allocations.rs) | `cargo +1.85.1 test --locked -p layerfs-telemetry --features platform --test off_allocations` | Linux, macOS | A disabled runtime makes zero allocations |
| [`stalled_output.rs`](../../../../core/crates/layerfs-telemetry/tests/stalled_output.rs) | `cargo +1.85.1 test --locked -p layerfs-telemetry --features platform --test stalled_output` | Linux, macOS | A blocked stderr changes no result and does not stop process exit |
