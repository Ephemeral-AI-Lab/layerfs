# WP4-M F2 — bounded transaction-local full-create construction proof

## Prospective preregistration — frozen before F2 source edits

- Date: 2026-08-19.
- Scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` only, branch
  `codex/empty-worktree`.
- Starting HEAD/tree: `ab8bed309a55ebe143054b1b4d50562311d1a5ae` /
  `c6367eb86de5433c366c5bc33161dc52845a3421`.
- Starting tracked/untracked state: clean.
- Classification: pass elimination. F2 changes only the private K64/F64
  full-create pre-COMMIT qualification algorithm. Overall full create remains
  `Theta(source bytes + references)`.
- F2 does not change CDC, CAS identity, canonical bytes, file topology,
  workspace root, transition, schema, write shape, durability, transaction or
  COMMIT count, M4.5 C0/C1, another operation, profile selection, production
  integration, metadata, or a dependency.
- Required terminal decision: exactly one of `PASS / retain`, `FAIL / REVISE`,
  or `FAIL / revert`. Stop before F3 and do not commit.

### Frozen authority and custody

| Item | Frozen value |
|---|---|
| F1-v3 control executable | `target/wp4m-f1-commit-io-k64-20260819-v3/binaries/phase4_create_edit_benchmark-f1-candidate` |
| F1-v3 control executable SHA-256 | `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805` |
| Starting benchmark source SHA-256 | `9a4fff15668726e2dc2fdd84258e368dbcc992ba4bf39658f3f97cc996655a64` |
| Retained 100-MiB source | `target/wp4m-f1-commit-io-k64-20260819-v3/S1-100.source` |
| Source bytes / SHA-256 | `104,857,600` / `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| Fixture manifest SHA-256 | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| Source BLAKE3 | `bb883eecf4ea85d80432953791dcc352243da94175e7503e2c476afe9bd0bab7` |
| CDC references / sequence BLAKE3 | `5,284` / `5bb376c3c54d8724973a7b160acab599f2f5cee4b4a56e855ff0cbe987425994` |
| Root / transition | `2d41c27f96b0332475fb8ec3c46a336c9c8a8084408bc545e5cbb24d51cb25d0` / `ba15fd20469414de99c135fc90a5c5ad028f99f115b8c0d138ace9ec98536412` |
| Ordered closure | `d6aac6e40cc851dd6295dbeec6488f1c5ebefa7520f86b0cd12bdcdce1f0d54a` |
| F1-v3 raw / primary / independent SHA-256 | `dfa78b82fd2cdd27b76ce2708a3411579a09a4b1bd11bbf3e39030e7fc1afd44` / `f36cc5c565e61ff48513588127a3c04c4472ea3592de59086adf1e0e20ecffee` / `6e9ba24e4b12260b20b3a8d2893d2ddd009649985a08f971b212fe829222e301` |
| F1-v3 artifact manifest / final audit SHA-256 | `23507ef09ed90d47fcd53cff2e788bd6d725c8c5f8feb865184c21e67808cd95` / `58f9f455b9c004a7555153adc4e051a59c75769e50be32b3351c0ba91c805ed9` |

Controlling source-document hashes read before this preregistration:

| Document | SHA-256 |
|---|---|
| `FULL_CREATE_OPTIMIZATION_NOTE_TO_READ_AFTER_M4_5_PASS.md` | `66a93c625da688fd5cf1bc9cb2dd7f0ab2754c5eba19e1d8ff4e2be55a3b79f4` |
| `note_to_read_after_m4.5.md` | `242dd268ad98dc5b81d58b2cef94d7788debca3df85cf4450e986da992f9fad6` |
| `wp04-opt-milestone-f1.md` | `9b4bc7df9b171cbc4822aa5efd165ae320b8d081d17ea11951b1ea51c8e6af9e` |
| `PHASE_4_WP4M_M4_5_OPTIMIZATION_SPEC.md` | `739620380446c8fc2fee5f7edc96c867bc32ed83bb6b54dcc98ecd76d5eab4c8` |
| `wp04-opt-milestone-4-5-independent-audit.md` | `2ea65fb6bd53d3100ead393da252deed04cf1aacf6519aff4344c0943a4384b5` |
| `RETAINED_100_MIB_FULL_CREATE_LIFECYCLE.md` | `aa648488e59901054cc6c0d582f0979109c471c53584d12fece0c3e1f47135d3` |
| `PHASE_4_ALGORITHM_COMPLEXITY_ANALYSIS.md` | `40c81eeeba9e766c9170c94020144173df4558f26e7f19424e078a7b30f19e86` |
| `PHASE_4_LOGICAL_PERSISTENCE_MAPPING.md` | `3e94b054e6bf0eb198f6b04287d8a6cb209fb2925450b6c6bc6a69c84ab63e06` |
| `PHASE_4_SQLITE_VISIBLE_HEAD_MIGRATION_SPEC.md` | `cfddcc291cfff40ffcfd19e8e93ba2a4e51b3b16c412d137ece5463acc7625df` |
| `wp04-optimization-progress.md` | `038353934c3f187cfb8d97d30f11288405a0c433b93949d9acf6891ac0bcd878` |

