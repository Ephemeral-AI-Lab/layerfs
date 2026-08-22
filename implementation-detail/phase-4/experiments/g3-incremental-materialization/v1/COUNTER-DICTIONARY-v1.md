# G3-v1 direct-counter dictionary

Status: **frozen before build, dry-run, or measurement**

The candidate command is exactly:

```text
phase4_create_edit_benchmark --g3-row ROOT SIZE SCENARIO
```

It prints one compact JSON object and no other stdout. All counter and timer
fields below are non-negative JSON integers. Boolean fields are JSON booleans;
`error` is JSON null on success. The runner adds sequence, command, frozen
custody hashes, environment hash, exit status, and `/usr/bin/time -l` values.

## Frozen rows

| Sequence | `scenario` | `size_bytes` | `route` | `qualification_reason` | `outcome` / `error` |
|---:|---|---:|---|---|---|
| 1 | `qualified-noop` | 10485760 | `qualified-noop` | `seed-hit` | `success` / null |
| 2 | `qualified-one-byte` | 104857600 | `qualified-patch` | `seed-hit` | `success` / null |
| 3 | `qualified-one-mib` | 10485760 | `qualified-patch` | `seed-hit` | `success` / null |
| 4 | `invalid-authority` | 1048576 | `complete-fallback` | `invalid-authority` | `success` / null |
| 5 | `external-mutation` | 1048576 | `complete-fallback` | `destination-invalidated` | `success` / null |
| 6 | `symlink-substitution` | 1048576 | `typed-rejection` | `destination-symlink` | `typed-error` / `NativeDestinationSymlink` |
| 7 | `count-change` | 1048576 | `complete-fallback` | `count-change` | `success` / null |
| 8 | `before-publication-fault` | 1048576 | `qualified-patch` | `seed-hit` | `typed-error` / `InjectedBeforePublication` |
| 9 | `lost-ack` | 1048576 | `qualified-patch` | `seed-hit` | `success` / null |

No row may be skipped, inserted, reordered, or rerun. A row process has a hard
five-second ceiling except the 100-MiB row, whose prospectively explicit
15-second process ceiling permits unmeasured preparation. Every reported
`operation_total_ns` remains strictly below five seconds, their sum remains
strictly below 20 seconds, and the complete campaign remains strictly below 59
seconds.

## Identity, route, and authority

| Field | Class | Meaning |
|---|---|---|
| `schema` | Observed | Exact value `phase4-g3-row-v1`. |
| `scenario`, `size_bytes` | Observed | Exact frozen row identity above. |
| `route`, `qualification_reason`, `outcome`, `error` | Observed | Exact route decision and typed result above. |
| `generation` | Observed | Store generation checked for the operation. |
| `parent_root`, `target_root` | Observed | Authenticated file-level parent and requested roots; never path or harness fingerprints. |
| `authority_bindings_checked` | Observed | Exact checked set: `store_instance`, `validation_authority`, `profile`, `integrity_epoch`, `generation`, `receipt_transition`, `parent_root`, `target_root`, `destination_identity`, `open_serial`, `mutation_serial`, `publication_serial`, `operation`, `nonce`, `seed_identity`. The preflight symlink rejection reports an empty list. |
| `authority_reads`, `authority_bytes_read` | Observed | Logical authority record reads and their encoded bytes. |
| `seed_authority_reads`, `seed_authority_bytes_read` | Observed | Protected seed descriptor/capability reads and checked bytes. These are not destination payload reads. |
| `authority_validations` | Observed | Permit/binding validation attempts. |
| `authority_validation_successes`, `authority_validation_failures` | Observed | Exact partition of `authority_validations`. |
| `permit_consumptions` | Observed | Successful single-use fast permits consumed; one on qualified routes and zero on every fallback/rejection. |

The authority equations are:

```text
authority_validations = authority_validation_successes
                      + authority_validation_failures

qualified route: successes >= 1; failures = 0; permit_consumptions = 1
invalid authority: successes = 0; failures >= 1; permit_consumptions = 0
fallback or preflight rejection: permit_consumptions = 0
```

