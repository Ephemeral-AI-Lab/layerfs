# Pair 3 implementation specification, including telemetry

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

**2026-09-20 owner revision:** read the [optimization specification](optimization-spec.md)
and [optimization handoff](optimization-handoff.md) before implementing this packet.
They preserve the agreed multi-writer/accepted-duplicate design, revise the
unqualified socket-buffer and large-load profile, and distinguish source/evidence
custody from qualification. Earlier global A=1 recommendations do not govern the
revised design. The older proposal and receipts remain available as history.

This packet turns the reviewed service/bridge/daemon design into implementation
work. In the current request, **C3 means co-design pair 3 / #181**, the foundation
for later Workspace/FUSE. It does not rename older component/risk classifications
or move pair 1 ahead of the established pair 3 -> pair 1 -> pair 2 sequence.
No runtime implementation, test result or performance qualification is established
by writing this specification. All milestones and acceptance results start NOT_RUN.

## 1. Read and implement these contracts together

| Document | Responsibility |
| --- | --- |
| [01-product.md](01-product.md) | Product boundaries, five operations, daemon/service startup, transport and C1/C2 composition |
| [02-telemetry.md](02-telemetry.md) | Single-crate telemetry changes, startup wiring, operation wrappers, disable/failure behavior and output |
| [03-verification.md](03-verification.md) | Acceptance cases, environments, feature/source checks and evidence rules |
| [04-benchmark-direct-forward.md](04-benchmark-direct-forward.md) | Draft later benchmark-verification contract: direct/forward execution, matched requests, phases, resource evidence and proof reuse |
| [05-issue192-handoff.md](05-issue192-handoff.md) | Implementation agent prompt for #192, including specification custody, isolated worktree/build ownership, milestones and completion evidence |

The parent [design index](../README.md), [file map](../02-file-layout-and-boundaries.md),
[operation catalog](../07-public-operations.md), [transport state contract](../03-operations-and-transport.md)
and [telemetry/retention proposal](../08-telemetry-and-retention.md) supply the
supporting detail. This packet gives execution order and exit criteria; it does
not silently promote their illustrative numbers or unresolved choices into
approved defaults. Follow [core agent rules](../../../../../AGENTS.md) and the
[repository rules](../../../../../../AGENTS.md).

The direct/forward benchmark driver is a **later implementation step** after the
product's prerequisite functional and deployment checks. Its case contract must
be committed/frozen before harness work or measurement. The draft creates no
runner flag, changes no Stage 6 row, and has no approved numeric performance gate.

## 2. Deliverable and mandatory deployment

Implement three new product crates and extend the existing telemetry crate:

- `layerfs-bridge`: shared logical contract, one initial native delivery adapter,
  bounded frame/metadata/payload handling, client and supplied-handler server.
- `layerfs-service`: native startup, configured Stores, authorization/admission,
  input capabilities and real C1/C2 operation handlers.
- `layerfs-daemon`: actual Linux executable, validated configuration and bounded
  headless stdin/stdout submission through its bridge client.
- `layerfs-telemetry`: keep the existing timing API; add reviewed portable reports,
  recorder composition and explicitly enabled native monitor/output modules in
  the **same crate**, not a second telemetry-runtime package.

```text
 macOS HOST                                  LINUX DOCKER CONTAINER
 +--------------------------------------+    +------------------------------+
 | External test driver                  |--->| stdin -> daemon headless     |
 | input/oracle + bounded report sink    |<---| stdout <- operation results  |
 |                                      |<---| stderr <- bounded diagnostics|
 | Native service process               |    |                 |            |
 | bridge server -> supplied handler <--+----+---------- bridge client      |
 |                    |                 |    |      authenticated network   |
 |             C1 <-> C2 -> SQLite       |    +------------------------------+
 +--------------------------------------+
```

The network crosses actual daemon/service processes. Direct calls are secondary
semantic parity. No FUSE, `/dev/fuse`, bind-mounted host directory, shared-data
volume, Workspace directory or payload/spool/result files in the container's main
acceptance route. Forward-mode telemetry creates no container report files.
Optional local/both telemetry is a separate, explicit output-mode test.

Real host-owned SQLite persistence is required. A host-prepared filesystem fixture
is logical Store content, not a mounted container Workspace. Fixture preparation
uses external code and public core APIs; no private test hook or mock product path.

## 3. How telemetry plugs into the foundation

```text
 PROCESS STARTUP
 native assembly
   -> resolve explicit product and telemetry configuration
   -> choose master-off or construct one process monitor/output owner
   -> supply concrete service handler / daemon client

 SERVICE OPERATION
 bridge receives request + verified context
   -> optional OperationRecorder resource-window reservation
   -> existing Timing::record or Timing::disabled
        -> service authorization/admission and real operation
             -> C1/C2 receive existing child TimingScopes
   -> original product Result, independently complete/incomplete diagnostics
   -> bounded nonblocking diagnostic submission

 PROCESS RESOURCE OBSERVATION
 one observer continues across operations and idle periods
   -> bounded recent ring and constant-size active-window aggregates
   -> periodic bounded reports to the configured output owner
```

Operation-owned byte/frame/replay counters remain distinct from process CPU/RSS.
Multiple simultaneous operations share process observations; do not assign the
entire process delta exclusively to each request. Use local monotonic windows,
process incarnation and report identity. C1/C2 need no sampler, output path or
retention policy; live timing scopes remain on their owning execution context.

Master-off is selected before creating telemetry-specific labels, buffers,
collectors/workers/files or exports. Mandatory product clocks, authorization and
resource enforcement still run. Expected telemetry failure never changes the
original storage outcome or skips its cleanup. A lost operation response remains
a genuine transport failure, potentially leaving a mutation unknown.

