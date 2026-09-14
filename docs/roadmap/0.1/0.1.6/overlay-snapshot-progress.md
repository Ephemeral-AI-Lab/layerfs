# Snapshot-Isolated Workspace: progress and resume state

Status: Dated planning checkpoint; not release evidence or a product contract.
Owner work: [#124](https://github.com/Ephemeral-AI-Lab/layerfs/issues/124)
(implementation) and [#125](https://github.com/Ephemeral-AI-Lab/layerfs/issues/125)
(final full non-#122 benchmark campaign). This file is the durable resume point
for that work; the append-only record of individual checks is
[overlay-snapshot-verification-ledger.md](overlay-snapshot-verification-ledger.md).

## Current position

**Latest owner direction: #130 is promoted before final capacity qualification
and benchmarks.** Read the
[current implementation plan](overlay-minimal-overhead-implementation-plan.md)
first. Its final objective is <=2 seconds for fresh Begin -> create/write 25,000
one-byte files -> Commit -> End, with no quadratic scaling. This supersedes the
old #130 after-closure prerequisite; the stopped checkpoints below are history.

Two subagents reviewed tiny-churn receipts and the actual write/storage path;
the coordinator reviewed contracts. The two-second objective remains unproved.
Current source has a whole-arena evacuation counterexample with quadratic
cumulative work, per-file storage amplification, and serialized host RPCs.
The selected plan fixes these in the existing ownership/backend, rather than
mandating a new pager. #124/#125/#130 remain OPEN; V1, complete production Docker
dispatch, capacity and final benchmark qualification are outstanding.

Current review/publication branch: `codex/promote-workspace-overhead`, based on
main `695c482e7aa8471710ad7e869334084de0bf2fa0`. The interrupted unverified changes
in lifecycle.rs, projection.rs, reconcile.rs and registry.rs were preserved,
including a binary patch backup before the branch switch. No product build,
benchmark, probe, cleanup or new product test was run during this plan review.
Trajectory evidence remains published on `codex/step6-capacity-trajectory`,
not merged here; use its exact commits rather than relabeling it as main evidence.

Next executable implementation task: P130.1's new public 25k fixture/route and
release cost instrumentation, alongside the bounded reclamation counterexample
and repair from P130.2. Preserve the 25k debug receipt unchanged. Check actual
process/measurement-lock ownership before running. Final full #125 collection
still waits for complete correctness and a sealed candidate, not issue closure.

### Historical stopping checkpoint

**Owner-requested stopping checkpoint: all three integration fixes have verified passes.** Product/evidence commit `f8ed6bbbb3cc86eea23b82d3efa96c70b0519d2a` and budget/handoff head `820ecab8079c7e0f09c29c6df1c983366e3dd77e` were merged through PR#126 to remote main at `74b2bdb93ee804ad8ef23877296eec1ab53976a8`; fetch and ancestry checks succeeded. This run stops after the documentation-only publication record.

Read [the successor handoff](overlay-snapshot-step1-handoff.md) first. It supersedes the older next-action entries retained below as history. Public default-path integration, V1, capacity qualification and the final benchmark campaign remain unfinished. Both issues stay OPEN.

| Item | Value |
| --- | --- |
| Phase | Components of2–5 implemented/checked; no new whole-phase completion; V1 exit stillOPEN |
| Bounded step1 | Mounted shutdown rerun PASS; actual-spill fixture PASS; maintenance oracle PASS |
| Publication checks | Build/fmt/whitespace/runner PASS; native executed tests pass in171s, historical150s gateFAIL retained; owner accepted171s and currentCIbudget is180s; old strictClippyFAIL retained, owner-approved balancedClippy passes locally |
| Active resources | No test/build process or owned container; measurement lock released |
| Phase7 | NOT_STARTED; no#122 registered scenario executed |
| Successor next task | Existing8192 persisted-token admission defect: bounded8193-file reproduction, then ownership correction; no capacity stress was run in this stopped checkpoint |

Scheduling correction (2026-09-14): the owner requested continued execution through
terminal success. The predecessor's blanket wait before Phases 2–5 is superseded
by the [successor handoff](overlay-snapshot-handoff-prompt.md). This corrects the
next action, not the recorded probe result. V2/V3/V4 contract decisions still need
implementation evidence; full public snapshot acceptance and Phase 7 remain pending.

## Frozen source and identities

| Identity | Value |
| --- | --- |
| Authoritative documents | rule, spec, spec-review, architecture, implementation-plan, this file, contract resolution, exclusion manifest — all under `docs/roadmap/0.1/0.1.6/` |
| Documentation commit | `86f6a0a0ff3de2d524b4984ab9c6b5de5272b18b` (rule/spec/review/architecture/plan/handoff + exclusion manifest) |
| Successor frozen instruction commit | `554b866838e50468b312f9d8e9ef70dc2988a38e` on `codex/snapshot-isolated-workspace`; preserves all three scoped local edits from takeover |
| Takeover comment | https://github.com/Ephemeral-AI-Lab/layerfs/issues/124#issuecomment-5655720706 |
| Reviewable implementation | https://github.com/Ephemeral-AI-Lab/layerfs/pull/126 (draft; not terminal) |
| Host operation checkpoint | `3f4a26d4a555918ea79a90bb69995946fa605abb`; #124 https://github.com/Ephemeral-AI-Lab/layerfs/issues/124#issuecomment-5656534308 ; #125 https://github.com/Ephemeral-AI-Lab/layerfs/issues/125#issuecomment-5656534522 |
| Initial product component commit | `5996231830eb05c986f7f9dcbb4b24cc52288281`; formatter repair `2728aebd3`, Store Clippy repair `6d907bc43` |
| CI status | Owner150s ceiling published in `f35e0039b`. Historical125s native suite pass + timing-step FAIL retained for run34789404326 at `7a4a920ac` under120s. Prior typed-range fixture CI failure repaired; current integration still pending. No blanket rerun or lint suppression |
| Source baseline named by the specification | `0814cc37f1dafb6041930c74489107f4a5035a26` |
| Phase-1 enumerating binary | `target/release/fs-benchmark-pro`, `LAYERFS_SOURCE_COMMIT=536aaf9ded4e09a9ccfcf2b251aa4db995b383e6` (documentation-only difference from the documentation commit), product seal `276c5970…`, compilation seal `8b63c852…` |
| Regression suite source commit used for the frozen pass ledger | `3e308a8f2` (recorded by the existing v0.1.5/#120 evidence) |
| V1 probe image | `rust:1.85.1-bookworm`; container kernel `6.12.76-linuxkit` |

## Evidence locations

| Evidence | Location |
| --- | --- |
| V1 kernel-visibility probe (source, log, hashes) | `docs/roadmap/0.1/0.1.6/evidence/v1-probe/` |
| V1 retrieval correction (new mechanism, retained-page counterexample) | `docs/roadmap/0.1/0.1.6/evidence/v1-investigation/` |
| Phase 2 focused component logs | `docs/roadmap/0.1/0.1.6/evidence/phase2-components/`; cursor attempt01 selected zero tests and is invalid; attempt02 executed one PASS |
| Owned index relocation / uncertain I/O | `evidence/phase2-components/index-relocation-manifest.json`, `index-ownership-recovery-manifest.json`; all failed attempts retained |
| Host payload ranges | `evidence/phase2-payload/`; 64 MiB -> 4 KiB -> zero physical payload, shared-source append, cross-owner failure repairs |
| Disk file ranges / canonical correspondence | `evidence/phase2-ranges/`, `evidence/v3-correspondence/`; component tests, not public Commit acceptance |
| Integrated owned Snapshot content | `evidence/phase2-components/snapshot-content-attempt01.log`: 8 targeted tests PASS; replay retained separately |
| CI failures and repairs | `evidence/ci/`; full remote logs retained with exact heads |
| Phase-1 benchmark catalog and scope ledger | `docs/roadmap/0.1/0.1.6/evidence/phase1-catalog/` |
| Local raw run directory for the same probe | `benchmark-results/v016/v1-probe/` (git-ignored raw evidence, preserved in place) |
| Local raw catalog directory | `benchmark-results/v016/phase1-catalog/` |

## Scope reminders that must not drift

- #122 is excluded at case level, never by family. Excluded rows are
  `EXCLUDED_ISSUE_122`, not PASS and not NOT_RUN. `shared/v016_matrix.py` is not
  the Phase 7 driver.
- No new numerical performance gate, capture target, or replacement benchmark
  family is introduced by this work.
- Phase 7 requires a correct sealed candidate; #125 does not wait for #124 to be
  closed.
- No release, tag, website publication, or #122 execution belongs to this work.
- Owner clarification: the solution must be **generic FUSE**. LinuxKit is only
  the observed Docker Desktop test-kernel identity, not a product dependency.
  Do not turn a hypothetical custom kernel/VM adapter into an approved solution.

## Current independent integration ownership

- `v1_mechanism`: HostClient/coherence/HostOverlay and new `host_sdk.rs`. Ordinary client/TCP ownership, SDK actor and Linux compile have recorded passes. Host SDK pre-reservation/retry composition is being wired. Generic V1 remains open.
- `publication_receipts`: `changes.rs`, `snapshot_candidate.rs`, `candidate_capacity.rs`, Store `objects.rs`/`objects/spill.rs`/`objects/scratch.rs`, narrow `overlay_budget.rs`. Candidate construction shares the existing encoder; actual scratch/index/FD/cleanup charging is being integrated. No new numerical performance gate.
- `overlay_index`: new `commit_maintenance.rs` child of root coordinator, after successful deleted-directory cleanup and prospective canonical-substitution piece-cap fix. New maintenance scheduling checks pending. Earlier component sources are stable.
- Root: `commit_attempt.rs`, `host_operations.rs` read leases, `host_runtime.rs`, local BackingServer support, lifecycle integration, CI and durable evidence.

The existing public default mount/Commit still uses legacy consumers; do not claim it has been switched or that host-only capture satisfies V1. Host runtime composition supplies acquisition explicitly so its ready construction/retry components can be verified independently. Removal/default switchover must preserve non-Commit SDK/fsync/reconciliation/End/Discard behavior. Dirty-state checks must compare monotonic live sequence with published coverage, not sequence!=0. Branch CAS and current same-branch multi-Workspace behavior must remain intact.

Current results and pending invalidations:

- Candidate and coordinator C1/C2 and lost-reply checks have passes in `evidence/phase4-candidate/`; original compile/fixture/incorrect-old-binary attempts retained. New immediate raw-snapshot release on retained stage, definite-conflict abandon and pre-reserved SDK reader paths need focused reruns.
- Canonical substitution six relevant checks PASS including real64MiB to4096B physical retention and actualC2 one replacement byte. `canonical-substitution-piece-cap-manifest.json` preserves original StorageFull failure and exact repair. Current common canonical scratch edits invalidate construction dependencies for later evidence; retain old passes for unchanged substitution eligibility algorithms only.
- SDK file-only lease and typed range fixture PASS on binary13b51d2c; new pre-reservation/activation changes need an affected reader refresh. SDK actor2, explicit detach1, shared control1, affected cancellation1 PASS; current Linux proxy build PASS in `host-client-linux-build-attempt04.json`. None is full mounted V1 acceptance.
- No final candidate is sealed. Remaining V1 prevents full-surface acceptance and Phase7; independent implementation remains executable.

## Resume instructions

1. Read [contract resolution](overlay-snapshot-contract-resolution.md) §1 for the
   open V1 decision, then §2–§4 for the selected V2/V3/V4 contracts.
2. Implement dependency-ready Phase 2–5 components immediately, while keeping V1
   open. Do not ship or claim a weaker visibility model as full snapshot support.
3. Reuse the frozen pass ledger rather than re-running unaffected checks; the
   ledger records why each retained pass is still valid.
4. Re-enumerate the benchmark catalog from the sealed candidate before any Phase 7
   execution; the recorded scope ledger is provisional by construction.

## Ordered next actions, subject to actual dependencies

1. Finish actual canonical scratch/FD/admission accounting and targeted capacity checks; compile exact pending source once all declared edits are stable.
2. Run only newly added or concretely invalidated host reader/coordinator/runtime/maintenance/SDK checks. Preserve raw failures and unaffected passes.
3. Wire new host mount/SDK/status/lifecycle consumers through the same HostOverlay and coordinator; retain generic V1 as an explicit acquisition obligation. Remove legacy Commit freeze/checkpoint coupling only with correct replacements and audited non-Commit consumers.
4. Complete supported-surface correctness and million-changed-file capacity proof, then seal the candidate and hand it to#125. No phase-completion comment without full exit evidence.
5. Reconcile full active benchmark registry/family entrypoints/modes/repetitions against exact36 #122 exclusions; freeze and execute complete included set under the measurement lock after Phase6. Publish actual matrix and repair required failures before either issue closes.

## Owner CI steering (2026-09-14)

CI budget is now150seconds. The125second run34789404326 remains a recorded historical failure against its original120second budget. Do not optimize toward120 or treat125 as a current blocker. Snapshot correctness, capacity and benchmark contracts remain unchanged. Next work is production Host FUSE/SDK and Commit integration, with exact pre-reserved SDK reader ownership and authoritative retry checks; canonical scratch admission and Linux SDK actor verification are in parallel.

## Integration checkpoint after CI150 publication

- CI150 source `f35e0039ba8f11fb97feb608b2bc179fc0e9bc04` pushed; #124 update https://github.com/Ephemeral-AI-Lab/layerfs/issues/124#issuecomment-5657194029 . Run34791110435 native step passed; overallstrictClippy stillfailed on pendingintegration, preservedraw.
- #124 first checklist item corrected back to open: frozen documents/V2-V4 do not satisfy openV1, and predecessor universalplatform/ownerwait claim was superseded. #125 reporting-only closure allowance corrected to owner's requiredterminalsuccess; allotherbody content preserved. Both remainOPEN.
- HostRuntime now composes HostOperations/CommitCoordinator/HostSdk, shared local/TCP backing and optional bounded maintenance. Its explicit capture callback does not claim that host-only root acquisition meetsV1. Public default stilllegacy pending auditedmigration; no request-error fallback.
- Actual helper/daemon mount code accepts an explicit hostauthority selectedbeforemount. Linux binaries and strictcodecPASS atL20. New ignored `host_runtime::tests::mounted_host_owner_preserves_sdk_mapping_handles_and_owned_commit_input` awaits run in an owned2CPU/2GiB/no-swap/256PID container with Store/coordinator/spool onmacOS. Container notyetcreated.
- First sharedcapacitycompile failed missing SpillFile::write_all_at adapter; second failed missingScratchFile Debug, both exactsource unchanged and rawretained. Third compile is owned bypublication_receipts; source freeze includes newHostSdk after_detach. Rootpending exactchecks: pre-reservedreaders2, coordinatorretry/conflict2, HostRuntime local/TCP1, thenmounted1. Siblings ownHostSdk4+Hostsnapshot1 andmaintenance3.
- Newread-onlycapacityconcern: defaultPayload limits.owners8192 maycount persistedtokens ratherthan transienthandles and obstructactualmillionfile proof; overlay_index willtrace#123's exactfixture thenboundedreproducer/fixaftermaintenancechecks. Do notraise caps orclaim capacity fromarithmetic.

## Active correction checkpoint

Attempt03 sharedbuild PASS with no source drift; native0869ef5d and Store89b166 exacthashes atL22/L23. Eleven root/SDK reader/attempt/runtime component checks passed; Storeprovider5passed; maintenance2passed/1oraclefailed; capacityfixturefailed because repeateddata didnotspill. Actual mounted HostRuntime testpassedallcontent/identity/C1C2assertions but overallFAIL atnormalunmountEBUSY. RootFDretainedbyHostClient wascause; v1 agentfixes prepare_shutdown + localSHUTDOWN, adds SDK path validation reproducer/fix. Productunmountfailure andcontainerabsencefollowup retained atL24. No testcontainer/process/measurementlock remainsactive.

Next build owner v1_mechanism after maintenanceoraclefix+variedcapacityfixture/sourcefreeze. Use nextbinaryforonlyfailing/newchecks; retainotherpasses. RebuildaffectedLinuxhelper forcorrectedmountedrerun. Root must connectHostRuntime after_detach onlyafterverifiedconsumer/controlretirement; normalEndfirstrecovers SDK, thenrequestshelper shutdown. Current publicdefaultmount/Commitstillawaitcohesivelegacyconsumer migration andgenericV1; nofallbackwasintroduced. Payload8193-tokenreproducer andhost-reader/canonical-concurrency accounting are stillreadyindependentwork.

## Owner stopping boundary for this execution run

Owner instruction relayed from side task `01a09c84-f7a3-7852-b8b2-be8e917ab316`: “i want the main agent to stop at finishing 1, push the changes to remote main, and create handoff note for the successor”. This supersedes this run's earlier continuation-through-closure instruction. The source task is ephemeral; the app cannot list its turns, so the relay and this exact quote are preserved here.

Step1 is specifically: (a) retained mount-root descriptor shutdown fix plus passing mounted rerun, (b) real-spill fixture correction plus focusedPASS, and(c) maintenance test/oracle correction plus focusedPASS. (b)/(c) are complete with failure retention. Finish(a), required publication checks/review, publish through normal workflow to remote main and verify its ancestry, publish successor handoff, then stop thisrun. Do not start default-path migration, furtherV1 investigation, million-file qualification or benchmarks. Keep#124/#125OPEN and#122excluded; no release/tag. Remaining knownCI/integration/capacity obligations must be explicit inhandoff.

Finalsource has been formatted and diff-whitespace checked. Source frozen while root runs native Rust1.85.1 publication build/gate under existing measurement lock, session55320. Next step after checks: rebuild only finalformatted Linuxhelper ifneeded, rerun failed mountedcheck, writehandoff andpublishall scoped source/evidence. Requiredreview ofthreefixes found no newscopedblocker; genericV1/productionmigration/resourcegaps remainopen.

## Latest direct owner CI update

“171 is acceptable, give it180s budget.” Applied to `tools/test-fast.sh` and current development guidance before main publication. Historical171s/150s gateFAIL and125s/120s gateFAIL remain unchanged. No suite rerun for a new timing; no benchmark/correctness/lint requirement changed. CurrentCIbudget180s supersedes the earlier150s instruction.

## Verified publication and stopped state

PR#126 MERGED at `74b2bdb93ee804ad8ef23877296eec1ab53976a8`. Remote main contains product `f8ed6bbbb3cc86eea23b82d3efa96c70b0519d2a` and latest owner180s budget/handoff `820ecab8079c7e0f09c29c6df1c983366e3dd77e`. Evidence comments read back: https://github.com/Ephemeral-AI-Lab/layerfs/issues/124#issuecomment-5657648675 and https://github.com/Ephemeral-AI-Lab/layerfs/issues/125#issuecomment-5657648911. Both issues remainOPEN. No owned build/test/container/measurement lock remains. Local branch remains `codex/snapshot-isolated-workspace`, fast-forwarded to the main merge before this documentation-only publication record; no source reset or unrelated change was discarded. Repository CI may run automatically on publication; strictClippy and all other successor obligations are explicit in the handoff.

## Authorized balanced Clippy follow-up

Owner approved making style/complexity/unused-code warnings advisory while keeping compilation, correctness, suspicious-code and unused_must_use failures fatal. Workflow and development guidance now agree. Replaced equivalent manual State::default with derive; removed one stale documentation comment caught by suspicious lint. Balanced workspace Clippy PASS, focused existing shutdown/initialization test PASS1, fmt/whitespace PASS. Raw initial failure and targeted passes are retained in evidence/ci/balanced-clippy/. The180s test gate and all product/benchmark correctness obligations are unchanged. No broader Snapshot work or benchmark case was executed.

## Owner constraint: third-party libraries are not to be patched (2026-09-14)

Owner instruction: "third party lib must not be patched."

Verified state at this checkpoint, so the constraint is not just a promise:

- `crates/layerfs-fuse` is a **first-party** workspace member (its own source),
  not a third-party library. It is currently unmodified: `git status` and
  `git diff HEAD` under that path are both empty.
- The third-party FUSE library is `fuser = "=0.18.0"` from crates.io. There is no
  `[patch]`/`[replace]` in any `Cargo.toml`, no `.cargo/config.toml` source
  replacement, and no `vendor/`/`upstream/` copy in the tree. The cached crate
  `~/.cargo/registry/cache/.../fuser-0.18.0.crate` hashes to
  `b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd`, exactly the
  `Cargo.lock` checksum, so Cargo builds the pristine registry crate.
- Shipped commits in this run touch only `crates/layerfs-workspace`,
  `crates/layerfs-workspace-core`, `crates/layerfs-layerstack-store` and `docs/`.

Rule for this and all successor work: a needed behavior change goes in our own
crate; if a change genuinely requires the third-party library, raise it upstream
or with the owner instead of patching, vendoring or source-replacing it.