### One bottleneck and one variable

The sealed F1-v3 K64/F64 full-create candidate medians are:

| Durable phase | Median |
|---|---:|
| canonical CAS mapping/object persistence | `412,469,958 ns` |
| complete pre-COMMIT replay | `391,422,750 ns` |
| SQLite COMMIT durability | `124,762,583 ns` |
| durable capture | `932,013,041 ns` |

Every F1-v3 candidate pre-COMMIT row repeats exactly:

```text
statement acquisitions / SQL queries / returned rows / row-BLOB reads = 5,373
objects authenticated / canonical authentication hashes               = 5,373
canonical bytes authenticated/hashed                                  = 105,291,608
raw bytes rehashed                                                     = 104,857,600
raw hashes                                                             = 5,284
```

The only F2 variable is replacing that full SQLite replay for a genesis
full-create with consumption of a complete transaction-local construction
proof. The hypothesis is that the proof reduces pre-COMMIT SQL, row-BLOB, and
object-authentication counts by at least 95%, preserves every identity/write/
durability result, and improves durable wall by at least 5% in at least four
of five adjacent pairs.

### Before and after algorithm

Control C0:

```text
one source/CDC/CAS/builder pass
-> transition construction
-> SQLite replay of transition plus complete requested file closure
-> one COMMIT
```

Candidate C1:

```text
one source/CDC/CAS/builder pass
  -> private per-put evidence after canonical insert or full incumbent auth
  -> chunk evidence folded into leaf completeness
  -> leaf evidence folded into branch frontier
  -> file -> singleton workspace -> genesis transition completeness
  -> source and ordered CDC hashers updated during that same source pass
-> consume one complete transaction/open/store/authority/epoch/profile proof
-> one COMMIT
```

The proof is private, nonserializable, non-`Clone`, non-`Copy`, and usable at
most once. It carries no all-reference/object/event list, map, cache, visited
set, spool, table, sidecar, or public abstraction. It is valid only for the
exact live `Store`, open identity, store/authority IDs, integrity epoch,
profile, writer transaction, authority serial, mutation serial, source
fingerprint, ordered CDC sequence/count, root, transition, and prepared
complete-construction expectation. Mismatch, later store mutation, rollback,
COMMIT, reopen, replay, or second use rejects it before publication.

### Flat closure-digest rule

The ordered closure digest is a flat root-first BLAKE3 transcript, while
construction is bottom-up. F2 will not treat subtree digests as composable and
will not retain linear closure events. Before timing, a separate full-verifier
oracle prepares the exact `(root, transition, ordered closure)` tuple outside
the row and stores it in the existing prepared-expectation grammar; the empty
measured database remains empty. Both arms receive byte-identical expectation
bytes. C1 may return the bound expected closure only after all construction,
source, CDC, identity, authority, and topology checks pass. Fresh post-COMMIT
ordered-closure verification recomputes the digest independently and must
match it exactly.

Shadow proof remains authoritative until its direct tests show exact equality
with C0 on root, transition, source fingerprint, CDC sequence/count, total raw
length, topology, closure expectation, typed failures, counters, and Q. The
measured C1 omission is not enabled before that shadow gate passes.

### Exact type and topology memory contract

The target is `aarch64-apple-darwin`. Existing measured type sizes are:

```text
size_of::<ObjectId>()                  = 32
size_of::<ObjectKind>()                = 1
size_of::<blake3::Hasher>()            = 1,920
size_of::<file_codec::FileReference>() = 68
size_of::<file_codec::FileChild>()     = 40
size_of::<Vec<T>>()                    = 24
```

