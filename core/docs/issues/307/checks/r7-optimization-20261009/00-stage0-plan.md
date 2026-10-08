# R7 Stage 0 plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Initial identity `2f8e1ec42`, product `2b4dc28a6`. This plan precedes product
edits. Build the real release daemon under the checkout lock and pinned image;
implement harness L through public ProjectApi/WorkspaceApi and actual binary
startup, N through the same external runtime, P as existing-dependency harness
passthrough under identical kernel profile. Commands are external bash, never
registered in the filesystem daemon. Host Init works from one independent
fixture copy, seals once, installs once into an owned named VM volume. Retained
daemon Store remains open across Saves and Commits.

Use the exact prospective workload matrix, scoped oracles, class A/B/C contracts,
per-file hint plus residency measurement, immutable identities and full required
counter/resource inventory. Register before sampling and leave every failed,
ineligible and unrun selection visible. Correct old evidence lane bound from 18
to 34 and run harness tests. Initial setup is not product timing.

Subagent ownership is disjoint: matrix agent owns new fs-bench-pro/r7 and the
stale evidence bound/tests; runtime agent owns new benchmark/r7-runtime;
environment agent owns new benchmark/r7-passthrough and benchmark/r7-cache.
Lead owns benchmark/r7-tools, integration, records and all execution/Git work.
These are harness paths and carry no production LOC. No source layout claim is
made by this ownership description.

Current source diagnostic gap: real-wire Status exposes only activity/native
aggregate/engine status; Commit omits namespace construction and storage
diagnostics; per-opcode accounting is internal. Before baseline, enumerate exact
missing observations and add a bounded legitimate additive diagnostics surface
if needed. Preserve old wire bytes and all custody/failure/permission contracts.
Document and commit the narrowed product plan before editing product files.

Functional lifecycle proof comes first: host Init/seal/install, real mount,
external command, changed Commit, normal unmount, fresh mount, exact oracle.
Then full baseline, scaling sweep, counts-led subagent analysis and disjoint
candidate implementation/review. No optimization acceptance precedes speed and
storage at sealed identity plus both gates. Two empty iterations and full final
proofs are required for success. An environmental blocker instead closes only
as truthful INCOMPLETE after independent work is finished.
