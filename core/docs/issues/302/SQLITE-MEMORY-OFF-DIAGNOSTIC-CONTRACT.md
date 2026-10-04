# Matched MEMORY/OFF implementation diagnostic

> **Status:** Research; informative and not a product contract.

Based on562391343. Ordinary Init1000 remains time FAIL at1.819920557x under
required WAL/FULL. This prospective diagnostic tests whether the current engine
still regresses during Init when mutating work uses the reference MEMORY/OFF
settings. It is not admission, a production profile option, or a replacement
speed receipt. All seven competitive selections and durability remain required.

## Intervention and honest lifecycle boundary

Production source and WAL/FULL validation are unchanged. A separately injected
first-party library intercepts only public prepare/close APIs. After the candidate
connection has passed its original validation, but before its first main-database
mutation, it performs real PRAGMA journal_mode=MEMORY and synchronous=OFF calls,
then reads back journal/sync, foreign keys, page size, cache/mmap and sync flags.
At close it reads back MEMORY/0 again. No SQL result/profile check is fabricated,
no retry/fallback, dependency patch or private source include. Actual normal SQL
arguments/results are forwarded once; intervention failure refuses the child.
The baseline only reads its already-declared MEMORY/OFF settings. It refuses a
WAL/FULL baseline rather than quietly changing that arm. Read-only main
connections receive no intervention; actual mutating main connections must be
observed. Fixed16records,512-byte bounded SQL-prefix detection, no payload owner.

The candidate's initial WAL/FULL configuration validation and transition are paid
inside measured bootstrap. Therefore the complete lifecycle is NOT a pure
MEMORY/OFF connection-open comparison. Report bootstrap, transition/readback,
Init, explicit checkpoint, close and whole-operation separately. Both engines'
actual schema/body/history mutations use MEMORY/OFF; Init compares that effective
profile. Never subtract an invented durability residual or claim lifecycle
identity that this protocol cannot provide. Existing fullfsync/checkpoint flags
and native cache sizes remain unchanged and their real values are disclosed;
sync0 means those flags do not request synchronous persistence. Product pending/
pack/transaction/channel limits and construction workers are unchanged.

SQL trace excludes the library's nested profile queries through an explicit
resolved thread-local guard, for both STMT and PROFILE. The library separately
records scalar query counts/VM/wall and synchronous assignment count. Product
trace VM remains the qualified counter; native SqlWork is unqualified after
trace resets. API step/reset and VFS counters still include real intervention/
readback operations, so their lifecycle scope is explicit. No physical fsync/
device-byte claim or sum of nested/concurrent spans.

## Work, cache, proof and budgets

One prospectively frozen paired diagnostic child per arm, same1000-file seeded
prepared fixture and canonical caller, four Init constructors/envworker1,
release/locked worktree-local build and immutable SHA archives, matched source/
harness/observer identity. Both source-content cold checks invalidate and confirm
whole-input resident pages0. Filesystem metadata residency is unobserved. No
source read/pre-touch outside the measured child. Fresh append-only outputs:
issue302-memoryoff1000-{baseline,candidate}-diagnostic1. Before each invocation
read benchmark_agent_report.md. CLI diagnostic run_namespace_cause.py ARM
FRESH_OUTPUT --memory-off enables the sealed API/VFS/profile observers only in
the driver. No production speed replay or previous receipt relabeling.

Complete cold+child command <=15s; needed release/tool builds <=30s; independent
proof complete <=9.5s. Production checkpoint/allocation release and close still
run and their actual behavior/frames are reported under this diagnostic profile.
The measured original remains append-only evidence. Candidate proof uses a fresh
independent byte copy, exact SHA equality before adaptation, then public native
PRAGMA journal_mode=WAL on that copy only for the unchanged read-only product
verifier. Copy/hash/header/verification and original-unchanged hash check all fit
one9.5s proof envelope. No application SQL/body/locator/history change in the
proof adapter; native3.51 matches product. Baseline uses its original independent
proof. Neither proof wall enters the performance comparison or warms a later arm.

