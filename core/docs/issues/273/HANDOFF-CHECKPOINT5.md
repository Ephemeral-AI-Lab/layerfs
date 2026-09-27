# Ready-to-use task prompt: #273 checkpoint 5

Continue issue [#273](https://github.com/Ephemeral-AI-Lab/layerfs/issues/273)
on draft [PR #274](https://github.com/Ephemeral-AI-Lab/layerfs/pull/274),
branch `codex/issue273-active-head`, in the existing isolated checkout
`/Users/yifanxu/.codex/worktrees/issue273-active-head/layerfs`. Keep the PR
draft and the issue open; do not merge or close either without a new owner
instruction. Check the current HEAD, working tree, PR head and other active
processes before making changes. Checkpoint-4 product source is
`05fca30fb46a616a6988a60d3532f4255c9b3cf8`; test-only count oracle is
`6f8c5a684`. Read [the checkpoint-4 log](CHECKPOINT4-LOG.md) and its
[retained evidence](evidence/checkpoint4/) before relying on any observation.
The checkpoint-4 source proves functional correctness/space for its named
cases only: **no checkpoint-5 speed sample has been taken**.

Read repository `AGENTS.md`, `core/AGENTS.md`,
`core/benchmark/fs-bench-pro/AGENTS.md`, `benchmark/AGENTS.md`,
`docs/general/benchmark_rules.md`, `benchmark/fs-bench-pro/QUICKSTART.md`,
the #273 [v1 format and evaluation contract](ACTIVE-FORMAT-AND-EVALUATION-v1.md)
especially **Registered public evaluation**, and the applicable release and
documentation policies. Those rules override this prompt. Do not run the
retired `tools/preflight.sh`, create an aggregate gate, patch third-party
crates, change a worker/deadline/cache policy to pass, or claim CI green.

Finish checkpoint 5 using the already registered public selection set.
Freeze candidate source, workload and harness identities before sampling.
The frozen control is #271 commit
`48b51e874a41b3e1e6c6661e145316df8b408f07`, available in the separate
`/Users/yifanxu/.codex/worktrees/issue271-root-custody/layerfs` checkout;
verify its current state and do not disturb another owner's process. Build
each arm with locked Cargo **release** binaries and a target directory in
its own worktree. Use the existing public mounted route and harness helpers
in `core/benchmark/fs-bench-pro/` (`write_patterns.py`,
`separated_writes.py`, `shell_package.py`) where they fit. Do not promote
their older 100-write exploratory receipts or uncontrolled-cache status.

Register, then run in the frozen row-major order, the nine case IDs
`issue273-{append,dispersed,repeated}-{100,512,4097}-10m-v1`, with the
control arm before candidate for each. The old head is one closed,
independently verified 10 MiB `data.bin` filled with `A`; each arm gets its
own independent writable `shutil.copyfile` Store/history clone. One SDK
Mount, one generic Exec, one explicit Commit, the C writer's ordinary
mounted byte writes, a full independent old/new-head byte oracle and clean
close are mandatory. Append uses `O_APPEND`; dispersed offset is
`(104729 + i*2654435761) % 10485760`; repeated offset is `5242880`;
byte `i` is `B + i % 24`. Do not change this schedule or omit a registered
case. Add separate `issue273-clean-commit-v1` and
`issue273-one-edit-commit-v1` cases after a retained 4,097-record old
generation, plus the original #248 8,194-byte `gate` with 4,097 separated
writes and unchanged 25 s limit. The additional external correctness/space
registry names `issue273-many-file-128-v1`, `issue273-multi-exec-v1`,
`issue273-g1-g2-v1`, `issue273-retained-32-v1`, and
`issue273-mutations-v1`; checkpoint 4 has focused proofs for some components
but not receipts under all these registered IDs.

Take **one performance sample per case per source arm** in fresh append-only
output paths. Do not repeat an unchanged arm to characterize spread or
replace a number. Complete-command limits are 15 s, except the three 4,097
matrix rows and #248 at 25 s. The independent verifier has a 9 s limit;
verification wall is separate from speed. Enforce the same declared cold
state in both arms: use the Darwin mmap/mincore invalidation and whole-input
residency check on each Store/history clone before the timed arm, record the
launch gap and source identities, and ensure any private backing pages read
during a measured Commit do not get free credit from their own recent writes.
If any cache state cannot be established equally, keep the functional
receipt but mark both numeric speed rows `INELIGIBLE`; no 2× claim follows.
Never pool warm and cold rows. A started timeout is `FAIL`; a missing
prerequisite is `NOT_RUN` with its measured wall and exact reason. Preserve
all such receipts. Use count-driven labelled diagnostics rather than a
second sample to investigate anomalies.

Record actual FUSE callbacks, Service calls, Exec/Commit/freezing/cleanup
walls, pack/index reads and writes, `Q_fetch`, live/dead/pinned pages,
physical `st_blocks`, Host quota charge, memory/cgroup domains, runtime
image, product/compilation/dependency/harness/workload hashes, clone method,
cache contract and any competing work. Keep the one construction worker
default except namespace Init. Report any unmeasured compaction slot/ref
counts explicitly; checkpoint 4's focused backing oracle counted 68/68 and
4,096 refunded pack bytes, not every future matrix row. A general 2× claim
requires a complete cache-qualified matched set, correct independent byte
oracles and all resource/correctness gates; do not infer it from the
checkpoint-4 command walls.

The full Core test command at checkpoint 4 still failed in two unchanged
`layerfs-content/tests/filesystem_ordering.rs` tests (19 objects versus
limit 18); targeted Workspace, release Clippy/examples/fmt, product boundary
and Linux ext4 backing/public selections passed. Report exact final checks
and remaining failures, and never relabel the historical #248 `FAIL` or #271
cache-`INELIGIBLE` receipts. Update the active ledger, issue #273 and draft
PR #274 with every PASS/FAIL/INCOMPLETE/INELIGIBLE/NOT_RUN cell. For each
commit, calculate exact first-parent versus final staged production LOC with
`tools/production_loc.py`, state reference/Core/combined totals and signed
delta in the commit message and handoff. Push only this branch. Stop for a
genuinely required external ruling or an irreversible action outside this
authorization; otherwise complete the registered checkpoint autonomously.
