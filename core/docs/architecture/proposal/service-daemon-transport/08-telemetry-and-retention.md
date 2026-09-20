# Portable telemetry, operation recording and bounded retention

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

This proposal records the requested reusable, disableable telemetry design.
Existing timing is implemented; CPU/memory collectors, the outer operation
recorder, output modes and numeric budgets below are proposed, not implemented or
qualified. No runtime/overhead test ran for this document. Current telemetry
source was inspected at HEAD `9f35c49ad62956f131dc2676787f99d69659686e`.
The [existing timer contract](../../../../../docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md)
remains authoritative for today's API. This proposal does not silently change it.

For the concrete service/daemon startup and operation-wrapper tasks, use the
[telemetry implementation specification](implementation/02-telemetry.md) and its
[overall implementation plan](implementation/README.md). They preserve the
existing timer API and make the byte-bound corrections and runtime evidence
explicit prerequisites rather than claiming this design is already wired.

## 1. Three owners, three lifetimes

```text
 PROCESS LIFETIME
 shared ProcessMonitor
   CPU counters / RSS observations / optional container scope
   bounded recent history + fixed-size active-window aggregates
                    |
                    | observations with identity, scope and coverage
                    v
 OPERATION LIFETIME
 OperationRecorder (proposed composition)
   existing Timing + optional process-resource window + owned counters
   original operation Result remains separate from diagnostics
                    |
                    v
 OUTPUT LIFETIME
 bounded encode -> bounded queue -> configured destination
                                  +--> forward to coordinator
                                  +--> local operational files
                                  `--> both, within one aggregate budget
```

One observer serves a process, not each operation or component. Daemon-hosted
Workspace, FUSE and bridge code share process observations; a separate LayerFS
process gets its own observer. Container/cgroup observations have a separate
scope and are not added to that process's RSS as independent memory.

Keep operation elapsed traces distinct from process CPU deltas and sampled
memory. Concurrent siblings contribute to process observations. Associate them
with local operation windows, process incarnation and concurrency context; do not
claim exclusive per-operation CPU/RSS. Caller E2E timing and each process's local
durations use their own clocks; cross-process timestamp subtraction requires a
declared mapping/uncertainty. Inclusive timing children must not be summed as
independent elapsed work.

## 2. Keep Timing; compose an operation report

Keep `Timing::record`, `Timing::disabled`, `TimingScope::child/run/is_recording/attach`.
C1/C2 continue receiving timing scopes. The proposed outer `OperationRecorder`
belongs at the service/application boundary and composes typed timing, CPU-window
and memory-window reports. Do not rename a timer into a universal metric object.

```text
 begin optional resource window
     |
 Timing::record -> original service operation -> original Result
     |
 end optional resource window
     |
 bounded completed diagnostics, independently complete/clipped/unavailable
```

An operation guard releases its summary slot on success, returned error and
unwinding. Preserve the current panic behavior: do not catch a product panic and
convert it to success or fabricate a complete report. A process crash can lose
volatile observations and terminal diagnostics.

Guard drop only releases bounded local ownership; it does not flush output, join
workers or run fallible disk/network cleanup. Telemetry capacity exhaustion skips
detail rather than changing application operation admission.

CPU observations use cumulative counter differences over a declared interval.
Memory reports hold baseline, sampled maximum and final observations, with sample
count, missing samples, largest gap and boundary coverage. Exact CPU-window claims
need actual qualified boundary readings. A reused recent snapshot carries its
age/offset and is not silently treated as an exact boundary reading. Resource
collection must not block the operation awaiting the next sample.

Long operations retain starting counters and a constant-size aggregate throughout
their lifetime. A 60-second raw-history ring does not end a longer operation or
erase its running sampled maximum. Old raw details can be evicted while the
summary remains. Operation deadlines are separate from telemetry history length.

## 3. Portable reports and pluggable collection/output

Use **one `layerfs-telemetry` crate**, with module and optional-feature boundaries.
Do not introduce a second telemetry-runtime or collector package. Today's default
crate is std-only, forbids unsafe code, opens no files and reads no global
configuration. The proposed extension preserves that default recording/report
surface and puts native collectors, workers and owned output behind an explicit
optional `native` feature. This feature is proposed, not present in Cargo today.

```text
layerfs-telemetry/
+-- Cargo.toml
+-- src/
|   +-- lib.rs                     declarations/reexports; optional native modules
|   +-- timer/                     existing timing API, preserved
|   +-- cpu/                       typed observations/window reports
|   +-- memory/                    typed observations/bounded aggregates
|   +-- operation/                 recorder/report composition
|   |
|   +-- runtime/                   optional native integration
|   |   +-- mod.rs
|   |   +-- config.rs              validate explicitly supplied policy
|   |   +-- monitor.rs             one observer, bounded history/windows
|   |   `-- lifecycle.rs           assembly, stop and bounded drain
|   +-- platform/                  optional native observations
|   |   +-- mod.rs
|   |   +-- macos.rs
|   |   `-- linux.rs
|   `-- output/                    optional native output ownership
|       +-- mod.rs
|       +-- encode.rs              valid bounded output
|       +-- queue.rs               byte/count caps and loss accounting
|       +-- writer.rs              destination/rate policy
|       `-- retention.rs           owned operational segment cleanup
`-- tests/                         external behavior/feature/bounds checks

C1/C2 -> timer scopes
service/daemon assembly -> runtime -> platform + portable reports + output
future runtime adapter -> portable reports and supported observations
```

