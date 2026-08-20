# Phase 4 Rollback Handoff Prompt — Execute WP0 Through WP3

```text
You are the next LayerFS Phase 4 rollback agent.

Your only objective is to execute WP0 through WP3 from:

/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/rollback/implementation-plan.md

Stop after the post-deletion correctness baseline. Do not begin WP4, design the
durable Phase 3 mapping, optimize SQLite, add a MemoryEngine, or build another
storage backend in this handoff.

Repository and Git authority
============================

Work only in:

/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty

Use the existing branch:

codex/empty-worktree

The intended starting checkpoint has commit subject:

phase4-backpaddling-rollback-fix-checkpoint-0

Do not modify the older parent repository:

/Users/yifanxu/Ephemeral-AI-Lab/layerfs

Do not commit unless the user explicitly asks. Do not reset, checkout, clean,
or rewrite history. Preserve unrelated worktree changes. Use apply_patch for
manual edits and file deletion. Use one Cargo writer/process at a time.

Meaning of this rollback
========================

This is a source/architecture rollback of two rejected experiments. It is not
a user-visible rollback feature, database rollback API, checkpoint system,
migration system, or request to change SQLite journal behavior.

The approved direction is:

1. delete the Phase 4B append-only/carrier implementation;
2. delete the Phase 2 PackedInMemoryCas implementation;
3. preserve the evidence explaining why both were rejected;
4. leave SQLite as the authoritative durable engine;
5. leave InMemoryCas as the ordinary core reference CAS; and
6. establish a clean, tested Memory/SQLite baseline.

Do not preserve either rejected implementation behind a feature, deprecated
wrapper, commented block, compatibility shim, or dormant module. There is no
supported migration requirement for experimental store.log files.

Read before editing
===================

Read these files completely, in this order:

1. AGENTS.md in layerfs-empty, if it exists;
2. spec.md;
3. implementation-plan.md;
4. ../storage/sqlite/spec.md;
5. ../storage/sqlite/implementation-plan.md;
6. ../storage/append-only/decision.md;
7. ../storage/append-only/spec.md;
8. ../storage/append-only/acceptance-ledger.md;
9. PHASE_4B_BENCHMARK_REPORT.jsonl;
10. ../storage/append-only/first-implementation-findings.md;
11. ../../phase-2/opt-2-packed-cas.md and ../../phase-2/handoff.md;
12. crates/layerfs-engine/Cargo.toml;
13. crates/layerfs-engine/src/lib.rs;
14. crates/layerfs-engine/src/append_only.rs;
15. crates/layerfs-engine/src/bin/phase4b_benchmark.rs;
16. crates/layerfs-engine/src/bin/phase4_fair_benchmark.rs;
17. crates/layerfs-core/src/cas/mod.rs;
18. crates/layerfs-core/src/content/mod.rs; and
19. every caller found by the required repository-wide searches below.

Inspect the checkpoint before editing:

git status --short
git branch --show-current
git rev-parse HEAD
git show -s --format='%H %s' HEAD
git diff --check

The starting worktree should be clean. If it is not, classify every dirty path
before editing and preserve anything outside WP0-WP3.

Historical evidence that must survive
=====================================

Do not delete or rewrite the Phase 2/4B specifications, ledgers, JSONL reports,
or finding document. Preserve at least these facts:

- Phase 2 PackedInMemoryCas did not clear its promotion threshold: corrected
  100 MiB rows were approximately 0.09% to 0.94% faster in selected pre-sized
  comparisons, while a non-pre-sized row was approximately 4.77% slower.
- The first Phase 4B diagnostic used one locator per collision page, produced
  about 55,240 index page reads for about 5,363 lookups, and reported about
  4.02x reopen read amplification.
- The original Phase 4B empty-root benchmark was not a fair full-logical-
  workload comparison.
- The later same-source proxy campaign ran five measured rows per lane.
- Append-only median: 3,164,894,458 ns, 31.596630 MiB/s.
- Conservative SQLite-control median: 2,833,641,750 ns, 35.290276 MiB/s.
- Append-only was 331,252,708 ns or 11.69% slower in wall time.
- Both proxy lanes explicitly had full_logical_workload=false,
  phase3_semantic_persistence=false, promotion_authorized=false, and
  target_attainment_authorized=false.
- The proxy source was exactly 104,857,600 bytes.
- Raw BLAKE3:
  0855eedd9498bf31a1eafb5a2f00bf84f646db5153cc86632fcb0cc0e180fb36
- Logical-v1 BLAKE3:
  52ce153eab81e33a0243a25a47a8805a86ba9bec125a27bee3c50de647cdafbc
- Historical expected SHA-256:
  27f82e57f589b7ed79f28a8cef02acd2db82682fbccb35cdd6b48a136d98a7d6
- Proxy workload: 4,801 chunk occurrences, 263 unique chunks, 4,803 object
  submissions, 265 creations, and 4,538 reuses.

These are rejection/diagnostic facts, not a valid 200 or 300 MiB/s promotion
row. Do not upgrade their claim.

WP0 — authority and evidence snapshot
=====================================

1. Confirm HEAD is the intended checkpoint and the worktree is clean or fully
   classified.
2. Record exact SHA-256 fingerprints for the files that will be deleted:
   append_only.rs, phase4b_benchmark.rs, phase4_fair_benchmark.rs, and the
   packed-only source before editing.
3. Inspect the committed proxy benchmark and append-only diff so no unique
   result or correctness finding disappears with deletion.
4. Reconcile any missing final proxy facts into
   [the append-only findings](../storage/append-only/first-implementation-findings.md).
   Label them exploratory,
   non-promotion evidence.
5. Add a short status notice near the top of ../storage/append-only/spec.md and
   ../storage/append-only/acceptance-ledger.md stating that the candidate is rejected and
   superseded for active implementation by the
   [rollback specification](spec.md). Preserve all original
   requirements and evidence below the notice.
6. Create the [rollback deletion record](deletion-record.md). Record starting
   HEAD, deleted
   paths, removed dependencies, retained historical documents, exact commands,
   nonzero test outcomes, final source fingerprint, and any unavailable check.
   Do not paste deleted source into the record.

WP0 exit gate:

- the rejection reason and final proxy evidence survive without depending on
  code that is about to be removed;
- historical numbers are unchanged and honestly labeled; and
- the deletion record identifies the exact intended surface.

WP1 — delete the Phase 4B append-only carrier
================================================

Start with these searches and follow every caller before editing:

rg -n "append_only|AppendOnly|Phase4B|phase4b|carrier marker|store\.log" \
  crates Cargo.toml
rg -n "\bfs2\b|FileExt" crates Cargo.toml \
  --glob '*.rs' --glob 'Cargo.toml'

Delete:

- crates/layerfs-engine/src/append_only.rs;
- crates/layerfs-engine/src/bin/phase4b_benchmark.rs;
- crates/layerfs-engine/src/bin/phase4_fair_benchmark.rs;
- the append_only module declaration and AppendOnly public exports from
  crates/layerfs-engine/src/lib.rs;
- carrier-only error variants, counters, observations, helpers, and tests only
  after proving they have no SQLite caller;
- crates/layerfs-engine's fs2 dependency if append-only is its only caller; and
- the workspace fs2 dependency if repository-wide search proves no remaining
  caller exists.

Keep:

- the complete SQLite Engine implementation and schema;
- SQLite journal, synchronous, temp-store, and mmap settings;
- shared EngineError variants that SQLite or another surviving path uses;
- all Phase 4B historical Markdown/JSONL evidence; and
- Phase 1/2/3 canonical, CDC, COW, root, delta, range, and typed-error behavior.

Do not port append-only cache, marker, receipt, locking, recovery, or index code
into SQLite. This work package is deletion, not redesign.

After editing, run:

rg -n "append_only|AppendOnly|Phase4B|phase4b|carrier marker|store\.log" \
  crates Cargo.toml
rg -n "\bfs2\b|FileExt" crates Cargo.toml \
  --glob '*.rs' --glob 'Cargo.toml'

Any remaining active-code hit must have a documented non-carrier owner.
Historical documentation hits are expected.

Run the smallest engine checks:

cargo test -p layerfs-engine --offline --lib
cargo check -p layerfs-engine --offline --all-targets
git diff --check

Require a nonzero SQLite test count. Do not accept compilation alone.

WP1 exit gate:

- no append-only Rust module, target, export, feature, or dependency remains;
- no dormant carrier path survives;
- SQLite behavior and format are unchanged; and
- the engine owner tests pass.

WP2 — delete PackedInMemoryCas
================================

Start with:

rg -n "PackedInMemoryCas|ChunkLocation|full_replace_packed|packed_cas" \
  crates --glob '*.rs'

Trace every caller, then remove:

- PackedInMemoryCas;
- its private ChunkLocation/index/carrier state;
- packed-only constructors, methods, counters, helpers, and tests;
- LogicalFile::full_replace_packed_cas and packed-only internal helpers; and
- packed-only benchmark modes or imports.

Keep:

- InMemoryCas;
- PutOutcome and all ordinary CAS semantics;
- LogicalFile's ordinary streaming paths;
- the frozen CDC profile and chunk sequence;
- canonical identities and Phase 1 golden bytes;
- standard COW behavior and existing non-packed tests; and
- historical Phase 2 packed reports.

Do not replace PackedInMemoryCas with another packed buffer, arena, locator map,
feature flag, generic CAS trait, or compatibility wrapper.

After editing, the active-code search must return no hit:

rg -n "PackedInMemoryCas|ChunkLocation|full_replace_packed|packed_cas" \
  crates --glob '*.rs'

Run:

cargo test -p layerfs-core --offline
cargo check -p layerfs-core --offline --all-targets
git diff --check

Require nonzero core test counts and unchanged canonical/CDC/COW assertions.

WP2 exit gate:

- only the ordinary in-memory CAS remains;
- no packed implementation or dormant hook remains; and
- core tests pass with frozen outputs unchanged.

WP3 — post-deletion correctness baseline
=========================================

On one unchanged source fingerprint, run exactly one stable closure pass:

cargo metadata --offline --no-deps
cargo test -p layerfs-core --offline
cargo test -p layerfs-engine --offline
cargo check -p layerfs-core --offline --all-targets --all-features
cargo check -p layerfs-engine --offline --all-targets --all-features
cargo fmt --all -- --check
git diff --check

Run affected repository architecture/custody checks once if the controlling
documents name them. Do not run unrelated long E2E suites.

Then repeat the absence searches for both rejected implementations and inspect:

git status --short
git diff --stat
git diff -- crates/layerfs-engine/src/lib.rs
git diff -- crates/layerfs-core/src/cas/mod.rs \
  crates/layerfs-core/src/content/mod.rs

Confirm that surviving diffs are deletion plumbing, status/evidence updates,
and test cleanup—not SQLite redesign, canonical-format change, CDC change, or a
new abstraction.

Complete deletion-record.md with:

- starting and final source fingerprints;
- exact deleted files and removed Cargo dependencies;
- retained evidence files;
- exact commands and nonzero outcomes;
- explicit confirmation that SQLite schema/profile were unchanged;
- explicit confirmation that canonical bytes, object IDs, and CDC outputs were
  unchanged; and
- any test not run and why.

WP3 exit gate:

- append-only/carrier production code is absent;
- PackedInMemoryCas production code is absent;
- SQLite and InMemoryCas are the only surviving active storage paths;
- core and engine tests/checks/format/diff gates pass;
- no unsupported migration or compatibility layer was added;
- historical rejection evidence remains available; and
- the implementation ledger marks WP0-WP3 complete and WP4 pending.

Required final response
=======================

Report:

- starting and final Git/source fingerprints;
- every file deleted and materially edited;
- every dependency removed or deliberately retained;
- evidence/status documents updated;
- exact search commands and remaining-hit classification;
- exact test/check/format commands with nonzero outcomes;
- any test not run and why;
- confirmation that SQLite schema/profile and Phase 1/2/3 frozen outputs did
  not change;
- remaining work beginning at WP4; and
- whether the worktree is ready for review.

Stop after WP3. Do not claim 200 or 300 MiB/s, do not implement the durable
mapping, and do not begin performance optimization in this handoff.
```
