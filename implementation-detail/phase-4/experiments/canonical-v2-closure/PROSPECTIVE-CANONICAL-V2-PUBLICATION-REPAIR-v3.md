# Canonical-v2 publication repair — authority-mode repair v3

Date: 2026-08-21. This prospective amendment controls only the fresh namespace
`target/phase4-canonical-v2-publication-repair-20260821-v3/results-v1`. The
sealed v1 and v2 roots remain byte-for-byte historical `REVISE`; v3 may
compose their evidence but may not rewrite, relabel, resume, delete, or rerun
either campaign.

## Historical result and one change

V1 established adjacent 100-MiB full-create wins of 26.721% in AB order and
27.622% in BA order. Its sole historical blocker was a mis-specified
one-byte-middle direct-counter assertion. V2 prospectively corrected that
assertion but stopped before producing a JSON row:

- terminal disposition: `CANONICAL-V2 PUBLICATION-REPAIR-v2 REVISE`;
- child chronology: exactly one start and one exit-1 completion;
- raw JSONL: zero bytes and zero rows;
- stderr: exactly `Error: ValidationAuthorityUnavailable`;
- terminal manifest: 20 entries, zero mismatches, SHA-256
  `1e94b51bbc46524ad164aa3db836026d4a79e200f8bf3bd1cb7ba5c176b35131`;
- complete sealed root: those 20 entries plus manifest and verification,
  exactly 22 files.

The pre-row cause was file mode, not candidate semantics. The retained v1
authority source was sealed mode `0444`. V2 copied it with the audited runner's
`open("xb")` under the campaign umask, producing mode `0644`, while the frozen
runtime admits authority only at exact mode `0600`. V3 changes only the copied
target authority mode: after byte copy and content-hash verification, chmod the
target authority only to `0600`, verify the hash is unchanged, and record
source `0444`, copied pre-chmod `0644`, and target runtime `0600`. The retained
source is never chmodded or otherwise mutated.

No semantic gate is loosened. V3 changes no source, executable, algorithm,
codec, identity, CDC, CAS, COW, schema, write shape, transaction, durability,
counter, Q, timer, schedule, performance, or decision rule. It makes no timing
claim and does not authorize promotion, integration, a commit, or another
optimization.

## Frozen custody

V3 reuses and rehashes the same frozen inputs as v2:

- candidate executable
  `75ce43857799f3de035b989fa0dcba49e6eec4b4279b9256cfbd214cbc1aa187`;
- benchmark-main source
  `a22db63db4179606ad0f5dce3a7cbb25d68e4a843f40f98207f9407f21e46f87`;
- CP-0009 control executable, reference only,
  `9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7`;
- 104,857,600-byte fixture
  `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4`;
- v1 one-byte-middle master database/authority/expectations
  `962b491e70551db76d3712d966c25259a96b23df453a4342b92c97adcc06a996`,
  `abac9762e55b20e4a7db6b42bfaa435fb9af8e3a0a79d061f4dd05ee63ef6f12`,
  and `a9bf6f2ae2592c755e584672bc55b371468beb00721c69fd06403d2b5d6d2b7d`;
- v1 terminal manifest
  `91b009a262ec30dc9503fcaa909f9f54103bc5004a47f98efa95606a39a93aef`,
  126 entries/zero mismatches and exactly 128 root files;
- frozen v2 preregistration/runner/analyzer/methodology hashes
  `9431ca518fae015c34189a42beb8108269b6f735bde98b476db9413c040acce3`,
  `34569b9c931cd4c6365b3c047420eae82afd409e5ed8395459cb87b4ee12c3b4`,
  `085a2d1a9ac32f8621bb0e6af583e1bf45a76547d559deed0adb5d363ca00ee1`,
  and `0b5df452a36b60eec06fe7597ce07d183641241b54bd55094b134bda2f09ca05`;
- v2 terminal verification SHA-256
  `02a7d0f17c8e80658dbacbc5d52d17a97aee542dccd189a3617132dfc47ef5e0`;
- v2 analysis/disposition/stderr SHA-256
  `576d53c6ae3d1b4104fbf28763065e53f23c6ad65a916d465f04d22f4d3264bf`,
  `035b2f46db17b748cb4cb2284940b7607bbad4efec60ad5d86b53ba95d60dc02`,
  and `f665ab00c6a188b15810f6c01f152a5941021e698ed13e11cad7c62416d56679`.

