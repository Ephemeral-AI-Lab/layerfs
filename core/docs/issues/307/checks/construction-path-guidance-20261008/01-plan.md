# Shared construction guidance update

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Owner request2026-10-08: update AGENTS.md after the Init/daemon construction
explanation. Source parent19806d692; no product implementation change.

- Change root AGENTS.md: require shared Content/Storage/Persistence construction,
  distinguish input ownership and Save completion from whole-Store sealing;
  route implementation details to core/AGENTS.md.
- Change core/AGENTS.md: record native Init and captured-file entrypoints,
  common FinalizedObject/Save sink, backed mutable state, current S10 gap and
  platform/lifecycle scope of the macOS seal allocation correction.
- Reuse current Project import, Workspace captured construction, Storage Save,
  daemon Store Commit, SDK Init, architecture56/65 and the seal report as evidence.
- Retain documentation/link/whitespace checks and exact first-parent/staged LOC
  in this directory; one local documentation commit. No Rust/test/benchmark
  rerun is needed because product, harness and manifests remain unchanged.
- Preserve protected notes, historical receipts, disabled Durable execution,
  source/layout contracts and all unrelated work. No remote issue mutation/push.
