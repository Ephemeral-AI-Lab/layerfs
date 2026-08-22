# Prospective G3-v10 incremental materialization contract

Status: **frozen before build or measurement**

v10 is the pre-execution copied-analyzer-self-check repair after zero-row v9.
It freezes the same candidate bytes, source-copy proof, evidence durability,
descriptor-anchored cleanup, Attempt-B mechanism, routes, counters, nine-row
schedule, limits, and gates. No historical row is reused.

## Only protocol change: execution-location-aware analyzer self-checks

Each analyzer has one small `self_check_context` helper which accepts exactly
two locations. At the exact source location it derives the repository from the
fixed source-method path and validates the expected target/results path. At the
exact frozen copy location
`<repo>/target/phase4-g3-incremental-materialization-20260822-v10/results-v10/methodology-v10`,
it uses `HERE.parent` as the results root and derives the repository only
through `repo_from_results`. Any malformed source/copy shape and the former
one-level-high repository are rejected.

The source-run synthetic self-check creates an isolated temporary repository
shape with the required markers and exact copied-method layout and exercises
the copied branch without creating the v10 result namespace or invoking a
child. The runner's planned copied `--self-check` commands are unchanged.

For actual campaign analysis, primary and independent analyzers continue to
derive the campaign repository only from the supplied, resolved results root.
It must be exactly
`<repo>/target/phase4-g3-incremental-materialization-20260822-v10/results-v10`,
with exact target/results names and repository markers `Cargo.lock`,
`crates/layerfs-engine/Cargo.toml`, and `.git`. The derived repository is
`results.parents[2]`; arbitrary roots, traversal aliases, and the prior
one-level-high location are rejected.

Actual campaign custody always receives the repository from
`repo_from_results`, never from the copied analyzer's method path.

## Retained exact sealed G2-v5 dependency

The finalizer pins its historical dependency to exactly
`target/phase4-g2-materialization-decomposition-20260822-v5/results-v5` and the
sealed v5 payload manifest, terminal, terminal verification, raw JSONL, primary
analysis, and independent recomputation filenames. Their known hashes and
normalized-ledger digest are unchanged. The finalizer self-check executes the
complete read-only `verify_g2()` and asserts the exact observed hash map and
primary/independent ledger agreement. It creates no G3 files.

## Retained evidence durability

Before cleanup PREPARE, each child stdout and stderr file is flushed and fsynced
and its parent directory is fsynced. The enriched raw JSONL entry is appended,
flushed, and fsynced; the raw file and its parent directory entry are durably
established before any row entry is appended. Thus durable PREPARE cannot
precede the evidence it claims was captured.

The fresh target/results roots are directory-fsynced before chronology begins.
Chronology appends are flushed/fsynced, with the parent synced when the file is
first created. A failure record is exclusively created, flushed/fsynced, and
parent-synced. v10 therefore claims crash durability for these evidence writes,
not merely ordinary-process preservation.

## Retained exact per-row transient retirement

After each once-only child, the runner first durably captures stdout/stderr,
the parsed/enriched raw row, external time/RSS, and exactness/residue fields. A
retained WORK dirfd opens the exact row with `O_DIRECTORY|O_NOFOLLOW`; that same
row dirfd remains open through inventory, fsynced PREPARE, and deletion.
Inventory uses descriptor-relative `listdir`, no-follow `stat`, and child
`openat`, recording each path, kind, device/inode, mode, link count, size,
mtime/ctime, and allocation. The cleanup log is precreated and its parent synced.

PREPARE binds the sorted inventory/hash/count and pre-delete row/WORK snapshots,
then is append-flushed-fsynced before removal. The runner fully revalidates the
inventory, verifies every directory's immediate names and identities, and uses
only descriptor-relative no-follow unlink/rmdir for the frozen set. Any late
addition or substitution aborts; no unexpected entry is deleted. After exact
delete-set equality, row-name identity is rechecked through WORK, the root is
removed, WORK is fsynced, and a bound COMPLETE is append-flushed-fsynced.

`CLEANUP-v10.json` requires 18 alternating events—nine exact PREPARE/COMPLETE
pairs in schedule order—every row absent, empty WORK removed, the exact anchored
deletion method, and peak storage computed only from PREPARE snapshots. Both
analyzers and the finalizer reject additions/substitutions, missing/misordered
COMPLETE, broken bindings, omitted methods, or cumulative-as-peak substitution.