F2 preregisters these private repeated sizes and compile-time/direct-test
guards:

```text
size_of::<PutEvidence>()               = 80 bytes, including its Q guard
size_of::<ConstructionNodeProof>()     = 64 bytes
construction fixed state charge        = 4,096 bytes
size_of::<construction fixed state>() <= 4,096 bytes
```

Let `L = H + 1` be the number of live builder frontier levels including leaf
children. The exact proof-owned live charge is:

```text
Q_proof(K,F,H)
  = 4,096                                      fixed state, including 2 hashers
  + K * 68                                     existing leaf-reference capacity
  + L * (24 + F*40)                            existing level Vec buffers
  + L * 8                                      existing level-total buffer
  + L * (24 + F*64)                            proof level Vec buffers
  + 80                                         one live per-put evidence
```

For the retained topology `K=64`, `F=64`, `N=5,284`, `P=83`, `H=1`, `L=2`:

```text
Q_proof = 4,096 + 4,352 + 2*(24+2,560) + 16 + 2*(24+4,096) + 80
        = 21,952 bytes
```

The retained fresh-store upper-bound equation is:

```text
prepared expectation live capacity                 14,486
maximum retained canonical object                  32,781
maximum simultaneous F2 proof-owned charge         21,952
                                                     ------
retained candidate analytical Q ceiling            69,219 bytes
preregistered hard retained-row Q cap               73,728 bytes
terminal Q                                          exactly 0
```

The global existing admission limit remains 1,073,741,824 bytes. All checked
products/sums, exact-capacity allocations, early errors, rollback, proof drop,
post-COMMIT drop, and report delivery must clean Q to zero. The fixed-state
size and every repeated type size are direct test gates; a mismatch is
`FAIL / REVISE`, not an amended equation after measurement.

### Direct counter equations

For the retained fresh full create:

```text
chunks                                      5,284
leaves                                         83
branches                                        2
file root + workspace root + transition         3
construction put evidences                   5,372

strong edges proven
  = transition->workspace                       1
  + workspace->file                             1
  + file-root children                          2
  + branch children                            83
  + leaf references                         5,284
  =                                           5,371
```

Control pre-COMMIT equations remain the sealed values above. Candidate
pre-COMMIT targets are:

```text
construction-proof consumptions                  = 1
pre-COMMIT current-head SQL queries              <= 1
pre-COMMIT SQL returned rows / row-BLOB reads     = 0 / 0 for genesis
pre-COMMIT objects/canonical bytes authenticated  = 0 / 0
pre-COMMIT canonical/raw authentication hashes    = 0 / 0
```

Thus preregistered minimum reductions are 99.981% for SQL queries and 100%
for row-BLOB reads and object authentication. Mapping adds exactly one source
fingerprint hash over 104,857,600 bytes and one ordered CDC accumulator over
5,284 `(raw_length, raw ChunkId)` entries during the existing source pass; it
does not reread the source. Created/reused objects, canonical bytes, canonical
ID hashes, mapping rewrites, SQL writes, BLOB writes, storage, transactions,
and COMMITs must remain exact.

### Shadow/adversarial test matrix

Before C1 may omit replay, direct tests must cover:

1. empty, one-reference, exact K, K+1, exact K*F, K*F+1, final partial leaf,
   unary-collapse boundary, and H=2 topology;
2. new insert, duplicate occurrence, incumbent reuse, and unequal incumbent;
3. missing object, malformed canonical bytes, wrong role, wrong leaf/branch/
   root summary, wrong height/fullness/cumulative end, and truncated input;
4. wrong singleton namespace edge, wrong transition kind/parent/child/
   operation, wrong root, wrong transition, and wrong closure expectation;
5. wrong open/store/authority/epoch/profile/transaction/mutation serial;
6. mutation after proof, rollback, COMMIT, reopen, replay into another Store,
   second consume, and proof issuance/consumption overflow;
7. source fingerprint, CDC sequence/count, raw length, root, transition, and
   complete-expectation mismatch;
8. exact type sizes, topology charge at every boundary, 1-GiB admission,
   checked overflow, allocation refusal, every error cleanup, and terminal
   `Q=0`; and
9. C0/C1 shadow equality on every identity/result/counter common to both,
   with exact typed failure agreement for adversarial rows.

