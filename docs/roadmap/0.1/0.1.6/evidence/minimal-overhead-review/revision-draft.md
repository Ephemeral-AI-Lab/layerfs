# #130 revision draft: minimal Workspace overhead

Status: retained earlier minimality draft following the
[rules/minimality audit](rule-audit/README.md). The
[promoted implementation plan](../../overlay-minimal-overhead-implementation-plan.md)
now selects the implementation order, scaling requirements and two-second
objective. This draft is not implementation or benchmark evidence; its narrow
four-change first cut alone does not resolve the 25k dirty-file costs.

## Scheduling and authority

The owner promoted #130 before final capacity qualification and benchmarks;
closure of #124/#125 is no longer a prerequisite. Continue dependency-ready
optimization alongside production integration, preserving all mandatory
correctness and cleanup. Use the linked current plan rather than this historical
draft's after-delivery assumptions below.

The [overlay rules](../../overlay-snapshot-rule.md) and applicable
[specification](../../overlay-snapshot-spec.md) remain authoritative. Preserve the
delivered supported-surface capture, C1/C2, ownership and publication contracts.
Record any necessary private-format/specification changes before enabling them;
no performance objective silently relaxes semantics.

## Objective

Reduce fresh Workspace-per-tool-call time, temporary storage, I/O and actual
resource demand. Every call keeps a fresh Workspace ID, lease, root, replay and
FUSE/handle scope. Reuse existing Store/container/daemon/runtime services where
their existing ownership permits; do not reset and reuse one mutable Workspace.

Cost follows changed files, affected names/ranges and retained owners rather
than copying the immutable base. Read-only traversal still accounts for real
kernel references/cookies. No zero-memory, zero-FD, fixed millisecond or final
100k-file storage claim follows from the architecture diagram.

## Selected first implementation

Recheck the delivered prerequisite source first; do not redo fixes already made.

```text
Delivered #124 implementation
    |
    +-- keep root Index and disk ownership
    +-- keep current ID, cookie, alias and change indexes
    +-- keep repaired Payload / range ownership
    +-- keep valid capture, canonical builder and V4 receipts
    |
    +-- narrow unchanged-sequence construction shortcut
    +-- defer unused payload backing and empty journals
    +-- inline singleton canonical correspondence
    +-- prepare one small range leaf in one bounded operation
    |
    v
Measure remaining public-call, disk and I/O cost
    |
    +-- select another mechanism only for an identified residual cost
```

### A. Narrow unchanged-candidate shortcut

Bypass construction only after a valid supported-surface snapshot has been
acquired and all of the following hold:

- The captured logical sequence equals the applied covered sequence of the
  same Workspace and canonical comparison context.
- That comparison context owns the exact canonical predecessor being reused.
- Existing expected-head/base, stage ownership and authoritative outcome handling
  are still executed; an unresolved attempt is not overwritten.

Canonical byte/root equality alone is insufficient. A dirty equal-output attempt
may carry new Origins; it must update captured metadata-only correspondence even
if canonical encoding is reused. Physical/cache-only root changes must not be
confused with logical mutations. The shortcut returns through the existing
coordinator/admission/receipt path, not a fabricated UpToDate response.

Required regression: equal-byte overwrite with new Origin -> UpToDate -> small
C2 edit retains exact output and localized predecessor reuse. Also preserve
head conflict, lost Created/UpToDate reply and capture/coverage oracles.

### B. Lazy resources only where no owner yet needs them

Defer unused Payload arenas/private index and empty construction journals using
existing ownership interfaces. Keep the root metadata Index: Begin currently
writes root metadata, so lazy constructors alone cannot make it disk-free.

Reserve actual prospective allocations, retained readers and recovery/release
headroom before acknowledgment. A future release slot is admitted when its owner
is created; Drop must not allocate or become fallible. Reader-cache charges and
existing token/reclamation repairs remain. Shared resources stay correctly scoped
by Store/Workspace/trust domain. This first implementation introduces no new
resident-only page/payload tier or fsync protocol.

### C. Inline singleton correspondence

