# layerfs-telemetry

Environment-independent parent/child timing trees for LayerFS.

> **Status:** Implemented standalone component; target LayerFS v0.1.7; not a
> released contract. The crate lives in the replacement product workspace under
> `core/` and has no product or third-party dependencies. Adapter integration,
> async execution and overhead qualification are follow-up work, listed below.

Specification: [`docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md`](../../../docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md).
Implementation: [#161](https://github.com/Ephemeral-AI-Lab/layerfs/issues/161).
Design workstream: [#160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160).

## Model

One operation owns one bounded timing tree:

- [`Timing::record`] starts a root before the operation closure runs, injects a
  running scope into it and returns the operation's original `Result` together
  with a completed [`TimingReport`].
- A component receives a pending [`TimingScope`] and calls
  [`run`](TimingScope::run) around its real body. `run` starts that scope's timer,
  passes a running handle to the closure and finalizes duration and outcome
  before returning.
- Child scopes come only from the running handle, so a child borrows the handle
  it was created from and cannot outlive its parent's measured region. A pending
  scope is consumed by `run`; the running handle has no `run` method.
- The outer caller decides whether the completed report is rendered, saved or
  dropped. The timer opens no file and reads no global configuration.

```rust
use layerfs_telemetry::timer::{Timing, TimingScope};

let (result, timings) = Timing::record("object.create", |root| {
    let object = construct(input, root.child("canonical.construct"))?;
    save(object, root.child("storage.save"))
});

pub fn construct(input: &[u8], scope: TimingScope<'_>) -> Result<Object> {
    scope.run(|content| {
        let bytes = content.child("encode").run(|_| encode(input))?;
        let id = content.child("hash").run(|_| identify(&bytes))?;
        Ok(Object { id, bytes })
    })
}
```

`record`, `run` and the injected closure all carry the caller's own
`Result<T, E>`; the error type is never wrapped or replaced. An infallible step
inside a recorded operation names its error type once, for example
`Ok::<(), DomainError>(())`, because the timer is generic over that type.

## Application-facing API

| Operation | Meaning |
| --- | --- |
| `Timing::record(name, operation)` | Start the root, run the operation, return `(Result<T, E>, TimingReport)` |
| `Timing::disabled(name, operation)` | Run the same operation with no clocks, nodes or output |
| `parent.child(name)` | Create a pending child scope below a running node |
| `scope.run(operation)` | Start the scope's timer, run the operation, return its `Result` |
| `scope.is_recording()` | True when this scope can still contribute a measured node |
| `scope.attach(optional_report)` | Attach owned completed data below a running node |

`TimingReport` is an optional root node: `root()`, `has_root()`, `node_count()`,
`levels()`, `is_incomplete()`, `into_root()`. `TimingNode` exposes `name()`,
`elapsed()`, `outcome()`, `is_incomplete()`, `children()`, `node_count()` and
`levels()`, and is built for synthetic or decoded data with `new`, `push_child`,
`with_children`, `with_outcome` and `with_incomplete`. `NodeOutcome` is `Ok` or
`Error`.

## Semantics

- Durations are inclusive monotonic elapsed time. A parent's elapsed time
  includes its children; no self-time, CPU time or network latency is derived.
- The original `Result` is preserved, including its error value. Ordinary errors
  are recorded before they propagate; completed nodes are kept when `?` returns
  early; uncalled operations have no node; repeated labels are distinct
  invocations in start order.
- A parent may recover from a child error and still succeed. Error outcome and
  measurement incompleteness are independent.
- An attached report becomes a child of the attaching node, keeping its own
  descendants and labels. `None`, a disabled report, a clipped report or an
  incomplete imported node marks the affected node and its ancestors incomplete
  without changing the product result. Attachment to a disabled scope is a no-op.
- Recording shares no mutable global or thread-local state. No internal borrow is
  held while a user closure runs, so a panic caught by the caller leaves the
  recording usable. Panics unwind normally: no report is fabricated.
- Disabled recording performs no clock reads and creates no nodes. It preserves
  mandatory product work and returns a report with no root, which stays distinct
  from a measured zero.
- Scope handles are neither `Send` nor `Sync`, and lifetimes prevent them from
  escaping their recording or their parent's measured region. See
  `tests/compile_fail/` for the externally checked cases.

## Limits

| Limit | Value | Behaviour when exceeded |
| --- | --- | --- |
| Nodes per report | `MAX_NODES` = 1,024 | Detail is clipped; the affected node and ancestors are marked incomplete; the operation still runs |
| Levels per report | `MAX_DEPTH` = 32 | Same |
| Label bytes | `MAX_LABEL_BYTES` = 128 | Labels are truncated on a UTF-8 boundary and the node is marked incomplete |

Attached nodes consume the assembled tree's remaining budget. Clipping bounds
both the live recording and the completed report; it never fails or shortens the
measured operation.

## Output

`write_text` renders a readable tree with escaped labels and visible markers:

```text
object.create  5.000ms
  canonical.construct  3.000ms
    encode  250ns
  storage.save  1.500ms [error]
```

`write_json` writes one fixed shape through any `std::io::Write`. Omitted
`outcome` means success; omitted `incomplete` means complete; a disabled report
serializes to `null`.

```json
{
  "name": "object.create",
  "elapsed_ns": 5000000,
  "children": [
    {"name": "canonical.construct", "elapsed_ns": 3000000, "children": []},
    {"name": "storage.save", "elapsed_ns": 1500000, "children": []}
  ]
}
```

Durations are checked when converted to integer nanoseconds; a value outside the
`u64` nanosecond range is reported as an `InvalidData` writer error instead of a
saturated number. Writer errors propagate through `std::io::Write`.

The caller owns saving: choose the path, create without clobbering, flush
explicitly and keep the save result separate from the product result.

```rust
let save_result = (|| -> std::io::Result<()> {
    use std::io::Write;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)?;
    let mut writer = std::io::BufWriter::new(file);
    timings.write_json(&mut writer)?;
    writer.flush()
})();
```

A failed write can leave a partial file; keep it as incomplete and surface the
error. Flush is not crash durability.

## Examples

```sh
cargo run --manifest-path core/Cargo.toml --example timer_nested
cargo run --manifest-path core/Cargo.toml --example timer_composition
cargo run --manifest-path core/Cargo.toml --example timer_composition -- /tmp/run-1/timing.json
```

`timer_nested` shows injected parent/child scopes, an error that stops one branch
and the same components under disabled recording. `timer_composition` shows
independently recorded reports composed at several levels through ordinary
function calls, then saved to a caller-selected path outside the measured
operation; running it twice shows the no-clobber failure while the product result
stays successful.

## Layout

```text
src/lib.rs               crate documentation and `pub mod timer`
src/timer/mod.rs         declarations and re-exports
src/timer/report.rs      completed owned data, outcomes and completeness
src/timer/recording.rs   private clocks, tree construction, attachment, bounds
src/timer/scope.rs       root/child lifecycle and disabled execution
src/timer/format.rs      readable tree output
src/timer/json.rs        fixed-shape JSON output
tests/                   public-API tests, compile-fail fixtures
examples/                runnable usage
```

`lib.rs` and `mod.rs` stay within the 200-physical-line declaration limit;
`src/` contains production code only, so tests, helpers and examples live outside
it.

## Checks

```sh
python3 core/tools/check_product_boundary.py
cargo fmt --manifest-path core/Cargo.toml --all --check
cargo test --manifest-path core/Cargo.toml --workspace --locked
cargo clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
```

`tests/timer_json.rs` validates emitted JSON with Python's standard library
instead of a Rust parser, and `tests/timer_compile_fail.rs` compiles the
fixtures under `tests/compile_fail/` with `cargo check` in a throwaway crate.

## Not in this component

Actual adapter/transport integration and its compatible encoding, async or
cancellation semantics, no-reply attribution, distributed correlation, background
delivery and crash-surviving journals remain follow-up work; the request/reply
notation in the specification is an abstract integration description, not a type
or transport this crate implements. Overhead qualification of a wired-in product
path is separate work. Future memory, CPU and storage observation domains are
independent siblings when implemented; unsupported observations are unavailable,
not zero.
