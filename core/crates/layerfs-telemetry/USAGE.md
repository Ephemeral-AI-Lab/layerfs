# How to use the timer

> **Status:** Current general guide.

A step-by-step guide for instrumenting a new LayerFS module. It assumes nothing
about the crate beyond what is written here; the API reference, limits and
semantics are in the [crate README](README.md), and the normative specification is
[`docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md`](../../../docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md).
The [C1/C2 measurement contract](../../../docs/roadmap/0.1/0.1.7/component-decoupling/content-io.md#7-measurement-and-completion)
requires construction-only, storage-only and integrated timing in the real component
implementations. This guide's small domain functions illustrate scope wiring;
they do not implement canonical hashing, CAS or SQLite and are not benchmarks.
LayerFS C1/C2 timing adds no retry, fallback or fsync/fdatasync/sync_all/sync_data.
Ordinary buffered output flush is not a sync-to-disk operation. SQL COMMIT remains
part of real storage completion under the no-WAL, synchronous-OFF contract.

## Who owns what

Three roles, three different jobs. Mixing them up is the usual source of
instrumentation that measures nothing or measures the wrong thing.

| Role | Owns | Uses |
| --- | --- | --- |
| Operation owner (SDK/CLI entry point, benchmark runner, daemon request handler) | The root of one semantic operation, whether recording is enabled, and where the finished report goes | `Timing::record` / `Timing::disabled`, `TimingReport::write_text` / `write_json`, saving |
| Component (content, storage, workspace, …) | Its own labelled steps, and nothing about output | `TimingScope<'_>` parameter, `scope.run`, `active.child`, `active.is_recording`, `active.attach` |
| Adapter / transport (existing request/response paths) | Carrying an independently recorded report back to the caller | Returns a `TimingReport` with the ordinary result; the caller attaches it |

A component never starts a recording, never chooses a file path and never reads
configuration. That is what lets the same component run unchanged when recording
is disabled.

## 1. Instrument a component

Every instrumented function takes a pending scope and runs its real body inside
`scope.run`:

```rust
use layerfs_telemetry::timer::TimingScope;

#[derive(Debug)]
pub enum ContentError {
    EmptyInput,
}

pub struct Object {
    pub id: String,
    pub bytes: Vec<u8>,
}

pub fn construct(input: &[u8], scope: TimingScope<'_>) -> Result<Object, ContentError> {
    scope.run(|content| {
        let bytes = content.child("encode").run(|_| encode(input))?;
        let id = content.child("hash").run(|_| identify(&bytes))?;
        Ok(Object { id, bytes })
    })
}

fn encode(input: &[u8]) -> Result<Vec<u8>, ContentError> {
    if input.is_empty() {
        return Err(ContentError::EmptyInput);
    }
    Ok(input.to_vec())
}

fn identify(bytes: &[u8]) -> Result<String, ContentError> {
    Ok(format!("obj-{}", bytes.len()))
}
```

What this gives you:

- The component keeps its own error type; `run` returns exactly your
  `Result<Object, ContentError>`, so `?` and early returns behave as before.
- `encode` and `identify` stay ordinary functions — pure helpers need no scope.
  Instrument them only if their individual duration is worth a node.
- `child` is available only inside the closure, `run` consumes the pending scope.
  A child cannot be created before its parent starts, a scope cannot be run
  twice, and neither can escape: those are compile errors, not runtime states.
- Nothing is written anywhere. A report with no root appears only if the operation
  owner chose disabled recording.

**Label naming.** Use stable, dotted, low-cardinality operation labels
(`object.create`, `canonical.construct`, `storage.execute`, `sqlite.insert`).
They are the schema downstream tools match on, and they consume the bounded node
budget. Never use per-file, per-object, per-request or per-thread labels.

## 2. Nest deeper

Call the next component with a child scope. Depth is free until the aggregate
limits apply:

```rust
pub fn commit(object: &Object, scope: TimingScope<'_>) -> Result<u64, CommitError> {
    scope.run(|commit| {
        // `prepare` and `publish` are this module's own functions.
        let prepared = commit.child("storage.prepare").run(|_| prepare(object))?;
        commit.child("publish").run(|_| publish(&prepared))
    })
}
```

Children appear in **start order**. Repeating a label is fine and expected (two
`pack` steps are two distinct invocations) — they are never merged or
overwritten. Keep nesting for real causal steps, not for every function call.

## 3. Measure work that happens elsewhere

When the detail is produced by a different recording — another component, another
process, or a peer behind an existing adapter — measure the call locally, then
attach the owned report:

```rust
#[derive(Debug)]
pub enum StoreError {
    Content(ContentError),
    Engine,
}

pub fn save(object: &Object, scope: TimingScope<'_>) -> Result<usize, StoreError> {
    scope.run(|store| {
        let packed = store.child("pack").run(|_| Ok(object.bytes.len()))?;

        // Ask the peer for timing only when this scope will actually record it.
        let (engine_result, engine_report) = engine_write(packed, store.is_recording());
        store.attach(engine_report);
        engine_result?;

        store.child("sqlite.insert").run(|_| Ok(packed + 1))
    })
}

/// Independent recording, normally behind an existing request/response adapter.
fn engine_write(rows: usize, requested: bool) -> (Result<(), StoreError>, Option<TimingReport>) {
    let run = |root: &TimingScope<'_, Active>| {
        if rows == 0 {
            return Err(StoreError::Engine);
        }
        root.child("compress").run(|_| Ok::<(), StoreError>(()))?;
        root.child("sqlite.execute").run(|_| Ok(()))
    };
    let (result, report) = if requested {
        Timing::record("storage.execute", run)
    } else {
        Timing::disabled("storage.execute", run)
    };
    (result, requested.then_some(report))
}
```

Rules for this pattern:

- Perform the work first, attach before propagating the result. `attach` consumes
  completed data, never a live scope, and copies nothing: the imported subtree
  keeps its own labels and durations and stays valid after its source is gone.
- `is_recording()` is the switch for *optional* requests. When it is false, do not
  ask for timing at all — pass `None`.
- `None`, a disabled report, an invalid report or a clipped one marks the
  attaching node and its ancestors `incomplete`, and never changes your product
  result. `incomplete` means "detail is missing", not "the operation failed".
- Attachment on a disabled scope is a no-op, so the same code path works when the
  operation owner disables recording.
- Do not add a telemetry-only round trip, acknowledgement or reply to obtain
  timings. If the existing exchange cannot carry them, report missing detail.

## 4. Own an operation root

Exactly one root per semantic operation, started at entry and finished after the
last required acknowledgement. The root's elapsed time therefore includes required
acquisition, handoff waits and instrumentation/transport cost. C1/C2 operations
do not retry failed work or select another execution path after failure.

```rust
use layerfs_telemetry::timer::{Active, Timing, TimingReport, TimingScope};

fn create_object(input: &[u8], recording: bool) -> (Result<usize, StoreError>, TimingReport) {
    let run = |root: &TimingScope<'_, Active>| {
        let object = construct(input, root.child("canonical.construct"))
            .map_err(StoreError::Content)?;
        save(&object, root.child("storage.save"))
    };
    if recording {
        Timing::record("object.create", run)
    } else {
        Timing::disabled("object.create", run)
    }
}
```

- Independent later operations get independent recordings. Do not keep one
  workspace-lifetime tree: it grows without bound and stops describing causality.
- A required component error fails the operation. The timer never retries or
  converts it to success; a separately requested operation gets a new root.
- To measure **one component alone**, call its own entry point under a diagnostic
  root and simply do not call the other components. A timer cannot remove hidden
  I/O or admission from a coupled function.
- The closure must return a `Result`; the error type is the caller's own. An
  infallible step therefore names its error once, for example
  `Ok::<(), MyError>(())`, and `std::convert::Infallible` works for steps that
  truly cannot fail.

## 5. Decide recording, then deliver the report

Recording is a caller decision — configuration, CLI/GUI flag or benchmark
selection. Components need no change, and disabled recording performs no clock
reads and creates no nodes while still running all product work.

```rust
fn deliver(
    result: Result<usize, StoreError>,
    timings: &TimingReport,
    recording: bool,
    output_path: &std::path::Path,
) -> std::io::Result<()> {
    println!("product result: {result:?}");
    if !recording {
        return Ok(()); // nothing was measured; do not invent a file
    }
    timings.write_text(std::io::stdout())?; // readable tree
    let save_result = save_json_once(timings, output_path); // caller-selected path
    println!("save result: {save_result:?}"); // separate from the product result
    Ok(())
}
```

```rust
fn save_json_once(report: &TimingReport, path: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true) // never clobber a receipt
        .open(path)?;
    let mut writer = std::io::BufWriter::new(file);
    report.write_json(&mut writer)?;
    writer.flush()
}
```

- A disabled report has no root: `has_root()` is false, text prints
  `disabled: no timing recorded`, JSON prints `null`.
- The caller creates the directory. A local convention for manual diagnostics is
  `benchmark-results/telemetry/<fresh-run-id>/timing.json` (gitignored, append-only
  evidence); a benchmark runner should write `timing.json` into its existing fresh
  invocation directory.
- A failed write may leave a partial file: keep it as incomplete and surface the
  error. Flush is not crash durability.

## Expected results

| Situation | Your product result | Timing report |
| --- | --- | --- |
| Success | original `Ok(value)` | full tree; durations inclusive (parent ≥ child, never summed, no derived self-time) |
| Component error | original `Err(error)` | node and ancestors `outcome: error`; siblings never called have no node |
| Early `?` return | original `Err` | completed nodes kept; uncalled operations absent |
| Repeated label | unchanged | distinct invocations, in start order |
| Disabled recording | unchanged | no root; `is_recording()` false everywhere; JSON `null` |
| Node/depth budget exhausted | unchanged | no node for the clipped detail; affected node and ancestors `incomplete: true`; the body still runs |
| `attach(None)` / disabled / clipped report | unchanged | attaching node and ancestors `incomplete: true` |
| `attach(Some(report))` | unchanged | callee root becomes a child, clipped to the assembled tree's remaining budget |
| Label longer than 128 bytes | unchanged | truncated on a UTF-8 boundary, node `incomplete: true` |
| Panic in a measured body | unwinds; no report is returned | nothing fabricated; a panic the caller catches leaves the recording usable and the unfinished node `incomplete` at `0ns` |
| Duration beyond the `u64` nanosecond range | unchanged | `write_json` returns `InvalidData` instead of a saturated number |

## Rules and gotchas

- **No async in this milestone.** The measured closure returns the `Result`
  synchronously; handles are neither `Send` nor `Sync`, so a scope cannot be held
  across an `await` or moved to another thread. Async, cancellation and no-reply
  attribution are follow-up work — do not improvise around them.
- **Never store a scope.** Keep it on the stack, pass it by value to the callee,
  drop it when the call returns.
- **One root per operation**, bounded by 1,024 nodes, 32 levels and 128 bytes per
  label. Clipping omits detail and flags incompleteness; it never shortens or
  fails the operation.
- **Outcome and incompleteness are independent.** A successful operation can have
  incomplete timing, and an error can be fully measured.
- **Never derive latency or CPU time.** No `parent − child`, no summing children,
  no CPU inference from elapsed time.
- **Keep the measured body honest.** Do not move real work outside the timer to
  make a number look better, and do not prime warm state to make a phase look
  fast; see the repository [benchmark rules](../../../docs/general/benchmark_rules.md).
- **No globals, no thread-locals, no background writer.** Independent recordings
  share no state by construction, and output stays with the caller.
- **Let tests assert structure, not wall-clock time.** Assert names, ordering,
  outcomes and flags; avoid sleeps and timing thresholds.

## Checklist for a newly instrumented module

- [ ] The component takes `TimingScope<'_>` and returns its own `Result`.
- [ ] `scope.run` wraps the real body; every labelled child describes a causal step.
- [ ] Labels are stable, dotted and low-cardinality.
- [ ] Optional peer timing is requested only when `is_recording()` and attached as owned data.
- [ ] Required errors propagate; no retry, fallback or recovery-to-success in the C1/C2 path.
- [ ] No scope is stored, awaited, sent across threads or returned.
- [ ] The component opens no file and reads no configuration; the caller renders and saves.
- [ ] Tests cover success, error, early return, disabled execution and missing detail structurally.

## Where to read more

- [Crate README](README.md) — API surface, semantics, limits, output shapes and checks.
- [`examples/timer_nested.rs`](examples/timer_nested.rs) — injected parent/child scopes, an error branch and a disabled pass.
- [`examples/timer_composition.rs`](examples/timer_composition.rs) — independent recordings composed at several levels, then saved outside the measured operation.
- [`tests/`](tests) — executable contracts, including the external compile-fail checks for the lifetime and thread restrictions.
- [Specification](../../../docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md) — worked cases A–E and the future adapter contract.
