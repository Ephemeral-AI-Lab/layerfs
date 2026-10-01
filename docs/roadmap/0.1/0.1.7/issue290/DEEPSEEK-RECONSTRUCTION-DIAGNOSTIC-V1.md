# Full repository remainder reconstruction diagnostic

Status: Research; informative and not a product contract.

The original b76808dee4d78312911abdd4443137f0e4dcb651 full-import selection
completed construction and upload, but its exhaustive download+metadata+read
proof timed out at10s (observed10.008257s). All5,836 downloaded packs were checked
against their original exact prepared bytes; full entries/xattrs EXCEPT comparisons
passed. Checkpoint records13,312 independently reconstructed files and
1,166,388,570 original bytes with exact EOF. That receipt remains TIMEOUT and
is never replaced or relabeled. No performance arm repeats.

This necessary correctness diagnostic finishes the remaining89,796 files using
22 indexed keyset batches: up to4,096 entries per batch, last batch smaller.
Each complete batch command has10s maximum. No speed claim, no aggregate wall
admission or cache comparison. Same closed master/corpus, retained downloaded
packs and immutable published catalog; SHA256 identities recorded before work.
The first cursor is the exact path of file13,312 in the original ordered proof;
one bounded scalar ordinal query derives it outside batches. All later cursors
are last successfully verified raw paths. Paths stay private in diagnostic
receipts; public evidence records counts and their hashes.

Reuse original full metadata/pack/first13,312-file proof with exact identities.
Each new batch uses the public C2 FULL decoder, authenticated C1 grouped reader,
and bounded comparator against original source bytes, exact size and EOF. One
producer; same bounded caches and SQLite query-only profile. Keyset traversal,
no population vector or repeated OFFSET scan. Final aggregate counts/byte sum
must equal103,108 files and3,475,776,149 bytes, otherwise INCOMPLETE. No mutation
of content roots, packs or captured input; no upload/resend/server restart.

Commit this diagnostic contract before implementation, freeze its source before
execution, retain each successful/failed batch and keep a timeout failed. This
can establish exhaustive content correctness alongside the original failed10s
phase; it cannot make that phase PASS or make the full import release admission.