The M4.5 protected regressions remain the sealed v4 same-open authority,
changed-spine C0/C1, deep H=2, exact Q 2,222,803, one-COMMIT, typed durability,
and no-full-replay tests. F2 must not alter those code paths or results.

### Validation gates before release

Run, in order:

```text
cargo test --offline -p layerfs-engine --bin phase4_create_edit_benchmark <focused F2 filters>
cargo test --workspace --offline --all-targets
cargo clippy --workspace --offline --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check HEAD
git status --short --branch
```

Also retain direct read-only schema/storage inspection, one-transaction/
one-COMMIT evidence, exact phase/counter equations, terminal Q, and the
smallest release M4.5 regression: one uncounted C0/C1 warmup pair plus one
adjacent measured pair from byte-identical v4 base images. No release candidate
or performance timing is valid before shadow, focused/full/static, and custody
gates pass.

### Frozen release campaign

Freeze exactly:

- A: the sealed F1-v3 control executable named above;
- B: one release executable built once from the final validated F2 source;
- the exact retained source/manifest and one prepared full-construction oracle;
  and
- one versioned F2 artifact root containing binaries, source/diff, commands,
  environment, prepared pair bases, raw rows, external observations, primary
  summary, independently implemented recomputation, schema/storage audit,
  complete hashes, and final read-only audit.

Before any pair preparation or executable invocation, the runner must assert
this exact schedule:

```text
pair0 warmup   AB
pair1 measured AB
pair2 measured BA
pair3 measured AB
pair4 measured BA
pair5 measured AB
```

Each pair is prepared once. The empty SQLite database, 32-byte authority
sidecar, and expectation file are copied byte-for-byte to A and B; all three
hashes and apparent/allocated starting endpoints must match before either arm
runs. Preparation, oracle construction, copying, hashing, and preflight remain
outside timers. Each child runs under `/usr/bin/time -l`. Started rows are
never deleted, replaced, or selectively rerun.

### Acceptance gates and decision

F2 is `PASS / retain` only if all of the following pass:

1. exact source/CDC/root/transition/ordered-closure/reconstruction/range
   identities and complete post-COMMIT results in every row;
2. exact created/reused/authenticated-write, canonical/mapping bytes,
   workload SQL writes/changed rows, BLOB writes, schema, logical/apparent
   storage, one transaction, one COMMIT dispatch/return, FULL+DELETE,
   publication and reconciliation results;
3. at least 95% pre-COMMIT SQL-query, row-BLOB-read, and object-authentication
   reduction, with the direct equations above;
4. candidate durable-capture arm-median improvement at least 5%, paired-median
   improvement at least 5%, and at least four of five paired wins;
5. mapping/COMMIT/post-COMMIT work is explained by exact counters; COMMIT,
   post-COMMIT phases, CPU, RSS, peak footprint, and allocated-store paired
   medians do not regress by more than 5%, with at least four of five pairs
   within that ceiling for each protected metric;
6. exact `Q_proof=21,952`, retained `q_high_water<=73,728`, and terminal
   `q_current=0` on success and every injected failure;
7. no source-sized/all-reference/event state, serialized metadata, endpoint,
   schema/write-shape, durability, M4.5, or other-operation change; and
8. primary and independent analyses agree on every row, median, paired delta,
   win count, counter equation, storage result, gate, and disposition; the
   versioned manifest and final read-only audit verify.

The `550–570 ms` range is planning context only. It is not an acceptance
threshold or a value that may override measured gates. Any authority,
identity, closure, typed-failure, one-COMMIT, exact-Q, custody, or independent-
verification defect is `FAIL / REVISE`. If bounded authority cannot be proven,
the candidate must not replace replay. If correctness passes but the material
wall gate fails without a protected regression, classify the mechanism
honestly and do not start F3. F3 eligibility is stated only after the final F2
decision; F3 itself is not started in this task.

## Prospective shadow-found topology correction — before C1 enablement/timing

The first direct K/F boundary shadow test found that the existing streaming
`FileBuilder` temporarily creates one extra full frontier level whenever the
canonical root's child count is exactly `F`; `finish` then authenticates and
collapses that unary top branch. The preregistered retained K64/F64 row has two
root children, not 64, so its `L=2`, `Q_proof=21,952`, and `73,728` hard cap are
unchanged. For the general topology equation only, define:

