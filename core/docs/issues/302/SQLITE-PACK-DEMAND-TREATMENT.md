# Concrete pack-allocation demand instead of whole-wave sufficiency

Status: count mechanism and covering checks PASS; matched speed pending.
Based on37eabc68f. The preceding Init1000 has93persisted packs, maxpack_id505,
next_pack_id1459 and5reserve calls (3ordinal). Wave logic replaced an unused
acknowledged range when it could not cover one hypothetical pack per wave object,
even when grouping needed only a small fraction of that count.

The deterministic reproduction first offers91valid46kB native records then1024
500B records. It emits21packs but old code reserves twice (count assertionFAIL,
pack-demand-before.log). The new code keeps the allocation block policy and
checks before each allocating operation: pending sealing uses the current open/
queued group bound; a new object adds one possible object pack and the maximum
value-group packs of an inode leaf. The acknowledged tail is reused while that
concrete bound fits. Count is at least the required bound before reservation;
no failure triggers a retry/re-reservation. This is conservative planning, not
observing an exhaustion and trying again. Finish/provider/read/closure bounds,
private delta-base first-writer acknowledgement and ownership transfer unchanged.
No new buffer/cache/worker/durability/port limit or production owner.

The reproduction passes at1actual reservation, with every canonical object read
back. A separate1024valid46kB-record pressure test captures the initial acknowledged
range after its first wave, proves the final pack count exceeds that range,
replenishes beforehand and reads every object; all pack IDs remain distinct.
Test fixture failures are retained: oversized native records were refused by
64KiB body limit; an attempted singleton lacked WholeFile grammar then exceeded
its131,071B raw limit. These were fixture mistakes, corrected with existing legal
native records; no product limit/format was changed. An edit-script assertion
also stopped before applying source and left an initial failing run unchanged.
All those logs stay under target/phase7-agent; no speed sample exists for them.

Final covering tests:8transaction/cache/pack tests (including the2new count/
pressure tests),2ordinal tests,3reference acquisition tests,9publication/profile/
atomicity tests and1full100/1000namespace oracle PASS (23unique). After the real
source change, the corrected pressure fixture alone was rerun; unchanged passing
suites were not repeated for it. All-target storage/persistence/project Clippy
PASS, followed by focused final fixture Clippy; Core fmt --all check and final
owned-test rustfmt check PASS. Production boundary439files/23self-testsPASS.
The production algorithm has not changed since those passing covering checks.

Prospective next: one matched release/locked Init1000-v2 pair at committed source/
harness identity. Same15s complete command/9.5s independent proof, cold input,
fresh database create/import/checkpoint/allocation-release/close product clock,
roots and final allocation<=reference gates. Hypothesis: use the acknowledged
pack tail longer and remove unnecessary reserve commits. Numeric speed requires
actual gate arithmetic; this count proof is not a speed result. Remaining3Init/
3history selections remain required and NOT_RUN at thisidentity. The history
reference25s timeout is unchanged; no deadline extension or unchanged-arm retry.

Architecture05-storage records the concrete bound algorithm. Production LOC
must be compared from exact parent/final staged/committed snapshots with the
existing counter and migration subtotals, then recorded in commit and handoff.

## Matched Init1000 at cf360c1d1

Reference142,588,792ns/candidate223,193,959ns,1.565298057x,timeFAIL:
10*candidate2,231,939,590 >11*reference1,568,476,712. Roots, independent sampled
proof,cold-content eligibility,cleanupPASS. FinalSQLite total23,101,440B reference/
20,553,728B candidate passes allocation. Complete227,530,125/2,452,631,917ns
fit15s, separate proofs57,726,875/43,498,041ns fit9.5s. Checkpoint1,704,792ns
inside product clock; main20,520,960B before/after allocation release unchanged.

Candidate303statements/203,509VM,32transactions/17write commits,0rollbacks.
C2reserve4(ordinal3)/publication10/93sealed bodies20,125,509B/0pack or payload
rereads. The previous ordinal treatment counted5reserve/18write commits/10pub;
the extra pack reservation is removed. GlobalCOMMIT117,264,789ns and inclusive
publication128,671,125ns/database publication124,951,291ns overlap; neither is
a physical sync count. This window's223.2ms vs prior204.5ms does not establish
controlled regression or causal time benefit; no repeat/best-of selection.
Current matched pair stillFAIL. All other3Init/3history cells NOT_RUN atidentity
and required; history25sreference timeout remains a separate open requirement.

Raw append-only packdemand1000-{baseline,candidate}-treatment1/comparison-treatment1;
compactchecks/sqlite-packdemand1000-*. Commands runner.py run --case
phase7-sqlite-init-1000-v2 --arm baseline --baseline-root target/phase7-baseline/
layerfs --out <fresh-owned-output>, then --arm candidate; report contract read
before each invocation. Release/locked artifacts, prepared input reuse, matching
source/harness/fixture/cache/observer identities, one arm each are recorded.

NEXT PREREQUISITE: paired small-step cause diagnostic before another product
optimization. Baseline public SaveOutcome/SaveProfile contain disjoint encoding/
resolve/group/place/SQL/commit buckets plus inclusive diagnosis and count fields;
the Service Init route discards outcomes. A narrow public-API canonical-producer
vehicle must capture them with counterpart candidate work observations, explicit
route/observer scope, root/inventory/readback equivalence and prospective budgets.
Use comparable work categories; old private-save publication and new acknowledged
batch publication must not be equated. Keep inclusive spans in a separate table
and mark missing producer/VFS/SQL/VM observations UNAVAILABLE. Include SQL EXPLAIN/
QUERY PLAN and phase/lifecycle/commit boundaries. This is a labelled diagnostic,
not a resample/replacement of the speed pair or terminal all-seven success.
No additional speculative optimization is selected before that report.

CodeLOC137743->137733(-10), reference65417/core72326->72316,active28282->28272/
inactive44044; migration old191/new7872->7862/rest64263. Exactcommitted tree
andparent confirmed. Evidence-only follow-up retains137733/delta0, subject to
exactfinal staged/committed counter confirmation. GoalACTIVE; no push/PR/merge.
