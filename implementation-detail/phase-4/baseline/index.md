# Phase-4 baselines

Current benchmark scoreboard:
[Phase-4 current benchmark scoreboard](current-benchmark-scoreboard.md).

Current sealed G3 mechanism baseline (`G3 PASS / G4 READY — v13 STATICALLY
CLOSED AND TERMINALLY SEALED`; G4 planning-only and unstarted):
[G3 incremental materialization baseline v1](g3-incremental-materialization-baseline-v1.md).

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
