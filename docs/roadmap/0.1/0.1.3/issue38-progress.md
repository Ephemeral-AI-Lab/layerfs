# Issue 38 four-family qualification

Status: TERMINAL PASS for the user-authorized nine-family infrastructure unification and fast baseline: 118 single performance samples and 27 selected independent proofs. See [nine-family fast baseline](nine-family-fast-baseline.md) for timings, exact sampled coverage, retained failures, and source differences. The earlier four-family Docker/host campaigns remain completed under their recorded scope. The expanded two-arm campaign was stopped at the user's request; it is not the current gate. Other families are deferred to issue #39.
Started 2026-09-05 from `46de7d42918257cd9e86075833d4f7f45af62a67` on
`codex/issue38-four-families`. Current user instructions supersede historical
runtime.sh, removed runner, fresh-Workspace and full-baseline requirements.

## Starting hypothesis and ownership (historical)

- Native owner: `layerstack.rs` task distribution; initial directory/inode
  construction is a separate hypothesis. Existing 512-file task grouping leaves
  the CAS directory and CDC variants on one producer. Predict lower producer
  tail/consumer idle time and balanced file/byte counters, with unchanged output.
- Workspace owner: candidate/continuation code and Store publication adapter.
  Shared per-inode construction can remove duplicate implementations; no creation
  speedup is inferred from that cleanup.
- Integration owner: `objects.rs`, benchmark definitions, evidence and every
  resource-sensitive command under the existing measurement lock, two CPUs/j2.
  Owned candidate consumption may remove admission copies; spill readback remains.

## Starting audit

#40 metadata cache, checked insertion, 8,191-object/<4 MiB carried batches and
Workspace sorted updates are already adopted. Native initial inode construction
already retains final nodes; canonical insertion ordering must be preserved.
Workspace candidates still complete before admission and spill beyond 8 MiB.
Pure native imports retain fresh output Stores. Workspace samples require clones.
#45 is closed for infrastructure and representative smoke coverage only; #40
namespace performance targets remain missed and are outside this campaign.

## Fixture reconciliation before final collection

Payload creation: compact-v2 low IDs still have empty genesis and the identical
N MiB prefix write + fsync operation. All four creation tiers are comparable.
Cross-file: N MiB; the tier-1 anchor is shared across three curves.
CDC: one MiB reference plus N variants, including +/-4096-byte length changes.
Workspace unique reuse: compact low tiers have different bases; retain those
smokes and add explicit fixed-128-file-base low cases for the intended scaling
experiment. The 100/500 operations and inputs must remain unchanged.

## Evidence and next measurement

Baseline image build requested from source seal
`12e9ea0d38be941cc439344eb5ad1524b61345ef0e3f681970f4a63d0c36a383`, product seal
`117e2f1fcbd7c9ef8022bf441d50e50b4a2f9c9044a3ff2b5d6062059e2fe5d8`.
The old #45 image has a different product seal and is not relabeled current.
Next: one payload 100/500 seed-1 diagnostic pair, separating Exec and Commit,
and one native offending pair; then the smallest warranted mechanism check.

## First diagnostic and repairs

The baseline image is `sha256:f988cd193ef6207386789cf71a3a835a4224a8e91ff2a486353985ee0e3b2790`.
Compact receipts are under `target/issue38-evidence/`.
`baseline/payload100-s1` failed after all 104,857,600 bytes were written. Exec
workload was 195.910417 ms; Commit failed with MissingObject after 211.939292 ms.
Cleanup passed. This is not a performance sample eligible for qualification.

Root cause: spilled candidate visitation assumed physical insertion order,
whereas selected reachability uses graph order. A reverse-order selected-spill
test failed before the repair and passes after using the existing offset index.
The repair preserves selected objects, authentication and bounded buffers.
Separately, capture selection rejected multiple contiguous spool pieces; this
discarded already-built sequential capture and repeated content work in Commit.
The narrow predicate now accepts the same exact contiguous stream as FileReader.