```text
P = ceil(N/K)
H = canonical file-root branch level
R = ceil(...ceil(P/F).../F) after exactly H divisions
L = H + 1 + usize(R == F)
```

Use this corrected `L` in the exact equation above. The additional level is
bounded `O(F)` state, preserves `O(K + F*H)`, and is required to prove the
existing builder/write shape rather than changing it. The corrected boundary
tests cover exact K, K+1, K*F, K*F+1, unary collapse, and H=2 before C1 is
enabled. No acceptance threshold, retained-row equation, identity, or measured
result is amended.

## Terminal implementation and evidence — FAIL / REVISE

### Decision

**F2 disposition: FAIL / REVISE.** Preserve the uncommitted candidate and all
versioned evidence, but do not make it the accepted rolling control. The
construction-proof mechanism, correctness, bounded authority, direct counters,
durable-wall gate, CPU, Q, RSS, peak footprint, storage/schema, and protected
M4.5 regression pass. The prospectively protected COMMIT phase does not:
candidate COMMIT median regressed `+30.125789%`, paired median regressed
`+28.184244%`, and candidate won `0/5` pairs. No threshold is relaxed after
observation.

F3 is **not eligible**. No batching, profile selection/promotion, production
integration, metadata, or Phase 4 completion work was started. No commit was
created.

### Minimal implementation

The measured source change is confined to
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`. It adds:

1. an 80-byte private move-only per-put evidence value issued only after a new
   canonical row is inserted or an incumbent is fully authenticated and byte-
   compared;
2. a 64-byte private node proof folded through the existing `FileBuilder`
   leaf/branch frontier;
3. one nonserializable full-create proof bound to open/store/authority/epoch/
   profile/transaction/authority/mutation serials, the one-pass source and CDC
   fingerprints/count, root, transition, and the preprepared complete oracle;
4. single-use consumption with mutation/rollback/COMMIT/reopen/replay/mismatch
   invalidation; and
5. private full-create counters and one out-of-timer full-verifier oracle using
   the existing prepared-expectation `base` tuple.

The flat root-first ordered-closure transcript is not composed from bottom-up
subtree summaries. The oracle binds the scalar before the measured operation;
fresh post-COMMIT reconstruction recomputes the exact transcript and compares
it. There is no reference/object/event list, visited set, map/cache, spool,
table, sidecar, schema change, public framework, or dependency.

Measured custody:

| Item | SHA-256 |
|---|---|
| source | `e9aaba6ef76a3f47dbaf55a492f66679cd114a24c3f525184abbca5f9cd9983c` |
| source-only implementation diff | `ff3e24628cb80160cc6e5289d1b684049bde36ae043f5eedf1075fb8f6fa2d5b` |
| F1-v3 control executable | `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805` |
| one-time F2 release executable | `25c3197f8b18914d7622da6cfe06b75c50e97d08abc62cbbce999aac4bd7e720` |
| frozen preregistration copy | `a7a746082f0254b53bd892051379899caddb03b208b09114acd043c32a42f9d0` |
| fixture / fixture manifest | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` / `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |

### Correctness and static gates

All gates passed before the release build:

```text
cargo test --workspace --offline --all-targets
  107 passed; 0 failed
    44 layerfs-core
     4 layerfs-engine library
    42 private benchmark
    12 phase4_engine_parity
     5 layerfs-eval

cargo clippy --workspace --offline --all-targets -- -D warnings
  PASS

cargo fmt --all -- --check
git diff --check HEAD
  PASS / PASS

debug self-test
  PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
  objects=20; auth_bytes=1,054,925
