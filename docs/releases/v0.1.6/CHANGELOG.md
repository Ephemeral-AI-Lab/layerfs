# LayerFS v0.1.6 changelog

> **Status:** released changelog for LayerFS 0.1.6, 2026-09-16. Every entry is
> evidence-backed. The compatibility boundary is stated in the
> [release contract](../../../release-notes/0.1.6/release-contract.md), the owner
> dispositions in [acceptance](../../../release-notes/0.1.6/acceptance.md) and
> [waivers](../../../release-notes/0.1.6/waivers.md), and every measured result in
> the [benchmark closeout](../../../release-notes/0.1.6/benchmark-closeout.md).

1. **Sandbox-local snapshots replace host authority.** The sandbox owns the live
   mutable state and the host receives only Commit-time mutable-state transfer;
   the pause/quiesce path is gone — `FREEZE`/`RESUME` survive only as wire
   constants with no handler in the live owner's dispatch, and
   `commit_pause_fence_ns` is 0 in 56/56 SDK-edit rows. The workspace is
   continuous: no pausing, no quiescing, one Commit worker. New
   `layerfs-workspace/src/remote_commit.rs` (730 lines) and `snapshot_input.rs`
   (395) carry the route; 78 commits and 28 files under `crates/` since `v0.1.5`
   (+4031/−3187). Product seal `276c5970…` (v0.1.5) → `970964e9…`. Evidence:
   [#152 final report](../../roadmap/0.1/0.1.6/evidence/issue152-final-report.md) §5.

2. **One construction worker, by rule and by default.** `construction_worker_limit()`
   and the canonical construction it feeds are single-producer in the default
   wiring, not only by environment variable; every run exports
   `LAYERFS_CONSTRUCTION_WORKERS=1` and no run raises it.
   **Measured cost, owner-accepted as recorded:** six material regressions, all
   single `initialize` calls that lost the released 4-way small-content
   parallelism — `dedup-cross-file-identical-500` 1.65×, `dedup-cdc-scattered-500`
   1.61×, `dedup-cdc-delete-100` 1.60×, `dedup-cdc-overwrite-500` 1.59×,
   `dedup-cdc-delete-500` 1.59×, `dedup-cdc-insert-500` 1.50×, with the direct
   diagnostic `dedup-cdc-scattered-500` at 2 290.09 ms one-worker,
   **1 458.89 ms** with the variable unset, against a 1 426.04 ms comparator.

3. **Bounded spool memory, and no phase credited by its own writes.** The sandbox
   spool keeps a bounded resident window instead of holding the payload in page
   cache: sandbox residency fell from ~500 MiB to **≤ 2.6 MiB**, and the transfer
   that used to read its own recent writes at 19 GB/s now reads storage at
   2.1 GiB/s. The honest read is the sandbox-owned model's cost and it is reported
   as such, not hidden. File cache still grows ~1.03× with the payload on the
   rewrite route, with v0.1.5 parity.

4. **Two correctness defects found in-campaign and fixed, plus one reliability
   divergence.** `58e4f7f47`: `truncate_async` was the one edit path never migrated
   to the sandbox-owned spool and still sent the removed host payload check, so
   every size-changing truncate failed `EINVAL` — which is why `git commit` could
   not open `.git/COMMIT_EDITMSG`; `29835f44d`: the host-continuation Commit route
   re-based the host shell before assembling its result, so
   `WorkspaceCommitResult::Created { previous_head }` returned the newly published
   head and a Commit reported itself as its own predecessor. Both carry regression
   tests verified to fail without their fix. `ac729dfeb`: recovery from a failed
   final publication on the sandbox route was Discard-only; the route now retains
   the failed attempt and re-drives that exact frozen generation, giving
   **27/27 verification-supported `workspace_reliability` proofs PASS**.

5. **Three new benchmark families and three extended ones — multi-branch graphs,
   longer histories, concurrent workspaces and retained-state access.** 33 regular
   cases plus 3 extensions, one performance and one separate verification
   invocation each, qualification evidence:
   [#154](https://github.com/Ephemeral-AI-Lab/layerfs/issues/154) /
   [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122) and
   [the final matrices](../../roadmap/0.1/0.1.6/evidence/issue154/README.md).
   * **New families:** `file_size_transition` (7 cases: the 131 071/131 072/131 073
     threshold transitions, the roundtrip, and the alias/inode-replacement
     sequence), `multi_workspace_development` (4 cases: 2 simultaneous workspaces,
     20/200 commits), `branch_development` (6 cases).
   * **Multi-branch coverage:** trunk10 plus two children forked from trunk commit
     5 at K10/K100 — 30/210 Created commits, 3 branches, longest ancestry 15/105 —
     plus two compact graph controls on fixture S: the convergent control (trunk10,
     A10/B10 forked from trunk commit 5, 30 commits, ancestry 15, 31 retained
     roots, children proved to publish *equal* file-content IDs) and the descendant
     control (plus C10 forked from A's local commit 5, 40 commits, ancestry 20, 41
     retained roots, B/C branch-salted with every sibling head preserved).
   * **Longer history:** `dedup_branch_history` K10/K100 for three profiles
     (large-hotset, namespace-inode, boundary-cycle) — 100 Created commits and 101
     retained roots each, every parent edge and every selected state verified; the
     two exhaustive extensions replay all **101 retained states**.
   * **Retained-state access:** six `historical_access` cases mount one selected
     retained state of a sealed producer (commits 48/49 of the boundary-cycle
     history, 94/95 of the namespace-inode history, trunk commit 5 and B local
     commit 10 of the compact controls), with the declared reads repeated by the
     verifier and compared byte for byte with the producer's own declaration.
     Performance is declared `N/A` for all six.
   * **Concurrency extension:** four live workspaces, 100 M1 commits each, 400
     total, 401 retained roots.
   * **Retained deepseek-harness history:** the 157-commit `deepseek-full`
     (stride-1) profile re-ran on this candidate with one performance and one
     same-Store historical verification — 157/157 Created, verification 570.6 s
     covering 904 143 path-states and 4.9 GB — together with stride-3 and
     stride-10, each paired against a reconstructed v0.1.5 control where declared.

6. **Store format boundary: no migration in this release.** `SCHEMA_VERSION` is 10
   on both sides and no schema or SQL change landed since `v0.1.5`; the benchmark
   receipts of every family observed schema 10. The canonical-identity promise
   itself is stated in [the release contract](../../../release-notes/0.1.6/release-contract.md),
   not here.

7. **Repository process.** GitHub Actions stays disabled by owner decision; the
   former CI steps plus the benchmark harness tests run from `tools/preflight.sh`,
   which is the pre-push gate. No third-party crate is patched, vendored or forked
   and builds stay `--locked`.

## Limitations carried into this release

* Kernel-dirty shared mmap is still not captured by a Commit — the specification's
  own declared open obligation, boundary isolated to exact byte level, with no
  registered selection affected.
* The host-side FUSE write-spool metric is dead on the sandbox route and is no
  longer a gate; re-wiring it needs a new cross-boundary counter.
* Sandbox *process* memory is not emitted by the frozen harness, so only
  container-scoped numbers can be compared.
* The time comparison is cache-stance and host-load sensitive beyond the declared
  allowance.
* The six single-worker construction regressions and the waived 2.7 s cold
  `namespace-100000` Init remain on the record as accepted, not repaired.