The residual stat/unlink micro-race is an ordinary POSIX limitation. v10 claims
process custody of its fresh private result namespace during the runner, not
atomic correctness against a malicious same-UID process with direct namespace
access.

## Retained complete build custody

Before building, the runner copies, hashes, sizes, and mode-checks exactly:

1. `Cargo.lock`;
2. `crates/layerfs-engine/Cargo.toml`;
3. `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`;
4. `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs`.

The ordered canonical JSON list of repository-relative path, SHA-256, size, and
source mode defines `source_set_sha256`. All source modes must be `0644`; each
distinct retained copy is `0400` and byte-identical. The source set is checked
again after build and after all rows. Every raw row, campaign record, and
terminal record binds the same `source_set_sha256`.

Every source record must have the exact relative traversal-free `copy_path`
`source-custody-v10/<repository-relative-source-path>`. Its resolved path must
remain inside `results-v10/source-custody-v10`, be a regular non-symlink with a
different device/inode pair from the original, and satisfy
`copy_size_bytes = size_bytes`, `copy_sha256 = sha256`, and copy mode `0400`.
The runner, primary analyzer, independent analyzer, and finalizer each enforce
this independently. Their self-checks reject missing paths, escape paths, and
wrong copy sizes.

The runner records and invokes exactly once, inside the 59-second campaign
wall and before freezing the executable:

```text
cargo build --release -p layerfs-engine --bin phase4_create_edit_benchmark --offline
```

The build is outside every operation timer, has a hard 30-second child ceiling,
and may not be retried. The resulting release executable is copied once to the
fresh result root, made `0500`, and all rows execute only that copy.

## Fresh namespace and unchanged schedule

The only result namespace is
`target/phase4-g3-incremental-materialization-20260822-v10/results-v10`, guarded
by its fresh sibling `.lock`. Existing root or lock means refusal. The rows are,
in exact order: 10-MiB qualified no-op; 100-MiB one-byte patch; 10-MiB 1-MiB
patch; then 1-MiB invalid-authority, external-mutation, symlink-substitution,
count-change, before-publication-fault, and lost-ack. Each candidate row runs
once. The 100-MiB process retains its explicit 15-second preparation ceiling;
all other row children retain five seconds. Every operation is below five
seconds, their sum below 20 seconds, and build plus all custody, rows, analysis,
and cleanup below 59 seconds.

## v10 gate clarifications

- Lost acknowledgement accepts exactly either (`reconciliation_outcome` =
  `target`, `old_or_new` = `new`) or (`prior`, `old`). No cross-pair or other
  value passes. The deterministic v10 injection may yield `target`/`new`.
- Primary patch counters exclude fault reconciliation, preserving the bounded
  one-byte mechanism claim. Lost-ack separately reports
  `reconciliation_sql_queries`, `reconciliation_sql_rows`,
  `reconciliation_blob_reads`,
  `reconciliation_canonical_bytes_authenticated`,
  `reconciliation_source_bytes_compared`, and
  `reconciliation_q_high_water`. They must truthfully charge the complete
  target/prior comparisons; destination bytes and source bytes compared are at
  least the target length, and total Q includes reconciliation Q.
- Reconciliation requires stable pre/post descriptor identity and a final
  no-follow observation proving the destination name still identifies that
  descriptor. Exact stable bytes different from target and prior return
  `PublicationConflict`; read, wrong-kind, descriptor/name instability, or
  other observational uncertainty returns `AmbiguousDurability`. A prior result
  preserves the original publication error and cleanup ownership follows
  whether rename actually consumed the temp.
- A symlink or wrong-kind final component is a route-precedence typed preflight
  rejection before authority or payload work. The scenario's exact injected
  symlink still expects `destination-symlink` and `NativeDestinationSymlink`;
  analyzers express the generic precedence predicate so a later wrong-kind
  fixture cannot be laundered into fallback.
- Clone failure is a complete fallback reason and consumes zero permits. v10 has
  no clone-failure schedule row, so this is a schema/negative-self-check gate,
  not a new measurement.
- The primary and independent analyzers must reject a wrong source-set hash, an
  invalid lost-ack old/new pair, and a reason/precedence mismatch in their
  synthetic mutation suites.
- Fault injection is operation-local and single-use; process-global fault state
  is forbidden.

No measured campaign is authorized by the dry run. A fresh v11 is required for
any pre-execution defect discovered after v10 is frozen.
