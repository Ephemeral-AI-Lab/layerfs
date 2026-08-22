# Phase-4 G4 materialization acceptance baseline v1

- Status: **ACCEPTED — G4 STAGE TERMINAL PASS under the user-approved 1-ms absolute-regression materiality rule; v12 remains SEALED TERMINAL REVISE**
- Date: 2026-08-22
- Stage disposition: **G4 TERMINAL PASS; closed; stop before G5**
- Branch / HEAD: `codex/empty-worktree` / `5c342f0ae24ecc69f2bfc03da1c05d1074fe956a`
- Release executable SHA-256: `e72988fc25e96f608d0d405e157ea8e837029595ace916f066932082a736db33`
- Normalized ledger SHA-256: `dc563d339401b0e7cdf84b20f1a8da20c99b5f0da849c700e86dceaa9de546b1`

This is the accepted benchmark-private G4 baseline under the controlling
[G4 stage terminal](../experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json).
V12 is a sealed 30-record / 50-arm / 76-child campaign whose primary and
independent analyses agree on `REVISE`.
Under the unchanged exact adjacent
`candidate_sum <= control_sum * 1.05` equation, sequence 17 (100-MiB
clone/no-op) regressed **+8.5353%**, sequence 20 (1-MiB count change)
**+6.7999%**, and sequence 26 (1-MiB pre-publication fault) **+14.3604%**.
Their semantic and work counters match, but all three exceed the frozen gate.
That old-gate decision remains unchanged. The controlling stage disposition
requires both a >5% ratio and at least 1.000 ms absolute regression for product
materiality. The candidate-minus-control mean deltas are only **+0.226229
ms**, **+0.285522 ms**, and **+0.099604 ms**, so none is material. Three fresh
independent read-only audits passed with no source/evidence P0/P1. This is not
a claim that the old relative-only gate passed.

The single v12 campaign completed in 91.262292709 seconds. Its passing 100-MiB
observations include 237.214083 ms warm authenticated R1, 237.381208 ms
fresh-process R1, 307.652375 ms first/full native M0, and 10.057750 ms
same-open protected-seed no-digest read. Maximum whole-child RSS is 20,578,304
bytes. The campaign-wide direct/static buffer ledger proves a maximum single
owned buffer of 1,048,576 bytes; terminal Q and residue are zero.

The remaining acceptance evidence passes: balanced two-sample adjacent
estimation and 76-child resource chronology, exact reconstruction and 1-MiB
rejoin identity, checked Q/counter arithmetic, M0 old-or-new publication with
data/metadata/directory synchronization, lost-acknowledgement reconciliation,
realistic bucket accounting with no overrun, source/operand custody, the
271-entry payload manifest, fsynced terminal verification, work-root cleanup,
and owner-bound lock release attestation. Static closure passed 166 tests with
1 intentionally ignored and 0 failed, plus formatting, clippy, release-build,
source, and methodology gates.

Cleanup claims remain limited to the frozen benchmark-private mode-0700
environment with no malicious same-UID actor. `TempName`
identity-check-then-unlink is not categorical protection against replacement
after its final check, and a post-clone identity-acquisition failure is a typed
unresolved outcome that may retain residue. This baseline makes no broader
race-free or production cleanup claim.

V6 remains historical measured numeric PASS / terminal REVISE; v7 is
historical resource REVISE; v8 is historical protected-estimator REVISE; v9
is historical measured-protocol REVISE; v10 is preserved as aborted invalid
execution; and v11 is sealed REVISE. None is promoted or reused by this
baseline. V12 alone supplies the accepted measurements, subject to the stated
user exception.

This acceptance remains benchmark-private, macOS/APFS-native, and limited to
operation-local same-open/process-lifetime custody. Cold OS/device state,
byte-level physical I/O, continuous storage peak, and controller stable-media
completion remain unavailable. G4 does not authorize production VFS/SDK
integration. This task stops before and does not authorize G5 implementation
or measurement; concurrent premature G5 planning files exist in the shared
tree as foreign work, are excluded from G4 custody, and were not edited or used
here.

Full report: [G4 report](../experiments/g4-materialization-acceptance/G4-REPORT.md).
Controlling terminal: [G4 stage terminal v1](../experiments/g4-materialization-acceptance/G4-STAGE-TERMINAL-v1.json), SHA-256 `0297ca2e3b49ddb7d8d2d435713450dcc336397b53cbaaaee9647a46eebcede8`.
Historical first decision: [0.500-ms micro-variance decision](../experiments/g4-materialization-acceptance/USER-APPROVED-MICRO-VARIANCE-DECISION-v1.md).
Sealed terminal: [MEASURED-TERMINAL-v1.json](../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/MEASURED-TERMINAL-v1.json).
Terminal verification: [MEASURED-TERMINAL-VERIFICATION-v1.json](../../../target/phase4-g4-materialization-acceptance-20260822-v12/results-v12/MEASURED-TERMINAL-VERIFICATION-v1.json).