All new paths are proposed responsibility locations, added with real bodies.
Separate monitor/history/window files if their actual responsibilities or line
ceilings require it; no empty files, per-component samplers or registry.
Configuration is passed in by assembly; importing or enabling the feature does
not start workers, probe the OS, open paths or export data. Runtime `enabled=false`
remains distinct from compiling without native capabilities.

Dependency direction stays one-way: `timer`, `cpu`, `memory` and `operation` do
not import native runtime/platform/output owners or any LayerFS service type.
C1/C2 keep using their current timing calls. Service/daemon builds explicitly opt
into native facilities and configure them; a portable target builds without those
facilities. Cargo feature unification can enable native dependencies for the
shared crate instance in a native application, so this is not separate-package
isolation. Verify standalone default-feature and native application build graphs;
never claim C1's dependency graph excludes native dependencies in every combined
build merely because C1 requests none.

Keep crate-wide unsafe prohibition: use suitable safe published APIs or supplied
safe observation capabilities for native collection. Do not copy handwritten
benchmark FFI into the crate or relax its safety rule as a side effect of merging.
If the chosen API cannot be supported within that boundary, report the concrete
blocker before changing the contract. Native I/O is explicit feature-enabled
behavior; update the implementation contract/documentation when implementing it,
without pretending the current crate already has collectors or files.

Inject a small observation capability and bounded output destination at assembly.
No provider registry, per-metric factory or dynamic plugin loader is needed.
Unsupported observations are unavailable; there is no implicit switch to another
collector. A future managed runtime uses its supported APIs and lifetime rules,
not a pretend native process sampler. Every new production file remains <=999
physical lines; `lib.rs`/`mod.rs` remain <=200 with declarations/delegation only.

### Production LOC baseline and estimated growth

**Measured existing crate: 763 production LOC in seven Rust source files.**
The same files total 1,075 physical lines, including comments and blanks. These
are different measures; 1,075 is the whole-crate physical total, not an oversized
file. Every inspected production file is below its applicable physical ceiling.

| Existing file under `src/` | Production LOC | Physical lines |
| --- | ---: | ---: |
| `lib.rs` | 3 | 16 |
| `timer/mod.rs` | 8 | 25 |
| `timer/scope.rs` | 135 | 206 |
| `timer/recording.rs` | 237 | 300 |
| `timer/report.rs` | 194 | 308 |
| `timer/format.rs` | 71 | 84 |
| `timer/json.rs` | 115 | 136 |
| **Existing crate total** | **763** | **1,075** |

Baseline revision: `9f35c49ad62956f131dc2676787f99d69659686e`. The seven working
files were byte-compared with that commit before counting. Method:
[tools/production_loc.py](../../../../../tools/production_loc.py), SHA-256
`c6e853c2280e2ee96caa6cafff4b221fa9201e1f5bf40d12e8251f70387210fc`.
It counts nonblank, non-comment production source, including declarations and
forwarding code, and excludes tests/examples/docs/tools/manifests/dependencies.
Reproduce against the recorded source and counter revision:

```sh
python3 tools/production_loc.py --files | rg 'core/crates/layerfs-telemetry/'
```