Supplementary host Store checks: 44 passed, one preexisting large-spill check
ignored. Includes canonical scheduling parity, failure publication, collision,
bounded admission and the new spill-order regression. These are developer
checks, not substitutes for Docker/FUSE evidence.

Fixed-base selections are `dedup-workspace-unique-{1,10}-base128-v3`: each keeps
128 existing unique 1 MiB files and adds N unique 1 MiB files with the unchanged
seed schedule and per-file fsync operation. They are explicitly excluded from
compact smoke eligibility by their actual fixture sizes. Original compact cases
remain separate; high-tier cases are unchanged. Final unique-reuse curve uses
these two low selections plus existing 100/500 cases (12 samples total).

Passed checks: source audit and Store checks above. Failing/inconclusive cases:
baseline payload100 failed; no final candidate qualification yet.
Remaining terminal gates: all 114 unique final samples; explicit actual work and
all median normalized adjacent ratios <1.10 (user amendment below); every sample <=15 product seconds;
bounded separate verification and relevant regressions; environment, setup,
resources and cleanup; final sample/statistics tables and issue acceptance updates.

Native diagnostic baseline, seed 1: unique100 initialization 0.237528541 s; unique500 1.116269208 s. Single-pair normalized growth 0.939899 (not a three-seed median qualification). Compact records retain actual bytes, candidate and transaction counters, command CPU and container peak. Native detailed producer diagnostics were not enabled in those two records; the existing debug-text emitter is now retained by the shared runner for subsequent measurements. No new product timer or operation was added.

## Candidate 1 diagnostic checkpoint

Image `layerfs-bench-infra:7518ceee9e841d64`. Payload100/seed1 passes at0.433295584s product time; exact selected verification PASS in4.040476791s. Payload500/seed1 passes runtime at3.625707043s but normalized growth1.67354 FAILS scaling. Both reuse all captured bytes. Admission rises0.201293375→2.386167751s; SQL begin/insert/commit accounts for approximately0.0733→0.5727s. This identifies candidate readback/authentication/copy as the next discriminator, not a terminal performance PASS. The baseline failed payload remains retained.

Workspace unique100/seed1 passes runtime at0.615605377s with Exec0.190417209s and Commit0.415408s. Its separate exact verifier outcome is recorded below. Native candidate diagnostics are pending. No final114-sample matrix has started.

Next admission experiment: consume owned selected pages, derive exact selected physical spool order with existing order_missing, buffered relative reads; preserve existing collision/stage/publication checks. Predict reduced read amplification and elimination of borrowed admission copies; no asymptotic claim. Workspace owner has exclusive objects.rs/workspace.rs/telemetry.rs ownership for this patch.

Workspace unique100 verification: PASS, 6.797901750s, cleanup PASS.

## Verification coverage correction

The live infra dispatcher selects fast-verify with independent_current_content and an empty covered-path map. Actual receipts read every current regular file byte for these selections (payload100104857600B; reuse100239075328B; CAS100104857600B; CDC scattered12097152B), and report zero skipped current Store bytes and zero skipped native bodies. They authenticate complete current namespace/inodes/metadata and exact selected dedup transcripts where applicable. They explicitly omit exhaustive canonical-object census (`fully_verified=false`, `full_canonical_census_performed=false`). Prior shorthand “exact selected verification” means this bounded current-state proof, not full object/storage census or unselected larger fixtures. The initial subagent claim of full-verify dispatch/full census was incorrect and is withdrawn. Final coverage must retain these explicit limitations.

Candidate2 adds owned physical-order admission and bounded sorted initial directories. Focused independent directory parity/fallback test, two spill tests, memory pointer-move test and v5 staged-publication/migration test all pass. No new baseline rerun or acceptance waiver.

## Candidate 2 results so far

Image `layerfs-bench-infra:5852be795fd0cdb4`. Payload500 product2.446243751s (Exec1.167110209, Commit1.274093125); admission1.232219375s,126 bounded transactions,525955698 selected spill canonical bytes,0 borrowed admission copy bytes. Payload100 product0.462784042s (Exec0.203737417, Commit0.254203750); seed1 normalized growth1.057183 remains above1. The complete three-seed comparison is pending; no pass is inferred from improvement.

