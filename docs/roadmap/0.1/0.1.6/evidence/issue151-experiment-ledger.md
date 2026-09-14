# #151 experiment ledger: sandbox-local snapshots from v0.1.6

Append-only evidence ledger for the combined #149 + #150 experiment executed
under the [#151 pipeline](../experimental-implementation-pipeline.md).
Every entry records stage, source identity, command, result, metrics versus
limits, raw receipt paths and next action. Failed and invalid attempts are
retained. A missing required metric is INCOMPLETE; a valid numerical miss is
FAIL.

- Profile/revision: `v016-local-snapshot-experiment-v1`
- Implementation branch/worktree: `codex/v016-sandbox-local-experiment` at
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016`
- Source base: peeled `v0.1.5^{commit}` = `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13` (verified)
- Control: sealed v0.1.5 product built from the same commit in this worktree
- One performance sample per case per arm; separate correctness verification.

## Frozen gates (from #149, recorded before any measurement)

For each control time T0: candidate <= T0 + max(0.15 * T0, 3 ms) — applied
independently to exec, complete Commit, End/required cleanup, complete
public-call workflow, and each B3 C1/C2/C3 cycle. For control total workflow
CPU C0: candidate <= C0 + max(0.15 * C0, 1 ms).

Resource gates: 64 MiB aggregate accounted algorithm allocations (capacity
charged); 8 MiB combined transfer/staging buffers within that total (preserve
the existing 8 MiB final-delta allowance); process-memory sum <= control +
max(15%, 8 MiB); B1/B2 temporary physical backing <= control + max(15%, 1 MiB);
B3 temporary physical backing <= 32 MiB peak. B2's legitimate 500 MiB new
payload is exempt from the 32 MiB backing ceiling only. Sandbox envelope:
2 CPU / 2 GiB / no swap / 256 PIDs. 300 s product / 310 s outer timeouts.

Cases, in order, one sample per arm each: `tiny-create-500-mixed-v4` (B1) →
`tiny-bulk-create-500-mixed-v3` (B2) → `local-snapshot-create-25000-onebyte-v1`
(B3, three distinct Commits C1/C2/C3 in one lifecycle sample).

---

## Ledger

### L0 — 2026-09-15: handoff read, environment verified, worktree created (I0 start)

- Read the ordered handoff documents (pipeline, spec, connection architecture,
  review record, research review, v0.1.5 release contract, benchmark AGENTS.md
  exception, benchmark_rules.md exception, QUICKSTART, tiny_file_churn family).
- Verified `git rev-parse 'v0.1.5^{commit}'` = `6ee1ec94cfdcb7bc8c55830e8348d553c20e2f13`.
- Main checkout at `7b73c4b33` preserved untouched (uncommitted scoped-exception
  docs remain there).
- Created worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-v016` on new branch
  `codex/v016-sandbox-local-experiment` at the exact v0.1.5 commit.
- Environment: Docker 29.5.2 server running; rustc 1.96.0 (MSRV 1.85); Python
  3.14.3; 332 GiB free disk.
- Next: freeze docs in an attributable commit; read v0.1.5 source areas before
  editing.

### L1 — 2026-09-15: planning documents frozen (I0)

- Copied the five 0.1.6 experimental planning documents and the research review
  from the main checkout into the worktree; applied the scoped benchmark
  hosting/sampling exception patch to `benchmark/AGENTS.md` and
  `docs/general/benchmark_rules.md` (verified it applies cleanly on v0.1.5).
- Added this worktree's 0.1.6 README and the evidence ledger.
- Commit: (recorded after commit) — docs only; no product/benchmark code changes;
  dependency tree untouched.
- Next: source reading phase (v0.1.5 live owner, workspace core, workspace
  lifecycle, store), then I1 implementation.
