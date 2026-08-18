/goal Implement/evaluate LayerFS WP4-M only in /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty on codex/empty-worktree; never edit /Users/yifanxu/Ephemeral-AI-Lab/layerfs. Preserve dirty work; no commit.

Read: AGENTS.md; PHASE_4_ROLLING_BACK_TO_PREVIOUS_OPTIMIZATION_{SPEC,IMPLEMENTATION_PLAN}.md (WP4-M/P,WP8/9); PHASE_4_LOGICAL_PERSISTENCE_MAPPING.md; PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md; PHASE_4_ALGORITHM_COMPLEXITY_ANALYSIS.md; phase_4_algorithm_spec.md; phase4-algorithm-test.md; PHASE_4_ROLLBACK_DELETION_RECORD.md; referenced Phase-1/2/3 doc; crates/layerfs-{core,engine} production/tests. Confirm branch/status and no Cargo writer.

First launch 3 parallel read-only subagents: codec/identity; Big-O/resources; benchmark/SQLite fairness. No edits/Cargo. Reconcile before edits.

Implement the minimum codec+SQLite lane comparing file K64/F64,K59/F101,K256/F256 and directory 64-KiB,256-KiB,1-MiB ceilings. Each gets a profile ID and isolated DB. Selector stays test/benchmark-private. No public flag/abstraction/multiprofile API, append/pack, WAL, async/workers/pool, unbounded map/cache or source-sized buffer.

Target files, only if needed:
crates/layerfs-core/src/content/persistence.rs
crates/layerfs-core/src/cow/persistence.rs
crates/layerfs-core/src/delta/codec.rs
crates/layerfs-core/src/{cas/mod.rs,limits.rs,error.rs}
crates/layerfs-engine/src/lib.rs
crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs
crates/layerfs-engine/tests/phase4_engine_parity.rs
No memory.rs (WP6), SQLite move or one-impl interface.

Preserve Phase-1 canonical Bytes/Directory ID, Phase-2 CDC 8/16/32-KiB and raw ChunkIds, Phase-3 COW/root/delta semantics. Authenticate full bytes before trust. Require immutable no-overwrite reuse; one SQLite transaction/commit/head publication; lost-ack reconciliation; checked u64; exact first/cleanup/reconciliation/dominant typed errors; bounded active ancestry; spool beyond resident limits. Q bounds live allocation; W/D are cumulative with checked u64 and no total cap. Support 100 GiB.

Big-O: capture Theta(source bytes)+O(objects); scrub/reconstruct Theta(reachable bytes); range O(Vb*Bv+Vl*Lv+Cv+returned), one-leaf reduction only when true; memory bounded by path/chunk/page/output; same-count edit rewrites changed chunks+leaf+spine; fixed-ordinal +1 suffix work is quantified; directory replace rewrites page/index/wrapper per ancestor, leading insert may be O(E).

Before timing run test-doc goldens/round trips, malformed inputs, identity, zero/cross-leaf ranges, same/+1 COW, wide-directory, root/delta, cycle/depth, Q/W/D, publication/receipt/lost-ack, migration and fresh reopen. Then focused, owner/package/all-target checks, fmt and diff check.

Build release once. Run phase4-algorithm-test.md WP4-M exactly: retained 100/512-MiB sources outside timers; file full-cycle/same/+1 early/middle; 100,000-entry directory create/lookup/replace/leading insert; warmup then 5 alternating isolated processes/row. Emit qualification=false,purpose=profile_selection. Record wall/CPU/RSS,Q/W/D,DB+sidecar bytes, mapping/auth/hash/rewrite and SQL/BLOB work, phases, IDs/closure, reuse/cache. Keep JSONL, commands, fingerprints, medians/spreads.

Validate equations; project 100-GiB height, metadata, suffix refs/objects/bytes and rewrite amplification; no invented timing. Apply exact 5%,4-of-5,protected-metric,500-ms,SQL-sensitivity gates. Do not run WP4-P, promote or delete candidates. Rank results; defer unresolved 100-GiB insert budget to WP4-P.

Do not stop at scaffolding, compile, partial tests or one run. Continue until implementation, correctness evaluation and WP4-M campaign finish. Stop only for irreconcilable authority/correctness or exhausted external blocker; report evidence, attempts and smallest decision. Otherwise report files, commands/results, fingerprints, resource/physical bytes, candidate table and PASS/FAIL.
