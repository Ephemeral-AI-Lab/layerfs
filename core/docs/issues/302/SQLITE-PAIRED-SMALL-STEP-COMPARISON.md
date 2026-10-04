# Paired small-step namespace cause comparison

> Status: qualified cause diagnostic with explicit gaps; NOT admission and NOT all-seven success.
> Frozen source e5163f151; original Phase4.5 product7edddbdb8 unmodified; one child perarm.

**The measured extra Init work is chiefly in the file-stage owner and database acknowledgement path.** The candidate performs fewer SQL statements and VM steps, and comparable FULL/group encoding differs by less than1ms each. Its producers spend substantially longer waiting for the single save owner. That is backpressure evidence, not proof of slower hashing. Physical sync/VFS calls, preparation/reset and mapping still require direct observation before assigning the entire remainder to WAL.

This is the requested paired explanation, using a shared snapshot of real bounded namespace caller orchestration through public C1/C2/C5. It preserves scan/order, four constructors, batch/channel bounds, canonical metadata/tree work and C5 authority/seed. Engine facades capture public baseline SaveOutcome and actual candidate save/SQL observations. Service authorization/response work is omitted; observer clocks/census/record formatting are added. Root equality to BOTH latest production Init1000 receipts, identical canonical inventory and independent sampled tree/byte proof establish workload equivalence; they do not make this a replacement speed route. No product optimization was added in this diagnostic round.

## Eligibility, identities and clocks

Both produce root`a71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c`,2003distinct canonical objects/20,187,652canonicalB and inventorySHA256`b997719ad382df32b053decb160b8531093fcf8685c48ad4c058ef86ed9f9b99`. Both closed cf360c1d1 productionStores were independently inventoried read-only after proof:2003IDs/20,187,652B/the same SHA256, with main hashes unchanged (production-inventory-equivalence.json). PythonSQLite3.51.2 is used only for that metadata census; native3.51.0 supplies plans and trace. Both independent verifiers reopen separately, inventory all paths/kinds/directory metadata and authenticate the declared file sample; neither is a full-content oracle. Same fixture20,000,000logicalB, same read calls2693, same4constructors/envworker1, no source pre-read/priming. Both native helper invalidation and whole-input mincore arePASS/zero resident content; metadata residency remains unobserved. FreshStores, prepared source reuse outside timers, release/locked SHA archives and source/harness/caller/observer/fixture seals are in retained receipts.

| Clock/scope | Reference ns | Candidate ns | Difference ns |
| --- | ---: | ---: | ---: |
| operation_ns | 147393542 | 245779417 | +98385875 |
| bootstrap_ns | 6103125 | 13359583 | +7256458 |
| init_ns | 140896750 | 229324417 | +88427667 |
| checkpoint_ns | 0 | 2031542 | +2031542 |
| close_ns | 166167 | 462625 | +296458 |

Init extra88427667ns. Operation clock includes caller/observer serialization plus final checkpoint/close; it is a diagnostic lifecycle clock. Reference has no WAL checkpoint, so its0 is inapplicable, not a missing timer. Neither this ratio nor a comparison with older speed windows is a new speed gate.

| Envelope/proof | Reference ns | Candidate ns | Declared bound ns |
| --- | ---: | ---: | ---: |
| Cold+child command | 1114789167 | 1217255458 | 15000000000 |
| Separate sampled proof | 59256708 | 583055459 | 9500000000 |

Both command/proof budgets and ordering scratch cleanupPASS. No unchanged child repeated. Trace version3.51.0, hook errors0/active statements0 in botharms; baseline6connections includes separateC2/C5 and validation-memory connections, candidate1combined connection. SQLite rawPROFILE duration is separately retained and quantized; the zero-duration capabilitySELECT with6VMsteps demonstrated why it is not the fine-clock source.

## Comparable caller and codec work

All rows below are measured, but **not an additive partition**. Owner spans contain nested C2 calls. Constructor/read/send are sums across concurrent workers: read/send overlap constructor and worker sums may exceed wall. SQL trace events nest in these regions. Negative differences are retained. Individual hash/CDC and file open/stat have no separate observation.