`external-mutation` and `count-change` may validate the logical authority, but
must reject incremental use before permit consumption. A fast-path miss never
mints authority.

## Payload, clone, patch, and fallback work

| Field | Class | Meaning |
|---|---|---|
| `mapping_sql_queries`, `mapping_sql_rows` | Observed | Mapping queries and returned rows in the operation. |
| `object_sql_queries`, `object_sql_rows` | Observed | Canonical object queries and returned rows. |
| `payload_sql_queries`, `payload_sql_rows` | Derived | Respectively mapping plus object queries/rows. |
| `canonical_blob_reads`, `canonical_blob_bytes` | Observed | Canonical BLOB acquisitions and bytes acquired. |
| `authenticated_objects`, `canonical_bytes_authenticated` | Observed | Fully authenticated canonical objects and bytes. |
| `source_bytes_reconstructed` | Observed | Raw output bytes produced by authenticated logical reconstruction. |
| `destination_bytes_read` | Observed | Bytes read from the mutable destination by the route or reconciliation. It excludes the independent post-operation exactness check. |
| `verification_bytes_read` | Observed | Bytes read only by the independent post-operation output oracle, outside `operation_total_ns`. |
| `clone_calls`, `clone_successes`, `clone_failures` | Observed | Native clone dispatch/result counters. |
| `clone_source_logical_bytes` | Derived | Seed logical length presented to a successful clone. It is not copied bytes, allocation, sharing, or physical-I/O evidence. |
| `copy_calls`, `copied_payload_bytes` | Observed | Explicit byte-copy fallback work distinct from authenticated reconstruction. |
| `patch_calls`, `patch_bytes` | Observed | Positioned changed-range write calls and bytes. |
| `fallback_calls`, `fallback_write_bytes` | Observed | Complete authenticated fallback invocations and published payload bytes. |
| `changed_ranges`, `changed_bytes` | Observed | Coalesced sorted disjoint authenticated ranges and `sum(end - start)`. |

Required equations and bounds:

```text
payload_sql_queries = mapping_sql_queries + object_sql_queries
payload_sql_rows    = mapping_sql_rows    + object_sql_rows
canonical_blob_bytes = canonical_bytes_authenticated

qualified-noop:
  payload SQL/rows = BLOB/authentication/reconstruction = 0
  copy/patch/fallback payload work = 0
  clone_calls/clone_successes/clone_failures = 1/1/0

qualified patch:
  clone_calls/clone_successes/clone_failures = 1/1/0
  fallback_calls = source_bytes_reconstructed = copied_payload_bytes = 0
  patch_bytes = changed_bytes
  canonical_bytes_authenticated <= changed_bytes + 1048576

qualified-one-byte: changed_ranges = 1; changed_bytes = patch_bytes = 1
qualified-one-mib:  changed_ranges = 1; changed_bytes = patch_bytes = 1048576

complete fallback:
  fallback_calls = 1
  source_bytes_reconstructed = fallback_write_bytes = output_length
  clone_calls = patch_calls = copied_payload_bytes = 0
```

`before-publication-fault` and `lost-ack` exercise a one-byte qualified patch.
The symlink rejection performs no authority, payload, clone, copy, patch,
fallback, temporary-file, sync, rename, or reconciliation work.

## Native publication, storage, and exactness