The following additions are **engineering estimates, not measured deltas or
mandatory targets**. They assume reuse of suitable safe native APIs, one shared
monitor, the proposed bounded operational profile, and no cloud provider,
general exporter registry or new benchmark framework. Folder estimates include
their declarations/reexports and required implementation; they are not per-file
ceilings.

| Planned production area | Estimated added LOC | Included responsibility |
| --- | ---: | --- |
| Existing `timer/` corrections | +200 to +350 | Retained-byte/label/encoding boundary work while preserving existing API behavior |
| `cpu/`, `memory/`, `operation/` | +400 to +650 | Typed reports, window aggregation, operation composition and disabled path |
| `runtime/` | +400 to +700 | Validated policy, one monitor, capped history/windows and lifecycle |
| `platform/` | +200 to +350 | Shared macOS/Linux adapters and explicit unavailable observations |
| `output/` | +600 to +1,150 | Valid bounded encoding, queue/rate limits, forward/local/both routing and owned retention |
| **Added inside the single telemetry crate** | **+1,800 to +3,200** | No second telemetry package |
| Thin service/daemon/coordinator integration | +100 to +300 | Startup injection, operation wrappers and selected output route; no duplicate collectors |
| **Total added first-party production code** | **+1,900 to +3,500** | Telemetry extension plus integration |

Adding the crate estimate to the measured baseline gives an **arithmetic planning
range of 2,563-3,963 production LOC for the telemetry crate**, roughly 2,600-4,000.
It is not a measured future size. The application wiring row is outside that
crate total. The existing 763 lines are reused, not counted again as new code.
This estimate excludes the separate bridge/service/daemon implementation budget;
when combining plans, count telemetry wiring only once.

| Non-production work, excluded from the totals above | Separate planning allowance |
| --- | ---: |
| External tests and focused integration helpers | +1,200 to +2,200 LOC |
| Runnable usage examples | +50 to +150 LOC |
| Documentation, manifests and lockfile changes | No production LOC contribution; no fixed size target |

Physical file limits remain hard: **999 lines per production file; 200 for
declaration/delegation-only `lib.rs`/`mod.rs`**. Split a responsibility before its
file reaches the ceiling; do not compress source, remove validation or move code
outside the counted scope to satisfy an estimate. In particular, `output/`'s
aggregate estimate is spread across its focused files, not placed in one file.

Current implementation status: **none of this extension is implemented by this
document**. This documentation task adds no production code; the inspected
telemetry source remains 763 production LOC. Recompute actual before/after counts
from exact parent/staged snapshots for every implementation commit under the
repository policy. Revisit these estimates after native API, encoding and byte
accounting choices are concrete; a package merge itself provides no measured LOC
or performance saving.

## 4. Disable and selective recording

Configuration belongs to outer assembly. A master `enabled=false` at startup
means no telemetry clock/counter reads, sample/history allocations, background
sampler/export workers, file opens, report encoding or telemetry exports.
Use the existing disabled timing path and lazy/static labels: eager `format!`
or prebuilt metadata would allocate before the disabled API can prevent it.

Required product work remains enabled: deadline clocks, authentication, admission,
buffer ownership and resource-limit enforcement are not optional telemetry.
When enabled, timing, CPU, memory and diagnostic counters can be selected
independently; disabled measurements are distinct from measured zero. A caller
cannot force the service to exceed its local recording policy or budgets.

Caller-side off suppresses its request for optional returned detail; it cannot
disable independently configured operator monitoring in the service. Do not
serialize a disabled placeholder merely to create an otherwise absent report.

Initial configuration is startup-scoped; hot reconfiguration is not required.
Thus there is no additional shutdown/restart state machine for toggling each
collector during an active operation. Invalid telemetry settings reject telemetry
initialization with an explicit diagnostic. Optional telemetry can remain disabled
while product startup proceeds; an evidence-required harness must refuse to call
that run fully verified. Never silently choose a different output or collector.

## 5. Reports and environment routing

Generate three bounded report categories:

| Category | Contents and scope |
| --- | --- |
| Operation | Operation/process identity, local begin/end, outcome, elapsed tree, owned byte/frame/resource counters, associated process-resource window and completeness |
| Resource interval | Process/container scope and source, CPU deltas, RSS/domain observations, sample coverage and concurrency context; can describe idle/overlapping work |
| Run summary | Deployment/source/configuration identities, operation outcomes, report loss/clipping and evidence completeness |

