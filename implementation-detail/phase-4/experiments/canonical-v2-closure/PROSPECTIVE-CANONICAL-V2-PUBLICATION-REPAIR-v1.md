# Canonical-v2 publication repair — prospective compact screen v1

Date: 2026-08-21. This contract applies only to a fresh result namespace:
`target/phase4-canonical-v2-publication-repair-20260821-v1/results-v1`.
All earlier canonical-v2 evidence remains historical and unchanged. In
particular, the production-shaped v3 result remains `CANONICAL-V2 REVISE`; it
is not retroactively relabeled.

## One variable and stop boundary

The candidate may only replace the redundant full-graph validation inside
publication with consumption of already-established, transaction-bound
publication authority, and route a same-count one-byte edit through the
existing changed-spine qualification. Raw or unqualified callers must retain
the full-validation fallback. No codec, identity, CDC, CAS, COW, schema,
format, write shape, transaction count, durability setting, retry, cache,
worker, pool, VFS, or later optimization changes are in scope.

This is a benchmark-private repair screen. It neither promotes canonical-v2
nor authorizes integration, migration, another optimization, or a commit.

## One global clock

One monotonic supervisor begins before focused validation and ends only after
the release build, custody checks, preparation, warmup, all measured rows,
analysis, disposition, manifest creation, and manifest verification. Its hard
ceiling is 119 seconds, strictly below the user's 120-second total. Every child
gets the smaller of 59 seconds and the remaining global budget. Timeout is
`CANONICAL-V2 PUBLICATION-REPAIR REVISE / TIME-BUDGET`; no row is deleted or
selectively rerun.

Validation is one offline filtered test command selecting exactly four
`publication_repair_` tests and one offline release build. The filtered tests
are the protected smoke: they
must cover qualified one-use publication, wrong/stale scope rejection,
full-validation fallback, rollback/terminal-Q cleanup, and same-count
one-byte changed-spine selection. No workspace-wide or long test matrix is
part of this screen.

## Frozen inputs and preparation

The CP-0009 control executable and source remain frozen at:

- executable SHA-256
  `9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7`;
- source SHA-256
  `3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a`.

The retained 104,857,600-byte K64/F64 fixture must have SHA-256
`63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`.
The candidate is built once, copied once into the result operands, hashed, and
used for every B row. Candidate source hashes and status are captured before
rows. Every scheduled row receives a distinct physical path/inode copied from
its immutable arm/operation master; database, authority, and expectations
hashes are recorded before invocation. Full-create input hashes must be stable
within each arm.

The runner's schedule assertion executes before lock acquisition, preparation,
or a benchmark child. `--dry-run` performs schedule, dependency, custody, and
command-plan validation only; it creates no result namespace and produces no
row.

The terminal analyzer requires the exact started/completed chronology for all
nine rows in both `ROW-STARTS` and the row-only subset of
`ACTUAL-INVOCATIONS`. Every row occurs once, in schedule order, and every
completed child exits zero. Any journal, WAL, or SHM path is residue even when
its observed length is zero.

## Exact nine-row schedule

1. warmup full-create `A`, then `B`;
2. measured full-create pair 0 `A`, then `B`;
3. measured full-create pair 1 `B`, then `A`;
4. candidate-only `same-middle`;
5. candidate-only `one-byte-middle`;
6. candidate-only `plus1-middle`.

The exact arm/order vector is `AB, AB, BA, B, B, B`, producing 9 JSON rows:
2 uncounted warmup rows, 4 measured full-create rows, and 3 candidate-only
guards. Guards make no control-speed claim.

## Hard semantic and direct-counter gates

Every row must report PASS with the exact operand, profile, source, operation,
CDC count, canonical closure identity, and expected result already checked by
the executable. Each row must have one writer transaction, one COMMIT dispatch,
one successful return, DELETE journal mode, `synchronous=FULL`, matched durable
and COMMIT timer equations, no journal/WAL/SHM residue, and terminal `Q=0`.
Full-create Q high-water is at most 131,072 bytes; edit Q high-water is at most
4,194,304 bytes.

The `phase_counters` entry whose phase is `sqlite_commit` is the controlling
publication boundary. On every qualified B row it must report exactly:

- `identity_bytes_hashed = 0`;
- `canonical_bytes_authenticated = 0`;
- `canonical_authenticated_nonnew_bytes = 0`;
- `canonical_authentication_hashes = 0`;
- `objects_authenticated = 0`;
- `statement_cache_acquisitions = 0`;
- `borrowed_row_blob_reads = 0` and `borrowed_row_blob_bytes = 0`;
- `sql_query_calls = 1`, `sql_execute_calls = 2`, and `commits = 1`.

For candidate full-create, whole-row `sql_query_calls = 4` and
`row_blob_reads = 4`. This is the exact qualified-path value and rules out the
second approximately 105-MiB authentication. The three B guard rows must also
show no full-graph work in `sqlite_commit`. `one-byte-middle` must be classified
same-count and its `precommit_closure` phase must show
`incremental_qualification_calls = 1`, matching changed-spine qualification;
it may not authenticate the complete source graph there.

`commit_dispatch_to_return_wall_ns` is the actual SQLite COMMIT dispatch/return
timer. `commit_pre_and_post_dispatch_wall_ns` is reported separately and may
not be labeled COMMIT. The analyzer reports both without subtracting historical
runs.

## Screen decision

Both adjacent measured full-create comparisons must independently satisfy
`candidate durable_capture_total_wall_ns < control durable_capture_total_wall_ns`.
Their position-balanced center is descriptive. Guard timings are descriptive;
semantic, authority, transaction, Q, custody, and direct-counter failures are
hard `REVISE` conditions. This compact screen intentionally defines no broad
promotion percentage and no claim about cold caches or physical I/O.

PASS means only that the publication repair is eligible for the next complete
canonical-v2 validation decision. REVISE leaves CP-0009 accepted and stops.
