# G3-v7 direct-counter dictionary

Status: **frozen before build or measurement**

The candidate command and all v1 flat JSON fields remain. v7 adds six explicit
reconciliation counters, complete build provenance, and repaired gate
expressions. This file restates the controlling equations so v7 analysis is
self-contained.

## Exact schedule and routes

| Seq | Scenario | Bytes | Route | Reason | Result |
|---:|---|---:|---|---|---|
| 1 | `qualified-noop` | 10485760 | `qualified-noop` | `seed-hit` | success |
| 2 | `qualified-one-byte` | 104857600 | `qualified-patch` | `seed-hit` | success |
| 3 | `qualified-one-mib` | 10485760 | `qualified-patch` | `seed-hit` | success |
| 4 | `invalid-authority` | 1048576 | `complete-fallback` | `invalid-authority` | success |
| 5 | `external-mutation` | 1048576 | `complete-fallback` | `destination-invalidated` | success |
| 6 | `symlink-substitution` | 1048576 | `typed-rejection` | `destination-symlink` | `NativeDestinationSymlink` |
| 7 | `count-change` | 1048576 | `complete-fallback` | `count-change` | success |
| 8 | `before-publication-fault` | 1048576 | `qualified-patch` | `seed-hit` | `InjectedBeforePublication` |
| 9 | `lost-ack` | 1048576 | `qualified-patch` | `seed-hit` | success |

The row schema is `phase4-g3-row-v1`; this is deliberately unchanged candidate
behavior. Runner-added evidence uses v7 names and adds `source_set_sha256`.

## Required equations

```text
authority_validations = authority_validation_successes
                      + authority_validation_failures
payload_sql_queries = mapping_sql_queries + object_sql_queries
payload_sql_rows = mapping_sql_rows + object_sql_rows
canonical_blob_bytes = canonical_bytes_authenticated
attributed_wall_ns = sum(the ten named timer_*_ns fields)
operation_total_ns = attributed_wall_ns + unattributed_wall_ns
```

Qualified routes validate complete bindings, read protected seed authority, and
consume exactly one single-use permit. Invalid/stale/external/count/clone-fail
fallback and preflight rejection consume zero. A typed symlink/wrong-kind
preflight does no authority, SQL/BLOB/authentication, reconstruction, clone,
copy, patch, fallback, temp, sync, rename, or reconciliation work.

No-op has one successful clone and zero payload SQL/BLOB/authentication,
reconstruction, copy, patch, and fallback work. Qualified patches have one
successful clone, `patch_bytes = changed_bytes`, no complete reconstruction,
and authenticated bytes no greater than `changed_bytes + 1048576`. The exact
changed byte counts are 1, 1048576, 1, and 1 for the two patch rows,
before-publication fault, and lost acknowledgement. Complete fallback has one
call and:

```text
source_bytes_reconstructed = fallback_write_bytes = output_length
```

It has no clone, copy, patch, or permit consumption. Count change has output
length 1048577; other rows retain their input length.

Every published row has exact bytes/mode, zero seed/temp residue, one data sync,
one metadata sync, one rename, and one directory sync. A renamed temp satisfies
`temp_files_created = temp_files_removed + rename_calls`. Before-publication
fault has no rename, preserves `old`, and explicitly removes its temp. Lost ack
accepts only:

```text
(reconciliation_outcome, old_or_new) in {("target", "new"), ("prior", "old")}
```

Every other row reports `not-needed`; the symlink and before-publication rows
preserve `old`, while ordinary successful rows report `new`.

Ordinary payload/mapping/object/canonical/source counters exclude
reconciliation. v7 adds these non-negative observed counters:

| Field | Meaning |
|---|---|
| `reconciliation_sql_queries`, `reconciliation_sql_rows` | Complete target/prior comparison SQL work. |
| `reconciliation_blob_reads` | Canonical BLOB acquisitions during reconciliation. |
| `reconciliation_canonical_bytes_authenticated` | Canonical bytes fully authenticated during reconciliation. |
| `reconciliation_source_bytes_compared` | Reconstructed canonical source bytes compared with the freshly opened destination. |
| `reconciliation_q_high_water` | Logical Q high-water attributable to reconciliation buffers/state. |

All six are zero when `reconciliation_calls = 0`. Lost ack requires each to be
positive. For target/new, `destination_bytes_read = output_length` and
`reconciliation_source_bytes_compared = output_length`; prior/old may perform
target then prior comparison and requires both values at least `output_length`.
`reconciliation_canonical_bytes_authenticated >= output_length`, and
`q_high_water >= reconciliation_q_high_water`. This full fault-recovery work is
descriptive and does not weaken the primary one-byte patch bound. Stable exact
different bytes map to `PublicationConflict`; read, wrong-kind, or pre/post
identity instability maps to `AmbiguousDurability`.

