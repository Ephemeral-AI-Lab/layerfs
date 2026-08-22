# Phase 4 G4 v12 preregistration

Status before execution: `PRE_EXECUTION_FROZEN`. V11 remains sealed `REVISE` and is neither reanalyzed nor rerun. V12 imports no v9/v10/v11 row, arm, command, child payload, or result artifact.

## Frozen source and operand custody

- Branch/HEAD: `codex/empty-worktree` / `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`.
- Candidate executable SHA-256: `e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33`.
- Benchmark source SHA-256: `01886da1d413ce73bbeba38f1b5cbc45a939e9d50e69fa7273c1af33f65554cb`.
- G3/native source SHA-256: `320ecb529c11de4464ce9a76ce97cc11f60d719d418f33a40d945e5f6dde196a`.
- Canonical-v2 source SHA-256: `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`.
- `Cargo.lock` SHA-256: `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8`.
- Frozen G3 control SHA-256: `535bfa178a8a569ea43d9f1d23808775c2349a29f9cdacddae508391a6e5e61e`.
- Frozen protected control SHA-256: `5d72b46d29a5b77494781f343cc6841a71879b5de426751afe744f27a033e8f5`.

The runner checks methodology, sources, live candidate, and controls before work. Both analyzers independently rehash the four sources. Terminal verification, while the benchmark lock remains held, rehashes all four sources plus live/measured candidate and both measured controls.

## Source repair frozen before measurement

- Reconstruction-only fold/digest/sink counters live in operation-local `ReconstructionEvidence`, not the copied shared `Metrics` hot struct.
- Rejoin segment/max-buffer evidence lives in operation-local `RejoinBufferEvidence`; shared Q totals retain checked live-allocation accounting.
- Early G3 symlink/wrong-kind rejection no longer initializes materialization-only buffer evidence.
- The fixed destination preflight uses a static C string with descriptor-relative `fstatat(..., AT_SYMLINK_NOFOLLOW)`; authority reading uses one `O_NOFOLLOW` descriptor and a fixed 32-byte buffer.
- Full rejoin authority remains predecessor through `edit_end + 1 MiB`, capped only by file length. `RangeSegments.values` uses checked requested-range capacity plus boundary slack, not file-wide reference count.
- Exact 1 MiB replacement identity, boundary reading, typed complete fallback, authenticated closure-on/off equivalence, requested-visible fsync retry, original typed first cause, and checked authority/durability counters remain frozen.
- Direct segment/Q/max-buffer evidence remains on rejoin and materialization paths. Unaffected protected paths report the static 1 MiB source bound without mutating G4-only evidence state.

The final-hash causal screen is methodology-only, not acceptance evidence. At 10 MiB/1 MiB under the same two-pair equation it observed: range `+3.427103%`, reopen `+4.741857%`, and symlink rejection `-15.904309%`, all within 5%. Its frozen artifact remains subordinate to the full campaign.

## Frozen measurement and exact protected gate

The frozen v1 logical matrix remains 30 records and 50 logical arms. Exactly 13 routes use two samples per role: `8,16,17,18,19,20,22,24,25,26,27,29,30`.

- even estimator index: `C1,P1,P2,C2` (CPPC; compatibility label ABBA);
- odd estimator index: `P1,C1,C2,P2` (PCCP; compatibility label BAAB).

The remaining 24 children are one-shot, yielding exactly 76 timed executions. The append-only chronology and `COMMANDS-v1.json` bind exact global order, sequence, role, sample, command, binary hash, start/end monotonic time, stdout/stderr hashes, and parsed external evidence.

Both analyzers independently reopen/hash every child stdout/stderr, parse every `/usr/bin/time -l` stderr into real/user/system seconds, maximum RSS, voluntary switches, and involuntary switches, rehash `command[0]`, enforce role-to-binary mapping, bind raw payloads to logical arms, and require exact global order.

For raw `phase4-g3-row-v1` payloads only, the analyzer accepts the logical adapter iff it adds exactly `status="PASS"` and `status_adapter="qualified-from-retained-g3-v1-exact-outcome-byte-mode-q-residue-invariants"`; all other fields must be identical. No other field is normalized.

The protected rule is unchanged:

`sum(candidate_ns) * 100 <= sum(control_ns) * 105`.

There are no micro-caps, outlier deletions, adaptive samples, alternative tolerances, or post-outcome issue removal. For the 13 prospectively estimated routes only, the frozen rounded one-shot adjacent decision is replaced by this exact raw-sum decision; one-shot routes 21, 23, and 28 retain the base decision.

## Private cloned preparation and bucket contract

For protected routes 8, 29, and 30, the base control preparation is completed before timing. The C2, P1, and P2 database/authority/expectations triplets are then created with same-filesystem APFS `cp -c`, each on a distinct inode, hash-equal to the source, fsynced, parent-fsynced, and bound into its child environment. Clone work is charged to preparation before the serial quartet. A focused mutation test proves one clone cannot alter the source or sibling.

Complete wall remains strictly below `120,000,000,000 ns`. The prospectively repartitioned caps, based on v11 attribution and summing to exactly 120 seconds, are:

- lock/preflight: 1s;
- private base/shared/cloned preparation: 75s;
- row dispatch/measured operations: 38s;
- exact verification: 1s;
- primary/independent analysis: 1s;
- cleanup/storage/mode audit: 1s;
- payload manifest/terminal/verification: 3s.

Completed-bucket overruns are inserted into `MEASURED-TERMINAL-v1.json`; all final overruns are inserted into `COMPLETE-WALL-v1.json` and force wrapper failure. The total ceiling is not extended.

Other gates remain: per-child RSS at most 20,971,520 bytes; measured operation sum at most 20s; exact record/arm/child counts; exact work/semantic parity; complete buffer evidence at most 1 MiB; direct M0 durability counters; seed cache class; frozen cache profile; equal primary/independent ledger; exact Q/residue/cleanup/manifests.

## Custody and cleanup model

Publication verification and benchmark lock custody retain no-follow descriptors and identity/token checks. The public lock is never unlinked; it is rewritten, fsynced, atomically renamed into a sealed success/failure attestation, parent-fsynced, and post-rename verified.

Claims are expressly limited to the benchmark-private, mode-0700, no-malicious-same-UID model. `TempName` still performs identity-check-then-`unlinkat`, and post-`fclonefileat` identity-acquisition failure returns typed unresolved cleanup with residue. The benchmark-lock rename is atomic but not an inode-conditional source rename or no-replace destination rename. Therefore neither cleanup nor lock namespace handling is claimed categorically race-free against a malicious same-UID actor.

Cleanup must name and remove only `work-v12`; residue must be zero.

## Scope boundary

`research/phase-4/g5-round-0` is concurrent premature foreign work already present in the shared tree. V12 does not edit, hash, import, or include it in G4 custody and authorizes no G5 activity. Final reporting will preserve that qualification.

Exactly one v12 full campaign is authorized after the dry-run and manifest freeze pass. No v11 rerun, unchanged-source noise attempt, or G5 work is authorized.
