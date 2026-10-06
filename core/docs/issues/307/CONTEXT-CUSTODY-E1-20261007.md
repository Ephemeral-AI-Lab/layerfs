# Direct context, local custody and E1 registration checkpoint

> Status: implemented component checkpoint, awaiting local commit. S0/S7–S13 remain incomplete.
> Parent: `a41d131f262c926015798d05096ebeaa01c70cdc`. No performance sample.

This continuation adds Content direct child-context validation, wires it before SDK
Save acceptance and saved-candidate staging, returns original local serving-scope
custody, and registers E1's27 prospective evidence rows. It follows the current
[S7–S13 plan](IMPLEMENTATION-PLAN-S7-S13-20261007.md). It adds no native mount,
process-crash receipt recovery, complete topology validator or qualified engine
measurement. The two separately owned notes, root reference and four unrelated
containers remain preserved.

## Implemented product boundaries

[Content object context](../../architecture/02-objects.md) is an additive public
`FinalizedObject::validate_context`. Private canonical/ID fields already establish
identity; it re-derives and compares exact references before child demands. It
checks actual Chunk slice bounds, extent/FileState levels/fill/summaries, inode
placement/content kind/portable metadata, direct namespace child keys/count/byte
summaries and expected filesystem-root scope/serial. Demands use existing canonical
grammars and one-ID windows. Valid edited FileState representations below the
fresh-construction cutoff remain accepted. There is no whole-root scan or growing
Save graph. Actual reads, copies, repeated children and owning cache work remain
costs. Deep ordering, serial membership, reverse bindings, aliases/cycles, saved
closure and predecessor provenance remain R2/K2 work.

[SDK admission](../../architecture/22-sdk-runtime.md) validates against the original
same Save before accepting its parent. Fresh per-demand authority precedes provider
reads; pending children are visible. An authority refusal returns its exact cause
and leaves the producer intact. Actual Storage or Content failure records the
original Accept phase and prevents later Accept/Finish. Returned Storage errors
and Completion share the same Arc; no narrower provider error substitutes it.
Saved-candidate staging checks captured scope/profile/root serial and the actual
zero-reference Directory root plus directory page and portable metadata before
`HistoryCatalog::stage_changes`; serial mismatch precedes table demand. Every stage
attempt remains once-only, with original receipts. Full closure/topology remains open.

[Local custody](../../architecture/46-runtime-custody.md) adds consuming
`Sessions::fence`. Exact service-owner refusal and fixed report allocation precede
all slot effects. Success explicitly ends remaining producers and moves original
Binding/SaveId/Completion/HistoryReceipts, including vacant positions, without
cloning errors or invoking Finish/history/discard/delete/refresh. Active producer
end, known terminal, retained cleanup failure and terminal unknown are distinct;
Save knowledge and history uncertainty stay separate. Earlier acknowledged waves
survive producer Drop. A known terminal may still retain an exact stage. The
application must enforce that custody admission across later scopes. Durable Store
contents do not persist this in-memory attempt registry; actual crash recovery and
unknown-publication qualification remain open.

## E1 tooling boundary

[The E1 checkpoint](E1-REGISTRATION-20261007.md) and its prospective roadmap
register exact workload/cache/source/budget/observer/oracle/numerical requirements.
All27 proposals remain NOT_RUN with zero samples. Missing implemented drivers,
observers, numerical constants, schedules and Q05 budget refuse registration.
Source pins are checked against current bytes and inventories; this continuation
does not silently refresh them to changing product code. Single original sample/
attempt identities, sealed executable paths, scoped typed authorities/calibration,
resource timestamp/byte inventories and cohort eligibility are checked.

Retained output is explicitly numeric-only: `numeric_gate_status` and
`retained_numeric_status`, with `qualification_status=NOT_EVALUATED`. Prospective
coverage and scalar counts do not prove completed/acknowledged operations, actual
cache success or actual oracle/lifecycle results. E2 and the owning executing family
must supply those. Known numeric failures remain failures; missing evidence remains
incomplete. No E1 exit, S7/S9 acceptance or performance waiver follows from tooling.

## Checks and preserved failures

[Fresh append-only receipts](checks/context-custody-e1-20261007/) retain all original
commands, stdout/stderr, wall limits, outcomes and [source diagnoses](checks/context-custody-e1-20261007/FAILURES.md).
The [source inventory](checks/context-custody-e1-20261007/final-source.json) pins exact
changed inputs and unchanged build configuration. Functional caches are uncontrolled.

| Scope | Result and coverage |
| --- | --- |
| Host Content | All-target locked build;62 unique focused bodies pass across context/admission/identity/inode/attribute/codec/read targets.38 unchanged earlier passing bodies reused; affected13 context+11 previously unreached identity bodies verified after fixture correction |
| Host SDK | All-target locked build;53 unique bodies pass: attachment2, custody5, runtime21, root binding10, supervisor6, wire9. Unchanged attachment/custody evidence reused after affected runtime fixture correction |
| Linux ARM64 | Pinned Docker all-target Content/SDK build;62 focused Content and12 portable SDK bodies pass. macOS Store runtime/root-binding/supervisor bodies execute zero on Linux and are not claimed |
| Static scope | Host/Linux all-target warning-denying Content/SDK Clippy, core format,680-file product guard and40 tooling self-tests pass |
| E1 |43 focused registration/numeric tests and syntax compilation of6 Python files pass; no sample/build/oracle execution by the registration tool |

Every test command has an explicit60/120s wall stop; Docker also stops its child
command at110s with1s kill grace. No timeout occurred. Original E1 executable alias,
Content import, noncanonical negative-fixture keys, SDK module visibility/Timing API
and incomplete portable fixture failures remain intact. The unused fuser patch
warning is expected while FUSE is excluded; no dependency update occurred.

These checks establish component behavior only. Physical I/O, phase residency,
service fairness, complete root/oracle and mounted execution remain E2–E4/Q1/F work.
The eight incumbent Init speed and strict allocated-storage FAILs and six approved
history pairs retain their exact identities; no unchanged arm was resampled.

## Accounting and continuation

Production LOC: 161586 -> 162382 (delta +796).
Core: 96169 -> 96965 (+796); root v0.1.6 reference:
65417 ->65417 (+0). This is additive validation/ownership implementation growth;
there is no relocation, duplication or reference retirement in this checkpoint.
The [exact source comparison](checks/context-custody-e1-20261007/production-loc-source.json)
uses first parent `a41d131f262c926015798d05096ebeaa01c70cdc` and final staged
`core/crates` tree `7c156e385b45e9ec80698325ff64e446bf1871b5`. Method:
`git archive TREE crates core/crates`, then unchanged `tools/production_loc.py
--root ARCHIVE --json` (SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
Scope includes runtime SQL and excluded predecessor production; tests/docs/harness/
tooling/third-party and inline tests are excluded. The final staged root tree will
be checked against the resulting commit before the tracker update.

Next ready work is real daemon upstream/application assembly, independent physical/
service observers and K0/K1/K2 owning backed construction corrections. Native FUSE
INTERRUPT remains an exact pinned public-API gap; the authorized timestamp patch
exception is not extended. S0 and S7–S13 remain unchecked and the active goal continues.
