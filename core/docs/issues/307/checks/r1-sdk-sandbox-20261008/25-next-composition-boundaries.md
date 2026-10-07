# R1 continuation: actual daemon and ordinary Sandbox composition

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Follows real facade subcheckpoint `932abe80894e070d4767eba2c213dc5d38a64d8f`.

R1 remains IN_PROGRESS; native Goal remains active. Next main-owner work uses
current components to build the actual production daemon and ordinary Sandbox.
Read-only source reviewers identified the following exact implementation seam.

1. Split genuine constructor-dependent Commit dispatch from ordinary controls.
Add a production non-Commit service path that refuses unavailable namespace Commit
before capture/Save, sharing original receive/encode/send/Served custody rather
than passing a placeholder constructor or adding a second Commit driver.
2. Enforce explicit Disposable metadata in daemon receive_install before claim,
file writes, rename/open; retained Durable implementation may compile but no
Durable test/proof/application/diagnostic executes.
3. Compose one long-lived install-then-serve process: protected static key/config
file, actual Overlay start once, authenticated listener/install-pending state,
one receive_install, transfer its actual OpenedStore into Service without reopen,
and bounded concurrent control connections. Add exact daemon incarnation and
ControlReady observation distinct from FUSE Ready. Optional existing-store mode
opens exactly once with explicit Disposable/no conversion or fallback.
4. Preserve/relocate excluded Sandbox before real path replacement. Actual
Sandbox lifecycle owns only its admitted container/local backing, preserves
shared Store volume/history and retains partial/unknown effects. Add explicit
Sandbox guard/dependency/unsafe-free coverage and SDK→Sandbox edge only with real
implementation. No old API-core/Server/host runtime/session/retry dependency.
5. Ordinary same-container Docker exec is initial mount visibility topology;
explicit configured nonroot uid/gid, no privileged exec, protected0700 directories,
0600 databases/config/keys, protected /dev/fuse/fusectl, and actual uid/groups/
CapEff/CapAmb/NoNewPrivs and /proc alias proof. Docker exec inherits creation-time
environment, so credentials never enter container env or CLI argv. Copied namespace
or separate-container mount visibility needs its own actual propagation oracle.
6. Standard runtime streams use caller stdio or bounded per-stream windows, no
Command::output lifetime collection, runtime/output cap or command classifier.
Explicit cancellation names its real runtime target/scope: killing local Docker
CLI is client loss, not remote command/descendant completion. Whole-Sandbox stop
is separately caller-requested lifecycle scope; no filesystem force kills callers.

Actual source/config/auth readiness, ordinary execution/access proof and each
failure/unknown/custody boundary must be retained before R1 acceptance. R2 native
readiness, R4 captured namespace/incremental topology, R5 live Commit and R6
sustained/forced cleanup remain separate required work. Root alone edits/runs
serialized locked builds and bounded tests/proofs, applies affected scoped final
checks and exact LOC, with all historical/protected resources preserved.
