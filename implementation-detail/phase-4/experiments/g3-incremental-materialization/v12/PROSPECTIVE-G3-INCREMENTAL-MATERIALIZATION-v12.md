# Prospective G3-v12 incremental materialization contract

Status: **frozen after independent repaired-source audit PASS and before build
or measurement**

v12 is the fresh repair attempt required after independent post-seal review
found four contract defects in otherwise reproducible v11 evidence. The sealed
v11 package remains immutable historical evidence and is classified REVISE by
the external post-seal disposition. v12 retains Attempt B, the full
authenticated fallback, nine-row schedule, evidence durability,
descriptor-anchored cleanup, source/method/binary custody, final static closure,
and exact sealed G2-v5 gates. It reuses no v11 row, binary, source copy, dry run,
analysis, closure, or terminal artifact.

## Only implementation variables: four smallest contract repairs

1. Reconciliation logically charges the fixed 32-KiB comparison buffer for
   its entire lifetime in addition to `stream_root` DFS/state Q. A lost-ack row
   with `reconciliation_q_high_water < 32768` is impossible and rejected.
2. Temp, seed, and clone-temp creation acquire the cleanup-capable private
   directory handle before the namespace-creating syscall. The cleanup guard is
   armed immediately after, and only after, successful name creation. Focused
   injected failures prove no named residue at each former guard gap.
3. If publication and cleanup both fail, the typed result preserves the
   publication error as primary and the cleanup error as secondary detail. A
   cleanup failure may never replace the publication failure.
4. Before permit minting, stable `O_NOFOLLOW` read-only regular preparation
   descriptors are fully compared against their authenticated canonical parent
   and target roots, then compared outside the declared changed range. The
   resulting non-`Clone` proof is consumed by minting. The keyed permit commits
   to store/validation/profile/epoch/generation/receipt continuity, canonical
   parent and target roots, both authenticated digests including target digest,
   the exact changed range, canonical-range commitment, destination authority,
   operation, nonce, publication serial, and protected seed identity. Permit
   validation recomputes that commitment from current authority and operands.

Canonical relation proof work is preparation, is outside every measured
operation timer, and uses bounded streaming buffers only. It creates no durable
carrier and does not weaken the complete fallback. v12 persists no replayable
destination receipt: permit key, nonce, proof ownership, and publication serial
remain operation-local/private authority and are not a reusable destination
sidecar.

## Retained explicit Cargo binary topology

The package retains `autobins = false` plus the one explicit binary named
`phase4_create_edit_benchmark` at
`src/bin/phase4_create_edit_benchmark.rs`.

Before any build, the runner's read-only preflight executes exactly:

```text
cargo metadata --offline --no-deps --format-version 1
```

