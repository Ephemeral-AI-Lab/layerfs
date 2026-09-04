# Phase 1 functional repair implementation

The [completion amendment](failure-repair-amendment.md) requires these repairs
before terminal pass. The original 84 performance outcomes remain preserved:
48 raw passes and 36 raw failures, all with their original producing identities.
Corrected performance and complete independent verification are still pending.

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