`q_terminal` is zero; Q high-water and external maximum RSS are at most 20 MiB.
Candidate temp/seed peaks and runner transient logical/apparent/allocated peaks
are at most 512 MiB. Physical I/O, cache warmth, and stable-media status remain
explicitly `Unavailable:` with reasons. Every operation is below 5 seconds,
their sum below 20 seconds, the build child below 30 seconds, and the complete
campaign below 59 seconds.

## Source and method custody

Each row's runner-added `source_set_sha256` must be a 64-hex digest equal to the
canonical digest in `SOURCE-CUSTODY-v7.json`. That record contains exactly the
four ordered source paths from the v7 preregistration, original/copy SHA-256,
size, original mode `0644`, copy mode `0400`, and distinct copy path. Binary
custody occurs only after the one recorded offline release build. Method custody
contains the v7 preregistration, dictionary, runner, primary analyzer,
independent analyzer, and post-static finalizer; the dry run is separately
hashed and copied.

For each source, `copy_path` is exactly
`source-custody-v7/<repository-relative-source-path>`, is relative with no empty,
dot, or `..` component, and resolves beneath the source-custody root. The copy
is a regular non-symlink whose device/inode differs from the original, with
`copy_size_bytes = size_bytes`, `copy_sha256 = sha256`, and mode `0400`.
Runner, both analyzers, and finalizer reject any missing path, escape, alias,
wrong size, wrong hash, wrong mode, or non-exact four-path set.

Only the current exact `results-v7/work-v7/<label>` row root may be removed.
`ROW-CLEANUP-v7.jsonl` has 18 fsynced alternating events. PREPARE binds the
retained WORK/row dirfd contract, exact sorted inventory with path/kind/dev/ino/
mode/nlink/size/mtime/ctime/allocation, canonical hash/count, pre-delete row and
WORK usage, candidate exactness/residue, and the exact deletion method. COMPLETE
binds PREPARE's canonical hash, the identical inventory/deleted-set hash/count,
WORK post-state, and row-root absence. Raw rows, cleanup events, stdout/stderr,
and chronology are append-once; no result row may be retried.

The exact method is:

```text
descriptor-relative-openat-fstatat-unlinkat-rmdir-no-follow-exact-inventory-v1
```

The same row fd spans PREPARE through deletion. Each directory is revalidated
against its expected immediate children before recursion; additions or identity
changes abort without deleting unexpected paths. WORK is fsynced after row
rmdir and before durable COMPLETE.

Before PREPARE, stdout/stderr each complete `write → flush → file fsync → parent
directory fsync`. The raw file is precreated and file/parent-fsynced; each
enriched row completes `append → flush → file fsync`. Chronology uses durable
append with parent sync on first creation. `FAILURE-v7.json` uses exclusive
create, flush, file fsync, and parent sync. Self-check markers prove this order
without invoking a campaign child.

Final cleanup requires exactly nine row-cleanup labels/order, removes the now
empty `WORK`, and reports for each storage dimension:

```text
peak = max(each PREPARE's pre_delete_row value)
cumulative = sum(each PREPARE's pre_delete_row value)
peak <= 512 MiB
```

The cumulative value is descriptive and may exceed 512 MiB; using it as the
peak is a protocol error.

The finalizer's only G2 dependency is sealed G2-v5 at
`target/phase4-g2-materialization-decomposition-20260822-v5/results-v5`, with
exact v5 payload/terminal/verification/raw/analysis filenames and their frozen
hashes. No G2-v7 path or filename is valid. The finalizer self-check reads and
verifies the complete G2-v5 anchor without creating any campaign or terminal
artifact.

## Static closure and finalization

After a PASS campaign, the parent creates `STATIC-CLOSURE-v7.json` with schema
`phase4-g3-v7-static-closure-v1`, the frozen `source_set_sha256`,
`candidate_retained: true`, and these exact ordered PASS command labels:

```text
focused-g3-tests
workspace-tests
workspace-clippy
workspace-fmt-check
git-diff-check
custody-review
```

Each command entry records its argv, zero exit code, and result-relative
stdout/stderr path, SHA-256, and size. `finalize_g3_v7.py` refuses incomplete or
non-PASS input, verifies campaign/ledger/custody/cleanup and sealed G2-v7
anchors, creates the payload manifest and PASS terminal, seals files `0444` and
directories `0555`, then performs independent manifest/hash/mode/closure
verification. The finalizer is never part of the measured campaign wall and is
not invoked by the zero-row dry run.
