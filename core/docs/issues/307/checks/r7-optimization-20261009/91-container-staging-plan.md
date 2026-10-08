# Stage 0 container input staging

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Source inspection found a concrete harness gap: Sandbox creation uploads only
the executable and configuration. Each selection starts a new container, so a
configuration boolean cannot establish that /code, /replay or native fixtures
exist. The current runner starts warmup immediately after protocol readiness.
Native manifests also pin actual Linux inode identities; their hashes cannot be
known before the native copy is created.

Taken under the owner's direction of 2026-10-09: add one explicit untimed staging
step after the original container acknowledgement and before any Mount, warmup
or cache predicate. It is harness code only, with no product/Rust ABI, image,
network, dependency or persistence-profile change. The host remains control-only
for the L data path. It never materializes a Workspace root for L.

Inputs are a prospectively sealed deployment manifest and staging implementation
with their SHA-256 identities, expected pinned image, exact owned container,
ordinary uid/gid, and a declared setup wall stop. Static workload/oracle/cache
helpers and closed replay artifacts go only to their declared /code and /replay
paths. Native/P input copies come only from this stage's owned /tmp prepared
roots, with complete source content/metadata seals and declared peer identities.
The preserved source checkout is never a command working directory or write
target. Reject overlapping roots, unsealed/missing inputs and foreign resources.

Staging uses external Docker operations against only that acknowledged owned
container. The SDK ordinary-command nonroot guard stays intact. Each copy,
metadata action and verifier is attempted once; failures retain their original
phase, exact container and available operation identity. No replay, image rebuild
or guessed cancellation follows a failed transfer. The lead alone stops a
retained container. Staging cannot write global Store data, Overlay data, mounted
Workspaces, fusectl or system configuration.

Validate actual deployed helper bytes and native contents/supported metadata.
Record ownership, modes, symlink targets and internal hardlink relations; do not
assume archive transport preserves nanosecond timestamps. Generate each native
inode-ordered cache manifest from its actual final container copy, seal it, then
bind its hash/path/cardinality into the cache treatment before any hint or
measured operation. The fresh setup receipt, raw outputs, source/deployment
identities and actual manifest hashes remain separate from performance phases.
Setup warmth never establishes cold eligibility: the existing per-file hint and
mincore observation still decide A, and every mismatch stays ineligible/unrun.

The matrix agent owns only the new harness deployment implementation, runner
configuration/lifecycle integration, external owning tests and runtime README.
No product file is assigned. The lead owns input preparation, all executions,
records, source seals and Git. Verify the new setup contract and actual transfer
before the full baseline; preserve previous tests and failed/incomplete receipts.
