# Phase-4 baselines

Current benchmark scoreboard:
[Phase-4 current benchmark scoreboard](current-benchmark-scoreboard.md).

Current accepted mechanism predecessor (`G3 PASS / G4 READY — v13 STATICALLY
CLOSED AND TERMINALLY SEALED`):
[G3 incremental materialization baseline v1](g3-incremental-materialization-baseline-v1.md).

Current controlling Phase-4 status is **G4 STAGE TERMINAL PASS under the
user-approved 1-ms absolute-regression materiality rule**. The sealed v12 campaign remains `TERMINAL REVISE`
under its unchanged <=5% adjacent gate at seq17 (+8.535%), seq20 (+6.800%),
and seq26 (+14.360%); that old gate is not reported as passing. Its absolute
mean deltas are only +0.226229 ms, +0.285522 ms, and +0.099604 ms, below the
approved 1.000-ms materiality floor, while source/static, semantic, work,
resource/direct <=1-MiB buffer, durability, Q, cleanup, residue, custody, and
independent-ledger gates pass. The
[G4 baseline](g4-materialization-acceptance-baseline-v1.md) is accepted with
that explicit qualification. The controlling terminal is
[G4-STAGE-TERMINAL-v1.json](../experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json),
whose stage PASS remains separate from immutable v12 REVISE.
Phase 4 remains incomplete, and this task stops
before and authorizes no G5 implementation or measurement. Concurrent
premature `research/phase-4/g5-round-0` planning is foreign/excluded rather
than evidence of an accepted G5 start.

Current accepted optimization baseline:
[SQLite writer-memory `cache_spill=2000`](sqlite-writer-memory-cache-spill-2000-baseline-v1.md).

Its manifest:
[SQLite writer-memory baseline manifest](sqlite-writer-memory-cache-spill-2000-baseline-v1-manifest.tsv).

Its execution predecessor:
[FastCDC contiguous-region kernel v2](fastcdc-contiguous-region-kernel-v2-baseline-v1.md).

Its identity/profile predecessor:
[Canonical-v2 baseline v1](canonical-v2-baseline-v1.md).

Historical control used for its adjacent A/B comparison:
[CP-0009 current-product baseline v1](current-baseline-v1.md).

The writer-memory policy is accepted only for the exact FastCDC-v2 control,
Canonical-v2 profile, source, executable, and runtime SQLite settings recorded
in its manifest. Automatic migration of a nonempty v1 store remains
unsupported.
