# M2 improvements execution prompt

Ready-to-paste task for a fresh agent session. Complete the items in
[`m2-improvements.md`](./m2-improvements.md), measure every change with the
[`m2-minibench.md`](./m2-minibench.md) matrix, and refresh the M2 evidence record. Do
not skip any step; there is no "fast mode" that excuses dropping a hard part.

## Context

- Repo: `C:\Users\yifan\code\Ephemeral-AI-Lab\ephemeral-ai-fs` (Windows, PowerShell,
  Node 24.11.1, pnpm 10.32.1).
- Milestones M0-M2 are accepted. The M2 evidence chain is: candidate
  `2e06a446aa5781102d9c028c62519582ee3b1519`, evidence commit `81eb1fe`, owned-tree
  digest `59aa967a0104944ef0079d3429e27298f46c0d85c1bdec2c042221c83ebf6fbb`, recorded in
  `docs/evidence/m2/`. Branch: `agent/draft-spec`. Worktree must start clean.
- The working tree contains a Node SQLite driver with a bounded prepared-statement and
  validation-verdict cache (256-entry LRU), which is already part of the accepted M2.

## Objective

1. Build the mini-benchmark harness `tests/performance/mini-bench.mjs` exactly per
   `m2-minibench.md` (cells A1-A7, B1-B5, C1-C3; cold and warm phases; JSON artifact per
   the result schema in `release-benchmarks.md` section 16; runtime under 120 seconds).
   Run it to record the baseline; the baseline must match the anchors in
   `m2-minibench.md` within measurement noise (write ~26 MiB/s, read ~61 MiB/s,
   small-read ~8 ms/op).
2. Implement R3 (host-injected native hashing seam), then re-run the mini-bench.
3. Implement R5 (statement batching in streaming-prepare and reconciliation), re-run.
4. Apply the FastCDC `acceptChunk` copy reduction, re-run.
5. Update the anchors in `m2-minibench.md` and the baseline/outcome tables in
   `m2-improvements.md` with the measured before/after numbers.
6. Refresh the M2 evidence record (see below) so `pnpm validate:m2` passes end-to-end.

## R3 design constraints (read before implementing)

- The seam is a synchronous hashing capability provided by the host adapter through the
  operations storage port. `packages/sqlite-node` injects `node:crypto` `createHash`;
  pure-JS `cas/sha256.ts` remains the fallback used by workerd and any adapter that does
  not provide the capability.
- `packages/fs/src` algorithm paths MUST NOT import node-only modules (architecture gate
  enforces this; it cannot be bypassed).
- WebCrypto is async and may not be used inside read transactions; if a sync native
  hasher is unavailable, keep the pure-JS path.
- Hashes must be byte-identical to the pure-JS implementation. M1 golden vectors and the
  workerd parity suite (`pnpm test:workerd`) must still pass unchanged.
- Memory admission, per-unit-of-work statement/elapsed budgets, `efs_usage` exactness,
  quota ceilings, and WAL backpressure semantics are untouched.

## Execution order

1. `pnpm fixtures:check` and confirm a clean tree.
2. Build the harness; record baseline into `m2-minibench.md`.
3. R3: add the hashing capability (port + adapter injection + use in
   `streaming-prepare`, `durable-edit-prepare`, `streamed-rebuild`, and
   `content-repository` verification), run the full M2 suite, re-run the mini-bench.
4. R5: batch per-chunk `putEntry` and reconciliation edge lookups within existing
   transaction budgets; run the suite, re-run the mini-bench.
5. Copy reduction in `acceptChunk`; suite + mini-bench.
6. Update both docs with measured numbers.
7. Refresh evidence and commit.

## Evidence refresh (required, follow the exact pattern)

Changing any m2-owned file (`packages/fs/src/**`, `packages/sqlite-node/src/**`,
`tests/storage/**`, `tests/node-integration/**`, `tests/maintenance/**`) makes the
accepted digest stale and `check:evidence` fails until the record is refreshed:

1. After all gates pass, commit the work (candidate commit).
2. Compute the new digest:
   `node scripts/check-evidence.mjs --owned-tree-digest m2 <candidate>`.
3. Update `docs/evidence/m2/correctness.json` (new commit, digest, refreshed `elapsedMs`
   and metrics from the actual run) and `docs/evidence/m2/exit.md` (candidate commit,
   date, checklist status, deviations including any R3/R5 deviations) and re-run the
   suite to capture the final numbers.
4. Commit the evidence (must be directly parented by the candidate).
5. Run the complete `pnpm validate:m2` from HEAD; it must pass including
   `check:evidence`. Prettier must be applied to every file (evidence docs included)
   before the style gate.

## Acceptance criteria

- `pnpm validate:m2` passes from the final HEAD with a refreshed evidence record.
- Mini-bench: write improves to ~1.6-1.9x, sequential read to ~2.5-5x, small reads to ~1
  ms/op or better; warm re-reads measurably faster than cold; storage behavior unchanged
  (dedup, ~3.1% fresh-data overhead, exact quotas).
- All M2 tests (99), M1 tests (35), workerd parity (11), architecture, API/exports,
  docs, fixtures, and style gates pass.
- No new unbounded memory, statement, or CPU behavior; no node-only imports in core
  algorithm paths.

## Report back

- Commits created (candidate + evidence) with hashes.
- Before/after mini-bench tables (A1-A7, B1-B5, C1-C3) and the doc diffs.
- What was attempted, what was measured, and any item that had to be deferred with the
  reason.
