# Handoff prompt: #236 SDK route speed alignment

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Work in the existing worktree
`/Users/yifanxu/.codex/worktrees/bb50/layerfs` on branch
`codex/issue236-agent-sdk`. Do not switch to the main checkout or treat the
root `crates/` tree as v0.1.7 product source. The current product source
checkpoint is `bad49805cb4cacdd29ed15ed5f44e20274e63a3a`; this handoff
document may be committed after it. Inspect `git status` and preserve all
existing work. Read the root `AGENTS.md`, `core/AGENTS.md`, and
`docs/general/benchmark_rules.md` before changing code or taking measurements.
Before benchmark work, also read `benchmark/AGENTS.md`,
`benchmark/fs-bench-pro/QUICKSTART.md`, and the Core benchmark instructions
for any Core family you touch.

Objective: make the public Core SDK route `sandbox.create` →
`workspace.mount` → `workspace.exec` → `workspace.commit` →
`workspace.unmount` faster while preserving its stronger identity, history,
failure and ownership contracts. The target is the *real live Docker route*,
not a synthetic replacement. Use the implemented `layerfs-telemetry` crate
for time, CPU and RSS; do not build a separate benchmark mechanism.

Current evidence:

- The corrected live-test listener source `436a93ae6` measured a 404.007 ms
  route and 110.843 ms across the four post-Create calls: Mount 26.212, Exec
  28.506, Commit 49.133, Unmount 6.992 ms. The four owner Hello windows
  totalled 20.305 ms. Host Service operations within the Mount, Exec and
  Commit SDK windows totalled 3.287, 2.446 and 14.855 ms respectively.
  [Receipt](evidence/sdk-route-host-poll-20260924-01/report.json).
- The later Exec-poll source `ee876081e` passed functionally but measured
  454.830 ms overall and Exec 31.994 ms. It did not demonstrate a latency
  gain. [Receipt](evidence/sdk-route-exec-poll-20260924-01/report.json).
- Both rows are one sample each with uncontrolled cache state: functional
  PASS, performance INELIGIBLE. Never subtract them as a causal speedup.
  The exploratory v0.1.6 four-call result is 30.271 ms but has different
  APIs/topology and an external probe lockfile drift; it is not a qualified
  comparison arm. See the
  [v0.1.6 comparison](sdk-route-v016-comparison-20260924.md).
- v0.1.6 resolves endpoint/capability during Connect and its mounted FUSE
  owner keeps backing/control/snapshot lanes. Both versions write FUSE edits
  locally. Core currently creates a fresh authenticated host Service client
  per upstream request in `core/crates/layerfs-daemon/src/run.rs:101-117`.
  Core owner creates a new checked Hello session per Workspace API call in
  `core/crates/layerfs-sandbox/src/owner.rs:256-277` and reuses that one
  connection for the operation. Production Service already polls listener
  readiness; the live test was corrected in `sdk/tests/agent_route.rs`.

Four workstreams to complete, with instrumentation before any larger rewrite:

1. **Reuse healthy daemon-to-host Service transport within Mount and Commit.**
   Trace the sequential upstream calls from
   `core/crates/layerfs-workspace/src/runtime/host.rs` and
   `core/crates/layerfs-workspace/src/commit/operation.rs` through
   `core/crates/layerfs-daemon/src/run.rs`. The native Bridge Client/Server
   support multiple successful requests per connection. Preserve monotone
   request IDs, authorization, deadlines, response bounds, existing operation
   counts, concurrent FUSE behavior, and the rule that any failure closes the
   session. Never retry or replay a mutation with uncertain outcome. Prove
   connection/handshake counts before and after; avoid a global lock that
   serializes unrelated FUSE requests unless that cost is measured and
   accepted. Prefer the smallest sequence-scoped reuse that works.

2. **Split daemon Mount and Commit time with existing LayerFS telemetry.**
   Add bounded child spans/counts in the real route for Branch resolution,
   Workspace attachment, FUSE initialization, Commit capture, preparation,
   upstream calls and publication. Record Exec spawn/FUSE activity versus
   child-output wait as needed. Relevant entry points are
   `core/crates/layerfs-daemon/src/control.rs`, `lifecycle.rs`,
   `control_commit.rs`, `execution.rs`, and Workspace `commit/operation.rs`.
   Also split Create into Docker launch/readiness/shell checks because a
   41.642 ms Create swing accounted for most of the last route difference.
   Use one prospective source-pinned diagnostic, retaining raw LFT1 streams
   and zero-drop checks. Change the largest measured step; keep the required
   single construction worker outside namespace Init.

3. **Align the active Workspace control-session lifetime where worthwhile.**
   The four per-call Hello windows totalled 20.305 ms in the corrected row,
   which bounds the opportunity from removing them in that row. Explore a
   bounded authenticated active session across rapid Workspace calls, with
   Sandbox ID and daemon-instance validation before every operation. Account
   for the control server's one-session admission and 5-second idle timeout.
   A restarted daemon must reject the old Workspace; a broken or uncertain
   operation must not be automatically resent. Keep the public SDK behavior
   compatible unless a measured need justifies an explicit session API.

4. **Align FUSE request behavior based on counts.** Count LOOKUP, attributes,
   read, write, and host-upstream requests for the live one-file route, plus
   cache outcomes if available. Both implementations already write payload
   locally, so investigate avoidable host work or repeated metadata lookups
   rather than replacing the local write path. Preserve bounded backing and
   cache honesty; never prewarm measured paths or move timed work into setup.

Execution and handoff rules:

- Start with workstream 2's cause-specific spans and counts where needed to
  choose edits for workstreams 1, 3 and 4. One performance sample per source
  identity/case; do not rerun an unchanged arm or select the fastest result.
  A new source/image/harness identity gets one new append-only receipt. Keep
  failures and ineligible rows. Pin source/tree, Cargo config, binary/image,
  harness and workload hashes, and declare cache state. No release speed claim
  from the current exploratory route.
- Keep the live correctness proof: FUSE edit without publication before
  Commit, bounded Exec output, exact Commit readback after reopening Branch
  head, exact older Commit readback, `HeadMoved`, Unmount custody, and stale
  Workspace rejection after container restart.
- For Core source changes, update the affected architecture document in the
  same commit. Run locked Core tests and examples, warning-denying Clippy,
  `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check`,
  `python3 core/tools/check_product_boundary.py`, and its unittest discovery.
  Do not run the retired preflight or claim CI. Do not patch third-party code.
- Every commit needs exact first-parent/staged-tree production LOC before,
  after and signed delta using `tools/production_loc.py`, with reference and
  Core subtotals. Report the commands, raw receipts, functional status,
  performance eligibility, limits and remaining gap; update issue #236.
