## Scenario-version-2 baseline: 56/56 attempted, 45 complete, 11 product-side FAIL, 0 eligible

All 56 registered Workspace Exec/FUSE cases now have an append-only receipt
under one frozen release identity. The twenty structural cases are no longer
`NOT_RUN`: the frozen bounded-memory in-place shift is implemented in the
release workload tool (grow extends and moves the suffix backward, shrink moves
it forward and truncates, either direction writes the replacement over the
stale window, 128 KiB blocks), registered in
`core/benchmark/fs-bench-pro/registry/workspace-exec-edit-v2.json`
(`05a133b543db33729d60cc321cc88682046c16bfb1761daec6a57c5df3b11823`), with the
families renamed to the v0.1.6 style. The version-1 registry, its Phase 4
sample and the v1 receipts were not rewritten; the route name
`sdk-exec-fuse-edit-commit-v1` is unchanged and the case identity carries
`scenario_version=2` / `-exec-v2`.

Identity (one for the whole campaign):

* source `1b4dfdf7efc702e67fe335ea3cad6a0d50fd550d`, tree
  `7bfde2ee9f41ce84d94ab188b1c4e8b45c6af3dd`, dirty `false`
* product seal `9a8d6644fdf2ced0fb5219fa219d32a15d86f4f2e99b10b2eb51bc8e24356f9c`,
  harness seal `7d037a57af936b313cf0f9f4e1b4a8abd0733883d55636d2c0ff20bcaeb236a9`
* release image `sha256:07898cbabbeab35eb57d8fdcd43125b17386fae91968d9d3f3863c75d62e7815`,
  edit tool `70d41e2868196bad…`, daemon `e10b3ab6303b6949…`
* one sample per case, no retry, no warm-up, no raised timeout, no dropped case

### Result

| | count |
|---|---:|
| registered / attempted | 56 / 56 |
| completed Edit+Commit | 45 |
| driver FAIL | 11 |
| driver timeout | 0 |
| independent verifier PASS / FAIL / TIMEOUT / not run | 45 / 0 / 0 / 11 |
| `INELIGIBLE` (cache contract) | 45 |
| `GOAL_MET` | **0** |
| raw `edit_commit_ns` at or below its historical G2 target | **0** of 56 |
| complete command inside the 15 s budget | 56 (max 10.90 s) |
| `SandboxApi::delete` confirmed | 56 (no retained container or volume) |

Completed rows: 30.63–2959.96 ms `edit_commit_ns`; 34 of them verified with a
full-file digest, 11 with one or two declared bounded windows (196,608 bytes
total, 500 MiB rows). Every completed row passed size, content digest,
canonical-root change, extent count, published Branch head and the unchanged
pristine genesis root/extent count. Verifier wall was 38–692 ms, far inside its
15 s cap and under the 10 s design goal.

### Two product-side performance findings (not fixed here)

1. **An Exec that stays silent for more than 5 s fails.** All 11 failures are
   the structural shifts whose FUSE work exceeds five seconds: the 10 MiB
   prepend, the five 100 MiB shifts and the five 500 MiB shifts. Each failed at
   5.002–5.005 s with `Failure { code: Unknown, unknown: true }`, while the
   daemon was still working; these are not benchmark timeouts (`driver_timeout`
   is false for all eleven, and the complete command finished in 10.85–10.90 s
   including confirmed teardown). The matching source reading is the native
   bridge's silent-wait cap `IO_PROGRESS_MS = 5000`
   (`core/crates/layerfs-bridge/src/contract/request.rs`), re-armed from the
   last observed activity in `adapters/native/connection.rs` (`Socket::read`)
   and `adapters/native/pipe.rs` (`Pipe::ready`): a command that produces no
   output until it exits never refreshes that window. Every 500 MiB structural
   case therefore stays registered with this exact outcome — no case was shrunk
   to fit, and none was relabelled `NOT_RUN` or PASS.
2. **The in-place shift cost is superlinear in the number of 128 KiB blocks.**
   Where the shift completed, per-block cost rises from ~19–26 ms at 4 blocks
   (1 MiB rows) to ~68 ms at 40 blocks (10 MiB rows). A descriptive two-term
   fit over the nine completed shift rows (4–40 blocks) is
   `edit_ns ≈ 15.5 ms × blocks + 1.33 ms × blocks²`. That fit describes the
   measured domain only; the 250–500 MiB shifts never completed, so their cost
   is unmeasured beyond the 5 s cliff. The scaling alone puts a 2,000-block
   (250 MiB) shift far outside the 15 s complete-command budget even without
   the quadratic term.
3. The 45 completed rows are also all above their historical targets (closest
   group: 30.63–45.80 ms against 4.72–13.54 ms), and every row remains
   `INELIGIBLE`: the macOS Store domain is invalidated and checked (0 resident
   pages), while the Linux FUSE/backing domain cannot be invalidated without a
   forbidden benchmark-only eviction between Edit and Commit. The raw numbers
   are retained as declared-warm diagnostics; the check was not changed to
   PASS and no diagnostic is called eligible.

Per owner direction this session, no optimization was attempted: the measured
causes above are recorded for a separately approved product change, and any fix
needs its own reviewed design (and, for the cache domain, its own contract
revision) before a new campaign at a new identity.

### Evidence

Commits on the local branch `codex/issue236-agent-sdk` (worktree
`/Users/yifanxu/.codex/worktrees/bb50/layerfs`, not pushed by this session):
`1be1c59e9` (structural shift tool, contract v2, registry), `1b4dfdf7e`
(registry derived once per runner process) and the baseline-report commit at the
branch tip.

* `core/docs/issues/232/exec-fuse-edit-v2-baseline.md` — full report, findings,
  56-row table, custody and retained attempts
* `core/docs/issues/232/evidence/exec-fuse-edit-v2-baseline-20260924-01/` —
  `report-edit.tsv`, `report-edit.json`, `receipts.jsonl` (56 rows),
  `shift-scaling.tsv`, `summary.json`, this comment
* retained on disk: `benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign/`
  (56 case folders), case evidence under
  `benchmark-results/fs-bench-pro/sdk-exec-fuse/<family>/<scenario-id>/`, and
  the superseded pre-memoization batch `exec-fuse-edit-v2-phaseB/` (8 rows,
  different harness seal, not pooled)

Reproduce:

```text
python3 core/benchmark/fs-bench-pro/runner.py run --case <scenario-id> \
  --out benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign/<scenario-id> \
  --verification skipped
python3 core/benchmark/fs-bench-pro/runner.py verify-edit \
  --run benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign/<scenario-id>
python3 core/benchmark/fs-bench-pro/runner.py report-edit \
  --runs benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign \
  --out benchmark-results/fs-bench-pro/exec-fuse-edit-v2-campaign
```

### Status

Coverage is complete for all 56 registered cases, but #232 is **not** complete
and must not be closed: no row has an eligible cold Edit+Commit sample, no row
is `GOAL_MET`, and 11 rows have no completed Commit at all. No 56-PASS claim is
made.
