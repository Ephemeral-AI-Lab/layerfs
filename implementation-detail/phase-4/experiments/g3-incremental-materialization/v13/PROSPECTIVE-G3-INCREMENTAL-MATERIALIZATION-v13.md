# Prospective G3-v13 incremental materialization contract

Status: **candidate methodology; freeze only after all self-checks and the
zero-row dry run pass**

v13 is the fresh evidence-protocol repair after the frozen, unexecuted v12 was
classified PREEXEC REVISE. The four v12 product repairs remain unchanged:
truthful reconciliation Q, guard-before-create cleanup ownership,
publication-first dual-error provenance, and stable canonical changed-range
proof binding. The accepted four source files and their bytes are identical to
v12. The sealed v11 package remains immutable historical evidence and
historical REVISE. No v11 or v12 row, binary, source copy, dry run, analysis,
closure, or terminal artifact is reused.

## Only v13 variables: five smallest evidence-protocol repairs

1. Finalization rehashes the exact sealed G2-v5 primary analysis
   `432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803`
   and independent recomputation
   `86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e`
   in addition to the existing manifest, terminal, terminal verification, raw,
   and normalized-ledger gates.
2. Static closure accepts exactly six ordered command sequences and argv, not
   labels alone. Its focused-test stdout must name exactly the prospectively
   frozen 15 tests once each and contain the exact 15-pass/0-fail summary.
3. An external `PREMEASUREMENT-FREEZE-v13.json`, outside the six-file method
   set, binds the source set, method set, dry-run SHA-256, and absent result root
   and lock. Execution requires both the one-shot token and an independently
   supplied environment value equal to the exact external file SHA-256.
4. Both analyzers and the finalizer reconstruct the authoritative 14-command
   plan and equate every raw row's executable, methodology, environment, and
   argv to the frozen operand/method/environment artifacts and its exact one of
   nine planned row commands. Independent synthetic mutations must reject all
   four identities.
5. The runner constructs the exact build and runtime child environments before
   use. `ENVIRONMENT-v13.json` records the selected values and their canonical
   identities; runtime additionally binds the frozen executable. Every child
   invocation rechecks its selected environment before `Popen`, and chronology
   records the applicable build/runtime identity.

Canonical relation proof work is preparation, is outside every measured
operation timer, and uses bounded streaming buffers only. It creates no durable
carrier and does not weaken the complete fallback. v13 persists no replayable
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
`<repo>/target/phase4-g3-incremental-materialization-20260822-v13/results-v13/methodology-v13`,
it uses `HERE.parent` as the results root and derives the repository only
through `repo_from_results`. Any malformed source/copy shape and the former
one-level-high repository are rejected.

The source-run synthetic self-check creates an isolated temporary repository
shape with the required markers and exact copied-method layout and exercises
the copied branch without creating the v13 result namespace or invoking a
child. The runner's planned copied `--self-check` commands are unchanged.

For actual campaign analysis, primary and independent analyzers continue to
derive the campaign repository only from the supplied, resolved results root.
It must be exactly
`<repo>/target/phase4-g3-incremental-materialization-20260822-v13/results-v13`,
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
normalized-ledger digest are unchanged. In particular, primary is
`432f903ecebe3afc6370e422c559e346f71abd71ba16f328d35e169e28732803`
and independent is
`86ab101df69f82ec548d8baa223ea4a6fde13646660969f6478a4e73fe08df5e`;
both files are rehashed, not inferred from ledger equality. The finalizer self-check executes the
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
parent-synced. v13 therefore claims crash durability for these evidence writes,
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

`CLEANUP-v13.json` requires 18 alternating events—nine exact PREPARE/COMPLETE
pairs in schedule order—every row absent, empty WORK removed, the exact anchored
deletion method, and peak storage computed only from PREPARE snapshots. Both
analyzers and the finalizer reject additions/substitutions, missing/misordered
COMPLETE, broken bindings, omitted methods, or cumulative-as-peak substitution.

The residual stat/unlink micro-race is an ordinary POSIX limitation. v13 claims
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
methodology records. After that dry run, the external repository-relative file
`implementation-detail/phase-4/experiments/g3-incremental-materialization/PREMEASUREMENT-FREEZE-v13.json`
binds exactly its source-set digest, methodology-set digest, dry-run SHA-256,
and absent v13 result root/lock. It is deliberately outside `METHOD_NAMES`, so
it does not create a self-hash cycle. The campaign refuses before lock or result
creation unless both are supplied exactly:

```text
G3_V13_EXECUTE=authorized-g3-v13-once
G3_V13_FREEZE_SHA256=<exact SHA-256 of PREMEASUREMENT-FREEZE-v13.json>
```

The anchor is copied read-only into the result root and bound by method custody,
campaign, analyzers, finalizer, and terminal. Changing the anchor, any source
or method byte, or the dry run changes a bound hash and requires a freshly
supplied exact anchor SHA. Runner self-check uses isolated files and rejects
one-at-a-time anchor, source, method, and dry mutations. Any later source or
method byte change invalidates the attempt; no v11 identity is reused.