Represent the nonzero singleton predecessor description directly in its existing
parent value: canonical inode32 + content root32 + length8 + Origin24 + origin
offset8 + tag/kind2 = 106 encoded bytes. Validate/version the representation and
retain one Description/cursor/planner API, with the existing indexed form for
fragmented descriptions.

This is metadata-only correspondence; it must not retain raw payload. Avoid the
three dedicated description pages and their intermediate construction without
changing zero-anchor matching, canonical identity or C2 semantics. Account for
the parent index, retained versions and serialization overhead normally.

### D. Construct a small range leaf once

Reuse the current bounded page encoder and allocation/ownership transaction to
prepare the existing three-to-five range records together. Avoid successive
Index updates that immediately retire intermediate roots. Preserve rollback,
external token/child references and admission. This reduces construction work;
it does not remove the final dedicated range page and is not claimed to solve
all metadata footprint by itself.

## Optional mechanisms and their entry conditions

These are alternatives, not an additional mandatory implementation checklist.
Select the smallest one that addresses measured remaining cost. After selection,
its mechanism and checks must be concrete before implementation is called ready.

| Candidate | Evidence that would justify it | Contract that must be selected first |
|---|---|---|
| Compact base/singleton ranges or denser byte-aware pages | Retained range/page slack remains material after the first changes | Encoded layout, split/merge bounds and file-scoped ownership |
| Bounded cache/pager inside the existing Index | Actual repeated metadata/ownership I/O remains material | Cache scope, lock order, ownership/location transitions, eviction/rollback, explicit fsync and retained charges |
| Tiny or packed shared payload storage | Actual small-source allocation/write cost remains material | One chosen representation, append/join promotion, independent tokens, class/quota and partial-retention rules |
| Cookie-in-binding | Measured redundant name-to-cookie rows/lookups | Preserve numeric order/resume, deletion/recreation and kernel-reference behavior |
| Primary-row sequence summaries | Existing change indexes remain a demonstrated cost | Explicit spec revision, deletion markers, old-record cleanup and bounded cursor order |
| Canonical-ID virtualization or singleton aliases | Exact current mappings remain a demonstrated cost | Scope/root/ID allocation, legacy IDs, hidden aliases and promotion |

Do not implement all of these to satisfy a general instruction to be minimal.
Do not add a new database, parallel upper-map backend, generic storage framework
or canonical encoder merely to support the alternatives.

## Non-negotiable design guards

### Packed neighbors must have independent ownership

```text
Unsafe file-only lease: reader(A) -> packed leaf[A,B] -> retain A and B payloads
Required singleton:    reader(A) -> A's exact token/base ownership
```

Acquire the target's ownership while the source page is leased, then release the
parent. A retained reader of A must not keep deleted/replaced B's unrelated raw
payload alive. Fragmented packed ranges need a bounded file-scoped ownership/
cursor mechanism; copying every piece into RAM is not a substitute. A full
Commit snapshot legitimately owns its whole captured view; these are different
scopes. Keep dedicated file roots until the narrower lease contract is proved.

### A pager must define bootstrap and explicit flush

If selected, specify where owner/location records reside before any backing file
exists, what is reserved before eviction, how partial I/O is resolved and when
resident charges transfer. Captured bodies remain evictable without an unbounded
RAM map, whole-graph conversion or first-write clone. Shared-cache locks must not
span external owner callbacks or physical I/O.

Explicit fsync/flush must process resident-only acknowledged bytes and preserve
existing error/ownership semantics before pressure eviction. An absent backing
descriptor is not proof that nothing needs flushing. Test a small write followed
by fsync before eviction and inject transfer/flush failures. This adds neither
restart durability nor a per-mutation/Commit-capture durable flush.

### Tiny storage must preserve append continuity

Ordinary bytes remain ordinary Payload with ordinary-spool charging, not SDK
Inline. Crossing a small-storage threshold must preserve Origin coordinates,
old readers and sustained append coalescing. Promote a whole-source form before
any second independently owned token interval (including append/join or duplicate
whole range unless it correctly reuses the same token). Do not change allocation
rounding alone or retain a large mostly-dead segment for one tiny reader.

