# fs-bench-pro first-iteration rollout ledger

> **Status:** Dated planning checkpoint; no release evidence or product contract.

Started 2026-10-10 on local `main` at `84323b47a`. The
[implementation spec](implementation-spec.md) governs this run together with
the owner's handoff and the repository measurement policies. This iteration
records function, time, storage, counts and cleanup; it makes no speed,
comparison or release-admission claim. Durable is
`NOT_RUN — disabled by owner until explicit reauthorization`.

## Stages and batches

| Stage | Status | Commits | Next step |
| --- | --- | --- | --- |
| Handoff specification | COMPLETE | `3c32fbc97` | Harvest released references |
| 0.1 Released references | COMPLETE | `8e4a78fa0` | Stage 0.2 move-only organization |
| 0.2 Move-only organization | COMPLETE, historical check failures retained | `13f9dec38`; routing `9af29e01e` | Update routing guide, then stage 0.3 catalogue |
| 0.3 Case catalogue | COMPLETE | `f182e9a63`; prerequisite correction `30b798192` | Stage 0.4 reproducible engine |
| 0.4 Reproducible engine | CLOSED: functional proof complete; exact reproduction FAILED C10/C12 | Cell-route correction `bb8e07df8`; build support `c496025fc`; oracle support `7a27a4f26`; fixture support `516c2f009`; workspace/report support `358eca082`; cleanup-observer correction `a1f2c259c` | Begin batch 1 under failed-reproduction closure recorded below |
| 1 Pilot | COMPLETE: 36 functional/proof/clean-close passes; 7 recorded scoped B PASS, 29 INELIGIBLE; full numeric qualification of the 7 is unverified | Init `c1d7dd0e8`/`39bbcbd6b`; payload `200f81493`/`c1d9b6c58`; parallel routing `b3c96ecb1`; tiny `36849bcc2`/`d6e0ef952`; namespace `98b14b233`/`be1f56a23`; H-2 fix `222526fa0`; closure `d4314ed62`; membership supplement `cca598d0b` | Batch 2, retaining original receipts plus exhaustive membership supplement |
| 2 Edits | COMPLETE: all62 cases ported/parity/sampled once/recorded;39 strict proof and clean-close passes (numeric INELIGIBLE),20 strict retention failures and3 original capped-watchdog failures retained | Canonical `a34b5d44b`/`beeb7ad69`; preserving `b9ffa63be`; capped `4d8ccfc93`; changing `05ffbf971`; small-delta `c0c99a4a3`/`2e6a508b5`/`1c975ba30` | Begin batch3; no identified fixable product correctness defect remains in batch2 |
| 3 Trees and tools | COMPLETE: all33 original cases ported/parity/sampled, full applicable proofs and clean closePASS; numeric33INELIGIBLE; first snapshotFAIL retained separately; localitydense500 and snapshot EXCEEDED | Shared3c697ae92; directory50c297c68/ad4f95503; locality2590a4536/26c037d90; snapshot0344bfdc6/fix03033abdc/results98d3be30a/70d568e94; Git acebd7cbd/guard031d427a7/results31fff4d49 | Begin batch4 after this committed closure, no identified fixable product correctness bug |
| 4 History on one branch | CLOSED WITH FAILURES: all 43 original cases ported and recorded; 31 functional/proof/clean-close passes, 11 retained actual failures, one failed preparation with actual NOT_RUN; mixed aggregate parity FAIL retained under section 9 exception; no identified fixable product correctness bug remains | H11 2b52bdf17; H12 ea7148a19; H13 8ae040cdd; H14 e09c61bbd; product H15 a05ed6027; H16 9dfb70ee4; fresh seal3c67e9249; affected Init8f24e39a4; boundary88a2bd27b; partial bd3016957; mixed1a21ebfc3; dedup closure27708c11d | Begin batch 5 after this committed failed closure; verify existing public SDK Fork and History |
| 5 Branches and Workspaces | CLOSED WITH FAILURES: all17 original cases ported/recorded; eight functional/proof/clean-close passes, eight actual concurrent refusal failures, one failed-preparation actual NOT_RUN; branch aggregate parity FAIL retained under section9 | SDK proof711593c5f; H21/22 4a40fb690; setup exceptionc23ac0cfd; H23 68209bdb7; H24 669bc4853; H25 2f7160c92; H26 616947cb6; branch305bee680; multicb3a003ac; historicalb0b2cd386 | Begin batch6 under this truthful failed closure, preserving all original failures/limits/proofs/resources |
| 6 Dedup and storage | CLOSED WITH FAILURES: all54 original cases ported/recorded;45 functional/proof/clean-close passes (44 numericINELIGIBLE, one verification-onlyPASS),8 failed outcomes, one failed-preparation actualNOT_RUN; repository aggregateparityFAIL undersection9; no identified fixable product correctness cause remains | definitions77eb/63ff/a442/5505/55a3; support86ba/149d; H28 1cc; H30/31 082; H32 0a8; H33 fe205; footprint8b684a3b1; reuse4d725864f; crossad21161eb; CDC79ad08b2a; H34 08e1; H35 fad966f7e; repositoryoldf8752231d / this closure | Begin batch7 at this committed truthful failure closure; preserve all limits, failed/ineligible/unrun rows and exact resources |
| 7 Reliability | CLOSED AS NOT_APPLICABLE: all28 original cases source-reviewed/recorded;26 exactphysical-accounting gaps andtwo from-creationSDKprofile gaps; zero samples/fullcaseproofs, no missing-capability feature added | b1cc/dda917/f572/f707/a022/f560; spec04e1142f0; 093fe2068 | Iteration1 completion recorded; no further case work selected |

## Decisions
| Number | What | Why | Alternative |
| --- | --- | --- | --- |
| 1 | Commit the supplied spec and README addition unchanged before implementation; include this initial ledger | Explicit handoff; the ledger accompanies the work it describes | Editing the proposal before preserving its handed-over identity |
| 2 | Treat timing limits as reported soft limits for this iteration; retain independent watchdogs and verifier limits | Owner explicitly says slow completed cases are not failures; a watchdog expiry is still a hang/failure and is never enlarged after a miss | Judging latency or removing a watchdog |
| 3 | Preserve cache eligibility separately from functional observations; unknown preconditions yield INELIGIBLE | Measurement rules remain binding; functional completion cannot invent cold eligibility | Claiming PASS for unknown cache state |
| 4 | Create no additional family under P-6 and make no GitHub changes under P-7 | Explicit handoff scope supersedes proposal recommendations | Adding per-call families or closing issues |
| 5 | Use `Co-Authored-By: Codex <noreply@openai.com>` for this session's commits | This is a Codex session; no other attribution line was supplied in its instructions | Copying the prior agent's attribution |
| 6 | Correct section 8 to state the verification source has 227 total rows (28 reliability); preserve original seconds beside labelled decimal-to-ns conversion | Source CSV and its generator agree on 227; the handoff requires every row | Harvesting only the 28 reliability rows or inventing precision |
| 7 | Specify supporting-file destinations and permit path-only root/manifest/asset adjustments; retain historical source-gated tests | Section 4 omitted `r7-tools`, shared files and the old runner; depth changes require more than import text; old tests name retired product drivers | Inventing silent destinations or restoring retired product source |
| 8 | Correct P-1 and reuse existing public ProjectApi fork/history; record the ForkSource::Commit route for P-2 | Source at handed-over product tree already exports both methods and validates ancestor membership; duplication would add no capability | Adding duplicate SDK methods or bypassing the SDK with wire requests |
| 9 | Catalogue all 273 original IDs as pending; leave execution fields explicitly unset until each family is ported | The root contains 269 Rust registrations plus three repository-history profiles and one small-file-delta case; release rows are not the complete registry | Dropping unreleased/proof/extended cases or inventing runnable definitions |
| 10 | Resolve the original Rust registry using a std-only metadata exporter; read both Python wrapper registries separately | Root SDK cannot build after retirement, while its product-neutral workload definitions compile without product dependencies | Restoring retired crates or guessing formatted IDs from text |
| 11 | Route stage 0.4 through `bench.py run cluster-two/cells` and put its generated CSVs in cluster-two baselines | The spec requires the twelve existing cells through bench.py but only named family execution; separate subjects must remain separate | Treating cells as a 26th workload family or adding a second entry point |
| 12 | A source identity reports the protected untracked handoff but excludes it from compilation; refuse all other untracked source and tracked dirt | The owner requires that prompt remain untracked; it supplies no product, harness or workload bytes | Staging it or pretending an arbitrary untracked executable is clean |
| 13 | Preserve OWNER-3 checker failures as FAIL records and admit a build only when the exact two known passthrough refusals are the entire error set | Owner explicitly retains those refusals; daemon builds use the authorized product patch, while any new provenance error must stop | Weakening the checker or claiming its aggregate PASS |
| 14 | Reuse the intact cell tree/index checkers and the recovered version-pinned Git policy; collect actual image Git/config witnesses anew | Preserves the original full-tree and index exclusions/checks; copied policy plus provenance removes its untracked data dependency | Guessing defaults, comparing raw native/FUSE index stat caches, or weakening tree checks |
| 15 | Rebuild empty and full-written 64 MiB-zero masters through the existing SDK provision mode; retain prepared masters during the rollout and remove only passing sample resources | Same initial acquisition/install boundary as the working path; setup masters are needed for independent byte-copy reuse | Mounting a master for measurement or depending on old /tmp trees/master volumes |
| 16 | Freeze the twelve original B:L cells, enforce class B using actual demand counters and original terminal cleanup, generate all twelve result rows | Stage 0.4 requires exact count reproduction through bench.py; unavailable evidence stays unavailable and soft limits remain separate | Sampling pending workload families before the engine proof or filling absent measurements with zeros |
| 17 | Close stage 0.4 exact reproduction as FAILED for C10/C12, preserve strict checker/results, amend only the stage closure in its own spec commit and continue | All twelve functional proofs/terminal unmounts pass at clean identities with identical daemon/product; kernel batching and conditional reclamation probes prevent a semantic equality claim; handoff explicitly permits source-backed spec correction | Resampling unchanged cases, injecting requests/probes, altering workloads or pretending exact equality passed |
| 18 | Port the actual frozen Init-only infra route and original deterministic bytes/sample set; use current public Init and existing public read APIs | `src/infra.rs` selects namespace_init_diagnostic, not the older auxiliary edit/Commit function; sampled prefixes/metadata/reopen root match the selected verifier | Timing a different historical function or omitting undeclared auxiliary mutations |
| 19 | Init preparation creates only native inputs; every measured Init gets a fresh Store. Label setup warmth and unavailable host cold metadata as INELIGIBLE | Initial full acquisition must remain measured; preparing/cloning an already initialized Store would remove its product work; unknown coldness cannot receive PASS | Cloning a Store for an initialization case or claiming coldness from file generation |
| 20 | Reuse original product-neutral Rust planner/stream/digest definitions and compile native-only fixture/verifier tools | Guarantees original names, modes, mtime, count/byte allocation and exact content digest without restoring retired product code | Guessing fixtures or building the retired SDK |
| 21 | Reuse unchanged original product-neutral syscall program for payload work; fully check fresh mounted and persisted current-format roots | Exact generated bytes/read offsets/normalization/fsync requests remain in the frozen program; current public readers validate current representation and complete declared semantic tree outside timers | Transcribing random offsets by guess, weakening byte checks or interpreting retired encodings as current |
| 22 | Export only the new stopped sample Store for an independent offline oracle; current daemon remains the complete live data path | Closed export permits independent current Content/Storage/History reads without a host/server serving route or second constructor; root is tied to the original typed Commit reply | Reading a live WAL inconsistently or making the host a runtime data adapter |
| 23 | Copy generic original verification 45 s work / 59 s hard limits, preserve 15 s reported product limit and original 120 s operation watchdog | The frozen selected-verification policy declares these values; separate timers/topology are recorded as differences before any payload sample | Widening a limit after a miss or shrinking payload tiers |
| 24 | Owner-authorized subagents use separate new worktrees for tiny_file_churn and namespace_mutation while the lead samples payload_create_read; all remain in batch 1 | Latest owner instruction explicitly requests concurrent families in worktrees and supersedes the earlier no-extra-worktree restriction; independent worktree locks/targets retain local serialization | Serial family authoring or starting a later batch |
| 25 | Integrate worker changes locally onto main; keep engine, product fixes and the master ledger under the lead; record shared Docker VM interference in each source receipt through LAYERFS_BENCH_INTERFERENCE | Isolated family edits avoid conflicts; parallel samples share host/VM resources and cannot claim isolated timing; original workloads/cache/limits remain fixed | Multiple owners editing the engine, hiding interference, or pushing worker branches |
| 26 | Correct port_review.source_commit to the peeled reference commit and preserve the annotated tag object as source_tag_object; require both exact objects in parity | The annotated recovery tag is ae3310f..., while its source commit is d296981...; reviewed source bytes remain identical and old receipts remain valid at their actual source | Keeping a misleading commit label, deleting the tag pin, or replaying unchanged samples |
| 27 | Reuse a validated closed sibling master for an exactly matching complete initial tree, product, UID/GID and Disposable profile; keep expected/helper trees case-specific and sample byte clones independent | Setup reuse policy requires compatible sealed inputs; the changing family has six unique initial sizes despite 32 cases | Duplicate Init/install for the same input, reusing a mutated sample, or claiming coldness |
| 28 | Canonical preparation references the preserving worker's four immutable closed masters through explicit prepared-parent links, then validates the complete original seals and exact semantic fixture digest | Identical initial trees and unchanged product make those setup masters compatible; all 12 canonical expectations/helpers remain new and sample byte copies independent | Duplicate Init/install or borrowing a previously measured sample |
| 29 | Distinguish the declared B/C daemon/mount predicate from full per-phase OS/backing/metadata/SQLite qualification; preserve old scoped labels and fail closed for future unknown inputs | Owner forbids setup or earlier-phase OS warmth; zero canonical object demand does not prove the still-paid history read's input cache state | Treating a post-unmount mincore observation or zero demand as full coldness, editing old verdicts, or changing cache treatment |
| 30 | Prove all retained small-delta roots through public offline current-format history/filesystem reads; declare omitted historical kernel Fork/Mount topology | Direct mounting takes a Branch and the existing batch-5 Fork route has not yet been qualified; final native remount/cleanup remain required | Manufacturing a feature or claiming identical proof topology |
| 31 | Preserve original payload DELTA existence and thirty-second per-state proof stops | The frozen selected contract requires actual DELTA reconstruction and bounds each state, independently of numeric storage/latency targets | Treating reported DELTA counts as optional or replacing per-state stops with only a phase stop |
| 32 | Reuse exact unchanged compiled support with original source/provenance receipts when only host control/oracle support changes | Unaffected evidence may be reused when actual compiled inputs and binary hashes remain identical | Rebuilding unaffected binaries or reusing a measured sample |
| 33 | Parallelize only the four current batch3 families after committed batch2 closure; lead owns shared controller and local snapshot, workers own directory/Git/locality | Owner explicitly authorized separate worktrees; previous batch done-when conditions are now closed | Starting later batches or editing one shared checkout concurrently |
| 34 | Clean locality case skips warm-up and measured Exec; scan/construction proofs validate recorded original semantic operation counts | Frozen clean case is owner-only; final tree equality alone cannot prove a read-only traversal visited every required path | Running a no-op Exec or replaying the workload to reconstruct its counters |
| 35 | Git's prior untimed mount runs the original verification variant before End without Commit; the fresh measured call retains the exact performance variant | Preserves actual mounted intermediate porcelain/diff/temp-save guards without adding timed filesystem reads or replaying a measured call | Claiming native-only intermediate coverage or modifying the timed workload |
| 36 | Reuse original native fixture/C1 generator for local snapshot, derive C2/C3 with frozen97*j/byte equations and verify every state with unchanged original native checker | Avoids changing compiled recipe support solely to select an already defined expected step; runtime also compares each manifest with original Rust expected(case,step) | Guessing state bytes or compiling another fixture producer |
| 37 | Retain the failed full-size debug proof and qualify the benchmark's prescribed release compilation at identical fixture/oracle/30s stop | Debug child reached its stop during the first25k state, without an error or completed root; engine builds the host runtime in release; no source speed change is selected | Enlarging the stop, shrinking the test or optimizing product source |
| 38 | Preserve Git's frozen41s work cutoff,45s cleanup deadline and59s publication deadline with0.25s guard, cumulatively charging prior qualification/custody/final proof | Original selected verifier reserves4s within45s, not an added allowance; public current offline graph requires early Stop/export, also conservatively charged before graph work | Dropping reserve, omitting receipt hashing/publication or granting each verification subphase a fresh45s |
| 39 | Publish original receipt/manifest once and require bound completion; revoke only an owned provisional completion link after an observed overrun, preserving its exact attempt bytes | A lost late-failure sidecar must not let a provisional proof qualify; original immutable receipt and all metrics remain available | Overwriting original receipts, accepting missing completion or omitting a late final write from the guard |
| 40 | Add an unfiltered read-only current SQLite Commit census of a custody-qualified closed export, alongside mandatory public typed history/content proof | Original dedup history queries the complete global set and parents; anchored ancestry cannot detect an extra unreachable Commit | Adding a product enumeration feature, counting only rows, filtering by stack/ancestry or silently omitting the original census |
| 41 | M1 uses independent shadow/range algebra and only original explicit attributes, helper-created directory modes and declared byte ranges; two exhaustive IDs are proof-only | Frozen generic expected() returns pristine fixture for M1; actual original exhaustive scope checks20 cycle-end roots plus all100 Commit IDs/parents, not all100 stage payloads | Using a wrong generic manifest, inventing timed performance or normalizing automatic mtimes to fit an oracle |
| 42 | Preserve strict boundary-alias parent-directory mtime compatibility failure without restoring the parent in the workload | Frozen ordinary helper changes the parent during its final atomic save, while original executable verifier requires fixture mtime; changing the helper/checker is forbidden | Normalizing parent mtime, dropping metadata verification or making a product change for an incompatible original definition |
| 43 | Retain owned sample resources through finalization and outer acknowledgement; remove only afterwards with separate bound administrative custody | Late output-close/publication failure must retain its diagnostic container/volume; partial Docker deletion must never claim both resources retained | Deleting before final verdict or overwriting the original receipt after deletion |
| 44 | Recover failed agent service compaction in the same worktree with a fresh context and reuse exact completed qualifications | Two service stream failures left source and receipts intact; source-hash-matched final6tests had already passed | Replaying tests/exporter or discarding unfinished source |
| 45 | Seal final stable native/Linux assets once, then rebuild only the changed host oracle with qualified eight-binary reuse | Pinned Linux/CPP scopes are final and unchanged by subsequent runtime-only proof work | Duplicating unchanged Linux/native builds or using an unqualified stale binary |
| 46 | Close original500MB exhaustive preparation as FAIL and its caseNOT_RUN; preserve failed parity/batch closure while permitting other originalcases and laterbatches | One unchanged120s preparation expired after15of20 native expectations; a sealed fixture cannot be invented, another attempt/limit increase/scope reduction is forbidden, and no product correctness defect was identified | Retrying setup, shrinking original proof scope, pretending whole-family parityPASS or indefinitely blocking runnable originalcases |
| 47 | Keep separate authenticated runs and CSV pairs when a shared harness fix interrupts a family; select unaffected earlier results by their actual source in the final family report | Each run binds one source, and completed unaffected cases must not be repeated or relabelled | Copying earlier receipts into a new source run, inventing a common measured identity, or repeating all earlier cases |
| 48 | Correct Disposable startup by moving the existing selected synchronization calls before any SQLite initialization, preserving every statement/profile check | Actual public-API VFS evidence found24 forbidden sync calls; this is the owner's required profile, not a speed target | Leaving the violation, suppressing observer callbacks, skipping initialization checks, changing profile or optimizing unrelated work |
| 49 | Preserve strict M1 atomic-save metadata failures without changing the frozen helper, oracle or correct product write/rename semantics | Original helper sets mtime before the write; original shadow retains that old mtime, and retained metadata plus a native exact-order diagnostic confirm the inconsistency | Resetting timestamps after writes, weakening the assertion or changing product POSIX behavior to force PASS |
| 50 | Bind an external watchdog failure in a separate administrative closure when the original driver cannot publish a complete product receipt | The outer 120 s stop interrupted the attempt after actual Commits; no terminal, complete timing, final count/storage or publication witness can be manufactured | Inventing a complete receipt, calling the attempted case NOT_RUN, or retrying the unchanged schedule |

| 51 | Correct P-2: the original historical-access driver already forks a separate selected-Commit Branch before mounting | Frozen v016_access.rs344–350/396–403 directly show that route; current ForkSource::Commit preserves the same topology and zero-copy ancestry | Calling the existing access Branch an extra new operation or adding a direct Commit-bind feature |
| 52 | Build historical access from new setup-only full original producers; reuse immutable genesis inputs, never measured samples | Original access consumes a complete closed producer; sample0 and explicit setup custody keep phases distinct | Reusing a mutated measured Store or initializing only the selected ancestor |
| 53 | Use explicit anchored32-record public SDK History pages and authenticated continuation artifacts | Existing native bridge HISTORY_WINDOW is32, while provider Page allows128; pagination expresses every original ancestry without a product feature or refusal/retry | Sending128, raising a product cap, truncating ancestry or classifying an expressible case not-applicable |

| 54 | Explicitly reuse each new, complete closed setup producer for its paired historical access case, retaining original identity and immutable receipts | Six access selections use four complete original producer graphs; exact compiled/controller/model/workload/fixture admission avoids replaying setup and keeps each access independent | Running boundary100 and inode100 twice, reusing measured Stores, or forging the current source on an earlier producer |

| 55 | Retain the actual concurrent Stage1 EAGAIN and Commit Begin Busy workload failures without a product contention fix | Current integration3.3 explicitly requires serial/save refusal before effect, zero busy_timeout and no retries or writer gate; unique EAGAIN mapping identifies a contended creating mutation | Adding a retry/gate/serialization feature or weakening the original unconditional-success workload |
| 56 | Install historical backing as daemon-owned root0:0, directory0700 and file0600; keep Workspace UID/GID501:20 unchanged | Daemon private-directory/file guards require its own effective identity; source UID is filesystem fixture ownership, not backing authority | Weakening the guard, running daemon as fixture UID, or relabeling bad masters as repaired |

| 57 | Serialize historical access topology in original declared role/ACK order and validate exact planned Branch membership | Generic JSON sealing sorts object keys, while the complete typed acknowledgement checker deliberately preserves original sequence | Weakening the checker to sets, dropping duplicate canonical tuples, or editing the failed transcript |
| 58 | Retain expressible original fresh import verifier Fork/FUSE/full native bytes/End/Gone/Stop alongside typed current storage proof | Frozen oracle topology is expressible and must not be weakened; worker offline-only proposal corrected before sampling | Offline-only replacement |
| 59 | Prepare fresh native inputs without any Store for the directory-import and Store-footprint cases | Their Init is measured original work; a prepared Store would remove it | Use workspace master provisioning before the timed Init |
| 60 | Use separately hash-bound pristine Store copies for reuse attribution, no warmup workload, and cumulative original independent proof deadlines | Frozen reuse observes initial actual payload identities before one fresh mount; extra structural reads are setup/proof, never cold evidence | Generic ordinary warmup or inferred pristine payload identity |
| 61 | Preserve repository all-selected ACKs and a separate Created-only census transcript, with actual Fork-bound branch-only manifest derivation | UpToDate repeats must not become invented Commits, and unfiltered global SQL census stays strict; current Init adds an unused default main Branch | Drop repeats, ancestry-filter global census or silently alter manifest authority |

| 62 | Report unavailable legacy per-Workspace physical spool/live/superseded/peak and exclusive file axes explicitly; authenticate actual namespace cleanup separately | Current product exposes namespaced logical accounting in one permanent shared SQLite file; exported-copy allocation/startup high-water cannot prove the frozen physical equation | Guess physical zero from Gone, treat shared allocation as private spool, or add product observability features |
| 63 | Preserve legacy seven-key administrative removal identities; authenticate declared extra containers and exact partial custody separately | Repository proof uses two owned runtimes on one sample volume; incomplete removal must not rewrite original case verdicts or imply both resources retained | Always append absent optional fields or delete volume after uncertain extra-container removal |
| 64 | Observe actual owned host PID sampled RSS and expanded reaped-child swap/CPU/IO scope, with unavailable final values until known wait | Original import/repository RSS and zero-swap guards remain, macOS exposes child-aggregate usage rather than an exact native-specific counter | Use lifetime peaks, inferred zero, descendant-free claims or absent after values as PASS |

| 65 | Keep the actual sealed public Init result alive across an interactive installation boundary, using the existing public install API | A host Init manifest intentionally lacks daemon_sqlite; byte copying cannot qualify installation. Init ACK ends its paid phase before original verification-only installation | Fabricate daemon metadata, reopen Init or weaken startup installation guard |
| 66 | Add original family subdirectory for newly recorded parallel runs sharing one source commit | Explicit owner parallel authorization allows several independently registered families at the same clean source; section6.1 fixed filenames otherwise collide | Overwrite reports, invent a source identity, or alter closed run paths |
| 67 | Consume the original cache helper exit3 as a declared ClassA INELIGIBLE observation, then finish the unchanged full verifier and terminal shutdown | The helper intentionally returns3 for resident content pages; numeric ineligibility cannot skip functional proof/Stop. Every other observer exit or mismatched declaration stays a failure | Treat cache ineligibility as a product crash or weaken cold eligibility |
| 68 | Preserve authenticated ClassA unavailable cache observation while completing the full functional proof | Spec sections5/6.1 require unknown cache state INELIGIBLE, not invented coldness or skipped verifier; checker refusal and exact reason stay visible | Retry observer, wait for warmth/identity to fit, change cache phase, or claim cache PASS |
| 69 | Retain original repository staging15s setup ceiling and failed-copy custody, then run the distinct smaller profile unchanged | Staging is separate harness setup before measured state operations; changing its timeout after the miss would fit a failure. Original300s per-state operation declaration and owner120s whole-test stop remain distinct | Enlarge setup timeout, remove checkpoint assets, replay stride3 or infer completion from partial copy |
| 70 | Extend the existing failed-preparation closure to repository deepseek-full in a spec-only commit | The unchanged120s preparation expired before sealed input existed; no honest actual sample can run. General closure already applies the same rule to retained batch4/5 failures | Retry/widen preparation, sample partial fixture or claim passing done-when |
| 71 | Report the authenticated failed-preparation administrative record as actual caseNOT_RUN, preserving original preparationFAIL | No actual case or proof operation occurred; section6.1 status must describe the case rather than its preparation. Strict zeroattempt/source/schema/metrics guards prevent masking attempted failures | Treat preparation as an actual sampleFAIL or discard its failed receipt |
| 72 | Classify26 complete reliability cases not-applicable while retaining their original schedules and guards; port only the two original early integrity paths | Originalrun:413/live:175 unconditionally requires current and mutation-event peak physical per-Workspace allocation Some/errors0/both≤2GiB; current shared-overlay public accounting lacks these axes. Integrity paths360–385 bypass that call in the original source | Add observability features, substitute logical/shared/sampled counters, or claim partial primitive proof as full-casePASS |
| 73 | Reuse no whole reliability-case proof; source-review all28 and independently qualify the two expressible current-format cases | Exact closed receipt review found0 scope-identical whole current-case proofs; earlier150round receipts are not500Exec/600s and do not cover the frozen complete fixture/physical guards | Widen closedR8 receipts or relabel released v0.1.5 proof as current-source evidence |
| 74 | Preserve actual owned named-file and runtime-file zero guards, exact namespace-row release and≤2GiB backing boundaries separately for the early integrity paths | Current overlay is shared per daemon; shared file persists beyond Workspace release. Source-backed exhaustive named ownership and actual observations are required; missing coverage fails full proof, never guessed zero. Per-Workspace physical peak remainsUNAVAILABLE | Equate Gone or zero SQL rows with zero shared-file size, omit cleanup, or replace canonical corruption with compressed-frame damage |
| 75 | Remove89 exact owned passing closed setup masters after fresh identity/connection and custody admission; retain19 uncertain/failed/future candidates | Passing setup SDKKnownStop/container removal and source witness hashes establish custody; different frozen reliability fixture prevents reuse. No failed/uncertain sample resource, historical master or owner resource is selected | Keep unnecessary passing volumes or remove candidates by prefix/guess/force |
| 76 | Generate exact supported RAWsingleton through existing public encoders and mutate only a same-length canonical target byte; delete only selected locator/advisory closure for missing fault | Current32768BWholeFile differs from retired firstChunk; sharedpack framing must remain valid and unrelated records unchanged. Existing public APIs and stdlibsqlite3 avoid newdependencies/features. Current wholepackSHA refusal is supplemental only; ordinary originalFUSE EIO0 remains mandatory | Corruptcompressed frame/CRC, handwriteproductencoding, disableFK/triggers, or acceptbroadIO/startup errors asfullproof |
| 77 | Classify the two early integrity complete cases not-applicable before setup/sample; preserve every native/fault/EIO/cleanup/resource obligation | Public SandboxApi has private atomicCreate→Upload→Start with no CPU/memory/swap/PIDprofile, callback or adoption. Stopped backend create exists, but completing its owner would recreate SDK wire authentication/Hello. Post-Start limits cannot preserve frozen from-creation2CPU/2GiB/no-swap/256PID profile | Add forbidden productfeature/newwireowner/proxy, apply limits afterStart, or claim partial native diagnostics ascompletecasePASS |
| 78 | Ship only exact source admission/parity and pre-side-effect refusal for all28 reliabilityNA cases; retain rich prospective controller ignored/unqualified | No complete frozen case can execute under current capabilities; an unused runtime implementation would not qualify missing guards. Current-source NA skipping checks actual product/source; retained historical verification preserves original policy rather than masking recorded failures | Run incomplete cases, treat source parity as native proof, fabricate zero-cost samples, or drop failed/NA rows |
| 79 | Clarify spec with named source-applicability module, NA parity scope and verification-only performanceN/A/proof disposition | Section4 omitted the source-only reliability machinery; section6.1 omittedN/A despite verification-only cases; section7 did not distinguish honest missing-capability source checks from runtime proofs. Existing original guards/limits/schema columns/receipts stay unchanged | Claim fullruntime parity for unexpressible cases, hideNA in performanceN/A, or invent unnamed source paths |
| 80 | Retain all19 uncertain masters andthree new failedread-only custodyprobes after strictsidecar admission refused | Three previously future-kept passing setups have present0-byteWAL/32768-byteSHM; no-sidecar proof failed before hashing. Freshreadonly observations establishexit1/noOOM, not an admitted closed-byte proof; no unchanged retry, relaxedguard or deletion follows | Delete despite failedcustody, ignore SHM/WAL or rerun a weakened probe |

## Bugs

| Bug | Case | Symptom | Cause | Fix commit | External test |
| --- | --- | --- | --- | --- | --- |
| H-27 (harness custody) | Prospective multi-runtime disposer and legacy partial-loss receipt | Removal reader rejected after primary removal because an optional None key appeared in legacy seven-key identity | Generic identity builder added absent optional keys instead of exact conditional schema | 149d40bac; worker19b7de1a4 | Original root/worker RED retained; seven worker and eight integrated retention guards PASS, all Docker actions mocked |
| H-28 (harness installation) | First cross anchor and six footprint cases at149d40bac | Daemon refused existing installation metadata before readiness | Byte install copied host Init manifest with daemon_sqlite=None without public install qualification | 1cc25d4de; native3e37aa36f and controllerf05010629/11635c9b | External actual held-SealedProject SDK install RED; four native and nine Python final guards PASS; failed receipts kept and affected cases sampled once at corrected source |
| H-30 | CDC overwrite1 and cross anchor1 at1cc25d4de | Actual successful mount ACK followed by controller timeout; imported claim missed executed guard hashes | Waited for literal mount-branch instead of actual mount acknowledgement; claim omitted workspace/receipts hashes | 08288e824 | Three actual mount protocol and three claim identity guards; original red retained |
| H-31 | Footprint unique100000 at1cc25d4de | KeyError after actual Committed ACK | Init step indexed instead of declared mutation step and eager missing top-level fallback | 08288e824 | Three strict operation expectation guards; original red retained |
| H-32 | Footprint unique100000 at08288e824 | Completed paid work/End/Gone, then cache-observation exit3 skipped proof and Stop | Controller treated the cache helper declared INELIGIBLE status as an execution error | This commit | test_actual_ineligible_a_observation_retains_miss_and_completed_cleanup plus eleven error/forgery refusals; parent red retained |
| H-33 | Footprint unique100000 at0a8d15f0e | Completed paid work then original cache identity refusal skipped verification/Stop | Unknown cache classification incorrectly became a functional execution failure | This commit | Original UNAVAILABLE/status4 reproduction red; strict malformed/error refusal vectors and cache classifier INELIGIBLE guards |
| H-34 | repository deepseek-stride10 at08288e824 | First original importer refuses target as notFUSE; actual mount was/workspaces/1 | Workload defaulted/workspace instead of authenticated actual mount root, for both apply and fresh verifier | Workera85f0ab97 / this integration | Parent full17 wrongtarget RED; nine final strict choreography/actualACK/custody tests PASS |
| H-35 | repository deepseek-full administrative failed-prep closure | First report displayed actual caseFAIL although actual operation/proof attempts were0 | Generic status field copied administrative preparation verdict instead of actual case disposition | This commit | Parent external actualcase-status RED and27 negative vectors; final2 new/5proof-only/4incomplete-report tests PASS |
| H-36 (harness/reporting) | Prospective26 unattempted reliabilityNA rows | ProofCSV defaultedNOT_RUN and omitted declared missing capability; naiveNA fallback could mask an unexpected actual failed receipt | Reporter lacked source-reviewed not_applicable_reason/default and did not distinguish absent vs actual receipt | This commit | External wrong-proof-status and realmetadata missing-reason REDs; unexpectedFAIL masking RED; final2new/5proof-only/4incomplete checksPASS |
| H-37 (harness scope) | Prospective reliability faulted closed sample installation | Existing sample=True installer metadata describes fresh measured Init, which is false for isolated integrity-copy proof input | Generic sample naming and measured-Init purpose were conflated | This commit | Exact original two-case purpose/byte-seal guard RED; wrong case/master refused beforefolder/Docker; final2new+14historical checksPASS |
| H-1 | C07 | Workload and verifier passed, unmount returned, residency helper refused changed file identity | Harness inspected overlay while automatic post-unmount reclamation was still mutating backing | `a1f2c259c` | `test_backing_observer_waits_for_original_reclamation_to_finish` |

| H-3 | Generic mounted pilot proofs and prospective SDK proofs | Reachable path walk omitted exhaustive inode-table membership from original canonical verifier | Runtime compared declared paths/bytes but never enumerated current inode table | `37ec6826b` | edit_oracle_complete_membership_accepts_exact_and_rejects_unreachable_or_missing_rows |
| H-4 | Prospective B/C Commit eligibility | Zero source demands could credit own-written backing without independent pre-Commit coldness | Cache classifier checked source demand/lifetime but not own-written phase inputs | `98b6b7260` | test_zero_source_demands_cannot_credit_own_written_commit_input |
| H-5 | Seven recorded pilot read-only B labels; future B/C rows | Scoped zero canonical-object demand was treated as full numerical cache qualification | Still-paid backing/metadata/SQLite pre-phase domains absent; terminal mincore and an object-demand interval ending before unmount cannot establish them | `25719de3b` (prospective only) | test_zero_object_demands_do_not_qualify_unobserved_paid_phase_inputs; test_each_phase_proof_rejects_late_unknown_wrong_identity_or_warm_inputs |
| H-2 | 36 batch-1 reviewed registrations | source_commit was an annotated tag object; parity accepted it and rejected the actual peeled commit | git rev-parse on an annotated reference returns the tag object unless peeled | `222526fa0` | test_review_identity.py rejects the tag through both parity routes, admits the peeled commit and checks every ported review |
| H-6 | local-snapshot-create-25000-onebyte-v1 | Original native witness exited0, then controller raised dictionary update #5 after measuredEnd | Frozen sample receipt has multiline path/range values and no verification_status; harness assumed full-tree key/value receipt | `03033abdc` | test_original_multiline_sample_with_successful_exit_is_preserved fails beforefix; altered paths/ranges/scope/exit are rejected afterfix |
| H-7 | Prospective Git4, found before first sample | Late observation publication followed by failed failure-sidecar write or process death before final post-check could reload provisional PASS | Draft guard bound timestamp before the observation write and lacked mandatory completion/known outer return | `031d427a7` | slow observation/completion publication plus failed failure-sidecar and os._exit(91) regressions fail before correction; missing completion/outer acknowledgement remain failed |
| H-8 | Prospective batch4, found before any attempt | Output-close or final publication failure could retain FAIL after deleting sample resources | Administrative container/volume removal occurred before finally close/verdict/publication and driver acknowledgement; partial removal also claimed blanket retained custody | `56a422544` | test_close_failure_after_all_proofs_keeps_container_and_volume fails against a921bedbd with both rm calls observed; missing outer ACK/failed return do no removal, partial lost-volume acknowledgement remains uncertain and immutable bound |
| H-9 | Prospective two verification-only cases | Overall output-closeFAIL could disappear in CSV when performance wasN/A and semantic proof retainedPASS | Original proof-only mask omitted the independent full-outcome failure tag | Worker2fe8e9b2e / integration65df42e91 | test_overall_close_failure_remains_visible_beside_passing_semantic_proof fails beforefix; final5reportchecks PASS0.048s |
| H-10 | Prospective batch4 prepared SDK routes | Missing/partial prepared-step map could invoke native fallback and regenerate original payloads inside measuredwork; stale sealed helpers could disagree with reported current build | Controller trusted optional prepared records without mandatory originalstep/binary/descriptor/payload/script correspondence admission | Workera8c56d570 / integration91cf97fab | test_helper_refusal_precedes_case_claim_and_any_clone_or_mount fails at686344e8f; exact-source temporary assets reject five old-admission bypasses and all15guard checks PASS |
| H-11 | v016-boundary-small-control-v1 and dedup-history-distributed-1 first attempts | Native proof/acknowledged Commits/Gone/KnownStop pass, independent public history proof fails acknowledged Commit missing | Controller copies only exported main file into typed-store, dropping closed WAL; immutable main-only census would also miss WAL | Workerf2c143b37 / integration2b52bdf17 | Real closed-WAL controller regression sees0 of100 acknowledged test rows on old main-only copy; red0.067s under120s mainlock |
| H-12 | Mixed final failed preparation and prospective failed family parity | First missing/invalid preparation aborts parity without retaining earlier/later original case outcomes; CLI propagates only an exception | parity_workspace raises inside first failed case before writing parity.json | Worker47e5a5500 / integrationea7148a19 | Missing middle preparation red fails old behavior; all original membership/pin/bytes/native/seal guards retain FAIL reasons,22checksPASS; CLI old exception red retained,new5driverchecksPASS |
| H-13 | dedup-history-distributed-500 setup at7905 | stage-code-check exits255: exec /bin/bash argument list too long, before any mount/product operation | fixtures.stage embeds all500 prepared-step/helper SHA rows into one165,872B Bash -c argv | Worker359b63686 / integration8ae040cdd | Worker owns external500-step/complete-row/remote-manifest/payload tamper regression; original fullchecker retained |
| H-14 | Offline closed-Store normalization before remaining batch4 cases | Fresh target enters WAL initialization while SQLite synchronous is still default FULL(2) | synchronous=OFF was applied only after journal_mode=WAL in prepare_closed_store | `e09c61bbd`, harness only; product analogue under investigation | Real sqlite3.Connection subclass observes FULL(2) before journal switch on old helper; final10 normalization checks PASS0.352s and8 controller checks PASS0.159s |
| H-15 (product) | Public Disposable Store create across three layouts and two acquisition schemas | Actual VFS observer records24 forbidden xSync calls during creation; later Save/seal/reopen issue0 | Selected synchronous/fullfsync applied only after acquisition and WAL initialization had written main/journal | Workera9d8b0453 / integration `a05ed6027` | External public-API disposable_startup_sync.rs observes24 on old source; finalhost14/Linux6 covering checks PASS with Store sync0 and positive control1 |
| H-16 | Aggregate formatting during Disposable startup product verification | cargo fmt cannot resolve two daemon example/test digest modules | Stage0.2 missed the two old fs-bench-pro-storage-content import paths | `9dfb70ee4`, path-only example/test correction | Owning example/test no-run PASS16.27s; existingseven startup/digest receipt tests PASS0.10s; aggregatefmtPASS |
| H-17 (frozen workload/oracle) | Mixed M1 100 MB K10 at fa70bbb01 | Fresh-mount verifier rejects d000/f00054 mtime after ten successful commands and Commit ACKs | Frozen atomic save sets stage4 mtime before writing the temporary; writing updates it, while the original shadow retains stage4 mtime | No product or frozen-source change; strict failure retained | Exact original source/scope seals plus retained stat and bounded native sequence confirm correct POSIX write/rename behavior |
| H-18 (watchdog failure) | Dedup distributed500 at e308118a9 | Unchanged 120 s outer stop expires after 377 acknowledged Commits; no complete receipt/proof/End/Stop | The finite original 500-step schedule does not finish within the bounded attempt; ongoing progress observed, no source-backed deadlock or wrong result identified | No retry, optimization or timeout change; partial attempt stays FAIL with uncertain custody | Administrative external-supervisor closure binds original selection, invocation exit, runtime stream and clone; complete metrics remain unavailable |
| H-19 (harness) | Batch5 explicit parity-build CLI authoring | Branch/multi CLI rejected before parity despite implemented context setup/controller | Their two original family names were omitted from the CLI workspace allowlist | `821624cb3` | External CLI wiring red retained; final legacy/no-build and branch/explicit-build check PASS; no actual case |
| H-20 (harness) | Fresh empty preparation for P1 at9da25b495 | Copied native oracle crashes importing /code/engine/oracle.py, before Store Init or SDK test | Module-level repository-root parents4 resolution assumes deep host checkout | `e60a1aa2c` | External shallow-copy import red; final copied CLI and pinned host census/legacy oracle scope PASS; failed prep/resources retained |

| H-21 (harness) | Historical access result before first actual access invocation | Driver would reject completed access before publication ACK and disposal | Controller used a descriptive N/A performance_mode while bench.py requires canonical verification-only | `4a40fb690` | Actual controller result through real bench.main fails at parent; final driver publishes before disposal, samples0/attempt1 unchanged |
| H-22 (harness) | Paired historical input setup authoring | No explicit complete closed-producer reuse path; two paired selections would replay an identical setup producer | CLI and fixture adapter only created a new producer and required source commit equality, despite unchanged setup scope | `4a40fb690` | Parent reuse regression retained; exact original identity/files/AST/blob seals and full custody/graph/Store checks qualify reuse without a replay |

| H-23 (harness) | First historical boundary-before at c6ec3e467 | Sandbox Listener EOF before readiness or mount; exact daemon stderr private directory ownership/mode/type | Historical installer conflated Workspace source UID501:20 with daemon backing UID0:0 and retained file0644 | `68209bdb7` | External installer regression fails at parent; final39 covering checks and wrong-owner/mode refusal variants PASS; old actual failure/resources retained |

| H-24 (harness/reporting) | Failed concurrent branch and multi rounds | CSV reader copied initialized0 command/Commit/unmount totals despite observed partial work | Controller fails before accumulating a whole round; reporter treated all phase values as complete case metrics | `669bc4853` | Parent external report tests fail; final13 custody/report checks PASS; retained sealed actual rows read without changing receipts |

| H-25 (harness) | Historical fork-point atc79eb607f | Native selected-root/all-Branch proof PASS, but Python full ordered ACK transcript check FAIL | JSON-sorted closed graph was serialized alphabetically a,b,initial,trunk rather than original initial,trunk,a,b | `2f7160c92` | Sorted-JSON regression fails at parent for both compact producers; final42 scoped checks plus exact retained30ACK/15distinct/5Branch corrected-order check PASS; strict checker unchanged |

| H-26 (harness/counting) | Completed compact branch telemetry | Exact count summaries unavailable despite all original source observation frames retained | Mapper did not classify ordinary Sandbox commands with authenticated nonadvancing Control counters | Workerdf901bfd0 / this commit | Parent RED, final6 worker and6 integrated count guards PASS; separately bound original pre-proof prefixes qualify162/2844/12628 and216/3806/16873 without replay |

H-1 and H-2 are harness faults, not product correctness or performance defects.
The original file-identity checker remains intact; no product source changed.

## Failed, ineligible, not-applicable and unrun cases

At `358eca082`, C01–C06 passed with exact baseline request/owner-job/statement
counts, independent verifier PASS, automatic cleanup Gone and known Stop. C07
failed on H-1 after successful command, verifier and unmount. C08–C12 remain
NOT_RUN in this source's two generated CSVs. All 273 workload cases remain
pending; this checkpoint closes none of them. Corrected-source sampling covers
C07 and the remaining five cells only; unaffected successful cells are retained,
not re-sampled.

## Checks and source size

Handoff commit: documentation-only; checked whitespace and the new spec's
local links. Rust/runtime checks do not cover this documentation change.
Production LOC is counted before each commit with the pinned
`count_absent_aware.py staged` and confirmed against the committed tree.
Initial active core: 79,366. Reference absent (retired R9c; last counted
65,417 at tree `498dd1917812`).

## Resource custody

New immutable masters retained for setup reuse:
`layerfs-fsb-master-empty-ee794ff64e0a4bcb` and
`layerfs-fsb-master-big64m-c60eb32e5145496b`. Passing build, provision, native
oracle, byte-copy and C01–C06 sample containers have been removed; their exact
names and acknowledged removal are in raw receipts. Passing sample volumes
also removed.

H-1 failed resources retained: running container
`3b5974a094800d78dc72648dbeb08c75cdea933957ed188891a812bb7cad6483`, sample volume
`layerfs-fsb-sample-09fa81ddc18a4c0c`. No mounted key remained at the failure;
known Stop and physical cleanup were not established. Host runtime group was
fenced once with original-cause custody retained, not interpreted as Docker
Stop. Read-only inspection confirms the original container still running.

Protected handoff scripts,
prepared inputs, master volumes, running containers, merged worktrees and the
untracked `PROMPT-R8-R9-NEXT-AGENT-20261010.md` remain untouched.

Specification correction commit: docs-only; source rows counted directly and
whitespace checked. Production LOC 79,366 -> 79,366 (delta +0), same pinned
absent-aware method. Handoff spec/README tree preserved at `3c32fbc97`.

Stage 0.1: harvested 461 rows, preserving original fields and source hashes.
Scoped reference tests run under the checkout lock with a 120 s wall stop.
No benchmark invocation; no container/volume created. Harness/docs-only
production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware method.

Stage 0.2 specification clarification is docs-only. It does not permit an
expectation change or weakening a checker. Production LOC 79,366 -> 79,366
(delta +0); pinned absent-aware method.

Stage 0.2 relocated 316 tracked files into the specified subjects. Supporting
paths follow decision 7. All 14 moved JSON registrations and Cargo lockfiles
are byte-identical to their first-parent versions; active product and frozen
root definitions are unchanged. `git diff --check` and Python syntax pass.
Raw check outputs, including every failed intermediate path check, are retained
under `benchmark-results/fs-bench-pro/2026-10-10-stage-0-2-checks/`, with a hash
manifest and the complete move map. No workload sample or Docker resource was
created. No assertion, verifier, threshold or registration was weakened.

The final affected checks are: cell suite 300 passing tests plus the three
Git-index helper tests after their import correction; 65 proof tests; cache
suite 10 passing and two existing Linux-only skips; 44 evidence registration
tests; five retired writer registry tests; 19 isolation tests; 14 fuser guard
self-tests; two storage-content Rust registry tests. Both runtime and
storage-content `cargo +1.85.1 test --locked --all-targets --no-run` builds pass
using the repository ARM64 flags, `core/target`, construction workers 1 and
120 s locked wall stops. Other affected shell/mutation checks are in the raw
outputs. No command reached its wall stop; the slowest Python check was the
retired Workspace collection (87.719 s).

Historical failures retained, with their assertions unchanged:

- `test_phase7` expects the old `Handles::create` timer seam in the current
  Project example; that seam is absent in unchanged product source.
- `test_phase7_history_vehicle` expects two old textual reference seams and an
  old verifier failure order/profile. All three checks fail on current source;
  its first assertion stops before its later Durable argument can execute.
  Do not run this historical MEMORY/Durable selection again.
- `test_substrate` expects the retired host-mediated runner to dispatch/build;
  the actual retirement guard refuses it before resources are acquired.
  Four errors remain in its final selected check; no restored Server route.
- Storage-content's two source-scan assertions reject `sync_all` in native
  global Store installation and the FUSE `fsync` callback name in unchanged
  product source. Their frozen scanner is retained. This is not an observation
  of an fsync on Disposable Workspace backing.
- Fuser integrity keeps OWNER-3's two passthrough refusals at its relocated
  manifest/lock. The checker only changes the approved platform manifest path;
  no third-party bytes or allowlist policy change.

The old cold-native test needed its declared `target/phase7-agent` scratch
parent, which was prepared without editing the test or running a Store. Its
six selected tests then passed. These historical checks are not new workload
verdicts. Applicable relocation checks pass; unavailable/stale contracts remain
visible and do not authorize a retired route.

Stage 0.2 Production LOC: 79,366 -> 79,366 (delta +0), active core only;
reference absent (retired R9c; last counted 65,417 at `498dd1917812`). Method:
pinned absent-aware parent/staged archive counter. No speed claim.

Routing follow-up: update the current harness guide and add its README to
point to moved subjects and this rollout. Local links and whitespace checked.
Documentation-only; Production LOC 79,366 -> 79,366 (delta +0), pinned
absent-aware method. No runtime check or workload sample repeated.

P-1/P-2 source correction: source checked in `project/history.rs`,
`project/mod.rs`, Workspace binding, Persistence History fork, and the existing
`mounted_cycles` ancestor-fork test. No product execution or capability proof
was repeated. Public native route is verified when batch 5 is reached. This
specification-only commit keeps Production LOC 79,366 -> 79,366 (delta +0),
pinned absent-aware method. Catalogue work remains unstaged for its own commit.

Stage 0.3: `bench.py list` reports exactly 25 families and 273 unique case IDs,
all pending. This includes inherited, proof-only, capped and extended IDs.
Forty-seven exact IDs have no released timing; every one is listed in
`baselines/v0.1.6/missing.md`. No 26th family was found in the root directories,
Rust registries, SDK edit registries or Python wrappers; storage-smoke aliases
remain profiles of existing families. No P-6 family was added.

The original std-only workload module supplied 269 registrations; the original
repository-history registry supplied three more and small-file-delta its one.
The exporter includes harness definitions only and invokes no product, fixture,
workload or verifier. It was compiled with toolchain 1.85.1 and the ARM64 flags
under a 120 s checkout lock. Three owning catalogue tests pass in 4.843 s,
including mechanical membership against the compiled original registry and the
Python wrapper registry, and the product-free CLI. No workload case sampled.
Raw catalogue/check evidence is retained under
`benchmark-results/fs-bench-pro/2026-10-10-stage-0-3-catalogue/`.

P-2 source check: direct SDK bind selects the Branch's coherent current
snapshot. Existing public `ProjectApi::fork` can use `ForkSource::Commit`;
Persistence validates ancestry and returns the selected Commit root. The
existing native mounted-cycle source tests demonstrate the related-root
composition. Its actual SDK/mounted verification belongs to batch 5; an extra
Branch must be declared as a difference in historical-access ports. No missing
feature or not-applicable verdict is invented at the catalogue stage.

Stage 0.3 Production LOC: 79,366 -> 79,366 (delta +0); active core only,
reference absent (retired R9c; last counted 65,417 at `498dd1917812`), pinned
absent-aware parent/staged archive method. Engine execution is not implemented
or claimed by this catalogue. Containers/volumes created: none.

## Production LOC by commit

All comparisons use the same pinned absent-aware counter over exact first-parent
and committed snapshots; active core only, reference absent as recorded above.

| Commit | Purpose | Before | After | Signed delta |
| --- | --- | ---: | ---: | ---: |
| `3c32fbc97` | Preserve handoff and initialize ledger | 79366 | 79366 | +0 |
| `d5d33a73b` | Correct verification-source count/units | 79366 | 79366 | +0 |
| `8e4a78fa0` | Stage 0.1 reference harvest | 79366 | 79366 | +0 |
| `abcfad75c` | Clarify supporting-file relocation | 79366 | 79366 | +0 |
| `13f9dec38` | Stage 0.2 move-only reorganization | 79366 | 79366 | +0 |
| `9af29e01e` | Current routing guide | 79366 | 79366 | +0 |
| `30b798192` | Correct existing SDK History methods | 79366 | 79366 | +0 |
| `f182e9a63` | Stage 0.3 case catalogue | 79366 | 79366 | +0 |
| `bb8e07df8` | Define separate cell execution/report route | 79366 | 79366 | +0 |
| `c496025fc` | Stage 0.4 build and receipt custody support | 79366 | 79366 | +0 |
| `7a27a4f26` | Stage 0.4 independent tree/index oracle | 79366 | 79366 | +0 |
| `516c2f009` | Stage 0.4 fixture provisioning and clone support | 79366 | 79366 | +0 |

Stage 0.4 entry-point clarification: docs-only; no sample or product change.
Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware method.

Stage 0.4 build support: `bench.py build` now drives one locked/offline host
release build and one daemon release build in the pinned image. It checks host
source hashes inside the Linux container before and after Cargo; checks host
source equality; retains exact binary hashes in immutable archives; and records
worktree-local incremental cache reuse. A failed/unknown build container is
retained. Passing build containers alone are removed. Source admission and
receipts have seven passing scoped tests (three identity, four custody) under
120 s checkout-locked invocations. The CLI help and whitespace pass.
Raw tests: `benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-build-tests/`.
No new release build, preparation, container, volume or measurement has been
attempted by this step; execution waits for the committed engine inputs.
Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware method.

Stage 0.4 oracle support reuses the original cell tree comparison, hard-link
classes and semantic Git index/TREE/checksum checker without editing them.
Expected state is built on an independent native root outside every timer;
observations and comparisons are append-only. All ordinary cell payload bytes
are hashed. The original timing-cell inode/time exclusions stay explicit;
C12 retains its exact scoped index stat-cache exclusions and checks actual
Git binary, version, config and effective values through the existing query
collector. Paired pin witnesses follow that collector's existing same-context
rule; actual independent witnesses remain in each observation.

The recovered C12 Git policy and a source/provenance record are now committed
under engine recipes; runtime reads these committed files, not their historical
`/tmp` origins. Their version-pinned primary-source hashes/review receipt remain
unchanged. This reuses policy evidence only, never a measurement or a native
control arm. Three scoped external oracle tests pass in 0.047 s: exact bytes,
namespace/hard-link mismatch and tampered evidence refusal. Raw checks are under
`benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-oracle-tests/`.
No Store, mount, Docker resource or measured case was invoked. Fixture preparation
and mounted execution remain next. Production LOC 79,366 -> 79,366 (delta +0),
pinned absent-aware method.

Stage 0.4 fixture support adds `bench.py prepare empty|big64m --build <seal>`.
Committed generators write independent native roots with fixed modes/mtime and
seed, then the existing release runtime calls public ProjectApi Init/seal/install
once. No second encoder, Store API or data route is added. Native expected states
are built once from original C-cell commands in separate pinned-image containers.
Prepared data, helper hashes, native oracles, Store bytes and manifest are sealed
under gitignored result paths. Nothing reads the old prepared /tmp trees or old
master volumes at runtime. Source/provenance labels alone refer to history.

Samples get fresh named volumes and full writable byte copies with reflinks and
sparse-copy substitution disabled; compare/SHA-256 bind each clone to its closed
master. Clone warmth is declared and is not cold eligibility. Resource claims
precede creation; failed/unknown containers/volumes are retained. Passing copy,
provision and native-author containers alone are removed. New master volumes
remain immutable setup inputs until the rollout no longer needs them, when their
explicit owned disposition is recorded.

Four scoped fixture tests pass in 0.016 s (the recorded wall in the raw log),
covering deterministic identity/exclusive outputs, exact cell membership,
failed clone custody and all copied helper hashes. CLI help and whitespace pass.
Raw checks: `benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-fixture-tests/`.
No actual build, Store, Docker resource, prepare or sample has run in this step.
Workspace execution/cache enforcement and CSV reporting remain to implement.
Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware method.

Stage 0.4 workspace/report support adds one-shot cell execution through the
existing SDK runtime, original command bodies, independent native tree/index
oracles, terminal unmount plus observed automatic cleanup and known Stop.
Raw numeric telemetry is correlated with original control identities and priced
using the unchanged matrix helpers; exact totals are checked against the pinned
closed baseline (SHA-256 `a6ada570050f14bb6ff5bd61c74c0d0fda439f323f695fdea1212b26b590d3f8`).
Class B requires fresh measured mount, successful untimed warm terminal cleanup
and actual zero immutable object demands. Residency observation, setup, cleanup
and Stop are separate from the matrix performance clock. No cold claim is made.

`run cluster-two/cells --case Cxx` appends a fresh case to a source-fixed run
registration; the original claim refuses unchanged re-sampling. `verify`
compares retained independent observations without replaying product work.
`report` exclusively generates both exact section 6.1 CSV schemas with all
registered cases, raw ns/bytes/counts, original failure reasons and UNAVAILABLE
for missing values. A completed EXCEEDED row remains functionally PASS.

Twenty-one scoped engine tests pass at final source in 0.275 s under the 120 s
lock. The first invocation exposed macOS temporary-path aliasing in the new
receipt finalizer; resolving the comparison root fixed it without changing the
original failure or assertions. Both logs are retained under
`benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-workspace-tests/`.
CLI help/whitespace pass. No Store, mount or Docker resource has yet run.
Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware method.

| `358eca082` | Stage 0.4 workspace/cache/CSV support | 79366 | 79366 | +0 |

Stage 0.4 actual build/prepare: host and pinned-image Linux release builds pass,
with in-container source hashes checked before/after Cargo. Binary hashes match
the retained working path: runtime `fdd962b7258903252bf097cd1be22f8acd56d3047369d08b68affe4c491d0573`,
daemon `f44900cfc975901b7101f4874f708390b6f7401981e06d608b3df52a50848c0e`.
Exact OWNER-3 passthrough refusals remain recorded as FAIL; no checker success
claimed. Both fresh recipes provisioned through public Init/seal/install and
built independent native expectations for all twelve cells. Existing masters,
/tmp inputs and containers were untouched. Raw build/preparation paths:
`benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-build/` and
`benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-prepared/`.

Original C01–C07 receipts are under
`benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-cells-358eca082/`; generated
CSV files preserve six PASS, one FAIL and five NOT_RUN rows. No measured limit
exceeded among these seven observations. H-1 moves the same unmodified backing
observer after read-only original cleanup observations reach Gone, still outside
product time. It changes no workload, timer limit, cache class or demand-counter
eligibility. Twenty-two scoped engine tests pass at corrected final source;
raw checks are in `benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-cache-order-tests/`.
No product change or product check is claimed. Remaining cells run only after
this harness correction is committed. Production LOC 79,366 -> 79,366 (delta +0).

| `a1f2c259c` | Fix H-1 and preserve first cell run results | 79366 | 79366 | +0 |

Stage 0.4 corrected-source run: C07–C09 and C11 pass with identical baseline
counts, independent verifier PASS, cleanup Gone and known Stop. C10 and C12
complete all functional/cleanup checks but fail the unchanged exact count
comparison. No latency limit exceeded. C10 observed 1,547 requests / 1,546 owner
jobs / 14,406 statements versus 1,548 / 1,547 / 14,408. Its entire opcode delta
is one READ (512 versus 513); the corresponding one Workspace and one Inode
SQL execution account for both SQL deltas. Historical C10 receipts already
show both 512 and 513 READ counts across product identities; they are evidence,
not re-used measurements. The exact current daemon binary and product tree are
the same as the frozen baseline. An exact kernel-request partition is not
specified by the ordinary `cp` workload. The observed kernel is 6.12.76-linuxkit;
[the corresponding upstream stable FUSE source](https://raw.githubusercontent.com/gregkh/linux/v6.12.76/fs/fuse/file.c)
constructs asynchronous READ batches from the readahead window and congestion
state (`fuse_readahead`, `fuse_send_readpages`). Request partition variation is
an inference supported by that path and the observed READ-only delta; no
per-request length trace exists to identify the exact split. Do not claim an
EOF request or a proven individual kernel scheduling event.

C12 observed 3,436 / 3,439 / 24,145 versus 3,436 / 3,439 / 24,146. Opcodes and
job-class counts are identical. The sole difference is an Inode-family query
with 164 SQL bytes, 16 bound bytes, zero returned/changed rows and 12 VM steps.
Source identifies exactly that SQL text in
`core/crates/layerfs-overlay/src/lifetime/orphan.rs:50`: the `gen=-1` orphan-inode
probe. It is conditional on the live engine's `orphan_seen`; last-owner
reclamation resets the flag after deleting orphan rows. Git rename/unlink and
reclamation can interleave, so exact aggregate probe count is not a workload
semantic invariant. Exact event attribution cannot be reconstructed from the
bounded aggregates; this remains an explicitly labelled cause inference.
There is no wrong byte, refused successful operation, leaked mount or crash in
these two samples, and no authorized product correctness fix identified.

Raw source-pinned arithmetic/provenance:
`benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-count-diagnostics/diagnostic.json`.
Raw corrected run: `benchmark-results/fs-bench-pro/2026-10-10-stage-0-4-cells-a1f2c259c/`.
Generated CSVs retain four PASS, two exact-count FAIL and six NOT_RUN rows for
this identity. Independent retained verification confirms all six sampled
case observations PASS; CLI completeness returns 1 because the other six
registered cases are NOT_RUN at this identity, as intended. Earlier C01–C06
functional/count receipts remain unaffected and are not repeated. Overall ten
of twelve exact triples reproduced; twelve functional workloads verified.
The strict stage 0.4 target has not been met and is not claimed as met.

C10/C12 stopped failure containers and sample volumes are retained and named in
their receipts; their exact IDs are included below. Passing C07–C09/C11 sample
resources have been removed. H-1's original running container/volume remain.
Production LOC 79,366 -> 79,366 (delta +0); docs/results only, same pinned method.

| `23ba793ef` | Preserve remaining twelve-cell outcomes and count diagnosis | 79366 | 79366 | +0 |

Retained C10: container `96dd83a9e8ea10ad2e7a50aa3e4e2dd8d970509a22435c146569f20a16b8d200`, volume `layerfs-fsb-sample-070f622942e64f72`, acknowledged `KNOWN_STOP`.

Retained C12: container `6a122c4664d39141006eabc62c538060796dbb1da57579e4beeb8495a5f2b0f5`, volume `layerfs-fsb-sample-bc7e58da1e4b4068`, acknowledged `KNOWN_STOP`.

Stage 0.4 closure correction: the original exact-count target did not work for
C10/C12 and is truthfully closed FAILED, not satisfied or silently relaxed.
The unchanged checker still records FAIL for both original comparisons. Spec
section 9 now allows this explicit failed closure when functional proofs and
terminal cleanup are established and no product correctness bug is identified.
This follows the handoff's instruction to correct an unworkable spec in its
own commit, decide without owner questions and continue. All twelve cells were
attempted at clean source; the H-1 affected case alone received a changed-source
sample. No arbitrary tolerance, acceptance threshold, product optimization or
extra family was added. Numeric kernel partition/probe attribution remains
limited to the source-backed aggregate inferences above. Stage 0.4 independent
verification is complete for all twelve sampled workloads, with retained
per-case proof receipts; no speed or repeatability claim is made.

Scope of this commit: spec and ledger only. Whitespace/local links checked;
no build/test/Store/mount invocation. Production LOC 79,366 -> 79,366 (delta +0)
with the pinned absent-aware counter. Batch 1 is next, one family at a time.

| `1a8375dbe` | Correct stage closure while preserving reproduction FAIL | 79366 | 79366 | +0 |

Batch 1 / init_namespace implementation: four cases now have frozen ported
registrations and product-neutral workload.sh. Fixture generation transcribes
only the native create_namespace_fixture write loop and calls the frozen
std-only planner/content stream/metadata/tree digest. Preparation mechanically
compares the generated native tree with the original full native namespace
verifier (count, bytes, complete content digest). Parity binds original registry,
reviewed source hashes/commit, fixture equality, ordered Init operation and
original sampled verifier coverage. No preparation/sample has run yet at this
new harness source; registration is committed before those operations.

The runtime adds host-only `init` and separate-process `verify-init` modes over
existing public ProjectApi and Content/Storage/History ports. Public Init creates
one explicitly Disposable Store, constructs the complete root, publishes its
first Branch and consumes its session with seal. Independent proof reopens the
returned sealed root/first Branch and checks exactly the original <=11 sample
set, metadata/lengths and <=64 KiB prefixes. No daemon, mount, host data server,
product feature, encoder, optimization or product source change is added.
Unselected paths/bytes remain explicit proof omissions. Global daemon counts
are UNAVAILABLE for this host route, not invented as zero. A new release build
will compile both original native-only tools and pin all included frozen source
bytes. Existing dependencies/versions are preserved; the manifest/lock change
only adds already-transitive first-party layerfs-content as a direct dependency.

Two new external runtime tests pass in 2.00 s after locked --no-run compilation:
sealed public Init/reopen proof and exact-byte corruption rejection. Every
invocation had the explicit 120 s checkout lock and each child has a bounded
12 s wait. An accidentally broad first selection also discovered historical
inline tests through the frozen module; it was explicitly stopped at 77 s
before the ceiling while the unrelated mixed_v2_contract CPU check ran. Its
exit 143/logs are retained and it is not claimed passing. No historical test
was weakened/edited; the final selection filters only the two new sdk_init_
tests. Initial compile failures (manifest ID width/API field names) are retained.
An offline lockfile-generation attempt also upgraded unrelated versions; those
own draft changes were restored from the parent, leaving only the direct
first-party dependency list addition. That initial unsealed build is not a
release identity, measurement or dependency-change claim.

Twenty-three engine tests pass in 0.319 s; three catalogue tests pass in 0.124 s.
Report reference values now resolve the exact preserved released source row;
verification-only timing is not promoted into a performance reference. Class A
requires integer zero content-page residency and separately verified cold
metadata; booleans cannot fabricate eligibility. Rust formatting checks use
skip_children=true on the owned files to preserve frozen external modules;
whitespace passes. Product tree remains unchanged. Raw checks:
`benchmark-results/fs-bench-pro/2026-10-10-batch-1-init-harness-checks/`.
Actual parity and four samples remain next. Production LOC 79,366 -> 79,366
(delta +0), same pinned absent-aware method.

| `c1d7dd0e8` | Port Init definitions and host runtime/parity support | 79366 | 79366 | +0 |

Batch 1 / init_namespace complete: all four original mandatory cases are ported
and parity PASS. Exact generated native counts, bytes and content digests match
the original full native verifier; reviewed operation/coverage hashes are sealed.
The locked release build passes with host source hashes checked inside Linux
before/after its build. The new host runtime/native tools are archived by SHA;
the daemon remains `f44900cfc975901b7101f4874f708390b6f7401981e06d608b3df52a50848c0e`.
Four cases each sampled once at clean `c1d7dd0e8`, through public Init and a
fresh Store; independent separate-process reopen/root/first Branch/sampled
metadata-and-byte proofs all PASS. Stores seal/close without WAL/SHM/journal
sidecars. No Docker sample resource or mount was created for this host route.

All four numeric rows remain INELIGIBLE because source bytes were generated
and read in preparation and host cold metadata cannot be verified. Functional
completion is PASS for each; INELIGIBLE is preserved and no cold/performance
qualification is asserted. Requests/owner jobs/daemon statements and overlay
storage are UNAVAILABLE because this route uses no daemon/overlay. Complete
wall is host child launch-to-exit; command_ns is the complete public Init/seal
span. Neither includes fixture setup or independent proof. Reference rows
retain release 0.1.5 and its different layerstack_init_ns timer/topology, without
ratios. No declared 15 s wall or 9.5 s verifier limit exceeded.

| Case | Functional / proof | Numeric status | Complete wall ns | Public Init/seal ns | Proof wall ns | Store logical = allocated B | Close |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| namespace-100-compact-v3 | PASS / PASS | INELIGIBLE | 3336290834 | 51216875 | 23149958 | 5263360 | SEALED_CLOSED; no mount |
| namespace-1000-compact-v3 | PASS / PASS | INELIGIBLE | 182638250 | 166821125 | 22118958 | 20606976 | SEALED_CLOSED; no mount |
| namespace-10000 | PASS / PASS | INELIGIBLE | 2127532084 | 2076532083 | 39871875 | 305762304 | SEALED_CLOSED; no mount |
| namespace-100000 | PASS / PASS | INELIGIBLE | 7829361042 | 7694068833 | 187957917 | 516554752 | SEALED_CLOSED; no mount |

The two CSVs were generated once by bench.py report under
`core/benchmark/fs-bench-pro/baselines/core/2026-10-10-c1d7dd0e8/`; all four cases
are present. bench.py verify checks retained proof records and immutable
original evidence hashes, with four PASS and no product replay. Raw build,
prepared input, parity and measured receipts are under
`benchmark-results/fs-bench-pro/2026-10-10-batch-1-init-build/`,
`benchmark-results/fs-bench-pro/2026-10-10-batch-1-init-prepared/`,
`benchmark-results/fs-bench-pro/2026-10-10-batch-1-init-parity/` and
`benchmark-results/fs-bench-pro/2026-10-10-batch-1-init-c1d7dd0e8/`. No product correctness bug surfaced.
The complete prepared native trees and closed sample Store files remain owned
raw setup/evidence; their paths are explicit in each receipt. No new failed or
uncertain container/volume was created. Existing retained H-1/C10/C12 resources
remain untouched. Unaffected engine/runtime scoped test evidence from the port
commit is reused; this result/doc commit changes no harness or product source.
Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware method.

| `39bbcbd6b` | Record four Init samples/parity and generated CSVs | 79366 | 79366 | +0 |

Batch 1 / payload_create_read implementation: eight original cases have frozen
ported definitions, original names/tiers, seed1, ordinary syscall workload.sh
and explicit differences/review pins. A native-only workspace fixture generator
uses the original fixture/expected algebra and complete native verifier, retaining
full file count/byte/content digests. The build now also seals the original
product-neutral Linux syscall executable after host-source hash checks inside
the pinned image. It compiles no retired product package or root crates.

The same existing SDK/Sandbox control machinery performs an untimed identical
ordinary call, terminal unmount/automatic cleanup, then fresh measured mount,
one unchanged ordinary call and one public Commit. Known read-only UpToDate
is allowed without manufacturing a new Commit. Performance stops at terminal
unmount; setup, cleanup, fresh native and persisted-root verification and Stop
are separate. Original control identities/numeric pricing remain the count
method. A new typed root field in the harness Commit event binds the separate
proof to the exact acknowledged root; no Debug result is parsed.

Independent proof uses the original complete native tree/byte checker after a
fresh mount, then current typed/authenticated public Content/Storage/History
reads over a closed, exported sample Store. Complete names/kinds/portable
metadata/lengths/link classes and every payload byte are checked. Current format
is validated by current public readers, with no retired encoding substitution.
This is an offline oracle after known Stop, with no live host data path or new
product encoder. Unreferenced-object census/encoding identity are not comparison
claims. The source review records these topology/grammar differences explicitly.
The copied 45 s work/59 s hard verification scope is frozen before sampling;
product 15 s is a reported soft limit. The existing 120 s operation watchdog is
unchanged; every invocation additionally has the checkout's explicit 120 s stop.
Class B eligibility is checked against actual measured object demands after
warm terminal cleanup; any miss remains INELIGIBLE. Functionally passing samples
are removed after both proofs and confirmed Stop while retaining their numeric
eligibility and complete raw evidence. Failed/uncertain resources remain owned.

Two external persisted-proof tests pass in 1.72 s after locked --no-run build,
covering full bytes/names and corruption with otherwise matching metadata.
The tests filter workspace_graph_ so the 19 historical inline tests are not
executed; bounded child waits prevent indefinite hangs. Twenty-four final-source
engine tests pass and three catalogue tests pass, under separate 120 s locks.
Rust formatting/whitespace pass on owned files with external children skipped.
Raw checks: `benchmark-results/fs-bench-pro/2026-10-10-batch-1-payload-harness-checks/`.
No product change, new dependency or product correctness bug is claimed. No
payload build/preparation/parity/sample has run yet; registration is committed
first. Production LOC 79,366 -> 79,366 (delta +0), same pinned absent-aware method.

| `200f81493` | Port payload definitions and complete workspace proof support | 79366 | 79366 | +0 |

Batch 1 parallel routing: the owner's follow-up authorizes two new isolated
worktrees and subagents for tiny_file_churn and namespace_mutation. They own
only their respective family definitions/tests/CSV outputs; the lead owns the
shared engine, product fixes, integration and master ledger. They must finish
within batch 1 before later batches begin. Each worktree uses its own ignored
lock script/path and Cargo targets; passing sample resources are removed only
by their creating run. Protected existing worktrees, containers, volumes and
inputs remain untouched. Local worker commits are integrated onto main with
the required unchanged-product LOC comparison and attribution.

The existing generic workspace runner/preparer/parity/retained-proof routes
now accept the two remaining batch-1 ordinary syscall families once their
committed cases become ported. Pending selections remain refused. No workload,
verifier, budget or product behavior changes. Source receipts additionally
record the explicit LAYERFS_BENCH_INTERFERENCE declaration; the three campaign
worktrees share the Docker VM and host and make no isolated timing claim.

The payload release build, all eight native/Store prepared inputs and all
eight parity cases passed before this routing change. The sealed binaries
can be reused because no compiled input changed; their archive and build
identity remain the original 200f81493 receipt. No payload sample has run.
Raw setup: benchmark-results/fs-bench-pro/2026-10-10-batch-1-payload-build/,
2026-10-10-batch-1-payload-prepared/ and 2026-10-10-batch-1-payload-parity/.
Prepared master volumes are listed in each append-only prepared.json.

Final-source scoped engine/catalogue checks are recorded with this commit.
Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware method;
reference absent (retired R9c; last counted 65,417 at tree 498dd1917812).

| `b3c96ecb1` | Enable isolated concurrent batch-1 family work and declare interference | 79366 | 79366 | +0 |

Batch 1 / payload_create_read complete: all eight original cases prepared and
parity PASS; every case sampled once at clean b3c96ecb1 with sealed binary
reuse from the payload build at 200f81493. All eight functional/native/persisted
proofs PASS, exact acknowledged Commit/UpToDate roots match, terminal measured
and verifier mounts reach Gone, and Stop/container state is confirmed. All
eight passing sample containers/volumes were removed by their creating run.
No product correctness bug surfaced. Full allocated/logical bytes and numeric
counts are retained, without speed judgment or comparison verdict.

Numeric class B eligibility remains separate: the four creation cases each
record three measured source demands and the largest random-read case records
657; their rows are INELIGIBLE. The other three random-read cases record zero
and are PASS. All eight complete walls are WITHIN the unchanged 15 s reported
limit and all separate proofs are WITHIN the 45 s work / 59 s hard limits.
The unmodified actual demand guard is never bypassed or retried. All three
authorized worktrees share host/Docker VM resources and retained protected
services; source receipts contain the explicit interference declaration.

| Case | Functional / proof | Numeric | Complete wall ns | Proof wall ns | Requests / jobs / statements | Store logical / allocated B | Overlay logical / allocated B | Source demands / close |
| --- | --- | --- | ---: | ---: | --- | --- | --- | --- |
| payload-create-1m-compact-v2 | PASS / PASS | INELIGIBLE | 181987875 | 1486514208 | 22 / 241 / 1178 | 1411992 / 1413120 | 1294336 / 269729792 | 3 / Gone; KNOWN_STOP |
| payload-create-10m-compact-v2 | PASS / PASS | INELIGIBLE | 297180584 | 1799140334 | 94 / 1177 / 6290 | 12738560 / 12738560 | 10895360 / 279330816 | 3 / Gone; KNOWN_STOP |
| payload-create-100m | PASS / PASS | INELIGIBLE | 1275518541 | 3791561250 | 814 / 10537 / 57410 | 109649920 / 109649920 | 106856448 / 375291904 | 3 / Gone; KNOWN_STOP |
| payload-create-500m | PASS / PASS | INELIGIBLE | 9449516000 | 11953190375 | 4014 / 52137 / 284610 | 534945792 / 534953984 | 533385216 / 801824768 | 3 / Gone; KNOWN_STOP |
| payload-random-read-1-compact-v2 | PASS / PASS | PASS | 178474292 | 1471486375 | 6 / 52 / 236 | 1327496 / 1331200 | 229376 / 268664832 | 0 / Gone; KNOWN_STOP |
| payload-random-read-10-compact-v2 | PASS / PASS | PASS | 160773916 | 1673532375 | 15 / 61 / 254 | 10887584 / 10891264 | 229376 / 268664832 | 0 / Gone; KNOWN_STOP |
| payload-random-read-100 | PASS / PASS | PASS | 171707875 | 11578540166 | 105 / 151 / 434 | 530792864 / 530796544 | 229376 / 268664832 | 0 / Gone; KNOWN_STOP |
| payload-random-read-500 | PASS / PASS | INELIGIBLE | 318755417 | 12251908917 | 503 / 550 / 1232 | 530792864 / 530800640 | 229376 / 268664832 | 657 / Gone; KNOWN_STOP |

The two CSVs under baselines/core/2026-10-10-b3c96ecb1/ contain every one of
the eight cases beside its released reference provenance. bench.py verify
checks retained original evidence hashes and reports eight PASS without
replaying any product operation. Raw samples:
benchmark-results/fs-bench-pro/2026-10-10-batch-1-payload-b3c96ecb1/.
Verify/report stdout/stderr and template reads:
benchmark-results/fs-bench-pro/2026-10-10-batch-1-parallel-routing-checks/.
A mistyped short read-case CLI ID was refused before selection or any sample
attempt; the correct original ID then received its sole sample. That refusal
is retained in refused-short-case-id.json, with zero product/mount attempts.

Prepared immutable masters remain owned setup for affected-case source-change
reuse, and are never measured directly:

| Prepared case | Owned master volume |
| --- | --- |
| payload-create-1m-compact-v2 | layerfs-fsb-master-payload-create-read-382a946a21d44b51 |
| payload-create-10m-compact-v2 | layerfs-fsb-master-payload-create-read-9a9f6e961def4a02 |
| payload-create-100m | layerfs-fsb-master-payload-create-read-3a2c1e8cf03b4064 |
| payload-create-500m | layerfs-fsb-master-payload-create-read-1abdd3a8148a4972 |
| payload-random-read-1-compact-v2 | layerfs-fsb-master-payload-create-read-e148a8fb9ca84eb1 |
| payload-random-read-10-compact-v2 | layerfs-fsb-master-payload-create-read-390b40805ee04325 |
| payload-random-read-100 | layerfs-fsb-master-payload-create-read-b8fab313624045c7 |
| payload-random-read-500 | layerfs-fsb-master-payload-create-read-b869121a3e1143e5 |

The attached worktrees are
/Users/yifanxu/.codex/worktrees/fs-bench-pro-tiny-file-churn/layerfs and
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs,
both created from b3c96ecb1. Their own ignored lock scripts point to their
respective core/target/r4.lock files. The workers reuse immutable verified
binary archives, without sharing writable Cargo targets. They own no product
changes. Tiny registration c4c137c13 and namespace registration dac8212de
are locally committed in their respective codex/ branches and remain the
actual fixed preparation/sample identities even after later main integration.
Their results are pending; no later batch has started.

Unchanged engine/runtime scoped checks from the port and routing commits are
reused; this commit changes only results/documents. No new failed or uncertain
sample resources were created. H-1/C10/C12 and all protected resources remain
untouched. Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware
method; reference absent (retired R9c; last counted 65,417).

| `c1d9b6c58` | Record eight payload samples and generated CSVs | 79366 | 79366 | +0 |

Batch 1 / tiny_file_churn registration integrated locally onto main from
worker commit c4c137c13. All twenty original cases retain their exact compact,
mixed-v3 and mixed-v4 IDs/tiers, seed1, original dense ordinary syscall body,
normalization and filesystem requests. Each has one public Commit after the
ordinary call; tiny-stat expects UpToDate. The port explicitly declares that
the complete original native semantic tree plus persisted current-format public
reader proof is stronger than the historical selected tiny/mixed path/range
coverage. The original reported/work/hard/operation limits are unchanged.
No product source or shared engine change accompanies this integration.

Worker source reviews match frozen file hashes against reference-v0.1.6-final.
Its catalogue three tests PASS in 4.105 s under its own 120 s lock; bash syntax
and whitespace checks PASS. Exact staged/committed production tree comparison
was verified for c4c137c13. All twenty original native inputs/final states and
public Init/seal/install masters prepared PASS, and parity PASS twenty cases
in at most 17 s with zero samples. Fixed preparation/sample source remains
c4c137c13; main integration does not relabel or rerun those receipts.

Worker raw evidence and owned resources remain under
/Users/yifanxu/.codex/worktrees/fs-bench-pro-tiny-file-churn/layerfs/benchmark-results/fs-bench-pro/.
Prepared and parity roots are 2026-10-10-batch-1-tiny-prepared/ and
2026-10-10-batch-1-tiny-parity/. Every master volume name/digest is in its
prepared.json and resource-claim.json; only the worker's passing sample
resources are removed. It declares shared host/VM interference in every
source receipt and reuses immutable sealed binaries with matched compilation
inputs. Samples/results are in progress, so the family is not yet complete.
This integration preserves the worker's exact definition/workload bytes and
reuses its qualifying scoped evidence; no unchanged sample is repeated.
Production LOC 79,366 -> 79,366 (delta +0), same pinned absent-aware method.

| `c4c137c13` (worker) | Port twenty tiny-file cases at isolated worktree source | 79366 | 79366 | +0 |
| `36849bcc2` | Integrate tiny-file registration with master ledger | 79366 | 79366 | +0 |

Batch 1 / namespace_mutation registration integrated locally onto main from
worker dac8212de. All four original compact-v2/mixed-v4 IDs, fixtures, tiers,
seed1, original subtree rename/delete/normalization/sync requests and known
Committed outcomes are preserved. The complete mounted/persisted byte/tree
proof is explicitly declared as stronger than the historical mixed-v4 selected
paths/ranges and moved/deleted witnesses. Original report/work/hard/operation
limits remain 15/45/59/120 seconds. No product/shared engine changes.

Worker qualified the compact native rename/deletion transcript and corruption
rejection with two scoped tests PASS in 0.134 s under its own 120 s lock; three
catalogue tests PASS in 3.297 s. The initial test-only expectation used the
word bytes while the verifier emits native content mismatch; native.stderr
retains that failure. The final assertion strengthens matching to the exact
content-mismatch path and offset; native-fixed.stderr retains PASS. No workload
or verifier was weakened. All four original native input/final-state checks,
public Init/seal/install and parity PASS at fixed dac8212de.

The worker has finished four single samples with both independent proofs,
Gone and KNOWN_STOP. Results are integrated separately; existing samples and
receipt identities are never changed or replayed. Full raw qualification,
preparation, parity and measured evidence remains in its attached worktree
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/benchmark-results/fs-bench-pro/.
This integration preserves its exact registered bytes and reuses those scoped
qualifications. Production LOC 79,366 -> 79,366 (delta +0), pinned absent-aware
method; reference absent (retired R9c, last counted 65,417).

| `dac8212de` (worker) | Port four namespace mutation cases and scoped qualification | 79366 | 79366 | +0 |
| `98b14b233` | Integrate namespace registration with master ledger | 79366 | 79366 | +0 |

Batch 1 / namespace_mutation complete: worker results commit 3eaf14d95 is
integrated on main with all four original cases present in both CSVs under
baselines/core/2026-10-10-dac8212de/. All four original single samples at
clean dac8212de completed ordinary workload/public Commit, both full mounted
native and closed persisted-root independent proofs, exact acknowledged root,
terminal Gone and KNOWN_STOP. All functionally passing sample containers and
volumes, and passing setup/copy containers, were removed by their creating
worker. No product correctness bug, failed/NA case or limit exceedance surfaced.
All four numeric rows remain INELIGIBLE because exactly three actual measured
source-object demands violate the strict class B zero-demand guard. No
checker, workload, limit, source identity or receipt is changed to make PASS.

| Case | Functional / proof | Numeric | Complete wall ns | Proof wall ns | Requests / jobs / statements | Store logical / allocated B | Overlay logical / allocated B | Close |
| --- | --- | --- | ---: | ---: | --- | --- | --- | --- |
| namespace-subtree-relocate-delete-1-compact-v2 | PASS / PASS | INELIGIBLE | 185745666 | 1910991458 | 129 / 890 / 5287 | 938736 / 942080 | 245760 / 268685312 | Gone; KNOWN_STOP |
| namespace-subtree-relocate-delete-10-compact-v2 | PASS / PASS | INELIGIBLE | 279461750 | 1783612417 | 1092 / 7581 / 45196 | 1483504 / 1486848 | 380928 / 268820480 | Gone; KNOWN_STOP |
| namespace-subtree-relocate-delete-100-mixed-v4 | PASS / PASS | INELIGIBLE | 231905500 | 6029095334 | 832 / 5738 / 34262 | 106984296 / 106987520 | 360448 / 268800000 | Gone; KNOWN_STOP |
| namespace-subtree-relocate-delete-500-mixed-v4 | PASS / PASS | INELIGIBLE | 678906875 | 17898625750 | 4072 / 27726 / 166750 | 534258584 / 534261760 | 802816 / 269238272 | Gone; KNOWN_STOP |

bench.py verify checks all four retained independent records and immutable
original manifests PASS without replaying a product operation. All clocks,
phase counts, original reference rows, helper/fixture/source/binary/image
identities and removal receipts remain in the attached worker raw tree:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/benchmark-results/fs-bench-pro/2026-10-10-batch-1-namespace-dac8212de/.
Preparation, parity and tests are in the sibling -prepared/, -parity/ and
-tests/ roots. Root-ready exact handoff is /tmp/fsbench-namespace-handoff.json.
The four immutable preparation masters remain retained for valid source-change
reuse, never direct measurement: layerfs-fsb-master-namespace-mutation-
b9ef36d8567e402d, e3115545ed674d03, 245fb63fd78946b3 and 29a2b1062e35457b
(each suffix uses that full prefix). Ignored receipts must be preserved before
any managed archive; no archive or deletion has been performed.

All samples explicitly declare shared host/Docker VM interference and make
no isolated timing, speed comparison or release-admission claim. The passed
scoped qualifications from registration are reused; this integration changes
only generated results/documents. Production LOC 79,366 -> 79,366 (delta +0),
same pinned absent-aware method.

| `3eaf14d95` (worker) | Record four namespace samples and generated CSVs | 79366 | 79366 | +0 |
| `be1f56a23` | Integrate complete namespace outcomes with master ledger | 79366 | 79366 | +0 |

H-2 provenance correction: all 36 currently ported batch-1 review entries
labelled annotated recovery tag object ae3310fada4a1fe35cb99ab02270b7538a0d03f7
as source_commit. git cat-file confirms type tag. Peeling that same unchanged
reference resolves actual commit d296981ace76fe751bebf00dee52818fe18a3eef.
The reviewed file hashes were obtained through git show on the tag and are
correct; neither frozen source bytes nor any workload, verifier or product
operation was wrong. The two parity routes compared the misleading label
to the tag object, admitting the wrong object type.

Before the fix, three external scoped regression methods fail (38 assertions
and two errors): both parity routes admit a tag as a commit, reject a valid
peeled commit, and every ported review has the wrong label. The exact failed
stdout/stderr is retained under benchmark-results/fs-bench-pro/
2026-10-10-review-pin-fix-checks/before.*. The fix uses explicit ^{commit},
retains source_tag_object separately in every review, and validates both
objects in parity. The namespace qualification's pin assertion is updated
to the true commit. All original reviewed hashes, steps, budgets, cache
contracts, oracle scope and cases remain unchanged. Frozen root and closed
records/registrations/receipts remain unchanged. Active tiny samples continue
at their original isolated c4c137c13 source with no metadata/source mutation.
No unchanged measured case is replayed for this labeling correction.

At final changed source, all 27 engine tests PASS in 0.512 s; the changed
namespace pin qualification PASS in 0.012 s; catalogue three tests PASS in
0.114 s. Each invocation has its own 120 s lock/wall stop. The unaffected
native transcript/corruption test is reused from its passing registration
evidence. This is Python/JSON harness metadata only, with unchanged product
and compiled inputs; no product regression, Linux product build, Clippy or
boundary claim is manufactured. Corrected parity evidence will be recorded
without any product sample replay. Production LOC 79,366 -> 79,366 (delta +0),
pinned absent-aware method; reference absent (retired R9c, last counted 65,417).

| `222526fa0` | Correct annotated recovery tag/source commit review identity | 79366 | 79366 | +0 |

Batch 1 / tiny_file_churn complete: worker results commit 323cd6dfa is
integrated on main with every original case in both twenty-row CSVs under
baselines/core/2026-10-10-c4c137c13/. All twenty cases were sampled once at
clean c4c137c13, passed the full mounted native and closed persisted semantic
proofs with exact acknowledged Commit/UpToDate root, and ended both measured
and verifier mounts with Gone and KNOWN_STOP. All passing sample containers/
volumes and setup/copy containers were removed by their creating worker.
There was no product bug, failed/NA case or exceeded product/proof limit.

All four tiny-stat rows are numeric PASS with zero source demands. Eight
tiny-create/unlink rows are INELIGIBLE with three actual demands each; eight
bulk-create/delete rows are INELIGIBLE with four each. The strict guard remains
unchanged; completed work/clean proofs remain functional PASS independently.
No case was repeated to obtain a different cache observation.

| Case | Functional / proof | Numeric | Complete wall ns | Proof wall ns | Requests / jobs / statements | Store logical / allocated B | Overlay logical / allocated B |
| --- | --- | --- | ---: | ---: | --- | --- | --- |
| tiny-create-1-compact-v2 | PASS / PASS | INELIGIBLE | 172780583 | 2170305458 | 16 / 140 / 645 | 1405656 / 1409024 | 229376 / 268664832 |
| tiny-create-10-compact-v2 | PASS / PASS | INELIGIBLE | 171092084 | 2549466792 | 115 / 956 / 4452 | 11084912 / 11087872 | 266240 / 268701696 |
| tiny-create-100-mixed-v4 | PASS / PASS | INELIGIBLE | 270797708 | 5553232500 | 736 / 5745 / 27408 | 106945928 / 106946560 | 548864 / 268984320 |
| tiny-create-500-mixed-v4 | PASS / PASS | INELIGIBLE | 566336917 | 16850427292 | 3496 / 28841 / 134888 | 534873568 / 534876160 | 1847296 / 270282752 |
| tiny-stat-1-compact-v2 | PASS / PASS | PASS | 180196833 | 2107919500 | 4 / 51 / 249 | 1347976 / 1351680 | 229376 / 268664832 |
| tiny-stat-10-compact-v2 | PASS / PASS | PASS | 203780416 | 2550262542 | 22 / 69 / 501 | 10985888 / 10989568 | 262144 / 268697600 |
| tiny-stat-100-mixed-v4 | PASS / PASS | PASS | 199141125 | 5670476250 | 112 / 159 / 1671 | 107327904 / 107331584 | 389120 / 268824576 |
| tiny-stat-500-mixed-v4 | PASS / PASS | PASS | 255376583 | 16564206042 | 512 / 559 / 6871 | 532681120 / 532684800 | 598016 / 269033472 |
| tiny-unlink-1-compact-v2 | PASS / PASS | INELIGIBLE | 193983458 | 2233676000 | 12 / 122 / 594 | 1372696 / 1376256 | 229376 / 268664832 |
| tiny-unlink-10-compact-v2 | PASS / PASS | INELIGIBLE | 191469000 | 2653523250 | 75 / 608 / 3301 | 11018848 / 11022336 | 262144 / 268697600 |
| tiny-unlink-100-mixed-v4 | PASS / PASS | INELIGIBLE | 233279625 | 5783532667 | 435 / 3035 / 17991 | 107435168 / 107438080 | 389120 / 268824576 |
| tiny-unlink-500-mixed-v4 | PASS / PASS | INELIGIBLE | 410351250 | 16350211959 | 2035 / 13939 / 83655 | 532792048 / 532795392 | 565248 / 269000704 |
| tiny-bulk-create-1-compact-v2 | PASS / PASS | INELIGIBLE | 366151625 | 2475651167 | 1429 / 15063 / 65179 | 1426280 / 1429504 | 1495040 / 269930496 |
| tiny-bulk-create-10-compact-v2 | PASS / PASS | INELIGIBLE | 837759583 | 3245791000 | 4651 / 43140 / 196321 | 13705216 / 13705216 | 11735040 / 280170496 |
| tiny-bulk-create-100-mixed-v3 | PASS / PASS | INELIGIBLE | 1918278000 | 5651561541 | 8709 / 80032 / 375318 | 111820800 / 111820800 | 108724224 / 377163776 |
| tiny-bulk-create-500-mixed-v3 | PASS / PASS | INELIGIBLE | 13508792208 | 18308321000 | 39665 / 351248 / 1669693 | 536412160 / 536416256 | 541491200 / 809930752 |
| tiny-bulk-delete-1-compact-v2 | PASS / PASS | INELIGIBLE | 318064125 | 2024159625 | 1145 / 9966 / 53086 | 1422088 / 1425408 | 348160 / 268783616 |
| tiny-bulk-delete-10-compact-v2 | PASS / PASS | INELIGIBLE | 509469958 | 2184369209 | 3012 / 22772 / 129867 | 11039520 / 11042816 | 634880 / 269070336 |
| tiny-bulk-delete-100-mixed-v3 | PASS / PASS | INELIGIBLE | 761838833 | 2889839417 | 4987 / 36093 / 210354 | 107774848 / 107778048 | 913408 / 269348864 |
| tiny-bulk-delete-500-mixed-v3 | PASS / PASS | INELIGIBLE | 2643373875 | 5621920792 | 21187 / 146056 / 872798 | 533111632 / 533114880 | 3248128 / 271683584 |

The largest complete product wall is 13,508,792,208 ns for original
tiny-bulk-create-500-mixed-v3 (15,000,000,000 ns reported limit), with separate
proof 18,308,321,000 ns (45,000,000,000 ns work / 59 s hard limit). Its
39,665 requests / 351,248 jobs / 1,669,693 statements are recorded without
judgment or comparison. Every row includes its original released reference
provenance. Raw preparation/parity/run receipts remain under
/Users/yifanxu/.codex/worktrees/fs-bench-pro-tiny-file-churn/layerfs/benchmark-results/fs-bench-pro/
2026-10-10-batch-1-tiny-prepared/, -parity/ and -c4c137c13/ respectively.
The twenty immutable prepared masters are listed explicitly below; they remain
closed reusable setup, never direct measurement:

| Prepared case | Owned master volume |
| --- | --- |
| tiny-create-1-compact-v2 | layerfs-fsb-master-tiny-file-churn-d8611b3edb224292 |
| tiny-create-10-compact-v2 | layerfs-fsb-master-tiny-file-churn-4b582e0f3a314380 |
| tiny-create-100-mixed-v4 | layerfs-fsb-master-tiny-file-churn-9aeb5aba230e420e |
| tiny-create-500-mixed-v4 | layerfs-fsb-master-tiny-file-churn-6e39c5ff6dd247a3 |
| tiny-stat-1-compact-v2 | layerfs-fsb-master-tiny-file-churn-1dbdf1f9afe5439f |
| tiny-stat-10-compact-v2 | layerfs-fsb-master-tiny-file-churn-a329b9fd36d54027 |
| tiny-stat-100-mixed-v4 | layerfs-fsb-master-tiny-file-churn-d89da5abc69e450f |
| tiny-stat-500-mixed-v4 | layerfs-fsb-master-tiny-file-churn-307f1d1ac9d7400e |
| tiny-unlink-1-compact-v2 | layerfs-fsb-master-tiny-file-churn-6a25097b09874030 |
| tiny-unlink-10-compact-v2 | layerfs-fsb-master-tiny-file-churn-262566f5b9ae4d01 |
| tiny-unlink-100-mixed-v4 | layerfs-fsb-master-tiny-file-churn-f1511e0f2f824988 |
| tiny-unlink-500-mixed-v4 | layerfs-fsb-master-tiny-file-churn-93b0b33df1684caa |
| tiny-bulk-create-1-compact-v2 | layerfs-fsb-master-tiny-file-churn-9e8ec14e26e1446b |
| tiny-bulk-create-10-compact-v2 | layerfs-fsb-master-tiny-file-churn-1db068255c914185 |
| tiny-bulk-create-100-mixed-v3 | layerfs-fsb-master-tiny-file-churn-fe3860c319e64160 |
| tiny-bulk-create-500-mixed-v3 | layerfs-fsb-master-tiny-file-churn-254ce6c1f22543d6 |
| tiny-bulk-delete-1-compact-v2 | layerfs-fsb-master-tiny-file-churn-13f68d6d9c8d49fd |
| tiny-bulk-delete-10-compact-v2 | layerfs-fsb-master-tiny-file-churn-42fdc1d872284079 |
| tiny-bulk-delete-100-mixed-v3 | layerfs-fsb-master-tiny-file-churn-92c7d60100c841a9 |
| tiny-bulk-delete-500-mixed-v3 | layerfs-fsb-master-tiny-file-churn-37f4306ee67a4e15 |

bench.py report validates the original retained manifests and emits all twenty
performance/proof rows without product replay. The worker's original scoped
catalogue/syntax qualification is reused; this integration changes only CSVs/
documents. Ignored raw evidence is retained in its attached worktree. No
archive/deletion of that evidence or any protected resource has occurred.
Worker cohorts and old review-tag labels remain immutable in their receipts;
main's H-2 correction changes only prospective metadata and strict parity
validation, with no measured case replay. Corrected parity for all four
batch-1 families is the remaining batch checkpoint before batch 2 starts.
Production LOC 79,366 -> 79,366 (delta +0), same pinned absent-aware method.

| `323cd6dfa` (worker) | Record twenty tiny-file samples and generated CSVs | 79366 | 79366 | +0 |
| `d6e0ef952` | Integrate complete tiny-file outcomes with master ledger | 79366 | 79366 | +0 |

Batch 1 closed: all four families meet the done-when list with every original
case ported, parity PASS, one functional/proof/clean-close sample each, and
the two complete CSVs per original source run integrated on main. There are
36 cases: Init4, payload8, tiny20 and namespace4. Seven numeric rows are PASS
and 29 INELIGIBLE; no numeric eligibility is upgraded by functional completion.
All registered product/proof limits are WITHIN. Every passing mounted sample
resource was removed by its creating run; prepared masters and all ignored raw
evidence remain retained and explicitly owned. No product bug or unrun/failed/
not-applicable pilot case remains. H-1 and H-2 are fixed harness faults; stage
0.4's unchanged C10/C12 exact-count failures remain historical FAIL.

After H-2, at clean d6e0ef952 each corrected strict parity route passed once
without any new product sample: Init4 (13 s wall stop receipt), payload8
(5 s), tiny20 (18 s), namespace4 (individual 120 s wall stop). Each consumes
its previously sealed original native fixture/final-state evidence, validates
all reviewed source hashes and both exact source commit/tag objects, and
records zero samples. Raw roots are benchmark-results/fs-bench-pro/
2026-10-10-review-pin-init-parity/, -payload-parity/, -tiny-parity/ and
-namespace-parity/. These are metadata-check observations, with no performance
claim. The complete failed H-2 regression receipt remains under
2026-10-10-review-pin-fix-checks/before.*.

Batch 2 can now begin. The owner-authorized workers will be reused for
separate original edit families while the lead owns shared exact-write
machinery, production bugs and ledger/integration. Their pilot branches and
ignored receipt trees remain intact; later family work uses new local
branches from the completed main checkpoint, without resetting saved work.
No later batch may start until batch 2's complete outcome is recorded.
This closure changes documents only, reusing all final-source scoped checks.
Production LOC 79,366 -> 79,366 (delta +0), same pinned absent-aware method;
reference absent (retired R9c, last counted 65,417).

| `d4314ed62` | Close all four pilot families and corrected source-pin parity | 79366 | 79366 | +0 |

Batch 2 / shared native edit model: the original SDK scenario registries and
payload algebra now define the same native payload.bin input/final trees for
preserving, changing, canonical-count and capped families. Original start/
delete/replacement lengths, existing capped inputs, payload generators/digests
and exact final lengths are reused directly from frozen product-neutral Rust.
The native fixture recipe accepts these cases through one shared adapter; it
keeps the original full materialization/metadata/manifest/digest checks and
explicitly validates the original fixture byte SHA. Existing ordinary cases
continue through their unchanged original expected-state route. No product
package or retired encoder is compiled or restored.

The host native tool compiles with rustup1.85.1 and all repository ARM64 flags
under the main120s lock (4 s). Its native 1MiB overwrite-head fixture and
independent expected state qualify: exact original fixtureSHA
d7dfe3d2828aceb85177e6efbeb600f23672a326c902e525e401c1545bb05bdc,
replacementSHA faca857b3e7f8b1b8f46c19b1625a1b9995248ae9ac85eae672b85fbc9932375,
all 1,044,480 suffix bytes unchanged, root0750/file0640 and fixed mtime
1700000000000000000 ns. This is native-only preparation/model qualification,
with no Store, SDK call, mount or measured product sample. Raw evidence:
benchmark-results/fs-bench-pro/2026-10-10-batch-2-edit-model-checks/.

Three isolated workers author preserving12, changing32 and capped5 cases.
The added managed capped worktree is
/Users/yifanxu/.codex/worktrees/fs-bench-pro-edit-capped/layerfs, from
d4314ed62, with its own ignored lock script/targets. Existing pilot worktrees
use new codex/ edit branches from that same checkpoint while retaining pilot
branches and every ignored receipt/master. The preserving worker additionally
owns only shared writers/edit.rs and its native qualification test; the
changing worker additionally owns only new runtime/src/edit_oracle.rs and
external tests for current untouched-payload/source-offset preservation and
complete inode-table membership. The lead owns model/engine/runtime integration,
production bugs and ledger. This independent ownership avoids overlap while
shared source is composed. All dependent setup/sampling remains unattempted.

Original capped-v1 has a two-second SDK-edit+Commit+End group watchdog. It will
remain one two-second group for the ordinary edit+Commit+terminal unmount,
excluding setup/warmup/mount/proof, with actual expiration retained. It is
never substituted with a120s operation window. Standard SDK fixture rootmtime
was not fixed in the old native preparation, so deterministic rootmtime is an
explicit difference for the three direct SDK families. Capped inherited
preparation already fixed rootmtime; it needs no such difference. All input
root modes are0750 and payload modes0640. A mistaken pre-sample reviewer
assumption of0755 was checked against both frozen sources and actual native
stat, withdrawn and caused no source change or sample.

The capped review identified original complete inode-index membership beyond
reachable paths. The lead will restore that current-format check and audit
retained pilot Store exports separately, without replaying measurements or
altering sealed receipts; no completeness claim is silently widened.
Final shared binary/native/runtime qualifications and case parity await the
writer/oracle integration; none is claimed complete here. Production LOC
79,366 -> 79,366 (delta+0), same pinned absent-aware method; reference absent
(retired R9c, last counted65,417).

| `e2840c270` | Add original SDK-edit native fixture and expected-tree algebra | 79366 | 79366 | +0 |

Batch 2 / edit_length_preserving registration integrated from worker98fa838f4.
All twelve original head/middle/tail-by-tier cases preserve exact fixture
bytes, range starts, 4096-byte deletion/replacement, payload seeds/digests and
original plan/registry seals. The original standard native file0640, root0750
and filemtime are pinned; rootmtime normalization is an explicit topology
difference. The thin script uses the shared exact edit writer; no SDK/product
implementation is placed in the family. Every proposed current verifier
retains inode identity, full bytes/current grammar and applicable untouched
current payload IDs; retired source-format and SDK-only zero-FUSE/resource
fields remain explicit differences, never fabricated current zero values.

Worker scoped original registry/provenance/fixture qualification PASS in
3.416s, catalogue three PASS in0.134s; syntax/whitespace PASS, all under its
120s wall/own lock. Registration commit's exact parent/staged/committed
production comparison is79,366->79,366+0. Qualifying unchanged definitions/
checks are reused on integration. Shared final compile/verification, new
prepared inputs, parity, all twelve samples and CSVs remain pending; no
sample or mounted resource has been created for this family. Raw checks:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-tiny-file-churn/layerfs/benchmark-results/fs-bench-pro/2026-10-10-batch-2-length-preserving-tests/.
No product source or workload limit changed. Production LOC79,366->79,366
(delta+0), same pinned absent-aware method; reference absent retiredR9c.

| `98fa838f4` (worker) | Port twelve exact length-preserving scenarios | 79366 | 79366 | +0 |
| `5e8dc5fd6` | Integrate length-preserving registration and master ledger | 79366 | 79366 | +0 |

Batch 2 / edit_length_changing registration integrated from681413760.
All32 original plans and their ordered positional edit remain frozen, including
five original growing500MiB capped-v2 IDs/inputs already defined by registry().
Their final length is exactly500MiB; no new cap is added. Insert/delete/append/
prepend/grow/shrink/truncate/zero-extend offsets, payload seeds/digests, six
original input byte hashes and independent plan seals are retained. Every
sdk_scenario field independently reproduces the frozen registry TSV digest
f59a1a68a19b95e1dcc8e6e5e273c7279c69fca373adca9980a9a9ffbd9fa517.

Worker two scoped plan/provenance/capped-input tests PASS0.042s, catalogue
three PASS0.115s under its own120s lock; syntax/whitespace PASS. No input,
Store, mount or sample exists yet; original checks are reused for this byte-
identical integration. Explicit differences record P-3 ordinary suffix movement/
pwrite/truncate, deterministic rootmtime, actual cache/route/resource scope and
current typed grammar/retention identifiers rather than retired format fields.
Final shared runtime/writer build, fixture/parity,32 samples and both CSVs
remain pending. Raw worker checks:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/benchmark-results/fs-bench-pro/2026-10-10-batch-2-length-changing-tests/.
No product or budget change. Production LOC79,366->79,366(delta+0), pinned
absent-aware method; reference absent retiredR9c.

| `681413760` (worker) | Port32 original length-changing scenarios and plan checks | 79366 | 79366 | +0 |
| `cbdff2006` | Integrate length-changing registration and master ledger | 79366 | 79366 | +0 |

Batch 2 / edit_length_changing_capped registration integrated from worker
d03f8435b and prospective coverage/watchdog follow-up90029a1fe. Five original
capped-v1 cases retain exact existing input prefixes (524,283,904 bytes, or
524,285,952 for replace-grow), all final lengths524,288,000, original seeds/
payloads/plans/fixture hashes and fixed0750/0640/1700000000 metadata. No
fixture, workload, limit or verifier is shrunk. The two-second group watchdog
for edit+Commit+End remains an active exact group budget, distinct from15s
reported full wall and45/59s independent proof limits. A refused operation or
expiry retains its original receipt/resources, without enlarging a deadline.

Current proof obligations include stable scope/inode serial, all bytes,
untouched current payload IDs and shifted source-offset transcript, current
format grammar and complete canonical inode-table membership. Retired
encoding/root IDs and unreferenced non-inode object census are explicitly
not compared. Capped original inherited fixture already fixed rootmtime, so
no artificial root-time difference is introduced. Frozen full digest model
self_check passes independently; computed original final fileSHA values are
retained in the worker's native-model diagnostic as derivations, not product
samples. Four final prospective contract checks PASS0.020s under own30s
lock, whitespace PASS; earlier raw contract checks retained. Exact worker
parent/staged/committed LOC comparisons are79,366->79,366+0.

Raw checks/model diagnostic:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-edit-capped/layerfs/benchmark-results/fs-bench-pro/2026-10-10-batch-2-capped-registration/ and
2026-10-10-batch-2-capped-native-model-diagnostic/. No new container, volume,
Store, mount, preparation or product sample was created. Shared integration/
final qualification, five inputs/parity/samples/CSVs remain pending. This
integration reuses byte-identical qualification; no product change.
Production LOC79,366->79,366(delta+0), pinned absent-aware method.

| `d03f8435b` (worker) | Port five original capped-v1 edit scenarios | 79366 | 79366 | +0 |
| `90029a1fe` (worker) | Freeze active original2s group/current oracle obligations | 79366 | 79366 | +0 |
| `b9f955976` | Integrate capped registration and exact prospective constraints | 79366 | 79366 | +0 |

Batch 2 / shared ordinary edit writer integrated from cdf7fa635 and test-only
exact error attribution d7cce5759. One pure std Unix program executes all four
SDK scenario registries: original exact ranges/payloads,64KiB scratch, backward
suffix movement on growth, forward on shrink, exact final truncate, metadata
normalization and consumed checked close. Same-length overwrite moves no suffix.
Errors/zero IO preserve cause; successful short IO makes byte progress without
resending an error. No new fsync request, product API/encoder or dependency is
added. Independent native verification uses the unchanged full original checker
over the original expected algebra, never the mutation algorithm as its oracle.

At composed main source, Rust1.85.1/ARM64 compilation PASS in4s under120s
lock. Two dead-code warnings name model fixture-only helper functions unused
by this writer; they are retained, with no compiler/Clippy-clean claim. A fresh
independent byte copy of the native1MiB input receives one overwrite-head,
preserves native inode and exactly matches every independent expected byte;
its original complete native verifier PASS. No product Store, SDK, mount or
measurement ran. Raw native qualification:
benchmark-results/fs-bench-pro/2026-10-10-batch-2-edit-model-checks/
writer-apply.*, writer-verify.*, writer-native-checked.json. The original
model input/expected receipt trees remain unchanged.

The meaningful sealed-build qualification test covers all11 original small
overwrite/splice shapes, full bytes/metadata/native inode, wrong-length refusal
before effects, and an intentionally corrupted untouched byte. It requires a
matching immutable sealed build; it has not run before that prerequisite.
Writer/provenance syntax/format/whitespace PASS; full shared runtime/build and
final owning suite remain pending. All SDK samples remain unattempted.
Production LOC79,366->79,366(delta+0), pinned absent-aware method.

| `cdf7fa635` (worker) | Add shared positional writer and native qualification | 79366 | 79366 | +0 |
| `d7cce5759` (worker) | Pin exact original content-corruption cause before first test | 79366 | 79366 | +0 |
| `8dd9d99a8` | Integrate bounded positional writer with native source qualification | 79366 | 79366 | +0 |

H-3 current-format proof coverage correction: source review found the original
canonical verifier requires exhaustive inode-table membership, beyond reachable
path traversal. Earlier generic full byte/tree proofs did not enumerate that
table. Their sealed receipts remain unchanged and do not gain an unperformed
check. The integrated current oracle uses public typed rows_after with actual
root context, validates all unique serial rows, and rejects any unreachable,
missing or duplicate membership against the full resolved namespace. Existing
native byte, metadata, alias, root and current-format checks remain intact.
A membership-only offline mode reuses the previously proven bytes and will
audit independent copies of the32 retained pilot Store exports before any
SDK case setup/sample. Those audits are separate changed-oracle verification,
never a measurement replay or rewrite of old receipts. No product defect is
yet established; complete pilot qualification awaits those retained audits.

The SDK offline route now reuses the exact frozen edit model and validates
initial/final current roots: unchanged allocation scope plus inode serial,
original lengths, complete authenticated mapping grammar and parent/child
counts/lengths/levels, old untouched-payload subset rule using current IDs, and
capped strict shifted source-offset transcript for unchanged ranges. Guards
remain strict. A P-3 suffix rewrite may fail a legacy SDK representation
property; any failure will be retained and diagnosed against source/contracts,
without weakening a guard, adding a feature, optimizing storage/speed or
resampling unchanged work. Retired format/root/object IDs are never used as
current expectations.

Worker8e02fbed9's five external tests PASS0.01s after no-run compilation in
its own target, with19 frozen inline tests explicitly filtered. They cover
missing/unreachable table rows, insertion/deletion retention, scope/serial
replacement, capped shifted offsets and forged complete mapping summaries.
Composed main no-run initially failed on a harness ObjectId argument type
(borrow/success propagation), corrected to the actual public from_bytes API.
The host compiler failure is retained in tool output, not claimed a product
bug. Corrected main no-run PASS3.86s; same five scoped tests PASS0.01s under
separate120s locks. Owned rustfmt check PASS with skip_children=true; frozen
reference source is not formatted or modified. Two fixture-only unused helper
warnings remain visible, with no warnings-clean/Clippy claim. Basic persisted
full-byte/corruption tests are also checked at this source. Raw composed
checks: benchmark-results/fs-bench-pro/2026-10-10-batch-2-shared-oracle-checks/.
No new measured case, mount or Docker resource has been created. Production
LOC79,366->79,366(delta+0), pinned absent-aware method; reference absent
retiredR9c,lastcount65,417.

| `8e02fbed9` (worker) | Add typed edit-retention and exhaustive inode membership oracle | 79366 | 79366 | +0 |
| `37ec6826b` | Restore complete current inode membership and SDK edit proofs | 79366 | 79366 | +0 |

H-4 own-write cache classification: the zero source-demand/lifetime guard
was insufficient when Commit consumes newly written backing pages. The
external regression fails before the fix (ELIGIBLE rather than INELIGIBLE),
with exact failed output retained under
benchmark-results/fs-bench-pro/2026-10-10-batch-2-shared-oracle-checks/own-write-before.*.
B/C eligibility now requires explicit knowledge of own-written input; modified
input additionally needs independent actual pre-Commit verified-cold page/
metadata/identity observations with a preceding monotonic timestamp. No
existing observer supplies that proof here, so committed mutations are
INELIGIBLE. Read-only UpToDate cases explicitly declare no own-written input;
actual original zero source demands and cache lifetimes still apply. No
eviction, warm-state change, new hint, limit, workload, request or product
optimization is introduced to obtain eligibility. The seven pilot numeric
PASS rows are read-only UpToDate; all pilot mutations were already INELIGIBLE.
Earlier cell receipts remain unchanged; their missing own-write declaration
is not evidence of numerical cache qualification, regardless of functional
proof/count equality. No speed or release claim is made from them.

Completed counts/storage can also be collected once after a separately failed
closed-Store graph proof, provided Stop is known and telemetry has never been
requested. This is read-only receipt completion after the proof clock ends;
no product operation is replayed, no missing response is guessed and the
original graph failure remains primary. Collection errors are independent
observation failures. Passing-case timer/order remains unchanged. A scoped
test verifies original failure/status remain FAIL while measured counts are
preserved through the read-only collection helper. Ten final workspace
evidence/cache tests PASS0.056s under main120s lock. Raw final test output is
workspace-final.* in the same checks root.

These are stricter harness evidence guards only; product source unchanged.
Shared build/remaining final owning checks and pilot membership audits remain
pending, with no SDK case setup/sample started. Production LOC79,366->79,366
(delta+0), same pinned absent-aware method; reference absent retiredR9c.

| `98b6b7260` | Guard own-write cache qualification and preserve failed-proof metrics | 79366 | 79366 | +0 |


Batch 2 shared execution route: sealed builds now compile/archive both host and
pinned-image edit writers, and compilation reuse binds every current recipe and
writer source. Original native SDK fixtures and expected states use the shared
frozen model; native preflight invokes the original full-byte verifier through
the edit tool, and each prepared helper seal includes the actual Linux writer.
The three committed SDK edit families are routed through the existing ordinary
Bash/Commit path. Capped cases retain their frozen 2,000 ms combined edit/Commit/
End watchdog; command, Commit, observer snapshot and End share one absolute
stop. Per-operation watchdogs never extend that group. Setup/warmup/mount/proof
are outside the original group. No SDK sample has begun.

`bench.py verify <run> --membership-audit <fresh-output> --build <seal>` adds
only the outstanding H-3 census over independent copies of retained stopped
Store exports. It requires the prior byte proof, Gone, known Stop, unchanged
product, sealed prepared manifest, exact original acknowledged root, and
unchanged old evidence hashes after each audit. It records zero samples and
leaves failed audit copies visible. Pilot audits remain pending until the
new sealed build exists; this commit claims no new oracle result.

Scoped final checks under separate 120 s locks: workspace evidence/cache/group
11 tests PASS 0.058 s; fixtures four PASS 0.011 s; offline audit copy/root-binding
one test (two root outcomes) PASS 0.013 s; build identity four PASS at corrected
import scope. The first added build-identity test invocation failed to import
the engine when run alone; use the same explicit module path as the owning
suite, preserving that failure in tool output. It now rejects changed model
and writer inputs before old executable use. Whitespace check PASS. Native
sealed-writer qualification, actual build and pilot census remain pending.
No product change. Production LOC 79,366 -> 79,366 (delta +0), pinned staged
absent-aware method; reference absent (retired R9c; last counted 65,417).

| `ac7beb4dd` | Seal SDK writer routing, frozen group watchdog and offline pilot census | 79366 | 79366 | +0 |


Prospective edit proof declarations integrated from workers `27713d9c7` and
`637d22460`: changing cases explicitly require exhaustive current inode-table
membership, current authenticated mapping grammar and complete summaries,
unchanged allocation scope/serial, and the strict retained-ID subset rule.
Capped cases additionally declare independent pre-Commit own-write cold backing
as an eligibility prerequisite; absence yields INELIGIBLE. Workloads, payloads,
limits, cache treatment and strict checkers do not change. These are pre-sample
metadata corrections, with no prepared SDK fixture or SDK sample yet. Final
owning six changing/capped contract tests PASS under the 120 s lock. Both worker
commits and this integration count unchanged product 79,366 -> 79,366, delta +0;
reference absent, pinned staged absent-aware counter.

| `27713d9c7` (worker) | Declare strict current edit and inode proof scope | 79366 | 79366 | +0 |
| `637d22460` (worker) | Declare independent own-write cache evidence for capped Commits | 79366 | 79366 | +0 |
| `f11ed53d1` | Integrate prospective edit proof and cache declarations | 79366 | 79366 | +0 |


H-3 pilot closure supplement at `f11ed53d1`: all 32 mounted pilot exported
Stores pass exhaustive current inode-table membership, complete paths/kinds/
metadata/aliases and exact original acknowledged root binding. Payload 8,
tiny 20 and namespace 4 audits use independent byte copies; every original
receipt manifest verifies again after its census. No original receipt or CSV
was modified, no case was replayed, no mount or Docker resource was created,
and every audit records zero samples. The four Init-only cases retain their
original selected-prefix verifier and are outside the mounted canonical census.
No product defect was found by this stronger check. Pilot batch completion
now includes the supplemental missing membership proof.

Raw supplemental receipts:
benchmark-results/fs-bench-pro/2026-10-10-pilot-payload-membership-f11ed53d1/;
benchmark-results/fs-bench-pro/2026-10-10-pilot-tiny-membership-f11ed53d1/;
benchmark-results/fs-bench-pro/2026-10-10-pilot-namespace-membership-f11ed53d1/.
Original per-family sources, measured walls/counts/storage/cache dispositions
and cleanup claims remain exactly those of their existing two CSVs.

Shared release build PASS under the 120 s lock (wall <=42 s), at clean
`f11ed53d1`, with host Cargo +1.85.1 locked/offline, repository ARM64 flags,
pinned image and actual source SHA-256 checks inside Linux before/after build.
OWNER-3's exact two passthrough refusals remain FAIL, not hidden or relabeled.
Build: benchmark-results/fs-bench-pro/2026-10-10-batch-2-edit-build-f11ed53d1/.
Runtime SHA-256 30fdd82303cf8f958386d0b68e0fae534ab037459baf75e14a5a697c406187e3;
daemon unchanged f44900cfc975901b7101f4874f708390b6f7401981e06d608b3df52a50848c0e;
edit host 50713e6f1eba811cc3c37bf813154dbac2d6bfd2f9f05d3abda98e6081f6151c;
edit Linux def64be844b0d5edf96af3903af0eae9511dbdc9161e7066daafa8f533e96fa1.
Native sealed writer qualification's three tests PASS 3.438 s, including all
11 original small shapes, complete bytes/metadata/native inode identity,
wrong-length refusal before mutation and detection of an untouched-byte
corruption. It ran once with the sealed compiled inputs before any SDK setup.
No product source changed; no full product/Clippy/CI claim is made.

The lead releases the current batch's three subagents to prepare/parity/sample
preserving, changing and capped families in their separate worktrees. Each
uses a new run branch from this audited integration plus a meaningful final
family registration/proof declaration commit; prior branches/receipts stay
intact. This keeps each two-CSV date/source destination distinct. Sealed
compiled inputs can be reused only if exact bytes still match; no shared
writable Cargo target is reused. Shared host/Docker VM interference is declared.
The lead owns the remaining canonical and small-delta families in batch 2.
No later batch has started. Production LOC 79,366 -> 79,366 (delta +0), pinned
absent-aware staged method; reference absent (retired R9c; last 65,417).

| `cca598d0b` | Close pilot membership supplement and release qualified edit workers | 79366 | 79366 | +0 |


Batch 2 setup reuse correction: the changing worker found no cross-case master
reuse mechanism in the preparation controller. The Python-only helper now
selects only a sibling sealed PASS workspace preparation with identical full
initial native fixture digest, unchanged product, explicit Disposable/WAL/OFF
profile and exact UID/GID. It validates all old evidence, closed host Store,
installed manifest and helper seals before reuse. A matching corrupt candidate
fails explicitly, with no fallback/reprepare/retry. New native expected trees
and executable helper seals remain case-specific; every measured sample still
gets an independent closed byte copy and original cache checks. No input,
workload, timeout, verifier or cache treatment changes. Resource receipts mark
reused master ownership false and name its prior sealed preparation. Setup
warmth remains explicit; reuse never establishes coldness. Existing prepared
cases are preserved without replay. Only preparation machinery changes; the
qualified compiled runtime/writer/daemon bytes can be reused unchanged.

Six scoped fixture tests PASS 0.028 s under the main 120 s lock, including
wrong input/profile/product/UID/GID rejection and refusal of a corrupt matching
master. No new measurement. Capped worker's first original insert preparation
already passed at `4f52124e4` (27 s) and is retained as the compatible closed
524,283,904-byte initial master:
layerfs-fsb-master-edit-length-changing-capped-d2b25e60d4de4cdb.
Other workers have not prepared an SDK case. Their prospective declarations
are `e1062c6d0` preserving and `d5336c13b` changing; all three are unchanged
product 79,366 -> 79,366 (delta +0), pinned staged absent-aware method. The
lead's canonical changes remain unstaged and are not part of this Python-only
reuse commit; workers must not take unrelated compiled-source changes.

| `4f52124e4` (worker) | Bind capped root and full payload proof declaration | 79366 | 79366 | +0 |
| `e1062c6d0` (worker) | Declare final preserving current graph/cache obligations | 79366 | 79366 | +0 |
| `d5336c13b` (worker) | Bind changing current oracle and own-write eligibility scope | 79366 | 79366 | +0 |
| `849b0aff1` | Reuse compatible closed initial masters without reusing measurement | 79366 | 79366 | +0 |


Batch 2 canonical family: all 12 original fixed 64 KiB overwrite cases are
ported under their original names. Exact frozen plans, fixture/payload seeds,
SHA-256 values and original count/digest pins are transcribed from the original
product-neutral registry, preserving the 147,456-byte offset and 65,536-byte
replacement. The shared positional writer remains the P-3 ordinary surface;
no product feature or optimization is added. Current typed file/map validation
is complete; initial and final extent counts must match the exact frozen
54/544/5,394/26,995 initial counts and original preserve/increase/decrease final
counts. Native expected bytes additionally match the original frozen final
SHA-256. Retired file/map/namespace ObjectIds remain explicit format provenance,
not current identifiers. Inode scope/serial, table membership and untouched
current payload-ID subset remain strict. Numeric own-write coldness remains
unknown and INELIGIBLE; limits and workload sizes remain unchanged.

Source metadata exporter initially used an incorrect scratch relative path;
compiler refused before any fixture/Store/run. Corrected metadata compile PASS
under 60 s lock and original self-check/export PASS under 10 s lock, with no
retired product linked. One owning original-plan/count/digest transcription
test PASS 2.483 s under 120 s lock. Runtime external oracle tests compiled with
--no-run first (PASS 2.04 s), then six scoped tests PASS under 120 s lock, with
19 frozen inline cases filtered; new guard rejects wrong initial or final
counts for every original canonical scenario. Owned rustfmt check and whitespace
PASS. The two known fixture-only unused helper warnings remain visible.
No canonical case has been prepared or sampled; release build, native parity
and 12 samples remain pending. Three worker edit cohorts proceed at independent
worktree sources with the previously qualified compiled bytes; the new canonical
count/digest code applies only to the canonical family and does not invalidate
their unaffected earlier qualification. Production LOC 79,366 -> 79,366 (delta
+0), pinned staged absent-aware method; reference absent (retired R9c; last 65,417).

| `7c4273669` (worker) | Integrate closed-master setup reuse for preserving cohort | 79366 | 79366 | +0 |
| `6fee225f4` (worker) | Integrate closed-master setup reuse for changing cohort | 79366 | 79366 | +0 |
| `1479e4d07` (worker) | Integrate closed-master setup reuse for capped cohort | 79366 | 79366 | +0 |
| `a34b5d44b` | Port all twelve canonical chunk-count cases with strict original counts | 79366 | 79366 | +0 |


Canonical cohort closure at clean `a34b5d44b`: every one of the twelve
original cases has sample/attempt/completed count exactly one, strict frozen
initial/final canonical counts and final digests PASS, complete native/current
persisted graph and inode census PASS, Gone and known Stop. All 12 numeric
rows are INELIGIBLE: measured source demands 5/7/9 across tiers and independent
pre-Commit own-write cold backing/metadata proof unavailable. No case failed,
no limit exceeded and no product defect surfaced. Maximum complete wall
3,090,631,083 ns; maximum independent proof 17,830,134,041 ns, against unchanged
15,000,000,000 and 45,000,000,000 ns. Counts/storage per original case are in
the generated two CSVs, with no ratio or speed judgment.

All twelve native preparations and parity passed before the first sample.
Their complete initial tree digests match the already sealed preserving inputs;
the explicit reuse-inputs.json plus four links in the prepared parent record
that setup reuse. Each reused origin is validated by committed compatible_master
before selecting its unchanged master volume. No new master volume was created
for canonical; all 24 passing sample container/volume removal receipts PASS.
No old or other worker resource is removed. Root mode/mtime normalization,
POSIX surface/timer, retired-format identities/resource assertions and unknown
coldness differences remain declared; strict count/byte/inode guards remain.

Raw root benchmark-results/fs-bench-pro/2026-10-10-batch-2-canonical-a34b5d44b/;
prepared/parity/build siblings retain original identities. Retained bench.py
verify PASS twelve cases without replay; report generated exactly 30-column
performance and 14-column verification CSVs, twelve rows each, under
baselines/core/2026-10-10-a34b5d44b/. One controller inspection attempted to
read report CSV before the async report command finished and got FileNotFound;
waiting for that existing command then reading its completed output corrected
the inspection, with no report rerun or sample. Source/build/proof receipts
were unaffected. Final source build uses runtime
51e6d7fd03e933ca753d99d557c50da6d2c964e18fae2f95244382f4157d4132,
edit-host 0acfd6b498d19b5156014091bc337bfeb1a7e3126a01bbc5bb094a3c26a7bfff,
edit-Linux a2060e5795c3e3b1bb4b904cf2acb762b135c14bd36aa654e4af5bc99786ac56;
daemon unchanged f44900cfc975901b7101f4874f708390b6f7401981e06d608b3df52a50848c0e.
Linux source hashes verified before/after; known OWNER-3 refusals remain FAIL.
No additional product tests/Clippy/CI claim. Production LOC 79,366 -> 79,366
(delta +0), pinned absent-aware staged method; reference absent retired R9c.

| `beeb7ad69` | Record twelve canonical cases, full strict proofs and numeric ineligibility | 79366 | 79366 | +0 |


Preserving family integration: prospective declaration `e1062c6d0` and result
`dcf4e75b9` are imported locally onto main, without duplicating shared setup
patch `7c4273669` (equivalent to main `849b0aff1`). Actual sampled source stays
`7c4273669dfb8a434c1ab58ce5433062b873575b` in the retained worker branch. All
12 cases have one attempt/sample/completion, full native/persisted byte/tree/
metadata/inode/mapping/retention proof PASS, Gone and known Stop. All 12 numeric
rows are INELIGIBLE from nonzero source demands and unavailable independent
pre-Commit own-write coldness. Exact demand vectors by 1/10/100/500 MiB: head
5/7/7/9, middle 5/8/8/10, tail 5/7/7/9. The worker corrected an earlier unverified
middle/tail summary; retained receipts and committed CSVs were always unchanged
and authoritative. No failure, hang, leaked mount, bug or exceeded limit.
Maximum measured wall 193,938,333 ns and proof 13,487,312,500 ns, against the
unchanged 15 s and 45 s bounds. Per-case storage and exact count triples remain
in both generated CSVs under baselines/core/2026-10-10-7c4273669/.

All 12 preparations and parity PASS before sampling, with exactly four closed
initial masters and independent per-case expectations/helpers; every measured
sample is an independent byte clone. All 24 passing sample container/volume
removal completions PASS. Retained masters are the four preserving volumes
named in the canonical reuse supplement above. Raw worktree root:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-tiny-file-churn/layerfs/
benchmark-results/fs-bench-pro/2026-10-10-batch-2-preserving-7c4273669/.
Prepared and parity siblings stay intact; no archive/removal of ignored evidence.
Root integration's owning original scenario/payload/fixture/source test PASS;
imported 30/14-column CSVs have exactly 12 rows each, all proof PASS and Gone.
Unchanged writer/engine owning proof is reused, no native suite or measured case
replayed. No product source changed, no CI/product Clippy claim. Production LOC
79,366 -> 79,366 (delta +0), pinned staged absent-aware counter; reference absent
retired R9c, last counted 65,417.

| `dcf4e75b9` (worker) | Record twelve preserving cases and all strict proof results | 79366 | 79366 | +0 |
| `b9ffa63be` | Integrate preserving family closure and its two generated CSVs | 79366 | 79366 | +0 |


Capped family integration: prospective declaration `4f52124e4` and two-CSV
result `0ba775db9` imported onto main; shared `1479e4d07` is the already-main
setup patch, not applied twice. Actual sample source remains
`1479e4d07d6f75956bab750ba1eaee4344d60ca9` in the retained worktree branch.
All five original preparations and parity PASS before sampling; all five cases
attempted once, no retry/unchanged repeat. Append and zero extension complete
full 524,288,000-byte fresh native and strict persisted byte/inode/census/
retention proofs, Gone and known Stop; their passing sample resources are
removed. Both numeric rows are INELIGIBLE (seven measured source demands and
independent pre-Commit own-write coldness unavailable). Each recorded counts
11 requests / 179 owner jobs / 899 statements. Append wall 246,613,167 ns,
proof 11,862,696,041 ns, Store logical/allocated 530,875,288 / 530,882,560 B,
overlay logical/allocated 258,048 / 268,693,504 B. Zero extension wall
193,117,625 ns, proof 12,481,382,208 ns, Store 530,887,648 / 530,894,848 B,
overlay 258,048 / 268,693,504 B. Their unchanged 15 s product and 45 s proof
limits are WITHIN; no storage or speed judgment.

Three cases fail the original 2 s edit/Commit/End hard watchdog during command,
not a newly invented acceptance limit. Frozen supervisor source
benchmark/fs-bench-pro/src/main.rs:3883–3886 actually kills the original SDK
worker when that group expires. P-3 ordinary suffix read/write shifting is
incompatible with that unchanged watchdog for these cases. Later read-only
original Exec inspections find exit 0, separately retained; they establish no
bounded completion, Commit, byte proof, End, drain or Stop and cannot replace
missing original boundaries. No current-contract deadlock or byte defect is
established; a speed fix, workload change or SDK splice feature is forbidden.
These remain closed FAIL with proof NOT_RUN. Recorded partial controller
walls end at the original failure; command/Commit/unmount/count/storage values
remain UNAVAILABLE. The 15 s reported product limit is still WITHIN, independent
of the stricter group's failed execution. No limit is widened or reclassified.

| Exact failed original case | Partial controller wall ns | Retained sample volume | Retained container |
| --- | ---: | --- | --- |
| insert-middle-4k-input-524283904b-result-500mib-ops-1-capped-v1 | 2006271042 | layerfs-fsb-sample-88dfec81df194c75 | 8639e0ac339cbaec54f3797358f254279c31f167f27cfb81cf76a98f93b1db4e |
| prepend-head-4k-input-524283904b-result-500mib-ops-1-capped-v1 | 2009652334 | layerfs-fsb-sample-ca229edb569948cd | 8409dbd1acaaf98335da318be943bef3d6ad9ea2991d2fae11c0bc39857a9e1c |
| replace-grow-middle-2k-to-4k-input-524285952b-result-500mib-ops-1-capped-v1 | 2012600042 | layerfs-fsb-sample-085683615ce24524 | 2e2b4f2e30fa915a83ed047d025297e544f0f8a84d189394448af34043f55af5 |

Retained unknown mount keys, respectively:
8e28e42f94f1be1f5d6c4fdde8bdcdbe2eeb4e2bb70d00038f5355784ab20fc5;
6d50157b4c261c744631edbe9e27a02df34553f63c78dd434e1e15981f938d2b;
34f9c916704f2e210ee685fc694bf1357871e9f6ca72bfab75b3134ff59c3f6a.
Keep these containers/volumes/mount uncertainty intact. Closed prepared masters
layerfs-fsb-master-edit-length-changing-capped-d2b25e60d4de4cdb (524,283,904 B)
and layerfs-fsb-master-edit-length-changing-capped-7a73125237e64642
(524,285,952 B) remain available; exact shared fixture reuse is sealed in each
case-specific preparation. No old/other-owned resource is touched.

All five performance and five verification rows are committed under
baselines/core/2026-10-10-1479e4d07/. Retained verify preserves two PASS and
three NOT_RUN with aggregate exit 1, no proof replay; report succeeds with all
unknown failed metrics UNAVAILABLE. Raw worktree root:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-edit-capped/layerfs/
benchmark-results/fs-bench-pro/2026-10-10-batch-2-capped-1479e4d07/.
Sealed report sibling family-handoff.json lists full custody and six worker
LOC comparisons. Root integration's four owning case-contract tests PASS
0.019 s under 120 s lock; imported 30/14-column CSVs each contain five cases.
No product source change, no optimization or CI/Clippy claim. Production LOC
79,366 -> 79,366 (delta +0), pinned staged absent-aware method; reference absent
(retired R9c; last counted 65,417).

| `0ba775db9` (worker) | Record five capped outcomes, including three unchanged watchdog failures | 79366 | 79366 | +0 |
| `4d8ccfc93` | Integrate capped closure and preserve every failed resource/case | 79366 | 79366 | +0 |


Changing family integration: prospective declaration `d5336c13b` and result
`b4ef6e208` imported onto main; local shared reuse `6fee225f4` is equivalent to
main's already integrated patch, not applied twice. Actual sample source stays
`6fee225f419ab019245592668300d693df88015a` in the retained worktree. All 32
original cases have sample/attempt/completed count exactly one, mounted full
native byte proof PASS, all mounts Gone and known runtime Stop. Twelve strict
persisted proofs PASS: append, truncate-tail and zero-extension at all four
sizes. Their passing sample resources are removed. Twenty strict proofs FAIL:
insert-middle, delete-middle, prepend, grow replacement and shrink replacement
at all four original sizes. All retain the unchanged payload-ID subset guard;
no checker is relaxed, operation replayed or product changed. Actual CSV status
is 12 INELIGIBLE plus 20 FAIL; all 32 cache predicates independently remain
INELIGIBLE for unavailable pre-Commit own-write cold evidence.

The maximum complete wall 12,772,310,459 ns and proof 11,821,126,209 ns remain
WITHIN unchanged 15,000,000,000 / 45,000,000,000 ns limits. Every case's exact
phase wall/count/storage observations are generated in the two 32-row CSVs
under baselines/core/2026-10-10-6fee225f4/. Failures keep completed measured
counts/storage collected once after known Stop, without replacing or replaying
the failed graph proof. No speed, storage or reference comparison verdict.

All twenty failures have independent copied-export public authenticated mapping
audits. Every acknowledged root matches, every complete mapping total is valid,
and every missing required old ID lies in the region actually overwritten by
the ordinary suffix-shifting writer; none is wholly before the actual write
start. Old SDK insertion/deletion was a semantic reference splice, whereas P-3
ordinary writes normalize into final-offset overwrite plus a tail adjustment.
Rechunking of actually modified capture therefore can replace old payload IDs
while exact logical bytes remain correct. This is an unsupported old SDK
physical-retention property on the authorized ordinary surface; no current
byte/refusal/publication/crash/hang defect is established. Restoring a splice
feature or changing construction for storage/speed is outside this iteration.
The strict original failures stay visible and closed, without a product fix.

First 1 MiB insertion example: inode serial 2 preserved, exact final length
1,052,672; mapping 54 -> 56 extents; three old IDs absent at
[532242,551701), [551701,567642), [1039075,1048576). Actual ordinary write
range [524288,1052672) becomes overwrite [524288,1048576) and tail insert
[1048576,1052672). Final root equals the exact acknowledged Commit. All native
and current persisted bytes/table membership were checked before the strict
retention guard rejected it. Original receipt/export hashes remain unchanged.

Raw root, containing handoff.json, retention-scope-analysis.json and current
resource observations:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/
benchmark-results/fs-bench-pro/2026-10-10-batch-2-changing-6fee225f4/.
One offline diagnostic assumed a case-local installed.manifest despite reuse
and failed FileNotFound before Store reads. Failed diagnostic is retained; a
new copied-export audit used the original graph command's exact manifest path.
No measured case or original export was replayed/mutated. Root's initial
handoff inspection treated an integer retained-resource count as an array and
failed TypeError; reading the documented resources list corrected inspection,
without changing results or resources.

All 32 preparations/parity PASS before the first sample: six exact immutable
initial masters, 26 validated reuse records, independent sample byte clones.
All 32 copy helpers and six provisioning helpers removed with original receipts;
12 passing sample pairs removed. Twenty failed sample pairs retained, known
Stop plus independent read-only Docker observation of stopped state. No mounts
remain in these failed cases; their actual Gone boundaries are preserved.

| Exact failed original case (retention property unsupported) | Retained sample volume | Retained stopped container |
| --- | --- | --- |
| insert-middle-4k-on-1mib-ops-1 | layerfs-fsb-sample-948436f3f23c4906 | 2f3211741fe6571ec6b2a56b377881b02ed7f3dec3443dca3f171f86474e8742 |
| insert-middle-4k-on-10mib-ops-1 | layerfs-fsb-sample-5beb04a85c12451f | 436946c2b114f502fa0a0e764ca51217db84182dee68bd04475c999ab118670e |
| insert-middle-4k-on-100mib-ops-1 | layerfs-fsb-sample-faf93ba5a2a9415a | c97f4c389a40e4c2b9205161eec3129859e5ce362b2b633a2297ff47bc40917c |
| insert-middle-4k-on-500mib-result-capped-v2-ops-1 | layerfs-fsb-sample-722e993fa9a3493f | 57ab521d22f1990c5b9d04471a851e8c497c6ed265448b23765d7c4f7bbad924 |
| delete-middle-4k-on-1mib-ops-1 | layerfs-fsb-sample-42f4e454986c4610 | ef2fe1d8eaf2f7385730ad0f152d2e5b8d7f76ee023af1a991d8bbef01b6af2a |
| delete-middle-4k-on-10mib-ops-1 | layerfs-fsb-sample-b3ca99691e13414e | 029d95fcba2babb48e6457e0a3edded730389969936d74ba00df5764cd3ed569 |
| delete-middle-4k-on-100mib-ops-1 | layerfs-fsb-sample-5dfd9b92757a48d5 | c4a9096010dc1e81edd0dd73cc66f19a4ad4f8c24fceaac0d2bab2fec3f6f589 |
| delete-middle-4k-on-500mib-ops-1 | layerfs-fsb-sample-bbff2bdcac9a4549 | 6deb7ee42d76e68492999cef2718d467104aa147fa062c68399ced8cb30b1ce9 |
| prepend-head-4k-on-1mib-ops-1 | layerfs-fsb-sample-db30059350ac4178 | 7722cd9c332d8eb338b50a571246c98701c8d0a0409da2b540c7ce24c8da4b60 |
| prepend-head-4k-on-10mib-ops-1 | layerfs-fsb-sample-c33751b3b7cf4db5 | cc0e8dfd30eef3462a27491b820e4d1713367d3933cda38ec9ddea0d8c3c1579 |
| prepend-head-4k-on-100mib-ops-1 | layerfs-fsb-sample-961158709b444be1 | e215d5335233ada1599026c17dc3ab6aa579e39051f84d5f4c62248be762e9d0 |
| prepend-head-4k-on-500mib-result-capped-v2-ops-1 | layerfs-fsb-sample-28fdc3ccb85d44df | 7aadcc99dfce485fe36c3a89b66e0721357bc6628e64d30aaa87b51f2a97d33f |
| replace-grow-middle-2k-to-4k-on-1mib-ops-1 | layerfs-fsb-sample-11b57e9ab2c341a8 | aed5089da6a14f86ce7b707f4c8b74d8e3d728f4b6c0bd4a0d650280a68b4763 |
| replace-grow-middle-2k-to-4k-on-10mib-ops-1 | layerfs-fsb-sample-652b7f0606eb411b | 9467fb87e8971e24255feca9e6a23f35cf14a2b68540d8d9914ff1a7f160d311 |
| replace-grow-middle-2k-to-4k-on-100mib-ops-1 | layerfs-fsb-sample-d4ec0d5eec1549ec | 744bfaad410b2abf0a8ecce4b113f68ec00967ae1f34d0d573db0c72ee6b2e1d |
| replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1 | layerfs-fsb-sample-24556ffe6bf04529 | 8501df1719413d8c6ed530f88d4878312808bf4e1d4284af6b507fda7e0ec059 |
| replace-shrink-middle-4k-to-2k-on-1mib-ops-1 | layerfs-fsb-sample-65d255aadca341d7 | 2529b115192553f13d5154ca50411fee2d6e873427852bea5a211a7cee080e11 |
| replace-shrink-middle-4k-to-2k-on-10mib-ops-1 | layerfs-fsb-sample-1a343f218bb84d47 | d1ae0966f11f9972407a9dd3c85f32dce85a182615541719c7f4cf26d5769290 |
| replace-shrink-middle-4k-to-2k-on-100mib-ops-1 | layerfs-fsb-sample-0dd8ae3cc2ca4145 | bfacaef8520c9aabe9026ec3f12d4041d1486dc436ca170930c69850c9d64bad |
| replace-shrink-middle-4k-to-2k-on-500mib-ops-1 | layerfs-fsb-sample-87253580856148db | fb5f3b0552d51f533e47a168be33af089cd50c2dd3dbd38d5fa2db9f320dcb79 |

Six retained immutable master volumes:

- layerfs-fsb-master-edit-length-changing-131fa3579df147c5
- layerfs-fsb-master-edit-length-changing-1569ecbe61d64fe0
- layerfs-fsb-master-edit-length-changing-29c45685f6bf487b
- layerfs-fsb-master-edit-length-changing-67a2cd17955540f8
- layerfs-fsb-master-edit-length-changing-87a51e24fc0946df
- layerfs-fsb-master-edit-length-changing-99b212255e814bab

Root integration's two original-plan/payload/metadata tests PASS 0.040 s under
120 s lock; imported schemas/rows/failed statuses/Gone/limits match all original
receipts. Retained verify appropriately returns exit 1 for 20 FAIL / 12 PASS,
with all original manifests valid; no sample replay. No product change, no
optimization, no Clippy/CI claim. Production LOC 79,366 -> 79,366 (delta +0),
pinned staged absent-aware method; reference absent (retired R9c; last 65,417).

| `b4ef6e208` (worker) | Record all 32 changing cases and preserve twenty strict failures | 79366 | 79366 | +0 |
| `05ffbf971` | Integrate changing closure and exact retained failed resources | 79366 | 79366 | +0 |


H-5 cache qualification scope audit: at the original seven pilot read-only
numeric PASS receipts, Class B's daemon canonical-object predicate passed, but
full per-phase backing/metadata/SQLite cache eligibility was not established.
S8-PROOF-PLAN-20261008.md section 6 says B still pays the history read and must
declare Store residency; cluster-two/cache/README.md:32–35 explicitly limits
ELIGIBLE to the tested predicate and leaves metadata/reader/SQLite scope
separate. Actual bind still invokes read_handle::snapshot -> history.branch_snapshot
independently of canonical-object counters. Every reviewed row has UpToDate
and zero object-batch delta, but no pre-mount cold backing/metadata/reader proof.
Mincore runs only after measured unmount and automatic Gone, without eviction.

| Original recorded scoped-B PASS case | Post-phase Store resident/total pages |
| --- | --- |
| payload-random-read-1-compact-v2 | 301 / 301 |
| payload-random-read-10-compact-v2 | 2634 / 2634 |
| payload-random-read-100 | 121178 / 129564 |
| tiny-stat-1-compact-v2 | 306 / 306 |
| tiny-stat-10-compact-v2 | 2658 / 2658 |
| tiny-stat-100-mixed-v4 | 26179 / 26179 |
| tiny-stat-500-mixed-v4 | 121665 / 130025 |

These post-phase observations cannot establish pre-attempt state or attribute
warmth. Original receipt/CSV labels, walls, counts, storage and functional proofs
remain unchanged as history; the seven labels establish only their original
scoped predicate and must not be read as full numerical cache qualification.
All 36 pilot cases retain functional proof/clean closure, with the missing
membership check separately completed. No pilot replay or numerical performance
claim follows from the scope audit. The lead previously summarized those labels
as numeric PASS without this qualification; this supplement corrects that
interpretation, without relabeling or promoting a historical receipt.

The spec's cache paragraph is clarified in its own commit to follow the owner's
absolute no-setup/earlier-phase warmth rule. B/C remain the requested product
regimes, while full cache eligibility needs identity-bound pre-phase backing,
metadata and SQLite/reader evidence. Future missing/unknown scope fails closed
INELIGIBLE, without eviction, cache/workload/limit changes, or product changes.
The current guard correction and its external failing regression are pending
next, with no new sample begun. Small-delta already declares unavailable own-write
coldness and INELIGIBLE; its authoring continues independently. Production LOC
79,366 -> 79,366 (delta +0), pinned absent-aware staged method; reference absent
retired R9c, last counted 65,417. Documents only, no tests or measurement replay.

| `c9aaddf02` | Clarify full phase-cache qualification and preserve old scoped predicates | 79366 | 79366 | +0 |


H-5 prospective fail-closed guard: cache.regime_status retains the original
selected B/C predicate, while cache.validate additionally requires complete
identity-bound observations before mount, command, Commit and unmount for
still-paid OS backing, metadata and SQLite/reader scopes. Exact measured
phase bindings and starts must match observations, clocks must be monotonic,
observations strictly precede starts, all scopes must be complete, present
Store/overlay identities checked, backing pages zero and metadata/reader states
verified cold. Missing, mismatched, warm, late, partial or unknown evidence is
INELIGIBLE. The original own-write pre-Commit guard remains independently
required; no observer or measurement is fabricated to satisfy either guard.
Current runners supply no such full evidence, so future B/C numeric rows fail
closed while independent functional proof status is unchanged. No eviction,
cache policy/workload/worker/limit change or product optimization is introduced.

External regression fails on the before source with ELIGIBLE instead of
INELIGIBLE for zero object demand plus only a terminal observation. Raw failed
output is benchmark-results/fs-bench-pro/2026-10-10-phase-cache-guard-checks/
before.stdout and before.stderr. Thirteen final owning workspace evidence/cache
checks PASS 0.061 s under the 120 s lock; raw after.stdout/after.stderr retain
all results. Positive synthetic evidence tests include all required phase
bindings, so existing terminal cleanup/demand/lifetime and own-write checks are
still meaningfully tested rather than masked by missing new fields. Every
phase rejects late observation, wrong identity, positive/bool residency,
unknown metadata/reader scope or incomplete paid inputs. Whitespace PASS.
Original seven labels/receipts/CSVs remain unchanged and their full numerical
qualification remains unverified, as the separate spec supplement records.
No case replay. No product source change, no Clippy/CI claim. Production LOC
79,366 -> 79,366 (delta +0), pinned staged absent-aware method; reference absent
retired R9c, last counted 65,417.

Additional audit scope: StoreWork.object_batches scalar ends at measured Status
before timed unmount, not the complete timed boundary. The seven original
mount-after-warm intervals report 27 foreground SQLite executions (31 for tiny
stat 500), 10/11 canonical cache hits and zero object batches. Those instrumented
counts remain paid observations, not disk/cache-miss/coldness proof. History
snapshot reads are not canonical-object batches. This guard avoids using that
scoped scalar as an all-phase cache certificate.

| `25719de3b` | Require complete pre-phase cache evidence beyond B/C scoped predicates | 79366 | 79366 | +0 |

Small-file-delta support integrated from worker commits `bec3a0e75`,
`cf9fe706e` and `53853a02e`; worker `561b5ec10` carries the identical H-5
Python guard already on main and is not duplicated. Registration is committed
before preparation or sampling. The unchanged original builder supplies ten
files (245,760 initial bytes), thirty whole-file fs::copy imports and thirty-one
independent semantic manifests. Current public History/Filesystem/Storage reads
bind every actual acknowledged Commit, parent, stack/base and final head to the
same stopped sample Store, check every byte/path/mode/mtime and exhaustive inode
membership, then inspect authenticated selected regular-file payload records.
The historical kernel Fork/Mount proof topology is declared as an omission;
the final fresh mounted native proof and terminal cleanup remain required.

Original actual DELTA-path execution remains an existence condition independent
of complete semantic proof. Known FULL-only output or unavailable payload
encoding evidence leaves the case FAIL and retains stopped resources; generic
metadata links do not substitute. No count/percentage or latency/storage quota
is added. Every retained state is proved by its own child with a thirty-second
stop, while each child also validates all thirty-one acknowledgement rows and
the final head. The original absolute 600-second phase stop is unchanged.
Original resource/free-space guards run at their original boundaries; exact
read-only observer walls are retained and excluded from product time using the
existing PerformanceClock exclusion API, but still consume the absolute phase
deadline. The actual own sample container must read back the original two-CPU,
2-GiB/no-swap/256-PID profile. No master or protected resource is configured.

Pre-sample authoring failures remain visible: the first host --no-run found an
extra extracted-verifier borrow and missing PackPersistence trait import, fixed
in harness support; a formatting invocation followed modules into six frozen
files, which were restored exactly and checked against the recovery source
before any commit/sample; the first broad history/workspace test selection
included unrelated frozen reference self-checks and reached the 120-second
ceiling (FAILED/hang), diagnosed from the full-size debug history self-check.
Final restored-source builds and explicitly scoped complete_history_ and
workspace_graph_ tests passed without weakening any test. The first native
qualification failed E0603 on the frozen wrapper's private storage_smoke module;
the corrected test compiles the unchanged original module directly. Final
native qualification PASS 6.846 seconds covers all thirty-one states, thirty
full imports, metadata/inode preservation and intentional final-byte corruption
rejection; its first failed receipt stays retained. No product bug or sample
replay occurred during these authoring checks.

At integrated source the five mechanical family tests PASS 0.256 seconds; the
native test is explicitly skipped because its qualified worker seal is reused.
Thirteen owning workspace/cache tests PASS 0.061 seconds, both under the
120-second lock. One lead discovery invocation mistakenly named the parent
tests directory, found zero tests and returned failure; corrected discovery
names tests/engine and runs all thirteen. Read-only probes also named nonexistent
engine module paths; corrected file inventory established the owning paths.
Neither invocation touched product/source or repeated a benchmark. Worker final
external complete/selected-old-root ancestry and byte-corruption tests pass;
the final host compiled identity is still being sealed before any sample.
Product tree remains 10bd8215443766fa2509fa5f90cf0044200ba7be. Production LOC
79,366 -> 79,366 (delta +0), exact pinned staged absent-aware method; reference
absent (retired R9c; last counted 65,417). No Clippy/CI or numerical cache
qualification claim follows from harness-only checks.

| `bec3a0e75` (worker) | Register original small-delta fixture/history support | 79366 | 79366 | +0 |
| `cf9fe706e` (worker) | Qualify unchanged native importer through original module | 79366 | 79366 | +0 |
| `561b5ec10` (worker) | Apply main H-5 guard without duplicate main integration | 79366 | 79366 | +0 |
| `53853a02e` (worker) | Preserve payload mechanism and per-state proof stops | 79366 | 79366 | +0 |
| `c0c99a4a3` | Integrate reviewed small-delta support before preparation/sample | 79366 | 79366 | +0 |

Qualified compiled-support reuse integrated from worker `dbcae7b12`. The
committed entry point now accepts bench.py build --reuse-unchanged ORIGINAL:
only host-runtime compiled inputs may differ. Exact eight binary keys,
hashes/sizes, original full build manifest, clean before/after source identities,
both complete Linux source hash passes, pinned image/readback, ARM64 flags,
Disposable/overlay/worker profiles, product tree and exact OWNER-3 refusal
evidence are checked before reuse. Changed product, recipe, workload or writer
bytes refuse admission. Failure retains declared scope and attempted child
custody without a successful seal. No Docker resource is created by the narrow
host build. Chained host-only seals are not accepted as original full builds.

Eight owning build identity tests PASS 0.012 seconds under the main120s lock;
worker eight PASS0.018 seconds and original full-build evidence validation PASS.
The earlier ad hoc host diagnostic rebuilt in8seconds but remains diagnostic
and unsampled. The committed route created a separate supported final seal in
<=3seconds at dbcae7b12, runtime
fb2031585ac06026a4871f755d63d79420db7a7d3d1d63ceb68f0c1b9dd33c2f,
seven immutable support binaries reused and daemon f44900... unchanged.
Raw worker build directory is benchmark-results/fs-bench-pro/
2026-10-10-batch-2-small-delta-build-dbcae7b12-supported. Final case preparation
is in progress; no sample outcome is yet claimed. This implements decision32
without changing the product, workload, cache, worker count or any limit.
Production LOC79,366 ->79,366 (delta+0), pinned staged absent-aware method;
reference absent (retired R9c; last counted65,417).

| `dbcae7b12` (worker) | Add qualified host-only build support to bench.py | 79366 | 79366 | +0 |
| `2e6a508b5` | Integrate qualified immutable support reuse and its scoped checks | 79366 | 79366 | +0 |

Small-file-delta closed from worker result commit `be5fc295e`. One sample at
clean dbcae7b12: all30 original whole-file imports and Created/Committed
acknowledgements completed on one Workspace. All31 actual roots from the same
closed measured Store passed complete bytes/metadata/ancestry/inode proof,
7,624,960 verified bytes and341 total paths. Authenticated regular-file payload
DELTA dependency existence passed,257 per-state record observations (repeated
content IDs possible, not a unique-object count). Final fresh native verifier
PASS, measured and verifier terminal unmounts Gone and KNOWN_STOP. Numeric
INELIGIBLE stays visible because initial/warm interval, full paid backing,
metadata/SQLite and own-write coldness are unavailable. No product bug surfaced
and the case was not repeated.

Recorded complete product wall1,688,425,918ns; raw absolute mount-through-End
wall5,420,649,583ns includes60 read-only original resource-observer intervals
totaling3,732,223,665ns, retained individually. Phases are mount8,160,042ns,
commands1,289,511,875ns, Commits236,492,002ns and unmount4,011,125ns. Separate
proof2,402,421,666ns; max importer+Commit step71,246,667ns and max selected-root
proof child43,129,417ns. All original600s phase and30s per-state stops are
WITHIN, with no limit or workload changes. Counts402 requests/3142 owner
jobs/14764 statements. Store logical2,412,624B/allocated2,416,640B; overlay
logical286,720B/allocated268,722,176B. Original2CPU/2GiB/no-swap/256PID profile
readback is exact; all30 resource checks record zero OOM/OOM-kill/swap. The
53,334,016B VmHWM is process-lifetime diagnostic only, not a phase peak.

Raw run remains in the tiny worker at
/Users/yifanxu/.codex/worktrees/fs-bench-pro-tiny-file-churn/layerfs/
benchmark-results/fs-bench-pro/2026-10-10-batch-2-small-delta-dbcae7b12/.
Retained receipt verification PASS, report generated once. The two generated
CSVs under baselines/core/2026-10-10-dbcae7b12 have the exact30/14-column
schemas and one row each (performance INELIGIBLE/proof PASS). Functionally
passing sample container
96cfc0e55fae6165dd4362632339ffd2112e9f47d37bd25b2bd9964c771f12e4 and sample
volume layerfs-fsb-sample-60324439af044ce5 were removed with successful cleanup
receipts. Closed initial master
layerfs-fsb-master-small-file-delta-smoke-7ab14dadc3d1479c remains for reuse.
All earlier failed/uncertain resources, protected resources and unsampled
diagnostic builds remain untouched.

Batch2 closure: five original families/all62 cases registered as ported,
parity passed with differences declared, one sample each, generated CSVs
committed with every row/reference. Canonical12, preserving12, changing12,
capped2 and small-delta1 have strict proof/clean-close PASS;39 numeric rows
are INELIGIBLE. Changing20 strict original retention failures and capped3
watchdog failures remain FAIL with the source-backed reasons and resources
recorded above. None establishes a fixable wrong-byte/publication/refusal/
deadlock product defect; no optimization or new splice feature is authorized.
All batch2 declared reporting limits are WITHIN. The rollout now has98 ported
cases across nine families and175 pending cases across sixteen families.
Product79,366 unchanged; no speed, release or reference-comparison verdict.

| `be5fc295e` (worker) | Record one small-delta sample with all31 root proofs | 79366 | 79366 | +0 |
| `1c975ba30` | Integrate small-delta CSVs and close all62 batch2 cases | 79366 | 79366 | +0 |

Batch3 shared activation admits the four original families through the existing
fixture/entry-point allowlist; pending registrations remain unsampleable. A
control_only registration now skips both untimed warm-up Exec and measured
Exec, retaining mount, Commit and terminal unmount and command_ns0. The explicit
original_ordinary_guards declaration configures/read-backs only the own sample's
original2CPU/2GiB/no-swap/256PID container profile before timing, then observes
the exact frozen main-file Store apparent/allocated4GiB bound and actual
cgroup OOM/swap before/after Commit and after native proof. Read-only observer
walls remain individually recorded and excluded from product time. No page
touch/eviction, retry, Disposable backing fsync or product configuration change
is introduced. Full Store+sidecar/overlay metric scopes remain separate.

Retired host Store RSS and physical-spool peak observers belonged to the retired
host data plane. Current cgroup/stat checks do not assert equivalence or a PASS
for those unavailable peak domains; registrations must state the exact gaps.
The original_operation_counts declaration additionally requires each exact
frozen semantic counter once in the original workload stdout (paths/files/
directory entries/read bytes or constructed chains), rejecting absent,
duplicated, unknown or wrong values without rerunning any operation. Counts
remain source/receipt observations; no nominal syscall-count equality is added.

Sixteen owning workspace/controller/cache tests PASS0.076s under120s lock,
including clean no-Exec control flow with original refusal custody, exact main
Store boundary, unknown/type/overflow/OOM/swap rejection, and missing/duplicate/
wrong traversal-counter rejection. Fixture scoped checks and whitespace PASS.
No batch3 sample has begun. Worker source review finds all33 batch3 cases
expressible using existing product routes, subject to actual parity/proof.
Production LOC79,366 ->79,366(delta+0), pinned staged absent-aware method;
reference absent, retiredR9c,last65,417. Harness changes only.

| `3c697ae92` | Activate batch3 routing with clean control and original ordinary guards | 79366 | 79366 | +0 |

Directory-construction/traversal registration integrated from `5773c34fb`:
twelve original IDs, seed1, four exact tiers, original ranked chain/depth
algebra and untouched ordinary workload. Construction expects Committed;
metadata/content scans expect UpToDate. Mechanical source/provenance/algebra
checks PASS under120s lock, and original total-path/file/directory-entry/read
byte or constructed-chain counters are declared for retained stdout checking.
Unknown ClassB paid backing/metadata/SQLite and own-write cache evidence remains
INELIGIBLE. Current complete native/persisted names/kinds/modes/mtime/bytes and
inode census are stronger than the original mixed-tier sampled reopen proof;
retired-format ObjectId equivalence/unreferenced-object census and retired
host Store RSS/physical-spool peak diagnostics are explicitly omitted.
All15s report/120s operation/45s proof/59s hard bounds, original container and
4GiB main Store guards remain. No preparation or sample outcome is claimed.
Eight distinct initial masters suffice after exact checks: four construction
trees and four scan trees shared by the two scan modes; samples stay independent.
Product79,366 unchanged, reference absent, exact pinned staged counting method.

| `5773c34fb` (worker) | Register twelve original directory cases | 79366 | 79366 | +0 |
| `50c297c68` | Integrate directory registration and its scoped contracts | 79366 | 79366 | +0 |

Locality registration/writer integrated from `fa69ab6b7`. All16 original cases
retain clean Commit/fixed move/distributed edits/dense rewrite at the four
frozen tiers. Clean uses control_only with no workload Exec; distributed edits
reuse the unchanged original sdk_edits ranked paths and exact4KiB XOR0x5a
replacements at offset0/delete4096 via ordinary pwrite, original target-length
guard, checked close and exact metadata restoration. There is no suffix copy,
read, truncate, splice feature or new dependency. The precise fixture staging
condition additionally copies the edit-linux binary for ordinary_sdk_edits;
its independent native oracle remains the original ordinary workload verifier.
The clean cache declaration now describes the actual prior read-only mount
without Exec. Retired host Store RSS/spool peaks remain declared gaps.

Two owning mechanical locality contracts PASS0.028s on main; two sealed-native
qualifiers were explicitly NOT_RUN there pending changed writer compilation.
Worker final clean build at67ec7f464 PASS<=97s with actual pinned Linux source
hashes before/after, unchanged daemon f44900.../runtime fb203158..., new edit-host
aab49ace5121d8c7a8b39baf6590d4ec5b6b3b754f4ee2c647d25daa5e1d77c4 and edit-linux
efc81ac5d393630e1d41b1c7d09fcea5430eb399e8b460624df0d770d92201b6. Passing build
container removed; OWNER-3 refusal retained. After source was sealed during
build, the Python-only clean declaration correction was committed960a7f81e;
compiled-input validation admits that exact unchanged binary set. All four
mechanical/native qualifiers PASS3.775s at final source: full compact original
oracle, selected stable inodes/suffix/background/modes/mtime, intentional byte
corruption refusal and wrong selected length refusal before mutation. No
product or original source changed, no sample has yet been reported.
Production LOC79,366 unchanged, reference absent, pinned staged method.

| `fa69ab6b7` (worker) | Register sixteen locality cases and ordinary positional adapter | 79366 | 79366 | +0 |
| `c66af2c19` (directory worker) | Apply exact shared ordinary source without lead ledger | 79366 | 79366 | +0 |
| `67ec7f464` (locality worker) | Apply shared source and precise edit helper staging | 79366 | 79366 | +0 |
| `960a7f81e` (locality worker) | Correct clean mount provenance without compiled changes | 79366 | 79366 | +0 |
| `2590a4536` | Integrate locality adapter/registration and exact helper provenance | 79366 | 79366 | +0 |

Local snapshot registration/shared history support: one original25k one-byte
case, one Workspace, exactlyC1 create25000/C2 edit256 ordinals97*j/C3 restore
original bytes, three distinct Commit operations. Existing frozen fixture/C1
generator is unchanged; independent Python equations produce all four semantic
manifests and C2/C3 native inputs, checked by the unchanged original full native
verifier before sampling. Runtime compares each local manifest against the
original Rust expected state, binds all actual acknowledgements/parent/stack/
base/finalhead and checks every retained root's bytes/metadata/complete inode
census. Equal C1/C3 immutable roots are valid; distinct Commit identities and
correct parent ancestry remain required. Small-delta's31-state uniqueness and
payload DELTA existence guard remain intact. Final native proof uses the exact
original deterministic root/seven-file sample; all75,004 retained paths/75,000
bytes have complete independent persisted coverage.

The local15s reporting limit is separate from its original120s execution
allowance; no soft limit is used as the sequence watchdog. Original45s proof/
59s hard remain. ClassC initial/own-write/full paid cache evidence is unknown
and numericINELIGIBLE. Retired-format object census and retired host Store RSS/
spool peaks remain explicit gaps. No product or frozen source changed.

Checks at this source: host history --no-run PASS11.70s, exact existing
complete_history_ proof PASS1.73s (twenty imported tests filtered). The new
complete_snapshot_history_ debug test FAILED/hang at its original30s child
stop, total34.54s; retained child Store/transcript/events at
/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/fsb-snapshot-history-91527/.
Only initial-root verification finished; stderr was empty and first25k-root
work was unfinished at fencing. No failure output is relabeled or deleted.
One read-only diagnostic incorrectly assumed /tmp, then mishandled a null
event span; corrected inspection found the exact retained evidence. Source
uses full canonical reads and complete membership, and benchmark compilation
is release. At the separate prescribed release identity, --no-run PASS7.51s
and the unchanged full-size external test PASS10.30s, including restored-root
equality and deliberate C2 oracle corruption refusal. Same30s child stop,
same25000/256 data, same verifier, outer120s; no timeout/test/workload weakening
or product optimization. The debug failure remains unverified as a debug
performance problem; it is not claimed repaired. Existing two unused fixture
helper warnings remain visible; no Clippy/CI claim.

Two local mechanical tests PASS0.187s, sealed-native qualification explicitly
pending committed final build. Sixteen owning workspace and six fixture checks
PASS under120s locks; bash/skip_children rustfmt/whitespace PASS. No local
snapshot preparation or sample yet. Production79,366 unchanged, reference
absent (retiredR9c,last65,417), exact pinned staged absent-aware method.

| `0344bfdc6` | Register original three-Commit local snapshot and full retained state proof | 79366 | 79366 | +0 |

H-6 first local snapshot sample at0344bfdc6 FAILED after all three measured
Committed replies and measured terminalEnd/Gone, then native witness exit0.
The controller could not parse the original multiline sampled_paths/ranges;
that receipt intentionally has no verification_status. Measured product wall
22,049,536,459ns is EXCEEDED against15s, with command13,025,547,001ns,
Commit8,851,071,916ns, mount9,189,333ns and unmount33,389,042ns. Counts/storage
remain UNAVAILABLE in its original row, not guessed. Original final witness
is runtime.events.artifacts/000062-verify.stdout. Its six exact fields specify
root/seven paths/seven one-byte ranges, no absent paths, full namespace/file
coverage false and original omissions. Actual checks succeeded; full persisted
four-root proof had not yet run. This is a harness contract mismatch, not a
wrong-byte/publication/product defect.

Failed sample container
f23dc735b265869a9ff91544ba366b5568787993fe23ad9d9df62a173a442bf9 is still running;
volume layerfs-fsb-sample-b86ad3b8b2b94919 retained, verifier mount and Stop
uncertain, host fenced-9. Do not infer teardown or remove it. Initial master
layerfs-fsb-master-local-snapshot-ec2d6edff2f14397 remains. Failed worker raw
run2026-10-10-batch-3-local-snapshot-0344bfdc6, handoff-failed.json and both
generated FAIL/EXCEEDED CSVs are retained at worker commit3a7c3fd96; root
integration is pending. One earlier parity call named the sealed case folder
instead of its parent, failed before fixture load, retained; a fresh wrapper
link and correct parent passed with zero samples and no setup replay.

H-6 fixed prospectively by a case/schema-bound exact original six-field
sample-receipt comparison plus known exit0. It preserves all multiline values,
exact path/range/absence selection and false coverage flags/omissions; it does
not invent verification_status or promote the bounded witness into a full
native tree proof. Existing full-verifier PASS check stays required elsewhere;
independent full four-root/history/census proof stays required after this
witness. Regression fails at the before-source parser with the original #5
ValueError, raw before.stdout/stderr under
benchmark-results/fs-bench-pro/2026-10-10-local-snapshot-native-receipt-fix/.
Three final scoped native-receipt tests PASS and sixteen owning workspace
checks PASS0.063s under120s locks. Altered paths/ranges/absence/coverage, extra
inventedstatus and missing/nonzero exit are rejected. Source/runtime/workload
compiled inputs unchanged: reuse qualified native/build/external proofs.
Only affected local snapshot may be sampled once at the corrected committed
engine identity; failed original row/resources remain visible. No product
change, optimization, limit/cache/workload/test reduction or unchanged replay.
Production79,366 unchanged; reference absent; pinned staged absent-aware method.

The qualified release history authoring check is source-qualified reuse, not
an identical-binary claim: main --release --test emitted runtime9bcf33f9...,
worker sealed release9a9e7fd5...; all four checked source hashes match0344bfdc6.
The main authoring checks.json identifies summary vs original tool-transcript
evidence explicitly. Worker native qualification PASS11.124s is a separate
actual sealed-binary check. Native/preparation/runtime/source identities stay
distinct; no evidence or original verdict is relabeled.

| `3a7c3fd96` (worker) | Preserve first local snapshot FAIL/EXCEEDED and uncertain resources | 79366 | 79366 | +0 |
| `03033abdc` | Fix exact original sampled-native receipt contract without weakening proof | 79366 | 79366 | +0 |

Directory cohort integrated from worker f9d6e366e: all12 original cases sampled
once at c66af2c19, full native/persisted inode/semantic count/resource proof
PASS, Committed/UpToDate exact, measured/verifier Gone and KNOWN_STOP. All12
numericINELIGIBLE underH5; noneFAILED/NA/EXCEEDED. Max product5,456,931,833ns,
max proof22,433,335,875ns; unchanged15s/45s limits. Thirty-six original read-only
count/resource intervals remain excluded and individually retained; no guessed
syscall values. Both12-row CSVs have exact30/14column schemas and source/raw
identities unchanged. Retained verification PASS withoutreplay; raw complete
case/phases/counts/logical+allocated storage/limits/profile/custody inventory is
namespace worker benchmark-results/fs-bench-pro/
2026-10-10-batch-3-directory-c66af2c19/handoff.json. All44 removal receipts
PASS (12samplecontainers/12samplevolumes/12copyhelpers/8provisionhelpers), no
teardown repeated. No productbug or uncertain/failed resource in this cohort.
Eight closed reusable masters retained:
layerfs-fsb-master-directory-construction-traversal-4cfb97a81b1c4764,
-50de5bba1d6449fd,-906afd940f3e4df7,-9f2e95078b7d45de,
-a398b7f797434873,-c206c4580d884ef2,-c679ac16c49b4245,-cdf67089171d4f76
(all suffixes share the full preceding prefix). Original/frozen/protected
resources and earlierfailedreceipt rows untouched. Retired-format object
census/RSS/spool diagnostics and full coldness remain explicitly unverified.
Production79,366unchanged, referenceabsent, pinnedstagedmethod; CSV/docs only.

| `f9d6e366e` (worker) | Record twelve complete directory outcomes | 79366 | 79366 | +0 |
| `ad4f95503` | Integrate directory CSVs and complete family custody | 79366 | 79366 | +0 |

Locality cohort integrated from7f29c1dc4: all16 original cases sampledonce at
960a7f81e, fullnative/persisted/inode proofPASS, Gone/KNOWN_STOP and removed
sample resources. Numeric16INELIGIBLE underH5, including zero-demand clean
cases whose actual no-Exec behavior/command_ns0/UpToDate is recorded. Counts
and allphase/storage fields come directly from originalreceipts, not revisited
measurement. Complete rawHANDOFF.json remains in tinyworker
benchmark-results/fs-bench-pro/2026-10-10-batch-3-locality-960a7f81e/.
Both16-row CSVs exact30/14columns verified. Original generic tags retained,
H5 paid backing/metadata/SQLite denial specified separately; retiredcanonical
format/hostRSS/spoolpeak remain explicitgaps. Retainedverify16PASS, reportonce.
One reportwrapper attemptR4_LOCK_BUSY75 refused while verifier stillactive;
no reportexecuted/overlap or caseattempt, corrected invocation onlyafterknown
verifyexit0/noCSVdestination, then generatedonce. Keep this orchestrationfailure.

Dense500 original15s reportinglimit EXCEEDED:15,251,166,166ns, excess
251,166,166ns; separateproof20,002,030,708ns WITHIN45s/59s, no120s hang.
Requests33359/ownerjobs282059/statements1353145. Storelogical1,066,713,088B /
allocated1,066,717,184B; overlaylogical541,495,296B/allocated809,934,848B.
Slowcase completesfunctionally and remainsnumericINELIGIBLE. No optimization,
limit/workload/cache/worker/fixture alteration or repeat; no productbug.
Four ownclosedmasters retained:
layerfs-fsb-master-workspace-change-locality-d26a3afb0f06417a,
layerfs-fsb-master-workspace-change-locality-1570757bb67b4c04,
layerfs-fsb-master-workspace-change-locality-7094f06f0f3542eb,
layerfs-fsb-master-workspace-change-locality-f5fe3ef6c5d94aa5.
Allpassingsamplepairs/helpercontainers removedwithoriginalreceipts; prior
failed/uncertain/protectedresources untouched. Product79,366unchanged, reference
absent, pinnedstagedcount; CSV/docsonly. Currentbatch28of33distinctcases closed,
onefailedsnapshotattemptretained pendingcorrectedsource, fourGitunsampled.

| `7f29c1dc4` (worker) | Record sixteen locality cases and dense500EXCEEDED | 79366 | 79366 | +0 |
| `26c037d90` | Integrate locality outcomes without optimizing the soft-limit miss | 79366 | 79366 | +0 |

First failed local-snapshot CSVs integrated unchanged from3a7c3fd96, exact
30/14columns/one row each underbaselines/core/2026-10-10-0344bfdc6. StatusFAIL,
proofFAIL, wall22,049,536,459ns/15sEXCEEDED; unknowncounts/storage andStop
remainUNAVAILABLE, no inferredcleanup. H6fixed03033abdc is prospective only:
the failedrows/verifierstdout/source/raw/resourcecustody stay original.
Workerb1f8150e applies exactthreefileH6fix withoutleadledger, staged/committed
tree1f81185621f54cd278b68935274d66e9905f6244 and79,366unchanged. Owningthree
receipt checksPASS0.012s. Exactoldpreparedmaster/helper/fixture andcompiled
seal reused; correctedparity/newsource sample is pending. No setup/native/
debugproof replay. No productfix/optimization or publicationclaim.

| `b1f8150e` (worker) | Apply exact sampled-receipt fix at changed case identity | 79366 | 79366 | +0 |
| `98d3be30a` | Integrate original failed snapshot rows without relabeling | 79366 | 79366 | +0 |

Local snapshot corrected-source closure froma95ca5375: one sample atb1f8150e,
originalthree25k/256/restoreoperations, threeCommitted replies, exact native
samplePASS and completeactualsameStore4roots/3Commitancestry/75004paths/
75000bytes/inodecensusPASS. RestoredC1/C3 rootexactlyequal. BothmountsGone,
KNOWN_STOP, passingcontainer67298f865bd6d1db6d252613ec982aa11c7cf55f9d82bd51a5e52a394cea3b54
andvolumelayerfs-fsb-sample-40a7ee17335e48df removed. NumericINELIGIBLE underH5,
wall24,397,444,499ns EXCEEDED15s (excess9,397,444,499ns), proof11,633,937,583ns
WITHIN45s/59s, no120s hang. Counts177581/1456520/6956183; Storelogical921712B/
allocated925696B; overlaylogical28672000B/allocated297111552B. Absolutecontroller
24,965,019,583ns minus6read-onlyobservers567,575,084ns matches recordedproductwall.
FinalretainedverifyPASS/reportonce; exact30/14-columnone-rowCSVs integrated.
Raw bothattempts/arithmetic/proofidentity/resource inventory remains namespace
worker benchmark-results/fs-bench-pro/2026-10-10-batch-3-local-snapshot-b1f8150eb/handoff.json.
Fiveoriginalpassingremovalreceiptsaudited, no teardownreplay. Initialmaster
layerfs-fsb-master-local-snapshot-ec2d6edff2f14397 retained; firstfailedlive
container/volumeverifiermount retained, includingmountkey
 a442fd804322640166fdeaa1d505535fbe1ea76e94417c63f7389969465f02fe.
No productbug/feature/optimization/limit/cache/workloadchange. FirstFAILrow
and correctedINELIGIBLErow remain separate at originalsources. Currentbatch
29of33distinctcasesclosed; Git4pending, no batch4. Production79,366unchanged,
referenceabsent, pinnedstagedcounter. Allsource/binaryreuse gaps remainexplicit.

| `a95ca5375` (worker) | Record corrected snapshot proof/closure withEXCEEDED | 79366 | 79366 | +0 |
| `70d568e94` | Integrate corrected snapshot while retaining failed history | 79366 | 79366 | +0 |

Git workflow support integrated from worker d73fb8a0841fd58e09a2895ed78ea9ed2ac24e20:
all four original IDs retain exact native Git preparation, genesis/configuration,
six performance commands, intermediate verification variant, editor temporary
saves and positional edits. Real .git is imported by Init; independent reference
HEAD/tree/parent and full working-source expectations cannot be replaced by
custody. Complete precommit and fresh-root .git manifests cover raw index stat
bytes before any semantic Git command. The persisted hybrid oracle uses custody
only for .git, retains the independent source oracle and complete inode census.
Lead controller wires prior mounted qualification discarded without Commit,
excluded precommit custody, fresh custody before semantic Git, acknowledged
root graph binding and cumulative proof work. Counts explicitly include custody
FUSE/SQL work; no guessed verifier subtraction or pure performance count claim.
Prior additional reads and full paid cache inputs remain unqualified under H5.

Root owning checks at this composed source: Git engine4PASS0.013s; family4PASS;
Rust --no-runPASS5.36s then exact five external Git oracle tests PASS11.65s,
nineteen frozen inline tests filtered. Worker had engine4/family4/shared16/
fixture6 PASS and native recipe ARM64 compilation PASS. Retain its earlier
unintentionally broad Rust invocation stopped by its owner at67s/exit101
while frozen inline history tests ran; this was not a product hang or successful
check. Remaining exact Git check subsequently passed1.81s at worker source.
Two harmless test-runtime dead-code warnings concern edit_model helpers; no
product source changed. These source-qualified checks are not mounted samples.

Read-only review found final controller publication and cleanup deadline scopes
needed completion before the first Git run. The frozen ordinary verifier has
45s work including a4s cleanup reserve, 59s hard through receipt publication,
and a0.25s publication guard. A worker is preparing an append-only late-failure
receipt overlay; lead owns controller wiring and meaningful ordering/deadline
checks. No Git sample is authorized by this intermediate support commit.
Compiled support may build and native preparation/parity may proceed in its
own worktree; samples wait for the reviewed final guard source. No batch4 work
has started; read-only planning uses exact existing dedup_branch_history name.
Raw support check/outcome provenance remains capped worker
benchmark-results/fs-bench-pro/2026-10-10-batch-3-git-tool-support-d73fb8a08/.
All previous failed/uncertain/protected resources and receipt identities remain.

| `ce47e2b18` (worker) | Apply lead shared source without ledger duplication | 79366 | 79366 | +0 |
| `d73fb8a08` (worker) | Original Git workload and complete custody support | 79366 | 79366 | +0 |
| `acebd7cbd` | Integrate Git support before final guarded-run qualification | 79366 | 79366 | +0 |

Git final controller review preserves the frozen ordinary policy: cumulative
proof work stops at41s (45s less original4s cleanup reserve), cleanup at45s,
and receipt publication at59s with original0.25s margin. Prior mounted original
verification and precommit custody are charged once, including their real
read/export/hash/cleanup walls. Measured performance remains outside proof time.
The current offline oracle requires early terminal unmount/known Stop/closed
export before graph reads; their cost remains conservatively charged before
remaining graph work. No new allowance, operation, cache treatment or timer
extension is introduced. Final counts/log collection, resource checks, passing
removal and stream closure are bounded in their declared work/cleanup domains.
The final prepublication proof wall includes these intervals; separate bound
publication observations retain hash/write durations, rather than substituting
an earlier graph-only wall. Prior/custody exhaustion fails before measured
Commit; no replay or automatic cleanup of an uncertain failure.

Root's six Git engine tests PASS at the composed controller, including exact
41/45/59 deadline arithmetic and rejection of changed budgets; one original
qualification discarded before fresh performance; custody before the single
Commit; fresh custody before any semantic Git; refusal/mismatch remain failed,
retain exact mounted ownership and charged proof wall. Shared engine16PASS,
receipt-custody4PASS, frozen Git family4PASS. Current tests use original declared
fixtures only where relevant; controller fault tests do not constitute samples.
No product source or compiled input changed after acebd7cbd's support seal.

H7 was found in the prospective publisher during source review, before Git's
first sample. External regression at the initial guard returned provisional
PASS/PASS after a late observation write and an OSError creating the failure
sidecar. Additional absent-manifest regression raised FileNotFoundError and
prevented a failed case CSV. Keep actual before-fix outputs in tiny worker
benchmark-results/fs-bench-pro/2026-10-10-publication-completion-guard/.
Original receipt/manifest remain immutable. Mandatory completion binds their
SHAs, source/product/case/image/binary identity, exact observation SHA, clock,
deadline, retained completion-attempt bytes and declared scope. A late or
interrupted attempt/link retains exact raw bytes and removes only its own
provisional successful completion link before optional failure emission; missing
completion fails closed even when failure emission also fails. A guarded attempt
with no original manifest reports FAIL using registration identity only; all
unauthenticated metrics are UNAVAILABLE. Existing tampered seals still reject.
All historic unguarded load/finalization behavior remains scoped unchanged.

Git full8-binary build at acebd7cbd PASS under its private120s lock in<=73s,
host release21.91s; pinned Linux source hash checks before/after and host source
unchanged. Passing build container removed, OWNER-3 exact refusal retained.
Runtime faf3b953674eb72ae7d6958141e99aba86d9ef2716f23cfea5ba0362731ece74;
workspace writer5fa075450ba74f3fea041d4a8212697dd766060defb8e1080cc76ff62d3bc13b;
daemon f44900cfc975901b7101f4874f708390b6f7401981e06d608b3df52a50848c0e.
Declared compiler cache copies are independent private byte copies of closed
main registry/incremental artifacts; actual8scope rebuild and source verification
still ran, no writable main target or binary substitution. Seal remains capped
worker benchmark-results/fs-bench-pro/2026-10-10-batch-3-git-full-build-acebd7cbd.
All4 native preparations and parity PASS at this source, zero samples, original
Git2.39.5 and independent HEAD/tree/parent/source/fsck/clean/two-commit guards.
Initial real .git/source logical bytes2,120,903/8,467,515/70,886,251/95,873,900;
complete files243/558/4850/10865. Transient bytes38,859,392/45,254,556/
104,085,977/129,928,663 retain original256MiB bound. Closed masters retained:
layerfs-fsb-master-git-tool-workflow-5126837eb6534685,
layerfs-fsb-master-git-tool-workflow-6808aa0ae3474ff4,
layerfs-fsb-master-git-tool-workflow-0b236dd02e6f4192,
layerfs-fsb-master-git-tool-workflow-24100f1f10984c27.
Passing native/provision helpers removed with receipts. Prepared parent and
parity remain under worker2026-10-10-batch-3-git-prepared-acebd7cbd and
2026-10-10-batch-3-git-parity-acebd7cbd; final guard-source parity still required
before sampling. No product bug or preparation failure; full coldness and all
original topology/format omissions remain explicit. No batch4 started.

The H7 completion review also reproduced actual process death: a bounded child
os._exit(91) immediately after provisional link creation, before its post-check,
left an old draft load reporting PASS/PASS. Raw failed-before-outer-ack.txt is
retained. The final bench.py driver invokes acknowledge_guarded exactly once,
only after one_family_case returns. A successful guarded report requires that
known outer return, exact completion SHA, original receipt/manifest identity and
nonpromoted returned verdict. Missing or partial acknowledgement is FAIL with
all authenticated metrics retained; altered bindings reject. Its emission is
explicitly outside the original59s family timer, and does not claim its own
unwitnessed write duration. It attests that the guarded finalizer returned after
its post-publication deadline check, closing the prior process-death gap without
rewriting any receipt or inventing a successful unknown operation.

Final Root composed checks:50 engine tests PASS0.531s (23 publication,7 Git
including actual outer driver ordering/refusal,16 shared workspace/report/cache,
4 original receipt custody), locked<=120s. Frozen Git family/catalogue checks
PASS; exact25families/273IDs retained. No native/mounted workload repeated for
these harness corrections. Worker final23 publication checks PASS0.258s and
16 shared checks PASS0.064s, strict original0.25 margin and monotonic numeric
clocks. Four worker purpose commits and original guard scaffold all have exact
pinned absent-aware+0 LOC/tree confirmation. No product change/optimization.
Compilation and prepared source checks remain source-qualified at acebd7cbd,
with unchanged compiled bytes; final guard-source parity precedes Git sampling.
Concurrent private worktrees and protected/retained containers share host and
DockerVM; this unquantified interference remains declared, no numeric eligibility.

| `b8f1d4c36` (worker) | Append-only original Git publication guard scaffold | 79366 | 79366 | +0 |
| `524a8bded` (worker) | Mandatory completion, late-link revocation and partial-manifest failure rows | 79366 | 79366 | +0 |
| `1ccdbeb65` (worker) | Freeze0.25 margin and validate monotonic numeric clocks | 79366 | 79366 | +0 |
| `3d92ebb10` (worker) | Require known outer finalization acknowledgement after post-check | 79366 | 79366 | +0 |
| `031d427a7` | Complete original Git deadlines and H7 guarded finalization before sampling | 79366 | 79366 | +0 |

## Batch3 closure

Git4 CSVs integrated unchanged from76545dd1bb3ae621d5877e864ce64338aba7daee,
exact30/14columns, four rows each, measured source031d427a7. Allfour once-only
cases PASS original mounted intermediate guards, six measured Git commands,
complete precommit/fresh.git index-byte custody before semantic Git, independent
HEAD/tree/parent/fsck/clean/twoGitcommits, current authenticated persisted hybrid
and exhaustive inode census, Gone/KNOWN_STOP and valid guarded publication plus
outer returned-finalization acknowledgement. Final parity and retained verify
PASS, reportonce. NumericfourINELIGIBLE: prior additional verifier reads/full
paid backing/metadata/SQLite/own-write coldness unqualified. Counters include
excluded precommit custody FUSE/SQL work; pure-command intervals are not claimed.
Raw family-handoff.json remains capped worker benchmark-results/fs-bench-pro/
2026-10-10-batch-3-git-report-031d427a7/. No failed/NA/exceeded Gitcase, product
bug, resample, optimization, changed limit/workload/fixture/cache/worker profile.

| Case tier | Complete ns | Combined prepublication proof ns | Requests/jobs/statements | Store logical/allocated B | Overlay logical/allocated B |
| --- | ---: | ---: | --- | --- | --- |
| 1 | 569462750 | 4085816083 | 5895/7423/45762 | 1516464/1519616 | 299008/268734464 |
| 10 | 882749792 | 4880959000 | 9394/12330/75496 | 4875688/4878336 | 401408/268836864 |
| 100 | 4612947959 | 15050067417 | 51386/69392/416614 | 64759408/64761856 | 1490944/269926400 |
| 500 | 10502533917 | 33127619000 | 118176/193539/1079116 | 80558592/80560128 | 4886528/273321984 |

Allfour passing sample pairs removed with original acknowledgements after
proof/Gone/KnownStop. Four closed Gitmasters listed above retained; earlier
capped/snapshot/R7 failures and owner-protected resources untouched. Worker
CSV-only countedtree2d93ae28ef468b07aa56686e0b51de3b1d3a3f7f confirmed/+0.
Root integration changes only the twoCSV files and thisledger. Product79366
unchanged, referenceabsent, pinnedstagedcounter/first-parent method.

Allfour batch3 families meet done-when:33 original IDs ported; familyparity,
single-source attempts, original applicable independent proof and cleanclose;
alltwoCSV files committed. Initial snapshot H6FAIL remains at0344bfdc6 and the
corrected sourceb1f8150e has its separate row. Two slow completions retain
unchanged15s reporting limits: localitydense50015,251,166,166ns and corrected
snapshot24,397,444,499ns. No speed/comparison/release verdict. Thirteen families
and131distinctcases are now closed acrossbatches1–3;132attempt rows include
the failed snapshot identity. Twenty-three original batch2failed cases remain,
and all their known reasons/resources are retained; no identified fixable
productcorrectnessbug remains. Later142cases/12families remain pending.
Batch4 has not been authored or executed; only originalsource read-only plans
were prepared concurrently. The next committed work activates its3families
under the same separate-worktree/current-batch-only policy.

| `76545dd1b` (worker) | Record four original Git outcomes without replay | 79366 | 79366 | +0 |
| `31fff4d49` | Integrate Git CSVs and close all33 batch3 cases | 79366 | 79366 | +0 |

Batch4 begins only after31fff4d49476219b9a172e010abe37f4e4056ee0 committed
batch3 closure. Three existing managed worktrees take exact original families
file_size_transition7, dedup_branch_history26, mixed_load_bearing10. Leadowns
shared Python/controller/public-runtime/census and ledger integration; worker
ownership is isolated for the native recipe CLI, independent model, P3 writer
and eachfamily. Old branches/raw receipts/resources remain recoverable. Private
locks/targets and sharedhost/DockerVM interference retain decision33's scope.
No batch5 and no stage4 sample while sharedsupport is unqualified. Routing adds
only these existing family names; pending cases remain unsampleable before
clone/mount/claim. Owning17 engine checks PASS0.107s, locked<=120s; the new
pending-family refusal uses isolated prospective registrations so it remains
valid after actual families are ported. No product/compiled source changed in
this routing commit, no new family or limit.

Read-only plans established three essential distinctions before authoring.
Dedup globalcensus must stream all current Commit rows, validate raw BLOB tags/
widths and complete five-field tuples, custody/hash/no-sidecar closure and real
current schema/binding; public typed proof remains mandatory. Pythonstandard
SQLite read-only/immutable/query-only/synchronousOFF can observe a separate
closedcopy without a product feature or data-serving path; no checkpoint,
repair, deletion or retry on that input. Extra valid unreachableCommit is the
required external regression, not only a malformedrow test.

M1 fixtures follow executable decimal high-water arithmetic: initial4,984/
29,984regularnames,99,934,464/499,934,464B, initial64/314dirs includingroot,
5,048/30,298entries; caps5,000/30,000names and100,000,000/500,000,000B reserve
16names/65,536B. Role-data dirs53/303 and additional10 come fromsource; frozen
fixture prose differs and staysunchanged. Common guard is per-file500MiB and
aggregate<1GiB, so no bypass is authorized. Two verify-only exhaustive cases
retain original120/300s declarations and currentowner120s testceiling separately;
never split/extend a test to wait out that ceiling. Current routines preserve
five sequential stages, original14POSIX+2orderedP3 edits at stage2, original
ranges including exact131072/empty-byte omission and required modes/attributes.
No concurrent discard/reopen is added to sequential cases. Automatic mtime
normalization is forbidden.

Boundary alias byte model requires original hardlink class forstates0–4,
separate alias/target at5, executable finaltarget131072 vsalias131071; roots
canrepeat but actualCommitIDs/parents cannot. Original workspace_bench2315–2347
and workspace_verify865–875 require final directorymtime; unchanged atomic
helper does not restoredata/ parent after temporary-name changes. The native
compatibility refusal stays strict and visible, not a benchmarkPASS or a new
workload call. Final caseclosure waits actual qualified source/evidence.

| `65f0c1fbe` | Activate only current three history-family routes while pending remains refused | 79366 | 79366 | +0 |


Batch4 shared native authoring integration reuses exact worktree changes:
Namespace10bafc75d/d81a8af86/f97230027 (BA hardlink model, independent M1
shadow/range/explicit-attribute algebra and exact decimal constants),
Cappedf938d7faa/f5280d654 (selected native checkpoint and M1 selective CLI),
Tinye5e8216f3 (ordered ordinary P3 splice members). Original frozen source and
all product sources remain unchanged. Private native recipe failures, including
Capped's frozen-private validate_path compiler refusal before its owned-parser
correction, remain raw authoring history; no product failure or sample is
inferred. M1 native selective verification accepts only original cumulative
17-name created-directory mode domain and required subset, rejects unsafe,
duplicate/missing modes and declared-range corruption, and deliberately does
not invent automatic mtime or exact128KiB byte coverage omitted by the original.

Lead exact composed qualification: external Rust tests compiled --test first
with Rust1.85.1/repository ARM64 flags; BA3PASS0.02s, M1 model5PASS2.27s, frozen
inline reference tests filtered from this focused invocation. Fresh native
recipe compiled from composed source;7Python native CLI checks PASS4.470s.
Fresh native writer compiled from same model;5covering native checks
PASS1.305s. Writer tests substitute only load_build with these exact fresh host
binary paths for uncommitted direct-source authoring qualification; no clean
full-build seal, mounted operation, preparation, parity or measurement is
claimed. Raw source/binary hashes and all build/test attempts are under
benchmark-results/fs-bench-pro/2026-10-10-batch-4-native-composition-root,
-root-writer and-root-writer-launcher-fix. Initial writer launcher had a
SyntaxError because token replacement also touched its environment-variable
name; it failed before loading/running tests. Its exact failed receipt remains;
corrected launcher passes using unchanged compiled binaries. Every invocation
has an explicit120s wallstop under the main checkout lock. No Docker resource
or sample is created by these authoring checks. Full sealed Linux/host support
qualification waits the actual final composed batch4 source.

Ownership for remaining shared authoring is explicit: Tiny owns selective
preparation and untimed sealed SDK payload descriptors; Namespace owns persisted
public proof and the resolved history-step argument guard; Capped owns the
independent unfiltered closed SQLite census; lead owns Python controller/build
assets/finalization and thisledger. Workers are forbidden to revert sibling
changes or sample until the reviewed source/seal handoff.

| `10bafc75d` (worker) | Original boundary alias class model | 79366 | 79366 | +0 |
| `d81a8af86` (worker) | Original M1 independent shadow and ordered P3 model | 79366 | 79366 | +0 |
| `f97230027` (worker) | Exact decimal authoring constants | 79366 | 79366 | +0 |
| `f938d7faa` (worker) | Native selected checkpoint recipe | 79366 | 79366 | +0 |
| `f5280d654` (worker) | Native original selective M1 proof | 79366 | 79366 | +0 |
| `e5e8216f3` (worker) | Generic ordered ordinary SDK splice writer | 79366 | 79366 | +0 |
| `a921bedbd` | Integrate and qualify batch4 native model/recipe/writer | 79366 | 79366 | +0 |


Prospective H8 correction retains exact successful sample resources pending
finalization/known driver return. Original End, automaticGone, Stop, export,
independent proof and deadlines stay unchanged. Administrative Docker removal
moves after successful guarded outerACK (where required) and authentic final
functional/proof/cleanup custody. Separate resource-removal attempt/completion
and manifest bind the original receipt/manifest/ACK and exact64-hex container /
owned sample volume. Any finalization failure attempts no deletion. A first
successful deletion followed by missing volume acknowledgement records
containerREMOVED / volumeREMOVAL_ATTEMPT_UNCERTAIN; no inferred replay, retry or
both-retained claim. Original receipts remain immutable and report loading
validates the separate administrative custody without promoting a verdict.

External regression at exact parenta921bedbd observed both original rm calls
before output-closeFAIL, so failed as intended. Corrected source3retention
checks PASS; existing7Git controller checks and23publication checks PASS,
explicit120s wallstops/main lock. Rawbefore/after evidence is append-only under
benchmark-results/fs-bench-pro/2026-10-10-h8-resource-retention. No actual Docker
resources, prepared inputs or benchmark samples were created by these isolated
controller tests; old recorded removals/receipts remain unchanged.

| `56a422544` | Retain failed finalization resources and bind separate administrative custody | 79366 | 79366 | +0 |


Batch4 full builds now compile/archive the same workspace native recipe CLI
for Linux as workspace-linux, alongside its host workspace-writer. This
supports actual original selective M1 native proof in the pinned image rather
than routing the frozen generic expected() pristine fixture as M1's oracle.
There are nine prospective sealed binary keys; runtime-only rebuilds require
the nine-key full seal and reuse eight unchanged binaries. Closed eight-key
build receipts remain immutable; current changed recipe/writer scopes already
require a new full seal, so none is relabelled as nine-key qualification.
Linux compilation retains host source checksum validation before and after,
locked offline daemon build, pinned image and repository ARM64 flags. Full
build/execution has not occurred yet. Owning8build-identity tests PASS0.018s,
including refused missing workspace-linux reuse;120s wallstop/main lock;
raw benchmark-results/fs-bench-pro/2026-10-10-batch-4-linux-recipe-asset-authoring.
No product source or Docker resource changed in this authoring check.

| `cc8c239b8` | Seal Linux copy of current native selective recipe in future full builds | 79366 | 79366 | +0 |


Batch4 file_size_transition authoring is integrated from Tinyff03191bd,
e4f7f76cb and4b18789cf: exact seven originalIDs, all19Commit steps and26
initial/published states, original overwrite/roundtrip/alias helper calls,
strict exclusive131072-byte representation contract, every0..N proof index,
and sealed prepared-P3 payload hook with unchanged native fallback. Limits,
fixtures, worker count and original alias syscall sequence are unchanged.
Lead5owning definition/shell/native-alias checks PASS with explicit120s stop;
exact new native binaries substituted only for authoring load_build, no full
clean seal or sample claim. The unchanged BA helper completes allfive calls,
keeps expected complete file bytes/identity/link classes, and its final
strict native metadata checker refuses changeddata/mtime as predicted. This
is retained definition incompatibility, not a passing benchmark or authorized
parent-restoration change. Raw qualification:
benchmark-results/fs-bench-pro/2026-10-10-batch-4-boundary-family-root. Actual
family parity/preparation/one-attempt case execution remains pending final
shared support. No product source or Docker resource changed.

| `ff03191bd` (worker) | Register original seven boundary cases and calls | 79366 | 79366 | +0 |
| `e4f7f76cb` (worker) | Native alias strict compatibility qualification | 79366 | 79366 | +0 |
| `4b18789cf` (worker) | Bind every boundary proof step and prepared SDK payloads | 79366 | 79366 | +0 |
| `fc9e71fae` | Integrate original boundary family with owning native qualification | 79366 | 79366 | +0 |


Batch4 dedup_branch_history authoring integrates Capped2789b9dac,
44f2048c4 and26e414cb0:26original IDs and3,385orderedCommit steps,
unchanged seed1 fixtures and original checkpoint lists/budgets. Full routine
checkpoint coverage for unrelated-mixed-v2 is explicitly additional proof;
frozen executable2534–2597 checks global measuredCommit membership/parents
and final bounded payload sample before bypassing the routine fast checkpoint
loop. Nativegenesis is a Layer, not a fabricated Commit: current Init also
starts branchheadNone, transcript hasK+1roots but globalcensus must haveexactK
measured CommitIDs with firstparentNone. Prepared descriptor hooks preserve
original ordinary SDK member order and native fallback, including HNstage2.

Reuse exact owning worktree evidence by final changed-file scope: initial6
family checks PASS0.338s; affected mixed-v2 scope check PASS0.003s after44f;
affected shell/prepared descriptor route check PASS2.650s after26e. No changed
scope remains unqualified, and no duplicate native preparation or performance
sample is used. Raw receipts and each worker countedtree remain at
benchmark-results/fs-bench-pro/2026-10-10-batch-4-history-authoring in the
Cappedworktree. Strict immutable globalcensus and composed persisted/native
proof support are still authoring prerequisites;26actualcases remainunrun.
No product source or resources changed by this port.

| `2789b9dac` (worker) | Register all26 original branch-history cases | 79366 | 79366 | +0 |
| `44f2048c4` (worker) | Label original mixed-v2 final sample versus added routine full proofs | 79366 | 79366 | +0 |
| `26e414cb0` (worker) | Bound prepared SDK descriptors with native fallback | 79366 | 79366 | +0 |
| `6a5433e79` | Integrate original dedup branch history family | 79366 | 79366 | +0 |


Batch4 mixed_load_bearing authoring integrates Namespace96d472294,
319fad349 and13f413bee:10original IDs,4ordinary single-call agentepisodes,
4regular sequential M1 cases,2original exhaustive verification-only IDs.
M1declares every ordered helper/Commit and two separate stage2 ordinaryP3
members, selected original checkpoint roots (finalregular /20exhaustive cycle
ends), allactualCommitIDs/parents/head/globalcensus, selective original ranges
and explicit metadata with actualcreatedmode custody. Exhaustive performance
isN/A, original120/300s declarations retained beside owner120s testceiling;
no performancezero, fabricated coldness or automatic metadata normalization.
Original decimal fixture/caps and common guard arithmetic are recorded above.

Exact final changed scopes reuse owning private120s evidence: original/schema
3checks PASS2.024s after319f; finalprepared-script3checks PASS3.600s after13f,
including missingdescriptor seal refused before mutation and unchanged native
helper/member1/member2 order. Sourcefile hashes and counted worker trees pin
this reuse; no repeat of an unchanged benchmark. Raw owning summaries:
2026-10-10-batch-4-mixed-registration, -mixed-history-schema and prepared-hook
in Namespaceworktree under benchmark-results/fs-bench-pro. All10actualcases
remainunrun untilcomposed support; no product source or resources changed.

| `96d472294` (worker) | Register all10 original mixed cases and exact fixtures | 79366 | 79366 | +0 |
| `319fad349` (worker) | Use approved original history checkpoints/census schema | 79366 | 79366 | +0 |
| `13f413bee` (worker) | Preserve helper-first two-member prepared P3 order | 79366 | 79366 | +0 |
| `4762b720c` | Integrate original mixed load-bearing family | 79366 | 79366 | +0 |


Batch4 selective native preparation and original untimed P3 payload support
integrates Tiny7f381e27c/f439855d7/5a7dac7a6/9a434a6f5/a05b850dd.
Only declaredverification_steps have native expected trees/manifests; every
actualCommit remains in the separate transcript. M1 selected manifest is the
original8-column selective shadow scope, while full digest/None-created-mode
materialization is explicitly setup-only native recipe evidence. No501copies
or extra all-state payload claim. SDK generators run untimed; descriptor/member
bytes and SHA seals are staged, and measured wrappers read/hash-check the
originals without regenerating recipes. Each wrapper retains originalrun,
operator/member order, descriptor/hash and helper stdout. Successful positional
artifact reads advance by bytes; error/zero/checked-close failure stops, without
convenience EINTR retry. Linux native recipe stages aswriters/recipe, matching
original family fallback and exact sealedworkspace-linux asset.

Authoring corrections: the initial staged asset namewriters/workspace did not
match the family recipe fallback; affected worker test failed with missing
writers/recipe before9a, thenPASS0.080s. The unexecuted M1 prepared test initially
omitted the original14POSIX stage2 edits; source review corrected it ata05b to
call unchanged nativehelper before both preparedmembers, retaining strict
selective verifier. These are harness/test authoring corrections, no product
bug or failed measuredcase. Native prepared writer compiled at composed final
source first with1.85.1/ARM64;3newcovering tests PASS3.072s. Existing unaffected
native writer evidence remains;12owning fixture checks PASS0.157s, allunder
explicit120s/mainlock. Raw direct-source authoring binary/hash qualification
benchmark-results/fs-bench-pro/2026-10-10-batch-4-prepared-support-root; no clean
full-build seal, actualprepare/parity/sample/Store execution or Dockerresource
claim. Final full sealed Linux/host build waits runtime/controller composition.

| `7f381e27c` (worker) | Untimed sealed original payload generator and measured prepared operator | 79366 | 79366 | +0 |
| `f439855d7` (worker) | Positional no-retry artifact read and checked close | 79366 | 79366 | +0 |
| `5a7dac7a6` (worker) | Original selective checkpoint/payload preparation | 79366 | 79366 | +0 |
| `9a434a6f5` (worker) | Correct native M1 staged recipe path | 79366 | 79366 | +0 |
| `a05b850dd` (worker) | Include original14POSIX edits before test's two P3 members | 79366 | 79366 | +0 |
| `364e1a87a` | Integrate selective preparations and qualify original prepared splice support | 79366 | 79366 | +0 |


Batch4 unfiltered closed-Store census source integrates Capped70c61a91f.
It streams every globalCommit row toEOF using read-only immutable/query-only
SQLite/OFF/timeout0; exact K uniquefive-field raw typedBLOB tuples, actual
schema/application/source/definition identity, provisioning binding and current
mandatory public provider validation are required. ExportKnownStop/container/
volume/local/remote paths/hash/size and actual original successful Dockerexport
completion/runtimeevents/manifest/transcript hashes are independently bound.
No cursor or binding-key material is returned. Public typed offline proof runs
on a separate bytecopy; original census export must remain unchanged and free
of all-WAL/SHM/journal sidecars evenempty.

Eight owning schema/census unitchecks PASS0.291s and existing3oraclechecks
PASS0.033s underprivate120s locks, exact unchanged integrated filehashes reused.
These use isolated synthetic six-schema WAL/OFF SQLite fixtures and mocked
provider/custody attestations; extra unreachable tagged/FK-bound row is rejected,
130rows prove bounded batch-toEOF, and substitution/missingtuple/type/tag/schema/
binding/sidecar/hash/custody corruption are refused. They are not yet the
required genuine public canonical extraCommit regression: source review found
fixture IDs/rootbytes are invented, not actual publiccreated savedobjects.
Namespace/Capped are preparing a separate current-publicStore extraCommit
proof with anchoredtypedverificationPASS and unfilteredcensusFAIL. Decision40
qualification remainspending until that actual external test; no productfeature
or claim from syntheticvalidity. Rawowning tests/census-supplement.json remain
in Capped's2026-10-10-batch-4-history-authoring folder. No realcase/export/resource
was created by the synthetic checks.

| `70c61a91f` (worker) | Unfiltered immutable current-schema Commit census and synthetic guards | 79366 | 79366 | +0 |
| `1fa7facc4` | Integrate unfiltered census, retaining genuine public extraCommit test prerequisite | 79366 | 79366 | +0 |


Batch4 guardedpublication integrates Cappedc8a5a72db. Exact closed Git scope
string remains unchanged; new HISTORY_PUBLICATION_SCOPE identifies original
family cumulative proof/publication separately. Only those two scope constants
are admitted; same0.25s margin, immutable receipt/manifest/observation, guarded
completion attempt/link/post-check and mandatory known outerreturn apply.
Driver acknowledges every guarded result before existingH8administrative
resource removal, independent of family name. No original receipt/verdict is
rewritten. Owning newhistory3checks PASS0.009s, retained oldpublication23checks
PASS0.234s and2driverorderingchecks PASS0.008s, eachprivate120slock; final exact
changed sourcefilehashes reused. No actualcase/export/resource executed;
controller original work/reserve/hard budget integration remainsuncommitted
and unqualified until its own coveringtests. Rawpublication supplement is in
Capped's2026-10-10-batch-4-history-authoring folder.

| `c8a5a72db` (worker) | General original-family guarded publication and outer acknowledgement | 79366 | 79366 | +0 |
| `5ebdac535` | Integrate family guarded publication without altering closed Git scope | 79366 | 79366 | +0 |


Batch4 verification-only reporting integrates Capped6d47e3be3/2fe8e9b2e.
Authoritative original case.proof_only masks performance status/cache/time/
phases/limits/counts/storage/reference timing as literalN/A, including unrun
cases. Verification retains actualsemanticproof status/wall/originaldeclared
bound/cleanup/source and reasons, with original rawdiagnostic metrics unchanged.
Driver requires verification-only mode, sample_count0, verification_attempt_count1;
proof success does not credit any performance cache state. Neither case is
closed as not-applicable simply because performance isN/A. H9visibility fix
retains original overallFAIL and exact original/close cause in CSV tags even
when semanticbytespassed. Red regression remainsraw, final5reportchecks
PASS0.048s and4driverchecks PASS0.020s underprivate120s locks. Integratedfinal
receipts/bench hashes equal worker's f1ac20a... /70580094..., so covering exact
scope evidence is reused. No sample/preparation/resource has executed yet.
Controller owner120s invocation ceiling remains separate from original120/
300s proof definitions and is not a widened or silently rewritten declaration.

| `6d47e3be3` (worker) | Verification-only performance N/A and explicit no-sample driver | 79366 | 79366 | +0 |
| `2fe8e9b2e` (worker) | Keep overallFAIL/close cause visible beside semantic proofPASS | 79366 | 79366 | +0 |
| `65df42e91` | Integrate verification-only reporting and H9 outcome visibility | 79366 | 79366 | +0 |


Batch4 controller authoring integrates exact current interfaces: every ordered
acknowledgement, sparse original checkpoint manifests with '-' sentinels,
prepared_step_runs dispatch, perstep original semantic counters, helper-only
actualcreatedmode cumulativeprefix TSV/hash seals, finalK native proof, and
separate typed-provider bytecopy/immutableexport. Public-provider fields and
allactualCommit tuples/source/runtime/exportcustody feed the unfilteredcensus.
Full mandatory public proof and actualcanonicalextraCommit qualification still
wait Namespace/Capped; isolated controller mocks explicitly claim neither.

Original25/35 or45/59 work/cleanup/publication budgets retain3/4s reserves and
0.25s publication guard, including allfour ordinary agentepisode cases. The
separate owner120s exhaustive invocation cap retains its3s cleanup reserve;
original120/300definitions staydeclared and performance remainsN/A. Source
admission/setup/allcalls/Commits/proof/cleanup/Stop/close are recorded as the
exhaustive testinvocation wall, with zero performance samples. Ordinary source
15s reporting limits remainsoft; existing hard operation budgets are unchanged.
New ordered phase watchdog includes its measuredmount; old previouslyclosed
Small/Local timer wiring is retained. Resourceobservers retain raw excluded
wall and remain charged to applicable absolute stops, rather than each gaining
a fresh full allowance. NativeM1 JSON is parsed as its exact selective receipt
(case/seed/K/exit/scope/rangebytes/digests), never generic full coverage.

Lead covering5componentchecks initiallyPASS, existing7Git controller and3H8
retention checksPASS, then new full sparse100-ack orchestration test exposed
an isolated test-fixture path alias (/var versus canonical/private/var), not
an actual product/case failure. Failed6-test invocation remainsraw; testfixture
nowresolvesitsroot, matching real controller canonicaloutput, and final6checks
PASS0.061s underexplicit120s/mainlock. No weakened path/ownership guard. Raw
2026-10-10-batch-4-controller-authoring-root plus-orchestration and
-orchestration-canonical-path under benchmark-results/fs-bench-pro.
No actualclone/mount/Store/benchmarkcase was executed by these isolated tests.

Read-only worker review found a remaining mandatory preparation guard gap:
missing prepared_step_runs could silently choose native SDK fallback and
regenerateoriginalbytes inside measuredwork. Tiny owns a fail-closed helper/
SDKstep seal guard; leadwillwire it before claim/clone. No batch4case mayrun
untilthisguard, currentpublicoracle/canonicalcensus test andfull9-asset seal
qualify. This is a source authoring checkpoint, not batch4completion.

| `686344e8f` | Wire original history checkpoints/mode custody/census/deadlines and verification-only execution | 79366 | 79366 | +0 |


H10 integrates Tiny a8c56d570's read-only mandatory helper admission and wires
it before output directory, caseclaim, clone or mount. Exactoriginal SDKstep
indices derive from source case definitions (M1/HNstage2; otherSDK everydeclared
step; originalnonSDK none), notoptionalpreparedrecords. Generator edit-host
binary/sourceargv, current workload/edit/M1recipe runtime seals, source/staged
descriptor/payload geometry/metadata/SHA, exactoperator/member/run/script text
and helperasset set are bound. Missing, partial, swapped, stale or extra
SDKrecords cannot invoke nativefallback/regeneration in measuredwork. Existing
nonbatch4admission staysunchanged. Positive cases preserve original prepared
member order and partial/nonSDK fallback onlywhereoriginaldeclaresnoSDKedit.

Worker15owning byte/seal checks PASS0.273s and five explicit legacyadmission
refusal regressions FAIL beforeguard in0.012s, no product/resources. Lead actual
controller boundary regression against parent686344e8f failed because original
code created a claim/attempted clone rather thanraising the guard refusal.
Corrected7controller checks PASS0.082s;15fixture/admission checks PASS0.242s,
all120s/mainlock. Full sparse control unit uses opaque assets and explicitly
mocks admission/census onlyforchoreography; separate real temporary byte/seal
checks cover admission. All failed/green raw evidence remains at
benchmark-results/fs-bench-pro/2026-10-10-h10-prepared-helper-admission-root and
Tiny's2026-10-10-batch-4-selective-preparation-authoring. No actualcase/clone/
mount/Store was created; no product source changed. Currentpublicoracle and
actualcanonicalextraCommit qualification remain the last source prerequisites
before clean fullbuild/prepare/parity/oneattempts.

| `a8c56d570` (worker) | Fail closed on original prepared SDK placement and current helper seals | 79366 | 79366 | +0 |
| `91cf97fab` | Wire H10 admission before claims/clones and qualify exact guard boundary | 79366 | 79366 | +0 |


Batch4 public persisted proof source integrates recovered Namespace62246886d.
It covers exact boundaryallstates/currentmapping/alias serial+root retention;
frozen dedupselectedroot/metadata/A-B contentroot stability; original M1
selectiveranges/explicitattrs and measuredmode domain; allactualK IDs/parents/
root/scope/profile/stack/base/finalhead and provider catalog/binding/input+after
SHA/size. Args history-step bounds derive from originalcase, while exact
selection is additionally checked bypublicproof. LegacySmall30 rootdistinct/
delta condition andLocal3 repeatedroot/fourfullstate guards remainstrict.

Two agent service/remote-compaction failures (88524d1d-017d-46cb-8cff-
461dbfe33f9c;6b0c659b-1cf9-49ff-96b7-ee6d6e2ab210) are coordination errors,
not testcase failures. Fresh history_runtime_finish reused the same existing
namespaceworktree and exact source. Retained finalboth-target no-runPASS10.06s,
6owned batch4_ tests PASS5.45s (19frozeninline filtered), exact finalsrc hashes
match; no repeat. Earlierunfiltered21PASS4FAIL103.95s<=106/120 remainedraw;
fixtures had unsorted directory changes/missingemptyDirectoryUpdate/65536B
mapping extent above32768 max. Source-valid fixtures repairedthese without
changing any oracle/productguard. No wall ceiling expired. Scopedrustfmt1.85.1
andwhitespacePASS. Recovery also found no prior exporter; it invoked the single
exactexporter once,1PASS0.04s/1,574,447,000ns supervisorwall<=3/120, actualpublic
canonicalC3retrieved/derivechecked/savedroot; Capped'srealimmutablecensus proof
is integratedseparately. No mountedcase/Dockerresource/measurement here.

Lead covering legacyoracle validation at exactintegrated runtime source:
locked/offline release+1.85.1/ARMflags no-run firstPASS8,966,701,042ns;
2Small/Local historytests PASS10.90s (externalwall11,063,823,250ns),
2genericworkspace tests PASS0.56s (1,431,517,667ns),5Gitcustody tests PASS1.37s
(2,227,285,250ns). Each mainlock<=120s; frozen19inline namespaces filtered
perinvocation, no preparation-dependent unrelatedsuite run or case replay.
Raw benchmark-results/fs-bench-pro/2026-10-10-batch-4-legacy-oracle-checks-root.

During private publicoracle recovery, lead clean main91cf97fab built final
nineassets once: raw2026-10-10-batch-4-nine-asset-build-91cf97fab, host+Linux
PASS, completeouterwall<=51/240s build(not test); each actualcompilercommand
<=120s. Pinnedimage and hostsourcechecks insideLinuxbefore+afterPASS,
explicitDisposable/overlayMEMORYOFFEXCLUSIVE/workers1, exactOWNER3two-fuser
refusals remainFAIL; noDurable executed. DaemonSHA remainsf44900cf..., new
Linuxrecipe17aff632... andLinuxpreparedwriter846366af... sealed/archivebound.
Passingownbuildcontainer removedwithacknowledgement; no volume or sample
created. This first nineasset seal stillcontains pre-B4publicoracle runtime
7c66ee0a... and claims compilation only; currentchanged runtime will be rebuilt
with exact eightasset reuse before anyfamily preparation/measurement. No stale
old handler is qualified as a B4execution path. Producttree/79366 unchanged.

| `62246886d` (worker) | Currentpublic full/selected history proof and original step bounds | 79366 | 79366 | +0 |
| `93f0d3dfe` | Integrate public history proof, preserved failures and qualified unchanged assets | 79366 | 79366 | +0 |


Decision40genuine-extraCommit qualification closes with Capped0a6887af4,
dependency Namespace62246886d. Dedicated native publicfixture buildsactual
savedfilesystemobjects andK2 history, publishesC3 via publicHistoryCatalog,
checksCommitId::derive andretrievesexactrow. C3 parent isC2, root is savedC1
and differsfromC2. AfterHandles::seal, onlyisolated WAL/OFFcopy branchhead
is restoredC3toC2; allthreeCommit rows remain. Actualruntime anchoredoriginal
K2typedproofPASS on separatecopy, then exactlysameimmutableSQL/schema/provider/
tuplecore rejectsvalidunreachableC3 as expected. No fabricatedDockerStopped/
exportevents are used: publicsamplewrapper stillenforces actualDockerKnownStop
custody; nativefixtureexercises privateunchanged-readonlycore explicitly.
Synthetic8unitguards are now prospectively labelledsynthetic, receiptsunchanged.

Singleexactexporter1PASS0.04s reused; oneactualPython regressionPASS2.857s /
2,943,888,792ns supervisor<=4/120, typedchild25s stop. BaselinecensusPASS2;
retainedextra EXPECTED_FAIL 'global Commit membership or five-field tuples
differ'. Immutable reset217,088B/hash860b1193c6f05cde5bab691ed4f7016cf2a2d88c9c1fce136940e01a6c0a5c48
staysunchanged. CanonicalC3
1213fdbaa48a28294b483f5d63051ec2b11b42b80ffe4b8df3f3d0632194a0aeb1.
ActualprivatequalifiedruntimeSHAaacd4e5867495f2036d6ae6a216aa32fbbb4f57d663d09a3e76c1be3c17dcdfb.
Postfactor8syntheticguardsPASS0.327s. RawCappedcanonical-census-fixture/
python-canonical-check/qualification.json SHA5a4e106a30bcbb3994aba695c36bf7fcfe5c4a21d5a9aae469f4869012ce9dfe;
canonical-census-supplement.json SHAfc582d62e7e8b26ad3a0f7069debe0e1a020ae63b7be0f7ddf8c6dfd38b79426. Rootcreates no
secondfixture or unchangedproof replay. No mounted/Dockercase/preparation/
measurement/resourceoperations. Final source/binary handoff and familyparity/
actual43oneattempts are next; batch4remainsIN_PROGRESS andall43UNRUN.

| `0a6887af4` (worker) | Qualify actual canonical unreachableCommit and exact readonlycensus | 79366 | 79366 | +0 |
| `7d9380fe0` | Integrate genuine publiccensus regression and close native qualification gap | 79366 | 79366 | +0 |


Finalbatch4 hostruntime seal uses cleanmain7d9380fe0 and qualified fullnineasset
build91cf97fab via bench.py build --reuse-unchanged. Hostlocked/offline release
+1.85.1 PASS, external120s mainlock<=3s; newruntimeSHA
4f1a7d154ef588498116220e2c052d1d54a6000be10685fdb1bced1f03d59db6,
4,654,608B. Exacteight nonruntime binary hashes/profile/ARMflags/image/product/
compiledsource scopes and originalLinuxbefore-after sourcechecks are validated
from immutable fullbuild receipts, notrecompiled or relabelled. Currentfinal
build manifests allnineassets, sourcebefore/afterclean equality and unchanged
producttree. NoDockerresourcecreated byruntime-onlybuild; firstfullbuild's
passingcontainer alreadyremoved. Raw2026-10-10-batch-4-runtime-seal-final.
Durable disabled; explicitDisposable/WAL/OFF, overlayMEMORY/OFF/EXCLUSIVE and
workers1 remain. Fullsource/native/public/census/controller admission checks
are nowqualified; no new productsource/dependency or optimizerwork.

Workers receive currentmain source snapshot and thisseal, synchronize only
reviewed harness changes in their existing three worktrees with own source /
scopedexact counter commits, then preparetheir currentfamily, runbenchparity,
and oneattempt per originalcase. Familyvariants/limits/cache/worker counts
stayfixed, everyfailed/ineligible/unrun outcome retained and resourcecustody
recorded; no laterbatch or unchangedresampling. Nativealias scope already
qualified atfc9e71fae; only genuinelyaffected finalsource checks are permitted.
All43actualcases stillUNRUN at this checkpoint, before dispatched preparation.

| `a70f3197b` | Record final hostruntime seal with qualified eight-binary reuse and dispatch current families | 79366 | 79366 | +0 |


The batch4 workers imported reviewed maina70f3197b harness bytes into their
same existing private worktrees. Exact synchronization commits are
Tinyd57450ed36b1b1ec12d9ca3b45ac3870cf9a0a36 and
Capped7e594f61579dfc7da1b1c96a5608715c7cd456c0, both tree
2c59ed1088c0cdb56189029e267ce974de480532; mixed4c83d1c79.
Each reported exact final nine-binary/compilation-input validation, unchanged
owned family/workload bytes, clean source and unchanged79366 productionLOC.
No redundant build or already-qualified native/public proof was replayed.

Boundary preparation7/7 and parity7 PASS; dedup preparation26/26 and parity26
PASS with zero samples during parity. Closed setup masters are reused only
through exact immutable fixture-digest compatibility. Boundary retains six
owned immutable masters ef90327722e54bf2,31d03200fe4a404e,
838717697a554c50,70cc7bd57bfe4b21,239e74723af54fe8,083f065425ff4f29
(fullprefix layerfs-fsb-master-file-size-transition; exact names remain in
raw prepared.json records). Dedup retains five owned immutable masters
2550381b32d04032,9eeb4a6867d044b0,1e35ef1e53ab4836,
b32f7508b7d24f29,e7d5133801934842, fullprefix
layerfs-fsb-master-dedup-branch-history-. These closed setup volumes are
retained for declared setup reuse, never reused as mutated measurements.
Mixed preparation9/10 PASS at this checkpoint; exhaustive100MB preparation
70.123s under120s, exhaustive500MB preparation underway. No whole-family
mixed parity or actual case outcome is claimed by this checkpoint.

Actual original case executions are dispatched after each family's parity.
Every worker invocation uses its private r4 lock and an explicit external
wall stop at most120s. The original internal operation/proof/publication
limits are retained, including declarations which can exceed the outer
ceiling in combination. The outer ceiling is never extended to accommodate
those declarations; expiry remains FAIL with diagnostic resources retained.
Separate worktrees serialize their own builds/tests/samples, while host and
DockerVM interference from concurrent workers, protected owners and retained
failed containers remains unquantified and disclosed. No unchanged repeat,
case reduction, workload/limit/cache change or optimization is authorized.
Root owns integration/ledger/product fixes; workers own disjoint families and
report exact failed/uncertain resource custody before any corrective source
change. Raw roots are the workers'2026-10-10-batch-4-file-size-transition-d57450ed3,
2026-10-10-batch-4-dedup-7e594f615 and mixed current run under their private
benchmark-results/fs-bench-pro directories. No later batch has begun.

| `87857f5bd` | Record qualified parallel preparation/parity and fixed invocation ceiling | 79366 | 79366 | +0 |


Batch4 first actual attempts expose H11 in the controller, not an identified
product history defect. Boundary small-control at privated57450ed3 acknowledged
both Commits, native verificationPASS, both mounted viewsGone and runtime
KNOWN_STOP. Its191,549,251ns product wall and1,570,163,833ns failedproof wall
are retained; exact14requests/192ownerjobs/857statements. Failed container
57aaa6346ac4cdeed0ce952ffd44d35d3d48cfd28b55b7a6e06835143b236813 and
volume layerfs-fsb-sample-f09bfe9cc54f40f4 remain retained. Raw closed main
180,224B/SHA7f6613a45069123e8b226782402c2202885c56a29aa752a2c585a6772a9d1dfc
matches initial clone; exported WAL181,312B/SHA
5d0a87a2b6faea60ba812aa8feb2d003b47d38fc447ebaa66a501a6a420e49f3
and SHM32,768B remain present. The separate typed copy omitted them and
therefore exposed only the initial state. Six other boundary cases remain
NOT_RUN at this source. Two CSVs preserve allseven rows, worker882f50cb8.

Dedup distributed1 at private7e594f615 likewise acknowledged actualCommit
1216470861ade6aff4c3e208f2879316b2ff9ed5aed3aa1095c970428a2df284b3,
nativeproofPASS, mountsGone/KnownStop;158,878,375ns product wall,
2,268,414,542ns failedproof wall;10requests/102jobs/466statements.
Storelogical1,434,304B/allocated1,437,696B; overlaylogical245,760B/
allocated268,681,216B. Failed container
e06cd7ce378329d80fba7f223d5e563bb6b310f235cac8af556ea95709ed4b24 and
volume layerfs-fsb-sample-e18eab20c1ee4f61 remain retained. MainSHA
d708e027865b52341e0adcab3e7ba4303597b18af4aed180b916513c37e24440
matches originalclone; WAL115,392B/SHA
cb05890a334878460c49b18ef741666113d38ca87f2bf3e60bee9c995d82ac83
and SHM32,768B were omitted by typed copy. Twentyfive othercases NOT_RUN.
Both26-row CSVs preserved, workera38242d7b. No limit miss in either sample,
no failedresource deletion, no replay; all furtheractual B4cases stopped.

Root read exact failed graph stderr/completions/receipts and traced the copy
at workspace.py to the mandatory unfiltered census's sidecar-free contract.
A real SQLite/WAL external controller regression keeps100 acknowledged test
rows in copied WAL; immutable main-only typed copy sees0, producingFAIL.
Red invocation0.067s/120mainlock remains at
benchmark-results/fs-bench-pro/2026-10-10-h11-closed-wal-controller-red.
This isolated orchestration fixture has synthetic rows, explicitly not a
canonical product/measurement proof. Prospective correction preserves raw
KnownStop export and binds a separate WAL-aware standalone snapshot to every
raw file hash, then uses independent typed/census copies. Sidecar refusal,
all original globaltuple/provider/schema/custody/deadline guards remain.
No product source/fix/performance claim follows from this retained failure.

Mixed has no actualcase/claim/sample resource. Nine preparations PASS; the
last v016-mixed-exhaustive-500mb-30000-k100-v1 preparation reaches external
120s ceiling: exit124,wall120,033,743,083ns, no prepared.json. Fifteen selected
cycle-end native states5..75 completed; step80 hasstdout297B/stderr0 but no
completion acknowledgement beforekill and remainsUNKNOWN, notPASS. Those
15completed writers total101,214,199,501ns, each6.389..7.299s and499,934,464B
logical paths; no individual state hang/native error/unsupported recipe was
observed. Source creates original full native fixture/manifest for each of
20selected states; this setup scope remains fixed. Partial preparation is
retained, never retried, shortened, split or promoted. Case is presently
UNRUN because original preparation failed; full-family parity is NOT_RUN.
Raw private namespaceworktree2026-10-10-batch-4-mixed-dispatch/preparation and
2026-10-10-batch-4-mixed-prepared/v016-mixed-exhaustive-500mb-30000-k100-v1.
No speed fix, workload/limit change or productfeature is selected.

| `882f50cb8` (worker) | Preserve first boundary FAIL plus six original NOT_RUN rows | 79366 | 79366 | +0 |
| `a38242d7b` (worker) | Preserve first dedup FAIL plus25 original NOT_RUN rows | 79366 | 79366 | +0 |
| `57648f448` | Integrate both failed run CSV pairs, H11 diagnosis and mixed failed preparation | 79366 | 79366 | +0 |


Section9's passing done-when list cannot be fulfilled for the retained original
500MB exhaustive preparation: no sealed input exists after its one120s attempt.
This own-purpose spec correction records explicit failed closure, notPASS:
preparationFAIL/caseNOT_RUN/parityFAIL, everyoriginal row retained in the two
CSVs; othercases require their own unchanged parityPASS. No nativecheckpoint,
workload, limit, cache, samplecount, verifier or productfeature changes. Family
aggregate parity/batch closure stayfailed for this case, and that truthful
closure permits the nextbatch after all remaining applicablecase work. Source
and bounded receipts show aggregateoriginal setup cost rather than an isolated
state error or identified product correctness bug. No preparation replay is
selected. Root owns guarded report/CLI visibility; worker will preserve original
per-case parity assertions and make failed selections visible without promotion.

| `1ba406e30` | Specify truthful failed closure for the original unprepared exhaustive case | 79366 | 79366 | +0 |


H12 preserves complete failed parity evidence without weakening any original
check. Worker47e5a5500 keeps membership, peeledreference/tagobject, reviewed
sourceSHA, preparedcase/family, nativeverification and fixtureseal assertions
verbatim inside per-case handling. Every original selection yields ordered
PASS or FAIL(reason,error_type), overallFAIL ifanycasefails. It writes new
parity.json/manifest once, then raises ParityFailure(ValueError) carrying exact
.result/.output and first original reason. Existing ValueError rejection tests
remain unchanged, all-PASS return unchanged; global catalogue/output/publication
exceptions stillraise without inventing case verdicts. No missingcase omission
or aggregateFAIL promotion. Worker external missing-middle regressionFAIL on
parent; final4new+3unchangedreview+15fixture checks22PASS0.485s/637,248,167ns
supervisor<=2/120. A first candidate test-fixture macOSpathalias failure remains
raw; resolving expectedpath didnot changeimplementation/guards. Raw private
2026-10-10-batch-4-parity-failure-visibility-red, -green and -fixed.

Root CLI catches only that typedfailed-parity exception, prints retained
family/statusFAIL/allcasecount/samples0/evidence, returns1; all-PASS returns0.
Red driver isolatedexception propagated beforefix, retained under
benchmark-results/fs-bench-pro/2026-10-10-parity-failure-cli-red. Final5driver
checksPASS0.020s/120mainlock at integratedsource, raw -fixed. Previouslyqualified
workerchecks sourcehashmatched and reused, not replayed. No real parity,
nativepreparation/actualcase/retry/product/source-speed change occurs here.
Mixed will retain allten rows, including finalpreparationFAIL/actualNOT_RUN;
only individual unchangedparityPASS cases can be dispatched after H11 closes.

| `47e5a5500` (worker) | Retain every failed parity outcome with original guard rejection API | 79366 | 79366 | +0 |
| `ea7148a19` | Integrate complete failed parity evidence and CLI nonzero summary | 79366 | 79366 | +0 |


H11 corrected source integrates Cappedf2c143b37's prepare_closed_store and
strict optionalnormalized census custody. Root controller preserves exact
KnownStop raw export, creates freshcensus-store/store.sqlite through thehelper,
then an independent writabletyped-store bytecopy. Allnormalization work is
charged to originalproof_work_stop/proof_stop; no timer/limit/cache/workload
changes. Both SQLcensus and publicprovider use the same initialstandalonebytes,
while provider's offlineWritable changes cannot alter censussource or rawexport.
OriginalDocker cp/mainfileSHA/size/manifest/transcript/runtimeStop/volume bindings
remain raw, additionally bound to normalization.json/hash/fullrawfileSHA-set,
helperSHA, exactoutputSHA/size/profile/singlebackup/closedconnections/deadline.
Legacy nonnormalized closedreceipts and wrapperbehavior remainunchanged.

Helper copies the full exact regular main/WAL/SHM set in64KiB windows, never
opensrawexport as SQLite, opens isolatedsource mode=ro/query_only/OFF/WAL,
backs up in128-page steps with zero sleep and immediateBUSY/LOCKED/error abort,
target explicitlyWAL/OFF. It checkpoints onlyits own target, closes bothowned
connections and requires sidecar-free output with WALheaderbytes18:20=[2,2].
Everyrawfile remainsSHA/sizeunchanged. Missing/extraneous/journal/symlink inputs,
sidecars on immutablecensusinput, invalidprofile/deadline/attempts, raw/receipt/
output tamper and rehashed forgedcontract remainrefused. No fsync, busyretry,
manualWAL/header editing, newdependency or productoperation is added.

Worker9externalSQLite-WAL/normalization/strictsyntheticcensus checks PASS0.307s
underprivate120s; legacy8census guardsPASS0.265s and finalnormalized-only guards
leavelegacybranch unchanged. Worker pre-helper regressionERROR(absentAPI) is
retained after demonstratingoldmain1/WAL3rows; Root's actualcontrollerred is
stronger: oldmaincopy0/100. SyntheticStoreFixture IDs/provider/Dockerevents are
explicitly labelledsynthetic; no fabricatedcanonical qualification. Actual
publiccanonicalC3 regression and frozenlegacyoracle qualifications are unaffected
and reused. FinaloracleSHA88393ba40bce9dd08da9e7afa8e8046588c2904fc6a1071faadc2ceb589cc4b4,
newtestSHA c68991799be4c3da8b82ca564d5cedbae9f6fb2eafbc4080191b2cb7846c76a7
matched importedsource exactly. Root final8controllerchecksPASS0.139s/120mainlock,
including realclosedWAL100rows arrivingin typedproof/censuscopy withrawbytes
unchanged; raw2026-10-10-h11-closed-wal-controller-fixed. No tests weakened.

Tiny retainedsample diagnosis independently foundoriginalraw-awarecopyexactly
twoacknowledgedfivefieldCommit tuples/finalhead, versusmain-onlyzero/headNULL.
Original180,224Bmain/181,312BWAL/32,768BSHM hashes stayedunchanged. Its first
analysisSQLitebackupDELETEheader was not a productStoreprofile qualification;
oneapprovedpublicdiagnostic promptlyFAIL44,835,792ns because existingbackend
profile.rs44–49 rejects journal!=wal, beforeprovider/roots. That failure remains
raw, no unchangedsecondattempt, no productbug/receipt promotion. Currenthelper
instead preservesrequiredWALheader. An exactcurrenthelper/newartifact public+
censusdiagnostic maynowqualify recovery before changed-source cases; original
sampleFAIL/custody/counts/CSVs remainimmutable. No mountedworkload/Commit replay
is partof that diagnosis. Producttree/79,366 unchanged; allninecompiledassets
stillmatch finalseal, so no redundantbuild is required. Batch4 remainsinprogress.

| `f2c143b37` (worker) | Normalize complete closed WAL snapshots with strict unchanged raw/census custody | 79366 | 79366 | +0 |
| `2b52bdf17` | Wire H11 complete WAL recovery before independent public/census proof | 79366 | 79366 | +0 |


H11 actual KnownStop sample recovery qualifies at Tiny private
 e2461e160e529c03dc31f0346f6851f565389132, sourcebyte-exact reviewedmain2b52,
countedtreebc7bd355c08a99cd2ba3f6e01d009d3f21514814/+0. Finalninebuildinput/
actualbinary hashes allPASS, runtime4f1a7d...unchanged, no rebuild. Exactlyone
newartifact from actual completefailed rawStore: currenthelperproducesWALformat
217,088B/SHA3d4b147a6b3dac1fa979bd02639b5f846fdb4e16651838c6569db2bf6e077f1d.
Original publicverify-history argv/case/manifest/UID501/GID20/completeacknowledged
transcript remain, onlysealedinput and newreceipt differ. Independenttypedcopy
PASS48,923,875ns, allthree selectedroots0/1/2 andtwoacknowledgements, real
publicschema/catalog/binding/requiredDisposableWALOFF profile qualification.
Actualunfiltered immutablecensusPASS2, exactfivefieldtuples/head/firstparents/
stack/base/catalog/manifest/actualDockerStop/export/runtime/normalizationcustody.
Originalmain/WAL/SHM SHA/sizebeforeafter unchanged. Whole newhelper/public/census
within25s absolutestop, privateouter<=2/120. Thisis retainedcause diagnosis,
no workload/Commit/mount replay, no originalFAIL promotion orfailedresource
removal. Rootverified diagnosticSHA
5c373ffb08f0d1fb92b94c39460cb2d39aaf07e4db5562d24e10e15bf3201ac3;
normalizationreceiptSHAa1215a3fefb73832171ff667322eec6ecba87570a1f084dff12efa262d919534.
Raw Tiny originalcampaign/diagnosis-h11-e2461e160/diagnostic.json. Capped9-test
qualification evidence is explicitly retainedconsoletranscription, not arerun,
originalcampaign/h11-normalization-qualification.jsonSHA
d5f02ab0e804ad40de80484b3d09718972c1740a36cdf5dc399b311ac6b52dfc.

Sourcequalification/coldpolicy/originalcase declarations remainfixed; changed
Pythononly source affects proof machinery, not product/compiledassets. Root
GO dispatches originalsevenboundary cases at e246,26dedup cases at clean
7905e26a2a01af93d9dfacdd9b5256dc4f8fc43e/treea195e225520d43cccdc19e08c3334a63aa7e0a67,
ninemixed preparedcases atclean5faaa263c275f2a747adefab232e51d65be68f4b/
tree7b9fe57b9b7d03d3f51a80f73d7a1bb31255d45c. Eachprivatesync79366delta0,
ownedfamily/workloadhashes unchanged and allninecompiledsource/binarysealsPASS.
Closed originalpreparedinputs reused, nevermutatedsamples orfailedpreparation.
Onlyfirstboundarysmall/distributed1 are affected-source retests; all other
actualcases receivefirstattempts. No unchangedcase repetition orbestof.

Corrected dedupparity PASS26/zerosamples7,615,398,416ns/<=9/120. Mixed whole
parity exactlyonceFAIL34,618,928,375ns/<=35/120,10orderedrows: exactlyninePASS
andlast500MBmissingprepared.json FileNotFoundErrorFAIL. AggregateFAIL unchanged;
no failedselection omitted/promoted. Tiny reuses exactqualifiedsevenunchanged
per-case guards/preparationparity, no redundantnewparity replay. Newrunroots:
Tiny newrunpath willbe recordedfrom itsfinalhandoff; Capped
2026-10-10-batch-4-dedup-7905e26a2, Namespace
2026-10-10-batch-4-mixed-cases-h11-5faaa263c. Firstchangedboundarysmall completes
functional/native/public/globalK2PASS/Gone/KnownStop, numericINELIGIBLE;
223,655,332ns wall,1,802,061,417ns proof,14requests/192jobs/857statements,
no limitmiss; passing administrativepair removal follows knownouterACK.
Remainingcases underway; no family/batchcompletion claimedhere. Original
failedfirstpairs and CSVs remain unchanged. LastmixedpreparationFAIL/actual
NOT_RUN receives explicit administrativeclosure withzero actualattempts and
no fabricatedbenchmark/proofmetrics. Laterbatches remainpending.

| `e2461e160` (worker) | Sync current H11/failed-parity source and qualify actual retained WAL recovery | 79366 | 79366 | +0 |
| `7905e26a2` (worker) | Sync current shared source before corrected dedup one-attempt cases | 79366 | 79366 | +0 |
| `5faaa263c` (worker) | Sync current shared source before mixed failed-aggregate parity and ninecases | 79366 | 79366 | +0 |
| `ba000f704` | Record genuine H11 recovery and dispatch qualified changed-source cases | 79366 | 79366 | +0 |


Batch4 file_size_transition family closes allsevenoriginalcases at reviewed
privatee2461e160, resultse1081cb4b3f06f85f1530d7f33de1fb9f0b9d1e6 /
treef2723a686189653485be1eaf578b91a391c81aad/+0. Bothsevenrow CSVs at
baselines/core/2026-10-10-e2461e160 are importedunchanged. Root verified exact
rawhandoffSHAfb2eeaea43061ebe7d1f27814f3017cc6218828b1c66cf192c4a7800a68d0854
and row/cardinality/status alignment. Raw privateTiny
2026-10-10-batch-4-file-size-transition-e2461e160/FAMILY-HANDOFF.json includes
allsource/build/prepared/checkpoint/census/cache/metrics/limits/removal hashes.
Sixcases strictnative/publicselectedroot/inode/globalcensusPASS, Gone/KnownStop,
passingpairs removedafterguardedpub+outerACK; numericall6INELIGIBLE(H5),
no cachequalificationor speedclaim. Alloriginalfive overwrite K2 checkpoints
and SDKroundtrip K4 checkpoints/censuses qualified. No limit exceeded.

| Original boundary case | Complete ns | Proof ns | Requests/jobs/statements | Store allocated B | Overlay allocated B | Outcome |
| --- | ---: | ---: | --- | ---: | ---: | --- |
| `v016-boundary-small-control-v1` | 223655332 | 1802061417 | 14/192/857 | 397312 | 268668928 | functional/proofPASS; numericINELIGIBLE; Gone/KnownStop |
| `v016-boundary-below-v1` | 237781501 | 1738565250 | 14/194/865 | 790528 | 268664832 | functional/proofPASS; numericINELIGIBLE; Gone/KnownStop |
| `v016-boundary-exact-v1` | 221404584 | 1754203709 | 14/272/1357 | 532480 | 268664832 | functional/proofPASS; numericINELIGIBLE; Gone/KnownStop |
| `v016-boundary-above-v1` | 195659750 | 1710119667 | 14/272/1357 | 532480 | 268664832 | functional/proofPASS; numericINELIGIBLE; Gone/KnownStop |
| `v016-boundary-large-control-v1` | 233357666 | 1804117333 | 14/272/1357 | 1466368 | 268668928 | functional/proofPASS; numericINELIGIBLE; Gone/KnownStop |
| `v016-boundary-roundtrip-v1` | 337660999 | 1724911667 | 26/407/1900 | 729088 | 268668928 | functional/proofPASS; numericINELIGIBLE; Gone/KnownStop |
| `v016-boundary-alias-roundtrip-v1` | 376628789 | 30703500 | UNAVAILABLE/UNAVAILABLE/UNAVAILABLE | UNAVAILABLE | UNAVAILABLE | strictnativeFAIL; verifiermount/StopNOT_ESTABLISHED |

Alias executedallfive originalcommands withfiveCommitted acknowledgements and
measuredEnd/Gone, thenstrictnativefinalFAIL: data mode750/mtime
1791643360.694999760 differs fromexpectedmode750/1700000000.0. Frozenatomic
replacementhelper updatesparentmtime; its own nativeauthoringproof already
refuses that laststate. No parentrestore, weakenedmetadataoracle orproduct
change is authorized. Persistedproof/globalcensus/finalcounts/storageNOT_RUN
orUNAVAILABLE, neverzero/lifetime substitute. Failedverificationmount
 df6da775e176678746558265196886a4e1853ae9c8874365eab37d255fbc0d49 remains
retained; KnownStop notestablished, hostfencePermissionError/observedhostexit1
retained. Failedcontainer
fc826216619af5bc2907d540e8df3301835282b22c719708827d80b4ad375910 and
volume layerfs-fsb-sample-6edfcf62abf74b08 stayretained. OnlymeasuredmountGone
is established; completecase cleanclose is notclaimed. This is retained
originaldefinition/metadata compatibilityFAIL, no identifiedfixableproductbug.
Siximmutablepreparedmasters andoldH11failedsmallpair remainretained.
LaterH13stagingfix doesnotaffectthese completedreceipts; nonearerun.

Dedup at7905 completeddistributed1/10/100 once: allfunctional/proofPASS,
numericINELIGIBLE/Gone/KnownStop andpassingpairsremoved. Nextdistributed500
failssetup stage-code-check exit255 with165,872BsingleBash-c argument, before
anymount/productoperation (sample_count0/attempted_operation_count0). Every
preparedhelperSHA/path is expandedinto that argv byfixtures.stage. Raw
Capped2026-10-10-batch-4-dedup-7905e26a2/run/dedup-history-distributed-500/
staging/stage-code-check.attempt.json andstderr retained. Failedcontainer
2af4f1956a1853e695b5c8b916a849355be16de2112944c5e9c894a816e65ac4 and
volume layerfs-fsb-sample-c60f0a7c016f438a retained; cleanupUNAVAILABLE/
Stopnotestablished/mountkeys[]. Rootstopped furtherunattemptedcases, preserving
currentoriginalsource andallpreps. Workerowns H13 smallestfile-backed full
manifest correction: authenticate exactremotechecksumfileSHA, thenunchanged
sha256sum fullfilechecker; no omittedentries/truncation/tamper/retry/largeargv,
newdependency/product/workload/limit change. Earliercompletedcases unaffected.

Mixed at5faaa completedfouroriginalepisodes once(allstrictfunctional/proofPASS,
numericINELIGIBLE/Gone/KnownStop withpassingpairsremovedafterpubACK):
product/proof ns 1=193,719,124/2,480,153,458;
10=247,202,416/2,808,790,250;
100=1,048,731,959/18,072,620,625;
500=4,548,366,793/17,816,215,666.
Counts95/461/2367,833/3349/18118,8213/37158/190372,41013/186571/952225.
Externalwalls6.569/7.322/27.508/33.558s<120; noadditionalactualcases underway
whenSTOP honored. Preserve thissource/run withallten CSVselectionrows;
remainingfivepreparedM1 cases willusefreshcorrectedsource/run, earlierfour
neverrelabelled/rerun/copiedinto newrun. Finalfamily ledgerselects unaffected
oldfour+newfive+failedprep byexactsource andbothper-runCSVpairs.

Root inspectedlastmixed administrativeunrun-preparationclosure atold5faaa,
receiptSHA38a46e650bae7cc5f74540dc2b5b45cd817c75c4f03a44af2ecbccec085c35a4,
manifestSHA5159e3bafdbcffac69f16d278ca2af2e895e0db751f65d699e084985a49ce323;
allfour referencedoriginalfailedprep rawfilesSHA match. Correctschema/current
source-product-image/original4c83preparedsource, statusFAIL butactualcase/
functional/proofNOT_RUN, zero sample/verification/actualattempt counts;
allbenchmarktiming/count/cache/cleanup/storage fieldsabsent. Preparationwall
120,033,743,083ns is explicitlysetup only, notbenchmark/proofmeasurement.
ReportpreservesperformanceN/A/originaldeclared300s verifier plusactualNOT_RUN
reason; no finalcase bench.run/claim/retry invoked. Laterclosure mayrefer the
samefailedsetup atnewsource withoutinventing anotherattempt orsample.

| `e1081cb4b` (worker) | Record allsevenboundary outcomes, completeproofs and retainedstrictfailure | 79366 | 79366 | +0 |
| `88a2bd27b` | Integrate completed boundary CSV pair and preserve H13/partial mixed custody | 79366 | 79366 | +0 |


The first corrected dedup and mixed source runs are committed without changing
any original receipt. Dedup worker672f3320623762b735f61bae3de483f2136657fd,
tree5109ff7cdff6f62e66ca79d1e21690b39b899a51, adds only the CSV pair under
baselines/core/2026-10-10-7905e26a2. Both files have all26 original selections:
three functional/proof PASS with numeric INELIGIBLE, distributed500 setup FAIL
with proof NOT_RUN/sample_count0, and22 NOT_RUN. Root checked cardinality and
ordered case alignment. This does not claim that all26 cases ran or passed.
The original7e594 H11 failure remains separately committed.

Mixed worker36cd1ee26387787834c83246dd8a4df000e68585,
tree6680657b71c02ecbf4c979cd7ba46d53089baa8f, adds only the CSV pair under
baselines/core/2026-10-10-5faaa263c. Both files preserve allten original cases:
four ordinary episode proof PASS/numeric INELIGIBLE; five prepared M1 cases
NOT_RUN; final unprepared exhaustive case proof NOT_RUN with explicit failed
preparation reason, zero actual attempts and performance N/A. Both exhaustive
selections have performance N/A, with original declared proof limits retained.
Root verified exact performance SHA
6cd83a5ba8ed950a494772a61fff9298081dfbd66cb357a5ec9afb127f54d3c1
and verification SHA
045c9f1b6192ef5b44c27ccc1e76a3afb2d2a3688ba3c7372872bf1ebfb86170,
ordered ten-row membership and four PASS/six NOT_RUN proof outcomes.
Worker report376,161,416ns is reporting only, with no replay. The complete raw
handoff is private namespaceworktree
benchmark-results/fs-bench-pro/2026-10-10-batch-4-mixed-h11-sync/first-source-handoff/handoff.json,
SHAf3d2c04c9e3915630eccc576733dc497e70b08b55b2272e283dcddb509e13c62;
its manifest SHA is71cb9407b897a54ab214556235c7a68c51a06b8ceb28c26f4e4edcd6eb412337.

| Batch4 family | Original cases | Current recorded position | Remaining work |
| --- | ---: | --- | --- |
| file_size_transition | 7 | Six strict proof and clean-close PASS, numeric INELIGIBLE; one original strict metadata failure with verifier mount/Stop unestablished; allseven attempted | Closed failed case retained; reuse unaffected evidence after H13 |
| dedup_branch_history | 26 | Three corrected-source strict passes; one setup argument-size FAIL;22 unrun; older H11 failure retained | Qualify H13, then original23 affected/unattempted cases once at new source |
| mixed_load_bearing | 10 | Four strict passes at5faaa; five prepared cases unrun; final preparation FAIL/actual NOT_RUN; aggregate parity FAIL withnine individual PASS | Qualify H13, then originalfive prepared cases once at new source; retain failed preparation |

The new source runs will again select every original case, preserving NOT_RUN
rows for earlier unaffected cases and citing their earlier authenticated runs
in the final family report. Raw receipts remain at their actual source; no
receipt is copied into a different source registration. The two CSVs per run
keep this distinction explicit. Root will assemble family completion from the
unaffected earlier results and genuinely affected/new attempts, with every
failed and unrun selection visible. No new result ratio, comparison verdict,
cache claim, threshold, workload or limit is introduced.

| `672f33206` (worker) | Preserve three dedup passes, failed500 setup and22 unrun rows | 79366 | 79366 | +0 |
| `36cd1ee26` (worker) | Preserve four mixed passes and every remaining selection with failed preparation reason | 79366 | 79366 | +0 |
| `bd3016957` | Integrate both partial source CSV pairs and preserve evidence across the staging fix | 79366 | 79366 | +0 |


H14 corrects the offline snapshot helper's initial profile selection. Source
review found that target journal_mode=WAL ran before synchronous=OFF. A real
sqlite3.Connection subclass regression queried synchronous immediately before
that journal switch and observed FULL(2), failing against the integrated H11
helper in0.030s/120s main lock. That is a concrete initial-profile ordering
error, distinct from the already-fixed missing WAL. It does not establish a
lost Commit or alter earlier byte/history/census outcomes. Earlier backup and
data writes did verify WAL/OFF, but initial header/journal setup did not have
OFF selected; actual earlier sync syscall counts were not recorded. Preserve
all original receipts and that limitation, without promoting their initial
profile claim or repeating unaffected completed cases.

The helper now selects and verifies synchronous=OFF immediately after target
connect, before page-size/journal initialization, and retains the existing
post-WAL and post-backup profile checks. No new profile, limit, cache state,
workload, retry, product feature or dependency is added. Final owning ten
normalization tests PASS0.352s and eight controller tests PASS0.159s, each once
at this changed source under explicit120s main locks. The retained red and
final stdout/stderr are under benchmark-results/fs-bench-pro/
2026-10-10-h14-normalization-initial-profile-red and -fixed. Product source,
compiled binaries and production total remain unchanged by this correction.

Read-only product inspection found the analogous create path in persistence
backend/sqlite/profile.rs: journal_mode is selected before synchronous in its
later configuration loop; Session::connect has no earlier OFF setting. Capped
owns an external public Disposable Store create/seal regression observing real
SQLite VFS xSync calls, with no third-party edits or new dependencies. No product
fix or defect verdict is claimed until that evidence. The product test builds
with --no-run first and has an explicit120s private stop; Durable remains
NOT_RUN. All new cases remain stopped. H13's separate qualified file-manifest
commit359b63686 is ready for integration; its proof and prior failed stage are
retained. After the product investigation closes or fixes the defect with the
required covering checks, dispatch will use exact reviewed final sources.

| `e09c61bbd` | Select OFF before offline snapshot journal initialization and retain initial-profile limitation | 79366 | 79366 | +0 |


H13 integrates worker359b63686083028daae2131eeff71105bff5e552,
tree8f2ec6f5af4ca3a3768e09cede2c66675bcfe79c, without changing any original
prepared helper or workload. fixtures.stage retains its interface. It writes
the exact ordered digest+two-spaces+/code/path rows to a fresh local checksum
file and stages that file at a reserved control path outside the measured
Workspace. Its complete expected SHA is authenticated remotely first; only
then does the original GNU sha256sum complete-file checker execute under
bash -euo pipefail. A truncated or substituted manifest cannot omit a helper.
The remote checker argument is191B, independent of helper count; the actual
full manifest is160,676B. Each command retains its original15s child stop and
one attempt. This fixes setup transport, not product speed or a case limit.

The external regression rejects the old expanded500-step argument. Final
pinned-image qualification checks1,501 OK rows: the authenticated manifest and
all1,500 helper entries, including history/steps/step-500.sh. Three distinct
negative checks each exit1/one attempt/15s: omitted final manifest row, changed
remote manifest, and actual history/payloads/step-500/member-1.bin tamper with
its exact FAILED line. Final affected pinned check PASS1/3.865s under<=5/120;
malformed path checks PASS0.007s, prior unchanged shape/order unit guards
remain qualified. Unsafe/noncanonical/absolute/traversal/newline/NUL paths,
empty/dot paths, invalid digests and the reserved manifest path or descendants
are refused before Docker. No checksum, payload, verifier or original check
is weakened. No new dependency, module, retry, workload or cache change.

Tiny independently reviewed final source and read the actual pinned receipts,
without replaying any test. Review found the initial dot/reserved-descendant
admission gaps and a script-only tamper fixture; those were corrected with
negative tests and a true prepared member-1 payload check. Root imported only
fixtures.py and test_staging_manifest.py and verified exact qualified SHA
099e9bc251359300146e545bba978a959e054319fcc8242e1e7acc1da93ae8e7
and2031b4dda7f9ef206bbe8c98e0d6b11adb56ae23fed0007aa895fc1dc3f2fee0.
Owning scoped evidence is reused at identical bytes, not repeated. Raw private
Capped benchmark-results/fs-bench-pro/2026-10-10-h13-staging-final-qualification
retains every copy/check/tamper completion and removal receipt. The owned
passing test container was stopped/removed; no volume, product Store, mount or
benchmark sample was created. The original distributed500 setup failure and
all earlier cases/CSV rows remain unchanged.

H13 is qualified, but new samples remain stopped for the separate product
startup xSync investigation. Public Disposable Store lifecycle observation
must establish the actual behavior; final-profile assertions alone cannot do
that. The existing first-party VFS observer patterns and rusqlite::ffi ABI
permit same-SQLite-instance host/Linux coverage without changing third-party
code. No product defect or fsync count is inferred merely from the configuration
order. Durable execution stays disabled. After that investigation and any
necessary exact-source checks, remaining case dispatch will include H13 and
H14; earlier unaffected case records will not be relabelled or replayed.

| `359b63686` (worker) | Authenticate complete file-backed helper checks without oversized argv | 79366 | 79366 | +0 |
| `8ae040cdd` | Integrate H13 with complete pinned checksum and tamper qualification | 79366 | 79366 | +0 |


H16 repairs two missed Stage0.2 imports discovered by the product check's
aggregate formatting failure. The daemon e2_startup example and its external
receipt tests still named the retired benchmark/fs-bench-pro-storage-content
location. Both now use the exact existing relocated
benchmark/cluster-one/storage-content/src/workload/digest.rs. Only the two path
strings change; the digest implementation and every test/check remain intact.
This changes no product source or closed R7/R8/R9 record, registration or receipt.

Owning host example/test build uses +1.85.1, --locked/--offline, repository
ARM64 flags and main-private r7-host-release target under120s main lock;
--no-run PASS16.27s/<=17/120. Existingseven e2_startup receipt/digest tests
PASS0.10s/<=3/120, with no new test or benchmark attempt. Their diagnostic
explicitly reports global persistence NOT_IN_SCOPE; its local overlay/profile
and ordinary refusal/Stop assertions are unchanged. Aggregate cargo fmt --all
--check nowPASS/<=4/120 at this path-corrected source. Raw
benchmark-results/fs-bench-pro/2026-10-10-stage-0-relocated-digest-import keeps
all no-run/test/fmt stdout/stderr. The earlier failed fmt remains in Capped's
product qualification, without promotion. Exact product integration will also
run its required final-source formatting after importing the changed files.

The separately qualified product startup correction a9d8b0453 is ready; it
has not yet been imported by this path-only commit. Actual host red observed
24 delegated xSync calls during six Disposable creates, and final host/Linux
checks observed zero Store syncs. Root is preserving that evidence and will
integrate the minimal selected-policy ordering change as its own product
commit with the exact +4 productionLOC and completed final-source checks.
No new sample, speed result or Durable execution is selected by H16.

| `9dfb70ee4` | Correct the two relocated digest imports without changing their content or proofs | 79366 | 79366 | +0 |


H15 is the first identified product defect in this rollout. The external public
Disposable Store regression delegates the actual SQLite VFS and observes all
four file classes (main/WAL/journal/other), with opens/writes/close/error coverage.
A separate non-Store control forwards one real xSync and resets counters before
each Store phase; it never syncs Disposable backing itself. Before the fix,
three layouts without acquisition each issued1 main+1 journal sync, and three
with acquisition tables each issued2 main+4 journal sync:24 total. Every sync
used SQLITE_SYNC_NORMAL flags2; no VFS errors or unclosed files. Save, seal,
writable reopen and reopen-seal issued0. The single host red exited101 in
0.04s/776,134,167ns supervisor under<=1/120; every configuration was recorded.
The failed fixture remains at
/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-disposable-startup-sync-41956.
Initial test compilation found only a macOS ffi alias mismatch; the external
signature was corrected to equivalent *const c_char before final no-run and
this single red. No third-party source or product test hook was changed.

Source cause: profile::apply selected page/acquisition/WAL initialization before
its later synchronous/fullfsync loop. The correction moves those same two
selected calls to the beginning, preserving every PRAGMA, count and profile
validation, WAL-only existing-Store guard, public API, format and one-attempt
behavior. It adds no retry, dependency, feature, benchmark branch, new profile,
threshold, index or construction producer. This is enforcement of the existing
Disposable requirement, with no speed result or optimization claim. The shared
Store architecture guide describes the implemented ordering and external proof.
Root read the optimization guides for the applicable path and retained the
owner's no-optimization scope; no query-plan/performance claim is made.

Worker a9d8b0453295d769c92c3eed7b57317e63ffefbb contains only profile.rs,
external tests/disposable_startup_sync.rs and the architecture guide. Exact
private counter79366->79370(delta+4), countedtree
5de64ace7306a830715e4a929391c8b6ab011b14. Root verified source SHA
941e502affb580b71d833dc46b793eecf1717fe47b0756bf955a2099709418c7,
externaltestSHAcaf639cc693a33a7e44fe27d872b0ce11936cf91c909ac1c27c2c6156841debc,
and guideSHA47c4be76bec2cabfd329b73007202b2713bf45f8fc09085a1e89e34a34421b35.
The rest of the qualified persistence/dependency source is unchanged; only
Root's separately tested nonproduct digest-import paths differ elsewhere.

At final product bytes: host14 covering tests PASS4,430,852,208ns, pinnedLinux6
applicable tests PASS228,708,666ns. The portable VFS test exercises allsix
layout/acquisition configurations on both platforms, now0 Store xSync in every
create/Save/seal/reopen phase, with positive control1 and no hook/close gap.
The two macOS-only targets execute0 Linux tests, not claimed as coverage there.
Actual SQLite versions3.51.0 host/3.53.2 Linux are witnessed by those binaries.
Each test used no-run first and explicit<=120s private lock, one final-source
invocation. Host/Linux persistence all-targets Clippy-Dwarnings PASS
4,220,645,583ns/5,379,079,917ns; boundaryPASS674 files494,431,750ns;
all50 tooling testsPASS0.149s/314,040,042ns supervisor. These exact source-matched
covering/guard results are reused, not repeated. Scoped persistence fmtPASS;
old aggregatefmtFAIL from H16 remains raw. Root's exact integrated final-source
aggregate cargo fmt --all --check nowPASS/<=5/120 after H16 correction, raw
benchmark-results/fs-bench-pro/2026-10-10-product-disposable-startup-integration.

Pinned image remains sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6.
All1,328 host source/manifests hashes were verified inside before Linux build
and after tests/Clippy; manifestSHA
41c3e37ff20464e3b5286711de2d456f0debad2e55aa1fffaebedb9761d307bb.
The first offline no-run failed before compilation because the image-default
Cargo cache lacked libproc index metadata; that failure is retained. An existing
private Linux Cargo cache was copied to the owned target volume, with no new
dependency/network substitution. Changed preparation then no-runPASS
45,708,458,250ns and all tests/ClippyPASS. Owned Linux container
3895cb42a80d62e6a1d6bd25a847ea83798f29625cc832a29d5c0f408b986fce
(name layerfs-product-sync-h15-capped) and volume
layerfs-product-sync-h15-capped-target remain stopped/retained conservatively
because of the initial failed preparation; no other container/volume was touched.
Host failed fixture and all raw receipts remain. Full qualification is private
Capped benchmark-results/fs-bench-pro/2026-10-10-product-disposable-startup-sync-green/qualification.json,
SHA43f600aa6214622dc61a863df3352ef2e10604244be56de2ec38e86a2a7dafc7,
verified by Root; the sibling red/source/binary receipts remain immutable.

Earlier receipts are history: their final WAL/OFF checks did not observe these
initial create syncs. No old result is promoted or overwritten. The profile's
format and saved roots do not change, so closed prepared bytes remain compatible
setup inputs. Existing-Store Workspace phases had no create path and their
unaffected functional receipts remain at original sources. The measured Init
namespace checkpoint creates Stores and will receive exactly one changed-source
attempt per originalcase after a fresh current nine-asset build. No timing
comparison or Durable reauthorization follows. Durable is
NOT_RUN — disabled by owner until explicit reauthorization. Fresh sealed build,
affected Init checkpoint and remaining batch4 cases are next; later batches
remain pending. No speed, release gate, VM/kernel crash or full unrelated suite
qualification is claimed.

| `a9d8b0453` (worker) | Enforce selected Disposable synchronization before SQLite creation writes | 79366 | 79370 | +4 |
| `a05ed6027` | Integrate H15 public VFS regression, minimal product fix and exact required checks | 79366 | 79370 | +4 |


The fresh full nine-asset build at clean main `a05ed6027` is admitted for the
remaining batch 4 cases and the four affected Init checkpoints. Raw receipts
are in `benchmark-results/fs-bench-pro/2026-10-10-batch-4-product-fix-nine-asset-build-a05ed6027/`.
The build seal SHA-256 is
`fd51062c80f9c4d1520cf90c998511c9093ee1bc3cc6cf6a5fa503042e449886`;
product tree is `9ac88edb2cc44d44edd4a53f9ad89f4dee063a58`.
Root authenticated all nine archived binary hashes and sizes, source identity,
identical before/after source manifests, pinned image and both complete Linux
source-check sets before and after compilation. Each compiler invocation had
one attempt and an explicit 120 s stop. Host runtime build took 8,716,620,834 ns;
Linux daemon and helper build took 32,026,333,875 ns. Native writer, verifier and
SDK payload/helper binary hashes match the closed preparations exactly. Runtime
is now `7f6a4eec7313837219c59d11cca3926c9bf9a13c17bd879eb208dd67c73a8418`;
daemon is `aa9de24f0ba916c0ac483f8e559d9d6986dfe4296f4d26a829801a1cb7e4ffd4`.
The checker still reports exactly OWNER-3's two passthrough refusals as FAIL.
Its aggregate PASS is not claimed. The owned passing build container
`layerfs-fsb-build-19bb02c9229a44f9` was removed with its successful removal
receipt; no build volume or owner resource was removed.

Capped's read-only compatibility audit authenticated all 26 closed dedup
preparations, 192 selected checkpoint manifests and 6,905 helper files; all
five immutable initial Store copies retain their exact roots and hashes with
no sidecars. Mixed's read-only lookup authenticated all four original native
Init inputs (100, 1,000, 10,000 and 100,000 files), their original digests and
recipe sources. These reads establish compatible inputs, not coldness or a
replayed verifier. All preparations retain their original source identities
and initial-profile limitations. No input was regenerated or relabelled.

The existing Capped and Mixed worktrees now receive the exact reviewed main
source plus this sealed build. They own only the remaining 23 dedup and five
prepared M1 cases respectively, one attempt per new execution identity. The
previous three dedup and four mixed episode results stay at their original
sources and appear as NOT_RUN in new source selections, with evidence retained
separately under decision 47. Mixed's incomplete final 500 MB preparation stays
FAIL with actual execution NOT_RUN and zero attempts. Root owns the four
changed-source Init attempts, each creating a fresh Store from compatible
native inputs. All cache, operation, proof and watchdog limits remain intact.
Shared host/Docker VM interference from concurrent worktrees and retained
owner/failed services is declared and unquantified; numeric cold admission
remains INELIGIBLE. Later batches remain pending until batch 4 closure.

| `3c67e9249` | Admit the current nine-asset build and dispatch affected checkpoints without reusing measurements | 79370 | 79370 | +0 |


H15's affected Init checkpoint is complete at clean source
`3c67e9249d614c348afb273ec3d95769377a4dfa`, product tree
`9ac88edb2cc44d44edd4a53f9ad89f4dee063a58`, using the authenticated nine-asset
build above. Each of the four original Init cases ran once at this changed
product source, from its original closed native preparation into a fresh Store.
The original recipe, seed, fixture, native hashes, supported Init parallel
profile, one construction worker, limits and public SDK route are unchanged.
Original preparation/parity evidence is reused at exact unchanged recipe and
workload bytes; no preparation or native verifier was replayed. Original Init
receipts and CSVs at c1d7dd0e8 remain immutable, with their initial-profile
limitation recorded under H15. These new checkpoints make no comparison.

All four functional and independent public-read proofs PASS. Their imported
entry/file counts, reopened root/first Branch and original selected metadata,
lengths and prefix bytes pass. Consuming seal and separate-process verification
exit cleanly, with no WAL/SHM/journal sidecars. Custody is
KNOWN_HOST_EXIT_AND_SEAL; no mount, container or volume is created by this route.
Closed sample Store files stay in ignored raw evidence. Every actual invocation
has a 120 s external main lock stop; each public Init child has a 120 s stop and
its separate verifier a 9.5 s stop. All product command walls are within the
unchanged 15 s reported limit; all verifier walls are within 9.5 s. CLI exit 1
preserves each numeric INELIGIBLE status, with successful functional outcomes.

| Case | Functional / proof | Complete wall ns | Public Init/seal ns | Proof wall ns | Store logical = allocated B | Imported entries / files | Selected files / bytes |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| namespace-100-compact-v3 | PASS / PASS | 1907223250 | 58851125 | 24359541 | 5255168 | 102 / 100 | 6 / 148294 |
| namespace-1000-compact-v3 | PASS / PASS | 185673875 | 163763250 | 22955458 | 20619264 | 1011 / 1000 | 10 / 141704 |
| namespace-10000 | PASS / PASS | 2199006667 | 2137579375 | 42281708 | 305696768 | 10101 / 10000 | 10 / 161912 |
| namespace-100000 | PASS / PASS | 8080275083 | 7931753125 | 183348209 | 516333568 | 101001 / 100000 | 10 / 132793 |

All four numeric rows remain INELIGIBLE. Source data is warm from preparation
and admission hashing, and host kernel metadata coldness is unverified. Shared
host/Docker VM activity from parallel dedup/mixed worktrees and retained
owner/failed services is declared and unquantified. Host SQLite 3.51.0 is
witnessed. Request/owner-job/daemon-statement counts and overlay storage are
UNAVAILABLE because host Init launches no daemon/FUSE/overlay; imported counts
above come from the actual public Init receipt. The independent proof retains
its original selected-prefix omissions; full unselected payload/namespace
traversal, isolated timing and cold qualification are not claimed.

Raw receipts are under
`benchmark-results/fs-bench-pro/2026-10-10-init-product-fix-3c67e9249/`.
`bench.py verify` authenticated retained evidence and reports four PASS without
product replay. `bench.py report` wrote exactly four rows in each of
`baselines/core/2026-10-10-3c67e9249/performance.csv` and `verification.csv`, with
original release 0.1.5 reference clocks and topology differences preserved,
without ratios or comparison verdicts. Both administrative commands used an
explicit 120 s main lock stop and ran once. No new defect or failed resource
surfaced; unaffected earlier families are not repeated. This result/document
commit changes no product or harness source and reuses qualified scoped checks.
Disposable/WAL/OFF remains explicit and Durable stays
NOT_RUN — disabled by owner until explicit reauthorization. Remaining batch 4
case execution continues before any later batch.

| `8f24e39a4` | Record four changed-product Init checkpoints and both complete CSVs | 79370 | 79370 | +0 |


The first changed-source dedup distributed500 attempt at private clean
`e308118a9a01b6fb79cb82db5fbf2c8d5ef7da56` passed the full H13 authenticated
helper staging check, then reached the unchanged 120 s outer watchdog. Its
external driver exited 124 after 120,023,452,708 ns. The retained runtime stream
contains 377 successful Committed acknowledgements, followed by the parent
termination's stdin/BrokenPipe failure. The original finite 500-step schedule
was still progressing; no wrong result, refused operation or source-backed
deadlock was identified. This attempt is FAILED under the owner's test ceiling,
with no retry, speed change or expanded limit. The registered product soft
limit remains 15 s and verifier limit 45 s. A complete product wall, complete
request/owner-job/statement counts, final storage, independent proof, End/Gone,
Stop and publication/outer acknowledgement are unavailable.

Raw evidence is Capped's
`benchmark-results/fs-bench-pro/2026-10-10-batch-4-dedup-e308118a9/run/dedup-history-distributed-500/`.
The separate administrative closure has SHA-256
`8739171b767d18018ee99d27ab558fac0304a92f9ceda851ca233b20296c5b13`.
Root authenticated every bound selection, invocation, runtime and clone file,
the exact 377 acknowledgement rows and last ID/root. It records sample/actual
operation attempt 1, completed 0, with external wall and partial progress only
as diagnostics. No product receipt is fabricated. Existing reporting retains
FAIL/proof NOT_RUN and unavailable complete metrics; it also withholds partial
receipt limit fields, so their known registered declarations are given above.
Container `046adc09eb527af87ad8eb781c6004089656ebcfd941ed6eb185a6c901d24625`
and sample volume `layerfs-fsb-sample-23b0cb54e27f4674` remain retained with
uncertain custody. Remaining 22 distinct dedup cases may each proceed once;
old three completed cases and this attempted distributed500 are not repeated.

The first changed-source M1 case,
`v016-mixed-development-100mb-5000-k10-v1`, ran at private clean
`fa70bbb01c413a0bbbadd5c10d81652580b046f8`. All ten original workload commands
succeeded and all ten Commits were acknowledged. The fresh-mount native final
verifier failed on `d000/f00054` metadata. Expected regular file, 4096 B and
mode 0640 all match retained actual stat. Expected mtime is
1700000020.123456789; actual is 1791646338.023150971. This is an atomic-save
destination, link-target index 4, replaced by the actual cycle-2 new inode.
P-3 insertion/deletion targets are different paths and cannot explain it.

The frozen `v016_m1.rs` atomic-save sequence sets the copied stage-4 mtime on
an empty temporary before write_all and sync, then renames it. POSIX writes
update its mtime; rename preserves that new inode's attributes. The frozen
`v016_oracle.rs` stage-5 shadow changes the content while retaining stage-4
mtime, then requires those declared seconds. The current port retains that
assertion; this failure is not just stricter nanosecond comparison. Root
reviewed product write/rename source: it applies the current write timestamp
and preserves the moved inode's attributes. A single tiny native exact-order
diagnostic confirmed the same timestamp change and rename preservation in
73,542,166 ns under the private 120 s stop, with no Store, preparation or case
replay. A single read-only stat on the retained verifier mount took
83,638,625 ns under 15 s. Product, frozen helper and oracle remain unchanged.
No timestamp normalization or product behavior change is authorized by this
inconsistency. The strict case remains FAIL; later independent public persisted
and census proof steps were not reached and remain unverified.

The sealed diagnosis is Mixed's
`benchmark-results/fs-bench-pro/2026-10-10-mixed-m1-metadata-diagnosis-fa70/diagnosis.json`,
SHA-256 `8bf26f7790c069e348b04e70bcbe364174752f76d074bf035ed556e3ac7eea6d`;
Root authenticated its six original/transcribed source and prepared scope
seals. Failed container
`17104e241466b65645273b056fd534d72632f0b5f594593b41e4fdc09d60d497`,
sample volume `layerfs-fsb-sample-7b1776f836b24ae4` and verifier Workspace key
`055bcf6d443c9f062e4061b25a5b8f5c6d7196f02eedc9f3ad85a968cc1ab29b`
remain retained. Measured mount Gone does not qualify the leaked verifier mount
or establish Stop; no inferred teardown is recorded. The other four expressible
prepared M1 cases proceed individually once at unchanged source and limits,
preserving each actual outcome. The incomplete final preparation is untouched.

A read-only audit at main 8f24e39a4 covers all 138 original cases of closed
batches 1–3 plus boundary7. It found exact paired CSV membership and no missing
records or contradictory final closures. There are 114 functional/proof passes,
24 failed mandatory cases, and 134 complete clean closes with four unestablished
closures (capped3 and boundary alias1). The seven historical scoped numeric
PASS labels remain explicitly unqualified for full numeric cache admission;
107 numeric rows are INELIGIBLE. New Init checkpoints are authoritative only
for their affected product route, with original Init4 preserved separately.
Original snapshot failure and H11 boundary failure/unrun rows are also retained.
Two final completed cases exceed their 15 s soft limits: locality dense500
15,251,166,166 ns and snapshot 24,397,444,499 ns. The original failed snapshot's
22,049,536,459 ns exceedance stays historical. No speed verdict follows.
Audit JSON is Tiny's ignored
`benchmark-results/fs-bench-pro/2026-10-10-closed-evidence-inventory-8f24e39a4/inventory.json`,
SHA-256 `63b5d464da1845d032e11a2931d4b454696adfa1205991e095cf68016a7ec8c6`,
verified by Root. This audit runs no product/test/measurement or Docker mutation.
Later batches remain pending; no completion claim is made while batch 4 runs.

| `fa9d4c8fa` | Preserve first bounded dedup and strict M1 failures with source diagnosis and unaffected evidence audit | 79370 | 79370 | +0 |


Batch 4 / mixed_load_bearing is truthfully closed with all ten original cases
ported and visible: four prior-source episode functional/proof/clean-close
passes (numeric INELIGIBLE), five new strict M1 failures, and one failed
preparation with actual execution NOT_RUN. The full aggregate parity remains
FAIL for the last unprepared case; nine completed preparations retain their
individual unchanged parity PASS. This closure follows the specific failed
preparation exception in spec section 9 and keeps that failure visible. It
claims no passing full done-when result. No identified fixable product
correctness defect remains from the frozen M1 timestamp inconsistency.

The four episode results stay in the existing source5faaa263c CSV pair. The new
source is `fa70bbb01c413a0bbbadd5c10d81652580b046f8`, product9ac88edb2,
with the exact admitted a05ed6027 nine-asset build. Its two ten-row CSVs are
integrated from worker commit `dae0c5ba63bc0b7a658b83c33eb79005bc9d73cb`,
tree `c5058858487c4680e946d1f3ea4fd9f8bff6e95d`. Root authenticated both
CSV hashes, exact original order, six receipt manifests, all five actual
acknowledged Commit counts, failed volume/container bindings, and both sealed
handoffs. New performance rows are four NOT_RUN, four FAIL and two N/A; proof
rows are five FAIL and five NOT_RUN. Prior episode rows contain only exact
old-source receipt references in their tags; no old observation is relabelled,
copied into a new receipt or replayed. The old CSVs and all failures remain.

Each prepared M1 case ran once. Every declared command and Commit completed,
then final mounted native verification rejected the frozen atomic-save mtime.
The 100 MB cases fail at d000/f00054; 500 MB cases at corresponding d003/f00004.
All five failed verifier mounts and their containers/volumes remain retained.
Measured mount Gone does not establish verifier cleanup or KnownStop. All
closed persisted public proofs, exhaustive cycle-end payload proofs and
unfiltered global census steps are NOT_REACHED. Request/job/statement counts
and final Store/overlay storage stay UNAVAILABLE; raw empty dictionaries do
not become zero. Original created-mode observation prefixes and final
17-domain maps remain in raw evidence. H17's source-backed native diagnosis is
preserved without product, helper, oracle, fixture, workload or limit changes.

| Original case | Commit ACKs | Sample / proof attempt | Product phase ns | Proof wall ns | Outer wall ns / 120 s | Actual outcome |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| v016-mixed-development-100mb-5000-k10-v1 | 10 | 1 / 0 | 1274316748 | 1496499708 | 11103476500 | FAIL, frozen M1 metadata assertion |
| v016-mixed-development-100mb-5000-k100-v1 | 100 | 1 / 0 | 11798344667 | 1895425000 | 38179369625 | FAIL, frozen M1 metadata assertion |
| v016-mixed-development-500mb-30000-k10-v1 | 10 | 1 / 0 | 2101087669 | 9688224875 | 28481784791 | FAIL, frozen M1 metadata assertion |
| v016-mixed-development-500mb-30000-k100-v1 | 100 | 1 / 0 | 21059319673 | 9343090875 | 70322889458 | FAIL, frozen M1 metadata assertion |
| v016-mixed-exhaustive-100mb-5000-k100-v1 | 100 | 0 / 1 | N/A (raw diagnostic 12491242382) | 50993633416 | 52235080209 | FAIL, frozen M1 metadata assertion |

The 500 MB K100 ordinary case records EXCEEDED:21,059,319,673 ns against the
unchanged 15,000,000,000 ns soft product limit. It still completed its declared
commands/Commits; its final metadata verifier failure is retained separately.
No speed change or limit adjustment follows. Exhaustive100MB is verification
only: sample0, proofattempt1, performance N/A, with the raw phase clock merely
diagnostic. Its 50,993,633,416 ns proof clock covers the whole verification-only
scope. Original 120 s operation/300 s proof declarations remain distinct from
the owner's effective 120 s whole invocation ceiling. All five outer commands
exit1 within their 120 s stops; no outer expiry or retry occurred.

| Failed case | Retained container | Retained volume | Verifier Workspace key |
| --- | --- | --- | --- |
| v016-mixed-development-100mb-5000-k10-v1 | `17104e241466b65645273b056fd534d72632f0b5f594593b41e4fdc09d60d497` | `layerfs-fsb-sample-7b1776f836b24ae4` | `055bcf6d443c9f062e4061b25a5b8f5c6d7196f02eedc9f3ad85a968cc1ab29b` |
| v016-mixed-development-100mb-5000-k100-v1 | `934e370752ddec15080428e8f1e406ec1abb1e05cc7ccf40832f9b9dd33c5336` | `layerfs-fsb-sample-a65f4f9bc9164028` | `84d4d3246f3b242334a88ccb134f03853c41d9e53758f5ccc4a8e85b516ff966` |
| v016-mixed-development-500mb-30000-k10-v1 | `634ab49ac260d895e8ad28d72e48ed3c1ea13be89a94f91c6fa96c20500a2616` | `layerfs-fsb-sample-91d006f84a304656` | `f4ee509d079378156c15610a0f7d022943d110c8c1f86917227afb4c40658618` |
| v016-mixed-development-500mb-30000-k100-v1 | `00001cd2d7c3721819e408a16670acf8aa292044c9cdde9b82ec8cdb90e9e6af` | `layerfs-fsb-sample-17a6d767cd95424b` | `825a1c474ea98e055b93494796fd876534a94bb7535be1878c367438e6524b97` |
| v016-mixed-exhaustive-100mb-5000-k100-v1 | `bb8b22eec8835a8d519db0b2fc91ec7dc4d81bd8f024978426a8e3b3e03d1af3` | `layerfs-fsb-sample-db4c34b2f9c845a0` | `e4198fad4556a1b45055f30ac07beeb0b993fe4c25a22310a063a8bb7512ef4c` |

The final exhaustive500MB case has only the separately authenticated
administrative preparation closure. Its original source4c83d1c79 preparation
failed at120,033,743,083 ns/exit124 and has no sealed complete input. Actual
case/sample/proof attempts all remain0, with no benchmark observations or
resource custody invented. Its original partial step80 and all raw hashes
remain untouched; no reprepare or sample claim is made. Both proof-only
performance rows are N/A, and their differing proof/actual dispositions are
explicit in verification CSVs and tags.

Compact handoff is private Mixed
`benchmark-results/fs-bench-pro/2026-10-10-batch-4-mixed-product-fix-sync/compact-handoff/handoff.json`,
SHA-256 `d1a4d5b6963bd8b12c2cc1ee507efbd94f9b3528b5f11e313018b63353abaede`.
The sibling final-handoff/handoff.json is
`e06a27c476b693684de7f2e96ef0970d3fb16e9cb710bca15d71a174e6d39240`.
These bind exact preparations, binary/image/source identities, actual ordered
Commit tuples and parent chains, final mounts, all mode prefixes, native
stderr, original scopes/omissions and resource uncertainty. The report adapter's
first invocation failed with KeyError before any CSV write; both that tool and
its stdout/stderr remain. A corrected derived step count then wrote the two
CSV outputs once under the explicit 120 s stop, without any case or verifier
replay. This changes no committed harness or product source; qualified owning
checks remain reused at exact bytes. Shared host/Docker VM interference and
cache limitations are still declared. Remaining dedup execution must close
before batch 4 closure and any batch 5 work.

Tiny's read-only prepared-master inventory also authenticated original
creation/provision/known-Stop/removal provenance for68 exact master volumes
covering closed batches1–3 and boundary7. No missing/conflicting provenance was
found. Current Docker existence/live use remains UNAVAILABLE: no Docker
inspection or deletion occurred. Ignored master-custody.json beside its case
inventory has SHA-256
`a7b3ad8f2158eb891b232fdf1d3c8c49051d9b0e2fc9e48d91fcb028d49453f1`,
verified by Root. Original owner resources and failed sample resources are
excluded. Prepared masters remain retained for declared reuse during rollout.

| `fa70bbb01` (worker sync) | Import exact reviewed corrected product/harness into Mixed worktree | 79366 | 79370 | +4 |
| `dae0c5ba6` (worker) | Record every new M1 failure and original unrun selection in both CSVs | 79370 | 79370 | +0 |
| `1a21ebfc3` | Integrate complete mixed family result pair with strict failed closure and custody | 79370 | 79370 | +0 |


Batch 4 / dedup_branch_history is closed with all26 original cases ported,
prepared and parity-qualified, and every directed actual attempt recorded.
Three distributed cases completed at original source7905e26a2 remain unchanged.
At clean sourcee308118a9,18 new cases passed functional/native/public persisted
history proofs and the unfiltered global Commit census, with Gone, KNOWN_STOP,
bound publication/outer acknowledgement and authenticated administrative removal.
Five original500-step cases failed at the fixed external120 s ceiling while
still progressing. Aggregate family outcome is21 functional/proof/clean-close
passes and five bounded failures. Every complete numeric observation remains
INELIGIBLE; no coldness or isolated timing qualification is asserted.

Root integrates exactly two26-row CSVs from worker
`c4aea655d74b5277086000d397e7ee00520d10b4`, tree
`1e73a1bdd06ffa3ce63c72e126d6be8d01205010`. Their SHA-256 values are
`2302821d4fbec8cf06db11ceee222e38db69f9fde25cd3dacf8306e13c9a2509`
and `d260d24abcc242d99b6757589210689327e10c4a73263dbb9ac61b9ff7fcb95d`.
New source selection has18 INELIGIBLE, five FAIL and three NOT_RUN rows, with
proof18 PASS/five NOT_RUN/three prior-source NOT_RUN. The original three are
linked in tags at their exact source and receipt hashes; no observation is
relabeled or replayed. Older H11 and H13 failed receipts and their CSVs remain.

Root authenticated all18 passed receipt manifests and guarded return/removal
evidence in the owning Capped checkout, both CSV hashes and original ordered
membership, and all five separate administrative external-failure bindings,
actual acknowledged Commit counts and last IDs. An initial read-only inspector
used main's relative-path context for a private partial directory and refused
it; the inspector was then run in its correct checkout context. This was no
product, test, verifier or case invocation. No receipt or source was changed.
The final handoff is Capped's
`benchmark-results/fs-bench-pro/2026-10-10-batch-4-dedup-e308118a9/family-handoff.json`,
SHA-256 `d9669ca7d715bdf5e6d698e71a13c525cf9143a0e4fb6731e0c98479294d6996`.
It binds the final source/build, all preparations/helpers, binary identities,
counts/storage, case limits, source-separated references and resource dispositions.
Run registration remains dated2026-10-10; local closeout is2026-10-11.

| New completed case | Measured phase ns | Proof ns | Requests / jobs / statements | Store logical / allocated B | Overlay logical / allocated B |
| --- | ---: | ---: | --- | --- | --- |
| dedup-history-hotset-1 | 201370084 | 3162223333 | 11 / 108 / 496 | 1434304 / 1437696 | 245760 / 268681216 |
| dedup-history-hotset-10 | 766349919 | 12492346125 | 201 / 1273 / 6776 | 2159424 / 2162688 | 245760 / 268685312 |
| dedup-history-hotset-100 | 5800506292 | 10699432917 | 741 / 10273 / 46736 | 6106760 / 6107136 | 245760 / 268685312 |
| dedup-history-recurring-1 | 146911542 | 2910611083 | 10 / 104 / 479 | 1483744 / 1486848 | 278528 / 268713984 |
| dedup-history-recurring-10 | 579336499 | 7329841459 | 55 / 960 / 4306 | 1817464 / 1818624 | 278528 / 268713984 |
| dedup-history-recurring-100 | 4447416838 | 5131018166 | 505 / 9510 / 42556 | 5348304 / 5349376 | 278528 / 268718080 |
| dedup-history-metadata-1 | 193854126 | 2993447917 | 8 / 86 / 390 | 1434304 / 1437696 | 245760 / 268681216 |
| dedup-history-metadata-10 | 657671290 | 7499832667 | 35 / 798 / 3569 | 1768024 / 1769472 | 245760 / 268681216 |
| dedup-history-metadata-100 | 5369214828 | 5783784041 | 305 / 7908 / 35339 | 5327704 / 5328896 | 245760 / 268685312 |
| dedup-history-unrelated-1 | 523610125 | 3315491083 | 2206 / 10450 / 52021 | 2649704 / 2650112 | 1548288 / 269983744 |
| dedup-history-unrelated-10 | 3300676626 | 15962014542 | 19027 / 100279 / 476416 | 15855616 / 15855616 | 1548288 / 269983744 |
| dedup-history-unrelated-100-mixed-v2 | 7645023676 | 5489526250 | 6315 / 56515 / 269052 | 109027328 / 109027328 | 1306624 / 269742080 |
| v016-history-large-hotset-k10-v1 | 714443125 | 2356735208 | 59 / 1341 / 6658 | 3711664 / 3715072 | 241664 / 268677120 |
| v016-history-large-hotset-k100-v1 | 5398337128 | 2507814209 | 539 / 13139 / 65204 | 7175648 / 7176192 | 241664 / 268677120 |
| v016-history-namespace-inode-k10-v1 | 704197708 | 2334659583 | 162 / 1688 / 8014 | 3711640 / 3715072 | 237568 / 268673024 |
| v016-history-namespace-inode-k100-v1 | 5685274293 | 2509333500 | 1530 / 16761 / 79337 | 7409312 / 7409664 | 237568 / 268673024 |
| v016-history-boundary-cycle-k10-v1 | 670640959 | 2335999667 | 115 / 1472 / 7083 | 3530384 / 3530752 | 233472 / 268668928 |
| v016-history-boundary-cycle-k100-v1 | 5286368682 | 2442578625 | 1105 / 14612 / 70308 | 7192056 / 7192576 | 233472 / 268668928 |

All completed measured phases and proof clocks are within their unchanged
reported limits. Original inherited declarations retain15 s product/45 s
proof/59 s publication, with120 s execution; the six original v0.1.6 cases
retain15 s product/25 s proof/35 s publication, with60 s execution. Every
external invocation still has the owner's120 s ceiling. No new limit is added
or adjusted. Five incomplete attempts have no complete product/proof clock,
final counts/storage or complete-limit verdict. Their external elapsed wall and
partial acknowledgements are diagnostics, not substitute headline metrics.

| Failed original case | External wall ns / 120 s ceiling | Partial acknowledged Commits | Retained container | Retained sample volume |
| --- | ---: | ---: | --- | --- |
| dedup-history-distributed-500 | 120023452708 | 377 | `046adc09eb527af87ad8eb781c6004089656ebcfd941ed6eb185a6c901d24625` | `layerfs-fsb-sample-23b0cb54e27f4674` |
| dedup-history-hotset-500 | 120025993167 | 431 | `d30bb0206b2b8581b3154b6aaeef0d6986955ebe317a88a2e0bc3f2eb948f220` | `layerfs-fsb-sample-dced6d7107df42e4` |
| dedup-history-recurring-500 | 120032936166 | 378 | `952df1ec53aa8e6e3544f00aab745e416cc1ab54a32896621ac0d8d472331b86` | `layerfs-fsb-sample-143c263052c348d0` |
| dedup-history-metadata-500 | 120031003000 | 387 | `4e3c949e7dabf7aaffec07563be8894a7967bea9273df84439f7bb3af46f74ef` | `layerfs-fsb-sample-a4e29256c1544436` |
| dedup-history-unrelated-500-mixed-v2 | 120023031709 | 349 | `24df68fac6a06f11b05bcb5484abf03bda708049d6a9e44bfe5aef3c4970c1ac` | `layerfs-fsb-sample-ce9a815415e844b9` |

All five outer attempts exited124. Final runtime events report parent stdin
ending without explicit Stop, with no complete receipt, End/Gone, KnownStop,
verifier or publication/outer ACK. Separate administrative closures preserve
actual sample/operation attempt1, completed0, functional unverified and proof
NOT_RUN. Their original cloned volumes and containers remain untouched. A
read-only finite-schedule review found exactly500 commands/required Commits,
ordered bounded command inspection and bounded Commit waits; recent successful
work continued to the cutoff. No concrete source-backed deadlock, step500
refusal, nonzero completed command or owner panic was identified. A lost Commit
reply remains unknown publication/cancellation state. No retry, speed change,
workload reduction, new producer or expanded timeout is selected. The review's
sealed handoff is private Mixed
`benchmark-results/fs-bench-pro/2026-10-10-batch-4-dedup-watchdog-readonly-e308/handoff.json`,
SHA-256 `b1cbf97e68785b8a3ab6e470fb0bf836551dafd7ef415b5aa695a91ef7cc9309`,
authenticated by Root. Full original500 completion remains unverified.

Only owned passing sample containers/volumes were removed after guarded proofs
and acknowledgements. All five immutable dedup masters, original failed sample
resources, protected owner resources and preparation bytes remain preserved.
There was no fixture regeneration, redundant build, unchanged-case repeat,
optimization, cache/worker/profile change or later-batch execution. All six
original v0.1.6 cases completed their original proofs; no new product correctness
bug surfaced. This result/doc integration reuses exact qualified owning checks
and changes no product or harness source.

Batch4 is now truthfully closed with43 original selections across its three
families: boundary6 pass/one strict metadata failure, dedup21 pass/five bounded
failures, mixedfour pass/five strict M1 failures/one failed preparation whose
actual case is NOT_RUN. Thus31 functional/proof/clean-close passes,11 retained
actual failures and one retained unrun actual case. All new complete numeric
rows remain INELIGIBLE; proof-only rows remain performance N/A. The mixed
aggregate parity stillFAILs its missing sealed preparation, and the explicit
section9 exception permits continuation while preserving that failure. The
500 MB K100 ordinary mixed clock21,059,319,673 ns exceeds15 s; no optimization
or speed verdict follows. Known H11/H12/H13/H14/H16 harness faults and H15
product policy defect are fixed and qualified; H17 frozen inconsistency and
H18 bounded failures remain visible. No identified fixable product correctness
bug remains. Batch5 may now begin using the existing public SDK Fork/History
methods; batches6–7 remain pending. Durable stays
NOT_RUN — disabled by owner until explicit reauthorization.

| `e308118a9` (worker sync) | Import exact reviewed main source before remaining dedup attempts | 79370 | 79370 | +0 |
| `c4aea655d` (worker) | Record18 completed and five bounded dedup attempts plus three prior-source references | 79370 | 79370 | +0 |
| `27708c11d` | Integrate final dedup CSV pair and close batch4 with every retained failure | 79370 | 79370 | +0 |


Batch5 source review begins after committed batch4 closure27708c11d. The three
original families have17 cases: branch_development6, multi_workspace_development5
and historical_access6. Existing worktrees retain disjoint family ownership;
Root owns shared runtime, fixture/controller/oracle integration and SDK route
qualification. No batch5 preparation, native work, parity or sample has begun.
The original public ProjectApi fork/history facades already send one existing
Fork/History request; no duplicate SDK method or product feature is required.

P-2's earlier proposed difference is corrected in this separate spec commit.
Frozen v016_access.rs344–350 calls fork_branch from the selected source Branch
and Commit, then396–403 mounts the resulting access Branch. This is already
the original selected lane, not a new extra Branch introduced by current core.
Current persistence/history/branch.rs verifies source stack and actual ancestry,
then stores the selected Commit's own base and head on the new Branch without
copying content or history rows. The direct Branch-only mount route therefore
has a prospective existing expression, still requiring actual SDK/proof
qualification. Original producer heads, selected roots, local ancestry and
reference-versus-distinct-Commit cardinality remain binding. Earlier decision8
and P-2 planning statements are historical; decision51 corrects the topology
interpretation before any historical-access sample. The spec changes only this
source-backed prerequisite recommendation; no workload, guard or limit changes.

The original source also distinguishes common segment-start barriers from
per-stage barriers, actual K/2 discard for two-worker M1, and legal identical
compact A/B Commit tuple reuse. Family authors are preserving executable
behavior and documenting disagreements with older prose. They retain H17's
frozen timestamp assertion. Shared machinery must preserve original workers,
ordered writes, concurrent histories, ancestry and exact tuple deduplication;
no serialization to fit, invented threshold or weaker census is selected.
Upcoming implementation and proofs remain pending. This docs-only commit has
no product/harness change or test invocation. Durable remains
NOT_RUN — disabled by owner until explicit reauthorization.

| `19344b6a3` | Correct original historical-access Fork topology in P-2 before batch5 execution | 79370 | 79370 | +0 |


### Batch 5 branch registration, 2026-10-11

Imported worker commit `de61928039ecb4d6eea39f452667dfba4d04433f`'s
three owned files unchanged: six original branch registrations, their shell
wrapper and external family tests. The worker's private clean tree retained the
frozen original hashes, names, role-local stages, ancestor Fork points, common
child start barrier, operation order, seed, selected proof checkpoints, resource
profile and independent watchdogs. Trunk has ten Commits; A/B fork from its
fifth, and descendant C forks from A's fifth. Canonical identical A/B Commit
tuples remain legal; all Created acknowledgements are counted separately.
Strict H17 metadata assertions remain. No case or limit is reduced.

Four owning authoring tests passed once in 2.305 s, complete invocation
2,402,266,041 ns, under the private 120 s wall lock. Their unchanged source
qualification is reused here; no native preparation, parity, mounted sample or
P1 SDK proof is claimed by this registration commit. Raw qualification remains
in the owning worktree under
`benchmark-results/fs-bench-pro/2026-10-11-branch-authoring-tests`.

The explicitly authorized three existing worktrees continue disjoint work.
Root owns Python controller and integration; Capped, after finishing contextual
native support, now owns runtime session/runtime/events/main/args and scoped
external runtime tests; Namespace owns fixture admission; Tiny owns Python and
Rust independent graph/access oracles. Each checkout has its own lock and
target. Shared host/VM interference is unquantified and retained in numeric
eligibility. No measurement begins until the common SDK route and final build
are qualified. Batches 6 and 7 remain pending.

| `de6192803` (worker) | Register six original branch cases and wrapper; four owning tests pass | 79370 | 79370 | +0 |
| `c45de8d94` | Integrate exact branch registration and record batch 5 ownership | 79370 | 79370 | +0 |


### Batch 5 multi_workspace_development registration, 2026-10-11

Imported only worker `cd89fa6d7ba3eeb22bce152b7faf2aa62bd85c86`'s exact three owned registration/wrapper/test
files. This records five original multi-workspace cases, preserving two original workers or the original four-worker extension, one segment-start barrier, worker-local command/Commit order, exact K/2 Discard and same-Branch fresh-key reopen, full ACK topology and selected original M1 payload obligations.
Five owning authoring tests passed once in 4.777 s under a private 120 s wall stop. Bash syntax passed. The initial valid SHA witness exposed macOS Bash repeat-count portability; the wrapper now checks lowercase hex and exact 512-character length separately without changing any workload. The initial failure is retained.
The owning source qualification is reused; no unchanged test or case is replayed.

All source hashes and independent original limits remain declared in family.json.
Preparation, final SDK/runtime qualification, parity and actual case attempts remain
NOT_RUN. Shared fixture/runtime/oracle work is in progress under disjoint worktree
ownership. Closed prior receipts and protected/failed resources are unchanged.
Durable stays NOT_RUN — disabled by owner until explicit reauthorization.

| `cd89fa6d7` (worker) | Author five original multi-workspace cases and owning tests | 79370 | 79370 | +0 |
| `f4dcb18a7` | Integrate exact multi_workspace_development registration | 79370 | 79370 | +0 |


### Batch 5 historical_access registration, 2026-10-11

Imported only worker `1c1a99917433d0af1ec6fbfe8a93f81c2b76272b`'s exact three owned registration/wrapper/test
files. This records six original historical-access cases, preserving the exact producer case, source Branch/local ordinal, independent read target algebra, reader then verifier on one mount with zero Commits, original Fork-then-mount topology, alias/inode checks, unchanged producer heads/census and original 25/35 s proof limits.
Four owning authoring tests passed once in 1.930 s under the private 120 s wall lock. No producer preparation or mounted access invocation was performed.
The owning source qualification is reused; no unchanged test or case is replayed.

All source hashes and independent original limits remain declared in family.json.
Preparation, final SDK/runtime qualification, parity and actual case attempts remain
NOT_RUN. Shared fixture/runtime/oracle work is in progress under disjoint worktree
ownership. Closed prior receipts and protected/failed resources are unchanged.
Durable stays NOT_RUN — disabled by owner until explicit reauthorization.

| `1c1a99917` (worker) | Author six original historical-access cases and owning tests | 79370 | 79370 | +0 |
| `7d071e39f` | Integrate exact historical_access registration | 79370 | 79370 | +0 |


### Batch 5 contextual native model/writer support, 2026-10-11

Imported only `4329298a56930b3990d8af67f1fa83687d58250c`'s four owned
harness files: context model, native materialization/verifier recipe, authenticated
prepared writer and external model tests. Original sequential APIs and descriptor
schema remain compatible. Role/local-step APIs preserve ancestor-prefix content,
compact original algebra and separate inherited/own actual M1 created-directory
modes. No fixture/default mode substitutes for an actual observation. Context
prepared descriptors bind the case, role, local step, exact ordered member
geometry, explicit attributes and payload hashes; measured application consumes
sealed inputs without recipe generation.

The worker compiled native assets and external test targets first, with
Rust1.85.1 and repository ARM64 flags, under private locks. Six context checks
passed once in2.61 s (6,725,614,542 ns complete), five unchanged legacy M1
checks passed once in0.29 s (1,637,344,042 ns complete), and one additional
legacy prepared-header/member compatibility check passed once in0.11 s. Every
test invocation had an explicit120 s stop. Scoped rustfmt with skip_children
passed. Frozen Rust sources independently matched recovery commitd296981ac.
The final qualification receipt in the Capped worktree is
`benchmark-results/fs-bench-pro/2026-10-11-context-native-final-tests/qualification.json`,
SHA-256 `ce0e148a9bfa3f50cd2789ba82c1c09f993241fe125b40dc080d78a5de4ccc2a`.

Development failures are retained: an initial external-test field typo failed
compilation; an initial rustfmt invocation followed includes and reformatted six
frozen files. The worker restored exactly its accidental original-byte changes,
checked recovery-source hashes, and rebuilt final native assets before final
qualification. No frozen change survives and no failed receipt is relabeled.
Neither error establishes a product defect. The unchanged owning qualification
is reused on import. Linux build seal, real public SDK Fork/History proof,
preparation, parity and all17 batch5 actual cases remain unverified.

All17 registrations are now committed on main. Shared Rust runtime, fixture
admission, Python controller and independent graph/access oracles remain in
progress under the recorded disjoint owners. Batches6–7 remain pending; all
protected resources, failed sample custody and historical evidence are preserved.

| `4329298a5` (worker) | Add contextual native model/recipe/prepared writer and external qualification | 79370 | 79370 | +0 |
| `7c9e011ff` | Integrate exact qualified contextual native support | 79370 | 79370 | +0 |


### Batch 5 contextual fixture admission, 2026-10-11

Imported only worker19a6dbf798ad6ff9066bacbf7803266cc824ac48's fixture
module and external contextual fixture tests. The new prepared-context schema
binds the exact committed case and model/native source, initial role and full
materialization manifest, selected original role/local checkpoints, prepared
SDK members and scripts, helper binaries, receipt hashes and role-specific
actual-mode scope. It never treats materialization modes as measured observations.
Legacy batch1–4 interfaces, schemas, source/profile guards and member checks
remain unchanged. The original two-worker Discard witness is a sealed native
recipe asset; branch/four-worker records require its absence.

Twenty-five final owning tests passed once in2.869 s under a private4/120 s
wall stop, including ten context admission tests and fifteen unchanged legacy
fixture tests. Earlier mocked checks are development evidence; final changed
asset coverage was run at final owned source. No native asset generation,
Store preparation, Docker action, parity, case or Linux seal was performed.
Fixture/module SHA-256dda1a0f51d5a5a3fe82ca8f34eb796124dc41e9fd9acf9efcce10b9d627dfdb2;
test SHA-256840579f911e0e9da4977cfc6a5f0f6b0c895e581f27baaaf618d5a8ec805396e.
The unchanged owning qualification is reused on import. Root's separate native
discard CLI and controller are still being implemented; no partially integrated
source is presented as prepared or sampled.

The fixture worker proceeds to source-only historical producer dependency
admission. Every access fixture will derive from a new complete closed producer
setup and its exact typed transcript, retaining initial master reuse without
reusing a mutated measured sample. The runtime and independent oracle workers
retain their disjoint ownership. P1/public SDK/Linux/actual cases remain pending.
Durable stays NOT_RUN — disabled by owner until explicit reauthorization.

| `19a6dbf79` (worker) | Add exact contextual fixture/prepared-member and discard admission;25 final tests pass | 79370 | 79370 | +0 |
| `4f7760944` | Integrate qualified contextual fixture admission | 79370 | 79370 | +0 |


### Batch 5 controller and native Discard authoring, 2026-10-11

Root added role-local context choreography to the existing workspace engine and
a native prepared Discard-witness CLI to the existing recipe. The controller
uses exactly one original SDK write/acknowledgement per request. It preserves
original names, all role-local command/Commit order, historical Fork ancestry,
actual one-start-barrier concurrent rounds, explicit A witness/Discard/reopen
with B already published, cumulative actual role-separated directory modes,
selected native checkpoints, public SDK History records and full topology TSV.
Individual public spans retain the original summed-call clock; actual external
whole-graph wall and different-worker Commit overlap coverage remain separate.
No stage6 hold, skip body, per-step barrier or invented overlap threshold is
added. Unknown paid cache inputs keep numeric observations INELIGIBLE.

Original batch5 performance and verification work/reserve/hard limits are
validated without enlargement. The native witness derives the frozen original
refresh-cohort member and256-byte content during setup, seals descriptor/payload
hashes, and refuses branch/four-worker or non-fresh outputs. Explicit
preparation_only controller mode creates no measurement claim and reports
sample0/setup-only; a fresh full producer's exact ACK graph, End/Gone/KnownStop,
closed export/normalization and proof become an immutable access dependency.
It never consumes an earlier measured sample or creates a selected-root-only
Store. Callback and runtime composition remain source-level work; no actual
producer or case has run. Concurrent exact counter attribution is currently
UNAVAILABLE with actual raw numeric groups retained, rather than duplicated
daemon cumulative deltas. This gap remains open before actual collection.

Native recipe compiled first in4,958,997,625 ns. Six new controller checks
passed once in the initial seven-test invocation, which FAILED the new witness
test in2.688 s, complete3,008,966,583 ns. The new test incorrectly assumed
`data/`; frozen v016_common.rs136–153,298–357 uses dNNN/fNNNNN and pool-base
ceil((files-1120)/100). The corrected test requires exactd039/f00064 and
d289/f00064, preserving all schema/byte/hash/freshness/refusal assertions.
Only that changed failed test reran and passed in0.128 s, complete401,164,583 ns;
all six unchanged checks were reused. Thirty-three owning legacy controller/
driver/workspace/native-receipt checks passed once in0.362 s, complete
674,988,167 ns. All build/test commands had explicit120 s stops and serialized
main locks; scoped rustfmt used skip_children. No product or frozen source
changed. The native recipe bytes were unchanged by the test correction.

Raw receipts remain under
`benchmark-results/fs-bench-pro/2026-10-11-context-controller-native-qualification`;
qualification SHA-256ad0fa2314d22b70b313ef01a03a94aa4772998911672fec28046d707999849bc.
The retained failure is authoring evidence, not a benchmark or product result.
Shared runtime/oracle integration, final sealed Linux build, real P1/P2 SDK
covering proof, historical preparation/admission, parity and all17 cases remain
pending. No source-level test is represented as mounted or Linux qualification.

| `604548cd7` | Add scoped context controller, fresh producer callback and original prepared witness CLI | 79370 | 79370 | +0 |


Source review before any P1 call found that the current provider supports
Page1..128 but the existing native bridge's HISTORY_WINDOW is32. The prospective
controller128 request was corrected to explicit anchored32-record SDK pages,
with exact page counts, original head/tuple order toEOF, opaque continuation
file hash/byte checks and no repeated or non-progressing cursor. This changes
transport page geometry, not the original workload or ancestry coverage. No
128-record live request, refusal, retry or product cap change occurred. One
new owning page/negative-tamper test passed once in295,632,917 ns under120 s.
It supplements the unchanged earlier controller qualifications; no passing
check replayed. Its separate authoring qualification is
`benchmark-results/fs-bench-pro/2026-10-11-context-history-pagination-qualification`,
SHA-256c5cf8e4759ad7abbb89bfedae731d11bca35c0086ffdf7c8081092c0a65ed8e2.
The four newly written test artifacts were relocated unchanged from the earlier
authoring folder before sealing this addition; raw invocation path strings
retain their original locations. The earlier sealed authoring manifest remains
byte-identical and verifies; the new manifest binds the relocated exact bytes.

The producer callback seals its parent dependency after the separate owned
passing-resource administration subtree is complete, keeping all evidence
hashes stable. Uncertainty still retains resources. Public runtime composition
and every real producer/case operation remain NOT_RUN.


Concurrent counter pricing is now authored with a separate narrow qualification.
It uses one shared daemon cumulative first-ready/last-measured-Status delta,
correlates every actual observed SDK range by authenticated channel scope/call,
accounts for unobserved original Fork/History sends, subtracts every completed
Resources observer cost exactly once, and retains distinct terminal mount
opcode totals. It never sums a shared daemon delta per worker. Missing,
ambiguous, saturated or non-terminal evidence still yields UNAVAILABLE with
raw groups/reason retained; actual native snapshot availability remains
unverified until the case receipts exist. Three owning arithmetic/rejection
tests passed once under120 s, including missing-group/shared-scope, duplicate
terminal-owner and missing-observer failures. Qualification is
`benchmark-results/fs-bench-pro/2026-10-11-context-concurrent-count-qualification`;
no real daemon counter or case qualification is claimed.


### Batch 5 independent topology and access oracle, 2026-10-11

Imported the worker's four exact owned oracle/test files. The approved topology
TSV binds every original role-local typed acknowledgement, historical Fork
edge, published Branch, selected original payload root and provider identity.
Public History reachesEOF; the independent read-only global SQL census checks
ALL Commit and Branch tuples toEOF, including unused initial and access
Branches. Identical full canonical Commit tuples are deduplicated only in
physical-row union; acknowledgement and retained-reference counts remain
separate. Access mode limits payload reads to original selected targets and
compares independently derived full hex/SHA, absence, aliases and same-mount
inode relations across reader/verifier. Existing oracle guards remain intact.

Fourteen Python checks passed in0.526 s. Corrected exact compact native proof
passed once in11.00 s (20 other tests filtered), with30 ACKs/15 actual distinct
tuples,4 Branches and4 original selected roots. The unchanged native retained
Trunk5 access proof passed and is reused:22 paths,65,536 B,5 Branches and30
ACKs/15 distinct tuples. Native public fixtures are not live SDK P1 or mounted
case qualifications. Initial compiler errors are retained. An initial unfiltered
native invocation accidentally included frozen inline tests and was stopped
bySIGTERM within its explicit76/120 s wall bound; it FAILED and is retained.
The new fixture initially asserted20 distinct rows; exact frozen algebra shares
child-local1..5 with trunk6..10, so its own new assertion was corrected to15.
A first partial scripted correction missed that expanded assertion and failed
again; only the corrected failed owning test reran. No oracle, frozen workload,
limit or product test was weakened. Failed native fixtures65883 and66218 remain
under the owning temporary root; no Docker action occurred.

Owning raw handoff:
benchmark-results/fs-bench-pro/2026-10-11-batch-5-oracle-authoring/handoff.json,
SHA-25662905ee78d1c02d32e0ccc3e8b1da1a9dc81c07b768e842666091a2af2b4e94b.
Runtime composition, Linux seal, realP1, preparation/parity/cases remain pending.

| `24be90ced` (worker) | Author independent topology and access oracle and owning qualification | 79370 | 79370 | +0 |
| `3145bad86` | Integrate exact independent topology and access oracle | 79370 | 79370 | +0 |


### Batch 5 closed historical producer fixture admission, 2026-10-11

Imported only the exact fixture module and new historical fixture tests. Six
access cases bind four complete original producer plans: main48/49 and94/95
of100, trunk5 of the30-ACK convergent graph, and originalB/controllerb-local10
of the40-ACK descendant graph. Source/local ordinal excludes inherited prefix.
A new setup-only complete producer's ACK graph, parents/Forks/heads, known
End/Gone/Stop, normalized standalone WAL Store and immutable raw main/WAL/SHM
export are mandatory. Every attempt/normalization/manifest/receipt hash is
authenticated. A fresh byte-copy master retains full history; there is no Init
of a selected-root Store, measured-sample reuse, access reader or warmup during
preparation. Matching immutable genesis inputs may be reused explicitly.

Thirty-two final mocked fixture tests passed once in4.437 s under private
6/120 s stop: seven historical, ten context and fifteen legacy checks. Parent
regression failed as expected and remains visible. No native generation,
Store/Docker activity, build, actual producer, parity or case ran. Handoff
benchmark-results/fs-bench-pro/2026-10-11-batch-5-historical-fixture-authoring/handoff/handoff.json
SHA-256864ea5ef5827f9188db6f9d520f9b0a9533a8f959975e50fe045a5f803fe5912.
Source-only namespace worker now owns workspace.py/access controller while
Root owns integration/ledger/parity/build and live execution.

| `615c3df22` (worker) | Author closed historical producer fixture admission and owning qualification | 79370 | 79370 | +0 |
| `103f82a9a` | Integrate exact closed historical producer fixture admission | 79370 | 79370 | +0 |


### Batch 5 public SDK Fork and History covering test, 2026-10-11

Imported only the independent external P1/P2 test CLI. It has no canonical
family registration or performance sample: test IDpublic-sdk-fork-history,
sample0, diagnostic clocks. Planned actual route uses current SDK Fork/History,
trunk one/two Commits, an ancestor child, child Commit, a further Fork through
the child's inherited ancestor, exact typed public histories toEOF and every
fresh key's End/Gone plus channel EndSession/fences/KnownStop. Independent
post-Stop toy SQL census rejects extra Commit/Branch rows; no live host data
route or fabricated canonical topology proof is used. Passing owned pairs
remain until Root sees the sealed test manifest and actual outer return; failed
resources retain custody without replay or teardown guesses.

Four source-only checks passed in0.052 s under private120 s stop; the affected
final typed EOF check also passed once after boolean cursor evidence was
rejected. Empty/profile/source seals, exact tuple/EOF, single failed sender
attempt and unfiltered-extra-row refusal are covered. Actual SDK/Docker,
preparation, build seal and live covering test remain NOT_RUN. The CLI retains
inner60 s and outer120 s explicit stops. Raw handoff:
benchmark-results/fs-bench-pro/2026-10-11-batch-5-oracle-authoring/p1-handoff.json
SHA-256b81ebc84083890be9dd7a6c0135f814e43f48131f112f76d45bcf67c5657363b.

| `e7bd36c3f` (worker) | Author public SDK Fork and History covering test and owning qualification | 79370 | 79370 | +0 |
| `181e70f40` | Integrate exact public SDK Fork and History covering test | 79370 | 79370 | +0 |


### Batch 5 authenticated concurrent SDK runtime, 2026-10-11

Imported only the worker's six owned runtime/protocol-test files. Existing
public ProjectApi Fork and History methods are reused; there is no product
addition or direct Fork/History wire substitute. Selected named or explicit
@taggedBranchID sources remain typed and SDK membership-validated. Original
concurrent rounds admit every body/key/channel before one segment barrier,
then distinct authenticated channels run command/Commit loops within the
existing four-connection ceiling. Each channel has its own nonzero public
observation scope. Output streams directly to files; individual spans and
role/local/channel/scope plus send/receive boundaries are retained. History
uses the actual32 window and authenticated private continuation artifacts.
All mounts must End/Gone, then every channel EndSession/fence, before Stop.
Legacy commands retain their route and acknowledgement shape.

Final host release --no-run passed in11,033,263,250 ns; five exact scoped
protocol/barrier/concurrent-receipt tests passed in829,689,417 ns with nineteen
frozen inline tests filtered. Private120 s locks, Rust1.85.1 locked/offline,
repositoryARM64 and construction workers1 were used. Scoped fmt/diff checks
passed. Failed earlier compiler outputs remain visible. Owning handoff
benchmark-results/fs-bench-pro/2026-10-11-context-runtime-protocol-tests/handoff.json
SHA-256b1391316daa2e019bce36562a8ef30040eb2c12cf49aa4646241bb37a5276e07.
Actual SDK/concurrency/Linux/sharedbuild/preparation/parity/cases remain
unverified; no Store/container/volume was created by these source checks.

| `5e6dbd260` (worker) | Author authenticated concurrent SDK runtime and owning qualification | 79370 | 79370 | +0 |
| `794a66380` | Integrate exact authenticated concurrent SDK runtime | 79370 | 79370 | +0 |


### Batch 5 covering-test native History protocol correction, 2026-10-11

The source-only P1/P2 test now matches the actual committed runtime protocol:
request32 records, EOF cursor_path minus, integer cursor_bytes0 and absent
continuation hash. Every exact typed tuple, cardinality, branch binding and
one-exchange guard remains intact. Four final source-only checks passed once
in0.050 s under private120 s lock; prior authoring qualification is retained
without relabeling. No live128 request, refused SDK call, retry or weakened EOF
check occurred. Actual live covering proof remains NOT_RUN until a full nine-
asset seal and fresh empty preparation are qualified.

| `52479b485` (worker) | Author covering-test native History protocol correction and owning qualification | 79370 | 79370 | +0 |
| `985136704` | Integrate exact covering-test native History protocol correction | 79370 | 79370 | +0 |


### Batch 5 explicit parity build and CLI family routing, 2026-10-11

Parity has an optional explicit --build forwarded to the workspace parity
engine. Context and historical admission will require the real sealed build
to authenticate native/helper bytes; no expected seal is inferred from its own
prepared metadata. Legacy parity calls retain their argument-less route.
The source-only CLI wiring test exposed that branch/multi preparation and
controllers existed but those two original names were absent from the CLI's
WORKSPACE_FAMILIES allowlist. It failed before a parity call. Root added only
branch_development and multi_workspace_development; unported families remain
refused. This is a harness routing omission, not a product or case failure.

The failed external authoring receipt is retained. Its mock result was updated
to the actual CLI response grammar, and the final changed test exercises a
real legacy family without build plus branch with explicit build; it passed
once at final source under120 s. Intermediate changed-test evidence remains
separate; no benchmark, producer, real parity or SDK operation ran. Raw
qualification is benchmark-results/fs-bench-pro/2026-10-11-parity-build-cli-qualification.
This routing change does not claim current Linux/P1/mounted concurrency.
Root is preparing the final nine-asset build from the clean integrated runtime;
access controller and source-aware parity remain in their private worktrees.

| `821624cb3` | Expose original context families and explicit sealed parity build in CLI | 79370 | 79370 | +0 |


### Batch 5 verification-only historical access controller, 2026-10-11

Imported the final two exact owned files, including the follow-up provenance
correction. Access consumes its sealed fresh complete producer dependency,
resolves the exact role/local ancestor with anchored public SDK History32
pages, Forks the actual Branch/Commit pair and mounts the returned access
Branch. The unchanged reader then verifier run once on the same fresh key,
with exact body SHA/knownzero exit, no warmup and no Commit. End/Gone,
EndSession/fences and KnownStop precede raw export, separate typed-copy proof
and immutable full Branch/Commit census. Header retains the producer ID, with
only the original extra zero-ACK access Branch appended. Independent original
bytes/SHA, aliases/absence/stable inode and unchanged producer heads/tuple
union remain mandatory. Original22/25/35 s proof boundaries,3 s reserve,
.25 guard and owner120 ceiling remain; current4GiB main-file resource guards
are labelled supplemental to original2CPU/2GiB/noSwap/256PID/OOM checks.
Sample0/proofattempt1/performanceN/A stay explicit. Failed/uncertain mount
keys are retained before an original send. Old producer public-proof/census
fields cannot appear as current access qualification after an early failure.

Final32 scoped checks passed in0.342 s under private2/120 stop: six new,
seventeen workspace, three resource-retention and six pure history guards.
Parent absence regression is retained. No native/SQL/live/SDK/preparation/
build/parity/case/Docker action occurred. Exact owning handoff
benchmark-results/fs-bench-pro/2026-10-11-batch-5-historical-controller-authoring/handoff/handoff.json
SHA-2561db22049ca16613b74b3702ba7428f7cbb30b7530ba2c14405913025a42d7334.

| `d4a665f0e` (worker final) | Author verification-only historical access controller; exact final owning checks | 79370 | 79370 | +0 |
| `db0f6bbc0` | Integrate exact verification-only historical access controller | 79370 | 79370 | +0 |


### Batch 5 source-aware prepared batch-five parity, 2026-10-11

Imported only parity engine and its new external tests. Batch5 requires an
independently admitted sealed build, authenticated prepared input and actual
helper admission before parity PASS. Original membership/pins/frozen hashes/
differences/oracle/all-case failure sealing remain. M1 preparation proves
initial full native input and selected recipe/scope hashes; actual directory
modes remain NOT_OBSERVED and measured proof NOT_RUN. Compact requires every
original selected full-native checkpoint check. Historical binds complete
closed producer ACK/reference/distinct-ID/custody/source/Store/transcript,
exact original/normalized role/local ancestor and independent full read
expectation, with both actual reader statuses NOT_RUN. No fabricated final
M1 verifier or current access result is obtained from preparation.

Final13 source-only checks passed in0.204 s under private120 stop, including
four unchanged legacy parity guards and actual helper admission rejecting an
authenticated stale Linux helper against the independently admitted build.
Failed-before M1 scope regression and initial authoring assertions remain
visible. No actual parity/native/build/preparation/Docker/case ran. Owning
handoff benchmark-results/fs-bench-pro/2026-10-11-batch-5-parity-authoring/handoff.json
SHA-25686c996aa977f925fb32010e588d0df8ca73696034ec6abae1eaa87ef5c4b5e27.
Root's prior821624cb3 CLI wiring supplies explicit --build and the two original
context family names; all17 batch5 source definitions are now connected.

| `4661ff5e8` (worker final) | Author source-aware prepared batch-five parity; exact final owning checks | 79370 | 79370 | +0 |
| `9da25b495` | Integrate exact source-aware prepared batch-five parity | 79370 | 79370 | +0 |


### Batch 5 full nine-asset seal and copied native oracle fix, 2026-10-11

Full nine-asset build at clean9da25b495 passed, raw folder
benchmark-results/fs-bench-pro/2026-10-11-batch-5-nine-asset-build-9da25b495.
Build metadata SHA-25678ce9959b9f2528e0bd570ceb2429336cdc9d86f664e5663de6800127bb9b6f5;
product remains9ac88edb2cc44d44edd4a53f9ad89f4dee063a58. Host release
9,556,211,958 ns and Linux27,805,955,042 ns; allcompiler commands had120 s
stops, outer serialized build240 s. Pinned image sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6 verified.
Every one of1322 selected host source hashes matched inside the Linux build
container before and after compilation; before/after clean identities match.
The exact two unchanged OWNER3 passthrough refusals remain FAIL evidence; no
aggregate checker PASS is claimed. Owned passing build container removed.
Runtime89ea95a478632726bdc32a342f998f3a9e092e5109eb2061ecba628f8bdf5008;
daemon unchangedaa9de24f0ba916c0ac483f8e559d9d6986dfe4296f4d26a829801a1cb7e4ffd4.
All nine immutable archive hashes are in build.json. No product feature changed.

Fresh empty input preparation failed at its copied native oracle import before
Project Init or any P1 SDK attempt. Under /code/engine/oracle.py there are
only three parent levels; eager parents4 raisedIndexError. The generic host
SQL census's root resolver had unintentionally become a native CLI import
dependency. Retain the whole failed preparation
benchmark-results/fs-bench-pro/2026-10-11-batch-5-public-sdk-empty-9da25b495,
failed stopped native container2529d3183a36b0bd2aa44c3d705738c91513d1fc7be0c5011884ed535957a050
and created master volumelayerfs-fsb-master-empty-95af702904c04049. No master
Store/provision success, actual SDK sample, native pending cell result or
cleanup is inferred. P1 actual attempt remains0.

External shallow-copy import regression fails at parent source; host schema
pin check passes there. The minimal harness fix allows the native module to
import without a host root. SQL census still requires authoritative repository
sources and rejects their absence before opening SQLite; all schema/source
pins/profile/provider/EOF checks remain. Final copied-import/current-schema/
topology/legacy oracle scope passed under120 s. Raw red/green receipt tree:
benchmark-results/fs-bench-pro/2026-10-11-oracle-container-import-regression.
No product, workload, budget, fixture, checker or receipt was weakened. This
Python-only source change preserves every compiled input of the qualified
full build, so reuse that exact seal and prepare a new identity once.

| `e60a1aa2c` | Fix copied native oracle import; retain H20 failed setup and full build evidence | 79370 | 79370 | +0 |


### Batch 5 existing public SDK P1/P2 qualification, 2026-10-11

After committed H20 fixe60a1aa2c, new empty preparation passed once:
benchmark-results/fs-bench-pro/2026-10-11-batch-5-public-sdk-empty-import-fix.
Its owned immutable master islayerfs-fsb-master-empty-4e709a73985e48b2;
complete native empty input, explicit Disposable Init/seal/install and closed
provision are bound by prepared.json/manifest. The failed pre-fix master and
native container remain unchanged. Reused full9asset build is admitted by
exact unchanged compilation scopes; only Python oracle/helper source changed.

The actual independent public SDK covering test passed once at cleane60a1aa2c,
complete3,744,106,250 ns, under main120 s outer and60 s inner proof stops.
Raw benchmark-results/fs-bench-pro/2026-10-11-batch-5-public-sdk-proof-e60a1aa2c;
proof SHA-256ce22728330ecee7f3a9a88680ad892e39984aed4394735538ae622c9cf96f0c7.
It proves the public Fork-from-Layer, explicit historical Commit Fork, inherited
ancestor Fork through an advanced child, independently read one/two/child
bytes, all exact typed public History records/EOF, unchanged source heads,
three successful fresh mounts and explicit End/Gone, three independently
authenticated control channels with EndSession/native fences, successful
runtime return and KnownStop. Unfiltered normalized closed SQL census sees
exactlythree typed Commits andfour Branches, including unused initial and
zero-ACK ancestor; full EOF and independent manifest/schema/profile bindings
pass. Normalized Store208,896 B; root-source export/WAL/SHM and normalization
receipts remain immutable. This is a covering test with samples0 and only
diagnostic clocks, not a workload, numeric cache qualification or comparison.
The existing route expresses P1/P2; no SDK/product feature was added.

After sealed proof verification and known outer return, separate stopped-state
readback/removal deleted only own passing containera2a329492182b602a45202d1ba706e16ab814c1b5f838f1c23605a1b9caf7ed1
and sample volumelayerfs-fsb-sample-7669fd58f5004da2. Administration folder
benchmark-results/fs-bench-pro/2026-10-11-batch-5-public-sdk-proof-disposal-e60a1aa2c,
completion SHA-256d9504e19f6a447ac39d0f564b4cd576e73119c92a1c5c417f38cde54a6efa245.
Original proof manifest verifies unchanged. Preparation master remains for
rollout setup reuse; failed/uncertain and protected resources remain untouched.

Batch5 preparation and parity may now begin from the final integrated shared
source/build in the three existing worktrees. True mounted worker concurrency
and exact original17-case results are still unverified; every case gets one
original attempt, strict M1 assertions remain and counts missing exact evidence
remain UNAVAILABLE. No batch6/7 work begins until batch5 is truthfully closed.
Durable remains NOT_RUN — disabled by owner until explicit reauthorization.

| `711593c5f` | Record one actual SDK Fork/History covering proof and owned passing-resource disposal | 79370 | 79370 | +0 |


### Batch 5 historical result contract and explicit closed producer reuse, 2026-10-11

Before any historical access invocation, the agent found that one_access_case
returned descriptive performance_mode N/A while the actual driver requires
verification-only. The external regression executes the actual controller
result through bench.main; it fails at parent711593c5f before publication.
The controller now uses the exact canonical mode and keeps the original
verification-only description separately. Actual reader/verifier, zero Commit,
sample0/attempt1, all budgets, publication guard and cleanup are unchanged.

bench.py prepare now accepts explicit --reuse-producer pointing to an earlier
sealed historical preparation. It creates a new independently installed byte
copy and exact selected native expectations while reusing only the new full
closed setup-only producer. It never invokes the producer again. The original
summary, attempt identity, Store, raw WAL export, normalization, complete ACK
list, Branch graph, selected tuple and receipt hashes stay unchanged.
Cross-commit admission compares complete source membership and every producer
execution/model/workload file. Only the reviewed CLI/access adapter changes
are permitted: original/current fixture producer ASTs must be identical,
including all generation, dispatch, model, helper and install functions; the
only controller byte change admitted is H21's exact access-only mode label.
Original Git blobs must match their authenticated hashes. The added admission
receipt equality guard is itself checked without excluding any prior helper
validation. Product/binaries, oracle normalization helper and source scope
remain exact. Changed producer code is refused. No coldness is inferred.

The historical worktree at af7dbdd3c completed four first-occurrence setup
producers: boundary100, inode100, compact30 and descendant40. Authenticated
producer attempts have zero samples, complete proofs, End/Gone/KnownStop and
passing sample-resource removal. Four original source scopes pass a read-only
adapter authoring check; neither those receipts nor their source identities
were rewritten. Four-producer handoff SHA-256
a870bc0de3e5e9352c1accef8891a4f54a3954d8f44723a1c952c35a29b86c08
in that worktree's2026-10-11-historical-access-af7dbdd3c folder. Setup masters
remain for rollout reuse. Paired two preparations and all six actual access
attempts are still unrun at this commit, awaiting this reviewed source.

Owning final38 harness checks PASS in5,940,744,291 ns under120 s main R4 stop.
The two parent regressions, initial new-test path assertion failure (/var
versus resolved/private/var), and authoring script's wrong divergent ID are
retained. Corrected only the new assertion/script; no existing verifier was
weakened. Raw qualification folder
benchmark-results/fs-bench-pro/2026-10-11-historical-setup-reuse-qualification;
qualification SHA-256 d1afa459905722b296b0422b852930bc93ee0f06eaf9e9865e015e5df87df36e. No build, Store, Docker operation
or measurement ran during these checks. All nine compiled inputs still match
the existing full seal; no rebuild is needed for this Python-only change.

Branch500/K100 preparation reached its unchanged120 s outer stop in its
worktree. Late inner sealed output cannot promote the failed invocation;
actual case remains NOT_RUN and parity FAIL. Other five original parity rows
PASS and proceed once. Full cutoff/custody and source closure will accompany
the final family results. No limit, checkpoint, cache, workers or product
speed change is selected. Durable execution remains disabled.

| `4a40fb690` | Repair historical driver result contract and qualify explicit complete setup-producer reuse | 79370 | 79370 | +0 |


### Batch 5 bounded branch preparation closure, 2026-10-11

Section9's existing failed-preparation rule now explicitly also names the
original branch500/K100 case. At clean private1bc68451d its external120 s
R4 preparation supervisor exits124, without an outer completion receipt.
An immediate later read-only inspection sees no remaining original host
process; inner stdout reports PASS and prepared.json/manifest exist. These
late artifacts establish no known return inside the original wall stop and
cannot promote the invocation. Complete preparation wall is UNAVAILABLE;
actual product/sample attempts0. The reused500MB master
layerfs-fsb-master-branch-development-35913130b8a24310 came from passingK10
setup, and this failing preparation created no additional master. All raw
files, failed supervisor and late artifacts are retained.

Owning raw folder2026-10-11-batch-5-branch-1bc68451d in the edit-capped
worktree; prepare-500-k100-administrative-failure.json SHA-256
65f1881dab20b5c7112462aa95b2dd19548df5e1fcc7b5e4afb5a12d7e1b4cb8. Both CSVs will retain actual500/K100 NOT_RUN with explicit
failed setup reason. The qualified-prepared view contains only the otherfive
original inputs; aggregate parity has five PASS and one FAIL for absence of
an admissible500/K100 manifest, SHA-256
5a48a44b5f18625cc16f991c8101827a75dfb0d102d6b20587e28381ca638f4c,
complete21,718,825,584 ns/120 s. No source, timer, checkpoint, fixture, cache,
worker or product speed change is selected. This is a failed batch closure
exception, never parity PASS or a performance/capability verdict.

The first branch100/K10 and multi100/K10 actual concurrent cases have since
failed with ordinary Stage1 EAGAIN before proof. Their receipts/resources are
retained and unattempted contextual cases are held for source-backed diagnosis.
The current integration contract explicitly permits global writer/serial
reservation Busy before effect; diagnosis must distinguish that selected
outcome from a wrong result or a refusal that should succeed. No product fix,
retry or contention-masking change is selected at this checkpoint.

Read-only main commit-message inventory through4a40fb690 contains111 local
rollout commits, each with one production LOC line, this session's attribution
and reference-absent scope declaration. Raw inventory folder
2026-10-11-rollout-commit-loc-inventory-4a40fb690; SHA-256
41791f991bf7e11e34989f1429613b6d517c22b31ddfca83384fd6e194cdefae.
This audits messages only and does not substitute for each staged counter.

| `c23ac0cfd` | Explicitly preserve batch5 late-input preparation failure under section9 | 79370 | 79370 | +0 |


### Batch 5 historical private backing installer correction, 2026-10-11

First original historical boundary-before attempt at privatec6ec3e467 failed
before protocol readiness, mount or either reader. The actual50 stderr bytes
are daemon I/O: private directory ownership/mode/type. Read-only inspection
binds acknowledged own container4891bb1c3fa69f0103c187219108c4ff2dab86c48fa972c925d5f0787bb66a37
(exited1, OOMfalse, effective Config.User0:0) and clone volume
layerfs-fsb-sample-1dc1bc4c39e947b6. Both remain retained. Raw original folder
2026-10-11-historical-access-af7dbdd3c/run-c6ec3e467/v016-access-boundary-before-v1
in the tiny-file worktree and separate readonly listener diagnostics are
unchanged. Receipt container is unavailable because startup never emitted its
normal ready event; the explicit original create acknowledgement still names
exact custody. No mount, successful reader, clean End or KnownStop is inferred.
The otherfive original selections remain unrun at that identity.

Source confirms the product guard is correct. Historical installer had
chowned global directory/file501:20 and preserved source0644, while root
Sandbox daemon requires its private directory owned by effective UID and
protected file0600. The installer now explicitly uses0:0, preserves exact
Store bytes and fixture UID/GID, sets backing directory0700/file0600 and
checks actual root/mode stat output plus Store SHA before declaring PASS.
Wrong ownership or mode refuses before passing container removal. The change
adds no product feature, retry, Store write, fsync, Init or measured pre-read.

Explicit source reuse admission now additionally excludes the reviewed
install_closed_history_store access adapter from producer AST comparison:
that function installs the independent historical access master, and is
never invoked by the complete setup producer. Every original producer
function/model/helper/workload/compiled file still matches; existing summary,
raw Store/WAL, graph, ACKs, source identity and custody remain untouched.
The six old prepared inputs/masters cannot be repaired in place. Prepare six
new copies at this changed source via explicit --reuse-producer, each using
its same original full closed producer. Sample the affected first case once
at new identity, then the still-unattempted five once. Rebuild is unnecessary
because all nine compiled scopes are unchanged.

Parent installer regression retained. Final39 covering checks PASS in
6,072,853,750 ns, and additional wrong-directory-owner, wrong-file-owner and
file0644 readback refusal check PASS in336,415,542 ns, each explicit120 s
R4 stop. A test-selector authoring error invoked zero tests and is preserved;
only the selector was corrected before the actual red. Raw qualification
2026-10-11-historical-backing-ownership-regression, SHA-256
cd1f5e61660f8f58b01fac04bc1fdede7c91c6d0df9a0dac6f973522fe05d05e. These are mocked external harness checks; no actual new install,
SDK access or Docker operation has yet qualified the correction.

Concurrent branch/multi evidence now also includes actual typed SDK refusals
Busy at Commit Begin with detail Store busy before effect. These agree with
current integration3.3, including original no-effect serial reservation and
Save Begin refusal. The unique explicit EAGAIN mapping is Declined::Contended
on a creating mutation with no locally reserved serial; early refill Busy
keeps its serial. Branchfirstpath inference is d039/f00000, and actual B
failure overlaps A command1 but no A Commit; local session try_lock versus
SQLite BEGIN contention remains unknown. The retained syscall wrapper did
not capture syscall/path. No unknown cause is promoted to a precise assertion,
no product defect or fix is claimed, and each remaining original case runs
once. Capped source diagnosis SHA7e1ddc674 is in its raw family folder;
namespace first-case readonly daemon log SHA-256
0c93d71c13538fd554deb7e5ad5a491b7ad2bd14196571dec10894a309bac43e
contains540 observation rows and only two EOF connection diagnostics.

The formatter must separately avoid promoting partial-context initializer0
phase totals to measured durations. Root owns that reporting correction;
original failed receipts will remain unchanged. No new source is sent to
actual cases merely for formatting. Durable remains disabled.

| `68209bdb7` | Correct private historical backing ownership/mode without changing producer or product | 79370 | 79370 | +0 |


### Batch 5 incomplete context phase-total reporting correction, 2026-10-11

Actual branch/multi concurrent failures have initialized phase totals even
though raw streams contain successful peer commands/Commit ACKs. No complete
round sum was published. The CSV reader now withholds all four aggregate
phase totals when an actual context case is FAIL and has no integer complete
work wall; both CSV tags explain that partial runtime spans remain diagnostic
raw evidence. Original status, failure, available counts/storage, source and
receipts are unchanged. A completed workload with later proof failure retains
its exact measured totals; completed compact and other routes keep legitimate
zero values. Administrative NOT_RUN rows acquire no partial-work claim.

Four new external report checks fail at the parent for the incomplete totals.
Final13 report/proof-only/custody checks PASS in343,651,667 ns under120 s R4.
Read-only qualification loads each sealed actual branch/multi retained run,
checks failed totals unavailable and completed compact phases exact, and
confirms original receipt hashes unchanged. No product, Store, Docker,
preparation, measurement or runtime operation was replayed. Raw qualification
2026-10-11-incomplete-context-report-regression, SHA-256
3faaeea55b0225c3e252d13c30e58d26c07f825a02b07f9c1dc400da79be43b6. Workers can import only this reader and its owning tests to generate
their original-source CSV pairs without changing or replaying a case.

Branch500/K100 now has an explicit administrative unrun closure in its fresh,
never-invoked run case folder: schemafs-bench-pro-administrative-unrun-case-v1,
zero operation/sample/proof attempts, original case/source/product and exact
failed120s preparation reason. It binds the original65f1881d supervisor
closure and5a48a44b aggregate parity without a timer/count/storage/custody
claim. Receipt SHA-256986f24219040950a1dd2af710de07dffd21dcb4e7161fdfa816f237ac2d3faf9;
manifest648d37c227be3c57ff1b92bfa6304cc9a00b904899eba77af3640a953347a085.
All originals remain immutable. Retained verify observes two passing compact
proofs, three actual failures with proof NOT_RUN and this unrun case, returning
exit1 under120 s; no product operation ran.

Allfive original multi cases are now attempted once atafbbd6c03: fiveFAIL,
proof NOT_RUN and no End/Gone/KnownStop. There are210 actual partial Commit
ACKs across peers; no completed totals, independent proof or clean custody is
inferred. Final raw handoff SHA-256
ea36449055aea0715c459abd9d36532c19aabe4543521b346b12fc942bfcac31,
manifest6748917d2bc0260b27f014056adfc6bcc4c6b56c3a537b7aa5d5cbbed0f49b93,
partial-eventsdbb1c7850dfb155296a5c66010241b2168af9abc5079d04fda62420dfec6b0da
are in the namespace worktree's2026-10-11-batch-5-multi-afbbd6c03/final-raw-handoff.
Fivefailed containers/volumes and twelve measured keys remain retained.
The independent host-group fence PermissionError stays visible and proves
no remote cancellation or filesystem drain.

Branch final raw results are three actual concurrent FAIL, two compact
functional/proof/clean-close PASS with numericINELIGIBLE, and one failed-setup
actualNOT_RUN. Compact convergent30ACK/15distinct and descendant40ACK/35distinct
pass full native/public/closed unfiltered graph proofs and known terminal
custody; own passing sample pairs are removed through authenticated disposal.
Full final family pairs and exact case/resource metrics remain to integrate.
Historical H23 fresh preparations are in progress atc79eb607f; actual new
accesses await allsix unchanged parity checks. Batch6/7 stay pending.

| `669bc4853` | Withhold uncompleted context phase totals without altering retained outcomes | 79370 | 79370 | +0 |


### Batch 5 historical pre-fix failed cohort retained, 2026-10-11

Imported exactly the worker54dee04c6 two CSV files at
baselines/core/2026-10-11-c6ec3e467. Each has allsix original access IDs.
Performance fields are N/A by the frozen verification-only contract.
Verification contains original boundary-before FAIL, diagnostic complete
proof-attempt wall2,896,690,000 ns, sample0/proofattempt1, and the otherfive
NOT_RUN because the new installer cause was diagnosed before invoking them.
No mount, reader, Commit, native proof, cleanup or KnownStop is credited.
Original overall FAIL and full exact listener error remain in both tags.

Pair SHA-256: performance2dd2de54e58afdd10aa2e64ff8f5f40f94e0220d23e3cf892a96226d8fe3bb19;
verification74e3002b6f8770c3b769f6185436dee71350806a2b6822e75b1de44e87f298f4.
Owning initial-failure-handoff.json SHA-256
4cd214d6ccb2951240bca4a0fc8f9bbf14ffb68373ae2a37391e17235e38427b
in the tiny-file worktree2026-10-11-historical-access-af7dbdd3c folder binds
every preparation, four complete original producer graphs, source/build,
failed runtime startup, separate read-only diagnosis and exact retained
container4891bb1c3fa69f0103c187219108c4ff2dab86c48fa972c925d5f0787bb66a37
and volumelayerfs-fsb-sample-1dc1bc4c39e947b6. Older prepared inputs/masters
remain unchanged and are not promoted by H23.

Worker preparation/actual execution source atc6ec3e467 is preserved;
reporting commit54dee04c6 and this import add no source/product behavior.
H23 correction68209bdb7 is synced to privatec79eb607f; six new independent
private-backed masters are prepared via exact closed setup reuse. New cohort
is separate and still pending actual results. The failed pair is history,
never overwritten or relabeled. No batch closure is claimed at this commit.

| `54dee04c6` (worker) | Retain historical failed startup and five unrun selections | 79370 | 79370 | +0 |
| `f810a54b7` | Import exact historical pre-fix failed-cohort CSV pair | 79370 | 79370 | +0 |


### Batch 5 original historical topology serialization correction, 2026-10-11

At clean privatec79eb607f, firstfour historical selections pass full original
reader/verifier and typed retained-root/global graph checks, End/Gone/Stop,
guarded outer publication and passing-pair removal. They remain unaffected
proof evidence and are never resampled. Fork-point then fails after successful
reads and known terminal custody: native public verify-access returns PASS
for exactTrunk5 and allfive Branch bindings, but Python strict complete ACK
transcript reports an order mismatch. Divergent-head is not attempted at that
identity. The failed stopped container3be01f6f3ca00b08e5f73ffbabce3ad485d39139a1025b64ef6e06d48713d2d2
and volumelayerfs-fsb-sample-31c5ae8a461041fc remain retained.

Both wanted and actual contain30 full five-field ACK tuples,15 distinct IDs,
with equal complete multiplicities. Wanted first is Trunk1 CID
120a0dd3311b61c054f3d99b6bdf4da3fa14f99ad4c5391e9b451fa94e051b6d4a,
parentNone, root9f41fbea522dadb9f4f59116344edbf90003763b218a95a0318ed8d4356b411f.
Actual first was A1 CID128b9d29bbb3144bbf534d340e57a3403ad3ad07764877cab484cb7b6e56c7758d,
parentTrunk5/rootfe20246a19fc69cc5dc3d011bcb0226146deaca50aa639c12bc34882ee07bf1b.
JSON sorting changed Branch insertion order to a,b,initial,trunk,access;
original producer execution/ACK order is initial,trunk,a,b followed by access.
No tuple, root, parent, head or unusedinitial Branch error was found.

Only the access proof serializer now reconstructs exact declared producer
role order with accesslast, rejecting any missing/extra Branch before emitting
the transcript. The generic transcript grammar and complete ordered tuple,
ancestry, duplicate-ID, typed provider, manifest and unfiltered SQL checks
remain unchanged. No set comparison, truncation, schema change, product fix,
additional workload or budget change is introduced.

Prepared dependency validation now reads its original/current committed Git
source blobs and verifies both declared hashes, allowing a stored preparation
to be checked after a later adapter fix without rewriting its identity.
Producer scope remains exact. Explicitly reviewed non-producer exceptions are
access_graph_proof and reportresult_rows plus the already recorded H21 mode
label and access fixture/install adapters; all other producer controller and
receipt functions must be AST-identical, with compiled/model/workload files
byte-identical. This admits existing correct six H23 inputs without regenerating
or relabeling them. A changed producer function or substituted source blob
is refused. Actual case execution still records its fresh clean current source.

Parent sorted-JSON controller regression fails for two compact producers.
Final42 owning checks PASS in5,431,777,500 ns/120 s mainR4, including fifteen
linear/compact/fork/head/selected-byte/extra-Branch variants and committed
source snapshot admission checks. A read-only retained-evidence check confirms
the original transcript fails and exact reordered30ACK/15distinct/fiveBranch
transcript passes the unchanged strict oracle against the actual native
provider. No Store, Docker, SDK, preparation or sample operation ran there.
Qualification folder2026-10-11-historical-transcript-order-regression;
SHA-2567614aed6b7b8b0dd25877cf1e004029ef0debb79d4335bd6dba758441d057a53. All nine compiled scopes are unchanged.

Worker434dc0a3c retained its original six-row H23 cohort pair: fourPASS,
oneFAIL and oneNOT_RUN. Original proof-attempt diagnostic walls in ns are
4,501,832,209;5,011,988,667;5,067,557,042;5,093,394,291;5,171,643,000;
lastUNAVAILABLE. No declared25s limit exceeded. Numeric fields remain N/A,
raw cacheINELIGIBLE. Raw order-failure-handoff.json SHA-256
0e48c32a64eb774d6eb0da791232e6c6f38292a4681385437f1cc9bf305c9f11
binds the fixedinput cohort/source/masters and all outcomes. Keep that pair
unchanged, rerun only the affected fork-point once at this changed source and
attempt divergent-head once. Reuse the unaffected firstfour proof receipts.
No batch6/7 starts yet; concurrent count-mapper omission is being tested in
its owning separate worktree without a measurement replay.

| `2f7160c92` | Preserve original ordered historical ACKs through JSON-sealed graph reuse | 79370 | 79370 | +0 |


### Batch 5 branch_development final family results, 2026-10-11

Imported exact worker222e0cd5d CSV pair atbaselines/core/2026-10-11-1bc68451d.
Allsix original cases remain: three actual concurrent FAIL, one failed-setup
actualNOT_RUN, two compact functional/proof/clean-close PASS with numeric
INELIGIBLE. Performance/verification SHA-256
b1c199edb2fea526115cc7c967e84af7a107da22cd71670bc73563f4ba124f05 /
93537b68a8ea18f034d2f8c451bf530965c2a4c322c225b2809b7f3f0d8a6312.
Execution source1bc68451d and product9ac88edb2cc44d44edd4a53f9ad89f4dee063a58
are unchanged; report-only private0754e1547 imports exact669bc4853 reader.
No case, setup, product operation or receipt is repeated or relabeled.
Original parity remains aggregateFAIL with five eligiblePASS and failed500/K100
setup absence, under committed section9 exceptionc23ac0cfd.

| Original branch case | Actual outcome | Outer invocation diagnostic ns | Complete command+Commit ns | Separate proof ns | Actual ACKs Trunk/A/B/C | Closure |
| --- | --- | ---: | ---: | ---: | --- | --- |
| v016-branch-mixed-100mb-5000-k10-v1 | FAIL | 10162822166 | UNAVAILABLE | UNAVAILABLE | 10/10/0/0 | UNAVAILABLE / UNAVAILABLE |
| v016-branch-mixed-100mb-5000-k100-v1 | FAIL | 23748204292 | UNAVAILABLE | UNAVAILABLE | 10/100/0/0 | UNAVAILABLE / UNAVAILABLE |
| v016-branch-mixed-500mb-30000-k10-v1 | FAIL | 26793157458 | UNAVAILABLE | UNAVAILABLE | 10/0/10/0 | UNAVAILABLE / UNAVAILABLE |
| v016-branch-mixed-500mb-30000-k100-v1 | NOT_RUN | UNAVAILABLE | UNAVAILABLE | UNAVAILABLE | 0 actual case attempts | NOT_RUN / NOT_RUN |
| v016-branch-convergent-content-v1 | INELIGIBLE | 8816724958 | 1391780251 | 2065161417 | 10/10/10/0 | Gone / KNOWN_STOP |
| v016-branch-fork-descendant-v1 | INELIGIBLE | 9407897125 | 1878857337 | 2184426209 | 10/10/10/10 | Gone / KNOWN_STOP |

Outer invocation walls include setup/driver and are never substituted for the
original completed pure-call clock. Failed prefix phases remain raw diagnostics;
all incomplete aggregate CSV phase totals are UNAVAILABLE. Compact convergent
and descendant are within15s reported and25s verification limits, with no
completed limit exceeded. Unknown setup/own-write coldness, paid preconditions
and declared shared host/VM/protected/failed-service interference retain cache
INELIGIBLE. No speed, reference ratio or acceptance verdict is made.

Convergent storage logical/allocated B: Store4,655,120/4,657,152,
Overlay229,376/268,664,832; descendant Store6,138,320/6,139,904,
Overlay229,376/268,664,832. These are the original observer scopes.
Convergent complete graph has30ACK/15distinctCommit tuples/fourBranches;
descendant40ACK/35distinct/fiveBranches. Exact declared selected roots have
native full-payload, public current typed-root/history and complete unfiltered
closed SQL census proofs, one final authenticated control fence and clean
End/Gone/KnownStop. Both own passing pairs were removed after publication/outer
ACK. Original numeric counts remain UNAVAILABLE in this pair because the
mapper rejected genuine empty ordinary-command ranges; that separate source
bug is being qualified offline, without changing these closed records.

The three concurrent failures agree with required before-effect contention
refusals:100/K10 and100/K100 Binitial creating-mutation EAGAIN,500/K10
ACommit Begin typedBusy. They do not reach full original graph, native M1
proof, public census or whole End/Stop. Trunk alone ended cleanly; child
mount custody remains uncertain. Keep every failed container/volume/key:

| Case | Retained container | Retained volume | Child keys A / B |
| --- | --- | --- | --- |
| v016-branch-mixed-100mb-5000-k10-v1 | a2bd5be44af29eed2d22f6094907e9895f9874326552bacbea342709b0690f32 | layerfs-fsb-sample-bf467b65bfc44fd6 | ee4409af92dde503e1b3a36b99a227c38f5a8d56286132c49ed3ecf831a38f9d / 4efbba7a0e9a926cc3c122e9bf3d034b322adff44938cfc3f189df069e11bf1f |
| v016-branch-mixed-100mb-5000-k100-v1 | df382f92fbf74c30444cce63c785aa71a10fc760a262ad1d61a70296bb7d57b8 | layerfs-fsb-sample-838f42e9bc614e3c | c87645e138eb2517f48ce748e482ec3730a92318ed4ebc3942c429e86f4da886 / 58adaad6df16e8d3d5900515e8046131280e00252bf65dec419620a723ddb1bf |
| v016-branch-mixed-500mb-30000-k10-v1 | 6c8186e0744503a58718bedbb88783f5054ae171a1c333d7abb731247f0c1a2c | layerfs-fsb-sample-1f9422d10aed4c6c | 08717f4ca8b19319e9b1963fff3385bf8a801ef94b9fff7900ad1f4729d64591 / d303093216d1dd9ad8863ff307edd8ab33ce743ba87d435a410829a6fb714b8b |

The three original immutable masters remain for rollout reuse. Failed500/K100
preparation and its late sealed artifacts stay unqualified; its separate
administrativeNOT_RUN row binds exact original120s failure and zero actual
case attempts. Neither a changed limit nor a retry/workload/checkpoint reduction
is used. Full family-handoff-final.json SHA-256
6fc4858eda77e401bdd39f46fcb2a37a82a823454eaf120a5cf19fa9e76b6e29
in edit-capped worktree raw2026-10-11-batch-5-branch-1bc68451d binds all
metrics/limits/resources/seals/omissions. Read-only count diagnosis SHA-256
4acd2c0d65f8a67292b6d01edda87b6e6cb38e33e0477d397cc2cc8465f21479
identifies only30/40 authenticated empty command ranges in original pre-proof
prefixes, with complete genuine observer groups; counts are not invented.

| `0754e1547` (worker) | Import reviewed incomplete context reporting only | 79370 | 79370 | +0 |
| `222e0cd5d` (worker) | Commit exact six-case branch CSV pair | 79370 | 79370 | +0 |
| `305bee680` | Import final branch family result pair and exact retained custody | 79370 | 79370 | +0 |


### Batch 5 multi_workspace_development final family results, 2026-10-11

Imported exact worker087191e1 twoCSV pair under
baselines/core/2026-10-11-afbbd6c03. Allfive original cases are attempted once,
FAIL, with proof NOT_RUN and incomplete whole-case/phase/count/storage totals
UNAVAILABLE. Original error and partial-events diagnostic qualification remain
in both tags. Pair SHA-256 performance
4eb85ca05e66e8d7f2bc3248e0e967d966e6727a112065cd5e000d1e7b47d9c8,
verification0de829c7d8c1e4821b707c5d9697604f3545434d8f0eb239e87fb4fc996822d2.
Actual sourceafbbd6c03/product9ac88edb2cc44d44edd4a53f9ad89f4dee063a58
remain; private report-onlya4756a0e6 imports reviewed669bc4853 without replay.
Five original preparations and parity PASS, both immutable masters retained.

| Original multi case | Outer invocation diagnostic ns | Actual partial ACKs by worker | Actual failure | Independent proof / terminal closure |
| --- | ---: | --- | --- | --- |
| v016-workspace-mixed-100mb-5000-k10-v1 | 7215536209 | 0:5/1:0 | worker1 initialStage1 EAGAIN | NOT_RUN / UNAVAILABLE |
| v016-workspace-mixed-100mb-5000-k100-v1 | 13574872583 | 0:50/1:0 | Commit Begin typedBusy before effect | NOT_RUN / UNAVAILABLE |
| v016-workspace-mixed-500mb-30000-k10-v1 | 21197207500 | 0:0/1:5 | Commit Begin typedBusy before effect | NOT_RUN / UNAVAILABLE |
| v016-workspace-mixed-500mb-30000-k100-v1 | 30511851000 | 0:50/1:0 | worker1 initialStage1 EAGAIN | NOT_RUN / UNAVAILABLE |
| v016-workspace-four-100mb-5000-k100-v1 | 21842672125 | 0:0/1:0/2:0/3:100 | workers0/2 initialStage1 EAGAIN; worker1 successful first command but no specific Commit outcome recorded | NOT_RUN / UNAVAILABLE |

Allfive exit1 before their unchanged120 s outer ceiling; none expire. These
outer clocks include setup/driver and cannot substitute for a completed
original performance operation. There are210 actual partial Commit ACKs;
zero native/public/global proof invocations. The intended witness, discard,
same-Branch fresh reopen and full final graphs are not reached. Neither proof
PASS nor strict M1 metadata failure is inferred. Current global serial/Save
contention contracts require definite Busy refusal before effect, with no
retry/writer gate; no source-backed correctness defect is established.
Precise syscall/path and underlying lock origin for errno11 remain inferred
or unknown. No limit/workload/barrier/worker/cache alteration was selected.

Keep each failed own container/volume and exact measured mount keys:

| Case | Retained container | Retained sample volume | Measured worker keys |
| --- | --- | --- | --- |
| v016-workspace-mixed-100mb-5000-k10-v1 | 3780b2054cfabfb86123dba2989c095d062386f4ec75fb47a1771958ae79ea3d | layerfs-fsb-sample-76010d7922af4d53 | 0:3699a8467622cfecb722b58a129c85d9015664296a261012817e4b1b59667216 / 1:933e2ea75e5e74e43027c1313dbfa9034c2ad98d46bba2bc25561c421c549a9f |
| v016-workspace-mixed-100mb-5000-k100-v1 | 4dca45fee617db7473cea8b6998874abfcd061a27f6ff109ed657ab42e5425e1 | layerfs-fsb-sample-818d123eed3741d9 | 0:7d19f501ddd28b249ce765ebdeb3ee4ab99196521925920942c21c24ad354ac2 / 1:20aef8a527c59f44071b0cf85cf7452dfa20ec72c671d9bcce2248d521909b58 |
| v016-workspace-mixed-500mb-30000-k10-v1 | 4297330f87ef77b6da40557741e9b652d60e63f9d895dc41b632a63a2c0aeb87 | layerfs-fsb-sample-f8c24053bd2e4c79 | 0:68a7b1929f91cdbf7200ee8128d8bf882d1ccb26d4689c55bb8963184e4aa350 / 1:8691b3f5ea76c5fe76d982794bf239a15df663088376e03fec16fc6f33a6cc9f |
| v016-workspace-mixed-500mb-30000-k100-v1 | 8b5e66ae444e50a1410c2966999934abf15235ab49d1408a62f83d357c36a327 | layerfs-fsb-sample-f0f93c61e58c4fef | 0:57d3656f56899c5b34f028fe82d375bcbae5a4dba1cb71fd3173da8916c40605 / 1:aaacd2644a4c47ee972a804a921a5686c9ae648c68ee87ee5a7c674a094974d3 |
| v016-workspace-four-100mb-5000-k100-v1 | e250f4d234ec54e276557be318f5a2341f660b946e8aff507d750cd6a1969cf4 | layerfs-fsb-sample-4a44f3d9692443cd | 0:cfc78cf99fafd28b811ef334c18ec17582dc726d115c9f67aa7e40a6111107f6 / 1:ab2a7271515b61fa4a8b3c338131943b8fa0b6187eeaeae8b8e920e3a890cdee / 2:2bf78c90ebfff702257d91084db41c5fffc81e462663ebc9e1899f561332ad0a / 3:bb8844497fd255f3a7a8587625fcf09091cd87694c9b9346c1b3cc303be3a68f |

Allfive failed pairs and twelve measured mount keys remain retained.
No End/Gone, channel fence, KnownStop or removal is established. Independent
host-group fence PermissionError remains in raw custody and proves no remote
stop or drain. Do not delete these pairs or any owner-protected resource.

Final family closure SHA-256
87b2eea184e412b6d845c2ee0ed6a90c63027837b3856e013f3043e1f1bfad7f;
manifest50c37f8fdfb35910fde24c92ce35e8a2d272662626cc8d12a7ef4524d2ee3cd0
in namespace worktree2026-10-11-batch-5-multi-reporting-669bc4853/family-closure
binds case/source/image/build/prepared/parity, resources, diagnostic spans,
all210 partialACKs, report and exactLOC records. All23 original receipt/prep/
registration/parity hashes remained identical before/after report, which ran
once in277,726,166 ns. No source/product/build/prep/sample operation followed
these failed attempts. Every original case is present; no later batch began.

| `a4756a0e6` (worker) | Import exact reviewed report reader only | 79370 | 79370 | +0 |
| `087191e1` (worker) | Commit original-source five-case multi pair | 79370 | 79370 | +0 |
| `cb3a003ac` | Import all multi-workspace failures and exact retained custody | 79370 | 79370 | +0 |


### Batch 5 authenticated empty command ranges and offline count supplement, 2026-10-11

Source-backed read-only diagnosis found the first ordinary command at sequence16,
Trunk1/channel0 had exact send4→4 and receive4→4. Ordinary Sandbox commands
send no filesystem Control request; treating their authenticated empty range
as an unclassified positive range made the completed compact numeric mapper
stop. The original pre-proof capture contains45/60 genuine numeric groups,
plus30/40 exact empty command ranges, with complete observation-cost flags.
No positive Control range is missing and no later proof/fence frame is included.

Integrated only workerdf901bfd0 context_numeric_counts and owning external
count tests. A pre/post AST comparison confirms all other workspace functions,
including the H25 access serializer, remain unchanged. Ordinary empty ranges
are explicitly correlated with same authenticated scope/channel, exact integer
and paired unchanged send/receive counters and empty numeric identities.
Advancing, foreign, missing and unpaired ranges remain refused. All existing
positive-range/frame/window/observer-cost/terminal-owner guards stay intact.
No zero counter is invented. Parent red and worker finalsix checks are retained;
integrated finalsix guards PASS under120 s mainR4, no native/product/Docker,
setup, sample or resource operation.

A separate authenticated read-only supplement qualifies the original capture
prefixes, without changing the original UNAVAILABLE receipts/CSV/cache status:

| Original compact case | Requests | Owner jobs | Statements | Captured last event / observed call | Native terminal owners |
| --- | ---: | ---: | ---: | --- | --- |
| v016-branch-convergent-content-v1 | 162 | 2844 | 12628 | 274 / 49 | 1,90,172; each54requests |
| v016-branch-fork-descendant-v1 | 216 | 3806 | 16873 | 363 / 65 | 1,90,172,258; each54requests |

Original manifest, runtime and daemon frame-set equality, complete observer
cost and distinct terminal connections pass. Raw receipt/runtime/daemon hashes
remain identical, and later verification/ControlEndSession events stay outside
the declared prefix. Numeric cache eligibility is still INELIGIBLE. This is
an offline count-availability supplement, not a new timing/sample or speed
claim. Owned raw compact-counts-qualified-supplement/supplement.json SHA-256
5546b4ebdeccd9381ae2a8a9b695e781d920a7b5c844fb64f5b44d3022fb2b83;
count-mapper-final-handoff.json0d8ff16569b77b7aeb02a22621fb4b2d64023a0811a7a22859ddca5282e23ba3
in edit-capped2026-10-11-batch-5-branch-1bc68451d binds all original/final
source, tests, qualified arithmetic, immutable raw input and exact LOC.
Original family CSVs remain historical; the supplement supplies their exact
otherwise unavailable completed-case counts. Root integration checks live in
2026-10-11-context-count-mapper-integration.

All actual batch5 execution is now closed; final historical pair/composite
handoff remains to import before the batch record can close. Batch6/7 have
not begun. No new product or workload feature is added; current global writer
Busy semantics, strict metadata oracle failures and all failed/uncertain
resources remain. Durable execution remains disabled.

| `df901bfd0` (worker) | Correlate exact empty command Control ranges; qualify separate retained-count supplement | 79370 | 79370 | +0 |
| `616947cb6` | Integrate isolated count mapper without changing historical execution or receipts | 79370 | 79370 | +0 |


### Batch 5 historical_access final family results, 2026-10-11

Imported exact worker434dc0a3c and6d3eecb3c pairs at
baselines/core/2026-10-11-c79eb607f and2026-10-11-5cea1710d. Each preserves
allsix original IDs. The first pair has fourPASS, oneFAIL and oneNOT_RUN;
the latter has two actualPASS and fourNOT_RUN because those four qualified
prior passes are reused unchanged. Original pre-H23 c6ec3e467 pair remains.
No allsix-at-new-source execution is claimed and no passing proof is replayed.
Composite authoritative family result is six functional/proof/clean-closePASS,
performance samples0 and one authoritative proof attempt percase. Two earlier
failed attempts remain in history at their exact source identities.

| Original access case | Authoritative source | Selected role/local | Separate complete proof-attempt ns / 25s | Both full read bytes | Full global Commit rows / ACKs / Branches | Closure |
| --- | --- | --- | ---: | ---: | --- | --- |
| v016-access-boundary-before-v1 | c79eb607f | main/48 | 4501832209 | 262142 | 100/100/2 | Gone / KnownStop |
| v016-access-boundary-after-v1 | c79eb607f | main/49 | 5011988667 | 262144 | 100/100/2 | Gone / KnownStop |
| v016-access-inode-before-v1 | c79eb607f | main/94 | 5067557042 | 16384 | 100/100/2 | Gone / KnownStop |
| v016-access-inode-after-v1 | c79eb607f | main/95 | 5093394291 | 8192 | 100/100/2 | Gone / KnownStop |
| v016-access-fork-point-v1 | 5cea1710d | trunk/5 | 4651186917 | 131072 | 15/30/5 | Gone / KnownStop |
| v016-access-divergent-head-v1 | 5cea1710d | b/10 | 4818232875 | 131072 | 35/40/6 | Gone / KnownStop |

All diagnostic proof-attempt walls remain within original25 s bound; no
verifier limit exceeded. Every access uses one fresh mount, original reader
then independent verifier, complete full bytes, declared alias/absence/inode
relations, public selected-root/fork/source-head binding, complete typed
producer ancestry and unfiltered closed SQL census including new zero-ACK
access Branch. Each has original End/Gone, authenticated fence, known runtime
return/Stop, guarded publication and outer acknowledgement. Allsix own passing
pairs were removed via separately bound authenticated administration.

Performance fields are N/A by original verification-only contract. Raw cache
INELIGIBLE and summary requests/ownerjobs/statements UNAVAILABLE remain;
source diagnostic arrays are retained and no positive numeric/peak/cold claim
is made. Storage values use exact original external snapshot scope, including
explicitly observed Store/Overlay sidecars, not an independent residency proof:

| Access case | Store logical / allocated B | Overlay logical / allocated B | Authoritative receipt SHA-256 |
| --- | --- | --- | --- |
| v016-access-boundary-before-v1 | 3084392/3088384 | 229376/268664832 | d6b74d6b163ec178ca3e645265f75fd597c33a5b38c1011e9d1ee38dfbee0e1e |
| v016-access-boundary-after-v1 | 3084392/3088384 | 229376/268664832 | 74ff776b5a44bbac86ba7bbcc9b5413d5cb6b61f3c46b226d8433c4a93df46e0 |
| v016-access-inode-before-v1 | 3534952/3538944 | 229376/268664832 | a3f7ff69688e4d28bfa057faabc9310963a59eb4aa4e94ae7dc1b9349fddc2d5 |
| v016-access-inode-after-v1 | 3534952/3538944 | 229376/268664832 | 3f1b7ff06545b24308d2219fb573779be02d3c1dff36bf2671e4e6baddcbe09c |
| v016-access-fork-point-v1 | 3129448/3133440 | 229376/268664832 | 66382e31c67ee51ca747618ac811aa4f5eb954875ed7a62793fb90b109b044e6 |
| v016-access-divergent-head-v1 | 3461224/3465216 | 229376/268664832 | 1bfa60235da313f88ebe46de8a564742df297f597bff5fe8936d81812211edef |

Four complete setup-only producer executions and zero producer replays supply
all original selections. Boundary and inode each retain100ACKs/101reference
roots; convergent30ACK/15distinct, descendant40ACK/35distinct. All graph
forks/parents/heads and retained histories stay complete. Exact independent
input byte copies and case-specific selected native manifests are preserved.
Firstfour c79 passes are unaffected by the compact transcript ordering fix.
The affected fork-point runs once at5cea and divergent runs for the firsttime;
no unchanged repeat or best-of is used.

Pair SHA-256 performance/verification:
c79:33c0c4760a6bc5682e0040fc46a9970e2373fa6efdfc1f4a06030ca73a82deb9 /
e7e35b5d8ea94b83d5100c819562646991d1b937f6d00bc41cb3e17d59350c51;
5cea:dea395298e5fac38de025091f4aa4cc444393d5dc71c1d4e192a1c9b6f981836 /
686bfa8039f3e29251ba15c130f61327ee79846719058018fa40067397d78d92.
Final COMPLETION-HANDOFF.json SHA-256
161a09495a50213f86ce5ad3c550e5fea5835054f6fcbdd90133937fb1ef257a
in tiny-file worktree2026-10-11-historical-access-h25-fixed binds each case's
actual source/receipt/master/product/image/build/profile/selected tuple,
producer closure, limits, storage/count scope, original failures and removal.
Current Docker existence of retained masters was not separately re-inspected;
exact creation/preparation custody is retained, not a present-state claim.

Keep historical failed own pairs unchanged: H23 startup container
4891bb1c3fa69f0103c187219108c4ff2dab86c48fa972c925d5f0787bb66a37 /
layerfs-fsb-sample-1dc1bc4c39e947b6; H25 stopped proof-failure container
3be01f6f3ca00b08e5f73ffbabce3ad485d39139a1025b64ef6e06d48713d2d2 /
layerfs-fsb-sample-31c5ae8a461041fc. Sixteen input masters remain under exact
custody for rollout; old H23 placements are failed-input evidence and are
never silently repaired. No product change or Durable execution occurred.

| `c79eb607f` (worker) | Sync reviewed private historical installer and qualifying source | 79370 | 79370 | +0 |
| `434dc0a3c` (worker) | Retain four access passes, ordered-transcript failure and unrun case | 79370 | 79370 | +0 |
| `5cea1710d` (worker) | Sync strict original-order access serializer/source admission | 79370 | 79370 | +0 |
| `6d3eecb3c` (worker) | Retain two actual final access passes and four reused earlier proofs | 79370 | 79370 | +0 |
| `b0b2cd386` | Import final historical access pairs and six-case authoritative closure | 79370 | 79370 | +0 |


### Batch 5 closure and batch 6 release, 2026-10-11

Allthree ordered original families are now truthfully closed with their full
result pairs in main. Of17 original cases, eight have complete functional,
independent proof and clean terminal custody: two compact branches and six
verification-only historical accesses. Eight actual concurrent branch/multi
cases fail before proof with explicit source-selected contention refusals;
one branch500/K100 case stays NOT_RUN after failed120s preparation, with
aggregate branch parityFAIL. These remain failures, never passing done-when
claims. The section9 bounded-preparation exception permits onward closure.
No identified fixable product correctness defect remains in this batch.

Every original family definition, complete applicable parity row, exact source/
product/image/binary/helper/fixture binding, original attempt/case result,
separate proof/cleanup/cache verdict and twoCSV per recorded source is retained.
The public SDK Fork/History covering proof passed before sampling. Source
H20–H26 harness defects were fixed with external red and scoped final checks;
affected historical cases ran only at changed sources, while four unaffected
passing receipts were reused. Count-only H26 qualification reuses original
pre-proof telemetry without a measurement repeat or old-receipt edit.
Product stayed79,370LOC throughout batch5 and no product feature was added.

Allfive preparations/parity in multi pass, but allfive actual runs fail.
Branch has five eligible parityPASS, three actual failures/two compact passes,
and one explicit failed-setup/unrun row. Historical composite sixPASS retains
two prior failed attempts and their exact old-source unrun selections. Six
historical actual passes have zero performance samples, one authoritative
proof attempt each, original two-reader/fresh-mount/zero-Commit behavior and
complete typed producer history/global census. No complete batch5 limit was
exceeded; bounded setup expiry remains failed rather than a timed speed result.

Ten failed actual sample pairs remain retained across branch3, multi5 and
historical2, plus prior H20 failed preparation container/master. Passing compact
and historical sample pairs and passing setup producer/build containers were
removed only after qualified source/receipt custody, clean End/Gone/fences/
KnownStop and outer acknowledgements. Owned immutable setup masters remain
under explicit rollout custody; failure/uncertainty and all protected owner
resources are untouched. No cache, worker, workload, fixture, limit or watchdog
was changed to make a miss pass. Global Store is always Disposable/WAL/OFF,
overlayMEMORY/OFF/EXCLUSIVE, constructionworkers1, Initownprofile. Durable
execution remains NOT_RUN — disabled by owner until explicit reauthorization.

The latest closed original-family catalogue has19 ported families/191 cases;
remaining five batch6 families and reliability total82 pending cases. All25
original families/273 IDs remain, with no26th or P6 addition. Batch6 may now
begin in the same three authorized separate worktrees, with scoped ownership,
private locks/targets and shared host/VM/retained-service interference declared.
No build/test/measurement overlaps within a checkout; no extra worktree or
push/PR/tag/publication/GitHub change is selected. Only local main receives
integrated purpose commits. Final all25 iteration completion is still pending.

| `7ccb44cb2` | Close batch5 with all original dispositions and release batch6 | 79370 | 79370 | +0 |


### Batch 6 CDC registration and shared support, 2026-10-11

Decision58: retain the expressible original fresh verifier Fork/mount/full-native
check/End/Gone/KnownStop for both directory-import families. The initial worker
authoring proposed an offline-only substitute; the integration review corrects
that proposal before any sample. Independent public typed storage proof is an
additional required half, with current authenticated framing explicitly adapted.
Alternative: drop the original mounted half; rejected because public Fork/mount
already exists and the frozen oracle must not be weakened.

All21 CDC IDs are now ported as native-directory imports, not ordinary mutations.
Seed1 and the all3seed boundary fixture remain exact. Boundary has60 regular
files,26 directories,884745 logical bytes,54 current positive whole-file objects
and6 empty states. Original independent CDC shape and exact transcript guards
remain separate from actual persisted representation; sharing metrics are
diagnostic and carry no new threshold. Raw native/preparation/parity/actual
execution is still NOT_RUN pending shared support and a fresh qualified build.

Worker owned-only commit6727d7793e6306044bb5d23140eba80c38e8340a has counted
confirmed tree666d4197235f8c1463ef4dca2013c537fd6c4367, product9ac unchanged.
Four external authoring checks passed once at2,282,674,000ns under private120s
lock; no native build, preparation, parity or case ran. Sealed worker handoff
family-authoring-handoff.json SHA-256
bc6a43575a9db8892a8eaee4a9c483ea0c3b9f9b743014be2a8a033020da3203.
Integration corrects only proposed verifier topology/resource descriptions;
original measured import/fixture/oracle obligations and frozen source are exact.
Root final four definition checks PASS once at3,248,940,542ns under the120s lock; raw scoped-tests completion retained under2026-10-11-batch-6-main-authoring/cdc. No product or frozen source changed.

| Worker6727d7793 | Author CDC registrations/wrapper/external tests | 79370 | 79370 | +0 |
| `77ebaa34e` | Integrate CDC registration with original mounted oracle retained | 79370 | 79370 | +0 |


### Batch 6 cross-file registration, 2026-10-11

All10 original anchor/unique/identical/mixed cases are ported as fresh public
directory imports, seed1 and1MiB per file. Mixed10/100/500 retain7/75/375
distinct bases. Exact independently scanned transcripts, anchor uniqueness,
identical roots/ordered mappings and disjoint distinct-base payloads remain
mandatory. Decision58 retains the original fresh mounted native verifier,
plus the current authenticated public typed proof; the worker's proposed
offline-only difference/test assertion is corrected before any execution.
The external assertion now requires that expressible original topology.
No frozen guard, definition, limit, workload or expected state is weakened.

Worker9963494d0aa49def1fe743ef3d3422ee7026fb6b has counted/confirmed
treeea11915f45df899fff1c92e65a40070510dc7d7e and unchanged product9ac.
Six authoring checks passed once2,610,742,958ns under private120s lock.
Sealed AUTHORING-HANDOFF.json SHA-256
65a4a8f71f3fc8b2edf7ec5ab03603ae103d79ac5185e6a01714a22713e0b8ea.
No native build, preparation, parity, Store or case ran. Root six final checks PASS once3,025,226,417ns under120s lock, raw2026-10-11-batch-6-main-authoring/cross; shared import support/fresh qualified build remain prerequisites.

| Worker9963494d0 | Author cross-file registrations/wrapper/external tests | 79370 | 79370 | +0 |
| `63fffa85e` | Integrate cross-file registrations and complete mounted oracle | 79370 | 79370 | +0 |


### Batch 6 Workspace reuse registration, 2026-10-11

All14 original reuse cases are ported. Exact/local/unique additions remain the
unchanged full1MiB dense POSIX write, explicit post-write metadata and file/
directory sync, followed by one public Commit. No SDK-edit substitution or
new family is introduced. Compact tier1/10 bases and128-file controls remain
exact. Original pristine base structural attribution occurs outside timers;
new public storage hooks must bind it separately and verify final mapping
identity/zero-new-payload exact reuse, unique disjointness and local distinction.
No warmup workload/Commit is permitted by this family's frozen cache contract;
the shared controller must gate that exact schedule before any invocation.
Cache paid backing/metadata/SQLite/own-write observations remain INELIGIBLE.

Worker eefbd751c0d2fcbbcef723b00106ca5002f067d2 owns only the family/wrapper/
external test, counted/confirmed treebdc45313d6601536c1ce3e78a0b9756fe7e54461.
Seven authoring tests passed once2,226,215,458ns under private120s lock.
Sealed handoff-final/handoff.json SHA-256
a644af6a2e0adb8541b6cfd16b3f91db13940b38516b7376f4e5cb774efce6b5.
Exact owned bytes integrated unchanged; shared support, native build, prepare,
parity and all14cases remain NOT_RUN. Root final authoring check is FAIL: six pure checks passed and the first exact wrapper/fake-/bin/sh dispatch reached its unchanged2s child stop; complete invocation2,121,796,000ns, original receipt2026-10-11-batch-6-main-authoring/reuse. Wrapper is a finite case selector then exec of a three-line printf fake; no product operation ran and no source-backed implementation hang was identified. Scheduling/startup cause remains unknown under declared shared interference. Preserve the failed receipt; no timeout change or unchanged rerun. The worker's seven exact-source qualified checks remain separate earlier evidence, not a relabelled main PASS.

| Worker eefbd751c | Author reuse registrations/wrapper/external tests | 79370 | 79370 | +0 |
| `a442c8879` | Integrate original reuse family and required original storage scope | 79370 | 79370 | +0 |


### Batch 6 Store-footprint registration, 2026-10-11

Allsix original controls are ported with exact500MB large controls and5/10MB
low controls,100000/100/10 infrastructure tiers,100000/100/10 file geometry
and seed1. Original full native fixture/digest construction remains setup-only;
its public directory Init is measured and never replaced by a prepared Store.
One unchanged namespace-edit, one Created Commit and terminal End follow.
The final oracle preserves its intentionally clipped selected-file boundary
(65536B either side of the10B E000000001 marker) and exact full file length;
unselected final namespace/bytes remain explicitly omitted. Initial typed inode
census and actual global Commit membership strengthen identity only.

Original4GiB main Store,2GiB owned spool, Store/spool arithmetic,2CPU/2GiB/
noSwap/256PID/OOM and zero host swaps remain required with exact actual owner
scopes. No inaccessible retired counter is a fabricated zero/physical proof;
current raw observations and availability are separate from functional proof.
No performance/storage ratio or product feature is introduced. Five owning
external source/geometry/topology/limit/oracle/ordinary-dispatch checks PASS
once713,244,834ns under main120s lock, raw2026-10-11-batch-6-main-authoring/
footprint. Native preparation, initial+final SDK/FUSE run and storage accounting
qualification remain NOT_RUN pending shared/native support.

| `5505bff73` | Port six original fresh Init/edit Store-footprint controls | 79370 | 79370 | +0 |


### Batch 6 repository-history registration and corpus recipe, 2026-10-11

Allthree original selections remain exact: full157 states/4,936,693,030 selected
logical bytes; stride3 53/1,676,767,835; stride10 17/561,010,345 including157.
The existing frozen ordinary mounted importer/observer is callable. There is
no source-backed not-applicable capability condition; no product feature or
retired crate is restored. The active component history lane is not substituted
for the original SDK/FUSE/Commit/history/full-byte oracle and none of its
storage targets is adopted. Corpus manifest/tip/all157 metadata authenticate
read-only; real transition preparation, Git batch/blob execution and cases
remain NOT_RUN. Native observed output stays outside Disposable backing; its
frozen sync_all applies only to that separate output artifact.

The stdlib source-only recipe authenticates exact Git/tree/blob/oracle identity
and prepares selected-to-selected transitions. Strides do not reuse adjacent
checkpoint previous.tsv; the external fixture catches a change in a skipped
state. Full157 also authenticates complete cached-input receipts/membership.
Original14400s phase/300s state limits stay recorded, with owner actual120s
external/per-invocation ceiling enforced; no selection/time/cache reduction or
component threshold is selected. Decisions58–61 bind current topology, native
source setup and independent global membership.

Worker dd6fa4b7796f274eaed579cbc23b369c7242c4ae has counted/confirmed
tree434495727268c426eb0fe6f1e8fef3c9ba525db1, unchanged product9ac.
Five authoring checks PASS once1,905,532,375ns/private120s. Worker sealed
family-authoring-handoff.json SHA-256
8c9126e965d344e1e4224ec5fbbb30eb9864f0ec956f36c70aef11d5bc1e01aa.
Root five exact-source checks PASS once2,380,493,166ns/main120s, raw
2026-10-11-batch-6-main-authoring/repository. These tests mock Git batch I/O;
they are authoring checks, never real corpus or runtime qualification.

All24 batch1–6 families/245 original IDs are registered as ported; reliability
retains28 pending IDs. Total25families/273 IDs unchanged. All54 batch6 actual
cases remain NOT_RUN awaiting final shared support, clean source admission,
pinned nine-asset build, own preparation and exact family parity. Parallel
worktrees own separate native/import/footprint/repository components; source
and compiler/test failures remain retained and no speed work is selected.

| Worker dd6fa4b77 | Author repository selections/wrapper/native corpus recipe/tests | 79370 | 79370 | +0 |
| 55a323f2f | Integrate exact repository-history definitions and independent corpus recipe | 79370 | 79370 | +0 |


### Batch 6 native verification integration, 2026-10-11

This commit integrates the independent std-only CDC input/chunk scanner,
source census CLI, public Init ACK qualification, exact typed dedup/reuse
proof, original footprint clipped-boundary proof, and repository all-selected
ACK/Created-only proof. SDK mount/unmount receipts now retain the actual
Workspace incarnation and numeric namespace so a closed overlay observation
can authenticate all original owners. All code is harness scope; product
9ac88edb2cc44d44edd4a53f9ad89f4dee063a58 remains unchanged.

Worker evidence is reused at the exact imported implementation: Init five
checks PASS6,598,234,625ns/e8866e973; dedup five unaffected proofs reused plus
final genuine public Init/Save/Commit/ACK-tamper check PASS1,322,236,250ns/
44bbead833; footprint four final affected/new checks PASS7,758,521,666ns/
a39490f48 and four unaffected checks reused; repository two real public
C1/C5 proofs PASS1,179,039,208ns/8d06d11f9. These are synthetic independent
proof qualification, not actual family samples, FUSE results or timing claims.
Private pinned handoffs remain under their authoring roots, listed in the
preceding working ledger and subsequent shared support record.

Root native boundary CLI compile initially failed from a JSON format closing
brace; corrected compilation PASS4,923,825,875ns. First CLI tamper test used
an unpromised verification_status key and failed; the corrected test binds
actual dedup_transcript_status plus input transcript SHA and rejects native
byte tamper, PASS683,401,792ns. Root runtime all-test compilation initially
failed because census_fixture lacked its crate-root dedup_input declaration;
after that correction PASS8,638,524,500ns. Final integration added repository
proof wrapper and exposed the same missing declaration there: compiler FAIL,
then corrected all-test compilation PASS7,112,072,875ns. Raw failures are
unchanged under 2026-10-11-batch-6-import-engine-checks. Owned native format
check FAIL147,899,542ns only on new session receipt formatting, corrected
without changing operation or workload. Final formatted all-test compilation PASS10,349,770,625ns; five protocol_
checks PASS1,064,147,125ns with19 frozen inline tests filtered, main120s
outer/110s child stops. No test expiry or actual mounted case occurred.

Worker native authoring failures also stay visible: first dedup API compile
errors; WrongLogicalRole from a synthetic unsorted serial fixture; missing
inner decode/ACK binding; first footprint FileView PendingScope/include-doc
compile errors; incorrect synthetic initial NamespaceRef1 (actual initial0);
and footprint provider-after identity originally taken from the owner input
rather than the actual independent typed copy. Each is a harness/test error,
corrected in the above worker commits and qualified by unchanged strong
oracles plus affected external negative tests. No product fix or physical
resource claim resulted. Raw failed synthetic native roots remain retained:
/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/fsb-batch5-footprint-positive-12135
and fsb-batch5-footprint-corrupt-12135 at the same parent.

All54 actual batch6 cases still NOT_RUN. Next step is commit shared Python
machinery, seal nine assets from clean source, prepare/parity and sample once.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| Worker e8866e973 | Public import ACK proof | 79370 | 79370 | +0 |
| Worker 44bbead83 | Exact dedup/reuse public proof | 79370 | 79370 | +0 |
| Worker a39490f48 | Footprint actual typed provider/census binding | 79370 | 79370 | +0 |
| Worker 8d06d11f9 | Repository all-selected native proof | 79370 | 79370 | +0 |
| 86baeb91c | Integrate native batch6 fixtures and independent proof adapters | 79370 | 79370 | +0 |

Each comparison uses the pinned absent-aware staged counter and confirms the
resulting commit tree. Reference absent, retired R9c; last65,417/tree498dd1917812.


### Batch 6 shared controller integration, 2026-10-11

This commit adds committed fresh native-only import/footprint preparation,
independent source transcripts, original fresh Fork/FUSE verifier topology,
no-warmup one-Commit reuse, and sustained repository-history control on the
working ordinary SDK/Sandbox path. Footprint clocks include fresh public Init,
sealed byte installation/current setup, original edit/Commit/End; no setup
Store substitutes for paid Init. Root/source/full9 binary identities are bound.
Known-Stop exports preserve WAL before separate normalization/typed copies.
Unfiltered global census and actual SDK tuple/Branch authority stay strict.
All54 actual cases are still NOT_RUN pending clean build, prepare and parity.

The closed SQL observer admits source/schema/trigger pins, immutable export,
known Stop, command/stream hashes, all actual mounted token owners and exact
namespace cleanup. Actual source stat and isolated-copy allocation remain
separate. Decision62 leaves original per-Workspace physical resource axes
UNAVAILABLE; byte/proof/clean results cannot imply those resource checks PASS.
Repository keeps every original selected ACK, including UpToDate, with a
Created-only census; both runtimes receive original sampled8GiB RSS/zero
expanded-child-swap guards. Import Init retains sampled2GiB RSS and zero swaps;
footprint uses only zero swaps, adding no RSS cap. Sampler join is finite1s;
unknown runtime wait stays failure without inferred Stop or zero.

H-27 (harness): multi-runtime disposer emitted an optional None key even for
legacy receipts while the reader required7keys, so the unchanged partial-loss
regression failed after primary removal. Original Root FAIL and worker
before-fix FAIL7,771,375ns retained. Worker19b7de1a4 fixes exact conditional
identity and source/receipt/manifest/outer ACK/attempt/completion/custody order
bindings; no deletion retry or verdict rewrite. Seven affected/new worker
checks PASS98,781,334ns; all8 integrated retention checks PASS378,122,666ns,
including the unchanged close-failure guard. All Docker actions were mocked.

Integrated owning checks under main120s outer and100s child wall stops:
repository controller six PASS377,417,625ns; closed storage controller two
PASS260,793,750ns (negative subcases retained); workspace PASS313,040,500ns;
parity failures PASS262,922,583ns; fixture machinery PASS435,731,000ns;
closed storage10 PASS492,558,416ns; host observer10 PASS202,304,125ns;
short host swaps8 PASS207,686,333ns; combined current RSS3 PASS140,305,500ns;
import admission6 PASS322,896,833ns (native CLI check previously qualified
separately at final source). Existing staging, receipt custody, parity build CLI
and history publication checks reused at their unchanged owning source bodies.
No expiry or actual case result is claimed by these authoring checks.

Exact worker handoffs reused:
- Native dedup oracle44bbead833: c1de1b3055db2778678429df3b0a27fd2b44b56134e6c44daeb90da46a2118f2.
- Closed accounting4e1e99045: 66262de646d9c6918c01122de5f89ded00f3ce3bdb00c2e512317cacf35ac267.
- Generic host observer58558e005: 7609d950968e262dbf2252d430a16da087e5d7d8975c938d540b395b917b16d2.
- Repository hooksb4cebee1a: 2c5c890dacebd08f1ef341be4b2e885584eb43cee7272db8a8c2804680eb70e9.
- Conditional cleanup19b7de1a4: d82b90ce63fd66b07e22bb6fdc451a2f93fbade185038536f8c228035f3cc7b4.
- Short host guardf661cb4a5: af11c078745b03f2678234423a186655e234f9478f10b8aeefaab5ef30f2a8c2.
Each handoff remains in its named worker authoring raw directory; integration
imports only owned exact functions/suffixes, preserving unrelated main edits.
The initial host test-file syntax error and native/test/format failures remain
unchanged in raw receipts. No product change, new dependency or speed work.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| Worker4e1e99045 | Closed source-schema accounting observation | 79370 | 79370 | +0 |
| Worker58558e005 | Finite host process observer | 79370 | 79370 | +0 |
| Workerb4cebee1a | Repository actual runtime guard hooks | 79370 | 79370 | +0 |
| Worker19b7de1a4 | Exact optional multi-runtime cleanup custody | 79370 | 79370 | +0 |
| Workerf661cb4a5 | Optional short-command zero-swap guard | 79370 | 79370 | +0 |
| 149d40bac | Shared batch6 preparation/controller/receipt machinery | 79370 | 79370 | +0 |

Pinned absent-aware staged counter, identical first-parent/final product scope;
reference absent (retired R9c, last65,417/tree498dd1917812). Full9 clean source
build and actual parity/sample/cleanup/results remain next, with shared host/
Docker VM/protected and retained failed services interference unquantified.


### Parallel report path correction, 2026-10-11

Owner explicitly authorized simultaneous family execution in separate worktrees.
Section6.1 previously put every run at date/commit only, so independently
registered families at149d40bac would collide. Decision66 amends only the new
run path to baselines/core/<date>-<commit>/<original-family>/; the two CSV
names and schemas stay exact, and all historical run directories are unchanged.
This is its own specification/documentation commit before new CSV integration.
Production79370->79370(+0), active core9ac unchanged, reference absent/retiredR9c
last65,417/tree498dd1917812, pinned absent-aware staged counter/tree confirmation.

Native nine-asset build at clean149d40bac passed; build.json SHA-256
0dde333de6c3b8703473d43abc670d52d4df2baabddd232080b22f052baeb431.
All1335 compiled source pins checked inside the pinned Linux image before and
after build. Host release143,799,584ns (final no-run cache reused), Linux
25,898,355,750ns; compiler stops120s, serialized build-only outer240s.
Runtime9b03fcdc39bfe1c921131df247d7d77cf254bd8de03ed716b5fdd3fc38c30dd3;
daemon unchangedaa9de24f0ba916c0ac483f8e559d9d6986dfe4296f4d26a829801a1cb7e4ffd4.
Passing build container removed; OWNER-3 exact two passthrough checker refusals
remain FAIL, no provenance PASS is claimed. Durable execution remains disabled.

H-28 (harness, fix IN_PROGRESS): first cross anchor and all6 original footprint
attempts at149d40bac fail before daemon readiness, exact43-byte stderr
"daemon I/O: existing installation metadata". Host Init correctly produces
manifest.daemon_sqlite=None; the new fresh source-only installation route byte
copied it without public install qualification. Installer SHA/root0:0/0700/0600
readbacks passed, so this is not a permission/product bug. No startup guard,
profile, workload, timeout or test is weakened. Decision65 uses the exact actual
SealedProject with existing public ProjectApi install, preserving the paid Init
ACK boundary. Capped owns native mode and Tiny import controller; Root footprint.

Cross has1FAIL+9NOT_RUN, all10 native preparations and parity PASS. CDC has18
native preparations PASS, remaining3/preparation and all21 actual cases NOT_RUN.
Reuse initial4 preparation PASS and installed metadata are source-backed
unaffected; its hold is released, while future fresh import sampling stays held.
The Root footprint orchestration already continued through its6 distinct cases
before receiving the shared diagnosis; all6 single-attempt failed receipts are
retained, with no unchanged rerun. No product fix or passing case is claimed.
Failed resources remain retained; exact all7 container/volume pairs and both
original CSV pairs are recorded in the following result integration commit.

| fe07454d3 | Correct parallel result directory in specification | 79370 | 79370 | +0 |


### Parallel CSV path implementation, 2026-10-11

The report default now implements Decision66 and specificationfe07454d3:
new workload reports use the original family beneath date/commit. Explicit
previous destinations and cluster-two/cells routing retain exact paths.
No CSV schema, recorded value, case verdict or closed report was altered.
External collision/closed-flat-file regression initially FAIL197,667,500ns;
final two path guards PASS185,063,958ns. Existing verification-only report
checks PASS149,420,917ns and incomplete-context report checks PASS147,555,625ns
at their unchanged result_rows source. All under main120s/child100s, no expiry.
Raw2026-10-11-batch-6-import-engine-checks retains original failures.

The first lead report CLI attempted unsupported --out and was refused before
writing anything (exit2); the public report command has no such option. The
subsequent ordinary bench.py report used its corrected default path and read
original receipts without repeating measurements. Old source149 footprint
six-row CSVs exist, and their independent result ledger is committed next.

| 80c39e522 | Implement collision-free report default/explicit path compatibility | 79370 | 79370 | +0 |
Pinned absent-aware staged counter, first-parent/final exact trees; product
9ac unchanged. Reference absent, retiredR9c, last65,417/tree498dd1917812.


### Store-footprint initial failed cohort, 2026-10-11, source149d40bac

All6 exact native-only preparations PASS once (32,273,440,459;
33,919,402,500;6,907,970,792;1,532,905,042;1,513,304,541;1,590,651,291ns
in original registered order). Complete original native-source family parity
PASS6 under main120s, samples0; original fixture geometry/digest remains exact.
All6 actual single attempts then FAIL at H28 fresh daemon startup. They remain
visible in the generated two CSVs at
baselines/core/2026-10-11-149d40bac/store_footprint/. No unmount or successful
Stop is inferred; the daemon refused before mount. Actual host Init succeeded
and sealed each source, but full case/required verifier did not complete.

| Case | Observed incomplete controller ns | Public Init ns | Initialized host Store allocated B | Retained acknowledged failed container | Retained owned sample volume |
| --- | --- | --- | --- | --- | --- |
| store-footprint-unique-100000 | 19252977375 | 7626703833 | 516325376 | 8e59c3f22d3b37aef2c88cbc729501cca3886315616e80e855fea0283d9d960c | layerfs-fsb-sample-6add3e811bc54857 |
| store-footprint-metadata-cardinality-100000 | 28248898417 | 15900226417 | 555814912 | 29ccc2d42d3fd8fa4eb460dff321a98f1c0d7dfbd3795d145a0e8c7f4a18b94f | layerfs-fsb-sample-2ea273f9e2c545b4 |
| store-footprint-large-object-500m | 13893280667 | 2786687458 | 506150912 | 889be9d51533aacc675e8960927f29a37451d01745dd00f4fff3f14adcd13410 | layerfs-fsb-sample-133f985fe0e145f2 |
| store-footprint-unique-100-low-v1 | 1036525375 | 47998000 | 5255168 | fb44a4fc7d4ae33f80bc5a044c848584df928397c3b2895a31ad48ec0d9ffd16 | layerfs-fsb-sample-82896e5628c94d3f |
| store-footprint-metadata-cardinality-100-low-v1 | 1067808500 | 53190042 | 5300224 | 826b235f4bc81af6d637c9f00b4e37d842502363d2648555bc68f8204de32653 | layerfs-fsb-sample-59def66dadeb4c14 |
| store-footprint-large-object-10m-low-v1 | 1170153708 | 68016708 | 10301440 | 33f068ceef5a77c2a153a77e1b5349f719bac0dad672d2bd842cce4de820e5e2 | layerfs-fsb-sample-438a507c259c4d8b |

The first two incomplete elapsed observations already exceed the original15s
soft limit (19,252,977,375 and28,248,898,417ns); CSVs mark EXCEEDED and retain
FAIL because installation/mount/verifier never completed. These are incomplete
attempt durations, not finished-case performance or speed results. The four
other incomplete durations are WITHIN15s. Final Store/overlay and daemon
request/job/statement counts remain UNAVAILABLE; initialized host Store bytes
above are separately scoped, never substituted for final case storage.

Raw all6 receipts and exact native failed events remain under
2026-10-11-batch-6-store-footprint-149d40bac/run/; passing setup installer shell
containers were removed after their own byte-install ACK, but the six failed
actual daemon/sample pairs above are retained. Root orchestration did not stop
between its distinct cases on the shared H28 diagnosis; no case was repeated.
The corrected actual-project public installation path is independently authored
and qualifies before each affected case gets one changed-identity attempt.
No workload, fixture, worker count, cache state, timeout or limit changes.

| c451c2b1f | Record all6 original failed footprint attempts and resource custody | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/actual commit-tree confirmation; active
product9ac unchanged; reference absent/retiredR9c, last65,417/tree498dd1917812.


### H28 existing public installation correction, 2026-10-11

Root integrates Capped3e37aa36f actual-project native init-install mode and
Tinyf05010629/11635c9b original-clock controller/copy correction. Exact actual
SealedProject remains in the native process after its paid Init ACK; one bounded
install-LF instruction passes it once to the existing public SDK installer.
The installed manifest is a distinct0600/create_new output. Its daemon_sqlite
and full root/stack/Branch/profile/size/SHA authority derive from actual SDK
ACK, followed by original snapshot/EndSession/fence/known setup Stop/finished.
No project reconstruction, new Init, fabricated metadata or product feature.
Native input uses finite unbuffered fd poll/read; EOF, wrong frame and deadline
refuse, no retry or reader thread.

For imports, the parent pays launch through actual Init ACK; subsequent source
fingerprinting, exact observer-copy creation, public typed proof, original SDK
installation and fresh Fork/FUSE proof/End/Gone/Stop all charge original41/45/59
proof allowance. Copy-only physical mutation is recorded separately; the actual
held sealed file must retain exact bytes/size. Boundary's verification-only Init
is itself charged. For footprint, the original full current controller phase
includes fresh Init/actual SDK installation/setup through ordinary edit/Commit/
End; required read-only resource observer intervals are excluded and retained.
Old incomplete timings/receipts keep their original clocks. Installer CIDs remain
authenticated additional owned containers until full case/outer publication ACK;
failed/uncertain pairs remain retained. Old namespace Init route is unchanged.

Worker native final no-run PASS9,589,165,334ns and four external guards
PASS1,172,393,667ns/private120,19 frozen inline tests filtered. Native handoff
SHA-256d7664619db9e69e3623d61d7fb53649d0f56998df3ef203b8a3900e78ee1ddea.
Python original parent RED6,899,375ns retained; four corrected checks PASS,
then namespace mock fixture lacked its temporary workload and FAILED. Only
fixture corrected, affected check PASS8,196,792ns; installation-custody guard
PASS4,518,792ns. Copy/clock supplement four affected checks PASS40,951,833ns,
five unchanged guards reused. Handoffs52285053d6466077b3fdd41619b18f4a91b28df09fa19fc368d56cf4b8dbeb82
and44fad016822c18f102e7fb224aee4d0067b1ab550c4b1a1c7cc6abe754d7b542.

Root external Args fixtures add only installed_manifest:None; project_import
wrapper initially failed native compile because new production-free fixture
helper lacked its module declaration. Original compiler FAIL retained; exact
fixture/runtime declarations added, final all-test no-run PASS7,345,622,042ns.
Four H28 native guards PASS1,016,092,875ns under main120s/child110s and frozen
inline tests filtered. Integrated nine Python guards PASS428,530,625ns;
import admission6 PASS314,049,333ns; workspace PASS384,491,792ns;
retention8 PASS365,561,500ns, main120/child100. Owned Rust formatting, Python
syntax and diff checks PASS. Raw2026-10-11-h28-main-integration retains all
compile/qualification failures and final outputs. No actual corrected SDK
installation/case is claimed; clean runtime seal and affected one-time samples
are next. Product9ac and every original workload/cache/worker/limit unchanged.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| Worker3e37aa36f | Actual retained Init/public install native boundary | 79370 | 79370 | +0 |
| Workerf05010629 | Existing SDK installation/custody import controller | 79370 | 79370 | +0 |
| Worker11635c9b | Immutable observer copy and actual proof-clock anchor | 79370 | 79370 | +0 |
| 1cc25d4de | Fix fresh installation through existing SDK and footprint wiring | 79370 | 79370 | +0 |
Pinned absent-aware staged counter and exact resulting tree confirmation;
reference absent/retiredR9c, last65,417/tree498dd1917812.


### H30 acknowledgement/claim and H31 operation expectation correction, 2026-10-11

H28 actual public installation now succeeds at source1cc25d4de/runtime7766209e.
CDC overwrite1 and cross anchor both subsequently FAIL on H30: actual runtime
emits a successful mount ACK, while imported verifier EventProcess.send waits
literal mount-branch. Existing context_send maps the same original request to
mount; one-line correction preserves target, body, deadline and one attempt.
Other call mappings reviewed source-correct. Original local mounted=False after
mismatched wait does not prove absence of the published mount. Separate bound
old event/custody supplements record actual key/ns/incarnation/NativeReady/root/
Fork without modifying failed receipts or claiming End/Gone/Stop.

Worker25b8c5af original actual-event RED10,940,166ns, final three schedule/
wrong-root/short-write guards PASS25,516,417ns/private120. Handoff SHA-256
4e59c87a975ff371f6b23ef917ee3b588748eb21e64818ed10952bd669092ecb.
Capped08332d117 also closes the prospective identity bug: imported Init claim
now pins executed workspace and receipt guard hashes. Legacy namespace
identity stays exact, commit-only changes do not enable replay, old claims
remain untouched. Original four-vector RED retained, final three checks
PASS207,974,542ns; handofff68bc2f6687a0c78ab1b4af107e0d6063b35e3d72b82d8a098b40e1c5fab15d9.
Root three claim guards PASS193,472,125ns; three actual mount protocol guards
PASS320,378,292ns. Initial integrated H28 tests failed3of9 because their
synthetic source_identity fixture lacked the newly required source hashes;
only fixture pins were added, final9PASS380,794,500ns. No guard was weakened.

H31: corrected-source footprint unique100000 completes actual Init/install,
fresh mount, ordinary original10B edit and a Committed ACK, then controller
KeyError expected_commit before terminal End. Its definition has Init step0
and one mutation step1, whose expected_committed outcome is already explicit;
generic controller erroneously indexed Init and eagerly evaluated a missing
top-level fallback. Fix selects exactly the one declared committed mutation,
requires schedule and step both Committed, and rejects missing/duplicate/
UpToDate/mismatched declarations before an attempt. Ordinary nonordered
families keep their existing top-level expectation; ordered explicit outcomes
remain strict. No registration/workload/oracle change. Original external RED
258,182,375ns retained; final three strict step/model guards PASS261,965,917ns;
retention8PASS372,473,959ns and workspacePASS358,014,833ns. A mistakenly
selected nonexistent test_batch4_history.py invoked0tests and FAILED(exit5);
this is not a passing check. Actual owning history controller finalPASS
417,600,333ns. Main120/child100 stops, no expiry. Python syntax/diff checksPASS.

All three newly failed actual receipts/resources remain retained; no unchanged
resampling. Old1cc counts: CDC1FAIL+20NOT_RUN, cross1FAIL+9NOT_RUN, footprint
1FAIL+5NOT_RUN. Full actual case/native/verifier End/Gone/Stop not established
for these first cases despite preceding successful SDK effects. Their generated
CSV pairs are committed separately. Product9ac unchanged; Runtime7766 and eight
qualified assets remain source-matched, so this Python-only correction needs
no new native build. Next each affected first case may be sampled once at the
new exact executed-code identity; untouched selections continue once.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| Worker25b8c5af0 | Consume actual public mount ACK | 79370 | 79370 | +0 |
| Worker08332d117 | Pin imported verifier/guard treatment identity | 79370 | 79370 | +0 |
| This commit | Correct declared operation/acknowledgement and claim interpretation | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812; no product/speed/limit/cache change.


### H32 declared cache ineligibility must finish the proof, 2026-10-11

Root082 footprint unique100000 ran actual Init/public install/edit/Committed,
End and an authenticated read-only Gone acknowledgement. Its complete paid
wall is14,657,679,166ns, within unchanged15s soft limit. The original cache
helper returned3 with ClassA/statusINELIGIBLE, eight resident Store SHM pages,
zero payload bytes read, exploratory=true/admission_eligible=false. The
controller raised before independent verifier/known Stop and recorded FAIL;
that receipt is retained without promotion. Raw root cohort
2026-10-11-h30-h31-store-footprint-run holds oneFAIL and fiveNOT_RUN.

Source residency.py intentionally maps INELIGIBLE to exit3, and observation
errors to exit4. H32 accepts only exact exit3 plus ClassA, positive integer
resident_pages, zero integer attempts/payload reads, exploratory=true and
admission_eligible=false. It retains the complete observation and numeric
INELIGIBLE; all other nonzero exits and mismatches still fail. Original
readiness, cache classifier, verifier, proof deadlines and shutdown are
unchanged. External parent RED had one ERROR out of two tests in tool wall
0.177723834s; the second negative-refusal test passed. Final two checks
PASS273,475,250ns, existing workspace17PASS331,229,000ns, retention8
PASS381,877,959ns, history controller8PASS401,969,583ns, each explicit120s
R4. Raw2026-10-11-h32-terminal-cache-status-checks records commands and results.

Retain failed actual performance runtime
9d484e598c9f9599c4ec80bb2def6e6b9bb3e78486dd04e629dbbc9837283b76,
known-stopped installation runtime
d23155a3a5ef18645580cae1f45a29f476ba9bbbcb998ceba91abf29ac021090,
and volume layerfs-fsb-sample-a459275354b44434. No verifier/Stop is inferred
from End/Gone. No product change or optimization; product9ac/runtime7766
remain qualified. The affected first case may run once at corrected source;
the five untouched footprint selections then run once in original order.
Parallel CDC/cross imports use their separate proof controller; repository
ClassC uses a separate recipe route. Those unchanged082 cohorts continue
without replay, with shared-host/Docker interference unquantified.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Preserve declared cache miss and complete functional proof | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812; no workload/limit/cache treatment change.


### H33 unknown cache observation preserves ineligibility, 2026-10-11

At source0a8d15f0e the affected first footprint case again completes paid
Init/edit/Committed and End/Gone. The unchanged observer exits4 with
UNAVAILABLE, attempts0/errno38 and original error `file identity changed
during residency attestation`. It returns no valid residency result. This
identity refusal is retained; neither a file identity nor coldness is
invented. Which backing file changed and the writer remain unobserved; no
product correctness or progress conclusion follows. Original raw cohort
2026-10-11-h32-store-footprint-run contains oneFAIL and fiveNOT_RUN.

Spec sections5/6.1 classify unknown measured input cache state as INELIGIBLE.
The functional controller now consumes only the declared ClassA unavailable
status4 with actual zero integer attempts, nonnegative integer errno and
nonempty error. Every nonmatching record, malformed output and other exit
still fails. Original observer/checker exit4/completionFAIL and reason stay
visible in raw evidence, cache state stays unavailable/ineligible; this grants
no cold credit. No retry, observation phase/order, cache state, operation,
verifier, stop, deadline or workload is changed. Original regression retains
parent ERROR; final four tests include eleven original refusals and ten
malformed unknown vectors, with unchanged workspace/retention guards.
final-01: exit0, wall297231916ns
final-02: exit0, wall331331000ns
final-03: exit0, wall382590250ns
parent-red: exit1, wall309062000ns

Actual incomplete paid wall15462692251ns; proofNOT_RUN; no
known runtime Stop/native or typed verifier is inferred. Preserve runtime
647492bb2d4fb61380533d8c6a2cdb85ec421c2bbb89b5d78ab74db30476cd5a, installer
3e84dd3e3176dccee70f838a4f1bcd8518a86ad415e5cf373bb99ddd0d739627,
volumelayerfs-fsb-sample-6017ae5a999c477b. This failure and H32 remain history. At the
new executed controller identity only this affected case may run again once;
the five untouched selections run once. Imported parallel CDC/cross and
repository separate controller paths are unaffected and retain082 identities.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Retain unknown cache refusal and complete functional proof | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent
retiredR9c, last65,417/tree498dd1917812. Product9ac/runtime7766 unchanged.


### Batch 6 Store footprint closed with two proof failures, 2026-10-11

All six original cases have committed unchanged definitions, sealed native-only
inputs and passing frozen parity; no Store/Init was prepared in advance. Final
actual cohort atfe205b6a6 runs each case once with public retained Init/install,
one unchanged ordinary ten-byte namespace edit, mandatory Committed ACK and
End. Four complete native/public typed/census/closed-storage proofs PASS with
Gone/KnownStop and guarded outer acknowledgement. Their eight owned runtimes
and four sample volumes were removed by the bound passing disposer. Numeric
fourINELIGIBLE: paid pre-phase input cache state is unqualified; terminal
residency observations confer no cold credit.

Two100000-file cases retain FAIL at the original cumulative41s proof-work
cutoff (45s minus4s cleanup reserve), despite native clipped proofs, actual
End/Gone/KnownStop and completed closed Store normalization. Unique100000
public typed proof completed; its subsequent authenticated normalization
validation/census deadline expired. Metadata100000 typed host verifier
timeout28.55025791702792s used only its remaining original work allowance;
its event transcript has observation_scope/selection only, no final typed
proof. No partial proof is promoted. Normalization operation receipts PASS
are preserved separately from overall deadline failure. Oracle source
_closed_store_file/prepare_closed_store performs finite64KiB streaming hashes,
128-page SQLite backup, separate target checkpoint/close and deadline-guarded
receipt/hash validation. Typed initial census store_footprint.rs251-316
walks the exact100000-file namespace and metadata/length/inode classes;
no live daemon operation occurs in that offline phase. No wrong bytes, crash
or identified fixable product correctness bug is shown. These are failed
proofs, with unavailable progress inside the expired typed scan; no
optimization, budget extension or unchanged replay is selected.

| Original case | Final status | Paid wall ns /15s | Proof wall ns | Store logical/allocated B | Requests/jobs/statements | Cleanup/Stop |
| --- | --- | ---: | ---: | ---: | --- | --- |
| store-footprint-unique-100000 | FAIL | 15686684833 /EXCEEDED | 41261847334 | 516735848/516743168 | 12/102/464 | Gone/KNOWN_STOP |
| store-footprint-metadata-cardinality-100000 | FAIL | 21637843626 /EXCEEDED | 41085351125 | 556090216/556097536 | 12/102/464 | Gone/KNOWN_STOP |
| store-footprint-large-object-500m | INELIGIBLE | 9198431374 /WITHIN | 24609321417 | 506352584/506359808 | 12/287/1500 | Gone/KNOWN_STOP |
| store-footprint-unique-100-low-v1 | INELIGIBLE | 2644407417 /WITHIN | 2327876333 | 5411592/5414912 | 12/102/464 | Gone/KNOWN_STOP |
| store-footprint-metadata-cardinality-100-low-v1 | INELIGIBLE | 2984081458 /WITHIN | 2085784542 | 5452504/5455872 | 12/102/464 | Gone/KNOWN_STOP |
| store-footprint-large-object-10m-low-v1 | INELIGIBLE | 2643697541 /WITHIN | 2407809125 | 10474344/10477568 | 12/152/771 | Gone/KNOWN_STOP |

Store axes are actual closed main/sidecar logical and allocated bytes; source
stat boundaries are bound separately from exported-copy allocation. Counts
cover the registered measured command/control phase, excluding host Init
and independent verifier; they are not a whole Init-plus-proof total.
Legacy per-Workspace exclusive physical spool/live/superseded/peak and
owned-file axes are UNAVAILABLE under Decision62, not zero or passed physical
equations. Current namespaced logical rows/payload accounting/automatic
Gone are separately checked; original physical observability is unverified.

Prior cohorts remain unchanged:149 six H28FAIL (already committedc451c2b1);
1cc oneH31FAIL+fiveNOT_RUN;082 oneH32FAIL+fiveNOT_RUN;0a8 oneH33FAIL+fiveNOT_RUN.
This commit includes all four newly generated six-row CSV pairs, each beside
its own original reference rows and exact source identity. H28/H30/H31/H32/H33
changed executed controller bytes justify only their affected attempts;
untouched cases ran once. A lead dispatch used an incorrect metadata case ID
and was refusedexit2 before any registration/claim/product operation; its
original stdout/stderr/input/exit files stay retained, then the correct
original IDs were read directly from family.json. No case was replayed.

Retained verify returned1 with fourPASS/twoFAIL in the final cohort; report
generated all four new pairs once. Exact command/result walls:
- verify benchmark-results/fs-bench-pro/2026-10-11-h33-store-footprint-run: exit1, wall7226269166ns, outer120s.
- report benchmark-results/fs-bench-pro/2026-10-11-h28-store-footprint-run: exit0, wall514810750ns, outer120s.
- report benchmark-results/fs-bench-pro/2026-10-11-h30-h31-store-footprint-run: exit0, wall513864084ns, outer120s.
- report benchmark-results/fs-bench-pro/2026-10-11-h32-store-footprint-run: exit0, wall498793250ns, outer120s.
- report benchmark-results/fs-bench-pro/2026-10-11-h33-store-footprint-run: exit0, wall6592524000ns, outer120s.

| Source/case | Retained runtime | Retained installer | Retained volume |
| --- | --- | --- | --- |
| 1cc25d4de/store-footprint-unique-100000 | e3cfc379c233270e20e4c828315d2ebbad53977484a211edac09e544e692bfcd | 04dbe324a5085a4cfbb75f04b8c9e7e891e2951c684a62a9b015c8ca20ff9813 | layerfs-fsb-sample-db9744f8474c443a |
| 08288e824/store-footprint-unique-100000 | 9d484e598c9f9599c4ec80bb2def6e6b9bb3e78486dd04e629dbbc9837283b76 | d23155a3a5ef18645580cae1f45a29f476ba9bbbcb998ceba91abf29ac021090 | layerfs-fsb-sample-a459275354b44434 |
| 0a8d15f0e/store-footprint-unique-100000 | 647492bb2d4fb61380533d8c6a2cdb85ec421c2bbb89b5d78ab74db30476cd5a | 3e84dd3e3176dccee70f838a4f1bcd8518a86ad415e5cf373bb99ddd0d739627 | layerfs-fsb-sample-6017ae5a999c477b |
| fe205b6a6/store-footprint-unique-100000 | 9fd3a1f7b58a3de4d67ef82ced70953286dbfabd41d7e152b3947fe477fca141 | 1ab2431bafdd279b63f40034a7fd824384eb529be5dffcc2f50ad040d0fb24bf | layerfs-fsb-sample-83122df33e1447b0 |
| fe205b6a6/store-footprint-metadata-cardinality-100000 | 2e1dffdf90a780ce4fc5f0e52afd3fd6436264a68c7831ecf09f787399ec832b | ba971583062de2685c2b4cb0323421f792e3dbf5f83980e80588ea7311192a7f | layerfs-fsb-sample-51beb9ac88a84e65 |

Shared host/Docker VM and retained failed services interference is unquantified.
Product9ac/runtime7766/other eight sealed assets unchanged. Every original
limit, workload, metadata/cache declaration and worker count remains.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Close footprint final6 and preserve3 newly failed cohorts | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### Batch 6 Workspace reuse closed with three proof failures, 2026-10-11

All14 original definitions/native inputs/parity PASS at149d40bac. Each case
ran once:11 complete native/two-root typed/unfiltered census/closed-overlay
proofs PASS with Gone/KnownStop, numeric11INELIGIBLE. Their11 owned passing
sample runtime/volume pairs were removed only after bound outer publication.
All14 have native full final proofs, exact Committed ACKs and End/Gone/
KnownStop, but three500MiB cases retain overall proofFAIL. Original45s
verification work reserves4s for cleanup, leaving41s; hard publication59s
and complete15s soft limits stay unchanged.

| Original case | Overall | Paid wall ns | Proof wall ns | Limit | Cleanup/custody |
| --- | --- | ---: | ---: | --- | --- |
| dedup-workspace-exact-1-compact-v2 | INELIGIBLE | 179043875 | 2319880000 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-exact-10-compact-v2 | INELIGIBLE | 270424791 | 4927691417 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-exact-100 | INELIGIBLE | 1470377709 | 23438047375 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-exact-500 | FAIL | 6224156291 | 41726486875 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-local-1-compact-v2 | INELIGIBLE | 197112749 | 2322207625 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-local-10-compact-v2 | INELIGIBLE | 293211041 | 3874080000 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-local-100 | INELIGIBLE | 1424302334 | 24173288375 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-local-500 | FAIL | 6425610250 | 41449185416 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-unique-1-compact-v2 | INELIGIBLE | 180455416 | 2295179459 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-unique-10-compact-v2 | INELIGIBLE | 279597500 | 4459694333 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-unique-100 | INELIGIBLE | 1520899792 | 27411561792 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-unique-500 | FAIL | 17485507501 | 41296518709 | EXCEEDED | Gone/KNOWN_STOP |
| dedup-workspace-unique-1-base128-v3 | INELIGIBLE | 197034625 | 18920740292 | WITHIN | Gone/KNOWN_STOP |
| dedup-workspace-unique-10-base128-v3 | INELIGIBLE | 292386875 | 19439316583 | WITHIN | Gone/KNOWN_STOP |

Exact500 proof41,726,486,875ns: native/typed/censusPASS; closed overlay
copy had2.647179625s remaining and timed out after2.6547s, partial
437,518,336B of533,868,544B logical source (802,308,096B source allocation).
Local-edit500 proof41,449,185,416ns: native/typed/censusPASS; copy2.98999s
remaining expired with484,343,808B of the same533,868,544B logical source.
These partial exports are not passing cleanup/accounting proofs. Unique500
paid17,485,507,501ns EXCEEDED15s; proof41,296,518,709ns fails during the
final typed proof after its remaining16.38467775s allowance. Only
observation_scope/selection events exist; no typed workspace_verified/
finished/census result is inferred. Native full631paths/628files/
658,505,728B proof and actual Committed/Gone/KnownStop remain independently
PASS. Native/typed oracle source loops are finite over selected paths and
positive file lengths, with no live SDK/daemon calls in the offline scan;
no byte mismatch/crash/deadlock was observed, continuous typed scan progress
is unobserved. No product speed fix, retry or limit extension is made.

Actual raw source remains in the namespace worktree:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/benchmark-results/fs-bench-pro/2026-10-11-batch-6-dedup-workspace-reuse-149d40bac/run/. Main imports the generated CSV bytes unchanged under
149d40bac/dedup_workspace_reuse, preserving their exact source/evidence
paths and every14 row/reference. PerformanceSHA
0827851ae214bd83258553bdd4ecbab0f81b48914309933d834108c7b52046af,
verificationSHAb3cd8c4c39576efca172accaf1fe240d299efc2954f82a6239eb7a3a4aae4221.
Sealed family-handoff/handoff.jsonSHA
a73a359ad040f6205daf4017fbc2130c820bf35150a9eca53a61c2ae52a90c0e
and manifestbc45397462279cd461e61dd496457c85dec38d54157a6f68bdc1b72f7fc49268
bind parity, each actual receipt/outer ACK/removal and source/count/storage
scopes. Source total triples7014/78769/410013 in500 cases include separately
scoped verifier/cumulative counters; they are not invented paid-only totals.
CSV counts retain their exact registered phase scopes. Legacy exclusive
physical spool/live/superseded/peak axes remain UNAVAILABLE under Decision62;
actual shared source allocation/exported copies/current logical accounting
are kept distinct. Shared-host/Docker interference remains unquantified.

Retained failed known-stopped runtime/volume pairs:
- exact500:4b2f6b3d79ad637230130a69189b7cf5e60214e99a2ad24753c05c44ae429368 / layerfs-fsb-sample-3f2acc020c5a4eff.
- local500:729c33f7f50678dc7dd3ad9731d462febb3c76f4df47ca6c4408e7d007386ec1 / layerfs-fsb-sample-2aadabb2c4d24538.
- unique500:ae7aff3221424892f2d2320879ceb6fef0ec0f04363fc72db61523d5fc65ca98 / layerfs-fsb-sample-793bb37e660d4118.
Three immutable prepared master volumes remain during the rollout, named
layerfs-fsb-master-dedup-workspace-reuse-b10dfc77fd9e4ac7,
layerfs-fsb-master-dedup-workspace-reuse-20865dd3058040dd and
layerfs-fsb-master-dedup-workspace-reuse-11adfd9c15b442b6.
No new fixed bug or product change. New H28/H30/H31/H32/H33 fresh/footprint
paths do not affect this completed149 reuse evidence; it is reused unchanged.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Record all14 reuse outcomes and retained3 proof failures | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### Batch 6 cross-file dedup complete, 2026-10-11

All10 original cases at08288e824/runtime7766 completed actual paid Init,
retained public installation, full original fresh FUSE/native verifier,
current exact typed membership/dedup proof and End/Gone/KnownStop. Guarded
publication/outer ACK and bound removal PASS for20 owned passing containers
and10 sample volumes. Numeric10INELIGIBLE: ClassA source-page/host-metadata
coldness was unavailable. No paid15s/proof45s limit exceeded. Original
10native prepared inputs reused unchanged; no master volumes were created.

| Original case | Paid Init parent ns | Independent proof ns | Sealed Store B | Function/proof/close |
| --- | ---: | ---: | ---: | --- |
| dedup-cross-file-anchor-1 | 36335334 | 4044610375 | 1232896 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-unique-10 | 81173375 | 4459818250 | 10797056 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-unique-100 | 599792167 | 9565501292 | 106270720 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-unique-500 | 2698613375 | 32340981459 | 530804736 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-identical-10 | 49557750 | 4177071000 | 1232896 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-identical-100 | 85611792 | 6482228292 | 1236992 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-identical-500 | 320223334 | 16991380250 | 1236992 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-mixed-10 | 63272333 | 4343617209 | 7614464 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-mixed-100 | 750255792 | 9028288625 | 79769600 | PASS/PASS/Gone+KnownStop |
| dedup-cross-file-mixed-500 | 2417079958 | 28155462875 | 398168064 | PASS/PASS/Gone+KnownStop |

Paid Init ends at actual acknowledgement; public installation/native and
typed proof are independent proof work. Host Init has no daemon/FUSE
request, owner-job or statement counters: those paid axes remain unavailable,
while verifier-only telemetry stays separately scoped. Store logical/
allocated and payload counters have actual Init/closed proof scope. Sampled
PID RSS is an observed maximum with sampling gaps, not lifetime/continuous
peak; host swaps use declared reaped-child aggregate scope.

Two old anchor failures stay unchanged:149H28startupFAIL plus9NOT_RUN,
1ccH30actualmount/wrongACKFAIL plus9NOT_RUN. H28/H30 corrected controller
identities justified only the affected anchor runs. This commit imports all
three exact ten-row CSV pairs, each original reference/source/receipt preserved.
No CSV record is relabelled or old NOT_RUN hidden.

Source149d40bac retained resources: {"acknowledged_startup_failed_container": "a3e9a084d2fdb8d9cf80869cee4d4479aaa0e459bab43e8a7a809c5e26a25ada", "installer_containers": [], "verifier_container": null, "volume": "layerfs-fsb-sample-44984fcf450d46a6"}.

Source1cc25d4de retained resources: {"installer_containers": ["fcb8284e733aa79b3732fff7dad85aed0d29012f3c6b0a2b71e02a6037a3d05e"], "verifier_container": "5d47e443a941479e7f5ce899ba895bb05fa8f77666c0d220e4ae2b483983cccd", "volume": "layerfs-fsb-sample-4441d6043ca446f5"}.

The1ccactual Mount supplement SHA
c0e6c691c9b5c0fcf9c11b87d4ef1c00c79f29187dcd78732975a1e9977ebc44
binds actual successful key/ns/incarnation/NativeReady/Fork/root; the old
mounted=False field is not absence proof. End/Gone/Stop remain unobserved
for those failed resources; none was removed. Final sealed handoff:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-tiny-file-churn/layerfs/benchmark-results/fs-bench-pro/2026-10-11-batch6-dedup-cross-file-08288e824/final-handoff/HANDOFF.json
SHA82ecf3518a5ec0b05cad0772fe900de08f1d44e5a3eec2026f703984861d453d,
manifest0f3f8563ea2d03c2b5ed192190a96b4fd7e135075a439b4a619e628e00152c2c.
PerformanceCSV5c3f157c9185b5ded8720d4e9874afd24bafa3e2622caadbb70358cbfb98343f,
verificationCSV07d300cfa575ef4577ca75c42d29d82930bdefb14b3a49da1d14d22174e94743.
Retained-evidence verifyPASS1,870,757,875ns/reportPASS1,591,824,667ns, each
private120s. Shared-host/DockerVM interference unquantified; no speed/storage
comparison or release gate. H32/H33 footprint code does not affect this
completed import controller, so082evidence is reused without replay.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Record all10 cross-file results and two failed anchor cohorts | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### Batch 6 full repository failed-preparation specification closure, 2026-10-11

The existing section9 failed-preparation exception now explicitly covers
deepseek-full in repository_history. Its original157-checkpoint native
recipe once reached external120s, exit124/wall120,027,598,333ns, with all157
transition folders and partial fixture.json but no sealed prepared.json/
resource-claim/Store/master. Finite recipe membership hashing/sealing follows
fixture publication (repository_recipe.py305-318). No retry/feature/speed
change or late-output promotion. Actual caseNOT_RUN and parityFAIL persist.
Both independently prepared53/17 profiles passed individual parity; aggregate
family parity remainsFAIL. The fresh administrative closure at082 binds the
exact four preparation files, original registration and zero actual/proof
attempts; it supplies no runtime/benchmark observations.

Root inspected administrative receiptSHA
79b6ff97ce2fd38da6d1a5cc55de613a47a425758add0d07ac919e6ee2f2837c,
manifest9787ef4e9b6b98d07b396c741178200bf2f6beeb8d8b87b7016d1c2a9e7f9a1c,
registration06660c1daa165c7d6a842b238abe73c409fd9accbebb07034dcea78c6c50a142.
Raw namespace WT2026-10-11-batch-6-repository-history-08288e824 retains
partial outputs and every failure. This is the same truthful failed closure
as prior batches, not a passing done-when result. No timing limit or checker
changed. Repository's current mounted-root controller bugH34 remains under
repair; batch6 is still in progress and batch7 is not started.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Extend existing failed-preparation closure to full repository | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### Batch 6 CDC locality closed with one proof failure, 2026-10-11

All21 original cases at08288e824 reuse their exact sealed native inputs
and admitted7766runtime/eight other assets. Frozen parity21PASS. Nineteen
routine cases fully complete actual Init/install, original fresh FUSE/native
verifier, independent typed proof and End/Gone/KnownStop, numericINELIGIBLE.
BoundaryproofPASS has sample_count0/verification_attempt_count1 and performance
N/A. All20 passing cases have bound outer publication/removal;40 owned
passing containers and20 sample volumes removed. No15s Init or45s verifier
reported soft limit exceeded. One scattered500 retainsFAIL at the original
41s work cutoff with native/remote completion and End/Gone/Stop unknown.

| Original case | Status | Paid Init ns | Proof ns | Cleanup/custody |
| --- | --- | ---: | ---: | --- |
| dedup-cdc-overwrite-1 | INELIGIBLE | 24974542 | 4102044708 | Gone/KNOWN_STOP |
| dedup-cdc-overwrite-10 | INELIGIBLE | 32021125 | 4893376084 | Gone/KNOWN_STOP |
| dedup-cdc-overwrite-100 | INELIGIBLE | 171007875 | 8853970292 | Gone/KNOWN_STOP |
| dedup-cdc-overwrite-500 | INELIGIBLE | 392115833 | 27433961709 | Gone/KNOWN_STOP |
| dedup-cdc-insert-1 | INELIGIBLE | 24343041 | 3952965167 | Gone/KNOWN_STOP |
| dedup-cdc-insert-10 | INELIGIBLE | 32001125 | 4643832750 | Gone/KNOWN_STOP |
| dedup-cdc-insert-100 | INELIGIBLE | 111739042 | 8548539958 | Gone/KNOWN_STOP |
| dedup-cdc-insert-500 | INELIGIBLE | 466466708 | 27163424250 | Gone/KNOWN_STOP |
| dedup-cdc-delete-1 | INELIGIBLE | 24225125 | 3964578125 | Gone/KNOWN_STOP |
| dedup-cdc-delete-10 | INELIGIBLE | 36872084 | 4467767375 | Gone/KNOWN_STOP |
| dedup-cdc-delete-100 | INELIGIBLE | 128189291 | 8872616667 | Gone/KNOWN_STOP |
| dedup-cdc-delete-500 | INELIGIBLE | 620201750 | 29357699459 | Gone/KNOWN_STOP |
| dedup-cdc-common-body-1 | INELIGIBLE | 26928125 | 4149867042 | Gone/KNOWN_STOP |
| dedup-cdc-common-body-10 | INELIGIBLE | 47437708 | 4303130542 | Gone/KNOWN_STOP |
| dedup-cdc-common-body-100 | INELIGIBLE | 221259458 | 9271582375 | Gone/KNOWN_STOP |
| dedup-cdc-common-body-500 | INELIGIBLE | 1162854958 | 31583453458 | Gone/KNOWN_STOP |
| dedup-cdc-scattered-1 | INELIGIBLE | 29525500 | 3987727875 | Gone/KNOWN_STOP |
| dedup-cdc-scattered-10 | INELIGIBLE | 75690333 | 4566803834 | Gone/KNOWN_STOP |
| dedup-cdc-scattered-100 | INELIGIBLE | 629160917 | 11811095750 | Gone/KNOWN_STOP |
| dedup-cdc-scattered-500 | FAIL | 3139060500 | 41005073333 | UNAVAILABLE/UNAVAILABLE |
| dedup-cdc-boundaries-proof | PASS | N/A | 4174873500 | Gone/KNOWN_STOP |

Scattered500 proof41,005,073,333ns exceeds the original work cutoff by
5,073,333ns;45s minus4s cleanup reserve is unchanged. Public SDK Init
3,139,060,500ns, independent typed project_verified PASS26,772,498,458ns,
actual public installation/installerStop and Fork/mount are acknowledged.
Native original verification began once with at most4.414s remaining work;
no verify/known exit/End/Gone/verifierStop followed. Original streams0B and
no byte mismatch/crash was observed. Later bounded read-only State/top/logs
show runningdaemon/OOMKilledfalse, only daemon process at observation and
no Bash/workload process. ConnectionFailure/UnexpectedEof follows host fence,
not successful native completion. Source native verify_native scans finite
503 paths/501 positive1MiBfiles; offsets advance, but read_exact may block on
FUSE. These observations cannot exclude an earlier stuck FUSE read or infer
success; uncertainty remains, no identified fixable cause permits a product
fix. No extra kill, unmount, replay, timeout increase or optimization.

Original1cc overwritten1H30FAIL+20NOT_RUN stays in both21-row CSVs alongside
final082 rows, with boundaryperformanceN/A. Worker oldhandoff calls that
protocol bugH29; the actual mount acknowledgement mismatch is rootH30.
No historical raw receipt or claim is edited. Exact retained custody:
- 1cc25d4de / dedup-cdc-overwrite-1: containersce269da4e9dffab830e1229337c8de2df9bd3f97a3bb50a8de2833e9912456a6,f2f1ea0c8bb4c187a9cc2a2d12b4c98568693b7244b6e5c8233d0085c27f2e6a; volumelayerfs-fsb-sample-65f8d08d9719414e; old H29 mismatched mount acknowledgement; native/End/Gone/Stop unverified.
- 08288e824 / dedup-cdc-scattered-500: containersd54824de048bf87ea8afa5b0b2d1ef46f6ff6802e787e5a4565f1b29d78a3ee8,1dec045d2cbee6d3c86569fcc9a929bdd877b03fd0fb7296158296e125f471ee; volumelayerfs-fsb-sample-79578db1bf654f23; original41s proof work cutoff; remote completion and mount cleanup unknown.

Final handoff:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-edit-capped/layerfs/benchmark-results/fs-bench-pro/2026-10-11-batch-6-cdc-08288e824/family-handoff.json
SHAe334775b2c58a91f56c7f94116c243b784a89df3a8d656a412b3d38f48e1902d;
manifestc6a0dd78c333c2144ce40f1d422700f887a7450d7aabc37e0de97a006d0ef2b9.
Final performanceCSV47ff64872189413f208898bfa990a816f81b600e64b4f7ee3035433efc057347;
verificationCSV1fb3c44dd2625a87571d9cf4b1e89c4e08b39fec77ca5e57499d66065c154d15.
Read-only diagnosisSHA459c8c73934189b9f6eada3ab3e9223868e40dc4d7876356aa21c8e67aa90c1c.
Retainedverify1(exit1,20PASS/1FAIL) wall2,072,307,375ns; first report request
was refused by the private lock before execution while verify still ran;
report ran once after known completion. Both source pairs imported byte
identically, all21 cases beside their original references, no ratio/verdict.
Host Init paid daemon/FUSE request/job/statement axes UNAVAILABLE; verifier
telemetry separately scoped. Source/backing/metadata cache preconditions
unqualified and shared-host/DockerVM interference unquantified.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Record all21 CDC outcomes and retained original failed cohort | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### H34 bind unchanged repository helpers to actual acknowledged mount, 2026-10-11

Original repository stride3 setup failed at the unchanged15s docker cp
stop before any Fork/Mount/Exec/Commit: outer53,208,544,375ns, copied command
wall15,007,938,000ns. Host sealed code has78,416 members/863,044,487B:
78,352 checkpoint manifest/blob assets,53 complete oracleJSONs,11 other
helpers. It is separate setup, not the original300s per-state operation.
Single later bounded read-only metadata census sees70,171files/703,945,215B;
this is partial presence, not byte equality/continuous progress/remote
cancellation/success. No15s limit, copy tree, payload or workload is changed
to fit. Actual case remains failed before measurement, zero attempted
operations, proofNOT_RUN and remote custody unknown.

Stride10 at082 stages successfully, Forks and actually mounts/workspaces/1.
Its original storage-smoke-import then refuses 'target is not a FUSE mount':
controller's workload.sh default/workspace is wrong. Actual command_admission
directory is/workspaces/1 but noLAYERFS_WORKSPACE_ROOT was supplied. No
CommitACK exists. Outer34,387,208,041ns, one attemptedoperation/completed0/
proofattempt0, no outer120s expiry. KnownEnd/Gone/Stop are unobserved.

H34 uses the actual mount ACK event/key/incarnation/namespace/canonical
directory, once after active mount custody is recorded; missing/wrong
ACKs fail before commands and retain the published key. It supplies only
quoted scopedLAYERFS_WORKSPACE_ROOT to the unchanged original apply and
fresh native verify command bodies. No extra process/product operation,
new native source, payload, verifier, schedule, cache treatment or timeout.
Two synthetic valid-mount fixtures now return actual matching fields; every
original assertion remains. Parent external full17 RED fails with the same
wrong-target cause in186,347,917ns. Final worker9PASS592,418,542ns includes
17paid/17freshverify commands, tenbad paid ACK mutations and twofresh
negatives; root integrated9PASS744579667ns, eachprivate/main120s, noexpiry.
Sealed worker handoff555fdaaf9a218d7440d660cfe40d3042b3116da8d3c19478a16f3c09c4035a1f
and manifest3cbbb855a8b0634eef990b302298daed3817f6c6e7363b609b98e8625fbe4d16
bind original red, final tests, exact3files and stagedLOC/resulttree.

Preserve all original repository082 resources:
- stride3 runtime8eeb73a0e2833f6c6f0101b01287500c0eeb1ab146094c2a4a349199e4d763e7; volumelayerfs-fsb-sample-cb89e3a4c9f54c29; setup remote cancellation/Stop unknown.
- stride10 runtimed9cd853ee652bfdfe453c449ecb2edb743409b294d8f53381ece13323dbee107; volumelayerfs-fsb-sample-ffb7a6f707e74b56; actual mount451ef75d0cdbbbf65c4f03b24f0fa4e1a32c5abfc53902f4e9f56d3dd72d7f2e/ns1, noEnd/Gone/Stop inferred.

The full failed preparation and stride3 failed staging are unaffected and
will not run again. Only stride10 may receive one affected attempt at this
new executed controller identity. Product9ac/runtime7766/eight other compiled
assets unchanged, qualified seal reused without native build. Original
300s state/14400s family declarations remain visible beside overriding
owner120s whole-test ceiling; no original timer changed. Batch7 stays pending.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| Workera85f0ab97 | Actual repository root binding and external regression | 79370 | 79370 | +0 |
| This commit | Integrate strict repository actual-root binding | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### H35 actual NOT_RUN remains distinct from failed preparation, 2026-10-11

The immutable082 full-preparation administrative receipt recordsFAIL for the
preparation and explicit functional/proofNOT_RUN, all four attempt/sample/
completion counters0 and no benchmark observations. Generic report copied
the administrative status into actual case statusFAIL. H35 now reports
NOT_RUN only for the exact closure schema/kind, source/family/case/product/
image agreement, original expired124/120s preparation, absence of sealed
input/resourceclaim, exact integer zeroattempts and absent timing/count/
cache/storage/cleanup/verification fields. Any observed work, wrong source,
invalid mode or changed preparation declaration is refused. Original receipt
FAIL, error and raw preparation survive; no attempted failure is masked.

Original external two-test RED had27 failing vectors; actualFAIL-versus-
NOT_RUN mismatch is retained. Final two tests include27 negative mutations
and all metrics remainingUNAVAILABLE. Five existing proof-only and four
incomplete-context report checks retain their assertions. A misnamed
test_proof_only_reporting.py invocation ran0tests and FAILED(exit5), kept
separately; the actual owning files were then resolved and invoked once.
Exact command results under explicit120s R4:
- final-1.json: exit0, wall299602625ns.
- final-2.json: exit5, wall66233375ns.
- final-3.json: exit0, wall150066083ns.
- final-4.json: exit0, wall143866583ns.
- red.json: exit1, wall306992042ns.

Read-only actual original receipt classification now yields fullNOT_RUN,
stride3FAILsetup, stride10FAILactual wrongtarget. Old worker first generated
threeFAIL CSV bytes remain in ignored SHA-bound custody31ac274bdd86473c455a65a4edb318ffeee8ebeb6372149ce7b196f697722751
andf19bfea2e80fe7b4e1f5831244d55c2b3379dd0fb9e86f62562ad1d693b37ad1.
A fresh corrected report reads the same original run, without overwriting
those files/receipts or replaying any case; main will commit only its actual
corrected pair. Reporting-only code does not justify a workload repeat.
Repository affectedstride10 is running once at integrated08e1 identity,
using its unchanged sealed input and qualified assets; no batch7 work yet.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Report actual unrun case and retain preparation failure | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### Batch 6 repository original cohort retained, 2026-10-11

This commit records the original082 three-case cohort after corrected
actual-case report classification, with no case/receipt replay. Full remains
actualNOT_RUN after its sole failed120s preparation; stride3 remains
setupFAIL before any measured operation; stride10 remains actualFAIL on H34
wrong target before any Commit. The two source082 CSVs each include all
three original selections, their zero/unknown axes and exact source/evidence
paths. Both failed resources stay retained, as recorded above. Original
worker report's administrative threeFAIL CSVs remain byte-identical in
ignored custody; a fresh H35 reader report creates this main pair and keeps
full-preparation FAIL in its tag. No former file or receipt is overwritten.

| Case | Actual status | Proof | Timings/count/storage | Reason |
| --- | --- | --- | --- | --- |
| deepseek-full | NOT_RUN | NOT_RUN | UNAVAILABLE | Sole native recipe preparation exceeded120s before sealed input existed |
| deepseek-stride3 | FAIL setup, actual operation attempts0 | NOT_RUN | UNAVAILABLE | Original docker cp staging15s expired; no Fork/mount/workload/Commit |
| deepseek-stride10 | FAIL actual, operation attempt1/completed0 | NOT_RUN | UNAVAILABLE | Original importer used/workspace while actual ACK/workspaces/1 |

Exact source082 family handoff
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/benchmark-results/fs-bench-pro/2026-10-11-batch-6-repository-history-08288e824/family-handoff/handoff.json
SHA04b0ec26722d440e20da72c3fa69eaf564be1ee0a2aba2f7b8d30a11712ac2a6,
manifest02ead0faa5fcda0100d01c8ef71ba985e87fe013d9c4bb767b7b41bfd1c27ca2.
Case-ledger64127098eeab6407f1a241c627c1752716e9567fcce88b3e0bf8d5553db66c1f
binds all11 outer commands, exact preps/native assets/parity and actual
PID guards. Correction to an earlier worker note: actual host waits are
known (-9forstride3,1forstride10), expanded host RUSAGE_CHILDREN swaps
0→0PASS and observed current-PID maxima2,899,968B/3,211,264B. These do not
prove remote Stop/drain/Gone or a continuous/lifetime RSS peak.

Root fresh reader invocation:
`perl core/target/r4-locked.pl 120 python3 -B core/benchmark/fs-bench-pro/bench.py report /Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/benchmark-results/fs-bench-pro/2026-10-11-batch-6-repository-history-08288e824/run`
PASS(exit0), R4wall≤1s. Its independent stopwatch isUNAVAILABLE: unified
tool elapsed0.202109792s also includes preceding commit, never attributed
to benchmark or exact standalone report wall. Report reads immutable
receipts only, no product work; actual corrected CSV hashes:
- core/benchmark/fs-bench-pro/baselines/core/2026-10-11-08288e824/repository_history/performance.csv: fa7d4d392432c18532aac5a0063423251d09b322b9fdab5dfd6ed35eeff912c9,3rows.
- core/benchmark/fs-bench-pro/baselines/core/2026-10-11-08288e824/repository_history/verification.csv: f19bfea2e80fe7b4e1f5831244d55c2b3379dd0fb9e86f62562ad1d693b37ad1,3rows.

Original14400s family/300s per-state declarations remain distinct from
overriding120s owner test stop; no measured total is inferred from partial
outer/SDK spans. Source/product/corpus identities, failed full/parity row
and every existing Docker resource remain. Only affectedstride10 runs
once at08e1, still active; this is an old cohort commit, not batch closure.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Record repository original failed/unrun cohort | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### Batch 6 repository final failed closure and batch completion, 2026-10-11

Only affectedstride10 ran once at corrected08e1. The sole outer attempt
reached original owner120s ceiling, exit124/wall120,025,972,458ns. All17
original importer commands report actual exit0 and17 Committed ACKs, exact
SDK History EOF17, measured unmount/Gone, EndSession/native fencing and a
container-stopped event. The original controller published neither final
receipt/manifest nor finished event/known host wait/outer ACK; final native
verifier/public typed roots/immutable census did not begin. Last native
event181 is BrokenPipe after the outer stdout peer closed. Native progress
continues through final Commit3,640,069,583ns and terminal operations; no
wrong result/crash or identified fixable product correctness cause appears.
No limit extension, speed change or unchanged case replay is selected.

Fresh ADMINISTRATIVE supervisor supplement records attempted caseFAIL/
proofNOT_RUN/functionFAIL, sample1/attempt1/completedwholecase0/proofattempt0.
It binds original registration/selection/claim, outerinput+exit/streams and
native event hashes, with all headline benchmark timing/count/storage/cache
observations absent. Actual17ACKs and terminal observations remain
diagnostic, not qualified proof/completion/cleanup authority. Its schema is
fs-bench-pro-supervisor-unpublished-failure-v1, receipt_kind supervisor-failure;
it is explicitly written after the expired invocation, not a product or
controller success receipt. Single bounded administrative closure
98,459,417ns and report111,595,500ns PASS/private120s, no signal/control/remove.
A bounded hostps finds owned RustPID absent, but cannot manufacture an
authorized hostwait or final aggregate swap result.

Fresh supervisor receiptSHAf9a235e8fe19582556dccd2bf43b36f985d8fb6309fc66d342bcac29fd8af8db,
manifestefc0dd3e9cdd7f2567b5591cdfb97ccae00bf93633d1ab72c3d27b5e73b15a81.
Complete handoff:
/Users/yifanxu/.codex/worktrees/fs-bench-pro-namespace-mutation/layerfs/benchmark-results/fs-bench-pro/2026-10-11-batch-6-repository-history-h34-08e1fb910/family-handoff/handoff.json
SHA2e3a75ec3b684a1ca4266ce94e04c869407e5401c8384b013d46c8277534347e,
manifesta2065ae9602e041fd5c18a43e9e13c455dee6406c1dfa127e1b878fc0e1b24c5;
acknowledged-progress.jsona133f32bd23184cd251d9d8a8d2cb95b33d520dfe0cdcdc6cad525760dd140b1.
Final own pair retained: containerb78f735f3463bc4120b55c4ee7c0fd810220413b5adeea1b82bed2c82a8e28c7,
volumelayerfs-fsb-sample-688f2e1e16f64d6d; key/incarnation
f1216e0b40d40d1fa842f7a885751be28f7a30298617bceb3c132c45b14d01d5/ns1.
CustodyUNKNOWN/cleanupUNAVAILABLE remain; no removal or inferred proof.

This commit imports the unchanged08e1 three-row CSV pair:
performance846db6db688cb6ce4dd12a43a07d4437f127093fb12a7018e43619d53a96b68f,
verification01a49f23310d844262b975d646073a6ffd672155faaa795cd815c1b3f580076c.
Other two08e1 rows are defaultNOT_RUN, with no receipts/attempts; final
authoritative family selection explicitly reuses full082actualNOT_RUN
failedprep, stride3original082setupFAIL and corrected08e1stride10FAIL.
Later placeholders do not replace original retained failures. Both source
pairs and the old worker administrative report bytes remain visible.

Batch6 is now truthfully closed with54 original ported selections:
| Family | Cases | Full functional/proof/clean passes | Failed | ActualNOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| dedup_cdc_locality | 21 | 20 | 1 | 0 |
| dedup_cross_file | 10 | 10 | 0 | 0 |
| dedup_workspace_reuse | 14 | 11 | 3 | 0 |
| store_footprint | 6 | 4 | 2 | 0 |
| repository_history | 3 | 0 | 2 | 1 |

All source definitions/parity differences and every current/historical CSV
row are committed. Four families have original complete parityPASS;
repository individualstride3/stride10PASS, full/aggregateparityFAIL retained
under section9 failed-preparation exception. Original workload/verifier/
limits unchanged. H28/H30/H31/H32/H33/H34/H35 repairs were source-backed
harness bugs with external reds/covering checks; only affected treatments
ran once at changed executed identities. No batch6 product change or
optimization; product9ac remains79,370LOC. Original physical-exclusive
spool/peak axes and unknown paid cache scopes remain unverified/unavailable,
never zero/PASS. Shared host/DockerVM interference unquantified.

After batches1–6,245 original selections are ported/recorded:192 full
functional passes,50 failed outcomes,3actualNOT_RUN after failed preparation.
28 reliability selections remain pending. Stage0.4 exactcountsC10/C12FAIL
and all historical attempts stay distinct. This failed closure authorizes
batch7 under the unchanged iteration rules; it is not passing release evidence.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Record repaired repository attempt and close all54 batch6 selections | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c, last65,417/tree498dd1917812.


### Batch7 original reliability metadata admission, 2026-10-11

This commit ports all28 original IDs/order/registrations/reference rows.26
complete cases are source-backednot-applicable, with a precise reason, needs,
original action/state/ordinal/checkpoint/fault schedule and unchanged physical
guards. Their initial live proof unconditionally callsphysical_state; the
current product cannot express exact per-Workspace physical current and
mutation-event aggregate peak. No new product feature is selected. Other
per-case retired API gaps remain recorded rather than masked by a shared reason.
The two descendant-integrity paths bypass that guard in the original driver,
so their exact initial/fault/detection/cleanup schedules remain ported and await
controller/build/parity/native qualification. They makezeroCommits. The native
fixture contains23entries includingroot,22non-root,9directories includingroot,
9regularfiles,1hardlinkname,4symlinks and1MiB original nominal logicalbytes.

The thin shell executes the unchanged original native initial verifier or
read-corrupt helper only from the controller-authenticated absolute mount
root. All26not-applicable routes refuse before helper execution. No SQL,
workload alteration, additional family, product/dependency change or actual
proof attempt is in this metadata commit. Every case isverification-only,
sample0; each expressible proof getsoneattempt. Original45swork/4scleanup
reserve/59spublication/.25sguard/owner120sceiling and2CPU/2GiB/no-swap/256PID
plusnamed-spool2GiB/zero-final-ownedfiles guards stay explicit.

Imported owned metadata handoffSHA446b76e2f5d7ccd530e6b661b7011c43b176ecfaff42eaaebdc89f4324e18679
from the tiny private worktree, patchSHAe7ebb46514b1ef347b60f6e5d2efa0c5d86574450eb29751699170cbd6bc5482.
family.jsonSHAcd2caba87ef9f68fbde94d4ec29730c0dad76a0c683eda03eb1939ee55c33589;
workload.shSHA7f71ee98b102aebf0c9baa0f10d912f8ba1975f2df15625f3a332923a4ff2060;
owning externaltestsSHA c8b74428d20a65462a4f53a6adbd8a4fe182ba1bf91e6c5b2b797dabf9e38f5b.
Private pending/missing-shell expectedRED162,500,084ns is retained, finalsyntax
PASS18,275,958ns and5checksPASS1,481,015,375ns. Root final imported5owning
metadata/shellchecksPASS1,418,374,583ns underR4explicit120s. Raw exact command,
exit andstreams:benchmark-results/fs-bench-pro/2026-10-11-b7-main-metadata-checks/check.json.
These are metadata/fake-helper checks, not native/product proof.

Independent whole-case reuse reviewSHA0e107fbc472ab655611b285396800206e9f4f37e11169e4d73b600905e321db6
finds0exact whole current-case reuse; frozen release has27PASS plus sustained
600sNOT_RUN_OPTIONAL, which remain reference observations only. Original27/28
receipt paths absent locally cannot authorize a current proof. R8 completion
records/registrations/receipts remain untouched.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Port original reliability schedules and source-backed applicability | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Passing setup master disposal administration, 2026-10-11

Exactly89 old owned closed passing setup masters were removed, once each,
with no force/retry. SourceinventorySHA2c2f034b0de13ef51a1c2ebf37bc14ddc2c85c1ed2d4f772023bd8917468b744
qualified their creation, immutable prepared/provision witnesses, SDKKnownStop,
stoppedsetupreadback andsetupcontainerremovedACK. Before each action root
rehashed every cited metadata/evidence file, freshly inspected exact Docker
Name/Driver/CreatedAt/Scope against original snapshot and freshly required
zeroallcontainers connected to that specific volume. Each removal returned
exit0 with exactvolumeACK. All89 qualified, zero uncertain removal results.

Administration underrootR4120s completed≤7s; percommand5swallstop, one
attempt. These are administrative timings, not benchmark/test measurements.
No build/test/sample/control/force/signal ran; every failed/uncertain sample
resource remains. Allrawsetup andcasefiles remain; no mutatedsample reuse.
Genericempty/big64m andall19uncertain/failed/future masters were excluded.
Passing setup resources are no longer reusable Docker inputs; their raw
Store/fixture/receipt evidence remains. Current reliability uses its exact
originalsentinels/work/alias/symlink fixture, not one of these different trees.

Rawsourceinventory, exact89names and each freshinspection/connection/one
removal input, stdout/stderr, completion and disposition:
benchmark-results/fs-bench-pro/2026-10-11-owned-master-disposal/.
Finaldispositions.jsonSHA1d48715f96d8ed39211327be8e86c465f6412e025dc7e7f8e1b2b428d9302abf.
Retained candidates (no removal attempted):
- layerfs-fsb-history-master-055a6f6661654877
- layerfs-fsb-history-master-0df72c727aad49fb
- layerfs-fsb-history-master-1111924d4ce54923
- layerfs-fsb-history-master-155a78f97c524a2f
- layerfs-fsb-history-master-3eae8ec291cd495a
- layerfs-fsb-history-master-4b3d7b52eff1427d
- layerfs-fsb-history-master-5a359f0a232d4304
- layerfs-fsb-history-master-7a018e5791fe4f74
- layerfs-fsb-history-master-83b64d7980194c20
- layerfs-fsb-history-master-bc13a64b0c71471d
- layerfs-fsb-history-master-c6f37813fff648fd
- layerfs-fsb-history-master-ee5766588f4a4d83
- layerfs-fsb-master-big64m-c60eb32e5145496b
- layerfs-fsb-master-branch-development-35913130b8a24310
- layerfs-fsb-master-empty-4e709a73985e48b2
- layerfs-fsb-master-empty-95af702904c04049
- layerfs-fsb-master-empty-ee794ff64e0a4bcb
- layerfs-fsb-master-repository-history-710a07b5f29a42a1
- layerfs-fsb-master-repository-history-ae97db5087604bc7

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Record exact passing setup disposal and retained custody | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### H36 explicit source applicability report disposition, 2026-10-11

Source-reviewed unattempted reliability selections now reportproof_status
NOT_APPLICABLE and their exactnot_applicable_reason; performance staysN/A,
proofwallUNAVAILABLE andoriginal45slimitretained. Any actualrecordedreceipt
retains its ownproof_status; an unexpected actualFAIL withoutproof result is
NOT_RUN plusvisibleoverallFAIL, never masked asnot-applicable. No recorded
receipt/case/history changed; no workload/verifier/Docker/product execution.

Retained root RED evidence:
- 2026-10-11-b7-cli-report-checks/report-red: expectedNA vsoldNOT_RUN.
- 2026-10-11-b7-cli-real-metadata-red/report: actualmetadata declarednot_applicable_reason omitted fromtag.
- 2026-10-11-h36-na-report-checks/red: unexpectedactualFAIL maskedbyNA fallback,1FAIL/1PASS.
Finalowning2checksPASS107,251,500ns; existingproof-only5PASS141,671,625ns;
existingincompletecontext4PASS147,780,000ns. AllR4explicit120s/readreporttemplate.
One mistakenfilename test_incomplete_context_reporting.py discovered0tests,
exit5/68,750,708ns FAIL retained; actualscopedfile resolvedwithrg and invoked
once, no testexpiry/repeat/workload resample.

CLI realmetadata source-route RED also exposed its wrongreasonkey; root
fixedprospectively andawaitscomplete engine integration/owning final check
in its separate controller commit. That red is retained and noactualcase
was attempted. Current report repair needs no new product qualification.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Preserve explicit NA reasons and every actual failure in proof CSV | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### H37 exact integrity closed-copy installation purpose, 2026-10-11

The existing unchanged byte installation helper gains an optional
integrity_case, accepted only forsample=True and the two exact original
descendant IDs. It refuses all other cases/master use before creating a
folder/resource or calling Docker. New claim and installed-history scope
accurately record original integrityproof, independentfaultedclosedbytes,
no measuredInit/noadditionalInit andexactcaseID. The byte SHA/installed
manifest seals, sourcecopy, UID/GID/backing0:0/0700/0600, resource custody,
noInit, singleDockeroperation sequence andolddefault scopes remainunchanged.
No old receipt is edited or relabelled. This is harness metadata/route
support, no product feature or integrity success claim.

Rawowning qualification:benchmark-results/fs-bench-pro/2026-10-11-b7-install-scope-checks/.
Parent unsupported-routeREDexit1/wall168,862,083ns retained. First wording
checkpoint2FAIL173,931,542ns retained; code explicitlystatesbothno-measured
andno-additionalInitwithout changing guard. Final2checksPASS169,811,833ns;
existing14historicalfixturechecksPASS2,585,641,750ns. Allprivate-free rootR4
explicit120s, fakeDockeronly/readreporttemplate, noactualcase or build.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Admit truthful original integrity-copy installation scope | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Batch7 current public initial/RAW target/fault native support, 2026-10-11

Native host Runtime adds onlyverify-reliability-initial/control/fault;
workspacewriter adds exact originaltwo-case materialize/verify-native routes.
Complete original23paths/1,048,534regularfilebytes (nominalfixture1MiB
includesalias+symlink lengths), metadata, hardlinkclasses, symlinktargets and
completeinode-table membership are verified through existingpublic APIs.
Authenticatedfirstmappingtarget is sentinels/f0003.dat, current32768BWholeFile
inplace of retiredChunk. Existing record::encode_stored(Singleton),build_group,
assemble produce supportedRAWsingleton; parsedRAWframe determineslasttarget
byte. Completepositivecontrol reads exactcanonicalbytes before anyfault.
The faulthelper authenticatesallotheroriginaltree state withpriorcanonical
targetbytes explicitlysubstituted only for that failedobject; it doesnot
pretend the faultedtargetpassescanonical proof.

Missing exacttarget returnsObjectMissing(exacttarget). Corruptsingleton
acquisition firstauthenticatesfullpackSHA inport/read_pack.rs16–18, yielding
exactIoOther(innerPersistenceError::Malformed) throughstore/error.rs173–185
before canonicaldependency identity. Nativehelper accepts onlythis actual
downcast plus exacttarget/isolatedRAWlocator/one-byte audit; arbitraryIO
errors remain failures. This is supplemental FaultVerified, not accepted
originalearlyStore-open/mountsuccess. Fullcase still requires unchanged
ordinaryFUSE read-corrupt EIO5/zerobytes, End/Gone/KnownStop andalloriginal
resource/namedfile/runtimefile guards. No productfeature, SQLmutation in
Runtime, dependency/lockfile orrootfrozen-source edit. Python controller
owns exactclosed-copy SQL fault injection/audit separately.

Imported five ownedfiles fromnativehandoffSHA155ecc55f27fc5277784e9fc2aa172a17a3bb9bd09e9f5c280acfa3f7551f96d,
ownedpatchSHA1fe41aefdad386279cd8eb4e7ce95caa7917980bff1262a2ca74fadd34d2ee04
at cappedprivate359 source; no privatecommit. Native initialmissingroute
RED retained; firstno-run wrongcurrentBranchRecord field failed, corrected
tocurrenthead_commit withoutproductchange. Firstnative5checks4PASS1FAIL
0.17s retained (oldlate errorcategory); affected changedsource1PASS0.35s
followingno-run8.69s. PrivatefixtureCLI3PASS0.917s followingARMrustc6s,
fulltree/alias/SHA/wrongpayload/extraname/existingoutput/unrelatedcaseguards.

Rootfinalexactsource no-runPASS8,601,399,292ns; externalRusttarget22PASS
84,494,787,459ns/120s. Root omittedprivatefilter, so17additionalfrozen inline
contracts ran; dedup_branch_history::mixed_v2_contract logs>60s thenpasses.
No hang/repeat/limit change; existingharness8dead-codewarnings retained.
Read-onlyownedPID69010 stackdiagnosticSHA97ae53e41e091e9005e4628e561546889920713fc5a1df9a9f294ef6c39c44f3,
not benchmarktiming/causeproof. One premature recipebuild invocation was
R4_LOCK_BUSYexit75 after13,358,083ns, before compilation; no overlapping
build/test andnoautomaticretry. Finalsealednine-assetbuild will own the
actualnewrecipe compilation onceclean committed integration exists. Raw:
benchmark-results/fs-bench-pro/2026-10-11-b7-root-native-checks/.
No actualprepare/parity/proof/Dockerinvocation occurred inthiscommit.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Add exact original integrity native fixture and public proof adapters | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Decision77 complete reliability applicability correction, 2026-10-11

Two prospective integrity cases additionally cannot express the complete
original frozen resource profile through current publicSDK. SandboxCreate
and backendSandboxRequest expose image/volume/port but no CPU/memory/swap/
PIDlimits. SandboxApi::create delegatesprivateCreate→Upload→Start, with no
callback/barrier/profile or publicadoption/resume. BackendstoppedCreate/
upload/Start arepublic, but ManagedSandbox.command_identity/privateSDK
Docker/controlcomposition prevent adopting that stagedowner without
recreatingauth/Hello/wire-control ownership. ExactcurrentAPI/backend source
hashes and originalcontainer creation/readback pins are preserved in the
two NA declarations. StartingunboundedthenDockerupdate would weaken the
frozenfrom-creation2CPU/2GiB/no-swap/256PIDguard. No newfeature/proxy/wire
replacement or caseattempt is authorized.

All28 current reliability cases are now not-applicable. Former26 complete
physical-accounting cases arebyte-identical; onlytwo caseobjects change
status/applicability/reason/needs/sourcecapability pins. Their original
23pathfixture, four semanticfault/positivecontrol/EIO/cleanupsteps, XOR/
locator/zeroCommit/strictdetection descriptors and every budget/guard/oracle
remainverbatim. verification_attempts_if_applicable=1 isconditionaloriginal
obligation, not an actualattempt: actual current proofs/samples are0.
Full-native initial/RAW/helper diagnostics passed on host atf707, but cannot
qualify missing runtimeprofile/resource/FUSE/cleanup scope. No actual
reliability preparation, master/sampleDockerresource, parityproof or mounted
case was attempted. No sealednewnine-assetbuild is needed for source-only
NA disposition; new native adapter/Linuxrecipe seal remainsNOT_RUN.

MetadatahandoffSHA4af7a02e955aba21fc6fb4aaf15f2378672b100752edae860b350cfac33c6848,
manifest a5fb9d2bcd12d22dce9cb1d8563e0666d75587d6107a6c0737e105895367e9ef.
Finalfamily.jsonSHAf45e761b8d97e48975769341dc70bda0786539e7faf70cbe1b2a43a41be8c6d9;
shellSHA199a1f6b3fa7a6c98ce6c20dd0ea335ad9bc66fce0c857bec4d081263f6786e5;
externalmetadata/shelltestSHA03ae0cc0cf1ce6bd52afc7fc766698c2e158ff42005b99433030dcd27b8875d0.
New6checks RED againstformer2ported/helperroutes2FAIL1,057,987,750ns retained;
finalprivate6PASS742,265,541ns/syntaxPASS17,197,667ns. Root importedfinal
6PASS820,646,750ns/R4120s at raw2026-10-11-b7-final-metadata-checks.
All28 shellroutes refusebeforehelper; unknownaction/ID/arity64. These
checks preserve guards and are source-applicability tests, notcase proofs.
The rich prospective controller prototype is preservedignored/unqualified;
shippedengine will admit exactsource-onlyNAparity and refuseprepare/one_case
before resources. No test, verifier or product guard is weakened.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Preserve original resource guards and classify two SDK-inexpressible cases | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Batch7 source-only applicability controller and CLI, 2026-10-11

129-line shippedengine/reliability.py pins exact reviewed dictionaries for
all28 original cases, currentproduct9ac cleanstatus and every original/
currentpublic sourcebyte. Sourceparity records everycase statusNOT_APPLICABLE
andseparateapplicabilityPASS, withruntime_parityNOT_RUN/sample_count0/
verification_attempt_count0; changedsource/product/dirtytree/unknowncase/
changedreason/profile/fault/limit preserves completeaggregateFAIL evidence.
prepare/one_case refuseeveryIDbeforefolder/build/resource/native work.
Runtimeprototype794lines andexternaldraft287lines arepreservedignored
byte-identicaldff4d3.../b446dbaa..., unqualified; no lifecycle/SQL/fullcase
proofis claimed. ExacthandoffSHAe915a36a351ac9b4efb2b7bbb1326e7aa5401734ac01d8e588162149bb5ac3e8;
manifest4f5e23e47be286251c0e79d3f27309198fd831a3ebd865e5fc83a69b5ec1d38b.
ModuleSHAa55381dc4f2680d99fcde5d0a9ffe81b7ab5a810d95ad1af9766a40445dedad2;
owningtestSHA68b118fe4ec5830ae4c2db97e3e3e2014befa2bcda96e649b832b0fe5701fde7.

CLIrunsourceNA skip callsactualadmit_sources before recordingzeroattempts;
verifymissingNA receipts checks exact reviewedpolicy and printsreason; any
actualrecordedFAIL isprocessednormally, notmasked. Parity/prepareroute
ordinaryentrypoint unchanged. Root H36 wrongreasonkey RED againstactual
metadata retained; correctnot_applicable_reason nowused. Genericprospective
ported-route doubles pin originalb1cc metadata explicitly and are notcurrent
case capability/evidence; actualengine refutesall28 execution.

PrivateD77 RED145,477,792ns retained/final6PASS2,543,428,250ns. Root6source
guardsPASS2,697,263,666ns; CLI5PASS327,696,708ns; existing2parityCLI checks
PASS176,452,166ns. AllR4explicit120s/readreporttemplate. PriorrootCLI
checkpoint4PASS/1ERROR281,222,292ns used wrongtestmockname load_results;
actual load_run corrected in testonly, affected verificationguard1PASS
257,820,750ns. Finalall28 verificationguard checks precise reason,51→55
policyadmissions and unexpectedactualfailurevisibility. Priorprivate/root
REDs/failedchecks remain, no workload/limit/oracle/product change. Raw:
benchmark-results/fs-bench-pro/2026-10-11-b7-minimal-controller-checks/,
2026-10-11-b7-cli-real-metadata-red/,2026-10-11-b7-cli-final-checks/.
No actualpreparation/SDK/Docker/nativeverification or benchmarksample.
Formalclean source-only parity/registration/report follows thiscommit.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Admit source-only reliability dispositions through bench.py | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Specification applicability/schema clarification, 2026-10-11

Own speccommit names engine/reliability.py insection4/table6 and makes
source-onlyNA parity explicit insection7, distinctfrom nativefixture/runtime
proof. It clarifies verification-only performanceN/A versus actualproof/
NA status inverification.csv and absenttimingUNAVAILABLE/zeroactualattempts.
The original layout omitted this missing-capability routing, and statusvalues
omittedN/A despite designatedverification-onlyfamilies/releasedproof-only
format. Fullcurrentcase guards/limits/physicalobservability/creationprofile
requirements andoriginaloperations remainmandatory. No case oracle, limit,
workload, numerical cacheclaim, receipt/verdict, CSVcolumn or productfeature
changes. All earlier source-onlyf560 parity/registration/report evidence is
retained atitsactualidentity; docsclarification doesnot replay a case or
reclassify a measured result. This is specification clarity, not a passing
reliability-proof or comparison assertion.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Clarify honest applicability and proof-only reporting in the spec | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Batch7 source-only parity and every-case report closure, 2026-10-11

Cleanmainf56023fe49b80fc3e333d0db8ee16ce3d262a397/product9ac supplied all
28originalcase selections. Formalbench.py source-applicabilityparityPASS
1,373,399,833ns; fullNA registrationPASS2,077,674,875ns; retainedsource
verificationPASS147,991,875ns; reportPASS99,785,042ns. EachinvocationR4
explicit120s/readreporttemplate/workers1, fullcommands/streams/status kept
inbenchmark-results/fs-bench-pro/2026-10-11-b7-source-disposition/. These
are administrative sourcechecks, not benchmark/product timings. Parity
recordsall28runtime_parityNOT_RUN, sample_count0/fullverificationattempt0,
exact originaldictionary/frozen-current-source/profile/reason/needs.
No currentreliability Storeprep/master/sample/mount/workload/Commit/native
fullcase verifier, daemonresource observation or actualcase receipt exists.
No newbuild is admitted or required forsource-onlyNA disposition; the CLI
build/prepared arguments explicitlyname NOT_BUILT_SOURCE_ONLY/NOT_PREPARED.
Nativeinitial/RAW/integrity supplemental adapter checks remainseparatehost
evidence; no exactoldwholecaseproofreuse is claimed.

The two generated28-rowCSV files retainalloriginalIDs/order/current source,
product andpinnedimage. Performance/status/time/count/storage/limit/cache
cellsN/A becauseverification-only; verificationproof_statusNOT_APPLICABLE,
proofwallUNAVAILABLE, originalprooflimit45,000,000,000ns andexactreason tags.
- baselines/core/2026-10-11-f56023fe4/workspace_reliability/performance.csvSHAc83ba73522934d797c816009933fe2dbd99cf12442947b5b8d738d60195a16aa.
- baselines/core/2026-10-11-f56023fe4/workspace_reliability/verification.csvSHA7b5c525bb2117da4d4f94b7ba215b8e4f8b95a0c99a6316a59b4fd4c3c1b2282.
Readerchecks exact28 IDs/source/product/dispositions/absenttiming/limitsPASS,
no case replay. SourceparityPASS never meansnative/runtimeverifierPASS.
Releasedrows below are originalv0.1.5 verification references (27PASS plus
oneoptionalNOT_RUN), notcurrentproof/timing or acomparison.

| Original case | Current complete-case status | Released proof | Released proof wall seconds (original string) | Missing complete-case capability / needs |
| --- | --- | --- | --- | --- |
| workspace-invalid-sdk-edit-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.4101969999901485 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: Missing public SDK atomic file-range edit/rejection; ordinary pwrite beyond EOF extends a file, and a harness prevalidation rejection would not prove product rejection.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: Missing public SDK atomic file-range edit/rejection; ordinary pwrite beyond EOF extends a file, and a harness prevalidation rejection would not prove product rejection. |
| workspace-invalid-namespace-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.4250047920213547 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-lease-lifecycle-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.5743714999989606 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: Current EndSession ends only Control session; no Workspace closes. Bound Workspace/Sandbox owners have explicit terminal unmount/stop, not the old Client-drop lease-release contract.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: Current EndSession ends only Control session; no Workspace closes. Bound Workspace/Sandbox owners have explicit terminal unmount/stop, not the old Client-drop lease-release contract. |
| workspace-open-writer-busy-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.7236085000040475 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-live-execution-busy-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.356204000010621 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-candidate-failure-retry-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.376748291979311 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: Current public SDK has no exact Candidate fault arming or hit-count receipt. Busy, transport loss or arbitrary IO failure cannot substitute for this production boundary.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: Current public SDK has no exact Candidate fault arming or hit-count receipt. Busy, transport loss or arbitrary IO failure cannot substitute for this production boundary. |
| workspace-admission-batch-failure-retry-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 3.511054790986236 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: No public production later-admission fault/receipt interface, including exact once and early-transaction count.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: No public production later-admission fault/receipt interface, including exact once and early-transaction count. |
| workspace-final-publication-failure-retry-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 3.9517992500041146 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: No public exact final-publication fault injection or reachability receipt.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: No public exact final-publication fault injection or reachability receipt. |
| workspace-published-presentation-failure-smoke-v3-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.156578291003825 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: Current public Commit returns typed publication/install custody, but no retired PresentationResume switch/status bit/recover_workspace_presentation API.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: Current public Commit returns typed publication/install custody, but no retired PresentationResume switch/status bit/recover_workspace_presentation API. |
| workspace-dirty-end-discard-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.3135516670008656 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: Current public terminal Unmount has no Clean-vs-Discard End mode or exact WorkspaceDirty rejection contract; EndSession closes no Workspace.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: Current public terminal Unmount has no Clean-vs-Discard End mode or exact WorkspaceDirty rejection contract; EndSession closes no Workspace. |
| workspace-dirty-net-zero-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.509194040991133 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-short-spool-write-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.3350951250176877 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: No public remote append fault arming/receipt, no exact retired spool_segment_bytes rollback counter; ordinary filesystem exhaustion or simulated user error cannot prove ambiguous owner append rollback.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: No public remote append fault arming/receipt, no exact retired spool_segment_bytes rollback counter; ordinary filesystem exhaustion or simulated user error cannot prove ambiguous owner append rollback. |
| workspace-deferred-nospace-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 1.9623064159823116 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: No public exact append NoSpace fault/reachability receipt or retired segment accounting; cannot replace with arbitrary quota/resource failure.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: No public exact append NoSpace fault/reachability receipt or retired segment accounting; cannot replace with arbitrary quota/resource failure. |
| workspace-workload-cancel-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.748615582997445 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: Existing Docker ExecHandle.cancel explicitly refuses Unsupported; no public equivalent old per-Exec stop with qualified descendant termination. A separately qualified ordinary external executor may cover process semantics, but its existence is not a full frozen SDK receipt proof.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: Existing Docker ExecHandle.cancel explicitly refuses Unsupported; no public equivalent old per-Exec stop with qualified descendant termination. A separately qualified ordinary external executor may cover process semantics, but its existence is not a full frozen SDK receipt proof. |
| workspace-dirty-runtime-disconnect-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.6058751250093337 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: No verification_disconnect_workspace_execution API or original InfrastructureLost output-reader state; closing an attached stream does not terminate process/descendants or establish that state.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: No verification_disconnect_workspace_execution API or original InfrastructureLost output-reader state; closing an attached stream does not terminate process/descendants or establish that state. |
| workspace-corrupt-descendant-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.156250125000952 | The original runner creates the sample container with 2 CPUs, 2 GiB memory, memory-swap=memory (zero swap), and 256 PIDs before Start, then independently validates that exact HostConfig. Current public SandboxApi::create performs private Create, Upload, Start and authenticated startup as one SDK lifecycle. SandboxCreate/SandboxRequest has no resource-profile input, pre-Start callback or public authenticated adoption of a separately created stopped container; the backend HostConfig omits all four limits. A post-Start Docker update cannot preserve the original from-creation profile, and recreating private wire/authentication/Hello ownership or adding a product feature is prohibited. The original early integrity path bypasses physical_state, but that does not remove this mandatory container-profile guard.; Needs: A supported public SDK resource-profile input applied during the original stopped-container Create before Start, or an explicitly supported public pre-Start/adoption lifecycle preserving the authenticated owner, with exact original 2 CPU/2 GiB/no-swap/256 PID creation-time readback. This product/API capability is absent and new features are prohibited in this iteration. All original 23-path fixture, exact target fault, canonical/EIO and cleanup/resource proof obligations remain required; native helper or format-aware partial proof alone cannot qualify the complete case. |
| workspace-missing-descendant-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.092963624978438 | The original runner creates the sample container with 2 CPUs, 2 GiB memory, memory-swap=memory (zero swap), and 256 PIDs before Start, then independently validates that exact HostConfig. Current public SandboxApi::create performs private Create, Upload, Start and authenticated startup as one SDK lifecycle. SandboxCreate/SandboxRequest has no resource-profile input, pre-Start callback or public authenticated adoption of a separately created stopped container; the backend HostConfig omits all four limits. A post-Start Docker update cannot preserve the original from-creation profile, and recreating private wire/authentication/Hello ownership or adding a product feature is prohibited. The original early integrity path bypasses physical_state, but that does not remove this mandatory container-profile guard.; Needs: A supported public SDK resource-profile input applied during the original stopped-container Create before Start, or an explicitly supported public pre-Start/adoption lifecycle preserving the authenticated owner, with exact original 2 CPU/2 GiB/no-swap/256 PID creation-time readback. This product/API capability is absent and new features are prohibited in this iteration. All original 23-path fixture, exact target fault, canonical/EIO and cleanup/resource proof obligations remain required; native helper or format-aware partial proof alone cannot qualify the complete case. |
| workspace-parallel-read-write-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.195508415985387 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-shared-path-contention-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.158009000006132 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-hardlink-alias-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.4398577910033055 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-symlink-semantics-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.3873098749900237 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-open-rename-unlink-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.2619809999887366 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-metadata-chmod-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.4940547500154935 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-metadata-mtime-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.2410823329992127 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-metadata-xattr-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.0055110410030466 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-exec-500-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 8.90395483301836 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-repeat-publication-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | PASS | 2.4142839169944637 | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. |
| workspace-sustained-600s-compact-v2-proof | NOT_APPLICABLE;0samples/0proofs | NOT_RUN_OPTIONAL | UNAVAILABLE | The original initial live proof unconditionally requires exact per-Workspace physical_spool_allocated_bytes and mutation-event physical_spool_peak_bytes as Some, observation_errors=0 and both <=2GiB. Current shared-overlay public accounting does not expose these physical axes; no sampled shared database size, logical count or lifetime peak substitutes. Additional original scope: Independent policy blocker: unchanged mandatory600s active duration cannot finish under owner120s whole invocation ceiling; released selected row is NOT_RUN_OPTIONAL/long_test_exclusion. This does not mean ordinary long execution is unsupported.; Needs: Exact original-scope per-Workspace physical current/peak/error observability and a public proof interface are required. Adding that product observability feature is prohibited in this iteration. Additional requirements: Independent policy blocker: unchanged mandatory600s active duration cannot finish under owner120s whole invocation ceiling; released selected row is NOT_RUN_OPTIONAL/long_test_exclusion. This does not mean ordinary long execution is unsupported. |

Allsevenbatches are nowtruthfullyclosed:25originalfamilies/273cases,
192fullfunctional/proof/cleanpasses,50failedoutcomes,3actualNOT_RUN after
failedpreparation and28source-backedNOT_APPLICABLE. The first245 source
selection andeveryoldfailed/ineligible/unrunrow staysunchanged. There is
no identified fixable product correctnesscause left from retainedreceipts/
source review; watchdog/strictfrozen-oracle/legalrefusal andmissing-capability
limits remainexplicit. Stage0.4 exactcountsC10/C12remainFAIL. No speed
optimization/workload/limit/cache/workerchange or release/comparison verdict.
Completiondocument/finalretainedresourceclosure follows; no extra sample.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Record all28 reliability applicability rows and close batch7 | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Final exact generic-master custody refusal, 2026-10-11

B7allNA resolves the potential need for three generic masters. Root verified
everyoriginalownership/provision/SDKKnownStop/setupremoved witnessSHA,
freshly inspected exactName/localDriver/localScope/nooptions andzero
connectedcontainers, thencreated onepinned-image/read-only/no-network helper
permaster. The originalstrictno-sidecar guard failedexit1 beforehashing
for allthree; no StoreSQLopen/write, FUSEmount, SDK/test/benchmark or global
volume mutation occurred. No helpers ormasters were deleted/retried.
WholeadministrationR4≤2s/120s, eachoperation5swallstop. These are
administrative observations, notcase failures, productbugs or benchmarktimes.

Subsequentoneboundedreadonlyinspectandglobalcopy perexactfailedhelper
returnedexit0: eachStateRunningfalse/ExitCode1/OOMKilledfalse, main172,032B,
0-byteWAL and32,768-byteSHM present. This diagnoses strictsidecar admission
refusal, not a productwrongresult/crash or successfulclosed-copy proof.
Hashingcopiesforobservation doesnotpromote thefailedoriginalprobe. Source
diagnosisSHA12465eb830716f6dd2f9a11f61927397353e3d963a6570747eb1fef2433d8409.
Rawallcommands,creationidentity, readonlymounts, streams, deadlines,
statuses andsourcecopies underbenchmark-results/fs-bench-pro/2026-10-11-final-generic-master-disposal/.

| Master volume | Retained exact failed own helper | Recorded disposition |
| --- | --- | --- |
| layerfs-fsb-master-big64m-c60eb32e5145496b | 2f208a9e9397b6e2b76881a30fc3bc6fc8612e10eb5ee4427239a9370b557297 | Knownexit1; strictsidecar proof refused; retainboth |
| layerfs-fsb-master-empty-4e709a73985e48b2 | 7c373fc93595f84340ba236ee8dc8dab380f4be491ad128bbe7a3a8d4c1beee2 | Knownexit1; strictsidecar proof refused; retainboth |
| layerfs-fsb-master-empty-ee794ff64e0a4bcb | 3672d824e5e3440d82839372f9922f8c6b33b446e8b601d63613e2fcd857573b | Knownexit1; strictsidecar proof refused; retainboth |

Previously89admitted passing setupmaster removals remain89; all19excluded
masters stayretained, with future-only reason for thesethree superseded by
thisactualfailedcustody admission. Allfailed/uncertaincase/sample/prep/
qualificationresources remain recorded. Source-only retainedresource
inventorySHAe03e7d6d7f469f2ab9dd9d959336ae182462c4ebe535c37dbd4fbc33a59f9ae3
fromtinyignoredresource-inventory-f572354ff enumerates82containeridentities/
92volumenames,50authoritativefailedcases/19earlierfailedattempts andstage0/
H15/H20/preparedmaster custody. It excludes242exactpassinghelperremovalACKs
and89removedmasters; currentexistence isnotinferred. Addthethreeabove
newhelpers (85recordedcontaineridentities total), same92volumenames.
Completefinaldocument willpreserve eachidentifier/reason/evidence and
unknownacquisition/Stop fields, without anyforeign/protectedresourceaction.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Preserve refused final master custody and exact retained probe identities | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.


### Iteration1 final completion, 2026-10-11

ITERATION-1-COMPLETION.md records all25originalfamilies/273authoritative
case outcomes:192functional/verifier/clean-closePASS,50failed,3actual
NOT_RUN after solefailedpreparation and28complete-caseNOT_APPLICABLE.
It includes everycase/source/cleanupstatus, all53failed/unrun reasons,
all28NA needs/originalverification references, six authoritative/four
historicalsoftlimitmisses, everybug/cause/fix/regression, all80decisions,
155prior mainLOC statements+thiscompletioncommit andeachretainedresource
identity. H27/H28 source-backed fixes fromdatedappendsections nowalso
appear in thefrontbugindex; historicalreceipts/details areunchanged.

First245 cases use sealedmap1f3756bab0ae61eebc23d2f0566be009122cacf1c8e4fcc40e653e04fb998cde;
all40oldCSVpairs and154historical/defaultrows reauthenticated unchanged.
B7f560 adds28NA rows, total41pairs/427rowsperCSV. Oldpilot7scopedBlabels
keepfullnumericalqualificationUNVERIFIED; no newcold/speed/comparison
claim. Stage0.4C10/C12exactcountsFAIL, frozenoracles/limits/legalrefusals/
unknownterminal/physicalscopes/prepared-inputtestexceptions/OWNER-3/noCI/
noDurable/newnativeLinuxNOT_RUN remainexplicit. OnlyproductH15+4;
no newfeatures/optimizations/family/unchangedrepeat/limitchange orforeign
resourceaction. Existingrawreceipts/inputfiles/threefamilyworktrees remain
forprimaryevidence. All89passing master removalACKs and85recordedretained
containerIDs/92volumenames, including3failedreadonlyprobes, aredocumented.

Scoped document/source/CSV crosscheckPASS271,408,250ns/R4120s/readreport
template:25families/273exactuniqueIDs/statuscounts,36uniquebugs(H29alias),
decisions1–80,41sourcepairs,155mainLOCstatements(+4onlya05ed6027), all
relativefilelinksresolve andeveryoldCSVhashunchanged. Raw:
benchmark-results/fs-bench-pro/2026-10-11-iteration-completion-authoring/validation.json.
No product/runtime/SDK/Docker/testsuite/benchmark replay occurred. One
initialdocumentationgeneration snippet had anunterminatedPythonstring
andwrote noartifact; correctedretainedgenerator produced thecomplete
document. This is documentauthoring, not a sampledcase orproductbug.

The owner's stoppingcriterion ismet by truthfulclosure: allremaining
cases areexplicitfailed/unrunafterfailedprep/NA, withnoidentifiedfixable
product correctnesscause left. Thecompletiondoesnotclaim allcasespassed,
exactcountreproduction, numericalperformancequalification orreleasegate.
CurrentbenchmarkREADME anddocumentationindex linkthecompletion and
replace stale82pending/batch1–5 status; earlierproposalskeep theirhistory.

| Commit | Purpose | Production before | after | delta |
| --- | --- | --- | --- | --- |
| This commit | Complete the273-case iteration handoff and document every remaining limit | 79370 | 79370 | +0 |
Pinned absent-aware staged counter/exact tree confirmation; reference absent,
retiredR9c,last65,417/tree498dd1917812.