Payload500 separate bounded verifier PASS8.940958542s, all524288000 current bytes and namespace checked canonically and via FUSE,0 skipped bytes/bodies; full object census remains explicitly omitted. Native unique500 product0.764629583s; final directory/inode phase0.000523292s versus candidate1's0.029375416s. SQL/cache variation also changed, so the entire timing delta is not attributed to sorted construction.

Candidate2 payload high pair is complete for all3seeds:100=[0.462784042,0.452368126,0.470105250]s;500=[2.446243751,2.359509918,2.554452293]s. Median normalized1.057185870 FAIL; no sample discarded. Next Exec hypothesis preserves the existing inline contiguous-spool representation on exact sequential appends rather than building100/500 tree pieces. Next admission refinement derives physical order from its bounded existing memory offset index, avoiding a redundant framing pass over spilled payload. Both preserve work/syscall/fsync and exact selected-object semantics.

## User gate amendment (side conversation, 2026-09-05)

The user authorized the main task through side task01a06eba-8b7c-7462-97c9-c6dfe1d5e731 to change the required adjacent-tier normalized median threshold from<=1.00 to strictly<1.10. This is bounded scaling overhead, not a claim of no worse than proportional growth. The coordinating message explicitly preserves original seeds,114 final samples, actual work,15s/resource/environment/clone/fresh-output/verification/cleanup gates and no deliberate slowing. The platform delivered the amendment; reading ephemeral turn history is unsupported. No original evidence or strict-gate failure is relabeled.

Under the amended criterion only, candidate2 payload high-pair1.057185870 and CASunique high-pair1.053614071 clear the median gate. CASunique seed3 ratio~1.134 remains individually above it and must remain visible. No whole family is complete yet. Candidate3 was already built with contiguous-spool preservation, bounded offset-derived ordering and explicit native prepared-source reuse; qualify its delivered paths and remaining full curves, without further speculative tuning.

## Subsequent host-owned deployment requirement (queued after frozen assessment)

The user explicitly chose host SDK/coordinator + host Workspace/Store/localSQLite with workload/daemon/FUSE in Linux, using existing ContainerBinding/ProxyHost transport. Finish the uninterrupted frozenCandidate3 campaign and assessment first. Preserve those records as Docker-owned topology evidence, not host-Store qualification. No system cache dropping/tuning or Docker/Linux memory-setting changes; no HTTP/cloudStore and no separate task/agent/worktree for the transition. Existing implementation agents completed read-only handoff; later work stays in this task/worktree.

Host setup must preserve all product optimizations, protect a compatible prepared SQLite master, and provide independent writable samples outside product timing. Closed/quiescent self-contained copying or a SQLite-supported consistent backup is required; never unsafe active-file copying or ignored sidecars. Compatibility keys must bind actualschema/fixture/seed/canonicalformat, not invalidate reusable host preparation merely for unrelatedsource/image/harness changes. Record actualcache hit/miss, preparationprovenance, snapshot/copy method and setup/product time separately, retain boundedcache/cleanup, and prove snapshotconsistency/isolation/repeatedreuse. Nativeinit still reuses sourcefixtures and measures freshoutputStore. Host and container resource scopes staydistinct; moving pagecache tohost does not establish lower totalmemory.

Candidate3 payload wholecurve now has all12 samples.100 median0.428373834s;500 median2.368766125s; their ratio is slightly above1.10 and remains a failure under the amended gate. The frozenmatrix continues. An untested candidate4 construction-order optimization was parked at /tmp/layerfs-issue38-construction-order-candidate4.patch and only that patch restored, preserving Candidate3 product source, following the instruction to complete the frozen assessment before deployment changes.

## Final frozen scaling allowance amendment

The user explicitly accepted a25% scaling-overhead allowance and requested moving on toSQLite migration after remaining assessment/checks: normalized adjacent-tier median must be strictly<1.25. This supersedes<1.10 and the interrupted1.20proposal; do not erase earlier strict/<1.10 failures. All114frozen samples and all30comparison medians clear the newgate; allotherwork/resource/runtime/verification/cleanuprequirements remain. Preserveindividualseed variability. No performance reruns or newmarginal tuning are authorized merely to remove the oldgate misses.

