# C3 implementation specification: telemetry and retention

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This is an implementation specification, not delivered code or acceptance
evidence. All new checks are NOT_RUN. Numerical candidates require the freeze
in the [implementation plan](README.md).

This work implements the [telemetry/retention design](../08-telemetry-and-retention.md)
inside the existing `layerfs-telemetry` crate and wires it into the
[product path](01-product.md). The [verification specification](03-verification.md)
owns commands, deployment identities and evidence. The [implementation plan](README.md)
owns overall milestone ordering when integrating the local task groups T1-T6 below.

## 1. Scope and dependency boundary

Keep the existing `Timing` API and original operation `Result`/panic behavior.
Extend the same crate; do not introduce a telemetry-runtime package or import
benchmark helpers. Default features remain std-only and forbid unsafe code.
An explicit optional `native` feature enables actual native collection, workers
and owned output. Merely compiling/importing it performs no observation or I/O.

| Module owner | Responsibility to implement |
| --- | --- |
| `timer/` | Existing scopes/tree, retained-capacity corrections and recording bounds |
| `cpu/`, `memory/` | Typed observations, availability and bounded window reports |
| `operation/` | Portable recorder/report composition and disabled path |
| `runtime/` | Supplied configuration, one process monitor, bounded windows/lifecycle |
| `platform/` | Selected safe macOS/Linux process and explicit cgroup observations |
| `output/` | Bounded encoding, queue, forward/local/both routing and owned retention |

Portable modules never import runtime/platform/output or service/daemon types.
Native facilities use safe published APIs or a supplied safe observation capability;
no handwritten FFI, unsafe exemption, dependency patch or error-driven fallback.
Select and qualify the actual macOS/Linux APIs before native implementation. If
none satisfies required capability/safety constraints, report that blocker.

Service/daemon assembly enables native facilities explicitly. Cargo feature
unification may enable them for the shared crate instance in a native application;
prove default-only and native build graphs separately. Do not promise package
isolation from a module boundary. Every production file stays <=999 physical
lines; `lib.rs`/`mod.rs` stay <=200, declaration/delegation only. Add files with
real bodies, split by responsibility, and keep all tests outside product `src/`.

## 2. Configuration freeze and startup

Before enabling collectors, freeze the supported profile from design 08: selection
of timing/CPU/memory/diagnostic counters; intervals; node/depth/label/retained-byte
bounds; active windows; queues; encoder buffers; output rates/destinations;
retention class; shutdown allowance. Record exact effective values with evidence.
The candidate 8 MiB producer, 16 MiB collector and 24 MiB cohost-role accounting
are planning inputs, not already enforced defaults or total-RSS claims.

Startup sequence:

1. Read application configuration in assembly; the telemetry library reads no
   environment variables. Separate operational retention from benchmark evidence.
2. Inspect master enablement before constructing providers, formatting labels,
   allocating sample/report storage, opening destinations or starting workers.
3. If off, inject the disabled recorder. Return no sample history or exporter;
   do not serialize a disabled placeholder. Keep ordinary product startup intact.
4. If on, validate all limits and checked arithmetic before allocation: count and
   byte caps, ring interval/slots, report/queue/segment fit, encoded expansion,
   role multiplicity, simultaneous buffers and total native-runtime ownership.
5. Validate selected observation capabilities and output ownership. Unsupported
   fields become explicitly unavailable only where the frozen profile permits;
   required evidence remains incomplete. Never silently select another provider.
6. Construct one monitor and shared handles per process, then bounded output.
   If resource collection is deselected, do not create a sampler/history merely
   because timing is enabled. Partial startup must release acquired resources.
7. Inject handles into service and daemon boundaries. An invalid optional setup
   returns a separate initialization diagnostic and leaves telemetry disabled;
   product startup may proceed. Evidence-required verification cannot claim PASS.

No hot reconfiguration is required. Native thread stacks, socket/pipe buffers,
filesystem overhead and application source/sink costs are additional declared
resource domains; owned-payload arithmetic alone is not a process-memory proof.

## 3. Operation recorder and disabled execution

Expose one operation wrapper, conceptually `run(operation_key, label, closure)`;
freeze Rust signatures before implementation, preserving `Timing` signatures.
Its result contains the original `Result<T,E>` plus a diagnostic disposition:
`Disabled`, bounded `Report`, or `Omitted(reason)`. Disabled/omitted status needs
no heap allocation. Telemetry errors never replace the original domain error.

Portable composition may accept one narrow optional window-source capability
(begin/finish/release) supplied by native assembly. Do not expose files, samplers,
thread handles or transport types through timing scopes. No global registry,
per-metric factory, generic plugin loader or per-operation exporter is needed.

For each actual service operation:

1. Enter the wrapper at the declared handler boundary, before required logical
   authorization/admission work. Use the existing bounded correlation and static
   operation label; the request cannot force recording or raise local budgets.
2. If master-off, execute through `Timing::disabled` and return the unchanged
   result. Do not call window providers, optional clocks/counters or output code.
3. Otherwise try to acquire optional timing/window capacity without waiting for
   a future sample. Exhaustion omits diagnostic detail; product admission proceeds
   under its own rules. Record omission in fixed bounded health counters.
4. Run the original handler exactly once. Inject existing child scopes into C1/C2;
   use `Timing::disabled` when timing alone is deselected. Keep live scopes and
   StoreProvider on their owned execution context; neither is a worker message.
5. On ordinary return, finalize the optional window and timing, preserve product
   success/error, and hand bounded completed data to the output owner. No disk or
   network write, worker join or wait-for-queue-space belongs in the hot handler.
6. On unwind, release the window/timing ownership without catching the product
   panic or promising a completed report. Guard drop performs only bounded local
   release; it must not block on sampler/export locks or attempt fallible I/O.

The daemon wraps the actual client call for daemon-local call duration; the
service wraps its handler locally. The host driver owns full E2E timing including
its stdin/result path. Do not count these inclusive durations as separate elapsed
work. Do not add an operation wrapper for every frame, object or SQL statement.
Master-off also guards eager `format!`, generated metadata and subtree construction.
Required deadline clocks, authentication, admission, buffer ownership and
correctness/resource-enforcement counters still run. Existing mandatory result
fields stay intact; audit optional diagnostic-counter computation separately.

## 4. Process monitor and operation windows

Assembly owns one monitor, shared by all instrumented components in that process.
Do not create a second sampler for a cohosted collector role. Bound producer and
collector role memory in aggregate, including in-progress copies and writes.

Each observation carries a bounded source/scope identity and local monotonic
sample time. Bind operation windows to run identity, host/container namespace,
process incarnation and PID; PID alone is not unique across containers/restarts.
Use existing configured run/correlation identity where available. Define the
incarnation source before implementation; identity creation is not per timer node.
Cgroup identity/observation is separate from process identity and RSS.

The sampler performs one scheduled observation per tick, skips missed ticks and
records gaps rather than bursting to catch up. A later scheduled sample is new
observation work, not a retry of a failed read. Preserve unavailable/denied/failed
fields with their scope; never interpret them as zero or switch collectors.

A fixed slot ring holds raw samples. Every active resource window also owns one
fixed-size summary slot: baseline/start counter, sampled maximum/final values,
first/last sample time, count, gaps, boundary offsets and concurrency context.
The summary survives ring wrap and has no age-based eviction while its operation
is live. Reaching the active-window cap refuses added observation detail only.

Opening/closing a window must not wait for a sampling tick. If the profile uses
a recent sample, report its age/offset and actual covered interval; do not call
its CPU delta exact operation CPU. Exact boundary claims require separately
qualified boundary reads. A counter decrease or incarnation change invalidates
the delta; do not saturate it to zero or bridge across the discontinuity.

Process CPU is a cumulative user/system delta over its stated interval. RSS is
baseline/final plus an observed sampled maximum, not an exact heap or phase peak.
Overlapping operations see shared process consumption; record overlap rather than
allocating CPU/RSS to whichever operation finished first. Do not add cgroup totals
to overlapping process/kernel fields. Do not subtract timestamps across machines.

Use nonblocking bounded acquisition for summary access. A window lease must have
a generation-checked release path that does not wait for the sampler lock; a
fixed-slot release flag is one viable design. Avoid allocation in guard drop.
The sampler must not update a recycled slot under its previous generation. Test
contended release, unwind and slot reuse; do not solve them by evicting live slots.

## 5. Close current byte-budget holes before bounded export

Source inspection targets are [recording.rs](../../../../../crates/layerfs-telemetry/src/timer/recording.rs),
[report.rs](../../../../../crates/layerfs-telemetry/src/timer/report.rs) and
[json.rs](../../../../../crates/layerfs-telemetry/src/timer/json.rs).
Current node/depth/label constants are structural caps, not retained-byte limits.

- Normalize labels on every public construction/import/attachment path. Public
  `TimingNode::new` currently accepts unconstrained names; owned String truncation
  retains capacity, including a short label with oversized capacity. Copy accepted
  UTF-8 prefixes into bounded ownership rather than retaining excess allocations;
  mark actual clipping. Document this correction without changing canonical data.
- Validate per-recording limits against hard ceilings; preserve existing API entry
  points with documented bounded defaults and an explicit configured entry path.
  Charge retained capacities and node/child storage before growth. On exhaustion,
  omit optional detail and mark completeness; continue the operation unchanged.
