# Wide-directory enumeration exceeds the baseline proxy response limit

> **Updated completion policy:** These are retained original baseline failures. The [failure-repair amendment](../failure-repair-amendment.md) now requires their repair in Phase 1. See [functional repair status](../functional-repair-status.md) for the corrected implementation and pending runtime qualification. Historical outcomes below remain failed.

Status: **reproduced product resource/capacity finding; product FAIL**. All
three frozen `tiny-bulk-delete-500` performance attempts fail during native
`readdir bulk/wide`, after 68,000 acknowledged unlinks. The workload does not
finish, no root fence is reached, and no Commit or independent final-tree
verification is claimed. These are not final-delta Commit failures.

The three attempts use source `4c207c70f3282c316d5ab18d832504085835eda3`,
product identity `810655a13d8621b2e04efeda5747e54929e4d4717e8d5d82dcddcf75f905b727`,
and image `sha256:781f4513dcba84f51bb5b7fda4704e7e5dfe52c8aabf777b310778afba41935f`.
The immutable prepared input is the declared 500-shard bulk tree plus its
independent witness; the workload uses the canonical native recursive deletion.

## Observed failure, decoded from retained Debug output

Concatenating the original SDK OutputPage `bytes` arrays in sequence order
produces the following relevant stderr in each seed. Original sealed artifacts
remain unchanged.

```text
partial_directory_entry_count=68632
partial_completed_file_write_count=0
partial_completed_write_bytes=0
partial_workload_opendir_call_count=633
partial_workload_closedir_call_count=632
partial_workload_lstat_call_count=68633
partial_workload_unlink_call_count=68000
partial_workload_rmdir_call_count=631
partial_workload_fsyncdir_call_count=0
partial_metadata_normalization_count=0
partial_benchmark_injection_count=0
partial_benchmark_reopen_count=0
partial_benchmark_verifier_count=0
fs-benchmark-workload: readdir bulk/wide: Input/output error (os error 5)
```

The helper's `attempted_syscall_count` and `completed_syscall_count` both equal
137,897 here: directory iterator errors occur outside its `call` wrapper.
Those counters are therefore not evidence that every syscall succeeded. The
explicit iterator error, missing completed directory count, and process exit 1
preserve the failure. Acknowledged unlinks do not establish publication.

## Source trace and exact fixture correspondence

1. `workspace_common::shards` places 64 files from every shard in `wide`.
   At 500 shards, `bulk/wide` therefore has **32,000 direct file entries**.
   Each shard has 135 files in `regular` and one in `spine`: **68,000** files
   outside `wide`. The observed acknowledged unlink count is exactly this
   non-wide prefix of the canonical sorted recursive traversal.
2. `ordinary_workloads::Ops::delete_tree` lists and sorts each directory using
   `Ops::children`, which calls `fs::read_dir` and preserves iterator errors as
   `readdir <path>: <error>`. It deletes the earlier regular/spine paths before
   reaching `bulk/wide`. There is no invalid-directory or fabricated-path call.
3. The FUSE callback asks the port for the complete directory through
   `port.readdir`/`port.readdirplus`. `ProxyClient` uses `Request::Readdir(node)`
   or `Request::ReaddirPlus(node)`; these requests contain no page cursor.
4. `proxy_host::dispatch` wraps the full result in `Response::Entries` or
   `Response::EntriesPlus`. `protocol::write_response_measured` rejects either
   response when its entry count exceeds **MAX_ENTRIES = 16,384**, returning
   `invalid("entry count")` before sending a response frame. The response
   decoder enforces the same bound.
5. `proxy_host::serve` breaks out when response encoding/writing returns an
   error, dropping that stream. `ProxyClient::raw_exchange_at` maps the ensuing
   response read failure to `PortError::Io`; native FUSE reports EIO.
6. Kernel buffer pagination in `filesystem::readdir` and `readdirplus` takes
   place only after the full port result has been fetched. Its offset/`reply.add`
   loop cannot reduce the 32,000-entry response at the earlier proxy boundary.

The entry-limit cause is established by the source protocol and exact frozen
fixture width, independently reproduced by all three seeds at the same native
path. The retained log itself contains EIO, not the private codec error text;
this document does not relabel that text as directly observed. No build, test,
extra product call, fixture reduction, or product modification was made for
this diagnosis.

## Reproductions and recovery limits

- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s1-performance-cf64c1d0ecbe`: original `stderr.txt`, `raw.jsonl`, `outcome.json`, and `evidence.sha256`.
- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s2-performance-3e206a75b5a0`: original `stderr.txt`, `raw.jsonl`, `outcome.json`, and `evidence.sha256`.
- `/Users/yifanxu/Ephemeral-AI-Lab/layerfs/benchmark-results/fs-bench-pro/phase1-v013/attempts/tiny-bulk-delete-500-s3-performance-ad9fbdc9b047`: original `stderr.txt`, `raw.jsonl`, `outcome.json`, and `evidence.sha256`.

Each attempt records successful Discard, zero post-Discard owned spool files
and logical/allocated bytes, and successful supervisor cleanup without timeout
or OOM. These are recorded recovery observations; independent prior-state,
publication atomicity, and complete lifecycle verification remain separate gates.

The three classification entries match the precise original byte subsequence
encoding `readdir bulk/wide: Input/output error (os error 5)`. Each links a
different matching seed as its reproduction. Existing classifications, original
failed outcomes, and all sealed evidence are preserved.

## Phase 2 dependency

Resolve transport of directories wider than 16,384 entries while preserving
complete enumeration and correct continuation semantics. Any protocol/algorithm
or supported-capacity change belongs to Phase 2. Do not shrink the frozen wide
directory, substitute direct SDK deletion, increase a baseline gate during
measurement, or call the partial deletion successful. Retain these baseline
failures for the identical 500-shard comparison and conduct final verification
separately.
