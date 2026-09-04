# Root fsyncdir exposes the baseline aggregate piece-allocation limit

Status: **reproduced product resource/capacity finding; product FAIL**. Three
performance attempts of `tiny-bulk-create-100` reached the required native root
fence and returned EINVAL. No Commit, successful publication, or independent
final-tree verification is claimed. This finding does not waive those gates.

The frozen workload creates 100 shards: 20,000 files containing 104,857,600
bytes. Source revision is `4c207c70f3282c316d5ab18d832504085835eda3` for all
three attempts. Input seed changes content, while the file sizes and operation
schedule remain the same. All three attempts have the same product identity,
image identity, failing operation, and retained spool boundary.

## Observed failure, decoded without altering sealed evidence

The SDK stores stderr in Debug-form `OutputPage` byte arrays. Concatenating the
`bytes` arrays in sequence order yields these relevant lines for every seed:

```text
partial_attempted_syscall_count=60468
partial_completed_syscall_count=60467
partial_completed_file_write_count=20000
partial_completed_write_bytes=104857600
partial_workload_open_call_count=20000
partial_workload_pwrite_call_count=20000
partial_workload_close_call_count=20000
partial_workload_fsync_call_count=0
partial_workload_open_directory_call_count=1
partial_workload_fsyncdir_call_count=1
partial_benchmark_injection_count=0
partial_benchmark_reopen_count=0
partial_benchmark_verifier_count=0
fs-benchmark-workload: fsyncdir .: Invalid argument (os error 22)
```

These are application acknowledgements. They cannot establish that all bytes
were accepted by the asynchronous host or became publishable. In each attempt,
the after-Create owned runtime census contains one 146-byte control file.
Immediately before Discard the census contains 20,001 files and 101,154,962
logical bytes, with 129,372,160 physically allocated bytes. Subtracting the
unchanged control file leaves **101,154,816 bytes of bulk spool content**.

## Source trace and capacity inference

1. `ordinary_workloads::create_entries` follows the canonical sorted entries
   and performs the specified native writes and closes. `Ops::finish` completes
   metadata normalization, opens `.` as a directory, then calls `sync_all`.
   It does not call fsync on an invalid descriptor or suppress a failed fence.
2. `layerfs-fuse::filesystem::fsyncdir` routes to `port.fsync(None)`.
   `ProxyClient::fsync` flushes pending data and calls `synchronize_locked`.
   Its Fence/Fsync requests acknowledge previously deferred host errors.
3. `proxy_host::serve` retains errors from requests marked `no_reply`. A later
   acknowledging request returns that error before dispatching its own body.
   Thus this EINVAL need not originate in `Workspace::fsync` at all.
4. `Workspace::write_inner` calls `check_piece_resources` before appending the
   bytes. It rejects aggregate piece-state charge above `MAX_PIECE_ALLOCATION`,
   fixed at **2,097,152 bytes**, with
   `InvalidInput("workspace piece allocation limit")`.
   `projection::storage_port_error` maps that variant to `PortError::Invalid`,
   which becomes EINVAL at the native fence.
5. `PieceTree::logical_allocation_charge` charges every piece using
   `size_of::<PieceNode>()`. The observed 64-bit structure has a 56-byte Piece,
   two pointer-sized children, priority, and six 8-byte aggregate fields: a
   128-byte charge per single-piece file. The 2 MiB budget therefore admits
   **16,384** such files. The ordinary write route uses Spool pieces, not inline
   SDK replacements, so the separate 8 MiB inline limit does not explain this.
6. The first 16,384 sorted files sum to **101,154,816 bytes**, exactly the
   retained bulk spool census. They end at `bulk/wide/s043-f031.dat`. The next
   file is `bulk/wide/s043-f032.dat`; the remaining **3,616** files are each
   1,024 bytes. Their missing 3,702,784 bytes exactly explain the difference
   from the application's acknowledged 104,857,600 bytes in every seed.

The aggregate piece-limit diagnosis is a source-and-census inference, not an
invented retained StoreError message: the original evidence exposes EINVAL,
not the private variant text. It is independently corroborated by all three
seeds at the exact byte boundary. In contrast, missing/changed spool descriptor
errors from `spool_file` become EIO through the current error mapper and cannot
explain this EINVAL. No source change, extra test, build, or measurement was
performed for this read-only diagnosis.

## Evidence and classification

- Seed 1: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s1-performance-6e322f16632d`. Original `stderr.txt`, `raw.jsonl`, `outcome.json`, and `evidence.sha256` remain unchanged.
- Seed 2: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s2-performance-15931b808655`. Original `stderr.txt`, `raw.jsonl`, `outcome.json`, and `evidence.sha256` remain unchanged.
- Seed 3: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-100-s3-performance-7775d37b9f21`. Original `stderr.txt`, `raw.jsonl`, `outcome.json`, and `evidence.sha256` remain unchanged.

The campaign classifications use the exact numeric byte subsequence encoding
`fsyncdir .: Invalid argument (os error 22)`, which occurs in each original
sealed stderr. Each row links a different matching seed as its reproduction.
Only these three `tiny-bulk-create-100` failures receive this finding; the
existing final-delta Commit classifications and unrelated failures are retained.

Discard succeeds in each attempt, the post-Discard owned spool census is zero
files/bytes, and supervisor cleanup succeeds without OOM or timeout. The
mutable sample Store is retained for investigation. These observations establish
that recorded recovery occurred, not atomic publication or full cleanup proof.

## Phase 2 dependency

Resolve aggregate ordinary-workspace piece-state capacity for the unchanged
20,000-file workload and preserve deferred-error propagation. Any implementation
or supported-capacity change belongs to Phase 2. Keep the original 2 MiB limit,
required root fence, fixture, and failed performance rows unchanged for baseline
comparison. Final verification remains separately required and must preserve
this actual failure rather than treating application acknowledgements as a pass.

## Additional frozen 500-shard reproduction

The three `tiny-bulk-create-500` performance attempts independently reproduce
the same native `fsyncdir .: Invalid argument (os error 22)` failure on the
same product/source/image. Their acknowledged totals are **100,000 files and
524,288,000 bytes**. Each pre-Discard census has 100,001 files and 113,050,770
logical bytes (136,994,816 allocated bytes). Removing the same 146-byte control
file leaves **113,050,624 bulk spool bytes**.

For the 500-shard canonical sorted schedule, the first **16,384** files sum to
exactly **113,050,624 bytes**, ending at `bulk/regular/s121/f112.dat`. The next
file is `bulk/regular/s121/f113.dat`. This different prefix size independently
corroborates the same piece-count ceiling; the shortfall is not a fixed byte
spool limit. These attempts also fail before Commit, preserve partial receipts,
and successfully Discard with a zero post-Discard owned spool census.

- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s1-performance-42d284ee49b7`
- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s2-performance-462a9388dc2f`
- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-create-500-s3-performance-fcfeb1c9523c`

Only the 100- and 500-shard attempts bearing the exact EINVAL signature are within this finding.