It requires exactly one `layerfs-engine` package and exactly one binary target,
with name `phase4_create_edit_benchmark` and repository-relative source path
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`. The path
`crates/layerfs-engine/src/bin/phase4_g3_materialization.rs` must not appear as
any target. Missing, extra, renamed, or redirected targets refuse before the
release build. Runner self-check and the zero-row dry run execute the same
read-only assertion; the dry run records one metadata child and zero build,
benchmark-row, and analyzer children.

## Retained execution-location-aware analyzer self-checks

Each analyzer has one small `self_check_context` helper which accepts exactly
two locations. At the exact source location it derives the repository from the
fixed source-method path and validates the expected target/results path. At the
exact frozen copy location
`<repo>/target/phase4-g3-incremental-materialization-20260822-v12/results-v12/methodology-v12`,
it uses `HERE.parent` as the results root and derives the repository only
through `repo_from_results`. Any malformed source/copy shape and the former
one-level-high repository are rejected.

The source-run synthetic self-check creates an isolated temporary repository
shape with the required markers and exact copied-method layout and exercises
the copied branch without creating the v12 result namespace or invoking a
child. The runner's planned copied `--self-check` commands are unchanged.

For actual campaign analysis, primary and independent analyzers continue to
derive the campaign repository only from the supplied, resolved results root.
It must be exactly
`<repo>/target/phase4-g3-incremental-materialization-20260822-v12/results-v12`,
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
parent-synced. v12 therefore claims crash durability for these evidence writes,
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

`CLEANUP-v12.json` requires 18 alternating events—nine exact PREPARE/COMPLETE
pairs in schedule order—every row absent, empty WORK removed, the exact anchored
deletion method, and peak storage computed only from PREPARE snapshots. Both
analyzers and the finalizer reject additions/substitutions, missing/misordered
COMPLETE, broken bindings, omitted methods, or cumulative-as-peak substitution.

The residual stat/unlink micro-race is an ordinary POSIX limitation. v12 claims
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

The independently accepted source identities are:

| Path | SHA-256 |
|---|---|
| `Cargo.lock` | `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8` |
| `crates/layerfs-engine/Cargo.toml` | `35fd9c667575fdb3dd6ae720c4c43e6c654a9fd47da8b5dadc9f7672bd04498d` |
| `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` | `c78738ab213c7438544abdf2a37131652813873e30077469d578624f86ce3cdb` |
| `crates/layerfs-engine/src/bin/phase4_g3_materialization.rs` | `f9ffe7058761c60e7d81c5da18ed3d7a9afdb5344f41b9a97dcb8c2b8a51f032` |
| canonical source set | `3a0330fc12cdc9b05b949a3f3f2b39f47e8d41d41234fffeedaa0ec65449058d` |

The final zero-row dry run binds those four records and the final six
methodology records. Any later source or methodology byte change invalidates
that dry run and requires a fresh attempt version; no v11 identity is reused.

Every source record must have the exact relative traversal-free `copy_path`
`source-custody-v12/<repository-relative-source-path>`. Its resolved path must
remain inside `results-v12/source-custody-v12`, be a regular non-symlink with a
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
`target/phase4-g3-incremental-materialization-20260822-v12/results-v12`, guarded
by its fresh sibling `.lock`. Existing root or lock means refusal. The rows are,
in exact order: 10-MiB qualified no-op; 100-MiB one-byte patch; 10-MiB 1-MiB
patch; then 1-MiB invalid-authority, external-mutation, symlink-substitution,
count-change, before-publication-fault, and lost-ack. Each candidate row runs
once. The 100-MiB process retains its explicit 15-second preparation ceiling;
all other row children retain five seconds. Every operation is below five
seconds, their sum below 20 seconds, and build plus all custody, rows, analysis,
and cleanup below 59 seconds.

## v12 gate clarifications

- Lost acknowledgement accepts exactly either (`reconciliation_outcome` =
  `target`, `old_or_new` = `new`) or (`prior`, `old`). No cross-pair or other
  value passes. The deterministic v12 injection may yield `target`/`new`.
- Primary patch counters exclude fault reconciliation, preserving the bounded
  one-byte mechanism claim. Lost-ack separately reports
  `reconciliation_sql_queries`, `reconciliation_sql_rows`,
  `reconciliation_blob_reads`,
  `reconciliation_canonical_bytes_authenticated`,
  `reconciliation_source_bytes_compared`, and
  `reconciliation_q_high_water`. They must truthfully charge the complete
  target/prior comparisons; destination bytes and source bytes compared are at
  least the target length. Reconciliation Q is the simultaneous fixed 32768-byte
  comparison-buffer charge plus `stream_root` DFS/state Q, so its high-water is
  at least 32768 and total Q is at least reconciliation Q.
- Reconciliation requires stable pre/post descriptor identity and a final
  no-follow observation proving the destination name still identifies that
  descriptor. Exact stable bytes different from target and prior return
  `PublicationConflict`; read, wrong-kind, descriptor/name instability, or
  other observational uncertainty returns `AmbiguousDurability`. A prior result
  preserves the original publication error. If cleanup also fails, both typed
  provenance details are retained with publication first. Cleanup ownership
  follows whether rename actually consumed the temp.
- Both analyzers reject a missing/reordered public authority binding claim.
  Target digest, exact range, proof commitment, non-`Clone` ownership, and
  descriptor stability are private typed internals covered by frozen source
  custody plus focused tests; they are not separately inferable from successful
  row JSON. Static closure must run the focused proof tests that reject an
  underdeclared range and target-digest replay and prove single-use ownership.
- Static closure must run the focused guard-gap and dual-error tests. Row
  analyzers independently enforce created/removed equations and zero residue,
  but do not pretend those counters alone identify syscall ordering or both
  hidden error objects.
- A symlink or wrong-kind final component is a route-precedence typed preflight
  rejection before authority or payload work. The scenario's exact injected
  symlink still expects `destination-symlink` and `NativeDestinationSymlink`;
  analyzers express the generic precedence predicate so a later wrong-kind
  fixture cannot be laundered into fallback.
- Clone failure is a complete fallback reason and consumes zero permits. v12 has
  no clone-failure schedule row, so this is a schema/negative-self-check gate,
  not a new measurement.
- The primary and independent analyzers must reject a wrong source-set hash,
  lost-ack Q below 32768, a mutated authority binding claim, an
  invalid lost-ack old/new pair, bad cleanup custody, and a reason/precedence
  mismatch in their independent synthetic mutation suites.
- Fault injection is operation-local and single-use; process-global fault state
  is forbidden.

No measured campaign, build, static closure, or finalizer is authorized by this
contract alone. The exact one-shot token and frozen zero-row dry run are
necessary but not sufficient without parent authorization. A pre-execution
defect discovered after identities freeze requires a fresh attempt version.
Even a terminal v12 PASS stops at `G3 COMPLETE; G4 READY`; it does not execute
any G4 row.