| Same caller/work boundary | Reference ns | Candidate ns | Difference ns |
| --- | ---: | ---: | ---: |
| construct_worker_sum | 368566031 | 686697289 | +318131258 |
| file_read_worker_sum | 89529921 | 128642776 | +39112855 |
| files_owner | 100174542 | 178857917 | +78683375 |
| filesystem_tree | 1244458 | 1325209 | +80751 |
| genesis | 198583 | 700542 | +501959 |
| owner_receive | 16241705 | 19423244 | +3181539 |
| prerequisites | 67292 | 55125 | -12167 |
| producer_send_worker_sum | 247848957 | 525319422 | +277470465 |
| reserve_inodes | 178500 | 740458 | +561958 |
| save_begin | 3369291 | 2290417 | -1078874 |
| save_finish | 26502083 | 38151084 | +11649001 |
| scan | 5840833 | 6058333 | +217500 |
| C2 full_ns aggregate | 5370878 | 5927064 | +556186 |
| C2 delta_ns aggregate | 0 | 0 | +0 |
| C2 cost_ns aggregate | 0 | 0 | +0 |
| C2 eligible_ns aggregate | 0 | 0 | +0 |
| C2 acquire_ns aggregate | 0 | 0 | +0 |
| C2 group_ns aggregate | 1530614 | 1871887 | +341273 |

File owner grows78,683,375ns, while scan grows217,500ns, tree80,751ns and FULL/group grow556,186/341,273ns. Delta trials and candidate base eligibility/acquisition/cost are0 in botharms. This fixture supplies no evidence for a delta-base acquisition bottleneck. Worker construct sums rise318,131,258ns; send waits rise277,470,465ns. Do not subtract worker/read/send sums to invent hashCPU: thread scheduling and overlaps prevent that. The same read20MB/2693calls establishes unchanged logical read demand; its longer elapsed worker-read span does not prove extra storage bytes or device latency.

## Database statements, VM and cadence

VM counts below are actual sqlite3_stmt_status(VM_STEP) at PROFILE events, reset by the observer. **Native candidate SqlWork.vm_steps is unqualified after this observer and is never used as a zero-work claim.** Scope is prepared statements exposing trace events; opaque blob/VFS/internal work is not inferred. Statement lifetime starts at STMT and ends at PROFILE, including row-iteration between calls. It excludes preparation/binding before STMT and can exclude reset/finalization after PROFILE. It is not CPU time or physicalsync time.

| Qualified trace scope | Reference calls / VM | Candidate calls / VM | Reference ns | Candidate ns |
| --- | ---: | ---: | ---: | ---: |
| ALL | 3640 / 326740 | 305 / 203615 | 58820343 | 85430168 |
| begin_write | 28 / 140 | 18 / 90 | 389541 | 783539 |
| begin_read | 0 / 0 | 15 / 45 | 0 | 184376 |
| commit | 27 / 81 | 33 / 99 | 43585792 | 70006792 |
| rollback | 1 / 3 | 0 / 0 | 2750 | 0 |
| select | 2157 / 110151 | 28 / 33585 | 5176481 | 2376125 |
| insert | 1169 / 209288 | 133 / 167706 | 7598995 | 8525839 |
| update | 148 / 4899 | 26 / 1121 | 280454 | 40164 |
| pragma | 59 / 559 | 28 / 299 | 325247 | 2780125 |
| create | 44 / 1322 | 24 / 670 | 1389708 | 733208 |

Actual statements3640->305 and VM326740->203615 decrease. The old101object-row INSERT statements are a subset of its1169traced INSERTs (C2/C5/signature/schema/etc), so do not equate the two. Baseline SELECT2157 includes C2/C5/memory-validation versus candidate28combined. Scope differences in table layout are explicit, but both are measured under the same trace observer. Candidate has18write BEGINs and15read BEGINs; baseline28write BEGINs and0explicit read BEGINs. Its one rollback is the old unused private transaction cleanup path, not a product failure/retry.