- Account arena-to-report conversion, attachment, encoding and queued/in-flight
  reports simultaneously until ownership actually transfers/frees. Avoid cloning
  a whole report to forward/store it twice; charge any necessary duplication.
- Enforce imported node/depth/label/encoded-byte limits while decoding, before a
  peer can cause a large tree allocation. Post-allocation clipping is insufficient.
- Encode into a bounded destination and produce a valid smaller projection with
  explicit omissions. Never truncate arbitrary JSON bytes. Preserve identity,
  product outcome and diagnostic completeness; check duration conversions.
- If even the required diagnostic envelope cannot fit, emit no report and charge
  bounded loss. It must not erase product output, increase product frame bounds
  or reinterpret encoding failure as operation failure. Allocation-abort immunity
  is not claimed; controlled reserve failures must omit detail where supportable.

## 6. Delivery, retention and shutdown

Use design 08's candidate profile only after explicit freeze and accounting proof.
Default forward mode uses bounded structured diagnostic records on daemon stderr,
collected by the host coordinator. Keep operation stdout exclusively framed product
results. Define a versioned diagnostic envelope, maximum record/decoder staging
sizes and separation from ordinary stderr diagnostics; serialize writers and test
partial/interleaved/broken streams without unbounded resynchronization/draining.
Native service diagnostics reach the same coordinator through its configured
local collection path. Periodic resources are not new product opcodes or RPCs.

Completed operation detail may accompany a versioned optional response field when
implemented; send one bounded subtree, never live scopes or per-node messages.
Reuse request correlation and negotiate capability before adding fields. Caller-off
suppresses requested detail but cannot disable independent operator monitoring.
Malformed separable diagnostics preserve a valid product response; damaged product
framing retains ordinary transport failure/unknown-outcome semantics.

`local`/`both` are explicit optional modes, not the main mount-free acceptance.
They use only telemetry-owned destinations in their own environment, never the
LayerFS Store, Workspace data or arbitrary peer-selected paths. No local spool
fallback on forwarding failure. Count both-mode copies/destination bytes against
one aggregate queue/rate budget and reuse stable report identity across copies.

Implement count AND byte caps for completed queues and in-flight reports. Evict
only eligible operational records, or drop incoming when the active writer prevents
space reclamation. Fixed loss counters must signal overflow without generating a
new recursive error queue. Sampling histories, active windows and output each
have explicit separate lifetimes; no additional unbounded operation-history map.

Use one writer per owned segment namespace. Check capacity before append, rotate
before exceeding size/count/bytes, and perform configured age maintenance with
bounded work. Deletion failure disables that sink; never add extra segments or
recursively remove unrelated files. Charge retired open handles/staging files
until released. Benchmark evidence is append-only and exempt from operational GC;
refuse/mark missing new evidence rather than deleting historical attempts.

Shutdown stops new diagnostics/sampling, attempts only the configured bounded drain,
and discards remaining operational output with available loss accounting. Do not
join a stuck writer indefinitely or claim synchronous filesystem I/O is preemptible.
Qualify actual exit behavior; no fsync/durability guarantee is added. Output failure
never prevents required C1/C2 cleanup or triggers a repeated product operation.

## 7. Implementable task groups and admission gates

| Task | Deliverable | Gate before progression |
| --- | --- | --- |
| T1 | Existing Timing reuse, disabled wrapper, static labels and separate diagnostic disposition | Original success/error/panic behavior; off has no optional observations/allocations/workers/output; no resource sampler yet |
| T2 | Validated recording limits, normalized labels and bounded valid encoding | Adversarial labels/capacity, node/depth and escaped-size limits, transient overlap accounted; no byte-cap claim before proof |
| T3 | One native monitor and fixed ring/window summaries | Qualified safe macOS/Linux capabilities; gap/reset/overlap/long-window and contended-release checks |
| T4 | Bounded forward queue and actual coordinator collection | Slow/broken stderr and collector cannot grow ownership or corrupt stdout; identity/loss/coverage explicit |
| T5 | Configurable local/both retention and shutdown | Namespace isolation, disk/rotation failure, rate/byte/count caps, open-file accounting, append-only evidence preservation |
| T6 | Real daemon/service operation integration and overhead evidence | All enabled selections and master-off through actual network path; matched identities and unchanged product semantics |

T1 can precede native monitoring. T3-T5 are independent bounded modules but no
end-to-end telemetry-completeness claim precedes their real integration. Reuse
existing timer tests and add focused external behavior/bounds tests; no product
hooks, fake clocks inside src, inline tests or harness-source inclusion. Every
implementation commit records exact production LOC before/after and updates the
affected architecture description. Report each verification command and gap via
[03-verification](03-verification.md); never use the retired aggregate preflight.
