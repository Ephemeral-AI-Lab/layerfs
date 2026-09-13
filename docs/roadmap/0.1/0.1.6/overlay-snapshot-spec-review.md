# Snapshot-Isolated Workspace specification: adversarial review

Date: 2026-09-14. Target: [overlay-snapshot-spec.md](overlay-snapshot-spec.md),
governed by [overlay-snapshot-rule.md](overlay-snapshot-rule.md). Context:
[architecture and source review](overlay-snapshot-architecture-design.md).
Source baseline: `0814cc37f1dafb6041930c74489107f4a5035a26`.

## Current owner decision

After this review, the owner rejected setting new numerical performance gates at
this stage. The specification now requires correct implementation followed by
applicable existing benchmarks and honest inspection of results. The review's
proposed capture ceilings, sample populations and SW/MW families are withdrawn
as requirements. Existing benchmark contracts and correctness/resource obligations
remain unchanged. Historical discussion below records why measurement can mislead;
it must not be read as authorization for a new mandatory campaign.

## Review scope and verdict

Three subagents drafted storage/snapshot, Commit/staging, and performance sections.
A separate adversarial round reviewed the assembled contracts across those
boundaries, concentrating on complete snapshot acquisition, foreground write
throughput, successive Commit locality, and hidden retention/work. Follow-up
reviews checked corrections, including a second zero-range counterexample that
invalidated the initial attempted fix.

**Verdict: detailed, adversarially reviewed specification draft; not ready for
complete supported-surface implementation or release qualification.** The concrete
textual defects below were corrected, but review is not a substitute for the V1-V4
kernel/backend/algorithm/outcome obligations. V5 is later existing-benchmark
evaluation, not an added design blocker. No performance sample,
live FUSE test, or product implementation was run/changed in this drafting turn.
The document specifies resulting behavior and shape, not an implementation plan.

Reviewers:

- `review_storage`: storage/ownership author; adversarial review of correspondence,
  publication, range retention, and complete filesystem visibility.
- `review_commit`: attempt/staging author; adversarial review of root installation,
  source ownership, change indexes, replay bounds, and reclamation reserve.
- `review_performance`: performance author; adversarial review of host round trips,
  COW amplification, interval matching, sample populations, and hidden work.

## Findings and dispositions

"Corrected contract" means the text now addresses the identified counterexample;
it does not mean the mechanism is implemented, fast, or qualified.

| ID | Severity | Attack / concrete counterexample | Specification correction | Final disposition |
| --- | --- | --- | --- | --- |
| A01 | High | Two different-file writers prepare R_A/R_B from R0. Both per-inode revision checks pass; installing R_B after R_A loses A. | PreparedMutation owns complete source-root identity/lease; compare the entire root; reprepare/rebase on conflict. Sequence s+1 and index keys are prepared before the atomic swap. | Corrected contract; retry fairness and actual allocation costs remain V2 proof. |
| A02 | High | Source root is replaced/reclaimed while another mutation is preparing child/range references. | Hold a source-root lease through preparation; candidate owns children/backing before installation. Capture retains already registered root ownership, without disk refcount I/O under the root lock. | Corrected contract; graph ownership algorithm still unqualified. |
| A03 | High | A latest-by-key tree requires scanning history; a sequence append tree retains every action. | Atomic `change_by_key` plus `(sequence,key)` secondary index; replace superseded secondary row; snapshot both roots and range-scan captured `(b0,b1]`. | Corrected contract; bounded disk/index implementation required. |
| A04 | High | After C1 deletes original-base `/a`, covered cleanup removes its mask and live lookup resurrects `/a`. | Separate overlay binding tombstones from change-index bookkeeping. Covered-key cleanup cannot remove a still-required live-base mask. | Corrected contract. |
| A05 | High | Greedy universal-zero matching: C1=[huge A][zero:1], S2=[new zero][A][zero]. Prefix steals suffix match and forces A backward/reconstruction. | First select nonzero lineage anchors, then match zeros only in gaps bounded by those anchors. | Corrected after two iterations; targeted reviewers found original counterexample closed. |
| A06 | High | First attempted cursor-only fix also fails: C1=[A:1][zero:L][B], S2 deletes A. Cursor is at nonzero A, so unchanged zero:L becomes replacement/CDC. | Allow bounded old-gap traversal before the next surviving nonzero anchor; skip deleted nonzero prefixes and reuse zero runs inside that gap. Explicitly start gaps after previous-anchor ends. | Corrected contract; both targeted reviewers found no further tiny-edit amplification within stated ordinary operations. |
| A07 | Medium | Repeated/overlapping physical-source occurrences require O(Po) candidate scanning per descriptor under unspecified interval lookup. | Nonzero logical occurrence lineage has disjoint origin-coordinate intervals; floor/lower-bound seek plus forward overlap iteration. Equal-content duplicate occurrences have new lineage. Count all visited intervals and emitted fragments. | Corrected for the stated invariant; adapter/provenance proof remains V3. |
| A08 | Medium | Fresh origin per 4-KiB append produces 100,000 descriptors when contiguous spool could coalesce. | An origin can extend into never-before-used coordinates; coordinate reuse/duplicate occurrence requires fresh lineage. Zero descriptors coalesce and use anchor alignment, not per-write zero history. | Corrected contract; use relevant existing append workloads to observe normalization and metadata cost. |
| A09 | High | Host acknowledgment plus immutable page paths makes 10-us writes become 1-ms writes; a 3-ms allowance per call hides ~100x slowdown over 100,000 sequential writes. | Inspect total wall/CPU and throughput using existing suitable workloads; do not let per-call noise hide cumulative cost. The proposed mandatory SW/MW schedules were withdrawn by the later owner decision. | Measurement loophole closed; actual host RTT/COW costs remain unqualified. |
| A10 | Medium | Whole-root retry fixes correctness but can multiply COW/refcount/I/O work or starve writers. | Explicit retry factor r, attempts/conflicts/abandoned allocation counters, source leases, fair bounded rescheduling obligation and disjoint-writer throughput case. | Omission corrected; exact fairness policy is still V2. |
| A11 | Medium | Correspondence built after publication hides a million-record finalization before acknowledgment/C2 despite fast capture. | Complete correspondence and its root before staging. Post-publication receipt is bounded context/root replacement only. | Corrected; no deferred large completion escape. |
| A12 | Medium | Repeated public Commits on one pre-mutated Workspace measure D=0 after first Commit; n3 cannot establish p95 at D=1,000,000. | Report actual relevant D and statistics supported by the existing workload/sample count; do not pool internal probes with public results. Proposed new fixed n100/n3 populations were withdrawn. | Measurement caution retained; no new sampling campaign mandated. |
| A13 | High | One live byte of a 64-MiB logical extent pins the complete extent; relocation copies it all. | Separate logical origin from physical blocks; <=4-KiB indivisible retained payload units, interval/block ownership, move only live blocks, no unchanged-range per-block loops on tiny edits. | Corrected contract; physical backend release and indexed interval algorithm remain V2. |
| A14 | High | Full physical quota prevents relocating mostly dead segments because no destination space remains. | Protected charged recovery headroom for <=64-KiB move units and bounded metadata/read overhead; normal admission cannot consume it. Actual free-list space and filesystem allocated bytes measured separately. | Corrected contract; host release mechanism still must be selected/proved. |
| A15 | Medium | Append reply lost; replay after result eviction applies append twice, or retaining every result grows indefinitely. | Bounded transport-session sequence window, retained unacknowledged results, cumulative watermark, stale rejection and explicit reconnect uncertainty. | Corrected contract; exact window/defaults and protocol tests required. |
| A16 | Critical | Host root cannot capture kernel-visible mapped data or current container-only acknowledged buffers. | Host-before-ack fixes ordinary transferred mutations; no claim that it fixes dirty mmap. Preserve full-surface V1 blocker, no freeze/direct-I/O-only/user-fsync escape. | Unresolved feasibility boundary, deliberately not approved. |
| A17 | High | Stage absence or a Commit row alone cannot prove this branch accepted an uncertain attempt; UpToDate has no new Commit ID. | Exact attempt/context and authoritative publication witness required; unknown stays unknown, no speculative coverage/new attempt. | V4 remains unselected for every supported uncertainty case. |

