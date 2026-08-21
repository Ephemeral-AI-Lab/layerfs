# CP-0007 — count-changing transaction-local construction proof

Status: `RETAIN / PASS`
Date: 2026-08-21
Experiment mode: `acceptance`
Primary operation: `100-MiB +1 early / +1 middle durable publication`
Observed campaign wall: `49 seconds`
Configured campaign / command ceilings: `120 / 60 seconds`
Transient databases and fixtures deleted: `yes`

## Identity

| Field | Value |
|---|---|
| Repository / branch | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` / `codex/empty-worktree` |
| Parent checkpoint | `CP-0006` |
| Starting HEAD | `febc20f046bba84ccdce1256363d77799eabf2db` |
| Candidate | dirty working tree |
| Compiled-source diff SHA-256 | `88ffb0bd6a30ee9a6926ccec4916ed917278ee0e80f6ababeaff18001395a3e9` |
| Benchmark source SHA-256 | `c074d7ce5abdcf85e9309c9d02377fb22ddb3707afdc0c17e404c0ddd40e4bdb` |
| Release executable SHA-256 | `145ca598308bb7ca367ee52eb65eb7e9b54151a8086a75c8575baca47728dfd4` |
| Runner SHA-256 | `ff54754e64ae128f8a402f5d3364d9be17d882a8d58fc80f145c9c6016e7dbe8` |
| Raw JSONL SHA-256 | `dca3af156410fa7b999600991ee0f93871c6462942a0507a0cefc1fb2ecde083` |
| Python analysis SHA-256 | `8457ae6fb300102cc88c1c56f1b9bb515479b31a56ee62eb68f2e34f34c89b24` |
| Ruby analysis SHA-256 | `07bffd1a5bcc5848d97ffd195bdc3fe1802b3c443d7718ea0e57097c6e8256e7` |
| Edit round-trip evidence SHA-256 | `20754b6a22dfa2304ea598396ea17865019e64881ddbf26acc73bc0acc363de0` |
| Toolchain / host | `rustc 1.96.0`; macOS 26.4.1 arm64 |

The dirty identity is the complete `(HEAD, compiled-source diff, release
executable)` tuple. Performance is not attributed to HEAD alone.

## One changed variable

Before, count-changing publication reconstructed the suffix and then replayed
the complete resulting file closure before COMMIT. The replay authenticated
about 105–106 MiB and 5,478–5,560 objects.

The candidate carries the already-required fully authenticated prior-head
permit into the fixed-radix suffix builder. Every prior reference occurrence
is covered once; the inserted chunk and every rewritten leaf, branch, file
root, namespace, delta page, and transition advance the existing
transaction/open/authority/mutation-serial evidence chain. One private,
move-only proof is consumed before publication. Successful COMMIT or exact
requested-visible reconciliation installs a nonserializable same-open
authority for the new head; rollback, mismatch, different/prior/unresolved
reconciliation, reuse, or reopen invalidates it.

Unchanged: CDC/chunk identities, canonical object bytes and IDs, K64/F64 +
DIR256K profile and production profile ID, selected goldens, SQLite schema,
`FULL` + rollback-journal durability, one transaction, and one COMMIT.

## Algorithm and memory

For old reference count `N`, insertion ordinal `p`, suffix `S=N-p`, height
`H`, leaf ceiling `K=64`, and fanout `F=64`:

```text
mutation + proof fold:
  O(S + rewritten leaves/branches + H)

pre-COMMIT proof consumption after prior authority exists:
  O(1) complete-head/receipt comparison

resident LayerFS memory:
  O(K + F*H + bounded mapping/SQL/proof buffers)

first authority after reopen:
  Theta(complete reachable authenticated closure), required and separately timed