```

The focused F2 tests pass exact type/Q sizes, empty/one/K/K+1/K*F/K*F+1/H=2
topologies, transient unary collapse, duplicate occurrences and incumbent
reuse, unequal/malformed/missing/wrong-role/wrong-summary paths, namespace/
transition/source/CDC/root/closure mismatch, authority mutation, rollback,
COMMIT, reopen, replay, second use, checked overflow/allocation cleanup, shadow
C0 equality, one-COMMIT publication, and fresh independent closure
recomputation. Every injected success/error path ends at `Q=0`.

### Exact proof/Q equations

The direct target-layout tests freeze:

```text
ObjectId=32, ObjectKind=1, Hasher=1,920,
FileReference=68, FileChild=40, Vec=24,
PutEvidence=80, ConstructionNodeProof=64 bytes.
```

The retained proof-owned equation is exactly the preregistered `21,952` bytes.
All six candidate rows report total `q_high_water=55,325`, below the frozen
`73,728` cap, and terminal `q_current=0`. The F1-v3 control reports `37,302`.
The higher candidate Q is the explicitly charged two-hasher construction
state and K/F frontier, not source-sized state.

### Campaign custody

Artifact root:

```text
target/wp4m-f2-construction-proof-k64-20260819-v1/
```

The runner asserted the exact complete sequence before preparation:

```text
pair0 warmup   AB
pair1 measured AB
pair2 measured BA
pair3 measured AB
pair4 measured BA
pair5 measured AB
```

There are exactly 12 rows, every pair is adjacent, and all 12 database,
authority, and expectation arm copies hash byte-identically to their once-
prepared pair base. No row was deleted, replaced, or rerun. The candidate
preparation retained six separate full-verifier oracle databases outside every
arm timer.

Principal hashes:

| Evidence | SHA-256 |
|---|---|
| schedule dry-run | `1c76b72fbc17336b57222e37d5ec83b75e90b6d039f204bab48561e0e0e797fb` |
| raw JSONL | `800aa1e8252fe1a39687713b4caaee85f4c746fc6467d6525bc563c9e932fb3f` |
| preflight | `2656e33643afae7527322607df34e3a1fb6f82d38edf112684c3de0e35f1be7e` |
| commands | `33621f17a5f77fcc6716c181538b97432e9cb9997efc053487db3c9e18b4ae48` |
| external observations | `523f3f485ccb541627978297bada1b453aadbfb5f89844497e783ac7e410c586` |
| primary summary | `e3bb3f542096f1c460551b427254488c9b77ee996a3ec54a6e68e767abfc0fb3` |
| independent recomputation | `cf793d192febe82631c9aa9809bfe33760dbbf6ae5ee953cb34e9674e11d951f` |
| storage/schema audit | `123c864c1b11f392a6ebaa74c1012135e3873f6f9d83b0987676a9eaf9eb8bd5` |

### Identity, work, storage, and durability

Every A/B row agrees exactly on:

- source BLAKE3, 5,284-reference CDC count/sequence, root, transition, and
  ordered closure;
- reconstructed bytes, reference count, and every range result;
- 5,372 created objects, zero reused objects, 105,291,554 canonical new bytes,
  365,262 rewritten mapping bytes, 5,284 chunks/references, 83 leaves, and two
  branches;
- mapping SQL executes/changed rows and BLOB writes;
- one writer transaction, one COMMIT dispatch/return, committed publication,
  exact timer equations, and fresh independent verification; and
- DELETE journal, `synchronous=FULL`, `temp_store=FILE`, `mmap_size=0`,
  109,268,992 logical/apparent DB bytes, 32 authority bytes, zero residual
  journal/WAL/SHM, and one identical schema hash across all 12 arm databases.

Allocated APFS bytes differ favorably: allocated-store-delta median is
`118,042,624 -> 109,248,512` bytes (`-7.449946%`). This is endpoint allocation,
not physical-I/O-byte evidence. Native prepares, VFS read/write calls/bytes,
sync call/wall, true journal/temp peaks, and byte-level physical I/O remain
Unavailable under the sealed F1 reasons.

### Direct counter result

Every candidate mapping phase reports:

```text
put evidences / completed strong edges       5,372 / 5,371
leaf / branch / file summaries                  83 / 2 / 1
workspace / transition summaries                 1 / 1
source fingerprint bytes / hashes       104,857,600 / 1
CDC entries                                    5,284
```

Every candidate pre-COMMIT phase reports:

```text
proof consumptions                               1
SQL queries / returned rows                      1 / 0
row-BLOB reads                                   0
objects / canonical bytes authenticated          0 / 0
raw/canonical authentication hashes              0 / 0
```

Against sealed F1-v3 pre-COMMIT:

| Counter | F1 control | F2 candidate | Reduction |
|---|---:|---:|---:|
| SQL queries | 5,373 | 1 | `99.981388%` |
| returned rows | 5,373 | 0 | `100%` |
| row-BLOB reads | 5,373 | 0 | `100%` |
| object authentications | 5,373 | 0 | `100%` |
| canonical authenticated/hash bytes | 105,291,608 | 0 | `100%` |
| raw bytes rehashed | 104,857,600 | 0 | `100%` |

Across the complete lifecycle, statement acquisitions fall
`16,236 -> 10,863`, SQL queries `10,953 -> 5,581`, row-BLOB reads
`16,160 -> 10,787`, object authentications `21,520 -> 16,147`, and canonical
authentication bytes `421,341,408 -> 316,049,800`. Post-COMMIT phase
identities/results remain unchanged.

### Wall and resource result

Independently selected measured medians:

| Phase/resource | F1-v3 control | F2 candidate | Change/result |
|---|---:|---:|---:|
| mapping/CAS + proof construction | `403.402 ms` | `606.564 ms` | `+50.362%`; required one-pass source fingerprint and bounded proof work |
| pre-COMMIT qualification | `386.637 ms` | `0.068 ms` | `-99.982%`, 5/5 |
| COMMIT | `135.886 ms` | `176.823 ms` | **`+30.126%`, 0/5; protected FAIL** |
| durable capture | `929.420 ms` | `786.868 ms` | `-15.338%`, paired median `-15.629%`, 5/5; speed PASS |
| durable throughput | `107.594 MiB/s` | `127.086 MiB/s` | direction favorable |
| fresh reopen | `1.023 ms` | `1.014 ms` | protected |
| fresh scrub | `265.791 ms` | `267.582 ms` | protected |
| reconstruction | `419.188 ms` | `421.555 ms` | protected |
| ranges | `0.671 ms` | `0.690 ms` | protected |
| complete lifecycle | `1,615.793 ms` | `1,476.144 ms` | `-8.643%` |
| complete-lifecycle throughput | `61.889 MiB/s` | `67.744 MiB/s` | direction favorable |
| total CPU | `1.630 s` | `1.490 s` | `-8.589%` |
| RSS | `93,503,488` | `93,208,576` bytes | `-0.315%` |
| peak footprint | `92,307,912` | `91,980,208` bytes | `-0.355%` |

All five durable paired improvements are `13.176–16.621%`. All five candidate
COMMIT rows are slower by `20.003–52.078%`. F1 pager observations remain
otherwise nearly exact: both arms write 26,676 dirty main-DB pages and have
the same 87,049,984-byte pre-dispatch cache snapshot; candidate has 6,675
spills versus control 6,676. The database is 27,435,008 apparent bytes and the
journal is 17,928 apparent / 20,480 allocated bytes at dispatch in both arms.
Those observations do not explain the protected COMMIT wall regression as
physical I/O because VFS/sync/physical-byte evidence remains unavailable.

### Permanent M4.5 regression

The frozen F2 release executable ran one uncounted AB warmup and one measured
adjacent BA C0/C1 pair from byte-identical v4 database/authority/expectation
bases. The measured result is C0 `432.398417 ms` versus C1 `9.132459 ms`.
It reproduces exact XOR identities, eleven objects, 110,745 canonical new
bytes, 7,382 mapping bytes, C0 `16,334/16,418` versus C1 `10,976/11,060`
acquisition/query counts, C1 `123/8` equal/different edges, one transaction/
COMMIT, Q `2,222,803`, and terminal zero. M4.5 remains PASS and protected.

Evidence SHA-256: raw
`47a27573ed9af20db6312eb1cccb5f4b91d2f1367c1d95316d3e7c35f18dc61a`,
preflight
`d131335a8964ae19747749ae03dfe01366f42d8d82fd39296576720417b467e1`,
commands
`b6e1ec83daefaa3e10836c700b8720abb2c6433d5d02ab75786b24f086d5d753`,
and external observations
`556b21477efaa1e94b0b15120b1011b017a4c64f594a3d950db090162b45f7b1`.

### Honest complexity and non-claims

Proof construction adds one source-fingerprint pass over the already streamed
source bytes and `O(K + F*H)` bounded frontier work. Single-use pre-COMMIT
consumption is fixed-size plus one empty-head query. Full create remains
`Theta(source bytes + references)` time, live proof state remains
`O(K + F*H)`, and durable space remains unchanged. Fresh scrub and
reconstruction remain independent linear phases.

The candidate does not reach 200 MiB/s, does not make the `550–570 ms`
planning range evidence, does not establish sync/physical-I/O causality, and
does not authorize F3. The next F2 revision, if separately authorized, must
explain the measured mapping/source-hash cost and COMMIT regression without
weakening durability or bundling F3 insertion batching. This task stops here.
