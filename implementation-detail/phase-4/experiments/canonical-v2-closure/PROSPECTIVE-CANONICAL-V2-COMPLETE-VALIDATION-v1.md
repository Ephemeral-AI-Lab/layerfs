# Canonical-v2 complete validation v1 — prospective contract

Date: 2026-08-21. This contract was frozen before any complete-validation
timing row. It composes, but never relabels, the historical compact-closure
REVISE bundles and the publication-repair v3 PASS.

## Frozen operands and scope

- CP-0009 control executable:
  `9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7`.
- Final Canonical-v2 source:
  `16e9beedd2fe49d6da65f89f53f488cffbfdcfc71f10477e854cd2d37d00e120`.
- Final Canonical-v2 release executable, built once before timing:
  `f3dd4c9420cc7bb7e7390960db9bf6e4a4a44de3d15dc0573002d3172b570280`.
- Native codec source:
  `8fe11085d8b27b1f2a833665b4afd11f6370f3e94821f5022d67ae14cac071dc`.
- Workspace evaluator source:
  `19b87be2add8dc80f48ecdcbebdaee22f1e703600ff65d4262008fa67e651855`.

No format, identity, CDC, COW, SQLite, transaction, durability, publication,
or performance implementation may change after these hashes are frozen. The
only campaign code is a thin wrapper around the existing compact runner and
analyzer. No dependency, rebuild, migration, production integration, later
optimization, commit, or selective rerun is allowed.

## Static correctness gate

Before timing, one fresh static bundle must record successful completion of:

1. `cargo test --workspace --offline --all-targets`;
2. `cargo clippy --workspace --offline --all-targets -- -D warnings`;
3. `cargo fmt --all -- --check`;
4. `git diff --check`.

Any failure is terminal REVISE. The timed campaign rehashes the sealed static
manifest supplied as an external custody anchor.

## Timed campaign

One fresh namespace and one fail-fast host lock are allowed. A single monotonic
clock covers custody, preparation, all rows, analysis, disposition, terminal
manifest creation, and manifest verification. The hard ceiling is 119 seconds;
each child is limited to the smaller of 59 seconds or remaining time. Started
rows are never deleted, resumed, or rerun.

The exact inherited 29-row schedule is:

1. 100-MiB full-create warmup `AB`;
2. 1-MiB full-create `AB`;
3. 10-MiB full-create `BA`;
4. 100-MiB full-create pair 0 `AB`;
5. 100-MiB full-create pair 1 `BA`;
6. comparable 100-MiB guards: same-middle `AB`, +1 early `BA`, +1 middle
   `AB`, warm materialization `BA`, fresh-process materialization `AB`, reopen
   `BA`, and authenticated returned 1-MiB range `AB`;
7. candidate-only 100-MiB guards: one-byte early/middle/late, first edit after
   reopen, and scrub-only.

All row databases, authority files, and expectations are independent byte
copies. Each runtime authority target is a regular distinct file, hash-equal
to its source and mode `0600` before the row. The terminal evidence seal may
then make artifact copies read-only.

## Hard decisions

Every row must preserve exact executable/profile/source/operation identities,
expected and actual CDC counts, selected roots/transitions/closure digests,
one writer transaction and one publication COMMIT for mutations, zero writer
transactions for reads, synchronous caller-thread execution, `FULL + DELETE`,
timer equations, terminal Q zero, bounded Q, and no journal/WAL/SHM path.

For every candidate mutation, the `sqlite_commit` phase must contain exactly
one current-head query, two execute calls, one changed row, four head BLOB
writes, one COMMIT, and zero object authentication, canonical authentication,
construction-proof consumption, or incremental qualification. Full create
must retain 5,284 references, 5,372 created objects, zero reuse, 105,122,466
canonical bytes, 196,174 mapping bytes, 5,381 SQL calls, and 10,748 BLOB writes.

Both adjacent 100-MiB full-create comparisons must favor Canonical-v2. Their
position-balanced center is descriptive and is not compared with historical
wall time. A comparable lifecycle row is a material regression only when the
candidate exceeds control by both 20 ms and 50%. Candidate-only rows make no
speed claim. The 1/10-MiB rows are scale and identity checks, not statistical
claims. Cache state is warm-or-unknown; unavailable instructions, cycles, and
physical I/O remain unavailable.

PASS makes the exact fresh-store Canonical-v2 source/binary/profile the frozen
Phase-4 baseline. Known nonempty v1-to-v2 automatic migration remains
unsupported and must return `SchemaMigrationRequired` before mutation. Any
failure retains CP-0009 and Canonical-v2 is not frozen. In either disposition,
stop before the next optimization lane and do not commit.
