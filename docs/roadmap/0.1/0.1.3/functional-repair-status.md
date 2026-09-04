# Phase 1 functional repair implementation

The [completion amendment](failure-repair-amendment.md) requires these repairs
before terminal pass. The original 84 performance outcomes remain preserved:
48 raw passes and 36 raw failures, all with their original producing identities.
Corrected collection is in progress; the source-bound checkpoint below distinguishes
confirmed repairs from remaining failures and verification work.

| Shared cause | Minimum repair | Focused qualification |
| --- | --- | --- |
| Structural Commit exceeds final-delta budget | Apply changed directory edges and inode records directly, preserve unchanged roots, add references before removal, traverse only deleted subtrees, and bound batches/cursors/transient paths inside the unchanged budget. | One low-budget namespace regression covers creation, rename, deletion, aliases, exact membership and retained subtree identity. The final stable-source check also rejects a valid long path whose temporary planning charge exceeds a custom limit. |
| Deferred writes exceed aggregate piece allocation | Represent one contiguous offset-zero spool extent with an inline length charged as eight bytes. Fragmented edits promote to the existing fully charged nodes. | One regression covers 100000 records/800000 charged bytes, range semantics, promotion, snapshot preservation, truncation and unchanged error/limit gates. |
| Proxy cannot deliver wide directory | Stream indexed directory response fragments, retaining the 16384-entry per-frame cap and aggregate encoded-byte ceiling. Reassemble the same directory snapshot before existing FUSE pagination; preserve small-response bytes. | One codec regression checks 32002 entries on both directory routes, ordering/metadata, actual frame counts, and malformed/truncated/over-budget streams. |

No file-size, logical-total, memory, spool, Store, evidence or deadline gate was
raised. No workload was shrunk and no benchmark-specific product bypass was
added. Required failure propagation and expected-error oracles remain intact.

The corrected representation changes `PieceTree` from 16 to 24 bytes,
`FileData`/`Data` from 104 to 112 bytes and cached `Node` from 192 to 200 bytes.
Consequently even the 24 read/stat slots have changed lifecycle memory layout.
The other 60 existing slots execute changed write or structural paths. All 84
old outcomes remain valid **original baselines**, but all require corrected
candidate collection; none are relabeled as new-product evidence. Their input
bytes, independently specified oracles and compatible pristine Store preparation
remain reusable. No final verifier or remaining family had run at this point.

The earlier nine-slot observation-only recollection plan is superseded by this
functional-repair invalidation. Candidate rows use the explicit `corrected`
source arm. Reports retain original and corrected arms separately and forbid
PHASE1_TERMINAL_PASS while any required candidate gate remains failed.

The build pipeline now checks cached host depfile source contents against the
requested checkout and invalidates only changed Cargo packages. It retains
unchanged dependency artifacts. The original 712-to-4c host reuse is disclosed:
176 of 177 source inputs were byte-identical; the sole change was excluded by
Linux cfg from the macOS host target. Later target-relevant changes cannot reuse
that old host binary. Failed build/compile attempts and the source-unstable
intermediate namespace test are retained separately; none are called qualified
passing evidence. The next source/image build is authoritative for runtime work.

Qualification evidence is under
`benchmark-results/fs-bench-pro/phase1-v013/qualification/`, including
`structural-frontier-final-stable`, `compact-spool`,
`proxy-directory-fragments-recheck` and `host-cache-provenance`.


## Current checkpoint: 34224330

The original three repairs completed their affected initial performance collection.
The later sampler fix replaced only one truncated observation; that original row
remains invalid. Before the unlink repair, 120 unique performance slots and two
focused independent proofs had been collected. These are historical checkpoints,
not a claim that the complete campaign passed.

A real Git workflow exposed another shared functional defect: an acknowledged
unlink under an uncached parent could remain visible through a host lookup until
a later barrier. Commit `0763fac6` changes only `ProxyClient::unlink`: finish the
existing barrier before acknowledging an uncached-parent deletion. Cached-parent
batching remains in place. The focused regression fails before the repair and
passes afterward; source, commands and both outcomes are retained in
`qualification/git-unlink-visibility/`.

The qualified successor build is `3422433020a678a77f88e8a110492ca293c05e30`, with
product seal `4637a27f57351decbee4f800ba97f63d743fb03c7c5b91bad56550eadb310170`.
The original `git-tool-10` seed-1 failure remains at
`attempts/git-tool-10-s1-performance-5abd0cdea1ba`.
Its corrected real-workload execution passed at
`attempts/git-tool-10-s1-performance-cd922cae2006`, including resource and cleanup
validation. Command wall was 78.496 seconds, including 54.838 seconds of input
preparation; this is not described as a fast whole-command result. Independent
Git verification and the remaining family samples are still required.

The explicit source map retains 96 prior performance slots and the old
`payload-create-1m` seed-1 proof. Exact product/Cargo source comparisons permit
only the unlink-method delta, and each retained attempt must contain complete
zero-unlink and zero-rmdir receipts. All 24 tiny-deletion performance slots, three
previous Git passes and the previous bulk-delete proof require recollection.
The interrupted Git seed-2 preparation remains **not-run**, not a product failure.
After the selected Git recovery, the current candidate has 97 validated performance
slots. The ledger and generated review remain the authoritative live counts.

The sustained harness had a separate source-proven error-path defect: one worker
could leave its peer blocked at an infallible barrier. The 34224330 harness uses
peer-disconnecting channels at the same three handoffs, each bounded by the
remaining original 30-second cycle deadline. The exact 600-second workload,
900/600-second guards and oracle are unchanged. Its one focused helper check passed.

The required sustained proof then **failed**, without timeout or OOM, after
11.138 seconds at `attempts/workspace-sustained-600s-proof-s1-verify-c3db3ad3ff04`.
Worker 0 reported `EINVAL`; worker 1 immediately reported peer disconnection.
The last periodic progress line was 1847 cycles. The focused public-API regression reproduced the exact cause: after 4096
successful mutations, a zero-length file rejected the next write. Repair
`101626e7` retires the prior logical edit generation only after a successful
nonempty-to-empty transition. The same regression now passes and still verifies
that a nonempty generation rejects mutation 4097 without changing bytes. The
actual 600-second proof must still pass on the repaired source; this is not
terminal pass or a performance-optimization task. Cleanup passed. No workload size, duration, budget
or expected-error oracle has been reduced to resolve it.
