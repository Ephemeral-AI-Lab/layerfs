# #284 implementation-only handoff: active backing into Phase 4.5

> **Status:** Archived; retained for historical evidence only.
> Issued 2026-09-29 for [integration issue #284](https://github.com/Ephemeral-AI-Lab/layerfs/issues/284).
> Construction source: `05eb5c14849f1b874383fd1600ba9288f103ad93`.
> Side product donor: `11a864fc133844cae7a4247b1f243d84d5763b10`.
> Frozen design documents: `f9f9abb37332e23d1968a5ed204ff70e3e66d783`.
> Phase A completed at product `671f4a46f8f354cf764d3d1555529b73a5c939e0`,
> documented at `4a4447db1abcbfccea0bcf1c79b9cc1d80ac2f04`.
> The owner subsequently assigned Phase B in [sub-issue #286](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286).
> Use the [Phase B handoff](../286/HANDOFF-PHASE-B-20260929.md) next.

This prompt records the completed implementation assignment. Its Phase B hold
and instruction to stop after construction are historical. The new handoff
supersedes them; the [Phase A report](PHASE-A-IMPLEMENTATION-REPORT.md) retains
its original evidence and unrun selections. Do not restart Phase A from this
prompt.

## Paste this prompt to the next implementation agent

You are the sole construction owner for **#284 Phase A**. Implement the
selective integration of the #273 active-backing and Commit mechanisms into
the corrected component-only Phase 4.5 namespace. Complete the implementation
and its necessary correctness checks, update the draft integration PR and
publish a precise implementation report. **Do not run Phase B benchmarks or
merge the PR.**

The owner's latest instruction is implementation only, with benchmark work
held as a separate phase. This supersedes the earlier design plan's benchmark
evaluation points during construction. Its architecture, conflict map,
invariants and later evaluation requirements remain applicable. Do not treat
an older continuation prompt as authority to start the benchmark campaign.

### 1. Verify and use this owned checkout

Work from:

```text
/Users/yifanxu/.codex/worktrees/phase45-active-integration/layerfs
branch: codex/issue284-phase45-integration
construction base: 05eb5c14849f1b874383fd1600ba9288f103ad93
review base branch: codex/issue264-phase45
```

First run `pwd`, `git status --short --branch` and `git rev-parse HEAD` in
that exact directory. Inspect the task's draft PR and the branch history.
The bootstrap includes the existing one-seal repair and this handoff; it does
not yet contain the selective active port. Verify the expected branch and
T0 ancestry before editing. Do not infer the directory from another checkout.

Read repository `AGENTS.md`, `core/AGENTS.md`, relevant component contracts
and architecture descriptions before changing their owners. Read benchmark
and release rules before touching their scope, but execute no benchmark phase.
All Cargo targets, builds, scratch and evidence belong to this worktree.

Preserve the #260/#263/#269/#274 PR heads and other owners' worktrees. Do not
alter `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-273-finalize` or its two pending
edits. The pending readable fixture there uses `maximum_version`, while the
sealed donor's actual capability field is `version`; it is not a verified
patch to import. Use the clean donor and adapt the external fixture explicitly.

The published Phase 4.5 head is
`6eb7553671d3000160ad023a57b335e52dd81a26`. T0 adds its one-seal repair.
The side donor is P0 above, not PR #274's older head. Frozen earlier control
`48b51e874a41b3e1e6c6661e145316df8b408f07` is a separate history and stays
NOT_RUN; it is not T0's descendant. Both histories occur in the donor.

### 2. Read the frozen design, using its exact source

These documents are published at the design seal, not necessarily present
in this older component-only checkout:

- [Selective implementation/merge plan](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/IMPLEMENTATION-MERGE-PLAN-20260929.md)
- [Architecture refinement](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/architecture_refinement.md)
- [Private backing/live Commit workflow](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/private_backing_workflow.md)
- [FUSE workflow](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/fuse_workflow.md)
- [Benchmark layout](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/BENCHMARKS.md)
- [Baseline inventory](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f9f9abb37332e23d1968a5ed204ff70e3e66d783/core/docs/issues/273/phase45-merge/BASELINE-20260929.md)

You can read exact versions locally without checking out another branch:

```sh
git show f9f9abb37332e23d1968a5ed204ff70e3e66d783:core/docs/issues/273/phase45-merge/IMPLEMENTATION-MERGE-PLAN-20260929.md
git show 11a864fc133844cae7a4247b1f243d84d5763b10:core/crates/layerfs-workspace/src/runtime/state.rs
```

Use `git show`/diff for source comparison. **Do not merge the donor branch,
blanket cherry-pick its history, or replace shared state/namespace files with
donor copies.** Keep a source-pinned reuse/adapt/exclude manifest. Recheck a
changed upstream target before continuing; do not silently update the frozen
construction method or overwrite another owner.

### 3. Implement this architecture

```text
ordinary caller command -> POSIX / real FUSE
        |
parent serial + one component / stable file handle
        |
main NodeName + parent/attached graph + complete charged ancestry
        |
selected View: immutable filesystem Base + ActiveSnapshot
        |
ONE active private I/N/D/E/P/R/L authority
        |
verified pages, shared packs, owned payloads, selecting pins and completion fund
        |
capture G1 / live G2 -> internal SaveFile v2 -> C1/C2/History
        |
known outcome -> affected C5 install, preserving later G2 bytes/edges/origins
```

Keep T0's component-only `NodeName`, `runtime/ancestry.rs`, `check_name`,
identity-relative reads and complete ancestor retention for both LocalEdit
and ReadOnly. The donor's active records are already serial/component based.
Reuse their physical algorithms and adapt callers to T0.

Remove donor aggregate paths from Node, View, directory handles and private
HeldEntry. Do not introduce NodePath, extended_path, child_path_active,
PinnedDirectoryPath or rename_paths into the target. Avoid inherited descendant
enumeration and resident path rewriting. Direct canonical LogicalPath,
physical backing/mount paths and an individual pathname syscall retain their
own independent limits.

Rename prepares its constant logical N/I/D patch and component owner. Under
the final State gate, recheck generation/revision and attached ancestry,
publish the verified active candidate, then install only the moved Node edge.
Do not refuse a cycle after publication. Preserve replacement/detach, hard
links, open-unlinked ownership, frozen directory names and current/last `..`.
Use node_index for one-identity attribute updates, not a new all-Node scan.

Public lease wire remains unchanged: serial/kind/size/references/mode/mtime
in entry results, selected context in the lease header, token and
serial/component/range in requests. Canonical content selection stays private.
Adapt HeldEntry and selected resolution without full paths, preserving token
binding, issued-entry authority, 32-lease admission, response Budget and checked
release. A frozen view is not authorized by the later live ancestry graph.

### 4. Port coordinated mechanisms, in dependency order

1. **Compatibility and scoped C1 repair.** Preserve v1 opcode 20. Port approved
   authenticated capability 28/internal SaveFile 29, request/result classifiers,
   native codecs, Service parser/fixed-record origin resolution and exact
   typed version 2 attachment admission. New LocalEdit must fail before
   accepting dependent mutations on an incompatible Service. No recursive
   daemon ReadFile while upload owns its session mutex, result-sized Base
   spool, new public range API or ioctl. Port the native read-deadline and
   Sandbox held-session expiry fixes without mutation/release replay.
2. **Independent filesystem repair.** Port
   `fbda0f0f1dbb6bb3ddd375694e3ea7edbab62ebd` on its matching T0 inputs:
   `filesystem/update.rs`, `validate.rs`, `validate/cycles.rs`, the external
   ordering proof and architecture description. Retain actual membership/
   resident-walk charges and alias/cycle validation. No new canonical parent
   index, root format, C2 schema or History schema belongs to this port.
3. **Coherent active Workspace.** Reuse path-independent `backing/active/`
   page/index/pack/extent/HotRef/retirement algorithms and completion primitives.
   Adapt state, selected reads and all namespace/content mutation callers to
   T0's components. New LocalEdit has one active authority; legacy saved-result
   bookkeeping is not a second mutation backend or automatic fallback.
4. **Capture/completion before dependent acknowledgement.** Wire active
   attachment, mutation, precharged fund, immutable capture, grouped upload,
   saved observations and C5 as one runnable route before accepting active
   writes. Combine dependent edits into a coherent commit rather than adding
   a temporary test feature or partial runtime mode.
5. **Identity-based selected owners and public control.** Port additive lease
   API/Bridge/daemon dispatch and native owners only when active selection is
   executable. Keep one lifecycle slot, conditional shared-Host metadata
   admission and the one serialized authenticated session. No second
   connection/construction worker is added for progress.
6. **Finish invariants and source descriptions.** Cover nonmatching and
   arbitrary overlap/append/resize/Zero, metadata, namespace/hard links,
   symlinks, old selections, clean/dirty Commit and failures. Update architecture
   and compatibility alongside actual product changes.

### 5. Correctness and custody must survive the adaptation

- `State.base` is a filesystem root; `I.base` is a file-content root. Selected
  active origins take precedence over canonical refresh. Do not rebase G2
  extents to saved G1 bytes just because filesystem R0 advanced to R1.
- A matching captured regular revision can collapse to saved Base. Later G2
  with an existing nonzero Base retains its coordinates; a fresh absent Base
  follows the source's allowed adoption rule. Symlink target representation
  and directory edges are not regular-file collapse cases.
- Dirty capture transfers the same completion fund. Clean capture without a
  live fund explicitly admits one. G2 first dirty publication has its own
  fund. Completion credit is owned before dirty publication; failure can
  retain precharged credit rather than invent a refund.
- Partial allocation conserves unused credit in the same fund. Finishing
  releases unused reservation. Allocated physical-owner refund follows exact
  identity/block-checked unlink. Current, pinned, foreign and uncertain owners
  are not released to create headroom.
- Save known file roots before another fallible metadata call; save a checked
  canonical outcome before local C5. Explicit native same-selector local
  resume never reissues the canonical command. Public SDK Commit is not a
  retry/resume facility for a retained Submission.
- Published notification/cleanup error retains the accepted revision and its
  receipt/handle. Unknown/lost results retain exact selectors, pins, charges
  and failed custody. Unmount, clean close and Sandbox delete stay distinct.
- WorkspaceApi.exec executes any caller-supplied command through ordinary
  mounted POSIX/FUSE. Verified byte/identity provenance may reuse Base extents;
  command text, test identity and benchmark fixtures never select that route.

### 6. Iterate quickly, using necessary correctness checks only

Build changed owners incrementally with locked, worktree-local targets. Use
the smallest public external check that covers the changed seam; diagnose
its retained output and fix the shared cause. Do not create helper-mirroring
unit suites or rerun unaffected passing selections after each edit.

Small semantic checks during Phase A may cover exact bytes, capability
compatibility/refusal, component traversal/held ancestry, G1/G2 origins,
same-fund ownership and known/unknown failure disposition. They are correctness
checks, not timing samples or substitutes for the later full selection.
An image-less early return/host ignore is NOT_RUN, not live SDK/FUSE evidence.
Use actual enum `Response::FileSaveCapabilities { version: 2 }` in external
fixtures. No test hook, fake, inline test or test feature enters product src/.

At the frozen implementation source, run the required owning Core checks once:

```sh
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked --workspace
cargo +1.85.1 check --manifest-path core/Cargo.toml --locked --workspace --examples
cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check
cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace --all-targets -- -D warnings
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
git diff --check
```

Split coverage into bounded owning-package/test-target commands if an aggregate
command would exceed three minutes or enter the held benchmark selection.
Record exact executed/deferred targets; do not disable discovery or claim a
skipped route passed. Use repository-root ARMv8 AEAD flags, not a replacement
RUSTFLAGS setting. There is no CI and tools/preflight.sh is retired.

Keep production files at most 999 physical lines, lib.rs/mod.rs at most 200 and
declaration/delegation only. Never patch/fork/vendor third-party code, increase
workers/quotas/deadlines, add Workspace fsync, relax an oracle or introduce a
private test carrier. The one canonical construction worker remains; Init's
existing exception stays unchanged.

For every commit, count exact first-parent and staged-tree production source
with `python3 tools/production_loc.py --json --root <snapshot>`. Record reference,
Core and combined before/after/signed delta, method and source scope in the
commit and handoff. Count this target tree, not the donor's totals. Algorithm,
format/boundary changes update their architecture in the same commit.

### 7. Phase B is held; leave its work explicitly unrun

**Do not execute or optimize the benchmark campaign during this assignment.**
Do not reorganize benchmark runners/registries or collect new timing baselines
as incidental implementation work. Do not invoke these full evaluation
selections even when a driver labels them functional or diagnostic:

- Full 3x3 append/dispersed/repeated x 100/512/4097 and original #248 campaign.
- Full native 8192/default 8 MiB, 10,240 pressure-boundary and prospective SDK 8192
  evaluations. A smaller NodeName allocation can change the old capacity
  outcome; do not manufacture its historical refusal.
- Full registered 64 MiB lowering, occupied-headroom, retained-generation Commit,
  mixed mutation/namespace, live-failure/cleanup qualification campaigns.
- History stride 10/3/1, 17/53/157 states, SDK Init performance tiers, package/
  many-file scaling and any numeric-profile/cache/memory qualification.

Preserve the design's future family names, case IDs, limits, exact oracles and
baseline roles. Stage these as Phase B TODOs with identities/dependencies and
any implementation concern, not fabricated PASS receipts. Independent small
correctness checks above remain allowed; do not rename a full held campaign
as a smoke test to execute it now.

Additional host-memory qualification is deferred to #283. Actual Budget/quota/
physical/pin/refund correctness remains mandatory. Do not spend Phase A trying
to solve symmetric host/VM/backend/device cache admission. Historical numeric
rows remain INELIGIBLE and the frozen matched control remains NOT_RUN.

The old 32 KiB pinned SDK-read Io observation remains unresolved; 16 KiB byte
checks do not prove 128 KiB support. #248 still has more-than-65,535-run/streaming/
Commit-progress objectives, #256 broader many-file scope, and deferred C1
parent-index work is in #276. Do not expand this port into those projects or
claim they were completed by current finite cases.

### 8. Required implementation handoff and stopping point

Persist through source diagnostics and fixes until the selective port is a
coherent implementation ready for evaluation. Do not stop at copied storage
files, a compile-only prototype, path-bearing hidden owners or a partial
capture/custody seam. On a real unavailable capability or new owner-format
decision, name the exact source evidence and next executable step, preserve
failures, and work independent implementation parts meanwhile.

At Phase A completion, commit/push only the owned integration branch and
update its draft PR and issue #284 with:

1. Exact final product/test/tree/image identities and per-commit production LOC.
2. Reuse/adapt/exclude manifest, resolved semantic conflicts and current file
   structure/architecture source pins.
3. Implemented invariants, exact small correctness-check commands/results,
   failures and unknown/cleanup custody, plus any required check not run.
4. Remaining limitations and the Phase B selection/dependency list, with every
   unrun campaign marked ON HOLD/NOT_RUN and existing numerical status intact.
5. The explicit outcome **implementation ready for Phase B evaluation** if
   achieved, otherwise the precise unfinished implementation/blocker.

Then stop. **Do not automatically start Phase B, mark the PR ready for merge,
merge, close #284/related issues or claim numeric/release admission.** The
owner will start evaluation as a separate assignment. The existing internal
v2 route is already approved; do not ask for the same approval again.