```

The change removes duplicate qualification work; it does not change the
fixed-radix suffix rewrite. Count-changing edits therefore remain `O(S)` and
worst-case `Theta(N)`. No source-sized/all-reference structure, serialized
metadata, cache, or second transaction was added. Candidate logical Q is
55,375 bytes versus 50,631 bytes in CP-0006: +4,744 bytes, bounded and terminal
zero.

## Performance

One warmup and three measured samples were run per arm from byte-identical
database/authority/expectation masters. All three candidate samples beat every
retained CP-0006 sample for both affected operations.

| Operation | CP-0006 median | CP-0007 median | Delta | Speedup |
|---|---:|---:|---:|---:|
| 100-MiB `+1` early | 432.939417 ms | **7.868417 ms** | **-98.182559%** | **55.022x** |
| 100-MiB `+1` middle | 432.324667 ms | **6.946583 ms** | **-98.393202%** | **62.236x** |
| 100-MiB same-count middle | 8.639167 ms | 8.503250 ms | -1.573265% | 1.016x |
| 100-MiB full write | 603.327666 ms | 578.403166 ms | -4.131171% | 1.043x |

| Affected phase | `+1` early | `+1` middle |
|---|---:|---:|
| mapping/proof fold median | 3.231750 ms | 2.611791 ms |
| pre-COMMIT proof median | 0.024500 ms | 0.043209 ms |
| COMMIT median | 4.569042 ms | 4.131833 ms |
| prior-authority median, outside durable timer | 239.660791 ms | 245.128417 ms |
| pre-COMMIT control | 426.203333 ms | 427.111875 ms |
| pre-COMMIT reduction | 99.994252% | 99.989884% |

The prior-authority timer is not hidden. A first edit after reopen still pays a
mandatory independent scrub, so first-edit wall is currently about 247–252 ms
on this fixture. The construction proof carries authority only within the same
open Store; the direct carry test shows zero canonical object authentication
when the next transaction issues the carried witness. Persisted receipt bytes
alone never authorize this skip.

## Exact work and resources

| Counter | `+1` early | `+1` middle |
|---|---:|---:|
| prior reference occurrences covered | 5,284 | 5,284 |
| suffix references / raw bytes | 5,284 / 104,857,600 | 2,642 / 52,377,184 |
| rewritten leaves / branches | 83 / 2 | 42 / 2 |
| construction put evidences | 90 | 49 |
| mapping-phase authenticated objects / bytes | 179 / 730,964 | 97 / 371,804 |
| qualification authenticated objects / bytes | **0 / 0** | **0 / 0** |
| proof consumptions | 1 | 1 |
| source bytes read | 1 | 1 |
| Q high-water / terminal | 55,375 / 0 | 55,375 / 0 |
| median user / system CPU | 0.64 / 0.08 s | 0.65 / 0.09 s |
| median RSS | 12,599,296 | 12,648,448 |
| newly written canonical bytes | 365,509 | 185,929 |
| allocated-store delta | 16,777,216 | 16,777,216 |
| transactions / COMMITs | 1 / 1 | 1 / 1 |

Exact LayerFS-owned Q increases by only 4,744 bytes. Median RSS is slightly
lower than CP-0006 (12,648,448→12,599,296 early and
12,713,984→12,648,448 middle). The external macOS peak-footprint observation
is noisier and increases from 4,833,688→9,077,144 bytes early and
4,768,128→6,488,448 bytes middle; the absolute candidate peaks remain below
9.1 MiB and do not correspond to an unbounded LayerFS owner. User CPU falls
from 0.83 s to 0.64/0.65 s. Allocated-store delta remains exactly 16,777,216
bytes.

The original preregistration's “no complete payload replay” and <=2-MiB
authentication gates apply to the changed mapping/qualification interval. The
separately timed first same-open authority intentionally remains a complete
scrub, as required by the authority contract. This phase scope is reported
explicitly rather than treating the mandatory scrub as optimized away.

## Correctness and fresh verification

- benchmark binary tests: 55/55 PASS;
- focused independent old full-verifier versus proof comparison: early and
  middle roots, transitions, closure, counts, totals, suffixes, Q, and
  single-use behavior PASS;
- carry-forward use, rollback invalidation, and reopen nonauthority PASS;
- warnings-denied clippy, rustfmt, and diff check PASS;
- Python and independent Ruby analyses: PASS, no reasons;
- 27/27 campaign rows: one transaction/COMMIT, stable promoted identities,
  exact timer/W/D/Q equations, terminal Q zero;
- two fresh 100-MiB edit round trips: reopen, full scrub, reconstruction, and
  range verification PASS with exact frozen roots/transitions/closure.

The round-trip results were 747.477083 ms early and 716.715458 ms middle. They
are correctness/lifecycle checks, not edit-publication medians.

## Decision

Decision: **RETAIN K64/F64; do not reopen WP4-P; WP5 remains eligible.**

Both 100-MiB count-changing medians pass the <=50-ms required gate, <=25-ms
strong gate, and <=15-ms stretch gate. Existing project authority accepts
honest suffix-linear count-changing cost; it does not require scale-independent
8–10-ms insertions at multi-GiB/100-GiB scale. The analytical 100-GiB middle
case remains 2,705,409 rebuilt references and 186,891,342 canonical mapping
bytes, with no latency projection.

If product policy later requires near-constant 8–10-ms count-changing edits at
those scales, stop before WP5 and specify a new canonical history-independent
prolly tree. Do not claim this fixed-radix result is scale-independent.

## Compact evidence

- [raw campaign](cp-0007-dirty-88ffb0bd6a30-count-change-proof.jsonl)
- [Python analysis](cp-0007-dirty-88ffb0bd6a30-count-change-proof-python-analysis.json)
- [Ruby analysis](cp-0007-dirty-88ffb0bd6a30-count-change-proof-ruby-analysis.json)
- [fresh edit round trips](cp-0007-dirty-88ffb0bd6a30-count-change-proof-roundtrip.jsonl)
- [preregistered experiment](../wp4p/post-promotion-count-change-proof.md)

No SQLite image, source fixture, authority file, expectations file, or release
executable is retained in the repository.