Qualification requires both production-equivalent roots, same2003-ID/
20,187,652-byte canonical inventory/digest, sampled byte/tree proof, cold0,
cleanup, budgets, API/VFS/trace coverage and zero observer errors/live files.
Actual readback must show memory/0 before first mutation and at close for every
observed mutating connection. Missing coverage invalidates the diagnostic; it
never triggers fallback or another arm. Report per-file VFS calls/bytes/time,
statements/VM/transactions, COMMIT step/reset, constructor/read/send stages,
selection/group/save spans and every nonpassing/unrun line. Time ratios are
informative diagnostic arithmetic, never terminal admission.

## Capability checks and source-size boundary

Final C-Werror builds PASS. Native tiny create/insert/read42 capability probes
show baseline MEMORY/0 unchanged, candidate WAL/2 ->MEMORY/0, fixed foreign keys/
page/cache/mmap/sync flags unchanged, final memory/0, errors0/closed/live0. Final
scalar counts24/25 and candidate one sync assignment are separate observer work.
A falsely declared memory baseline refuses WAL/FULL before first mutation.
Proof-copy header adaptation PASS with measured original SHA unchanged. These are
small functional capability checks, not workload speed samples. Python compilation,
reference public-facade exact14-source generation, example Clippy, Core fmt and
diff checks PASS. Product unchanged; existing22covering production checks are
reused. Expected production LOC delta0; exact parent/staged/committed comparison
must be confirmed before the paired diagnostic. No push/PR/merge.


## Revision1 failures and prospective revision2 correction

At9150e7612 the reference child was refused with UnknownOutcome before completing
Init: the observer incorrectly enforced4KiB pages for the old reference's1KiB C5
geometry. It had first observed a temporary-scope CREATE before the C2 main
schema selected2KiB pages. No root or independent proof; reference statusFAIL.
[Reference receipt](checks/memoryoff1000-baseline-diagnostic1/receipt.json) retains
both real geometries and observer error1. No speed comparison is qualified.

The candidate child completed with memory/0 before mutation and atclose,
errors0, root/inventory matching2003IDs/20,187,652B. Init138,081,959ns,
bootstrap15,724,584ns, checkpoint11,458ns, close14,619,500ns,
operation169,696,417ns. VFS main21,262,336submitted write bytes/40,460,593ns,
WAL0, journal524B; three sync operations occur in the initial profile transition.
Independent proof failed with Uncertain: public readonly SQLite diagnosed
SQLITE_CANTOPEN14 for the converted WAL-header copy lacking WAL read sidecars.
[Candidate receipt](checks/memoryoff1000-candidate-diagnostic1/receipt.json) remains
unqualified; no promotion, admission or matched-speed claim from these timings.

Revision2 changes observer qualification, not either product. Skip initial
CREATE TEMP/TEMPORARY and the reference's explicit temporary layerfs_read_scope
INSERT/UPDATE operations; observe the first main mutation.
Read the reference's actual page geometry rather than forcing the candidate's
4KiB profile. Candidate4KiB remains required; native cache sizes and flags are
recorded unchanged. The reference's native cache is2000pages (C2 geometry2KiB,
C5 geometry1KiB) while candidate remains-2048KiB; this implementation/configuration
difference is disclosed rather than described as identical database geometry.
The shared source/channel/wave/pack/transaction limits are unchanged.

Proof-copy adapter now uses public SQLITE_FCNTL_PERSIST_WAL and a readonly
user_version query to initialize and retain the copy's empty WAL/SHM read
sidecars before close. No application table/row mutation. Fresh copy capability
passes the unchanged native sampled verifier and confirms the measured original
hash unchanged. Native capability probes pass with2KiB/1KiB reference and4KiB
candidate, final memory/0, errors0, closed/live0. The header-only predecessor's
failure remains; it is not rewritten as passing. Final C-Werror builds/Python
syntax checks pass; product checks are reused because production is unchanged.

Next pair: one newly declared child/arm at the corrected frozen observer identity,
fresh issue302-memoryoff1000-{baseline,candidate}-diagnostic2 outputs. All original
budgets/cache/fixture/workers/independent proof requirements remain, including
one9.5s copy/hash/sidecar/adapt/verification envelope. The changed observer identity
must be matched in both arms; revision1 candidate cannot supply a qualified
revision2 pair. Record all failures alongside this correction. No ordinary
production speed arm is rerun and no production gate/profile changes.