Baseline C2 public outcomes report25write commits (11/4/10 per content/prerequisite/tree save); candidate native per-save snapshots report15write commits (10/1/4) plus3bootstrap/C5 write commits outside those saves, for18total. Candidate total33COMMITs includes15read transactions. **More totalCOMMIT calls does not mean more durable write acknowledgements here.** Old private publication is distinct from new reference-closed batch acknowledgement.

## Inclusive and unequal-scope regions

| Region | Reference observation | Candidate observation | Scope relationship |
| --- | ---: | ---: | --- |
| C2 FULL/group | 5,370,878 /1,530,614ns | 5,927,064 /1,871,887ns | Comparable aggregate operations; nested in admission |
| Public save begin /finish | 3,369,291 /26,502,083ns | 2,290,417 /38,151,084ns | Same API-boundary meaning; internals differ |
| Old offer /new admission | 38,762,329ns inclusive offer | 35,679,164ns inclusive admission | Unequal inner boundaries; no inferred delta |
| Old validation /collision query | 6,527,752 /6,355,655ns | not separately exported | Old validation includes collision query; new membership5,105,043ns is broader/different |
| Group seal /registration | old seal51,566,461ns | new publication154,366,166ns | Unequal operations, bothinclusive |
| Physical pack-write /database publication | old write-pack17,556,912ns | new metadata149,874,040ns | New includes locators/signatures/COMMIT, not merely bodywrite |
| Old final private publication | 106,292ns | new batches included in publicationabove | Not corresponding rows; cannot claim154ms publication regression |
| C2 SQL /transaction cadence | 24,840,626 /43,815,626ns disjoint old buckets | per-save nativeSQL147,180,871ns /nativeCOMMIT122,390,375ns inclusive | NewCOMMIT is inside nativeSQL; oldcadence includesCOMMIT/ROLLBACK/restartBEGIN |
| Global traced COMMIT lifetimes | 43,585,792ns | 70,006,792ns | Same trace boundary, includes readCOMMIT on candidate |
| Native global COMMIT wrapper | old global not exported | 129,505,084ns | Broader than trace; missing prepare/reset/mapping detail, not a sync syscall counter |
| Placement | old selector704,334ns | assembly/hash/locator-map not isolated | Unequal model; UNAVAILABLE counterpart |
| Exact reuse /pooled resolution | old93,959 /306,958ns | not separately exported | New membership/admission not equivalent substitutes |
| Pack /ordinal reserve | old not separated from SQLcadence | 1,911,792 /2,153,874ns,1/3calls | Current4calls already reduced; no paired reservation-time claim |

Candidate per-save nativeSQL records isolate the content save:142,533,290ns SQLwrapper/118,763,502ns COMMITwrapper,10write commits and88body INSERTs. Baseline content save:23,701,792ns SQLbucket/43,085,750ns cadence,11write commits and93packs. These different scopes cannot be added as extra stages or subtracted into an invented mapping/fsync remainder. The large measured candidate acknowledgement/SQLwrapper region and sender backpressure make the commit/statement-lifetime path the next investigation. Counter evidence does **not** support per-object SQL registration or extra logical read volume as the remaining dominant cause.

| Physical/canonical work | Reference | Candidate |
| --- | ---: | ---: |
| New canonical identities /reuses | 2003 /21 | 2003 /21 |
| Packs created/sealed | 97 | 92 |
| Existing private pack appends | 19 | 0 (immutable final BLOB model) |
| Submitted pack body/append bytes | 20,123,127 | 20,125,219 |
| C2 reservation calls | not separately exported | 4 (pack1/ordinal3) |
| New batch publication calls | inapplicable old private model | 11 |
| Candidate C2 pack/payload rereads | baseline chain reports2group decodes; comparable byte field unavailable | 0 /0 |

Only2092submitted bytes differ, too small to identify a large work-volume expansion. Physical grouping varies with the4-worker delivery order/engine; this diagnostic has11new batch publications versus the previous speed receipt10. Preserve that observer/scheduling difference rather than assuming identical physical grouping. A physical rewrite/syscall count is not inferred from submittedbytes.

