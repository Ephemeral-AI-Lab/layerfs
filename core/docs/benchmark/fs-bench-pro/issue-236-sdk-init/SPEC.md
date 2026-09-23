# #236 host-direct SDK `init_namespace` benchmark selection

> Frozen before the first SDK-route timed sample. This adds a separate route to
> the existing #231 case shapes; it does not change or promote daemon-host
> receipts. Source basis: `67946e5759302d3bdca27aeb482d56384068557d`.

## Claim and selected cases

The question is whether the real agent-facing
`layerfs_sdk::Client::init_project(name, host_path)` completes the 100-file
(5,000,000-byte) and 1,000-file (20,000,000-byte) `init_namespace` cases and
what one complete SDK call costs on this host. These are the only two registered
SDK-route cases, in that order, each with seed 1 and one sample. The case IDs,
file class counts, directory structure, modes, mtimes and byte apportionment
are those in [#231's registry](../issue-231/SPEC.md). The 10,000 and 100,000
cases remain visible as `NOT_RUN` in this two-case SDK selection; an explicit
later selection may use the same SDK route with a new receipt. The new fixture profile
is `core-sdk-init-fixture-v1`; the SHAKE input route and operation route are
`host-direct-sdk-v1`. No control arm, repetition, best-of selection or
cross-route speedup ratio is defined.

## Operation and timing

The host-side benchmark driver is an ordinary Cargo example in `layerfs-sdk`.
It creates a fresh default-policy Store and writable history authority outside
the operation timer, then makes exactly one public `Client::init_project` call.
The monotonic `Instant` timer starts immediately before that call and ends
immediately after the typed published-genesis result. Source validation, file
scan and reads, C1 construction, C2 saves and C5 publication are inside the
call. Result formatting, process launch and Store/history creation are outside
that timer but inside the separately measured complete command wall. The
Service and Store are on the macOS host; there is no daemon, container, FUSE,
pre-saved root or pathless bootstrap. No setup or verifier read touches the
source bytes to prime the timed operation.

The public operation is the only mutation route. The driver returns the
project/LayerStack ID, genesis Layer, root, root serial and operation duration.
One independent child, after the timed process exits, reopens Store/history
and verifies every path, portable mode/mtime, byte count and SHA-256 against
the sealed source manifest through public C1/C2/C5 readers. The verifier is
never included in operation or complete-command timing.

The existing `core/benchmark/fs-bench-pro/families/init_namespace.py` owns the
fixture and SDK public-call invocation, and the existing `runner.py` owns
`list`, `run`, `verify` and `report`. This replaces that runner's Init route;
there is no parallel SDK benchmark script or selectable daemon-host Init arm.
Historical daemon-host receipts retain their recorded route and values.

## Status rules and limits

For each case, **functional benchmark PASS** requires one confirmed SDK Init,
one matching genesis publication, full independent verifier PASS, a complete
performance command of at most 15 s, an independent verifier of at most 5 s,
and successful process cleanup. A failure or miss is retained with its raw
number and classified plainly; a selected case is never dropped. The Cargo
build, including first use, has a separate 30 s ceiling. The two-case family
cycle has a 30 s recommended budget, reported even on a miss. No small
exception, timeout increase or workload reduction is declared.

The source cache is **`source-cache-uncontrolled-v1`**: no cold invalidation
or whole-input residency proof exists for these cases. No numeric SDK latency
target or matched baseline is approved. Therefore a functional PASS is still
`admission_eligible=false`, `performance_gate=NOT_FROZEN`, and **performance
INELIGIBLE**. Report each single raw `operation_ns`, complete-command wall,
verifier wall, build wall, source-cache condition, Store/history size, lifecycle
CPU, and case status; do not call the raw number a cold result, median, or
performance PASS. The separate daemon-host numbers are context only.

## Custody and output

Run only from a clean committed worktree, with Cargo target, prepared sources,
fresh Stores, scratch and results under that worktree. Pin source commit and
tree, Core product seal including nested API packages, harness seal, Cargo
lock and root build config, binary SHA-256, fixture profile/route/seed and
manifest SHA-256. Use a fresh non-overwriting output directory and the
worktree-local nonblocking run lock. Retain stdout, stderr, exit/failure,
single raw sample, separate verifier receipt, report, Store/history, and a
manifest of evidence-file hashes. Read-only `verify` and `report` operations
must not invoke the product again. A dirty or changed identity is diagnostic
and cannot inherit an earlier result.

MCP/CLI, Workspace lifecycle, exec, the 10,000/100,000 tiers and the four-tier
#231 gate are outside this two-case SDK selection. No historical receipt is
rewritten or relabeled.

## v2 amendment: SDK-owned host setup

The v1 receipts at source `811ad517e7e21b8a20b978bb06d39a579135ac91`
remain immutable. Before any new sample, v2 moves fresh Store/history and
local authorization setup from the Rust benchmark example into the reusable
`layerfs_sdk::Host::create` constructor. The example imports only `layerfs-sdk`,
borrows its `Client`, and still times exactly one `Client::init_project` call.
The independent verifier remains outside the timer and deliberately reads
through public C1/C2/C5 APIs. Python prepares source bytes, launches the SDK
example, and records evidence; it never performs backend Init work.

This route is `host-direct-sdk-v2`, fixture profile
`core-sdk-init-fixture-v2`, scenario version 2 and receipt schema
`core-fs-bench-pro-sdk-init-v2`. The two case shapes, seed, one-sample rule,
15 s command, 5 s verifier, 30 s build, cache contract and status rules above
are unchanged. The v2 SHAKE route changes source bytes and manifest digests.
V1 and v2 numbers are separate observations and are not pooled or compared as
a treatment pair.
