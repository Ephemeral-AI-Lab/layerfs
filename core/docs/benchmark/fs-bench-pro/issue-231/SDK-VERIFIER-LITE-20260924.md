# #231: lightweight SDK Init benchmark verifier

> Frozen before changing the verifier or collecting a new row. Owner
> direction on 2026-09-24 explicitly replaces per-file full-content
> verification for this benchmark with a lightweight, sampled content
> oracle. Prior full-verifier PASS/TIMEOUT receipts retain their original
> scope and status. This change must not be described as a full-byte oracle.

The verifier still authenticates the sealed manifest, reopens C5 History
and C2 Store, matches the returned genesis root, and walks **every**
directory through public C1 listing and grouped inode lookup. It rejects
duplicate, unexpected or missing paths, wrong inode kinds, and checks
portable metadata on every directory. It counts all discovered files.
These structural checks cover the complete 100/1,000/10,000/100,000-file
namespace. They do **not** read every file payload.

For file content, choose a deterministic sample from the sealed manifest:

1. Sort regular-file paths lexicographically. Select every
   `max(1, ceil(files / 64))`-th path and the final path.
2. Group expected file sizes into bucket 0 for empty files and bucket
   `1 + floor(log2(size))` for positive sizes. Select the first and last
   path in each occupied bucket.

The union has at most **195** paths (`64 + 1 + 2 × 65`, with duplicates
removed). It includes empty and small files and, for the frozen 100k
fixture, both 100-MB anchors. This is a deterministic coverage selection,
not a random confidence interval. For **each selected file**, keep the
current public C1 `read_all` path and verify its kind, portable mode/mtime,
exact logical byte count and SHA-256 against the sealed manifest. Use the
existing four file workers and no new dependency. Never fill observed
metadata or content from expected data, skip a selected byte, or substitute
an object-ID comparison.

The verifier's new append-only release-v5-lite output must distinguish:
`paths` and `discovered_files` for the complete namespace,
`sampled_files` and `sampled_bytes` for actual content read, and
`manifest_bytes` for the sealed expected total. The runner checks expected
case path/file/directory counts, returned root and manifest identity,
sample policy identity, and positive sampled coverage before accepting
the verifier's PASS. A sampled-byte count must never be labeled as all
500 MB. Keep full stderr/stdout and any failure/timeout evidence.

Use the same locked release driver/fixture and one fresh Store/History per
case. The complete public Init command remains bounded by 15 s; the
separate benchmark verifier keeps the prospectively frozen **9.5 s**
watchdog (strictly below 10 s). Run 100k once first; if it passes, run
100, 1k and 10k once each at that new source/build/harness identity.
Never resample an unchanged arm to improve a number. The previous
traversal-overlap and unused batch-root plans are historical experiments;
the lightweight verifier returns to the simpler post-traversal worker
queue and does not retain their code.

This owner-approved lighter proof replaces the prior **full-content
verifier requirement for new SDK benchmark receipts only**. Full-oracle
unit/integration tests and earlier full benchmark receipts remain
separate correctness evidence; this sampled result alone cannot assert
every benchmark file's content was read back. The source-cache contract
remains `source-cache-uncontrolled-v1` and no numeric SDK latency target
is frozen, so a sampled verifier PASS still cannot turn a case into an
eligible performance PASS or by itself authorize #231 closure/main merge.
