# SQLite statement lifetime and public step/reset cause results

> Status: qualified diagnostic findings, not speed admission; goal remains all seven selections.

**The COMMIT delay is inside sqlite3_step, with negligible explicit reset, binding, preparation, mapping and statement-cache drop in this window.** Revision2 first located102,118,960ns of102,156,536ns wrapperCOMMIT inside Rows.next(step+DONEreset). Revision3 then separates the public calls without editing the dependency or changing SQL, profiles, arguments or return values. It rules out a reset/cached-wrapper optimization as a demonstrated large opportunity. VFS writes/sync, journaling/checkpoint and scheduling within step remain unobserved, so the result is not a physicalfsync attribution.

## Frozen revisions and equivalence

Runtime observation commitcc4afeec2 adds seven fixed sequential ordinary/COMMIT wrapper phases; API observer6842b0e70 is tooling only. Both revisions run one new cold childperarm at their observer identities; older receipts stay immutable. Shared publicC1/C2/C5 caller, same stack41/seed42, same prepared1000fixture20MB, constructor4/envworker1, queue/buffer/profile settings. Baseline7edddbdb8 remains unmodified/clean. Initial Service envelope omission and instrumentation scope are inherited from the paired cause contract; neither revision is a replacement competitive arm.

Both arms/revisions match roota71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c,2003IDs/20,187,652B and inventoryb997719ad382df32b053decb160b8531093fcf8685c48ad4c058ef86ed9f9b99. These match both closed productionStorecensuses from revision1. Independent sampled tree/content proof, native cold-contentzero, budgets and scratchcleanupPASS. No input priming or historicalreceipt relabel.

## Revision2 ordinary statement/COMMIT phases

Calls preserve the original order. Library source inspection is read-only: rusqlite0.40.2 Rows.next callsstep and reset onDONE; RowsDrop resets remainingcursor; CachedStatementDrop returnsit/cachebindings. Explicitdrops remain at the previous implicit scopeends. The arrays cover ordinary querywrapper, not schema execute_batch. NativeVM is unqualified because the retained trace observer resetsstatus; tracedVM is used. Sequentialphaseclocks are inside the broader statement/transaction spans, not extra additive charges.

| Current revision2 phase | All calls | All ns | COMMIT calls | COMMIT ns |
| --- | ---: | ---: | ---: | ---: |
| prepare/cache checkout | 279 | 4211970 | 32 | 9044 |
| bind/query creation | 279 | 1210665 | 32 | 1211 |
| next:step plusDONEreset | 3352 | 124792160 | 32 | 102118960 |
| row mapping | 3073 | 507040 | 0 | 0 |
| explicit cursor drop | 279 | 36371 | 32 | 1876 |
| VM/status accounting | 279 | 9469 | 32 | 1375 |
| cached statement drop | 279 | 433171 | 32 | 15583 |

Revision2 globalstatement303/nativeCOMMIT102,156,536ns,32transactions/17writecommits. Named non-nextCOMMIT phases sum29,089ns; mapping0rows. Do not assign the remaining unpartitioned bookkeeping or trace57,301,376ns difference to a missingphase. This source evidence supports measuring step/reset directly, not switching to execute_batch based on assumption.

## Revision3 public SQLite API delegation