## Native EXPLAIN and QUERY PLAN

Native helperSQLite3.51.0 matches the productruntime. Read-only mainStores, FKON, baseline exact memory-only temporary read scope; no writes execute. All9queriescompile; main hashesunchanged. BindingsNULL/representative1and512-row shapes are declared; they are plans, not workload execution. Full planner/bytecode files remain in raw plans1, compact summaries underchecks.

| SQL shape | Reference program opcodes | Candidate program opcodes | Planner fact |
| --- | ---: | ---: | --- |
| Locator1 | 42 | 24 | BothobjectPK; old also savesPK +single-row temp scope scan; new packPK |
| Locator512 | 1585 | 1567 | Sameindexed routing, no fullobject-table scan |
| Object INSERT1 | 64 | 58 | Oldtemp scope scalar subquery; newpotentialcontent_signature FKchild scan |
| Object INSERT512 | 10297 | 3654 | Old512temp-scope scalar subqueries; newVALUES/RETURNING/conflict shape |
| Current reservation update | notselected | 29 | store_policy integerPK lookup |

Program length is **not executed VMsteps**. The candidate FKchildscan appears in the plan; it is guarded in bytecode and its actual frequency/time was not collected. Do not call it a measured scan bottleneck or add an index solely from this listing. Current overall tracedVM is lower. EXPLAIN values support absence of broad locator scans and identify follow-up mechanisms; they do not quantify prepare/result mapping cost.

## Missing measurements and decision

UNAVAILABLE: individual hash versus CDC/construction CPU; file open/stat splits; statement prepare/bind/row mapping/reset/finalize splits; actual blob API/VFS/syscall bytes and sync counts/durations; disk-service/transport attribution (noMinIO/PG in thesearms); calibrated observer-only overhead and an uninstrumented same-identity control. No extra unchanged arm was run to fill these by subtraction. RawSQLitePROFILE durationquantization and nativeVMstatus interference are explicit. Lifetime wait4CPU/RSS remain lifecycle observations, not phase peaks or whole-importer memory proof.

Next promising measurement is direct preparation/step/row/reset/statement-drop and qualified VFS/sync observations on the existing acknowledgement path. That is needed to distinguish result handling, journal/checkpoint/fsync and scheduling within the broad COMMITwrapper; it is not permission to weakenWAL/FULL/fullfsync. Source/codec work now has paired evidence; further speculative encoding/read optimizations are not selected. Confidence in similar Phase4.5 speed remains unproven, and allseven gates remain required. Latest production Init1000 still1.565xFAIL; histories retain25sreference timeout and unbound final qualification.

## Evidence, reproduction and verification

Prospective contractSQLITE-PAIRED-CAUSE-CONTRACT.md, sourcee5163f151, armprospective/frozen/build/receipt/manifest plus archived executable/helper hashes. Reproduce only at a new authorized diagnostic identity via `python3 core/benchmark/fs-bench-pro/diagnostics/run_namespace_cause.py baseline <fresh-owned-output>` then candidate. Existing one-per-arm claims refuse repeats. Nativeplans invoke archivedsqlite-explain withDB/arm/sealedSQLfile after proof. Prepared source reused once; noStore/master mutation or historicalreceipt relabel.

Production telemetry verification:18coveringC2tests including successful final-drain selection/group/readback plus full100/1000namespace oraclePASS; all-targetClippy, finalobserver/exampleClippy, Corefmt,boundary439/guard23PASS. Baseline productcleanbefore/after neededreleasebuild; no third-party/manifest/dependency changes. Initial candidate lifetime/history-error conversion errors and reference generation seam mismatch retained in buildlogs; none supplies a speednumber. This round adds29productionLOC for actualruntime selection/group telemetry, not an optimization.

ProductionLOC code137733->137762(+29), reference65417/core72316->72345,active28272->28301/inactive44044; migrationold191/new7862->7891/rest64263. Exact codecommit tree/parentconfirmed. Evidence-only follow-up137762/delta0 must be confirmed by the same exactsnapshot counter beforecommit. GoalACTIVE, no push/PR/merge, no CIclaim.
