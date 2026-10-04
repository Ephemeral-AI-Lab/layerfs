# Exact initial pooled-ordinal reservation coalescing

Status: deterministic mechanism/correctness checks PASS; matched speed pending.
Based on a0201bfef. Prior sealed Init1000 count evidence:8reserve calls,
9publications,20write commits. Original first4pooled demands each reserve an
exact range to keep small-save physical ordinals dense before16leaf-block policy.
The same demand can be known before admission when the synchronized index is
empty; a count-driven test now checks four leaves with overlapping values.

Before the ordinary admission loop, a wave with zero prior ordinal demand
synchronizes the existing pooled index. A nonempty index keeps original per-leaf
reuse/window decisions. An empty index looks ahead at no more than4new/unlocated
inode leaves, counts distinct values exactly and acknowledges one reservation.
Ordinary leaf processing still assigns in the original order and performs
full value equality through its existing index. Distinct demands coalesced by
the call advance the original4-demand switch; diagnostics count one actual call.
Existing later16-leaf blocks, final unused-tail release and ownership remain.

The new temporary owner contains<=660fixed73-byte values (<=48,180value payloadB,
collection overhead separate) and one decoded<=165-row leaf. It is released
before ordinary admission; no input-size-proportional owner or payload copy is
added. Existing4MiB/512object wave,256KiB lane framing,131,072value index, body/
transaction limits, one producer, WAL/FULL/fullfsync/cache profile unchanged.
The helper does not retry, guess an unacknowledged range or recycle consumed IDs.
Known and uncertain failure handling is the existing persistence path.

Deterministic tests: four new leaves8row occurrences/5distinct values use1actual
ordinal reservation, persist dense ranges(1,2),(3,1),(4,1),(5,1) and next_ordinal6;
canonical leaf readback matches. A later leaf reuses all5values with0new
reservations. A preexisting acknowledged7-ordinal allocation is respected: new
value groups start8 and allocator ends10, not1. Existing6transaction/cache/queue
checks,3reference acquisition checks,9atomic publication/failure/profile checks,
and1full100/1000namespace oracle PASS (21unique tests). Storage/persistence/project
all-target warning-denying Clippy PASS, Core fmt --all --check PASS, production
boundary439files and23guard self-tests PASS. The first combined cargo test
selector selected the publication target only; the namespace oracle then ran
explicitly. No unchanged passing test suite was repeated.

Prospective next measurement: one fresh matched release/locked Init1000-v2 pair
at the committed code/harness identity. Same15scomplete-command/9.5sseparateproof,
cold source-content attestation, freshStore create/import/checkpoint/allocation
release/close product clock and candidate final allocation<=matched baseline.
Hypothesis: coalesced initial demands reduce actual acknowledged reserve/write
commit count. A smaller-case count proof is not a speed PASS. The current1.657x
failed Init1000 comparison and all earlier failures stay historical; remaining
threeInit and threehistory selections still required, NOT_RUN at thisnewidentity.
The full17-state reference count diagnostic's25sbudget failure is unchanged.

Exact first-parent/staged/committed production LOC comparison will be recorded
in the code commit message and handoff with migration subtotals, using the
existing tools/production_loc.py through target/phase7-agent/commit_loc.py.
Architecture05-storage records the algorithm and new fixed temporary owner.

## Matched Init1000 at e0149ce56

Complete product140,071,000ns reference /204,476,000ns candidate; ratio1.459802529,
timeFAIL:10*candidate2,044,760,000 >11*reference1,540,781,000. Root equality,
independent sampled proof,cold-content eligibility,cleanupPASS. Final allocated
SQLite total23,101,440B reference /20,561,920B candidate passes no-growth gate.
Complete envelopes219,275,584/1,722,106,459ns fit15s; separate proofs
58,047,750/47,328,917ns fit9.5s. Candidate checkpoint3,293,625ns is inside the
product clock; main20,529,152B allocation is unchanged by release in this sample.

Actual candidate307statements/203,498VM steps,33transactions/18write commits,
0rollbacks. C2reserve5/ordinal-reserve3/publication10,93body INSERTs/20,125,527B,
0pack reads,0payload rereads. COMMIT spans98,675,667ns, overlapping SQL/registration
spans, not physical sync syscall counts. Prior22e5f1d3c count evidence had
reserve8/write commits20/publications9/body INSERTs95. Coalescing reduces
reservations as intended but physical grouping introduces one extra publication.
These are count differences; the prior228.7ms and current204.5ms windows do not
establish a controlled cross-window speed improvement. No timing or cache rule
changed to pass, and this pair stillFAILs its declared speed gate.

One sample perarm/frozen identity; no retry. Raw folders
issue302-ordlook1000-{baseline,candidate}-treatment1 and comparison-treatment1,
compact receipts under checks/sqlite-ordlook1000-*. Reproduce through runner.py
run --case phase7-sqlite-init-1000-v2 --arm baseline --baseline-root
target/phase7-baseline/layerfs --out <fresh-owned-output>, then --arm candidate;
read benchmark_agent_report.md before each. Prepared source identity reused,
release/locked worktree-local builds and immutable archives/seals recorded.
Other3Init and3history rows NOT_RUN at thisidentity; all earlier nonpassing rows
remain visible and required. All-seven goalACTIVE. Next: examine the conservative
pack-ID wave bound and publication frequency, preserving private-base first-writer
acknowledgement, existing physical/canonical caps and durability profile.

Code commit productionLOC137677->137743(+66), reference65417/core72260->72326,
active28216->28282/inactive44044; migration old191/new7806->7872/rest64263.
Exact committed tree/first-parent comparison confirmed. This evidence-only
follow-up has unchanged production total137743/delta0, to be confirmed with the
same snapshot counter before commit. No push/PR/merge or CI claim.
