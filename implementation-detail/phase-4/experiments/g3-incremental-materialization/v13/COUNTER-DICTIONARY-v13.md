# G3-v13 direct-counter dictionary

Status: **frozen before build or measurement**

The candidate command, source bytes, and v1 flat JSON schema are unchanged from
frozen v12. v13 retains truthful reconciliation Q, namespace-cleanup guard
ordering, publication-first dual-error precedence, canonical changed-range
proof binding, the nine-row schedule, full fallback, cleanup, and no-rerun
rules. It repairs only external premeasurement authorization, exact G2/static
anchors, exact per-row operand/method/environment/argv custody, and actual child
environment evidence.

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
behavior. Runner-added evidence uses v13 names and adds `source_set_sha256`.

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

Qualified routes validate the exact ordered public binding claim, read protected
seed authority, and consume exactly one single-use permit. The keyed permit also
commits internally to target digest, exact range, and canonical-range proof
commitment; those private values are covered by frozen source custody and the
15 focused static tests rather than falsely presented as standalone row
counters. Invalid/stale/external/count/clone-fail
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
reconciliation. v13 adds these non-negative observed counters:

| Field | Meaning |
|---|---|
| `reconciliation_sql_queries`, `reconciliation_sql_rows` | Complete target/prior comparison SQL work. |
| `reconciliation_blob_reads` | Canonical BLOB acquisitions during reconciliation. |
| `reconciliation_canonical_bytes_authenticated` | Canonical bytes fully authenticated during reconciliation. |
| `reconciliation_source_bytes_compared` | Reconstructed canonical source bytes compared with the freshly opened destination. |
| `reconciliation_q_high_water` | Logical Q high-water attributable to reconciliation buffers/state. |

All six are zero when `reconciliation_calls = 0`. Lost ack requires each to be
positive and requires `reconciliation_q_high_water >= 32768`: the fixed
32-KiB comparison buffer is charged for its full lifetime and simultaneous
`stream_root` DFS/state charges raise, never replace, that value. For target/new,
`destination_bytes_read = output_length` and
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

