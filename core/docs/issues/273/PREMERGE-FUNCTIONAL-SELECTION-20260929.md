# #273 prospective functional completion selection

Status: prospective selection, not completion, numeric admission or merge permission.
Product source is `40f03ed7f68aacf3507c7b557959af521f437e36`; the first parent
of this test/docs revision is `30ebb4302acdf1b6608cf05b731114942339341d`.
The authenticated internal SaveFile v2 decision remains recorded in
[the approved specification](SAVEFILE-V2-PROSPECTIVE-20260929.md).

The owner's 2026-09-29 continuation instruction is to keep the work simple,
finish implementation/algorithm correctness and the 3×3 and later 8192 cases
before Phase 4.5 integration, and defer memory work to
[#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283).
Stop additional memory/cache-method work in this lane. Preserve raw timings,
all historical INELIGIBLE rows and the distinct frozen control NOT_RUN. This
functional selection does not claim a symmetric cache contract or numeric PASS.

Run the existing public SDK mounted-checkpoint harness once per candidate cell:
append, dispersed and repeated, each at 100, 512 and 4097 ordinary writes to
the unchanged prepared 10 MiB fixture. Use locked worktree-local release
driver, independent verifier and Linux daemon, existing byte-copy clone,
registered command, one construction worker and original 15/25-second bounds.
Use both independent verifiers where the frozen arm verifier supports the
schedule; otherwise preserve its explicit NOT_APPLICABLE and require the full
independent oracle. Record source/image/harness/workload/binary seals, physical
backing samples, WRITE counts, source and C1 counters, and complete cleanup.
No resampling or reclassification of old receipts.

Then run the existing separate `linux::stage_active_generic_profile_8192`
functional selection with its default 8 MiB Budget, byte oracle and checked
clean-close/refund assertions. Keep it distinct from the registered 3×3. Also
retain the #248 separated-4097 canonical regression and impacted Stage,
CommitStaged, G1/G2, mounted WRITE, failure/unknown-result and public SDK lease
routes. All native routes use the clean Linux test executable and a pinned
locked release Service. The original lowering workload remains 64 MiB with
the three specified edits, full 67,108,662-byte oracle, 8 local replacement
bytes, 64 MiB private quota, 10-second Stage and 60-second complete command.

The first new lowering attempt at source `30ebb4302` used a rebuilt unoptimized
Service and hit the 60-second complete bound. The corrected release-Service
identity passed in 31.852319 seconds; both receipts remain separate. The first
SDK image copy lost the daemon executable bit and refused Sandbox startup;
the corrected immutable image passed the real reordered/duplicated copy,
two known Commits and full old/new pinned bytes. These were build/package
corrections, not workload, deadline, quota or product changes.

The initial stopping test wrongly equated a physical WRITE refusal with
Workspace stopping. Its new-pin admission is not a product bug: read-only
custody remains available after that refusal. A second test reached genuine
stopping by moving the owned mount's parent during public SDK Unmount; it
proved Busy for new pin, held read and checked release, but incorrectly
expected recovery after terminal failed provider detach. The architecture
already specifies terminal failed-detach custody (`layerfs-fuse/src/mount.rs`).
The corrected test restores the parent, performs explicit external kernel
cleanup, asserts continued stopping/charge and reports CleanupFailed custody
through SDK deletion. It claims neither pin refund nor clean Workspace close.
Both failed observations remain append-only, with no product change.

Local-only evidence paths so far: `core/target/issue273-v2-stage-lowering-01`,
`issue273-v2-stage-lowering-release-02`, `issue273-v2-stage-headroom-01`,
`issue273-v2-sdk-origin-01`, `issue273-v2-sdk-origin-executable-02`,
`issue273-v2-sdk-stopping-01`, `issue273-v2-sdk-stopping-diagnostic-02`.
These are not published GitHub raw evidence URLs. A committed source-pinned
completion report must preserve every result and any remaining failure.