## Corrected zero alignment demonstration

```text
PASS 1: select exact monotonic NONZERO anchors from origin-coordinate matches.
        Journal anchors with bounded buffers; zero runs cannot move this cursor.

PASS 2: for each pair of anchors, consider only:
        old gap = [previous old anchor end, next old anchor start)
        new gap = [previous new anchor end, next new anchor start)
        Reuse zeros in order inside the old gap; emit unmatched new input.
        Virtual start/EOF anchors cover all-zero files and leading/trailing gaps.

Case 1: C1 = [A huge][Z1], S2 = [Z1][A huge][Z1]
        First gap before A has no old zeros: insert one zero, retain A.

Case 2: C1 = [A1][Z huge][B1], S2 = [Z huge][B1]
        Gap before B contains deleted A then old zeros: retain zeros, drop A.
```

This is a metadata algorithm, not byte lookahead. Old gaps are disjoint; total old
descriptor traversal is bounded by Po plus explicit boundary seeks. New descriptor
matching, old-index construction, anchor/plan journals and J fragments all appear
in the spec's complexity and counters. It does not promise O(changed bytes) time
for arbitrarily fragmented metadata. It must not allocate an all-piece plan vector.

## Performance verdict and remaining limits

The earlier proposed warm capture thresholds and fixed sample schedules were
withdrawn following the owner's direction. No new numeric target is an active
requirement. Implement correctness first, then use the existing benchmark suite
and its established contracts to understand actual performance.
Capturing a root cheaply can still produce a slow product if every ordinary write
pays excessive host RTT or COW metadata/ownership work. Likewise, bounded RAM can
conceal repeated disk scans, per-block refcount work, arena fragmentation or a slow
second Commit. The spec now requires these costs to be observed independently.

Remaining readiness conditions:

| Gate | Why it remains open |
| --- | --- |
| V1 | No selected/proved non-freezing snapshot mechanism for the complete existing supported kernel mapping/cache visibility surface. |
| V2 | Private page/allocator/interval ownership algorithms, complete resource defaults, retry fairness and physical allocation release are contracts without an implemented/proved backend. |
| V3 | Corrected matching design still needs bounded-cursor integration and independent provenance/zero/shifted-range/normalization proofs against the existing content engine. |
| V4 | Full uncertain publication/UpToDate witness and lifetime protocol not selected/verified. |
| V5: evaluation follow-up | Run applicable existing benchmarks after correct implementation; no new performance gate or mandatory new lane blocks design readiness. No benchmarks run here. |

The three reviewers' final targeted checks accepted the listed textual corrections
within their scopes; none approved a release or claimed benchmark speed. No review
finding is marked empirically resolved. The governing rule remains unchanged by
this record, and known blockers must remain visible in any downstream spec/plan.
