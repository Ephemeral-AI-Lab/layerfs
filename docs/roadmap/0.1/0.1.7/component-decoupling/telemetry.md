# layerfs-telemetry: environment-independent timing trees

> **Status:** Standalone timer implemented and verified in the replacement
> workspace; target LayerFS v0.1.7; not a released contract. Adapter integration,
> async behavior and overhead qualification remain follow-up work.

Implementation: [#161](https://github.com/Ephemeral-AI-Lab/layerfs/issues/161)
— delivered at [`core/crates/layerfs-telemetry/`](../../../../../core/crates/layerfs-telemetry/README.md).
Related: [co-design review](content-storage-co-design.md), [shared proposal](proposal.md),
[design workstream #160](https://github.com/Ephemeral-AI-Lab/layerfs/issues/160),
[release workstream #155](https://github.com/Ephemeral-AI-Lab/layerfs/issues/155).

## Final design decision

One operation owns one bounded timing tree. It injects child scopes into local
components and can attach independently completed timing subtrees. The outer
caller receives the assembled report and optionally saves it as JSON. Existing
communication adapters can later carry this data alongside ordinary results;
request/reply notation below is an abstract integration description, not a model
or transport implementation required in this crate.

The timer knows parent and child, not host, container, cloud, FUSE or transport.
Moving execution changes adapter wiring, not the measurement model. This is an
agreed design direction. The standalone timer crate is implemented (see
[Delivered implementation](#delivered-implementation)); product and adapter
integrations are still future work. All example durations below are invented
illustrations, not measured evidence.

This consolidates the owner's simplification decisions: no environment fields,
global trace registry, per-node distributed IDs, cross-process timestamps,
standalone trace module or collector service for joined request/response work.
Optional JSON output supersedes the earlier local-text-only scope; elapsed
hierarchies replace the earlier public root-relative start-offset schema.
The report describes causality and duration, not an aligned distributed timeline.

## Responsibility boundaries and layout

```text
core/crates/layerfs-telemetry/
  Cargo.toml
  README.md
  src/
    lib.rs                  # pub mod timer
    timer/
      mod.rs                # public reexports; implementation modules private
      report.rs             # owned data, outcomes and completeness
      recording.rs          # private clocks, tree construction/attachment, bounds
      scope.rs              # root/child lifecycle and disabled execution
      format.rs             # readable completed tree
      json.rs               # completed tree -> caller-supplied writer
  tests/
    timer.rs
    timer_format.rs
    timer_json.rs
  examples/
    timer_nested.rs
    timer_composition.rs
```

Within timer: recording uses report; scope uses recording/report; formatters use
only completed data. Root orchestration belongs with scopes. No god object may
combine measurement, transport, file policy and product behavior. Concrete
modules are sufficient; no provider/factory or sink trait is required.

The core is std-only, forbids unsafe code, and depends on no product crates.
Follow [core agent rules](../../../../../core/AGENTS.md): src/ contains product
code only; tests, fakes and executable examples live outside it. No inline test
attributes, test-only cfg/features or fake-clock hooks in product source.
lib.rs and every mod.rs are at most 200 physical lines and contain declarations,
reexports and thin delegation only; types, state and algorithms live elsewhere.
The path follows the [proposed product workspace](repository-layout.md); the
existing root crates remain a reference during that migration.
Inherit workspace edition, MSRV, license and version; register the workspace
member during implementation. The recorder neither chooses paths nor opens files.

Existing component/transport adapters own optional metadata encoding, decoding
and attachment. The outer application, SDK caller or benchmark runner chooses
recording and output. Components only receive a timing scope.

Future memory/, cpu/ and storage/ modules are siblings, created when implemented.
Each owns its units, scope, availability, cost and typed report. Memory deltas
are not peaks; CPU usage is not elapsed time; disk occupancy, physical I/O and
logical bytes are different observations. Unsupported observations are unavailable,
not zero. Follow cfg/fallback rules and keep future providers independent of timer.
No universal metric object or empty future scaffolding is required.

## Public API and data

```text
Timing::record(name, operation)     -> (original Result, TimingReport)
Timing::disabled(name, operation)   -> (original Result, disabled TimingReport)
parent.child(name)                 -> pending TimingScope
scope.run(operation)              -> original Result
```

Adapters additionally need two small operations on an active scope:

```text
scope.is_recording()               -> bool
scope.attach(optional report)      -> ()
```

Attachment consumes completed data, not a live scope. An expected but absent,
disabled, invalid or clipped subtree makes the active node incomplete, without
changing the product result. Incompleteness propagates to ancestors. On a disabled
scope, attachment is a no-op. On transport error, attach None before returning the
original error so missing remote detail remains visible.

```text
TimingReport
  root: optional TimingNode

TimingNode
  name
  elapsed: Duration
  outcome: Ok | Error
  incomplete: boolean
  children: list of TimingNode
```

Local labels are static. Imported labels must be bounded owned data; stdlib
Cow<'static, str> is one possible representation. Do not leak or globally intern
remote strings. Instants stay private and are never transmitted. Disabled output
has no root; that differs from a measured zero. Avoid per-file/object labels.

All Rust below is proposed API, not callable product code. Implementation must
supply compiling standalone versions of the examples.

### Parent and child across modules

```rust
use layerfs_telemetry::timer::{Timing, TimingScope};

let (result, timings) = Timing::record("object.create", |root| {
    let object = content::construct(input, root.child("canonical.construct"))?;
    storage::save(object, root.child("storage.save"))
});
```

```rust
pub fn construct(input: &[u8], scope: TimingScope<'_>) -> Result<Object> {
    scope.run(|content| {
        let bytes = content.child("encode").run(|_| encode(input))?;
        let id = content.child("hash").run(|_| identify(&bytes))?;
        Ok(Object { id, bytes })
    })
}
```

The sketches assume domain Result-returning helpers. Infallible helpers can use
Ok under the caller's error type. Pure helpers need no scope parameter if their
caller wraps them. No annotation macro is required.

## Delivered implementation

The standalone milestone is implemented at `core/crates/layerfs-telemetry/` and
is the first member of the independent `core/` workspace (own manifest, lockfile
and target directory; `core/` is excluded from the reference workspace). It is
std-only, depends on no product or third-party crate, forbids `unsafe`, opens no
file and reads no global configuration.

Delivered signatures, resolved while compiling the examples and tests:

```rust
Timing::record(name: impl Into<Cow<'static, str>>, operation: F)
    -> (Result<T, E>, TimingReport)
Timing::disabled(name: impl Into<Cow<'static, str>>, operation: F)
    -> (Result<T, E>, TimingReport)
where F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>

TimingScope<'_, Active>::child(name: impl Into<Cow<'static, str>>)
    -> TimingScope<'_, Pending>
TimingScope<'_, Pending>::run(operation: F) -> Result<T, E>
where F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>

TimingScope::is_recording(&self) -> bool
TimingScope<'_, Active>::attach(&self, report: Option<TimingReport>)
TimingReport::write_text(&self, writer: impl Write) -> std::io::Result<()>
TimingReport::write_json(&self, writer: impl Write) -> std::io::Result<()>
```

`Pending` and `Active` are public marker types. `child` and `attach` exist only
on the running handle and `run` only on the pending scope, so a child scope
cannot be created before its parent starts, a scope cannot be run twice, and a
running handle cannot be restarted; all of those are compile errors rather than
runtime states. Handles are neither `Send` nor `Sync` and borrow the handle they
came from, so they cannot escape their recording or the measured region of their
parent. `record`/`run` are generic over the caller's `Result<T, E>` and return it
unchanged; an infallible step inside a recorded operation therefore names its
error type once, for example `Ok::<(), DomainError>(())`.

`TimingReport` is an optional owned root (`root`, `has_root`, `node_count`,
`levels`, `is_incomplete`, `into_root`); `TimingNode` carries the label,
inclusive elapsed time, outcome and completeness and can be built for synthetic
or decoded data with `new`, `push_child`, `with_children`, `with_outcome` and
`with_incomplete`. Aggregate limits are public constants (`MAX_NODES` = 1,024,
`MAX_DEPTH` = 32, `MAX_LABEL_BYTES` = 128); recording, synthetic construction and
attachment all enforce them, attached nodes consume the assembled tree's
remaining budget, and clipping marks the affected node and its ancestors
incomplete while the wrapped operation keeps running. `write_json` emits
`null` for a disabled report and reports a duration outside the `u64` nanosecond
range as an `InvalidData` writer error instead of saturating it. Saving stays
outside the crate: the caller selects the path, uses no-clobber creation, flushes
explicitly and keeps the save result separate from the product result.

Verification for this milestone (all run locally; the repository has no CI):

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
cargo +1.96.0 fmt --manifest-path core/Cargo.toml --all --check
cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_nested
cargo +1.85.1 run --manifest-path core/Cargo.toml --locked --example timer_composition
cargo +1.96.0 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings
```

Tests live under `tests/` and exercise the production public API only: nesting,
repeated labels, errors, early returns, recovery, independent recordings, panic
unwinding, disabled execution, attachment, owned and clipped labels, aggregate
limits, text/JSON rendering, writer failures and saving. `tests/timer_json.rs`
validates emitted JSON with Python's standard library, and
`tests/timer_compile_fail.rs` compiles the `tests/compile_fail/` fixtures with
`cargo check` in a throwaway crate to prove the lifetime and thread restrictions.
Both examples are runnable and documented in the crate README.

## Timing and completeness semantics

1. Start a root at the declared semantic operation entry, before its required
   queue/lock/work; finish after its required result/acknowledgement. Explicitly
   composed workflows can own an outer root. Independent later operations get
   independent recordings, not an unbounded workspace-lifetime tree.
2. child creates a pending scope; the callee's run starts its timer. Passing a
   handle alone does not instrument a function body.
3. record/run finalize elapsed duration and Ok/Error before returning the original
   Result. Early returns retain completed nodes; uncalled operations are absent.
   A parent that recovers from a child error can succeed while retaining that error.
4. Repeated labels are distinct invocations in start order, not overwritten map
   entries. Remote descendants express causality, not a shared-clock timeline.
5. Durations are inclusive wall time. Caller duration includes remote work and
   instrumentation/transport costs. Do not add parent and child, infer CPU time,
   or label caller-minus-callee as network latency. No derived self-time is required.
6. Initial local scope handles cannot escape their recording/active parent and
   are not Send. Never hold an internal mutable borrow across the closure. Do not
   reduce concurrency or introduce blocking to fit this synchronous API.
7. Initial bounds: 1,024 nodes, 32 levels (both including root and attached nodes),
   and 128 UTF-8 bytes per label. Bound temporary and final storage. Omit excess
   detail and mark affected nodes/ancestors incomplete while executing all product
   work. Remote attachments share the assembled tree's remaining budget.
8. incomplete means requested detail is missing, including clipping or unavailable
   remote timing. It does not mean the operation failed. Completeness is relative
   to selected instrumentation, not every kernel event or instruction.
9. Disabled recording makes no optional clock calls or record allocations and
   requests no callee timings. Preserve mandatory product receipts, authentication
   and correctness checks; closure dispatch itself is not removed.
10. Preserve panic unwinding. Do not catch/swallow panics to fabricate reports or
    promise crash/abort recovery. An outer caller catching a panic may start a
    fresh independent recording.

## Request/response transmission

This section describes future use by existing adapters. Do not implement Request/
Reply types, simulated RPC machinery or wire codecs in layerfs-telemetry. The
standalone milestone demonstrates report composition through ordinary functions.

The existing request/response relationship supplies the parent association.
No globally unique ID is needed in every timer node for joined calls.

```text
Request { input, record_timing }
Reply   { result, optional timings }
```

1. Caller adapter opens a timed child around the existing request.
2. It requests timing only when that scope is recording.
3. Callee records locally and returns its tree with its original success/error.
4. Caller attaches the tree before propagating the result. Multiplexed transports
   use their existing request-correlation mechanism to find the right parent.
5. Outer caller receives one assembled tree. Nested remote calls follow the same
   rule: each callee returns its local tree with already-attached descendants.

The adapter implements this once. Same-process calls need no serialization.
Binary transports use their existing codec; JSON is final output, not required
wire encoding. Return one bounded subtree per completed request, not a message
per child. Add no telemetry-only round trip or acknowledgement.

Initial remote metadata ceiling: 64 KiB per reply, or less if required by the
existing frame budget. Enforce byte/node/depth/label limits during decoding before
allocating a full remote tree. Clip/omit optional metadata that will not fit,
preserve the product payload, and mark timing incomplete. Do not raise product
frame limits to carry timing. Invalid separable timing metadata must not replace
an otherwise valid product result; corrupt product framing keeps its existing
error behavior. Encode/decode/attachment overhead stays inside caller elapsed.

Each production adapter needs a concrete compatible encoding and peer-capability
plan. Preserve deployed SDK/CLI/daemon/FUSE contracts; do not unconditionally
append new fields. The core first proves this contract with a standalone example;
actual protocol integration, compatibility checks and overhead qualification are
explicit subsequent implementation work, not existing functionality.

### Limits that remain explicit

- No-reply requests may use an existing later completion/fence if attribution and
  bounded reporting are possible. Otherwise remote detail is unavailable. Never
  add a reply to every FUSE write for telemetry.
- Detached/background work needs later delivery/correlation and cannot silently
  attach after the parent returned its final report.
- Async adapters need explicit future/cancellation semantics before integration.
  Do not hold a synchronous scope across suspension or change scheduling to fit it.
- Kernel/FUSE activity without propagated ownership does not automatically belong
  to whichever SDK operation is running. Instrument known requests or report
  independent operations.
- This is selected timing, not a crash-surviving journal. A separate trace module,
  global collector and distributed-ID scheme are outside this implementation.

## JSON and saving

The successful case is deliberately small:

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

Absent outcome means ok; include "outcome":"error" for returned errors. Absent
incomplete means false; include "incomplete":true where detail is missing and on
ancestors. These flags are independent: successful product work can have incomplete
timing. Disabled JSON is null if explicitly serialized; default disabled execution
writes no file. No required environment tags, IDs, timestamps or start offsets.

Use integer nanoseconds with checked duration conversion and correct JSON escaping,
including controls/Unicode. Output errors stay separate from product errors.
The fixed-format writer accepts std::io::Write; it is not a general JSON framework.
Wire framing owns protocol versions. Freeze/version future incompatible saved-format
changes separately and never rewrite historical evidence.

Choose recording/output in the application or runner. The timer reads no global
config/environment variables. Start each root at operation entry, retain child
timings in memory, and save after the outer root finishes.

| Caller | Proposed output |
| --- | --- |
| Benchmark runner | timing.json inside the invocation's existing fresh output directory |
| Manual diagnostic | Explicit path, e.g. benchmark-results/telemetry/<fresh-run-id>/timing.json |
| SDK caller | Memory unless it chooses a destination |

The outer caller can run anywhere. Children receive no output path or instruction
to write files. Save the joined tree once. Do not put it in the product database,
Workspace backing or mounted Workspace.

```rust
// Proposed API, called after Timing::record returns.
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
// Keep the product result separate from save_result.
```

The caller selects/creates the parent directory. Never overwrite a receipt. A
failed write can leave a partial file: retain it as incomplete and surface the
error. Flush is not a crash-durability guarantee. Saving follows measurement and
counts toward the complete diagnostic command cost where applicable. Remote timing
transmission still costs time inside the outer operation; do not subtract it away.

Display remains a readable tree with escaped labels and visible error/disabled/
incomplete markers. No rolling logs, background writer, automatic directory,
general exporter, or Monitor dependency is required.

## Worked cases

All times below are illustrative. Labels describe work, not required placement.

### A. Entire operation in one process

```text
object.create                 5 ms
  canonical.construct         3 ms
    encode                    2 ms
    hash                      1 ms
  storage.save              1.5 ms
```

One root and injected scopes; no serialization during execution. Uninstrumented
work/overhead can fill the gap between the parent and its children. Save once.

### B. Storage through a request/response adapter

```text
object.create                 8 ms
  canonical.construct         3 ms
  storage.save                5 ms
    storage.execute           3 ms
      pack                    1 ms
      sqlite                  2 ms
```

The adapter measures storage.save and attaches the callee's storage.execute.
The 3 ms is included in 5 ms; the difference is not isolated network latency.
A callee making a further remote call returns that nested tree the same way.

### C. Reversed placement or local FUSE

| Wiring | Recording behavior |
| --- | --- |
| Local caller -> cloud service | Child call + returned subtree |
| Cloud caller -> reachable local service | Same child call + returned subtree |
| Process -> service in container | Same child call + returned subtree |
| Local FUSE handler -> same-process core | Direct injected child scope |
| Local FUSE handler -> another process | Request/response adapter where completion exists |

These are future wiring examples, not changes to approved benchmark topology.
The timer does not arrange networking, connectivity, mounting or deployment.

### D. Ordinary error, retry and missing remote data

```text
object.create                 4 ms  error
  canonical.construct         4 ms  error
    encode                    1 ms
    hash                      3 ms  error
```

Construction returns its original error and timing; storage was not called.
An existing retrying caller retains each actual attempt as a separate sibling.
Telemetry never adds retries; recovered parents can succeed with failed children.

For a timeout retain caller elapsed/error and mark remote detail incomplete.
For product success with missing timings preserve success and set incomplete.
Unknown time is not a zero-duration remote operation. A killed process does not
promise a returned report.

### E. Disabled recording and component-only measurement

Use Timing::disabled with the same child-injection code. Product work executes
normally, without optional clocks, record allocation, callee timing or auto-output.

Measure canonical construction alone using its production entry point and declared
inputs under a diagnostic root, without calling storage.save. This requires the
independent entry point from the [co-design](content-storage-co-design.md); a timer
cannot remove hidden DB access or admission from an already coupled function.

## Implementation sequence and acceptance

1. Implement synchronous ownership, bounded trees, attachment, text and JSON
   writers; no product runtime/transport dependencies.
2. Supply compiling nested and composition examples. Ordinary functions return
   independently recorded subtrees for attachment, including multiple nesting
   levels. No request/reply model, simulated transport or cloud deployment is
   needed to prove this data operation.
3. Select real product boundaries and specify codec/capability compatibility,
   no-reply/failure/cancellation behavior and qualified overhead before adoption.
   Async integration is separate from the synchronous core proof.

- [x] Preserve Results, child errors, early returns, recovery and panic propagation;
      prove independent recordings and scope lifetime/Send restrictions.
      `tests/timer.rs` plus the external `tests/compile_fail/` checks.
- [x] Verify nested/repeated children and attachment; imported labels outlive freed
      transport buffers through owned data, without leaks or global interning.
- [x] Verify independently recorded subtree composition, missing/error reports,
      disabled behavior and aggregate clipping through ordinary function calls;
      `examples/timer_composition.rs` composes reports at several levels.
- [x] Keep tests and test helpers outside src/ and exercise the actual public API.
      Use synthetic completed reports for exact composition/formatting assertions;
      use structural/ordering properties for real timer behavior. Add no test-only
      clock hooks, sleeps or fragile wall-time thresholds. Inspect disabled clock/
      allocation paths alongside public behavior checks; external instrumentation
      must not introduce product test branches. Disabled execution is verified
      through its public result (`is_recording` false everywhere, no root, no
      output) and by review of the disabled path, which performs no clock read and
      no node allocation.
- [x] Enforce node/depth/label budgets during recording and attachment. Future
      adapters enforce transport-byte budgets before unbounded allocation and
      preserve an otherwise valid product result when optional timing is rejected.
      (The adapter-side enforcement remains part of adapter integration.)
- [x] Validate emitted JSON with an independent parser: escaped names/control
      characters/Unicode, trees, error/incomplete flags, null disabled output,
      duration conversion and failing writers. Add no general parser to the core.
- [x] Verify no-clobber saving, surfaced write/flush errors and partial files;
      product results remain separate and recording owns no filesystem I/O.
      Demonstrations live in `tests/timer_json.rs` and the composition example.
- [x] Keep resource measurements, annotation macros, global/TLS collectors,
      distributed IDs, durable journals and Monitor migration out of this slice.
- [ ] Qualify product overhead without extra round trips, changed acknowledgement,
      worker/cache treatment or relaxed gates. Follow the shared proposal and
      repository benchmark rules; diagnostic trees are not release evidence.
      Follow-up: no product or adapter path is wired to the timer yet, so there is
      nothing to qualify; this is the first item of adapter integration.
- [x] Run applicable local checks and pre-push gate before implementation publication.
      Include the core source-boundary guard and 200-line entry-file rule.
      LayerFS has no CI. The candidate workspace checks are part of
      `tools/preflight.sh` and were run for this milestone.

## Estimated implementation size

The previous 190-310 production LOC estimate covered local timing and text only.
With attachment and fixed-format JSON, the plan was roughly 250-450 production
lines, 220-360 test lines, 80-130 example lines, 70-100 README lines and 10-15
manifest/workspace lines: about 630-1,055 total. These were planning estimates,
not measured facts or correctness caps.

Delivered for the standalone milestone: **732 production lines** (nonblank,
non-comment) in 7 files under `core/crates/layerfs-telemetry/src/`, 1,006
nonblank test lines in `tests/` (four public-API test files and seven compile-fail
fixtures), 167 nonblank example lines in two runnable examples, a 211-line crate
README, and the candidate workspace and package manifests with their lockfile.
Production counting uses `tools/production_loc.py` (tests, examples, fixtures,
docs, manifests and legacy inline test modules excluded). Product adapter changes
still need their own scoped estimate once the concrete existing protocol is
selected.

Earlier synthetic [summary](examples/telemetry-demo.summary.json),
[logs](examples/telemetry-demo.logs.jsonl) and
[Perfetto trace](examples/telemetry-demo.trace.json) files remain deferred broader
design illustrations, not this schema, current functionality or benchmark evidence.