## Frozen phase complete; host phase active

All114 samples, all30 mediancomparisons, allwork/resource/runtime/setup/cleanupchecks and boundedrepresentativeverification passed under the final<1.25 gate. FullDocker/Linux all-feature checks:289passed,0failed,1preexistingignored across36executables; warmphase96s with2jobs/2CPUaffinity(<120), formatting andClippyPASS. Prior1-job177s gatefailure preserved. CIcheckimage removed; no owned samplecontainers remain; preparedcache8entries/1282396850data bytes. Source remains eb9b79cd87e6d655e26cad37b69327c70379e831b4d61fb4dd49838795d6fd25, product20aee20d96b09e0cd1d934b751a1e0370523987adfabc220aa63f283bb7a7d9e.

Host follow-up storage root will be project-local ignored `benchmark-results/host-store/`, with reusablefixtures, protectedpreparedmasters, disposableindependentsamples and compactresults. OrdinaryproductStores remain caller-selected and outsidebenchmarkcleanup. Preparedcache eviction is not durablebackupretention. Preserve the frozenoptimizedproduct and existingauthenticatedtransport/writecoalescing; change runtimewiring first. Prove miss→hit, snapshotconsistency/isolation/masterunchanged, cleanup/eviction and publication/FUSE/continuation/failure behavior; then same-seed100/500MiB throughputcomparison with explicit host/containerCPU/memory scopes. The25%gate is scalingonly, not a25%topology-regression allowance. No more marginalDocker tuning, no separateagents/tasks/worktrees, noHTTPor systemcache/memorytuning.

### Host-owned Store continuation — implementation and focused acceptance

The integration owner implemented this follow-up in the same task and worktree, without another agent/task/worktree. Product crates remain byte-identical to frozen product seal `20aee20d96b09e0cd1d934b751a1e0370523987adfabc220aa63f283bb7a7d9e`. Host coordinator uses `Client::connect_with_container` and the existing `ContainerManager` binding. The one authenticated daemon owner now spans both Clients across the verification Store close/reopen boundary. An initial verifier attempt dropped that owner and failed `NotReady`; its FAIL receipt is retained. The corrected bounded payload and native verifiers passed. Payload verification also proves two full-status Commits, returned/published head agreement, continued reads/writes and mode preservation in the same real FUSE Workspace, a fresh projection, and clean End. The earlier SDK live proof passed injected attach failure and disconnect cleanup.

`--topology host-store` explicitly selects host SDK/Workspace/spool/SQLite with Linux daemon/workload/FUSE. Docker has no data mounts/volumes/socket; only the authenticated daemon port is published to host loopback. The existing image/source identity is retained separately from the host binary source and binary SHA. Native imports retain fresh output Stores and reuse host source fixtures. Linux container CPU/memory caps remain 2 CPUs/2 GiB; host CPU is not capped by that container and native construction uses up to 8 existing product workers. Resource receipts keep both scopes separate.

The ignored project-local `benchmark-results/host-store/` contains evictable `prepared/` SQLite masters, `fixtures/` native sources, disposable `samples/`, and compact `results/`. Preparation completes and closes all Store handles before a master is protected. Each invocation performs an independent byte copy, fsync, SQLite quick_check, equal hashes and distinct-inode checks before product timing; sidecars cause refusal. Live SQLite copies are unsupported. Post-sample hashing proves the master remains unchanged. Cache compatibility binds the initial descriptor plan, fixture profile, seed, actual v5 DDL and a versioned canonical/fixture contract. Native qualification additionally binds case/family. Producer provenance is separate from executor provenance; unrelated image/source/harness/report changes do not invalidate a compatible master. Changing content-byte generation or canonical format beyond these descriptors requires bumping the compatibility contract. Native fixture modes remain intact. The cache is bounded to 8 entries/10 GiB with owner-checked eviction; normal user Store paths are caller-selected and never passed to benchmark cleanup. These are caches, not durable backups; gitignore does not provide backup, and durable backup retention must be managed separately.