### Separate fixed-view scans from live enumeration

For a fixed captured directory, merge sorted lower entries and upper overrides/
tombstones with bounded cursors: O(L+K) stream work after index seeks, plus actual
attribute/physical-read costs. Stream contents directly through base/replacement
ranges; a full scan still processes its returned entries/requested bytes.

Live FUSE retains its existing numeric-cookie adapter and concurrent mutation
semantics. After EOF, a newly created earlier-sorting name must remain reachable
from the old cookie under the existing contract. Do not use a lexical frontier
or frozen root as a silent replacement. Count additional live-root revalidation,
cookie/attribute lookups and seeks; the fixed-view bound does not erase them.

### Commit never resets live state

Captured readers use their owned root; ordinary descriptors follow live inodes.
Publication updates exact comparison/coverage, leaves newer changes and lower
deletion masks intact, and does not remount or install a checkpoint. A conflict
or uncertain stage does not disable ordinary operations. Keep the same candidate
and expected context for retry. Canonical backing substitution remains optional,
byte-equivalent and source-checked; held owners and failed cleanup stay charged.

## Verification and source custody

Compare the delivered #124/#125 implementation with the selected #130 changes.
Use v0.1.5's published results as an additional historical reference only where
source, workload, cache, topology and timer boundaries support comparison. Its
old v0.1.3 ratios are not paired evidence. Never relabel old/legacy dispatch as
new snapshot-path performance or use inner Commit time as a full-call result.

| Selected change | Required affected checks |
|---|---|
| Clean shortcut | Valid capture/coverage; dirty equal-output Origins; exact stage, conflicts and lost Created/UpToDate; localized C2 |
| Lazy unused resources | Fresh Begin/read/End; quota/failure and release tickets; retained reader/attempt ownership; actual reservations versus allocation |
| Singleton description / range leaf preparation | Existing C1/C2 and zero anchors; serialization bounds; token/ref rollback; same canonical output |
| Packed live ranges, if selected | File A reader with large deleted/replaced neighbor B; fragmented file scope; bounded traversal and cleanup |
| Pager/tiny storage, if selected | Small-cache spill/reload; eviction/capture/read races; fsync-before-eviction; partial I/O; append threshold and subrange promotion |
| Scan changes, if selected | Fixed lower/upper merge, tombstone-only batches, live after-EOF/delete-recreate/partial-page resume, readdirplus ownership |

Carry forward the delivered natural-overlap and million-changed-file proof.
Record relevant source/fixture/environment invalidation before rerunning a pass.
If ownership/index/pager/admission changes invalidate those proofs, run the
affected original qualification, including one million changed regular files
before one final Commit and fresh reopen. The 100k accounting model is not a
substitute. Held-builder checks prove progress; natural active construction is
needed for foreground p50/p95/p99 and immediate post-capture work measurements.

Use existing benchmark entrypoints, repetitions, acceptance criteria and the
measurement lock. Preserve host Store/SDK/construction/spool and Linux FUSE/
workload placement. #122 remains excluded. Do not invent a numerical gate,
replacement campaign, speedup, or unconditional full-suite rerun. Publish actual
source/binary/image/fixture identities, counters, sample counts, resource domains,
retained failures and cleanup outcomes. Complete the applicable final-source
verification required by the selected changes.

## Completion and retained context

Completion requires the selected implementation, valid affected correctness/
capacity evidence, measured public-path results showing the selected improvement,
and no unresolved required regression. A single passing component or an
arithmetic estimate is not completion. Optional mechanisms are not mandatory
once a smaller evaluated solution satisfies the issue's objective and contracts.

Preserve the original [proposal snapshot](rule-audit/proposal.md),
[issue snapshot](rule-audit/issue130.md), subagent reports and source/benchmark
receipts as historical evidence. The first proposal's large layout estimates are
not accepted storage targets. This replacement should become the single selected
design document only when the revision is applied; #130 should carry its concise
scope, guards, acceptance list and a link to the published document.
