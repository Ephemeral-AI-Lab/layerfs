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