| Field | Class | Meaning |
|---|---|---|
| `metadata_operations` | Observed | Exact native metadata applications. |
| `temp_files_created`, `temp_files_removed` | Observed | Unique candidate creations and explicit cleanup unlinks. A successful rename consumes one temp name. |
| `seed_files_created`, `seed_files_removed` | Observed | Private seed names created and unlinked; retained read-only FDs do not count as named residue. |
| `data_sync_calls`, `metadata_sync_calls` | Observed | Separately attributed file data and metadata durability dispatches. |
| `rename_calls`, `directory_sync_calls` | Observed | Atomic publication and containing-directory durability dispatches. |
| `reconciliation_calls`, `reconciliation_outcome` | Observed | Fresh no-follow publication observations; `not-needed` except `lost-ack`, which requires one call and `target`. |
| `temp_logical_bytes`, `temp_apparent_bytes`, `temp_allocated_bytes` | Observed | Peak candidate namespace logical/apparent/allocated bytes. |
| `seed_logical_bytes`, `seed_apparent_bytes`, `seed_allocated_bytes` | Observed | Peak seed logical/apparent/allocated bytes. |
| `output_length`, `output_mode` | Observed | Independently observed final logical length and integer Unix mode. |
| `output_digest`, `expected_output_digest` | Observed | Independent complete output oracle and requested/prior oracle as appropriate. |
| `byte_exact`, `mode_exact` | Derived | Exact digest/length and metadata equality. |
| `old_or_new` | Observed | `new` after success, `old` after prepublication rejection/fault. |
| `temp_residue_count`, `seed_residue_count` | Observed | Reserved-namespace residue after the row. Both must be zero. |

For a published row:

```text
temp_files_created = temp_files_removed + rename_calls
metadata_operations >= 1
data_sync_calls = metadata_sync_calls = rename_calls = directory_sync_calls = 1
```

`before-publication-fault` requires `rename_calls = 0`, exact old output, and
`temp_files_created = temp_files_removed`. `lost-ack` requires one rename, one
fresh reconciliation with `target`, exact new output, and completed directory
sync. The count-changing target length is 1048577; all other output lengths are
their row `size_bytes`. Every row must be byte/mode exact with zero residue.

The per-row temp and seed storage counters and the runner-observed transient
logical/apparent/allocated peaks each have a 512-MiB ceiling. Only the runner's
fresh exact `work-v1` subtree may be removed, by enumerated no-follow unlink and
`rmdir`; no glob, repository cleanup, or unrelated recursive deletion is
permitted.

## Timers and resources

The candidate observes:

```text
attributed_wall_ns = timer_preflight_ns
                   + timer_qualification_ns
                   + timer_payload_prepare_ns
                   + timer_data_sync_ns
                   + timer_metadata_ns
                   + timer_metadata_sync_ns
                   + timer_rename_ns
                   + timer_directory_sync_ns
                   + timer_reconciliation_ns
                   + timer_cleanup_ns

operation_total_ns = attributed_wall_ns + unattributed_wall_ns
unattributed_wall_ns >= 0
```

`q_high_water` is the operation logical-memory high-water and `q_terminal` is
the value after report buffers are dropped. G3-v1 prospectively requires
`q_terminal = 0` and `q_high_water <= 20971520`. The runner adds
`external_real_seconds`, `external_user_seconds`, `external_system_seconds`,
`maximum_resident_set_bytes`, and `peak_memory_footprint_bytes` from
`/usr/bin/time -l`; maximum RSS must not exceed 20 MiB.

`physical_io_status`, `cache_warmth_status`, and `stable_media_status` must each
begin with `Unavailable:` and give the source/reason. Logical length,
allocation, clone success, block-operation counts, CPU, RSS, Q, and wall time
must not be relabeled as physical I/O, cache warmth, clone sharing, or stable
media durability.

## Runner-added custody fields

The runner adds `sequence`, `label`, `command`, `child_timeout_seconds`,
`child_exit_code`, `executable_sha256`, `source_sha256`,
`methodology_set_sha256`, `environment_sha256`, external CPU/RSS fields, and
block/context-switch counts where `/usr/bin/time -l` exposes them. It snapshots
the executable, source, and all five non-dry-run method files before the first
row, rechecks their hashes after the final row, captures raw stdout/stderr and
chronology exactly once, and refuses any existing v1 result root or lock.

Primary analysis and independent recomputation share no code. Each performs
synthetic schedule, authority, route, counter, timer, fallback, publication,
exactness, resource, and cleanup mutations before it is eligible to analyze a
campaign. Their normalized ledgers must agree exactly.
