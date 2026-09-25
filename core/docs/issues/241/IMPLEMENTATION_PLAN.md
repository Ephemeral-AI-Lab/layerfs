# #241 implementation plan and optimization handoff

> **Status:** Current planning checklist; no release candidate exists.
> Phase 1A's Linux ioctl mechanism and four-case functional selection are
> implemented. Phase 1B investigates Commit growth before #232's full rollout.
> This plan does not amend the frozen v4 specification, registry or receipts.

Tracking: [#241](https://github.com/Ephemeral-AI-Lab/layerfs/issues/241), child of [#232](https://github.com/Ephemeral-AI-Lab/layerfs/issues/232). Read the [specification](SPEC.md), [benchmark rules](../../../../docs/general/benchmark_rules.md), [Core benchmark rules](../../../benchmark/fs-bench-pro/AGENTS.md), and [#232 Phase 2 plan](../232/ROLLOUT_PHASE2.md) before new measurement work.

## Completed route

    public WorkspaceApi::exec(command)
      -> current launcher: /bin/sh -c
      -> cooperating program opens mounted file and issues STATE / EDIT ioctl
      -> Linux FUSE adapter -> authorized projected Workspace RangeEdit
      -> bounded piece splice in private Workspace
    public WorkspaceApi::commit -> canonical C1 edit -> C2 CAS save -> Branch

LayerFS treats command text as opaque. The **ioctl carrier** is agnostic to the shell or interpreter that launched its caller: a program invoked by bash, Python, /bin/sh, or a future direct-argv Exec could issue the same mounted-file request. The **current SDK Exec launcher itself still hardcodes /bin/sh -c**. The ioctl does not transparently accelerate an unmodified editor; ordinary positional writes, append, truncate, and rename retain their normal FUSE paths. A program that rewrites a suffix still pays for those bytes. The Linux ioctl accepts one explicit range replacement with at most 4 KiB inline data. It exposes no private Rust SDK method. Future macFUSE and WinFsp adapters require separate live capability/coherence proofs; unsupported platforms fail explicitly without a suffix-copy fallback.

The [v4 position proof](https://github.com/Ephemeral-AI-Lab/layerfs/blob/cb1bdb70e97628c2c38055ff2600e070e742010e/core/docs/issues/241/evidence/phase3-postfix-v4/REPORT.md) at source 8aaf623835cf1384fbf5558e868b8d1b945830bd passed **264/264** declared insert, overwrite and delete positions, including readback, old Commit and cleanup. The [v4 four-case report](https://github.com/Ephemeral-AI-Lab/layerfs/blob/87b10ab1b2144f1d7fb54f1be9419095f891fea0/core/docs/issues/241/evidence/phase4-v4-admission/REPORT.md) at source 374e9636abb9ae10a041e0f71b1c07e84f2128f4 completed all four release-built public SDK Exec→Commit routes with independent oracle and cleanup PASS. Each used two STATE callbacks, one EDIT callback, 4,096 accepted payload bytes and zero shifted suffix bytes. The original performance receipts remain **INCOMPLETE** because of the first telemetry parser; the retained-raw-log recheck derives **INELIGIBLE** for all four under the frozen Linux FUSE backing cache contract, without a new sample or receipt promotion. Their raw times are diagnostics:

| Pristine size | Edit | Commit | Edit→Commit | EditFile service.finish |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB | 25.548 ms | 22.073 ms | 47.632 ms | 1.344 ms |
| 10 MiB | 25.342 ms | 23.062 ms | 48.413 ms | 2.151 ms |
| 100 MiB | 27.859 ms | 31.414 ms | 59.284 ms | 9.562 ms |
| capped 500 MiB | 28.213 ms | 53.452 ms | 81.680 ms | 33.145 ms |

From 1 to capped 500 MiB, Commit rises 31.379 ms while service.finish rises 31.801 ms. This localizes the observed growth to the finish span; **four elapsed-time points do not prove an O(N) algorithm or identify its substep**. The v4 report does not publish per-save object counts; retain them with the next labelled diagnostic instead of importing counts from a different edit route. The retained Store has a bounded in-memory candidate index, so do not presume a full-Store scan from these readings. The historic v0.1.6 G2 target timed direct SDK range edit, not Exec/FUSE. Keep all v2, v3 and v4 receipts and original cache classifications append-only. Do not resample a v4 arm to select a better number.

The timed 1 MiB route made one public SDK Exec and one public Commit. Caller
control sent `Hello` before each operation; daemon-to-Service issued one
`Inspect` and four `ReadFile` calls during Exec, then `EditFile`,
`UpdatePortableMetadata` and `HistoryCommand` during Commit. The authenticated
Service session was already reused. Phase 1B's local Store work should cut
**zero transport roundtrips**. A separate combined-save experiment can cut one
Service call, but its retained cache-ineligible overwrite row did not prove a
latency benefit; do not bundle that protocol change into Phase 1B. Never drop
`Hello`, `STATE` or readback solely to improve a benchmark number.

## Algorithm and resource bounds

Let N be logical file length, k replacement bytes (0–4,096 in the current ioctl), P live Workspace pieces (at most 1,024), E canonical extents, A affected extents, and S physical Store entries.

| Stage | Time | Additional space and limit |
| --- | --- | --- |
| ioctl and payload custody | O(B) validation/transfer for a fixed B = 4,192-byte frame, plus O(k) owned-payload copy; one EDIT callback | O(B+k) transient request/payload space, with k ≤ 4,096; no untouched suffix copy |
| Workspace splice | O(P+k) per request because the piece vector is rebuilt | O(P+k) transient work; existing 256-edit, 1,024-piece and 8 MiB non-base replay caps apply |
| Repeated edits | O(sum(P_i+k_i)); can reach quadratic work in edit count if P_i grows linearly | Accepted edits remain private until Commit; later offsets address the current file |
| Commit lowering | O(P) to traverse final pieces and derive ordered edits | O(P) descriptors, including edits to bytes inserted by earlier requests |
| Chunked C1 | Approximately O(k + boundary work + A log E) for a localized edit; actual touched paths/chunks need counters | New chunk and path nodes; unchanged CAS identities reused; FULL/DELTA selects physical encoding |
| WholeFile/cutoff transition | May read/construct O(N) final bytes | Exact cutoff−1/cutoff/cutoff+1 correctness remains necessary |
| C2 service.finish | Unknown dependence on S, packs, SQL pages, locks and physical reads | Measure the substeps before changing the algorithm |

Neither the extent tree nor one bounded splice proves whole-route O(log N). A single middle edit leaves only a few pieces; a piece-tree replacement is unjustified until a **prospective repeated-edit count diagnostic** shows material piece rebuilding. Preserve canonical roots for the same base and ordered edits, save atomicity, one construction worker, and the no-sync persistence policy. Do not change delta hints, CDC cutoffs, CAS layout, or third-party packages without cause-specific evidence. Never patch, fork, vendor, or locally modify third-party code.

**Phase 1B does not promise to eliminate every linear or quadratic term.** It targets the observed growth inside C2 `service.finish`. Phase 1A removed suffix movement and repeated write callbacks for a cooperating ioctl editor; the current `O(P)` splice and possible `O(R²)` repeated-edit total remain. The #232 cases register one logical edit each, although an ordinary POSIX write may produce multiple FUSE callbacks; count those callbacks and pieces before deciding a piece tree is needed. Use the [separate repeated-edit diagnostic](../232/ROLLOUT_PHASE2.md#optimize-only-the-remaining-measured-mechanism) before proposing one.

## Exact files and folders for the next change

These existing paths are a review map against the v4 source tree. Touch only paths selected by counts; do not scaffold a new layer or expose private APIs to tests.

| Path | Responsibility |
| --- | --- |
| core/crates/layerfs-server/src/service/save/content.rs | Retain the parent LFT1 service.finish scope and add bounded LFT1 children for any diagnosed substep used in a reported wall/CPU/RSS optimization claim. |
| core/crates/layerfs-storage/src/cas/lifecycle.rs | Diagnose group seal, candidate flush, ownership publication, SQL commit, then **separately** the post-commit `PoolIndex` and `Candidates` shared-index clones within finish_inner. Record each index's entry count and bytes. Existing SaveProfile diagnostics can guide placement of production LFT1 children; only LFT1 supplies reported wall/CPU/RSS. The owner release/drop profile is **after** service.finish, so do not blame it for this span or presume either clone is the cause. |
| core/crates/layerfs-storage/src/cas/placement.rs, cas/pool_lane.rs, cas/store.rs | Conditional owners of placement, index or Store work **only if** counts implicate them. No blanket CAS rewrite. |
| core/crates/layerfs-daemon/src/execution.rs | A separate fixed Exec-cost inquiry may count spawn, output waits, FUSE calls and Service requests. Do not replace the command with a private edit method. |
| core/crates/layerfs-workspace/src/overlay/pieces.rs, commit/lower.rs | Conditional repeated-edit optimization only after measured P growth or lowering cost. |
| core/crates/layerfs-fuse/src/range_ioctl.rs, adapter.rs; core/crates/layerfs-workspace/src/filesystem/write.rs | Existing ioctl carrier and portable mutation. Preserve ABI, writable-handle authority, stale-version refusal, mounted coherence and reply semantics. |
| core/benchmark/fs-bench-pro/registry/, shared/, workload/src/{main,splice}.rs, runner.py, tests/ | New prospective scenario/diagnostic only. The existing splice tool issues the documented mounted-file ioctl; the host driver uses public SDK for every Project, Branch, Sandbox and Workspace operation. Never call private LayerFS Rust methods, Service or Store directly. |
| core/docs/issues/232/, core/docs/issues/241/evidence/ | Commit a new contract and identity before a new numeric sample; append raw receipts and all failures. Historical registries/receipts remain immutable. |

The benchmark is a separately registered **SDK Exec/FUSE workflow**. It is not the direct SDK edit_workspace_file_range(s) family governed by the distinct direct-edit invariant in the benchmark rules.

## Next decisions, in order

1. **Attribute finish growth.** Under a new, labelled diagnostic identity, compare the same 1 and capped-500 MiB shapes. Use layerfs-telemetry LFT1 alone for reported wall/CPU/RSS; retain existing SaveProfile diagnostics and object/pack/lookup/statement/page/physical-read counts only to guide and explain new LFT1 child scopes. Identify whether seal, index flush/copy, publication, SQL commit, or contention grows. This is a cause-finding diagnostic, not another v4 performance sample.
2. **Change only the measured substep.** Retain ioctl and canonical semantics. Prove the changed path with focused tests, public SDK Exec→Commit, an independent verifier and cleanup at the final identity.
3. **Freeze any new numeric gate prospectively.** Conditional engineering aims under an equal, declared cache state: raw Edit→Commit ≤55 ms at 1/10 MiB, ≤60 ms at 100 MiB, and ≤70 ms at capped 500 MiB (≤65 ms stretch); service.finish growth, defined as 500 MiB minus 1 MiB, ≤15 ms versus v4's 31.801 ms. If the other terms stayed equal, that finish reduction would yield about 65 ms at 500 MiB; ≤70 ms allows modest variation. These are **proposals, not existing PASS gates**. A changed admission target needs a new committed specification/scenario identity before candidate optimization or sampling. If the Linux backing cache cannot be enforced and checked, classify a fast row INELIGIBLE and do not claim a latency PASS.
4. **Close Phase 1B, then screen complexity under #232.** Retain cause attribution and the cache decision. If a narrow fix is justified, retain its new-source four-case SDK Exec→Commit result, independent verification, cleanup and zero suffix I/O. A target miss or ineligible timing remains visible. If no change is justified, record that decision instead of adding speculative code or repeating unchanged arms. The [Phase 1C screen](../232/ROLLOUT_PHASE1C.md) decides whether repeated-piece or C1 work needs a separate fix before the 56-case Phase 2 rollout. Phase 1 insert evidence does not establish performance of the other edit routes.

For any new run: locked **release** binaries, one construction worker, one sample per declared case and arm, fresh append-only output, complete command ≤15 s, separate identity-matched verifier, and no warm-cache credit. Preparation may reuse closed validated masters via independent writable byte copies outside the timer, never move Edit/Commit work into setup. Preserve raw LFT1 and producer/sample-window coverage; partial CPU/RSS windows are not exact phase CPU or peak memory.

## Commit checkpoints for Phase 1B

Keep these as separate commits on an isolated worktree based on the reviewed
#241 v4 source. Every commit message records exact first-parent production LOC
before/after/delta; a source algorithm change also updates its architecture
document in the same commit. The current `bb50` tree is a planning checkout,
not the implementation base.

- [ ] **B0 — Freeze the diagnostic contract (docs/registry only).** Pin the 1
  and capped-500 MiB shapes, cache treatment, exact source/image/tool IDs,
  output paths, LFT1 labels, index counts and expected SDK/FUSE/Service calls
  before the first attempt. A new numeric candidate gate and scenario identity
  must be committed before candidate optimization or sampling. Production LOC
  delta is 0; preserve all v4 receipts.
- [ ] **B1 — Instrument, without optimizing.** Add bounded LFT1 children for
  batch drain, pack seal, candidate flush, ownership publication, SQLite
  commit, post-commit `PoolIndex` clone and post-commit `Candidates` clone.
  Record index entry/byte counts. Focused storage tests check successful and
  failed finish behavior, parent/child scope completeness and unchanged save
  results. This checkpoint makes no speed claim and changes no ioctl ABI.
- [ ] **B2 — Diagnose once at the instrumented identity.** Run the two frozen
  release SDK Exec→ioctl→Commit diagnostics through independent writable
  master copies. Retain raw caller/Service/daemon LFT1, callback and object
  counts, cache status, complete-command wall, verification and cleanup;
  identify the growing substep. This is append-only evidence, not a replacement
  v4 performance sample. If no causal, safe optimization follows, record that
  decision and stop Phase 1B product changes.
- [ ] **B3 — Make one causal product change, if justified.** Edit only the
  responsible CAS/SQL/index substep, preserving transaction atomicity,
  canonical roots, one worker, no sync and published third-party packages.
  Add one focused regression check and update the affected architecture doc in
  this product commit. Do not preselect index cloning as the fix.
- [ ] **B4 — Prove the changed source.** At the frozen new identity, run the
  four selected release Edit→Commit cases once each through public SDK and
  mounted ioctl, then separate identity-matched verifiers. Report raw times,
  `service.finish` growth, CPU/RSS coverage, cache eligibility, every target
  miss, zero suffix I/O and confirmed cleanup. If B3 was not made, do not
  repeat the unchanged four arms.
