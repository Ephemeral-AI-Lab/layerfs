# v0.1.7 fs-bench-pro migration

> **Status:** The #235 first-pass runner is implemented; broader migration and
> performance admission remain proposed. This page approves no numeric result.

Tracking: [v0.1.7 migration issue #230](https://github.com/Ephemeral-AI-Lab/layerfs/issues/230).
The [shared substrate #235](https://github.com/Ephemeral-AI-Lab/layerfs/issues/235)
precedes the [#231 first-pass spec](issue-231/SPEC.md): one run each for 100,
1,000 and 10,000 native Init files. The 100,000 row remains `NOT_RUN` and the
original four-tier final gate remains open.
The later [four-tier SDK diagnostic](issue-231/SDK-FOUR-TIER-RESULTS-20260924.md)
retains one fresh observation per case and does not clear that gate.
Owner direction now requires release binaries for new SDK Init rows; the
[release-only four-tier result](issue-231/SDK-RELEASE-FOUR-TIER-RESULTS-20260924.md)
also leaves the gate open.
The owner-approved [lite verifier follow-up](issue-231/SDK-VERIFIER-LITE-RESULTS-20260924.md)
passes sampled content verification on all four tiers while leaving
performance eligibility open.
The [handoff prompt](issue-231/HANDOFF.md) lists what an implementation agent
must read, do and avoid; the new Init family test path and separate full
verifier are specified there and in the first-pass spec.

This proposal ports the **benchmark pipeline** of the root
[`fs-bench-pro`](../../../../benchmark/fs-bench-pro/README.md) to the v0.1.7
product. It keeps the case registry, sealed build/input reuse, isolated sample,
one selected case, the product's `layerfs-telemetry` timing path, separate
verification, cleanup, and append-only evidence flow. Product execution becomes
a recorded registry field so a run cannot silently change routes. The first
runner uses one `daemon-host` route and four operations (`list`, `run`, `verify`,
`report`); it adds a second mode only with its first real case. The separate
worktree-local run locks never serialize agents in different worktrees.

## Three prerequisite pilots

The migration starts with three complete family clusters, each tracked as a
sub-issue of [#230](https://github.com/Ephemeral-AI-Lab/layerfs/issues/230):

| Pilot | Required membership | Product route to qualify |
| --- | --- | --- |
| [#231](https://github.com/Ephemeral-AI-Lab/layerfs/issues/231) | First pass: 100/1,000/10,000 native Init; retain 100,000 as `NOT_RUN` under the original final gate | Public native Init, including source reads and construction |
| [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232) | All three active SDK `edit-*` families: 56 cases | Public SDK range edit into the live Workspace, then explicit Commit |
| [#233](https://github.com/Ephemeral-AI-Lab/layerfs/issues/233) | `tiny_file_churn` (20) and `local_snapshot` (one lifecycle): 21 cases | Ordinary mounted POSIX/FUSE operations and complete Commit lifecycle |

Freeze each selected family's full v0.1.7 case, operation, cache and numeric
contract before its harness implementation or samples. A pilot passes only when
every mandatory case has eligible performance, separate verification,
cleanup and custody PASS; retain every nonpassing attempt. Do not begin migration
implementation or collection for remaining families until **all three pilots
pass**. A `storage-direct` component result can exercise shared harness plumbing
but cannot clear a pilot. `mixed_load_bearing` and the multi-branch,
multi-Workspace and long-history families follow under their own contracts.
See the [pipeline gates](pipeline-and-modes.md#prerequisite-pilot-gates).

## Two modes

These are the eventual #230 claim scopes. The #235/#231 first pass implements
only `daemon-host`; it has no control/candidate arm or cross-mode comparison.

| Mode | Measures | Does not claim |
| --- | --- | --- |
| `daemon-host` | The selected v0.1.7 operation through the real daemon and host-side product route. The caller's monotonic interval covers the declared end-to-end boundary. | A result from the daemon alone or a component speedup. |
| `storage-direct` | A public C1-to-C2 storage pipeline with daemon and transport omitted. | Workspace Commit, daemon, Service, authorization, wire, FUSE, or end-to-end performance. |

The exact daemon/host placement is pinned from the implemented product route in
the run identity. `storage-direct` must call public product APIs; raw SQL, private
C1/C2 functions, and benchmark-only shortcuts are excluded. Main does not expose
a public Workspace-to-Store Commit bypass: the existing Stage 6 `pipeline.*`
route is C1 construction/edit plus C2 save/ack, not Workspace Commit. Keep it
under its frozen Stage 6 identity. These modes have different operation
surfaces, so their numbers are separate views, not a speedup pair or a pooled
distribution. A true direct Workspace Commit row stays `NOT_RUN` until the
product exposes that public route or its operation contract is deliberately
defined as the C1-to-C2 pipeline.

Initialization also needs its own operation match. The current `InitLayerStack`
is a bounded bootstrap with pre-saved file roots, not v0.1.6's native directory
import. Do not register or report an inherited `init_namespace` case as an
equivalent v0.1.7 result. The [ASCII route and limit map](pipeline-and-modes.md#initialization-equivalence-gap)
records the native import path and the former 128-entry test ceiling.

Names are deliberate. In [#193](https://github.com/Ephemeral-AI-Lab/layerfs/issues/193),
`direct` enters the **Service handler** without wire delivery and `forward`
crosses the daemon/network boundary. Neither name means `storage-direct` here.
Keep #193's direct/forward contract and the Stage 6 component campaign separate.

## Four macro timing scopes

Each case and arm has **one performance sample**. Its public caller interval is
the headline. The other scopes locate work within that sample or, for delivery,
define a separately registered same-handler comparison. They are not four
additive numbers or four runner modes.

| Scope | Question and owner | Timing boundary |
| --- | --- | --- |
| 1. Public workflow | #231 native Init, #232 SDK edit and Commit, #233 FUSE mutation and Commit | One caller monotonic interval from the registered public start to acknowledgement |
| 2. Daemon-to-host delivery | [#193](https://github.com/Ephemeral-AI-Lab/layerfs/issues/193) compares the **same Service handler** through `direct` and daemon/network `forward` | One caller interval per matched route; daemon and Service local spans are diagnostics, not a socket-only duration |
| 3. C1/C2 engine | Stage 6 and #230 `storage-direct` call public C1/C2 APIs on the host | C1 construction/edit and C2 save/ack on the same process clock; Service spans show the integrated path |
| 4. C5 history | Init reservation/publication and Commit stage/branch publication | Inside the public Init/Commit interval; named Service spans are still needed for a C5 duration |

C5 is a fourth **timing scope**, not an independently approved optimization
campaign. Its stage validation, Commit insertion, Branch compare-and-swap and
stage removal remain measured within the public operation. The
[macro/micro map and attribution rules](pipeline-and-modes.md#four-macro-timing-scopes)
show what is timed today and what still needs a named span.

## Documents

- [Pipeline and modes](pipeline-and-modes.md) — phases, route boundaries, and
  proposed code/results layout.
- [#231 handoff](issue-231/HANDOFF.md) — agent reading list, implementation
  order, focused Init test and full verifier responsibilities.
- [Parameters and telemetry](parameters-and-telemetry.md) — selectors, resource
  limits, use of the existing telemetry API/configuration, and receipt fields.
- [Telemetry ingestion and retention](telemetry-ingestion-and-retention.md) —
  parse `LFT1`, retain one raw event file, and recycle temporary captures after
  evidence validation.
- [Preparation and cache discipline](preparation-and-cache.md) — fixture reuse,
  per-sample setup, and cold-cache admission.
- [Simplification and speed](simplification-and-speed.md) — safe reuse paths,
  targeted iteration, and code-reading candidates to profile.
- [Agent workflow](../../../benchmark/fs-bench-pro/AGENTS.md) — normative
  one-sample, one-seed, and focused-check instructions for the implementation.

## Existing contracts and handoff

- The root [measurement rules](../../../../docs/general/benchmark_rules.md),
  [benchmark rules](../../../../AGENTS.md), and
  [fs-bench-pro quick start](../../../../benchmark/fs-bench-pro/QUICKSTART.md)
  remain governing references.
- [Stage 6's frozen contract](../fs-bench-pro-storage-content/CONTRACT.md) and
  its 217 registered admission rows plus 3 diagnostic rows remain historical
  #171 evidence. They cover public C1/C2 APIs, not Workspace Commit or
  daemon/runtime behavior. Do not relabel their `pipeline.*` rows as either new
  mode. Their separate harness resource instruments remain governed by that
  frozen contract; do not mix those fields into native `LFT1` telemetry.
- The [direct/forward benchmark proposal](../../architecture/proposal/service-daemon-transport/implementation/04-benchmark-direct-forward.md)
  remains the contract for #193. Coordinate the runner boundary with it; do not
  duplicate its Service verification matrix under a new name.

Issue #179 is closed with a bounded real-daemon Workspace/FUSE route and a
small two-Commit canary. It supplies a substrate integration test, not native
Init or load/performance proof. #231 still needs the public import, frozen
operation boundary and independent oracle. See the
[readiness gates](pipeline-and-modes.md#readiness-before-implementation-or-collection).

When implementation first starts under `core/benchmark/fs-bench-pro/`, migrate
any still-needed Stage 6 preparation, case, and receipt rules into their new
canonical home, preserve the old receipts and rulings, then remove
`core/docs/benchmark/fs-bench-pro-storage-content/` as requested. That point has
not been reached by writing this proposal; leave the frozen directory intact
until then.