Before source custody or build, read-only
`cargo metadata --offline --no-deps --format-version 1` must enumerate exactly
one `layerfs-engine` binary: `phase4_create_edit_benchmark` at
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`.
`phase4_g3_materialization.rs` must not be a Cargo target. The runner preflight,
self-check, and zero-row dry run enforce the same shape. The dry run records one
metadata child and zero build, benchmark, and analyzer children.

Each row's runner-added `source_set_sha256` must be a 64-hex digest equal to the
canonical digest in `SOURCE-CUSTODY-v13.json`. That record contains exactly the
four ordered source paths from the v13 preregistration, original/copy SHA-256,
size, original mode `0644`, copy mode `0400`, and distinct copy path. Binary
custody occurs only after the one recorded offline release build. Method custody
contains the v13 preregistration, dictionary, runner, primary analyzer,
independent analyzer, and post-static finalizer; the dry run is separately
hashed and copied.

The external `PREMEASUREMENT-FREEZE-v13.json` is outside that six-file method
set and binds the exact source-set digest, method-set digest, dry-run SHA-256,
and absent v13 result root/lock. Execution requires both exact environment
gates:

```text
G3_V13_EXECUTE=authorized-g3-v13-once
G3_V13_FREEZE_SHA256=<exact independently supplied anchor-file SHA-256>
```

The runner verifies them before lock/result creation, copies the anchor `0400`
to the result root, and binds its hash into method custody and campaign.
Isolated self-check fixtures reject one-at-a-time anchor, source, method, and
dry mutations.

`ENVIRONMENT-v13.json` records two actual selected child maps. Build fixes
`LANG=C`, `LC_ALL=C`, `TZ=UTC`, `RUST_BACKTRACE=0`, source set, and method set,
with no executable identity. Runtime adds the exact frozen executable SHA-256.
Selected `PATH`/`SHELL` are recorded and authorization variables are removed.
Every child rechecks its map before launch. Every raw row must exactly equal
the authoritative operand SHA, method-set SHA, environment-artifact SHA, and
its one exact argv from commands 3 through 11 of the frozen 14-command plan;
both analyzers and finalizer reject independent mutations of all four values.

For each source, `copy_path` is exactly
`source-custody-v13/<repository-relative-source-path>`, is relative with no empty,
dot, or `..` component, and resolves beneath the source-custody root. The copy
is a regular non-symlink whose device/inode differs from the original, with
`copy_size_bytes = size_bytes`, `copy_sha256 = sha256`, and mode `0400`.
Runner, both analyzers, and finalizer reject any missing path, escape, alias,
wrong size, wrong hash, wrong mode, or non-exact four-path set.

The exact repaired module SHA-256 is
`f9ffe7058761c60e7d81c5da18ed3d7a9afdb5344f41b9a97dcb8c2b8a51f032`;
the unchanged main benchmark SHA-256 is
`c78738ab213c7438544abdf2a37131652813873e30077469d578624f86ce3cdb`.
`DRY-RUN-v13.json` freezes all four source records and canonical source-set
digest `3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d`;
no v11 source identity is reused.

Only the current exact `results-v13/work-v13/<label>` row root may be removed.
`ROW-CLEANUP-v13.jsonl` has 18 fsynced alternating events. PREPARE binds the
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
append with parent sync on first creation. `FAILURE-v13.json` uses exclusive
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
hashes. It must rehash primary analysis as
`432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803`
and independent recomputation as
`86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e`;
ledger agreement alone is insufficient. No version-matched G2 path or filename is valid. The finalizer self-check reads and
verifies the complete G2-v5 anchor without creating any campaign or terminal
artifact.

Each analyzer's single `self_check_context` helper accepts only its exact source
directory or exact frozen campaign-copy directory. A source invocation validates
the fixed v13 source path and expected target/results shape. A copied invocation
uses `HERE.parent`, requires the exact v13 target/results/methodology namespace,
and derives the repository through `repo_from_results`; it never applies a
source-layout parent count to the copy. Both self-checks exercise an isolated
temporary exact copy shape and reject malformed source/copy and one-level-high
locations. Actual analysis still resolves the supplied `results-v13`, derives
the repository as `results.parents[2]`, and checks repository markers before
checking original/copy source bytes.

## Static closure and finalization

After a PASS campaign, the parent creates `STATIC-CLOSURE-v13.json` with schema
`phase4-g3-v13-static-closure-v1`, the frozen `source_set_sha256`,
`candidate_retained: true`, and these exact ordered PASS command labels:

```text
focused-g3-tests
workspace-tests
workspace-clippy
workspace-fmt-check
git-diff-check
custody-review
```

`focused-g3-tests` must report exactly 15 passed, 0 failed and include the
canonical proof underdeclared-range/target-digest replay cases, guard-before-
create temp/seed/clone fault cases, the exact combined publication `EIO` plus
cleanup `EACCES` provenance case, reconciliation fixed-buffer Q, and retained
route/fallback/publication tests.

The six complete argv and the 15 complete Rust test names are frozen verbatim
in the v13 prospective contract and `finalize_g3_v13.py`. Labels, a nonempty
argv, or the count summary alone are insufficient: sequence, label, argv,
stdout/stderr custody, exact named-test set, and 15-pass/0-fail summary must all
match.

Each command entry records its argv, zero exit code, and result-relative
stdout/stderr path, SHA-256, and size. `finalize_g3_v13.py` refuses incomplete or
non-PASS input, verifies campaign/ledger/custody/cleanup and sealed G2-v5
anchors, creates the payload manifest and PASS terminal, seals files `0444` and
directories `0555`, then performs independent manifest/hash/mode/closure
verification. The finalizer is never part of the measured campaign wall and is
not invoked by the zero-row dry run.