No unbounded producer labels, arbitrary metric-name registry or retained event
per frame/object. Store units and source explicitly. Allocation accounting, RSS,
container memory and overlapping kernel fields are different observations.

Default routing is **forward to the host coordinator**. The coordinator chooses
its output directory; a daemon cannot write an arbitrary host path without an
explicit delivery/storage mechanism. Optional daemon `local`/`both` paths resolve
inside its own environment and must be writable. Local container retention does
not survive container deletion unless explicitly exported or persisted.

```text
 Docker daemon reports -- bounded structured stderr --> host coordinator
 Native service reports -- bounded local output ------> host coordinator
 External process/container observations -------------> host coordinator
                                                        |
                                                        v
                                             configured host output directory

 Optional local/both: same completed report -> daemon-local managed segments
```

For independent periodic resource output, a host driver drains and demultiplexes
bounded structured stderr records from the actual Docker daemon; native service
output can be collected locally. This path requires implementation/qualification
of record sizes, ordinary diagnostic separation, draining and collector access.
It is not a new bridge RPC, unsolicited frame in the request protocol, or new
monitoring service. If that collection path is unavailable, report the gap rather
than pretending all resource data arrived with an operation response.

Completed operation diagnostics may instead accompany the existing bounded result
path when its schema is frozen. Daemon stdout stays framed: never mix arbitrary
log lines into it. Use stable report IDs to recognize the same report forwarded
and stored locally. Do not emit two independent copies by default merely because
both delivery mechanisms exist. A lost operation connection can lose diagnostics;
there is no guaranteed-delivery or durable telemetry contract.

Main pair 3 acceptance uses forward mode and creates no application telemetry or
payload files in the container. Local/both is a separately selected and tested
output mode, with its disk/page-cache/cleanup costs declared. No automatic local
spool on a forwarding failure. Store telemetry outside the LayerFS content Store.

## 6. Candidate operational profile, not approved benchmark limits

These values are configuration candidates. Validate overhead, capacity accounting
and target behavior before claiming them as enforced defaults. They do not replace
any registered benchmark's sampling, cache, time or evidence contract.

| Per-process setting | Candidate | Bound/action |
| --- | --- | --- |
| Sample interval | 100 ms | One observer; skip missed ticks, record gaps, no catch-up burst |
| Raw history | 60 seconds and 600 slots | Evict oldest on age or capacity; fixed storage, no resize |
| Sample slot allocation | <=256 bytes | 600 slots reserve <=150 KiB; identity/strings separately bounded |
| Active resource summaries | 32, <=2 KiB each | <=64 KiB; no age eviction of live operations; refuse added observation detail |
| Detailed timing | <=8 concurrent recordings within a 4 MiB pool | Pool includes live/completed/transient tree allocations; clipping/unavailable when full |
| Encoded report | <=16 KiB | Valid bounded representation, explicitly clip optional detail; also fit actual carrier allowance |
| Completed queue | <=2 MiB and <=256 records | Effective full-size capacity is 128 reports; drop oldest eligible operational record on overflow |
| Encoding/output buffers | <=256 KiB aggregate | Include current write, decoder staging and both-mode references/copies |
| Process telemetry-owned budget | 8 MiB target | All above plus bounded bookkeeping; not total RSS or a presently proved bound |
| Periodic aggregate emission | 1 second | Emit bounded interval summaries, not the full ring or every operation summary each second |
| Export allowance | 64 KiB/s, 256 KiB burst | Aggregate across destinations; both-mode copied bytes count for each destination |
| Output mode | forward | local/both explicitly configured; no hidden fallback |

The listed memory allocations total 6,614 KiB before remaining bookkeeping, leaving
1,578 KiB within 8 MiB. Include vector/string capacity, queue nodes, reports being
encoded or written, allocation rounding under the chosen accounting method and
simultaneous representations. Thread stacks and OS/runtime buffers need additional
declared accounting; this formula is not an RSS guarantee. If real structures do
not fit, revise and validate the profile rather than claiming an unenforced cap.

