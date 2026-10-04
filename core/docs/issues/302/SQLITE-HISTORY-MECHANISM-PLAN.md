# History17 stage mechanism comparison and bounded locator treatment

> Status: prospective count-driven diagnostic, not ordinary speed admission.

Based on7d44b04df. Keep prior history17timingFAIL/proofTIMEOUT and all Init PASS
artifacts intact. Supplementary processing objective is parity and<=19.3213359625s
from recorded matched reference17.564850875s, candidate22.938648045s. Whole-operation
<=1.10target and60/170/170performance/9.5proof/storage/correctness/cold/bounds/cleanup
remain unchanged. Acquisition is not subtracted from admission. Other historical
processing values have different identities/cache scopes and are contextual only.

Concrete product change: preserve current-demand locator hits before bounded miss
admission when projected cache occupancy exceeds512. No cache/queue/worker/byte
limit increase or additional persistent index. Reply membership uses binary search
on existing sorted miss pages. Public reader regression fills512then requests200
hits+312misses and proves exact canonical output with one312-IDlocator call,
preventing the old scalar relookups. No extra demand-set allocation is introduced.

One prospectively sealed mechanism child per arm uses actual shared full17state
producer and same release/locked/cold/helper/observer/policy, with original root
cross-check. It is labelled diagnostic, verificationSKIPPED, no admission/time
ratio promotion or unchanged gate rerun. Existing observations are reused for
whole prior gate totals; new observer adds genuinely missing per-state/stage
actualSQL/VM/step/exec/fullscan/sort/reprepare, actual preparation/reset call/wall,
VFS requested/submitted bytes/sync/close, pack-body acquisition and bounded65536-ID
first-seen pack bitmap. These are lifetime-first-seen counts, not per-stage unique
sets; repeated acquisition is not automatically redundant. Unknown IDs signalled.
No expanded BLOB SQL is materialized: pack-ID extraction applies only to two
known scalar body-SELECT shapes. Fixed observer snapshots add no product hook.

Candidate stage provider records are cumulative whole-Storage counts; reference
provider records cover only the operation reader's opens/group decodes/pooled work,
not hidden save-owner reads. Do not compare these unlike scopes as equal totals.
Both have commonSQL/VFSstage differences; nested clocks are not additive. Native
VM totals remain unqualified due trace reset; observer counters are authoritative.
Cache-hit/prepared-statement cache occupancy is not guessed from actualpreparecalls.
Representation/dependency distinctions unavailable from generic bodySELECT remain
explicit until further needed instrumentation, not inferred from1.70GB/45.56MB.

Validation: public bounded mixed-demand regressionPASS;13publication/9transaction
and selected-profile full100/1000namespace oraclePASS. Owning storage/project
all-target Clippy and final history-example ClippyPASS; fmt applied; boundary442
PASS. Facade tests initially rejected rustfmt-expanded exact seam, corrected,
3PASS. Synthetic observer capability confirms2body acquisitions/1distinct ID,
actualprepare/reset/VM counters, zero unknown IDs; not a workload warmup. Fresh
reference/candidate needed builds remain bounded30s with own targets/seals.

## Fresh labelled mechanism pair at6d148a2c1

Both17-state children DIAGNOSTIC complete with source/state-cold0, original17root
vector, fixed observer/no omissions and empty scratch. No independent proof was
run (explicitSKIPPED); no speed admission or old-row replacement. Reference
lifecycle33.618363667s/processing17.457401624s, candidate38.893189333s/
processing22.458082502s; complete commands50.720982750/54.126052250s within60.
The supplementary<=19.3213359625s processing objective is not reached. Do not
interpret cross-window changes against earlier rows as a selected speed result.

Common state/stage trace attribution:

|Stage|Reference wall s|Candidate wall s|Ref/candidate executed statements|Ref/candidate VM|Ref/candidate VFS requested read B|
|---|---:|---:|---:|---:|---:|
|Construction|1.663131124|1.620456667|878 /746|18336 /6674|10,727,151 /6,829,995|
|Filesystem|4.505912250|6.751541250|186295 /28383|6460550 /1818282|400,667,566 /24,922,581|
|Save/custody|11.288358250|14.086084585|337444 /158481|17938597 /7461146|812,164,935 /2,815,004,977|

Filesystem SQL step1.937591000s/reference versus0.422399000s/candidate while
whole stage is slower, identifying non-SQL reconstruction/validation work as a
necessary next attribution rather than assuming database latency explains it.
Save/custody body acquisitions16484/reference versus31155/candidate; VFS writes
148,494,336/reference versus218,337,280/candidate B. All observed VFS sync0.
Candidate readBEGIN228construction/9292filesystem/39679save-custody; reference
uses implicit/native read scope and therefore explicit readBEGIN0, not zero
read transactions. Actual prepare calls721/79493/28194reference versus60/106/
3426candidate by stage, with corresponding wall13.953738/264.556664/345.790881ms
versus1.307915/29.745119/145.230827ms. Reprepare0reference,34candidate in save;
fullscan0both, sorts33candidatefilesystem/1save. Actual reset calls/walls retained.

First-seen pack ID increments are lifetime observations: ref22/37/619 and
candidate21/43/524 by construction/filesystem/save; not stage-specific unique
sets. Unknown IDs0. Repeated acquisition can be required by bounded caches,
representation/dependency reconstruction and closure; no claim that all1.70GB
is redundant. Candidate provider cumulative whole-Storage and reference current
operation reader pooled counters are explicitly unequal scopes; e.greference
state17filesystem pool1184leaf requests/828chain edges/4024physical record calls,
232physical group decodes/4801value decodes/52pooled fetches2,422,410B. Do not
subtract that from candidate cumulative storage bytes as if scopes matched.

Candidate locators retain current demand and exact public regression proves the
batch-relookup defect fixed without capacity growth. This treatment alone does
not establish parity or proof-budget success. Next: attribute repeated pooled/
canonical reconstruction and save-owner read lifetimes, then concrete treatment
and appropriate checks before another changed-source gate. No ordinary unchanged
arm rerun. Init historical PASS artifacts preserved; full goal ACTIVE.
