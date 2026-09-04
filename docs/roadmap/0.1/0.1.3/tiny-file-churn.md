# Tiny-file operations and bulk tree churn

> **Status:** Current v0.1.3 planning specification; no release candidate or
> measured result is implied.

Family ID: `tiny_file_churn`. This v0.1.3 implementation contract contains
**20 new timed cases and no standalone proofs**. The family adapters are not
implemented yet.

## Purpose and boundary

Separate individual file-operation cost from bulk creation and deletion of a
complete tree. Existing namespace initialization imports prepared trees; these
cases create and remove files through the live Workspace filesystem.

Follow the [shared testing rules](testing-rules.md) for deterministic seeds,
fixture custody and reuse, sample isolation, timing, independent manifests,
size accounting, and resource gates. Each sample uses the public Workspace
Create/managed execution/Commit/End lifecycle through real FUSE, followed by
separate fresh-Store and fresh-mount verification. One workload process performs
all scheduled operations before one Commit attempt.

## Case expansion

Expand `N` over exactly `1, 10, 100, 500`; each table row defines four distinct
scenario IDs. No size × operation-count matrix is added.

| Scenario IDs | Affected files | Measured operation | Expected Commit |
| --- | ---: | --- | --- |
| `tiny-create-{N}` | N | Create and write scheduled absent files | `Created` |
| `tiny-stat-{N}` | N | `lstat` scheduled existing files without payload reads | `UpToDate` |
| `tiny-unlink-{N}` | N | Unlink scheduled existing files | `Created` |
| `tiny-bulk-create-{N}` | 200 × N | Create N complete shared-profile shards, including directories and all file bytes | `Created` |
| `tiny-bulk-delete-{N}` | 200 × N | Unlink every file in N prepared shards, then remove emptied shard directories | `Created` |

Small-operation cases use the fixed 500-shard shared workspace as untouched
background and a separate 500-target schedule. Parents are prepared before
measurement. Create targets start absent; stat/unlink targets start present.
Small target sizes repeat by scheduled ordinal:

```text
0, 1, 7, 31, 127, 511, 1024, 2500, 4096, 8192 bytes
```

Use the shared domain-separated schedule with domain `tiny-file-churn`, and
nested prefixes of its 500 ranked target indices. Paths are
`tiny/p{ordinal mod 10}/f{index:03}.dat`. Derive content with the existing bounded
generator and a path-specific seed; freeze every expected digest. Longer files
must not accidentally repeat a common payload. Empty files and the finite
one-byte value space are intentional exceptions to content uniqueness.

Bulk cases use the
[shared workspace fixture](testing-rules.md#shared-workspace-fixture): each
shard has 200 files and exactly 1 MiB of payload. The tiers therefore affect
200/2,000/20,000/100,000 files and 1/10/100/500 MiB. One separate immutable
1 MiB witness shard remains outside the mutation target in all bulk cases.
Create starts with all target shards absent; delete starts with them present.
Use the same seed-bound path/content prefixes in both curves. Record actual
file, directory, and syscall counts instead of labelling a shard as one syscall.

Small rows peak below 500 MiB + 500 × 8,192 bytes. Bulk rows peak at 501 MiB.
No regular file exceeds 48 KiB in the bulk profile or 8 KiB among small targets.
Every intermediate state must satisfy the shared strictly-under-1-GiB rule;
no scratch copy or deleted-but-still-open payload is excluded from accounting.

## Timing and verification

Measured work includes every create/write/close, `lstat`, unlink, and `rmdir`
required by the selected row, followed by the declared ordinary sync. Per-file
sync is not added unless the operation contract requires it. Record workload,
sync, Workspace Create, Commit, visibility, End, and complete lifecycle walls
separately. Target-tree creation may never be cached outside a measured bulk
create. Prepared delete/stat input Stores may use sealed cache reuse and a
fresh independent writable sample.

Record affected files and directories, bytes, achieved path/payload rates,
FUSE operation counts, logical I/O, Store growth, candidate/inserted/reused
objects, CPU, memory, swap/OOM, and cleanup under the shared schema.

Separate verification compares the entire reopened namespace to an independent
expected manifest, including every untouched background and witness path.
Creation adds exactly the planned paths/bytes; deletion removes every planned
path and no others. Verify modes, declared mtimes, file lengths, content hashes,
canonical root, Branch head, and exact Commit outcome. `lstat` responses must
match the expected metadata; stat cases preserve the full manifest and root,
produce `UpToDate`, and cause no payload or durable Store-state growth.
Intermediate observations verify completed creation/deletion and planned byte
bounds, so a later operation cannot hide an earlier missing mutation.

## Execution and completion

Three fresh performance samples per new ID produce **60 timed executions**;
every ID also receives separate independent verification. Reuse the existing
benchmark binary, workload helper, runner/custody/sample-clone machinery, and
report schema adapters. Introduce no second benchmark framework.

Prospective selection is one case and one seed; it is not implemented CLI
syntax. Begin with `tiny-create-1`, `tiny-stat-1`, or `tiny-unlink-1` for the
changed operation, then its focused verifier. An ordinary selected run has a
provisional 1–5-second target. The 100,000-file tiers, whole-family runs, and
complete manifests use the longer lane with baseline-derived budgets. Fast
preparation never skips measured filesystem work.

Completion requires all 20 case identities and seed samples, exact full-tree
and transient-size proofs, no resource/cleanup failure, and retained evidence.
Bulk create/delete are distinct from directory-only construction and from
prepared-tree import; their timings must remain separately attributable.
