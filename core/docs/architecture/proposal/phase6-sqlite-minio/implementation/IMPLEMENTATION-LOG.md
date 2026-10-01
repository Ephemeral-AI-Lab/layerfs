# Phase 6 integration implementation log

> Status: Dated planning checkpoint; not release evidence or a product contract.

Append-only checkpoints. Current action/gates are maintained in CHECKLIST.md.

## 2026-10-02 — full autonomous objective received; V4a selected

Parent `1a59e129f8a3d73f49efe9691b53ba4b377be612`, clean owned worktree/branch
confirmed; active goal read from app state. Previous discussion/source inspection
is evidence progress, not implementation completion. Current V3 path has actual
full-provider two-head proof, but 512-inode/full-reconstruction/full-audit scope
cannot prove the user's larger/local-update objective. Those gaps remain required.

Confirmed source cause candidates: per-locator Noise/TCP setup, unary packs/PUTs,
repeated exact-CAS lookup, full metadata reconstruction and whole candidate audit.
Existing C1 already has EditSequence/apply_edits and PreparedRows/filesystem COW;
shipping daemon already retains authenticated control/service sessions. Public SDK
ProjectApi currently binds concrete Server, so genuine MinIO-backed Init must be
integrated before Family2 can qualify; direct prototype genesis is not its proof.

Own prospective V4a specification/checklist/log. No performance collection or
product enablement in this checkpoint. Next: fixed-size transport instrumentation,
one labelled diagnostic, then one session lifetime treatment preserving custody.
All five owner groups and all seven families are still incomplete. No #288 update,
release claim, merge, rollback or other-owner mutation is authorized.

## 2026-10-02 — V4a transport counters prepared

Parent `46c3dc7cf0f206731ff63a23a1207f093e60db6b`. Added fixed five-action
actual request counts/times and actual connection attempt/authentication time,
shared across Remote clones and emitted cumulatively after each known Commit.
Collector retains a purpose label and the exact parsed statistics beside raw logs.
Native per-call connection algorithm, P6META3, storage/construction/oracles and
limits are unchanged. Owning host Clippy/fmt and locked Darwin/Linux ARMv8 release
builds pass. Existing engine-only tests are unaffected and reused; real full-path
diagnostic remains NOT_RUN at this source checkpoint.

Next: one fresh sealed count-driven diagnostic on V3's exact two-head workload,
then inspect actual connection and request costs before the single session change.

## 2026-10-02 — V4a cause diagnostic observed

Source `198c25296c7e447348422f98a1a11bf0d3732ccc`; one declared count-driven
real-provider diagnostic, fresh `v4a-connection-diagnostic`, child exit 0 in
7.410180 s. Bytes/mode/parent/head and cleanup PASS. No plain-arm resample.
Overwrite Commit 331.744250 ms: 37 actual metadata connections, 213.298414 ms
connect/authentication (5.764822 ms per connection), calls 29 lookup + 7 register
+ 1 publish. First cumulative snapshot includes bootstrap; its 38 connections
and 218.887334 ms are not relabeled as a pure Commit phase. All raw times remain
cache INELIGIBLE; physical/canonical and larger workload gates remain open.

Connection setup/accept polling is a demonstrated major cost. Next: one shared
authenticated session, monotonic exact reply IDs, known idle rotation before new
submission, quarantine on failure, bounded physical connection ownership. Keep
all construction/storage/validation algorithms unchanged for this treatment.
