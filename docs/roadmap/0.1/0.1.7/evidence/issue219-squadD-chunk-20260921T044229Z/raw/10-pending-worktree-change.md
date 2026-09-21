# raw/10 — WORKING-TREE STATE AND A PENDING CHANGE THAT MATTERS

## The tree is not clean

```
$ git -C <worktree> status --porcelain
 M core/benchmark/fs-bench-pro-storage-content/src/ops/pipeline.rs
?? docs/roadmap/0.1/0.1.7/evidence/issue219-squadA-profile-20260921T044041Z/
?? docs/roadmap/0.1/0.1.7/evidence/issue219-squadB-sql-20260921T043911Z/
?? docs/roadmap/0.1.0/0.1.7/evidence/issue219-squadC-cadence-20260921T044258Z/   (path abbreviated)
?? docs/roadmap/0.1/0.1.7/evidence/issue219-squadD-chunk-20260921T044229Z/       <- this directory
```

`pipeline.rs` carries **+55 uncommitted lines that are not in HEAD
`9c46930b846600e5f3c6ca4a4c4cbcf44ecdc356`**. Squad D did not write them and did not
touch that file — this campaign is read-only outside its own evidence directory. The
modification is recorded here because it changes how §2.3 must be read.

## What the pending change does

`git diff` adds three hunks to `fn namespace_scale`:

1. `let mut accept_span_ns = 0_u64;` before `instruments::heap_begin()`, with a
   comment stating that the span runs "from the moment the operation is owned to the
   moment the seal returns".
2. `let accept_started = std::time::Instant::now();` inside the measured closure,
   and `accept_span_ns = accept_started.elapsed().as_nanos() as u64;` after
   `operation.finish(...)`.
3. A loop publishing fourteen counters — the seven buckets and their five
   sub-parts, plus the denominator:

```
pipeline.profile_resolve_ns
pipeline.profile_resolve_eligible_ns
pipeline.profile_resolve_acquire_ns
pipeline.profile_resolve_cost_ns
pipeline.profile_resolve_reuse_ns
pipeline.profile_resolve_pooled_ns
pipeline.profile_full_ns
pipeline.profile_delta_ns
pipeline.profile_group_ns
pipeline.profile_place_ns
pipeline.profile_sql_ns
pipeline.profile_commit_ns
pipeline.profile_total_ns
pipeline.accept_span_ns
```

plus `pipeline.profile_reuse_repeat`. The hunk's own comment states: *"Diagnostics
only: none of these is pinned in `tests/golden/expected.tsv`, because a wall-clock
observation cannot be a frozen constant."*

**This is exactly the publication §2.3 says a future measurement must record — every
bucket beside `accept_span_ns`.** It is the right shape. But it is uncommitted, and no
receipt carries any of it.

## Evidence that the existing receipts predate it

```
$ grep -c "profile_" \
    core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/\
ns17-final-20260921T031259Z/pipeline-namespace-10000/receipt.json
0
```

**Zero occurrences.** The receipt analysed in this report was produced by a binary
that does not publish `SaveProfile`. The `NOT_MEASURED` marking in `raw/07` and
README §2.3 is therefore correct for the evidence that exists today, and it is not a
statement that the values are unobtainable — only that no receipt contains them.

## Line-number caveat (applies to every `pipeline.rs` citation here)

Squad D read `pipeline.rs` from the **working tree**. Every citation at or before
line 767 is identical in HEAD and in the working tree. The timed-region citations
shift by +11 after the first hunk:

| Anchor | HEAD `9c46930b8` | Working tree |
|---|---|---|
| `fn namespace_scale` | 591 | 591 |
| `// Construct the content. Untimed: …` | 621 | 621 |
| `let content_objects = content.len() as u64;` | 665 | 665 |
| `let (measured, report) = super::measure("pipeline", …)` (namespace_scale) | **775** | **786** |
| `accept_span_ns = accept_started.elapsed()…` | *absent* | 845 |
| `pipeline.profile_*` publication loop | *absent* | 1020-1033 |

The "excluded work" table in `raw/09` cites `pipeline.rs:598-767`, all of which is
stable. The included-work citations (`786-847`) are working-tree numbers; their HEAD
equivalents are `775-836`. Both refer to the same code.

## What the parent should do with this

1. Do **not** cite `pipeline.profile_*` as evidence until a receipt publishes it.
   Today it is a pending change, not a measurement.
2. When that change lands and a row is run, the seven buckets become measurable —
   and the §2.3 table can be filled in without any further product work.
3. A dirty tree cannot produce a sealed, comparable arm (`AGENTS.md` §4, "Keep the
   tree clean for sealed builds"). Any v0.1.7 arm built now records
   `LAYERFS_SOURCE_DIRTY=true`, which is the same condition that already taints the
   historical 578.245 ms `namespace-10000` row noted at
   `namespace-10000-parity-spec.md:45`.
