# H05 canonical-witness substitution preregistration

Status: **PROSPECTIVE / FROZEN BEFORE SOURCE EDIT OR CANDIDATE BUILD**  
Date: 2026-08-21  
Purpose: benchmark-private `<=120`-second mechanism screen  
Authority: CP-0009 current product baseline; WP4-P complete and closed

This document authorizes implementation and validation of one private H05
candidate. It does not authorize measured rows until an explicit benchmark
lock is acquired, and it does not authorize compact v2, production
integration, a profile change, or a Phase-4 completion claim.

## 1. Exact control

The immutable control package is
`target/phase4-h05-canonical-witness-screen-20260821-v1/control/`.

```text
HEAD                 febc20f046bba84ccdce1256363d77799eabf2db
source diff          b073a7e04c7a7a2b17671f80c42aee598cc5d8039e4ba83d63b7cac89d150f84
source               3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a
control executable   9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7
raw                  988f6960d2fa12a0d0fff1e0db5de655f05fb3b08d6682a451846f0bfa6d5224
analysis             616bbb186a9cb9ce4121b91bc96f8cff14407907b506e762995a21f63cbb323c
profile              K64/F64 + DIR256K
```

The control executable is copied before any build and must rehash exactly
before and after the screen. The candidate is built once after all validation.

## 2. One variable

Change only the first private full-create construction-proof commitment:

```text
control:
  BLAKE3(complete source bytes)
  + existing ordered repeated(u32be(raw_length) || raw_id)

candidate:
  BLAKE3 derive-key context
    "layerfs/phase4/h05/canonical-occurrence/v1"
    over repeated(u32be(raw_length) || canonical_object_id)
  + existing ordered repeated(u32be(raw_length) || raw_id)
```

The canonical tuple is updated only after the same occurrence's canonical
`ObjectId`, kind, canonical length, transaction/open/authority scope, and next
mutation serial pass `PutEvidence`. The same `FileReference` is then folded
through the current leaf, branch, file, workspace, and transition proof.

The existing raw-ID sequence remains because durable v1 references still
contain a separate `raw_id`. The external source fingerprint remains in the
prepared fixture/expectation and result row for custody and post-COMMIT byte
verification; it is removed only from the private pre-COMMIT construction
proof comparison.

## 3. Private evidence format

Candidate prepared expectations add one fixed 32-byte canonical-occurrence
commitment and receive a new benchmark-private version marker. It is computed
outside measured rows from the exact fixture and independently compared during
proof consumption. It is not SQLite metadata, a mapping field, a receipt, a
profile ID, or a public API.

No durable v1 object, mapping byte, root, transition, delta, receipt, schema,
authority sidecar, or profile value may change.

## 4. Predicted direct counters

For the exact 100-MiB/5,284-reference row:

```text
control construction_source_hash_bytes       104,857,600
control construction_source_hashes                      1
candidate construction_source_hash_bytes               0
candidate construction_source_hashes                    0

candidate canonical_commitment_entries              5,284
candidate canonical_commitment_input_bytes         190,224
  = 5,284 * (4-byte length + 32-byte ObjectId)
candidate canonical_commitment_hashes                     1

net witness hash-input reduction
  = 104,857,600 - 190,224
  = 104,667,376 bytes
```

The following remain exact and equal:

```text
raw_bytes_hashed             104,857,600
raw_hashes                         5,284
canonical_id_bytes_hashed    105,291,554
canonical_id_hashes                5,372
construction_cdc_entries           5,284
canonical_new_write_bytes    105,291,554
mapping_bytes                    365,262
transactions / COMMITs              1 / 1
```

Hash-input reduction is not speed evidence. The causal result is the complete
adjacent paired durable wall:

```text
paired_effect_i = candidate_durable_wall_i - control_durable_wall_i
```

Phase-local CPU remains unavailable. Whole-child user/system CPU is protected
but cannot prove which sublane caused the wall change.

