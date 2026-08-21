# Canonical-v2 publication repair — deterministic closure screen v2

Date: 2026-08-21. This prospective amendment controls only the fresh namespace
`target/phase4-canonical-v2-publication-repair-20260821-v2/results-v1`. The
sealed v1 root remains byte-for-byte historical `REVISE`; v2 may compose its
evidence but may not relabel, rewrite, copy over, resume, or selectively rerun
it.

## Why v2 exists

The sealed v1 campaign already established two adjacent 100-MiB full-create
wins, 26.721% in AB order and 27.622% in BA order, and its only reported
blocker was `guard-one-byte-middle-B:changed-spine-work`. Read-only inspection
found that the v1 analyzer compared the one-byte row with a different row's
tuple. The v1 one-byte phase itself reported the compact changed-spine tuple
below. No semantic gate is loosened: v2 prospectively corrects only the
mis-specified one-byte direct-counter assertion, with all other gates
unchanged. It changes no source, executable, algorithm, or workload; it
reverifies sealed v1 and acquires exactly one fresh deterministic
candidate-only one-byte-middle row.

V2 PASS means only `screen closed; eligible for complete canonical-v2
validation`. It is not promotion, integration, a timing claim, a commit, or
authorization for another optimization.

## Frozen custody

- sealed candidate executable SHA-256:
  `75ce43857799f3de035b989fa0dcba49e6eec4b4279b9256cfbd214cbc1aa187`;
- benchmark-main candidate source SHA-256:
  `a22db63db4179606ad0f5dce3a7cbb25d68e4a843f40f98207f9407f21e46f87`;
- CP-0009 control executable, reference only, SHA-256:
  `9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7`;
- 104,857,600-byte fixture SHA-256:
  `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`;
- sealed v1 terminal manifest SHA-256:
  `91b009a262ec30dc9503fcaa909f9f54103bc5004a47f98efa95606a39a93aef`,
  exactly 126 entries and zero mismatches; its verification SHA-256 is
  `f38dced6d98ffd30336e6b40694b1744bb90889bec657df6983ed134a5f5f1df`,
  and the sealed root is exactly those 126 entries plus the manifest and
  verification files, with no extras;
- sealed v1 36-row source/build custody SHA-256:
  `e78e83ff45add569ae6cf4674f796ac3a857c501cba37d035ab6c14b101630a0`;
- sealed v1 raw and input-custody SHA-256:
  `777ec722f95578c1717e86cd5100c01c497a876d0ffea557bcf2864f285eb532`
  and `d1b7f50897c59996672f761579f0904bb5453d469e09e8f977d72400f153635a`;
- sealed one-byte-middle master database, authority, and expectations SHA-256:
  `962b491e70551db76d3712d966c25259a96b23df453a4342b92c97adcc06a996`,
  `abac9762e55b20e4a7db6b42bfaa435fb9af8e3a0a79d061f4dd05ee63ef6f12`,
  and `a9bf6f2ae2592c755e584672bc55b371468beb00721c69fd06403d2b5d6d2b7d`.

Before acquisition, every anchor, all 36 sealed source/build inputs, and every
sealed-v1 manifest entry are rehash-verified. The row uses an independent
byte-identical copy, with distinct path and inode, of all three master files;
no claim about cloned versus allocated extents is made. The source fixture is
an independent byte-identical copy whose hash/size and source/target
path-device-inode custody are recorded. The executable's legacy
`physical-byte-copy-identical-database-authority-expectations` label means only
the observed byte-identical, distinct-inode filesystem copies here; it is not a
physical-I/O or allocation observation. The CP-0009 binary is never run.
There is no build, test, preparation child, warmup, full-create rerun, or
historical timing subtraction.

## Supervisor and exact schedule

One monotonic 59-second ceiling begins before lock/preflight and ends only
after analysis, lock release, terminal manifest creation, and read-only
manifest verification. Every child is capped at 15 seconds and the smaller
remaining budget. A pre-existing lock, result namespace, timeout, or child
failure is fail-closed `REVISE`; there is no resume or rerun.

The schedule is exactly one row:

1. `fresh-one-byte-middle-B`, candidate-only, not warmup, one invocation.

`ROW-STARTS-v1.tsv`, the row subset of `ACTUAL-INVOCATIONS-v1.tsv`, and
`RAW-v1.jsonl` must respectively contain exactly two ordered events, exactly
one started/completed child pair, and exactly one JSON row. `--dry-run`
performs only schedule, dependency, and custody reads: it creates no result
namespace, copy, lock, child, or row.

## Exact fresh-row gates

The row must be `PASS` for candidate profile
`94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`,
operation `one-byte-middle`, source fingerprint
`bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7`,
CDC sequence fingerprint
`4060424f80635c79ea7fba81c8daf7777e9261a3abf4df24104368de5e6b9745`,
ordered closure
`b71da56600ce3c2011cdca037771c9050fbf5f16df2a2297b19e4af11173878e`,
root `ae63b984c0ea1fd0ba7f8fe39c6acaa434f839ff3da2acf63cb2c91880d4a5e0`,
and transition
`db53b6664ddbc43c29e43c7fdb106f168dc203266b39383e188a9719fa7da24b`.
It must report 5,284 references before, after, expected, and actual;
same-count classification; offset 52,480,416; and replacement `f1 -> ab`.

The one `precommit_closure` phase must satisfy this exact tuple and equations:

- qualification calls 1;
- SQL queries/rows 22/22 and row-BLOB reads 25;
- borrowed row-BLOB reads/bytes 2/36,940;
- authenticated objects, statement-cache acquisitions, and authentication
  hashes 21/21/21;
- canonical and identity bytes authenticated/hashed 48,164/48,164;
- prior spine objects/bytes 4/5,104 and replacement spine objects/bytes
  4/5,104;
- new subtree objects/bytes 2/36,940;
- covered edges 126, new-or-different edges 5, and `126 + 5 = 131`.

The one `sqlite_commit` phase must have zero identity hashing, canonical
authentication, authenticated objects, statement-cache acquisition, borrowed
row-BLOB reads/bytes, and incremental qualification work. Its exact SQL tuple
`(query calls, execute calls, rows returned, row-BLOB reads, row-BLOB writes,
commits)` is `(1, 2, 1, 4, 4, 1)`.

The whole row must have one writer transaction, one COMMIT dispatch, one
successful return, DELETE journal mode, `synchronous=FULL`, `temp_store=FILE`,
and `mmap_size=0`. Durable, lifecycle, and COMMIT equations must match. Actual
COMMIT dispatch/return is positive and pre/post publication time is
nonnegative, but no elapsed-time performance conclusion is permitted.

Logical Q is exact: scan input 1,066,637 + old window 1,066,637 + base live
1,257 + old-chunk slots 12,672 = high-water 2,147,203; terminal Q is zero and
the fixed envelope remains removed. Any path ending `-journal`, `-wal`, or
`-shm` anywhere under v2 is residue even when zero length and forces REVISE.

## Composition and decision

The v2 analyzer independently rehashes sealed v1, reconstructs both v1
full-create comparisons directly from sealed raw rows, confirms the retained
v1 `REVISE` disposition and sole historical reason, and then evaluates the
one fresh row. A complete terminal manifest and verification are mandatory.

PASS requires every custody, chronology, identity, closure, count, Q, timer,
transaction, durability, direct-counter, residue, composition, and manifest
gate above. Otherwise v2 is `REVISE`, v1 remains historical `REVISE`, CP-0009
remains accepted, and the work stops.
