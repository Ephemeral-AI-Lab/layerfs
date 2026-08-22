# Prospective G3-v6 incremental materialization contract

Status: **frozen before build or measurement**

v6 is the evidence-durability-only repair after zero-row v5's final audit. It
freezes the same candidate bytes, source-copy proof, descriptor-anchored cleanup,
Attempt-B mechanism, routes, counters, nine-row schedule, limits, and gates. No
historical row is reused.

## One protocol change: durable row evidence before PREPARE

Before cleanup PREPARE, each child stdout and stderr file is flushed and fsynced
and its parent directory is fsynced. The enriched raw JSONL entry is appended,
flushed, and fsynced; the raw file and its parent directory entry are durably
established before any row entry is appended. Thus durable PREPARE cannot
precede the evidence it claims was captured.

The fresh target/results roots are directory-fsynced before chronology begins.
Chronology appends are flushed/fsynced, with the parent synced when the file is
first created. A failure record is exclusively created, flushed/fsynced, and
parent-synced. v6 therefore claims crash durability for these evidence writes,
not merely ordinary-process preservation.

## One protocol change: exact per-row transient retirement

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

`CLEANUP-v6.json` requires 18 alternating events—nine exact PREPARE/COMPLETE
pairs in schedule order—every row absent, empty WORK removed, the exact anchored
deletion method, and peak storage computed only from PREPARE snapshots. Both
analyzers and the finalizer reject additions/substitutions, missing/misordered
COMPLETE, broken bindings, omitted methods, or cumulative-as-peak substitution.

The residual stat/unlink micro-race is an ordinary POSIX limitation. v6 claims
process custody of its fresh private result namespace during the runner, not
atomic correctness against a malicious same-UID process with direct namespace
access.

## One changed protocol variable: complete build custody

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
`source-custody-v6/<repository-relative-source-path>`. Its resolved path must
remain inside `results-v6/source-custody-v6`, be a regular non-symlink with a
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
`target/phase4-g3-incremental-materialization-20260822-v6/results-v6`, guarded
by its fresh sibling `.lock`. Existing root or lock means refusal. The rows are,
in exact order: 10-MiB qualified no-op; 100-MiB one-byte patch; 10-MiB 1-MiB
patch; then 1-MiB invalid-authority, external-mutation, symlink-substitution,
count-change, before-publication-fault, and lost-ack. Each candidate row runs
once. The 100-MiB process retains its explicit 15-second preparation ceiling;
all other row children retain five seconds. Every operation is below five
seconds, their sum below 20 seconds, and build plus all custody, rows, analysis,
and cleanup below 59 seconds.

## v6 gate clarifications

- Lost acknowledgement accepts exactly either (`reconciliation_outcome` =
  `target`, `old_or_new` = `new`) or (`prior`, `old`). No cross-pair or other
  value passes. The deterministic v6 injection may yield `target`/`new`.
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
- Clone failure is a complete fallback reason and consumes zero permits. v6 has
  no clone-failure schedule row, so this is a schema/negative-self-check gate,
  not a new measurement.
- The primary and independent analyzers must reject a wrong source-set hash, an
  invalid lost-ack old/new pair, and a reason/precedence mismatch in their
  synthetic mutation suites.
- Fault injection is operation-local and single-use; process-global fault state
  is forbidden.

No measured campaign is authorized by the dry run. A fresh v7 is required for
any pre-execution defect discovered after v6 is frozen.