## 5. Frozen durability and resource contract

- synchronous caller thread;
- one writer transaction and one publication COMMIT;
- rollback journal `DELETE`, `synchronous=FULL`, `temp_store=FILE`,
  `mmap_size=0`;
- atomic visible-head publication and fresh ambiguous-outcome reconciliation;
- exact errors, failure precedence, rollback provenance, and committed-result
  wrapping;
- exact current v1 roots, transitions, closure, reconstruction, and ranges;
- exact logical/apparent/allocated main/journal/sidecar observations and no
  final journal/WAL/SHM residue;
- exact charged-capacity equation and terminal `Q=0` on every exit.

`ConstructionState` replaces one BLAKE3 `Hasher` with one BLAKE3 `Hasher`, so
its fixed-size Q charge must not increase. The fixed expected 32-byte digest
must not introduce an owned heap capacity. Any additional report string or
dynamic expectation capacity must be charged exactly and declared before the
screen.

## 6. Required focused validation

Before release build:

1. exact canonical-commitment framing/domain and independent recomputation;
2. fragmentation invariance and empty/repeated occurrence behavior;
3. omitted, duplicated, reordered, wrong-length, wrong-raw-ID, and
   wrong-canonical-ID failures;
4. wrong kind/canonical length, forged or unequal incumbent, stale evidence,
   mutation-serial, open/transaction/store/authority/profile/epoch failures;
5. second proof use, rollback, requested/prior/different/ambiguous COMMIT;
6. exact current v1 roots, transitions, closure, work, storage, and errors;
7. direct-counter equations, overflow, cleanup, Q cap, and terminal zero;
8. current full-create fresh reopen, scrub, reconstruction, and ranges;
9. same-count and count-changing guards remain unaffected.

Run focused tests, `cargo test --workspace --offline --all-targets`, Clippy with
`-D warnings`, formatting check, and whitespace/diff checks before the single
candidate release build.

## 7. Screen schedule and ceiling

No screen row may start while another local research task is issuing shell or
filesystem work. The coordinator first acquires `BENCHMARK_LOCK=H05_SCREEN`
and pauses the three read-only research sessions.

```text
warmup pair:      AB        uncounted
measured pair 1:  AB
measured pair 2:  BA
measured pair 3:  AB
```

Each arm receives a byte-identical isolated database/authority/expectation
start appropriate to its evidence version. Preserve every started row. The
screen command has a hard 120-second ceiling. No selective rerun, deletion,
outlier replacement, or post-observation amendment is allowed.

One non-controlling candidate correctness smoke protects same-count edit,
`+1` early/middle, warm/fresh logical materialization, returned 1-MiB range,
reopen/head, scrub, reconstruction, and ranges. These are guards, not bundled
optimization claims.

## 8. Decision rule

Immediate `REVERT / FAIL` on any identity, authority, error, timer equation,
Q, durability, storage, residue, transaction, one-COMMIT, or cleanup mismatch.

`RETAIN-FOR-FULL-CAMPAIGN` requires all of:

- direct counters match section 4 in every candidate row;
- independent canonical commitment matches;
- all three measured pairs favor the candidate;
- paired median durable improvement is at least 5%;
- no work moved after COMMIT;
- protected correctness/resource smoke passes.

A retained screen authorizes only the full prospectively frozen five-or-more-
pair CP-0009 campaign. It is not PASS, production authority, portability, or a
new accepted baseline. A sub-5%, mixed-direction, or semantically failing
screen reverts the candidate and records only this H05 mechanism as a local
NO-GO.

## 9. Explicit exclusions

No compact v2 references, mapping/profile change, K113, prolly tree, reopen
permit, CDC change, SQLite page-size/cache change, compression, carrier,
worker, async, pool, VFS, retry, schema change, public API, production
integration, WP4-P reopen, or Phase-4 completion claim.

Implementation and validation may now proceed. Measurement remains locked.
