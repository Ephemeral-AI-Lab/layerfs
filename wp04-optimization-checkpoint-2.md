# WP04 optimization checkpoint 2

This checkpoint records the first measured K64/F64 optimization result after
`wp04-baseline-checkpoint-1`. It is a release-build optimization checkpoint,
not WP4-M qualification or profile-promotion evidence.

## Repository scope

- repository: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`
- branch: `codex/empty-worktree`
- parent checkpoint: `63f2c38144ad0e7b34c20a8be89d8f67236c89df`
- SQLite remains the authoritative durable engine.
- The discarded append-only/pack carrier was not restored.
- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` was not modified.
- No campaign was rerun while creating this checkpoint record.

The implementation changes included in this checkpoint are:

- `crates/layerfs-core/src/content/persistence.rs`
- `crates/layerfs-core/src/lib.rs`
- `crates/layerfs-core/src/object/codec.rs`
- `crates/layerfs-core/src/object/mod.rs`
- `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`

The optimization is intentionally small:

- fresh object writes use one `INSERT ... ON CONFLICT DO NOTHING` and only
  read the incumbent on a conflict;
- repeated SQLite statements use the existing bounded `prepare_cached` path;
- authenticated byte objects use a validated byte-only decoder when a full
  logical object decode is unnecessary.

## Measured result

The row was K64/F64, full create, retained S1-100, 104,857,600 source bytes,
release build, five isolated measured processes after one warmup. The retained
fixture produced 5,284 CDC references and preserved the recorded source,
sequence, root, transition, and ordered-closure identities.

| Phase | Median time |
|---|---:|
| Canonical CAS mapping and object persistence | 498 ms |
| Pre-commit closure validation | 394 ms |
| SQLite commit durability | 102 ms |
| Fresh reopen | 1 ms |
| Full closure scrub | 274 ms |
| File reconstruction | 453 ms |
| Range verification | 1 ms |

The complete-lifecycle median was **1.732867666 s**, equivalent to
**57.707811 MiB/s**, versus the frozen baseline of **1.847956667 s** or
**54.113823 MiB/s**. This is a **6.228%** median improvement, with **5/5**
matched candidate rows winning.

The durable-capture median was **0.990837375 s**, equivalent to
**100.924736 MiB/s**, versus the baseline **1.079909292 s** or
**92.600370 MiB/s**. This is an **8.248%** improvement.

The same-middle locality check passed with 7 objects created, 7,382 mapping
bytes rewritten, zero suffix references/bytes/objects, and one transaction and
commit.

## Interpretation

The full-file builder remains streaming and bounded in its active mapping
state. The pre-commit closure pass remains a deliberate full authenticated
traversal required by the trust boundary; its time is not equivalent to the
SQLite commit time alone. The phase table is an observed decomposition, not a
claim that these phases are fully optimized or that their medians establish a
lower bound.

This checkpoint does not establish:

- that K64/F64 is the globally best file geometry;
- that K59/F101 or K256/F256 should be rejected;
- directory or 512-MiB performance;
- complete protected Q/allocated-delta/sync/cache qualification;
- the 200 MiB/s minimum or 300 MiB/s stretch durable target;
- WP4-M promotion.

The retained optimization artifact therefore remains explicitly:

```text
qualification=false
throughput_measurement_admissible=false
candidate_promotion=false
candidate_rejection=false
wp4m_status=unqualified
```

Evidence is retained in:

- `target/wp4m-opt2-k64-20260818/wp4m-k64-optimization-summary.json`
- `target/wp4m-opt2-k64-20260818/wp4m-k64-optimization-environment.json`
- `target/wp4m-opt2-k64-20260818/wp4m-k64-optimization.external-resources.jsonl`
- `target/wp4m-opt2-k64-20260818/wp4m-k64-optimization-commands.txt`

The next step remains the controlled mini-campaign across the required file
and directory candidates. No candidate is promoted or deleted by this
checkpoint.