Focused shared checks: 20 PASS, including closed-copy isolation, two independent sample identities, live WAL sidecar refusal, cache miss→hit across unrelated executor/image changes, fixture-plan invalidation, protected eviction/unowned deletion refusal, and loopback endpoint/no-mount rejection. Actual payload smoke miss→hit and native fresh-output performance/verification passed. Initial host payload 100/500 MiB seeds 1–3 passed: medians 0.408488916s and 1.899561791s, normalized ratio 0.930043248; 500 MiB ~263.22 MiB/s versus frozen Docker 2.368766125s/~211.08 MiB/s. Other family high-tier comparisons are being collected before deciding whether to extend host qualification. Frozen Docker evidence remains qualified only for its original topology.


### Final host outcome and delivered checks

Host collection expanded only after focused acceptance and all60 high-tier samples/10 comparisons passed. Final host coverage is114 unique samples: payload12, cross-file30, CDC60 and Workspace unique12. All30 normalized adjacent medians are strictly <1.25; maximum1.034081602 (Workspace unique100→500). Maximum product sample3.875961457s. Actual work, clone/fresh-output policy, all host master hashes, Docker environment, cleanup, container/no-swap/OOM and observed host process memory/swap checks passed. Every timing and corresponding-seed ratio is retained in [the host report](issue38-host-store-results.md), separately from [the frozen Docker report](issue38-candidate3-docker-results.md). Native workers are up to8 on host versus2 in Docker; host is not cgroup CPU-capped. Workspace unique500 median3.869114334s versus Docker3.680507918s is a visible5.12% topology slowdown; no topology tolerance is invented and no additional tuning was attempted.

Four final selected500-tier verifiers PASS: Workspace unique13.998164958s, CDC insert17.583759959s, CAS mixed13.656885417s, payload7.224484500s. Aggregate52.463294833s; each<59s, work allowance45s, total<600s. These verify all selected current bytes/path metadata and applicable CAS/CDC transcripts, not an exhaustive typed-object/storage census. The initial failed owner-reconnect attempt remains FAIL under its original identity.

Final Cargo1.96 formatting and strict benchmark Clippy PASS, shared checks20 PASS. A Clippy-only nested-if collapse affected the verification exchange guard, outside performance operations. Final Cargo1.85.1 release build succeeded. A final small payload performance+bounded verifier PASS covers that delivered guard and continuation proof (1.692918209s verifier). Host v5 migration/staging/publication failure-injection regression PASS (1 test). Frozen product Linux all-feature289 PASS/one preexisting ignored, warm96s suite and strict Clippy remain valid for unchanged product bytes.

The qualified host executor is source `f1eb7c6f3dd1ebd31f639e888953be523c842ad21c8d77d45b1d7c37cef42181`, binary `ab9ecba30a3cb792e9e21259b981ad61eed9b5d01f1fb0ba58fa1e311997d45a`. Its binary/metadata are retained under `target/issue38-context/host-qualified-executor/`. Delivered build source `69cf6ec212bed950f89b25ec547e6713f33e4dadc1fc19c68e9f70c534b5fa5f`, binary `c232aa143366c9870951326bf62091933505923f110b9ca2973f7905b940d573`, adds the lint-only verification guard simplification and final harness identity/output checks. Performance operations and product seal20aee are unchanged. Schema compatibility now comes from build-sealed DDL provenance instead of whichever checkout DDL is present at invocation. An actual preparation-only PASS reused the original seed3 fixed-base master across that unrelated executor change; producer identity stayedf1eb and executor identity69cf. No full matrix was redundantly replayed for those non-performance changes.

Final inventory: zero benchmark-owned containers, empty host samples directory,8 host cache entries/509,078,386 data bytes,8 Docker cache entries. Empty bootstrap test directories were removed with directory-only cleanup. Cache retention is intentional and bounded, not a durable backup. Reports, receipts and logs are preserved; no push, merge or release was performed.