First-party dylib observes publicsqlite3_step/sqlite3_reset and immediately delegates exactarguments once, preserves exactreturns and uses fixed4call/nsrows. CLOCK_MONOTONIC_RAW. Apple dyld public interpose protocol documented at[dyld-interposing.h](https://raw.githubusercontent.com/apple-oss-distributions/dyld/dyld-1042.1/include/mach-o/dyld-interposing.h); no dependency/vendor/sourcepatch. Library source/clangflags/archiveSHA and DYLDenvironment are sealed. Only the diagnosticdriver receivesit; builds/coldhelper/verifier are unmodified. Exclusive-create destructor report is outside productclock. Capability probe2step/1reset is separate nonproduct evidence. TraceCOMMITcalls exactlyequal APICOMMITstepcalls in botharms; hookerrors0/active0.

| Same public API scope | Reference calls | Reference ns | Candidate calls | Candidate ns |
| --- | ---: | ---: | ---: | ---: |
| sqlite3_step all intercepted | 5856 | 69067733 | 3371 | 128622557 |
| sqlite3_reset all intercepted | 3512 | 12859487 | 299 | 8752371 |
| COMMIT sqlite3_step | 27 | 52099082 | 31 | 113223999 |
| COMMIT sqlite3_reset | 0 | 0 | 31 | 193791 |

CandidateCOMMITstep113,223,999ns vsreset193,791ns; reset is0.171%ofstep, a negligible measuredpart. Referenceuses27COMMITsteps52,099,082ns and noexplicitCOMMITreset because its control path is different. The candidate31COMMITsteps include readtransactions. Fewerwriteacknowledgements and more totalCOMMITcalls were already separated in revision1; no count is equated to physicalsync. Step observer encompasses tracecallbacks and librarywork; it does not prove allSQLite-internal symbolcalls are interposed or claim private blob/VFS coverage.

Candidate revision3 COMMITphase next113,430,833ns versusprepare6582/bind953/map0/cursordrop1792/status1957/cachedstatementdrop15790ns. Direct step/reset accounting and wrappers therefore agree on the dominantregion. Runtimecall overhead/observerpart outside clock is not manufactured bysubtracting overlappingspans. The traceinterval can end before allpublicstepwork; its narrower scope is retained, not substituted for stepwall.

## Paired clocks, SQL counts and remaining limits

| Diagnostic revision3 | Reference | Candidate |
| --- | ---: | ---: |
| shared caller Init ns | 162382125 | 208534166 |
| diagnostic operation ns | 170705833 | 229706625 |
| complete cold+child ns | 1137059500 | 840860625 |
| separate proof ns | 65184916 | 48587292 |
| trace statementevents | 3644 | 299 |
| trace executedVMsteps | 326924 | 203530 |

These are observation-revision clocks, not admission or causal speedimprovement versusolder windows. Physicalgrouping/statement cadence can differwith concurrentproducerdelivery; preserve allnumbers. Source/harness/binary/DYLD/fixture/observer identity and complete15s/proof9.5s limits are retained in eachprospective/frozen/receipt. No n3/best-of/unchanged-arm replay. Native VMfield remains explicitlyunqualifiedaftertrace; zeroisnotno-work.

Next measurement: supported delegatedVFS boundaries for xWrite/xSync and checkpoint, recordingactual methodcalls/flags/bytes/wall and underlyingVFSidentity. Preserve defaultVFSbehavior/profile/args/returns and qualifycorrectness before use. A VFSsyncmethod is not automatically onephysicalsyncsyscall; rawsyscalls/storage service can remainUNAVAILABLE. No claimthat allstepdifferenceisWAL or that weaker durability would satisfythegoal. Preparation/resultmapping/reset/cachedstatementcleanup are not supportedasdominantCOMMIToptimizationtargetsbythiswindow.

Raw append-only paired-cause1000-{baseline,candidate}-diagnostic2/diagnostic3 andcomparison2/3, compactchecks folders, sourcecontracts and archives. Reproduce onlyatafreshauthorizedobserveridentity: run_namespace_cause.py ARM FRESH_OUTPUT --api. Sampleclaims prevent repeats. SourceRoot/inputqualifiers inheritedfrompairedreport; allother3Init/3history cells remainrequired/unqualified, referencehistory25stimeoutunchanged. GoalACTIVE, no newperformancegatePASS, no push/PR/merge.

Covering18C2tests/full100/1000oracle,owningClippy/Corefmt,boundary440/guard23PASS. Runtimephasecode productionLOC137762->137829(+67);rootreference65417/core72345->72412,active28301->28368/inactive44044,old191/new7891->7958/rest64263. APItool/evidencecommit137829->137829(+0). Exactcommittedtree/parentconfirmed. Thisreportfollow-up is evidenceonly/delta0, confirmedbeforecommitwithsamecounter.