Every methodology entry, every v1 manifest entry, all 36 v1 source/build
inputs, and every v2 manifest entry are rehash-verified before acquisition.
The CP-0009 control is never invoked. There is no build, test, preparation
child, warmup, full-create rerun, historical subtraction, resume, or rerun.

## Supervisor and exact schedule

The audited v2 supervisor remains unchanged: one monotonic 59-second ceiling
starts before lock/preflight and ends after analysis, lock release, terminal
manifest creation, and read-only verification. Each child is capped at 15
seconds and the smaller remaining budget. Root ownership, fail-fast locking,
no-resume behavior, bytecode-write suppression, exact command binding, final
wall evidence, and complete manifest closure remain hard gates.

The schedule is exactly one non-warmup, candidate-only, no-timing-claim row:
`fresh-one-byte-middle-B`. The exact invocation is the copied frozen candidate,
`--fast-row`, the exact v3 row root, `104857600`, `edit-one-byte-middle`,
iteration `990001`, `false`, `capture-only`. Row chronology is one ordered
start/completion pair with exit zero and exactly one JSON row. `--dry-run`
performs only read-only schedule/dependency/custody checks and creates no root,
copy, child, lock, or row.

## Authority-mode gate

`AUTHORITY-MODE-CUSTODY-v1.tsv` must contain exactly one record binding the
exact retained source and exact v3 target authority paths, distinct
device/inode tuples, content hash
`abac9762e55b20e4a7db6b42bfaa435fb9af8e3a0a79d061f4dd05ee63ef6f12`
before and after chmod, source mode `0444`, target copied mode `0644`, target
runtime mode `0600`, and `change_scope=target-only`. The analyzer independently
rehashes the unchanged target authority and observes `0600` before terminal
sealing. Any source-mode change, target mode other than `0600`, hash change, or
unbound path is `REVISE`.

The inherited terminal evidence seal intentionally changes every v3 artifact
file, including the already-consumed target authority copy, to read-only
`0444` after analysis and manifest verification. Runtime `0600` is therefore
frozen by the pre-seal mode-custody row and live analyzer result; post-seal
audit rehashes that proof chain rather than rerunning the live-mode analyzer.

## Unchanged row gates

The fresh row must reproduce the v2 preregistered identities: candidate
profile `94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b`,
source fingerprint
`bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7`,
CDC sequence
`4060424f80635c79ea7fba81c8daf7777e9261a3abf4df24104368de5e6b9745`,
closure `b71da56600ce3c2011cdca037771c9050fbf5f16df2a2297b19e4af11173878e`,
root `ae63b984c0ea1fd0ba7f8fe39c6acaa434f839ff3da2acf63cb2c91880d4a5e0`,
and transition
`db53b6664ddbc43c29e43c7fdb106f168dc203266b39383e188a9719fa7da24b`.
It must report same-count 5,284 references before/after/expected/actual, offset
52,480,416, and replacement `f1 -> ab`.

The `precommit_closure` tuple remains exact: qualification calls 1; SQL
queries/rows 22/22; row-BLOB reads 25; borrowed reads/bytes 2/36,940;
authenticated objects/cache acquisitions/hashes 21/21/21;
canonical/identity bytes 48,164/48,164; prior and replacement spines each
4 objects/5,104 bytes; new subtree 2 objects/36,940 bytes; covered/new edges
126/5 with `126 + 5 = 131`.

The `sqlite_commit` phase retains exact zero graph/authentication/incremental
work and SQL tuple `(queries, executes, rows, BLOB reads, BLOB writes, commits)
= (1, 2, 1, 4, 4, 1)`. The whole row retains one transaction, one COMMIT
dispatch and successful return, DELETE journal, `synchronous=FULL`,
`temp_store=FILE`, `mmap_size=0`, exact numeric durable/lifecycle/COMMIT
equations, no residue path, and exact Q:
`1,066,637 + 1,066,637 + 1,257 + 12,672 = 2,147,203`, terminal zero.

## Decision

The v3 analyzer independently replays sealed v1, retains v1's two full-create
wins without relabeling it, verifies sealed v2's pre-row REVISE and exact
failure, then evaluates the one fresh row and authority-mode custody. PASS
means only `screen closed; eligible for complete canonical-v2 validation`.
Any custody, mode, chronology, identity, closure, count, Q, timer, transaction,
durability, direct-counter, residue, composition, or manifest failure is
`REVISE`; v1/v2 remain historical and CP-0009 remains accepted.