Existing timing limits are 1,024 nodes, depth 32 and 128 label bytes. They are
fixed structural limits today, not configurable byte limits. Source inspection
found retained-capacity gaps: owned label truncation can retain a larger String
allocation, public completed-node construction accepts an unconstrained name, and
JSON output has no encoded-byte limit. Tree conversion/attachment can overlap
allocations. Resolve or explicitly constrain these paths before claiming the
4/8 MiB budgets. See [recording](../../../../crates/layerfs-telemetry/src/timer/recording.rs),
[report construction](../../../../crates/layerfs-telemetry/src/timer/report.rs)
and [JSON output](../../../../crates/layerfs-telemetry/src/timer/json.rs).

A large timing tree may not fit 16 KiB. Produce a valid smaller projection with
omission/completeness fields; never cut arbitrary JSON bytes. Preserve operation
identity/outcome. Required benchmark detail that is clipped remains incomplete.

Periodic reports summarize their interval; they do not serialize the full 150 KiB
ring or all active summaries into a 16 KiB record. The one-second reporting cadence
does not promise delivery within one second: 2 MiB needs 32 seconds to drain at
64 KiB/s before accounting for new traffic. Retained reports have no additional
unbounded completed-operation history map.

| Host collector setting | Candidate |
| --- | --- |
| Registered producer slots | 16, with bounded identity/incarnation records |
| Collector-owned memory | 16 MiB target, including an <=8 MiB completed queue |
| Aggregate export allowance | 1 MiB/s with <=1 MiB burst |
| Host operational segments | 16 segments * 16 MiB = 256 MiB, active included |
| Optional daemon-local segments | 4 segments * 16 MiB = 64 MiB, active included |
| Operational expiry age | 24 hours; maintenance applies expiry, size/count can evict earlier |
| Age-maintenance interval | <=60 seconds while writer is running; also check on startup/rotation |
| Shutdown drain allowance | <=2 seconds and no longer than remaining shutdown budget |

Collector totals are aggregate across producers, not allocations granted afresh
to every daemon. Registered slots include connected/closing sources until their
resources are released. Reconnects reuse/release bounded producer state under an
explicit incarnation rule, never an unlimited historical-ID map. Collector loss
and omission counts are fixed counters with overflow signaled, not another queue.

If producer and collector roles share one process, account both roles: the candidate
combined owned-allocation ceiling is 24 MiB (8 + 16), not an implicit exception to
an 8 MiB process ceiling. Shared storage is counted once and no second sampler is
started for the same process. Validate the actual aggregate role budget at startup.
Sixteen producers at 64 KiB/s equal the 1 MiB/s collector allowance, with no
steady-state headroom; their simultaneous full bursts total 4 MiB. All rates count
the same encoded-byte domain. Oversubscription leads to explicit telemetry loss.

Age expiry is serviced at the declared maintenance interval while the writer
runs; stopped processes do not perform background deletion. Capacity is checked
before every write regardless of age. At 64 KiB/s, 64 MiB retains about 17 minutes:
24 hours is an expiry threshold, not a guaranteed deletion instant or retention
duration: maintenance delay and stopped writers remain explicit. Stored byte
limits include record formatting but exclude filesystem allocation/metadata
overhead; use a qualified quota if a physical-storage ceiling is required.
At the collector's full 1 MiB/s, 256 MiB retains only about 256 seconds. Neither
file retention nor a two-second shutdown allowance promises lossless delivery.

## 7. Configuration and cleanup enforcement

Configuration fields are proposed, not existing CLI flags. Assembly validates
master enablement, per-kind selection, collector source, sample/history sizes,
active-summary/timing budgets, report/queue limits, rate/burst, destination paths,
retention class and shutdown allowance before enabling recording.

Use checked arithmetic. A report must fit its queue and local segment; forwarded
diagnostics must also fit the negotiated carrier envelope after mandatory result
fields. Queue counts do not replace byte caps. Both-mode queues, references and
in-progress writes count against the same process budget; copied output consumes
the aggregate export allowance. Validate target/writer overhead, not only the
encoded payload lengths. Missing capabilities remain explicit.