Every source record must have the exact relative traversal-free `copy_path`
`source-custody-v13/<repository-relative-source-path>`. Its resolved path must
remain inside `results-v13/source-custody-v13`, be a regular non-symlink with a
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

Before build, the runner constructs a build-child environment with exact
`LANG=C`, `LC_ALL=C`, `TZ=UTC`, `RUST_BACKTRACE=0`, and the frozen source and
method digests; `G3_EXECUTABLE_SHA256` is absent. After operand freeze it
constructs the runtime child environment with those values plus the exact
executable digest. Selected `PATH` and `SHELL` are copied deliberately and
recorded. Authorization variables are removed from children. The environment
artifact records the actual selected build/runtime maps, their canonical
SHA-256 identities, the source/method/executable identities, and the exact
build/self-check/row/analyzer role map. Rows bind the environment artifact
SHA-256; analyzers and finalizer reject any executable, method, environment, or
row-argv mismatch.

## Fresh namespace and unchanged schedule

The only result namespace is
`target/phase4-g3-incremental-materialization-20260822-v13/results-v13`, guarded
by its fresh sibling `.lock`. Existing root or lock means refusal. The rows are,
in exact order: 10-MiB qualified no-op; 100-MiB one-byte patch; 10-MiB 1-MiB
patch; then 1-MiB invalid-authority, external-mutation, symlink-substitution,
count-change, before-publication-fault, and lost-ack. Each candidate row runs
once. The 100-MiB process retains its explicit 15-second preparation ceiling;
all other row children retain five seconds. Every operation is below five
seconds, their sum below 20 seconds, and build plus all custody, rows, analysis,
and cleanup below 59 seconds.

## v13 gate clarifications

- Lost acknowledgement accepts exactly either (`reconciliation_outcome` =
  `target`, `old_or_new` = `new`) or (`prior`, `old`). No cross-pair or other
  value passes. The deterministic v13 injection may yield `target`/`new`.
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
- Clone failure is a complete fallback reason and consumes zero permits. v13 has
  no clone-failure schedule row, so this is a schema/negative-self-check gate,
  not a new measurement.
- The primary and independent analyzers must reject a wrong source-set hash,
  lost-ack Q below 32768, a mutated authority binding claim, an
  invalid lost-ack old/new pair, bad cleanup custody, and a reason/precedence
  mismatch in their independent synthetic mutation suites. Each also rejects
  one-at-a-time executable, methodology, environment, and row-command
  mutations against independently reconstructed authoritative custody.
- Fault injection is operation-local and single-use; process-global fault state
  is forbidden.

## Exact static closure

After a PASS campaign, static closure must record exactly these six ordered
labels and argv:

```text
focused-g3-tests: cargo test -p layerfs-engine --bin phase4_create_edit_benchmark phase4_g3_materialization::tests --offline -- --test-threads=8
workspace-tests: cargo test --workspace --offline --all-targets
workspace-clippy: cargo clippy --workspace --offline --all-targets -- -D warnings
workspace-fmt-check: cargo fmt --all -- --check
git-diff-check: git diff --check
custody-review: python3 /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/experiments/g3-incremental-materialization/v13/finalize_g3_v13.py --self-check
```

The focused stdout must contain exactly these 15 named PASS lines and the
exact `15 passed; 0 failed` summary:

```text
phase4_g3_materialization::tests::canonical_range_proof_rejects_underdeclared_range_and_digest_replay
phase4_g3_materialization::tests::clone_miss_falls_back_without_consuming_single_use_permit
phase4_g3_materialization::tests::fclonefileat_clones_an_unlinked_read_only_seed_fd
phase4_g3_materialization::tests::g3_rows_cover_qualified_fallback_rejection_and_fault_routes
phase4_g3_materialization::tests::missing_destination_and_seed_are_complete_fallback_misses
phase4_g3_materialization::tests::patch_retry_resets_target_and_proves_one_exact_range
phase4_g3_materialization::tests::permit_rechecks_retained_directory_identity
phase4_g3_materialization::tests::publication_error_dominates_cleanup_error_with_both_provenances
phase4_g3_materialization::tests::reconciliation_q_charges_fixed_comparison_buffer_exactly
phase4_g3_materialization::tests::reconciliation_rejects_identity_change_during_complete_compare
phase4_g3_materialization::tests::rename_error_cleans_target_temp_and_preserves_prior_failure
phase4_g3_materialization::tests::seed_post_create_failure_leaves_no_named_residue
phase4_g3_materialization::tests::stream_root_dfs_q_decharges_after_success_and_writer_error
phase4_g3_materialization::tests::symlink_preflight_precedes_invalid_authority_for_every_scenario
phase4_g3_materialization::tests::temp_counter_failure_leaves_no_named_residue
```

The finalizer rechecks command sequence, labels, complete argv, stream
hashes/sizes, exact test-name set, and summary before it considers a terminal.

No measured campaign, build, static closure, or finalizer is authorized by this
contract alone. The exact one-shot token and frozen zero-row dry run are
necessary but not sufficient without parent authorization. A pre-execution
defect discovered after identities freeze requires a fresh attempt version.
Even a terminal v13 PASS stops at `G3 COMPLETE; G4 READY`; it does not execute
any G4 row.