## 4. M0: freeze the implementation inputs

The following are explicit prerequisites, not prompts to invent defaults during
coding. Record concrete choices and reasons in a versioned freeze record beside
the implementation work before changing dependent product behavior.

| Freeze item | Required recorded decision |
| --- | --- |
| Source and qualification | Exact core revision/tree, lockfile and policy/schema identities; reconcile concurrent Stage 6/7 changes and affected findings |
| Deployment | Actual host/container versions, build targets, network route/listen endpoint and supported process lifecycle |
| Transport/security | One carrier (TCP candidate), actual authentication and confidentiality/integrity profile, credential lifecycle and caller-to-Store permissions; no custom unreviewed cryptography |
| Wire and operations | Header widths/byte order/version, exact five-operation schemas including Inspect variants, root/identity representation, error/outcome mapping and valid decoder bounds |
| Workload/resource envelope | Required input/edit/tree cases, per-op total bytes/counts/replay, frame/windows, C/A and Q=0, deadlines, synchronous execution ownership and output caps |
| Telemetry | Explicit enablement and feature wiring, safe native observation APIs, bounded report representation, startup/operation sampling windows, output route and validated retention/budget profile |
| Verification | Map every required case to an external test/real deployment proof; identify unavailable observations and required evidence rules |

Implementers may resolve routine library, module, signature and endpoint choices
within the authorized scope and record them here; no additional owner permission
is implied by the existence of this gate. A substantive unsupported capability
must be reported with evidence, not hidden behind a fallback, dependency patch,
smaller workload or unqualified success. New performance work needs its committed
case contract/issue linkage under the measurement rules before a harness/run.

As of this specification, M0 is **NOT_FROZEN**. The candidate telemetry profile
and buffer example are inputs to that decision, not a complete performance SLA.
Do not claim implementation-ready or qualified while these dependencies remain
unresolved. The observed checkout changes during other agents' work; HEAD alone
does not identify all source/evidence needed by the final integration.

## 5. Implementation sequence and exit conditions

| Milestone | Work | Exit condition |
| --- | --- | --- |
| M0 | Freeze the inputs above | Concrete source/security/schema/resource choices and case mapping recorded |
| M1 | Telemetry foundation | Existing timer compatibility and disabled path preserved; required label/capacity/encoding corrections qualified; minimal operation-recorder composition works without native monitoring |
| M2 | Product contract, native bridge and service slice | ConstructFile and ReadFile call actual C1/C2 through the common authorized handler; external codec/failure checks; direct parity available |
| M3 | Actual Docker daemon | Real Linux executable sends input/results across the network to native macOS service, with no shared-file shortcut; forward diagnostics stay separate from stdout |
| M4 | Complete product operations and multi-daemon behavior | Inspect, EditFile and prepared-tree update added; five-operation/error cases, old-root readback, Q=0 admission and caller/response isolation checked |
| M5 | Native CPU/memory and bounded output integration | Shared process observers, long-window summaries, forward/local/both, byte/count/rate/retention enforcement and telemetry failure isolation checked on actual targets |
| M6 | Final qualification and handoff | Full relevant core/feature/boundary checks, real topology evidence, source identities and actual production LOC recorded; every missing/failed proof explicit |

Do not build a full monitoring/export framework before proving the first real
construct/read route. M1 supplies the safe/off composition needed by that route;
native observation/output work can be developed alongside independent product
tasks after M0, with file ownership agreed. M5 must pass before claiming the whole
telemetry-inclusive scope complete. Every new member/module contains real code;
no empty future carrier/provider crates.

## 6. File ownership and review boundaries

Use the [responsibility-folder map](../02-file-layout-and-boundaries.md).
Implementation agents own disjoint modules or coordinate public-type changes
before editing consumers. Keep contracts and error semantics reviewed together
without copying algorithms across crates. All production files <=999 physical
lines; `lib.rs`/`mod.rs` <=200 and declarations/delegation only. Tests/examples
remain external; no product cfg(test), private-source inclusion or fault hooks.

Main change boundaries are bridge contract/native adapter, service operation/input/
native assembly, daemon config/headless lifecycle, and the one telemetry crate.
Keep C1/C2 algorithms unchanged unless a specific required contract correction is
identified and reviewed separately. Do not silently change schemas, canonical
profiles, worker counts or durability to make integration pass.

Retain legacy root crates as reference only. No dependency, include, fallback or
binary reuse from them. No third-party patches, vendor edits or unbounded retries.
Do not run the retired preflight or add an equivalent aggregate gate/CI workflow.
Respect other agents' files and the existing measurement lock; this specification
does not authorize overlapping resource-sensitive work.

## 7. Evidence, LOC and exclusions

The [verification specification](03-verification.md) defines check categories and
commands; it is not an already-run test suite. Keep product correctness, resource
evidence and measured performance claims separate. Histories/receipts remain
append-only; operational telemetry rotation cannot delete them.

The [telemetry LOC baseline/estimate](../08-telemetry-and-retention.md#production-loc-baseline-and-estimated-growth)
records 763 measured existing production lines at its pin and estimated growth.
It is not the current implementation's actual delta. Count exact parent/staged
production snapshots for every code commit with the repository counter, including
runtime source outside Rust if introduced. Report reference/core/adapter totals
and avoid counting integration wiring twice across plans. Documentation and tests
are excluded from production LOC; no fictitious zero-sized-product count.

Explicitly excluded: Workspace/FUSE mounting and backing, new inode allocation,
history/Commit publication, atomic composite saves, additional carriers, managed
database providers, durable telemetry or Store acknowledgements, in-band CANCEL,
waiting schedulers, mutation replay and generalized plugin/collector registries.
Those belong to separately reviewed later integrations, not hidden prerequisites
for this foundation.