| State | Release/eviction policy |
| --- | --- |
| Raw history | Fixed ring overwrites oldest at age/capacity boundary; report gaps, no fallback spool |
| Active operation aggregate | Scope guard releases on return/unwind; long operation stays until ended; cap additional summaries rather than evict a live one |
| Detailed timing state | Release after bounded conversion/encoding or omission; charge overlap until actually freed |
| Completed operational queue | Release delivered entries; evict oldest not currently being written on overflow; drop incoming if no eligible space |
| Current writer buffer | Bounded until completed/fails; never create another writer to escape a stuck one |
| Operational segments | Rotate before append exceeds size; remove oldest eligible owned closed segment before exceeding count/bytes |
| Process/collector state | Bounded producer registry; remove completed/disconnected entries after releasing owned output, without retaining unbounded tombstones |

Use one writer per managed output namespace and finite segment slots, not endless
per-operation directories. Refuse a conflicting writer. Only clean files owned by
that namespace; never recursively delete arbitrary configured-directory contents.
If deletion/rotation fails, stop that sink and report loss; do not create extra
segments. Account any temporary/staging files in the same retention budget.
A crashed partial final record is incomplete, not successful evidence; recovery
must not manufacture missing diagnostics. Operational handling may retire owned
segments under policy, but benchmark artifacts follow the separate rule below.

In-process readers/exporters must release retired segment handles or keep their
space charged: unlinking an open file need not release physical storage. The
configured payload cap is not proof of a physical-filesystem cap in the presence
of external readers. Do not recursively log an output failure through the same
failed sink; retain a fixed loss/health counter and use bounded external status
reporting where available.

On shutdown stop sampling and accepting new diagnostics, make a bounded drain
attempt, then discard remaining operational queue entries with available loss
accounting. Two seconds is a scheduling allowance, not a claim that arbitrary
blocking filesystem syscalls or thread joins are preemptible. Use cancellable or
nonblocking export where needed, do not unconditionally join a stuck writer, and
qualify actual process exit behavior. No telemetry sync/durability work is added.

## 8. Failure isolation and benchmark evidence

Expected measurement/output failures never replace the operation's original
Result or prevent required C1/C2 cleanup. No telemetry mutex is held across the
operation; no synchronous disk/network export occurs in the hot handler. Bounded
snapshot/queue access must refuse optional detail rather than wait indefinitely.
Unavailable collectors, lock contention, malformed observations and encoding
limits produce typed diagnostic availability/loss, not unwrap/panic or replay.

The contract does not promise immunity from allocator aborts, unsafe native bugs
or process crashes. Qualify collector code and retained-byte accounting; external
collection offers stronger isolation. Do not catch a product panic to hide it.
An operation-response connection failure remains a real transport failure and may
leave a mutation unknown; it is not automatically just a telemetry problem.

Operational output can rotate/drop as declared. **Benchmark receipts remain
append-only**: never delete failed attempts or rewrite historical evidence to fit
retention. Before a new run, validate output capacity; exhaustion preserves old
artifacts and marks required new evidence incomplete/unrun/failed as applicable.
Bound a benchmark campaign through admission, not automatic evidence eviction.
Missing required telemetry prevents an evidence PASS while leaving a known
successful product result unchanged.

## 9. Implementation admission and checks, all NOT_RUN

1. Preserve existing Timing API, original Result/panic behavior and !Send/!Sync
   scope ownership. Review retained-capacity/encoding corrections explicitly.
2. Prove master-off creates no telemetry observations, allocations, workers, files
   or exports with lazy labels; required product enforcement remains active.
3. Qualify macOS/Linux observations, scope/units and unavailable values. Existing
   harness code is reference, not a ready product dependency or universal proof.
4. Test long operations across ring wrap, overlapping operations and process
   restart/counter discontinuity; no false per-operation CPU attribution or peaks.
5. Test all count/byte boundaries, serialized expansion and conversion overlap;
   full queues, denied paths, full disks and failed rotation cannot grow budgets.
6. Verify forward/local/both, slow/broken stderr/collector and disabled sinks,
   report identity, absence of container files in the main forward-mode route,
   and bounded shutdown without corrupting operation stdout.
7. Verify guarded cleanup on success/error/unwind, independent telemetry failure,
   producer churn and preservation of benchmark evidence.
8. Measure overhead with an identity-matched declared profile and the existing
   measurement rules; no tuning samples/caps to hide a failure or aggregate CI gate.

No new production modules, native dependencies, monitoring protocol or benchmark
family are created by this proposal. Implementation should add only the smallest
working collectors/recorder/output adapters needed by the real initial route.
