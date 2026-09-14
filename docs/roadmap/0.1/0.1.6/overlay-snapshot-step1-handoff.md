# Snapshot-Isolated Workspace: verified integration checkpoint and successor handoff

This is an unfinished implementation checkpoint for [#124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124) and [#125](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125). **Both issues remain open. V1 and complete public-path acceptance remain unresolved. No final benchmark campaign has run.**

The owner changed this run's stopping point to: finish the three current integration fixes, publish the accumulated scoped source to remote `main` through the repository workflow, and leave this handoff. That instruction supersedes this run's earlier continuation-through-closure instruction. “Step 1” here names that bounded checkpoint; it does **not** mean the implementation plan's Phase 1 is complete.

## Delivered source and evidence

- Product/evidence commit: [`f8ed6bbbb3cc86eea23b82d3efa96c70b0519d2a`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/f8ed6bbbb3cc86eea23b82d3efa96c70b0519d2a).
- Review/publication PR: [#126](https://github.com/Ephemeral-AI-Lab/layerfs/pull/126), base `main`. The merge result and remote-main ancestry verification are published in the final issue checkpoint comments. Read back the PR and remote ref; do not assume a feature-branch push is main publication.
- Exact formatted source hashes: [publication source seal](evidence/step1-publication/source-before.json). Source did not change during publication verification. This handoff and final progress/ledger edits are documentation only.
- [Checkpoint result matrix](evidence/step1-publication/checkpoint-results.json), [append-only verification ledger](overlay-snapshot-verification-ledger.md), [current progress/history](overlay-snapshot-progress.md).

The final mounted rerun used:

| Artifact/domain | Identity |
| --- | --- |
| macOS native test binary, Rust 1.85.1, all features | `target/debug/deps/layerfs_workspace-6b336fb7a81e951e`; SHA256 `1f84e1f00c529a5796029a5fdfe644ac0faca06c4534353e84843f0993994325` |
| Linux FUSE helper, existing stable Rust 1.96.0 + Zig | `target/aarch64-unknown-linux-musl/debug/layerfs-fuse`; SHA256 `29f56dcd24aad94149510d8f0474a2d9895b96ea7904465404aa39172a33e513` |
| Docker image | `sha256:b9d3d2c3090596364d2d70ee304a5b7316b8dc51b9d672b8a0b3857e1de940f3` |
| Kernel / workload | `6.12.76-linuxkit`, Python 3.11.2; LinuxKit identifies the test environment only |
| Topology | macOS Store, HostOverlay, SDK/coordinator, canonical construction and spool; Docker helper/FUSE/workload only |
| Limits | 2 CPUs, 2 GiB, no swap, 256 PIDs, nonprivileged, no host data binds; `/dev/fuse` and `SYS_ADMIN` |

## The three requested fixes: verified results

1. **Mounted shutdown: PASS.** `HostClient::prepare_shutdown` now releases the preopened mount-root descriptor before unmount. A running flush retains its own `Arc` until completion. Attaching a new root after shutdown is rejected. Local SHUTDOWN uses the same preparation; remote shutdown acknowledgment still follows actual unmount/helper cleanup. The [failed mounted attempt](evidence/phase3-host-operations/mounted-host-attempt01.json) reached its content/identity assertions, then failed with EBUSY. Its stderr, fallback records and source identities are retained. The [corrected mounted rerun](evidence/phase3-host-operations/mounted-host-attempt02.json) passed, including normal unmount and verified owner-container removal. [Raw PASS output](evidence/phase3-host-operations/mounted-host-attempt02.log), [exact runner](evidence/phase3-host-operations/mounted-host-attempt02-runner.py).
2. **Actual-spill fixture: PASS.** Uniform 2 MiB data deduplicated below the spill threshold, so the original test correctly failed its “real spill” assertion. Deterministic varied input now forces actual canonical spill; the same output/admission ownership and exact return-to-baseline assertions pass. [Original FAIL](evidence/phase4-candidate/capacity-owner-attempt01.json), [targeted PASS](evidence/phase4-candidate/capacity-owner-attempt02.json). No production limit or oracle was weakened. The five real scratch/SQLite/data/order provider checks also [passed](evidence/phase4-candidate/scratch-integrated-attempt01.json).
3. **Maintenance oracle: PASS.** A construction error before staging correctly clears the attempt. The old test incorrectly expected retention. Its replacement first preserves that assertion, then injects a publication failure and proves an actual canonical stage/retained attempt before testing deferral and abandonment. [Original 2 PASS / 1 FAIL](evidence/phase4-candidate/maintenance-attempt01.json), [repaired exact PASS](evidence/phase4-candidate/maintenance-attempt02.json), [three-pass manifest and reuse rationale](evidence/phase4-candidate/maintenance-manifest.json). Production maintenance did not change for the oracle correction.

Related scoped repairs/checks include exact SDK BEGIN/APPLY/CANCEL retries, pre-reserved file readers, stable target pins across rename, post-detach cleanup and restoring the existing `CanonicalPath` input validator. The original invalid-path failure is retained. See [SDK handoff](evidence/phase3-host-operations/sdk-host-coordinator.md) and [adapter ledger](evidence/phase3-host-operations/adapter-ledger.md).

The mounted case proves SDK visibility through a retained mapping/descriptor, preservation of an unrelated dirty mapped byte during that SDK operation, stable inode/hardlink/rename/open-unlinked behavior, explicitly owned C1/C2 input and cleanup. **Its capture closure supplies an owned host snapshot; this is not proof of generic kernel-dirty Commit acquisition or the full public SDK Commit path.** Its approximately one-second command wall is correctness-test context, not a benchmark latency or speedup claim.

## Publication checks and CI debt

[Raw logs and receipts](evidence/step1-publication/) retain these actual results:

| Check | Result |
| --- | --- |
| Rust 1.85.1 `cargo build --workspace --locked` | PASS |
| `cargo fmt --all -- --check`, `git diff --check` | PASS |
| Python test-fast runner tests | PASS 7 |
| Full Rust 1.85.1 native gate | 637 selected tests/benchmarks, 37 binaries, 79 batches; 611 reported passes, 26 ignored, no executed-test failure; **gate FAIL because warm time171s exceeded150s** |
| Rust 1.96.0 strict Clippy, workspace/all-targets/all-features | **FAIL**; unused/dead code and style findings in `strict-clippy.log`; do not describe CI as green |
| Final Linux helper build | PASS with configured default stable toolchain/Zig |
| Independent review of the bounded fixes | No new scoped blocker; not approval of the unfinished feature |

The named `+1.96.0` toolchain lacked Linux target std; that failed attempt is preserved. Reusing the already configured default stable toolchain repaired the build without installing another runtime. The first publication `artifacts.json` listed existing files after the failed command; the subsequent `linux-helper-attempt02.json` is the successful helper provenance.

The owner raised CI's budget from120s to150s in `f35e0039b`. The historical125s run remains FAIL under its original120s budget. The final171s miss is a separate real miss under150s. No deadline was raised again, no passing suite was rerun to obtain a better time, and no lint suppression was added. Prior remote CI also reports unwired private Workspace components; default-path integration is still required. Publication checks are **not clean**, even though the three bounded fixes pass. GitHub reported no main branch protection or applicable branch rules; no required approval/status rule was bypassed.

## Current implementation and unresolved obligations

The working source now contains disk-backed overlay roots/indexes/payload/ranges, leased-source installation, bounded replay, owned snapshot readers, HostOperations/HostClient, exact attempt/stage/publication receipts, canonical predecessor correspondence, optional physical canonicalization, bounded directory/covered-key cleanup, HostSdk and HostRuntime composition. The helper and daemon support an explicit host authority chosen before mount; a failed request never selects a legacy fallback.

**The existing public default Workspace mount/Commit route is still legacy.** The new runtime is exercised by concrete local/TCP and mounted component checks. Do not claim production switchover or remove shared legacy helpers before their non-Commit consumers are correct.

| Obligation | State and next integration requirement |
| --- | --- |
| V1 generic FUSE visibility | OPEN. `NOTIFY_RETRIEVE` can expose dirty bytes but its retained folios remain mutable; the post-reference mmap write counterexample is preserved. No custom LinuxKit/VM dependency, mmap removal, implicit user fsync, weakened visibility or global freeze is approved. SDK cache reconciliation does not solve Commit acquisition. |
| V2 ownership and capacity | Component root/replay/range/reclamation checks pass, but default `Payload.limits.owners=8192` counts persisted tokens, not only transient handles. The proposed8193-file reproducer is NOT_RUN and no payload correction landed. Ordinary host SnapshotReader cache also needs explicit live-domain accounting. |
| Canonical scratch/admission | Current96MiB reservation is provisional and admits at most one default construction under shared128MiB. The proposed44/48MiB profiles are analysis only. Admission-owned seen spill, scratch placement, full-attempt peak sampling and scoped cleanup retry integration remain open; see the capacity note below. No second encoder was introduced. |
| V3 predecessor/C1/C2 | Component tests pass for last canonical predecessor reuse, new inode identity, localized edits, zeros/deletion and unchanged live bytes. Full public-path, capacity and policy qualification remain pending. Optional substitution preserves source-root CAS and later writes; piece-cap deferral keeps unconverted raw bytes charged. |
| V4 publication uncertainty | Exact retained-stage, lost Created/UpToDate, conflict rollback and authoritative abandonment component checks pass. Integrate them with the public Workspace lifecycle, recovery, End and Discard. Unknown publication must not erase a possible success. |
| Default FUSE/SDK/lifecycle | Migrate ordinary FUSE creation, SDK, observation, dirty state and End coherently to one host authority. Dirty means live sequence greater than published coverage, plus pending lifecycle work; never sequence!=0. Commit must not hold `worker.lifecycle`/live maps for construction or reject ordinary commands merely because a stage exists. |
| Normal shutdown / recovery | Recover SDK coherence before requesting normal helper shutdown. `HostRuntime::after_detach` requires verified consumer/control retirement; it resolves Commit abandonment, then SDK ownership and kernel references. Do not call it as a Commit shortcut. Interrupted End and lost-helper paths still need full acceptance. |
| Non-Commit consumers | Explicit Materialize/reconciliation, fsync/cache controls, conflict fingerprints, metrics and presentation recovery still use old paths/maps. Preserve reconciliation private readers and unrelated-write/conflict-choice semantics. The dormant branch-lease API is not currently called by Workspaces; retain existing same-branch CAS behavior rather than adding a lifetime-exclusive lease by assumption. |
| Plan phases / correctness | Phases2–5 have substantial component evidence, no whole phase completion or Phase6 seal. Phase1's V1 exit remains open. Do not post a phase-complete comment without its actual exits; corrections to the predecessor's Phase1 completion claim are already on#124. |
| Million-file qualification | NOT_RUN. #123 requires an explicitly declared at-least-million changed-regular-file case with fixed RAM, adequate disk, one final Commit, exact verification and fresh Store reopen. No fixture/work/deadline contract has yet been frozen for that million case. Do not close#123 or substitute small cases/arithmetic. |

Read [host consumer audit](evidence/phase3-host-operations/host-runtime-consumer-audit.md), [capacity handoff](evidence/phase4-candidate/capacity-progress.md), [capacity source inventory](evidence/phase4-candidate/capacity-stopping-source.json), [maintenance handoff](evidence/phase4-candidate/maintenance-handoff.md) and [payload admission audit](evidence/phase3-host-operations/payload-token-capacity-audit.md) before choosing the next change.

## First executable successor task

Reproduce the persisted-token admission defect with8193 distinct ordinary one-byte files under the unchanged existing disk limits, using the existing HostOverlay fixture and bounded reclaimer. Record the first failure's source/configuration/owner/physical counters. Separate persisted catalog ownership from transient handles and bounded release-queue admission, then rerun that failing component and the smallest affected retained-reader/reclamation checks. **Do not simply raise8192, allocate a million-entry resident queue, or call that small reproduction the million-file proof.** This task does not depend on V1.

After that, finish traced live-reader/canonical memory and scratch/admission accounting, then the coherent default-path migration from the consumer audit. Continue the broader original#124/#125 objective only when authorized in the successor execution; this run intentionally stops at this published checkpoint. Use distinct V1 hypotheses and existing evidence; do not spin on an unchanged probe. Final full-surface acceptance and benchmark sealing remain blocked on V1 and all other required correctness exits.

## Pass custody and rerun rules

Keep each passing result with its recorded binary/source/environment. `maintenance-manifest.json` explicitly qualifies the two reused passes; only the failed oracle was rerun. Valid SDK state-machine passes were retained when the path guard changed, with one representative valid retry and the new invalid-path check refreshed. The final formatted-source native gate was a publication custody check, not a claim that every development edit requires a full rerun.

Do not rerun an unchanged passing test just because HEAD advances. Trace affected callers/shared ownership/encoding/input/environment first. Do not label older binary evidence as final candidate evidence. Preserve every original failure, timeout, setup error and incorrect-selector record. Current native builds/caches and sealed helpers can be reused when compatible; no `cargo clean`, cache deletion, routine fixture regeneration or rerun for nicer timings.

## Benchmark scope, still unexecuted

The final active-suite campaign is NOT_STARTED. No#122 scenario was executed as part of this campaign. Native harness unit checks and a reused Docker infrastructure image are not execution of its registered scenarios.

- Exclude all36 exact#122 cases:33 regular and3 extended, every mode and case-specific setup. [Exclusion manifest](benchmark-exclusions-issue122.json) SHA256 `683137931278476b2e15215c432d5095204cf0b46263ce95e8ba27a05e1335c5`; source `cases.json` SHA256 `6a89e9ac1c5df25eb0a4a495cf013d0eb8ab66f55b10281de1f5d72b0a0fcdc8`.
- [Phase1 catalog](evidence/phase1-catalog/scope-ledger.json) is provisional:260 registry rows,232 included registry rows plus11 separate historical-access rows, provisional243 total. It also records optional/exploratory family entrypoints requiring reconciliation. It is not the final include manifest or launch authority.
- Reconcile current#122/registry/exact successors and every family-specific entrypoint, modes, seeds, repetitions and extended/proof-only obligations from the eventual sealed candidate. Preserve inherited cases in shared families; exclude cases, not families.
- Never use `shared/v016_matrix.py` or the generic `handoff-prompt.md` as this campaign driver; both target#122. Read `benchmark/AGENTS.md`, `docs/general/benchmark_rules.md` and `benchmark/fs-bench-pro/QUICKSTART.md` before benchmark work. General rules/QUICKSTART were consulted here; full final-scope review remains necessary.
- Phase7 requires a correct sealed candidate, not prior closure of#124. Serialize builds/performance/verification under the existing measurement lock and preserve host/Docker domains. Actual per-case matrix/numbers/full validation are absent. No new performance gate, release, tag or website action belongs to this handoff.

## Local state and resumption safety

Workspace: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`; working branch `codex/snapshot-isolated-workspace`. Product/evidence were committed before this document; all accumulated scoped source, predecessor evidence and failed attempts were preserved. After the handoff commit and remote merge, inspect `git status`, fetch main and verify ancestry before selecting another checkout. Do not discard this branch or start from an old clean checkout that lacks the work.

There is no owned running Docker container, build/test process or measurement lock after the final mounted check. Both test containers were removed; attempt02 verifies absence. The first failed fixture Store remains intentionally at `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-mounted-host-w:ceb6ea4c655d7f2658d4c90d6e1d6b14`; its diagnostic logs are published. Other failed temporary fixtures/raw `benchmark-results` evidence and compatible Cargo caches remain in place. No cleanup outside owned test resources was performed.

The PR/main push can trigger repository CI. A pending or failing CI run is an explicit successor obligation, not terminal feature success. Read the final checkpoint issue comments for publication/CI readback; leave#124/#125 open until the original full terminal conditions are actually met.
