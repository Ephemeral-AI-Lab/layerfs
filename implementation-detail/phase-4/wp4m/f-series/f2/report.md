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
| `../planning/full-create-plan.md` | `66a93c625da688fd5cf1bc9cb2dd7f0ab2754c5eba19e1d8ff4e2be55a3b79f4` |
| `../planning/read-after-m4-5.md` | `242dd268ad98dc5b81d58b2cef94d7788debca3df85cf4450e986da992f9fad6` |
| `../f1.md` | `9b4bc7df9b171cbc4822aa5efd165ae320b8d081d17ea11951b1ea51c8e6af9e` |
| `../../milestones/m4-5/spec.md` | `739620380446c8fc2fee5f7edc96c867bc32ed83bb6b54dcc98ecd76d5eab4c8` |
| `../../milestones/m4-5/independent-audit.md` | `2ea65fb6bd53d3100ead393da252deed04cf1aacf6519aff4344c0943a4384b5` |
| `../planning/retained-100-mib-lifecycle.md` | `aa648488e59901054cc6c0d582f0979109c471c53584d12fece0c3e1f47135d3` |
| `../../../algorithm/complexity-analysis.md` | `40c81eeeba9e766c9170c94020144173df4558f26e7f19424e078a7b30f19e86` |
| `../../../mapping/logical-persistence.md` | `3e94b054e6bf0eb198f6b04287d8a6cb209fb2925450b6c6bc6a69c84ab63e06` |
| `../../../storage/sqlite/visible-head.md` | `cfddcc291cfff40ffcfd19e8e93ba2a4e51b3b16c412d137ece5463acc7625df` |
| `../../progress.md` | `038353934c3f187cfb8d97d30f11288405a0c433b93949d9acf6891ac0bcd878` |

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

> Historical F2-v1 close, preserved for custody. The later versioned audit
> addendum withdraws v1's standalone-authority and exact-Q claims and corrects
> fresh-reopen and range protected gates to 3/5 FAIL. Nothing in this section
> is F2-v2 acceptance evidence.

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

## F2-v2 prospective repair — continuation from f2-ckp1

### Frozen checkpoint and historical evidence

- Continuation date: 2026-08-19.
- Starting branch / HEAD / tree: `codex/empty-worktree` /
  `4d20b7c5ca61fb2a5f61a198eac10a11bc631cd8` /
  `9355b1afc5eb082d7df2c5fbb6a94f40b3bf2e2a`.
- Starting tracked/untracked state: clean.
- Historical v1 root:
  `target/wp4m-f2-construction-proof-k64-20260819-v1`, 171 files; sorted
  complete file-hash stream SHA-256
  `1e232ac6f9aa7185904f7c4c2832a88c0b78699a2a5df11b650f93d490ea6de1`.
- V1 raw / candidate binary / source SHA-256:
  `800aa1e8252fe1a39687713b4caaee85f4c746fc6467d6525bc563c9e932fb3f` /
  `25c3197f8b18914d7622da6cfe06b75c50e97d08abc62cbbce999aac4bd7e720` /
  `e9aaba6ef76a3f47dbaf55a492f66679cd114a24c3f525184abbca5f9cd9983c`.
- Sealed F1-v3 control binary remains
  `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805`.
- V1 and its binaries/raw/manifest/original analyzers remain immutable
  historical **FAIL / REVISE** evidence.

The additive correction is
`v1-audit-addendum.md`. Two independently implemented
recomputations under
`target/wp4m-f2-v1-audit-addendum-20260819-v1` agree semantically. They
correct the omitted individual post-COMMIT `>=4/5` rules: fresh reopen is only
3/5 within +5% (pair 2 `+10.407895%`, pair 5 `+9.018463%`) and ranges are only
3/5 (pair 2 `+9.080780%`, pair 4 `+7.128157%`). Scrub and reconstruction pass
5/5. V1 environment/toolchain/build/test-output custody is Unavailable. V1
therefore failed protected COMMIT, reopen pair-count, and range pair-count.

### One repaired variable and standalone authority

V2 still changes only private K64/F64 genesis full-create qualification. It
does not change CDC, canonical CAS identity, IDs, topology, root/transition
semantics, schema, write shape, DELETE/FULL durability, transaction/COMMIT
count, post-COMMIT verification, M4.5 C0/C1, another operation, metadata,
backend, dependency, profile selection, or production integration.

The runtime proof no longer contains, binds, or consumes an externally built
root, transition, or closure. The transaction-local chain is:

```text
BEGIN under transaction_attempt
  -> exact generated/new or fully authenticated incumbent PutEvidence
  -> chunk occurrence + raw length/ChunkId + one-pass source/CDC accumulators
  -> leaf and branch edge summaries in the existing FileBuilder frontier
  -> exact file root
  -> internally construct and fold the singleton `file` namespace edge
  -> internally construct and fold the Genesis(None, child, zero ops) edge
  -> issue once and consume once against live open/store/authority/epoch/
     profile/transaction/authority-serial/mutation-serial and empty head
  -> compare derived source/CDC/count/total/root/transition with the request
  -> publish once
fresh reopen -> root-first transition/file verification -> closure report
```

An optional externally prepared `(root, transition, closure)` is comparison-
only golden data after COMMIT. Its absence is valid; arbitrary full create
succeeds without it. Its corruption causes a committed-post-verification
failure and never supplies publication authority. The flat root-first closure
digest is not composed from bottom-up summaries and is unavailable during
pre-COMMIT proof consumption.

Every failure after successful BEGIN in the F2 path is owned by the existing
`transaction_attempt`: the proof drops, rollback is attempted, authority is
invalidated, first/cleanup provenance is retained, no active transaction
remains, and Q returns to zero. There is no second proof framework.

### Exact topology/Q contract

The v1 repeated type sizes remain frozen:

```text
PutEvidence=80, ConstructionNodeProof=64, FileReference=68,
FileChild=40, Vec=24, Hasher=1,920 bytes;
ConstructionState fixed charge=4,096 bytes.
```

Let `L = H + 1 + usize(R == F)` with `R` defined by the v1 topology
correction. The exact simultaneously owned construction charge remains:

```text
Q_proof(K,F,H)
  = 4,096
  + K*68
  + L*(24 + F*40)
  + L*8
  + L*(24 + F*64)
  + 80
```

For retained `K=F=64`, `N=5,284`, `H=1`, `L=2`, this is exactly `21,952`
bytes and the preregistered total retained-row cap remains `73,728`. V2 moves
the frontier charge to the `FileBuilder` that owns leaf/level/total/proof
capacities; declaration/drop order keeps it live through unary collapse and
root finalization, then all charged frontier allocations are dropped before
the charge. Root finalization uses a nonallocating scan. Fixed proof charge
plus the evidence slot (`4,176` bytes) remains until the move-only proof drops.
Admission/overflow/error/success terminal Q is exactly zero.

The source pass performs exactly the required raw ChunkId hash per emitted
chunk plus one whole-source fingerprint and one ordered CDC accumulator. V1's
second `chunk_id(bytes)` over every just-derived chunk has been removed; no
third 100-MiB hash remains.

### Direct work/counter equations

The retained construction equations remain:

```text
put evidences = 5,284 chunks + 83 leaves + 2 branches + file + workspace
              + transition = 5,372
strong edges  = 5,284 chunk occurrences + 83 branch-to-leaf + 2 root children
              + workspace-to-file + transition-to-workspace = 5,371
summary objects = 83 leaves + 2 branches + file + workspace + transition
authenticated occurrence/edge work = Theta(N)
```

Pre-COMMIT candidate targets remain one empty-head query, zero returned rows,
zero row-BLOB reads, zero object authentication, and one proof consumption.
Created/reused objects, canonical bytes, mapping bytes, workload SQL writes,
BLOB writes, storage, transaction count, and COMMIT count remain control-equal.

### Frozen adversarial and validation gates

Before the v2 release build, direct tests must pass all topology boundaries,
duplicate occurrence/incumbent reuse, missing/malformed/unequal/wrong-role
incumbents, wrong summaries, open/store/authority/epoch/profile/transaction/
mutation mismatches, cross-Store replay, rollback/COMMIT/reopen, second issue/
consume, counter and allocation overflow, wrong namespace, wrong transition
parent/child/kind/operation, optional/corrupt golden, and exact Q cleanup.
Unary collapse must reject equal-total wrong-child, wrong-order, and corrupt-
branch injections. The full verifier and construction result must agree on
source, CDC, count, total, root, transition and all independently available
post-COMMIT closure/results.

Release is blocked until focused tests, all workspace/all-target tests,
Clippy `-D warnings`, rustfmt check, diff check, read-only schema/storage,
one-COMMIT evidence, exact counter/Q equations, debug self-test, and the
smallest release M4.5 C0/C1 regression all pass with retained outputs.

### Frozen v2 release campaign and acceptance

The new artifact root is
`target/wp4m-f2-construction-proof-k64-20260819-v2`. Freeze exactly the sealed
F1-v3 control and one once-built final v2 release binary. Use the exact retained
104,857,600-byte fixture and manifest. Before preparation or execution assert:

```text
pair0 warmup AB
pair1 measured AB
pair2 measured BA
pair3 measured AB
pair4 measured BA
pair5 measured AB
```

Each pair is prepared once and both arms receive byte-identical database,
authority, and expectation copies. Retain raw JSONL, preflight, commands,
separate user/system/total CPU, Q, RSS, footprint, pager/storage/COMMIT
observations, environment/toolchain/build/test outputs, two independent
analyzers, schema/storage audit, complete manifest, and final read-only audit.

V2 is **PASS / retain; F3 eligible** only if all original identity/work/
storage/durability gates pass; pre-COMMIT SQL/BLOB/auth reductions are at least
95%; durable arm and paired medians improve at least 5% with at least 4/5
wins; and COMMIT, each individual post-COMMIT phase, CPU, RSS, peak footprint,
Q, and allocated storage are each no worse than +5% by the specified arm/
paired/at-least-4/5 rules. No v1 threshold is loosened.

If COMMIT still fails after all repairs, preregister and run only a diagnostic
immediate-publish versus fixed approximately 200-ms idle experiment. The idle
is separately timed, included in durable total, uses balanced paths, and is
never acceptance evidence. Diagnose it before changing another variable.

Current v2 status at this preregistration point: implementation and debug
validation in progress; no release binary or performance row exists; F2 is
not yet accepted and F3 remains ineligible.

## F2-v2 diagnostic-only COMMIT preregistration

This section was frozen after the single acceptance campaign completed and
before any diagnostic binary or row existed. The acceptance rows already make
F2-v2 **FAIL / REVISE**: candidate COMMIT is `129.875125 -> 164.051542 ms`
(`+26.314829%` arm, `+25.652590%` paired, `0/5` within +5%). Fresh reopen also
fails its arm-median ceiling (`+6.592828%`), and ranges pass only 3/5 pairs.
No diagnostic can cure, replace, or rerun those rows.

The sole diagnostic variable is a fixed `200 ms` caller-thread idle inserted
after successful full-create proof consumption/request comparison and before
the existing COMMIT timer begins. The idle wall is measured separately and
printed to diagnostic stderr; because durable capture begins before mapping,
the idle remains included in durable total. Both arms use one separately built
diagnostic binary: I has no idle and D enables exactly 200 ms. Schema, writes,
transaction, COMMIT, FULL+DELETE, source, proof, and all post-COMMIT work are
otherwise identical.

Diagnostic schedule, asserted before preparation/execution:

```text
pair0 warmup   ID
pair1 measured ID
pair2 measured DI
pair3 measured ID
pair4 measured DI
pair5 measured ID
```

Each pair uses a once-prepared empty database/authority/expectation base and
byte-identical arm copies. Retain raw rows, stderr idle observations, commands,
preflight hashes, binary/source hashes, and environment reference. Report
COMMIT and durable pair directions only; there is no threshold, candidate
selection, acceptance, or causal claim about physical writeback because VFS,
sync, and byte-level physical I/O remain Unavailable. After this diagnostic,
do not change another runtime variable in F2-v2.

## F2-v2 terminal result — FAIL / REVISE

### Decision

**F2-v2 is FAIL / REVISE; F3 is not eligible.** Retain the repaired mechanism
and complete v2 evidence for review, but do not make it an accepted rolling
control. Correctness, standalone authority, exact absolute Q, pre-COMMIT
counter reduction, identity/work/storage/durability, durable-wall improvement,
CPU/RSS/footprint/allocation, one-COMMIT, and M4.5 regression gates pass.
Prospectively protected COMMIT, fresh-reopen arm median, range 4/5, and the
additional v2 control-relative Q ceiling fail. No threshold is relaxed and no
acceptance row is rerun or replaced.

### Final implementation and bounds

The only measured source file is
`crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs`, SHA-256
`c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
The once-built release executable is
`68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0`;
the sealed F1 control remains `732171041e…b805`.

V2 deletes runtime `FullCreateExpectation`/closure binding. Exact namespace
and Genesis edges are constructed inside the file/workspace/transition folds;
proof consumption uses the live transaction-local scope and derived
source/CDC/count/total/root/transition. The optional prepared triple is used
only after COMMIT as a golden comparison. All F2 failures after BEGIN use
`transaction_attempt`. Incumbent reuse now authenticates stored kind,
canonical length, canonical ObjectId, and complete bytes before evidence.

The existing builder remains the sole frontier. Live state is one K reference
leaf; one F child vector, F proof-summary vector, and total per active level;
two hashers; scalar scope/counters; and bounded canonical/chunk/SQL/output
windows. There is no all-reference/object/event state, map/cache/visited set,
spool, table, sidecar, metadata, schema, public framework, or dependency.

```text
time       = Theta(source bytes + references)
memory     = O(K + F*(H+1) + bounded chunk/SQL/encoding/output buffers)
summaries  = Theta(N/K) plus geometrically smaller upper levels
edge work  = Theta(N)
live space = Theta(unique source bytes + references), unchanged
```

The exact retained proof-owned peak remains `21,952` bytes. The frontier
charge is now owned through unary/root finalization by the actual builder and
drops after all covered capacities; the remaining fixed proof charge is
`4,176` bytes until proof drop. All six candidate rows report total Q
`55,325 <= 73,728` and terminal zero. The v2-only extra control-relative Q
gate fails at `37,301 -> 55,325` (`+48.320420%`, 0/5), while the governing
absolute bounded-Q contract passes.

The source path now performs one raw ChunkId hash per chunk (`104,857,600`
bytes / `5,284` hashes), one distinct source fingerprint (`104,857,600` bytes
/ one hash), and one 5,284-entry CDC accumulator. V1's unaccounted duplicate
raw ChunkId pass is absent.

### Validation and release custody

Retained gates all passed before the one-time release build:

```text
13 focused F2 tests                         PASS
workspace all-target tests                  113 passed, 0 failed
  44 core + 4 engine + 48 benchmark + 12 parity + 5 eval
Clippy workspace/all-target -D warnings     PASS
rustfmt / git diff check / debug self-test  PASS / PASS / PASS
```

The adversarial matrix covers topology boundaries, duplicate/reuse and exact
incumbent authentication, missing/malformed/unequal/wrong-role rows, wrong
summaries, every scope binding, cross-Store replay, mutation, rollback,
COMMIT, reopen, second issue/consume, counter/allocation overflow,
namespace/transition fields and roles, optional/corrupt golden, source/CDC/
count/total/root/transition mismatch, unary equal-total wrong-child/order and
corrupt branch, success/error Q cleanup, and fresh verifier equality.

Key retained validation hashes are focused tests `413826af…6c99`, full tests
`957ddc84…039c`, Clippy `06f5ca1a…c130`, static/self-test
`d821e6f2…e9a`, environment/toolchain `f1b5021c…1099`, and release build
`ec932c1c…2fff`. The environment is macOS 26.4.1 / Darwin 25.4.0 on Apple M3
Max with 38,654,705,664 bytes RAM, Rust/Cargo 1.96.0, SQLite 3.51.0.

The smallest frozen release M4.5 regression passes: measured C0/C1 durable
edit `433.194708 -> 8.422917 ms`; exact root/transition/closure, eleven
objects, `110,745` canonical and `7,382` mapping bytes, C0
`16,334/16,418` versus C1 `10,976/11,060` acquisition/query counts, C1
`123/8` covered/different edges, Q `2,222,803`, terminal zero, and one COMMIT.
Raw SHA-256 is `054af4a4…d7a`.

### Acceptance campaign and exact results

Artifact root:
`target/wp4m-f2-construction-proof-k64-20260819-v2`. The asserted schedule is
exactly `AB/AB/BA/AB/BA/AB`; all 12 database/authority/expectation arm images
match their once-prepared pair bases. Raw/preflight/commands/resource SHA-256
are `0f3fe228…c820`, `6c66b81d…80c4`, `46465076…3bf`, and
`d429c700…ab1`. Primary Python and independent Ruby analyses agree exactly;
their result hashes are `45fca64a…642c` and `b3fe136e…fddc`.

Every A/B row agrees on source/CDC/root/transition/closure, reconstruction and
ranges, 5,372 created objects, zero reused, 105,291,554 canonical new bytes,
365,262 mapping bytes, writes/changed rows/BLOB writes, schema, logical and
apparent storage, one transaction/COMMIT dispatch/return, committed complete
head, and fresh verification. Storage audit again has one schema and no
residual journal/WAL/SHM; SHA-256 `123c864c…8bd5`.

Candidate mapping construction and pre-COMMIT are exact:

```text
put evidences / strong edges                    5,372 / 5,371
leaf / branch / file / workspace / transition  83 / 2 / 1 / 1 / 1
source fingerprint bytes/hashes                 104,857,600 / 1
CDC accumulator entries                         5,284
pre-COMMIT proof consumptions                    1
pre-COMMIT SQL queries/rows/BLOB/auth            1 / 0 / 0 / 0
```

Pre-COMMIT SQL reduction is `99.981388%`; BLOB and object authentication
reductions are `100%`.

| Metric | F1 control | F2-v2 | Arm change | Paired median | Protected result |
|---|---:|---:|---:|---:|---|
| mapping/proof | `398.408 ms` | `486.716 ms` | `+22.165%` | `+22.235%` | diagnostic; v1 candidate was `606.564 ms` |
| pre-COMMIT | `386.597 ms` | `0.052 ms` | `-99.987%` | `-99.987%` | PASS, 5/5 |
| COMMIT | `129.875 ms` | `164.052 ms` | **`+26.315%`** | **`+25.653%`** | **FAIL, 0/5** |
| durable capture | `916.758 ms` | `652.573 ms` | **`-28.817%`** | **`-28.505%`** | speed PASS, 5/5 |
| fresh reopen | `0.913 ms` | `0.973 ms` | **`+6.593%`** | `-5.833%` | **FAIL arm; 4/5 pairs** |
| fresh scrub | `265.364 ms` | `265.507 ms` | `+0.054%` | `+0.203%` | PASS, 5/5 |
| reconstruction | `423.187 ms` | `422.547 ms` | `-0.151%` | `+0.145%` | PASS, 5/5 |
| ranges | `0.670 ms` | `0.702 ms` | `+4.737%` | `+4.737%` | **FAIL, 3/5** |
| complete lifecycle | `1,608.325 ms` | `1,343.971 ms` | `-16.437%` | `-16.293%` | direction favorable |
| total CPU | `1.610 s` | `1.360 s` | `-15.528%` | `-15.951%` | PASS, 5/5 |
| RSS | `93,732,864` | `93,274,112` | `-0.489%` | `-0.472%` | PASS, 5/5 |
| peak footprint | `92,537,288` | `92,078,512` | `-0.496%` | `-0.478%` | PASS, 5/5 |
| allocated-store delta | `118,165,504` | `109,248,512` | `-7.546%` | `-7.546%` | PASS, 5/5 |

Range pair 1 is `+10.655%` and pair 5 is `+5.157%`; only 3/5 meet the ceiling.
Fresh-reopen pair 5 is `+16.697%`; 4/5 meet the pair rule, but its independently
selected arm median exceeds +5%. These small phases remain hard prospective
gates and are not averaged into the favorable lifecycle.

### Diagnostic-only idle result

The separate diagnostic binary/source hashes are `30806c9e…68e3` /
`833265c0…6d0a`. Two orchestration failures are retained: a fixture-hash
literal stopped before any row, and the first warmup immediate row stopped the
runner after stdout but before JSONL append. The resume path imported that
exact row once and did not rerun it. The final diagnostic schedule is exactly
`ID/ID/DI/ID/DI/ID`, 12 rows.

The measured delayed idle median is `206.881 ms`. The outer reported COMMIT
phase rises `168.054 -> 367.946 ms` because its caller-wrapper boundary starts
at precommit end and therefore includes the idle. The nested publish call is
`168.050 -> 160.551 ms`, and actual SQLite dispatch-to-return is
`167.886 -> 160.304 ms` (arm `-4.516%`, paired `-1.082%`, with one pair
`+6.629%`). Thus a pre-publish idle does not explain or reliably cure the
acceptance COMMIT regression. VFS/sync/physical-byte causality remains
Unavailable. Diagnostic raw/summary hashes are `5544963a…569` /
`aad75c12…158` and are never acceptance evidence.

### Stop boundary

The smallest proven remaining cause is the repeatable candidate COMMIT/
dispatch regression under otherwise exact writes, pager counters, schema, and
durability; its physical mechanism is still Unavailable. Fresh-reopen arm
median and range pair-count are additional protected failures. Retain for
revision; do not start F3, batch writes, change profile, add metadata/backend,
select/promote, integrate production, claim Phase 4 complete, or commit.

## F2-v3 same-binary diagnostic preregistration

This section is frozen before any F2-v3 source edit, diagnostic build,
artifact root, prepared base, or row.

### Custody and scope

- Date: 2026-08-19.
- Branch / checkpoint HEAD / tree: `codex/empty-worktree` /
  `4d20b7c5ca61fb2a5f61a198eac10a11bc631cd8` /
  `9355b1afc5eb082d7df2c5fbb6a94f40b3bf2e2a`.
- Starting F2-v2 source SHA-256:
  `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
- Historical F2-v1: 171 files, sorted complete file-hash-stream SHA-256
  `1e232ac6f9aa7185904f7c4c2832a88c0b78699a2a5df11b650f93d490ea6de1`.
- Historical F2-v2: 304 files, sorted complete file-hash-stream SHA-256
  `a7d590e02e4181f4cdbc2abc2fb272f823183ed6beedfa56e969ce29f434ae1d`;
  manifest/final-audit/terminal-hashes SHA-256 are `f4d5b539…16d4`,
  `ecb9ee32…377e`, and `a0dc198c…028`.
- V1/v2 roots are immutable historical **FAIL / REVISE** evidence. Nothing in
  v3 may overwrite, relabel, append to, or rerun either root.
- Diagnostic root:
  `target/wp4m-f2-construction-proof-k64-20260819-v3-diagnostic`.
- This phase is diagnostic only. It cannot accept F2, replace an old row,
  amend a threshold, select a profile, start F3/batching, or change the
  retained algorithm.

The diagnostic preserves the F2-v2 standalone proof algorithm, CDC/CAS/IDs,
FileBuilder/Q, source-hash behavior, namespace/Genesis edges, schema/write
shape, FULL+DELETE, one transaction/COMMIT, cleanup/provenance, and fresh
post-COMMIT verification. It adds no VFS, metadata, backend, dependency,
serialized state, source-sized/all-reference resident state, or public API.

### Frozen severity classification

This classification is prospective for all F2-v3 decisions:

- **P0 — none currently demonstrated.** Standalone construction authority,
  canonical identities, exact writes, transaction/COMMIT count, cleanup,
  bounded-memory class, adversarial tests, and fresh verification pass. V3
  will not redesign or revert the proof without new failing evidence.
- **P1 — acceptance blocker under investigation.** The repeatable v2 nested
  SQLite COMMIT/dispatch regression (`129.875 -> 164.052 ms`, `+26.315%`,
  `0/5`) is the only substantive unresolved runtime issue. Its physical cause
  is unknown. It is neither presumed harmless nor presumed an algorithm
  defect; the same-binary C0/C1 diagnostic and phase-boundary state must
  decide whether it is verifier-induced pager/work redistribution or extra
  work.
- **P2 — resource policy.** Relative Q `+48.3%` is only `+18,024` bytes;
  exact total is `55,325 <= 73,728`, terminal zero, the bound remains
  `O(K + F*(H+1))`, and RSS/footprint improve. V3 uses the authorized absolute
  Q contract and will add no complexity to recover a relative 5%.
- **P2 — tiny-phase measurement instability.** V2 reopen moved only about
  `+60 us` and ranges about `+32 us` on unchanged code paths. Exact outputs
  and work remain hard. Before acceptance rows, v3 must freeze either a
  justified absolute-plus-relative noise envelope from retained v1/v2
  variation or a larger-sample confirmation; it will not optimize these paths
  without failing functional evidence.
- **P2 — prior diagnostic orchestration defect.** The 200-ms idle was placed
  after `precommit_end`, so its outer COMMIT phase included the idle contrary
  to preregistration. That outer metric is invalid for causality; nested
  publish/dispatch remains usable. The idle experiment will not be rerun.

Final reporting must keep correctness/authority defects, engine/runtime
limitations, benchmark policy/noise, and diagnostic orchestration failures
separate. Historical v1/v2 dispositions remain unchanged.

### One binary, two private qualification modes

Both arms use one once-built diagnostic executable and byte-identical empty
database/authority/expectation images:

```text
C0 = BEGIN
     -> proof-enabled one-pass mapping and complete construction proof
     -> complete full SQLite verify_transition + verify_file
     -> drop unused proof
     -> one publication COMMIT

C1 = BEGIN
     -> byte-identical proof-enabled one-pass mapping
     -> consume the construction proof and omit SQLite closure replay
     -> one publication COMMIT
```

C0 and C1 therefore differ only in pre-COMMIT qualification. Old F1 mapping,
old binaries, and the idle diagnostic are not comparison arms. The optional
prepared root/transition/closure remains post-COMMIT golden data only.

### Frozen snapshot and timer evidence

For both modes, record these four ordered boundaries:

```text
S0 mapping_end
S1 qualification_end
S2 immediately_before_COMMIT_dispatch
S3 immediately_after_COMMIT_return
```

At every boundary record, without resetting counters:

- `SQLITE_DBSTATUS_CACHE_USED` current bytes;
- cumulative cache hits, misses, main-DB dirty cache writes, and spills since
  the common pre-row reset;
- SQLite status read errors; and
- database/journal/authority apparent and allocated filesystem bytes.

S0/S1 are diagnostic read-only status/filesystem observations. S2/S3 reuse the
existing exact publish snapshots. Status reads and filesystem metadata calls
are observation work and never relabeled workload SQL or physical I/O.

Retain mapping, qualification, outer COMMIT, publish-call,
dispatch-to-return, pre/post-dispatch, caller-wrapper, durable, fresh phases,
separate user/system/total CPU, RSS, footprint, Q, SQL/BLOB/authentication,
pager equations, and storage. Exact per-row equations remain:

```text
durable = mapping + qualification + outer_COMMIT
combined_tail = qualification + outer_COMMIT
publish_call = dispatch_to_return + pre_and_post_dispatch
outer_COMMIT = publish_call + caller_wrapper
```

The idle experiment is not rerun. Its outer COMMIT value violated its own
preregistered placement because the outer phase begins at precommit end and
therefore included the idle; only its nested publish/dispatch measurements are
usable historical diagnostic observations.

### Schedule and custody

Assert before preparation/execution:

```text
pair0 warmup   C0C1
pair1 measured C0C1
pair2 measured C1C0
pair3 measured C0C1
pair4 measured C1C0
pair5 measured C0C1
```

Each pair is prepared once. Database, 32-byte authority, and expectation
images are byte-copied to both modes and hash-checked before either starts.
Run each child under `/usr/bin/time -l`. Retain schedule, preflight, commands,
stdout/stderr, raw JSONL, snapshot JSONL, environment/toolchain/build/source/
binary hashes, and one analysis. No started row is replaced or selectively
rerun.

### Prospective diagnostic decision

State-coupling support requires all of the following:

1. exact source/CDC/root/transition/fresh closure, reconstructed/range output,
   canonical writes, mapping bytes, SQL executes/changed rows, BLOB writes,
   schema/storage, one transaction/COMMIT, FULL+DELETE, publication,
   reconciliation, Q, and post-COMMIT results in every C0/C1 row;
2. byte-identical proof-enabled mapping work and exact S0 status/filesystem
   snapshots within each pair, apart from timing/external process resources;
3. C0 performs the complete verifier and C1 performs one proof consumption,
   with the exact expected SQL/BLOB/authentication difference;
4. C0 nested SQLite dispatch-to-return arm median is at least 5% lower than
   C1, C0 is lower in at least four of five pairs, and paired-median C0
   advantage is at least 5%;
5. C1 `qualification + outer COMMIT` arm and paired medians improve at least
   5% versus C0 with at least four of five wins; and
6. at S1 or S2, at least one named pager/filesystem state value affected by
   qualification—cache hits/misses/used, dirty main-DB writes, spills, or
   database/journal allocation—differs C0 versus C1 in a consistent direction
   in at least four of five pairs, while exact logical writes/durability remain
   equal.

If all six pass, the evidence supports the narrow inference that active full-
verifier read/BLOB work changes SQLite pager/filesystem state and shifts work
out of standalone COMMIT. It still does not identify physical write or fsync
bytes/calls and does not by itself accept F2.

If item 4 or 5 fails, or any exact logical/durability equality fails,
standalone COMMIT remains a hard veto and F2-v3 stops before any acceptance
build/row. If item 6 alone fails while 1–5 pass, state coupling is not directly
observed; keep standalone COMMIT hard and stop rather than infer causality.

Current state: diagnostic preregistered; no v3 diagnostic source, binary,
artifact root, base, or row exists. F2 remains **FAIL / REVISE** and F3 is
ineligible.

## F2-v3 same-binary diagnostic result

The diagnostic used one executable, SHA-256
`7efdcfabd76d1b05011faf9d23aaff0bddd7f610a518cd2b5ca7e7bdd065041e`,
and diagnostic source SHA-256
`092916ab9146386c53e193ba2326be076648b41e8f592fbae6bbd24ae2360471`.
The worktree source was restored immediately afterward to the byte-identical
v2 algorithm, `c8ac86be…cc158`.

One P2 runner failure is retained: warmup C0 completed, but the parser compared
the emitted `C0-full-verifier` label with short arm label `C0` and stopped
before append. The resume path imported that exact stdout/stderr once and did
not rerun it. No C1 or later row had started. The final schedule and all 12
rows are exact.

All six prospective gates pass:

1. identities, writes, schema/storage, FULL+DELETE, one transaction/COMMIT,
   Q, publication, reconciliation, fresh verification, reconstruction, and
   ranges are exact;
2. proof-enabled mapping counters and all S0 pager/filesystem snapshots are
   byte/numerically equal within every pair;
3. C0 performs exactly 5,373 verifier queries/rows/BLOB/authentications over
   105,291,608 canonical bytes and zero proof consumptions; C1 performs one
   head query, zero row/BLOB/authentication work, and one proof consumption;
4. nested dispatch is C0 `113.489041 ms` versus C1 `171.936500 ms`; C0 is
   lower in 5/5 pairs and C1/C0 paired-median movement is `+51.700701%`;
5. C1 `qualification + outer COMMIT` improves
   `499.246792 -> 172.375459 ms` (`-65.472896%`, paired
   `-65.457849%`, 5/5); and
6. qualification changes named state in exactly 5/5 pairs.

At S1/S2, relative to C0, C1 has exactly 43,703/43,702 fewer cache hits,
6,694/6,695 fewer misses, one fewer dirty main-DB write, one fewer spill, and
4,096 fewer allocated database bytes. S0 is exact-equal. At S3 both modes
report 26,676 dirty writes; C0/C1 retain 6,676/6,675 spills, so the exact
COMMIT dirty-page equations are 20,000/20,001. C0 moves one dirty write/spill
and one 4-KiB allocation before dispatch; C1 leaves that page for COMMIT.

Observed: full-verifier activity deterministically changes SQLite pager/
filesystem state before dispatch, and standalone dispatch moves in the
opposite direction to combined-tail/durable work. Inferred narrowly: the
standalone COMMIT regression is phase coupling/work redistribution, not extra
logical writes or an F2 proof correctness defect. Unavailable: VFS calls/
bytes, xSync calls/wall, journal true peak, temp peak, and physical media I/O;
the evidence does not attribute the approximately 58-ms dispatch difference
to the single observed page or claim a physical mechanism.

Diagnostic raw/snapshot/summary/storage SHA-256 are
`00760628…1aac`, `877eb9e7…2045`, `c9ffab8c…4f72`, and
`a6089029…5fab`. The diagnostic remains non-acceptance evidence.

## F2-v3 prospective acceptance contract

This contract is frozen after the supporting diagnostic and before creation
of the v3 acceptance artifact root, validation outputs, pair bases, or rows.
It never relabels v1 or v2.

### Frozen implementation and comparison

- A/control: sealed F1-v3 executable
  `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805`.
- B/candidate: exact sound F2-v2 executable
  `68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0`,
  copied into v3 without rebuild.
- Candidate source:
  `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
- Fixture/manifest:
  `63b3695b…eff4` / `8c64b5f4…d4ca`.
- Acceptance root:
  `target/wp4m-f2-construction-proof-k64-20260819-v3`.

No production source or binary variable changes from v2. Reuse of the exact
binary makes the passing v2 release M4.5 raw (`054af4a4…2d7a`) valid by hash;
copy/reference it in v3 custody rather than rerunning the same executable.
Fresh full tests, Clippy, fmt, diff, environment, and source/binary custody are
still required before acceptance rows.

### Amended phase-coupling policy

Standalone outer COMMIT and nested dispatch remain mandatory reported
diagnostics but are not independent `<=5%` vetoes. This amendment applies only
because the prospectively gated same-binary diagnostic passed all six gates.
Hard gates replace—not hide—the old veto:

```text
combined_tail = pre-COMMIT qualification + outer COMMIT
```

Candidate durable capture and combined tail must each improve by at least 5%
for arm medians and paired medians, with at least four of five paired wins.
Exact FULL+DELETE, one COMMIT dispatch/return, complete-head publication,
reconciliation, timer equations, logical writes, final pager equations,
schema/storage, and fresh verification remain hard.

Pager gates are exact for the frozen rows:

```text
final dirty main-DB writes  A/B = 26,676 / 26,676
final spills                A/B =  6,676 /  6,675
COMMIT dispatch/return and timer equations = exact
```

Any extra candidate logical write, dirty write, spill beyond that frozen
equation, durability change, or unexplained serialized/storage endpoint is a
hard failure. VFS/sync/physical facts remain Unavailable rather than inferred.

### Authorized absolute-Q contract

Relative Q is not an acceptance metric. Candidate Q passes only when:

```text
q_high_water <= 73,728 bytes
q_current = 0 on every row and injected failure
proof-owned retained equation = 21,952 bytes
memory class = O(K + F*(H+1) + bounded buffers)
```

RSS and peak footprint remain independently protected at +5% arm/paired and
at least four of five pairs within the ceiling.

### CPU and storage protection

Total CPU, RSS, peak footprint, and allocated-store delta use the +5%
arm/paired/at-least-4/5 rule. System CPU is separately hard-bounded using the
retained v1/v2 and same-binary observations plus `/usr/bin/time`'s 10-ms
display resolution:

```text
candidate - control system CPU arm median   <= 60 ms
paired-median system CPU increase           <= 60 ms
at least 4/5 pair increases                 <= 60 ms
```

The ceiling is frozen above the repeated retained +40–50-ms shift while total
CPU improved 8.6–19.5%; it prevents that component from being hidden inside
total CPU without treating coarse small-component percentages as precise.
Logical/apparent DB, journal, and authority bytes remain exact. Allocated
endpoints retain the +5% protected rule and no-residue requirement.

### Tiny unchanged phase envelope

V1/v2 pair variation reached 151,166 ns for fresh reopen and 77,791 ns for
ranges on unchanged paths. Before v3 observations, freeze a 200,000-ns
absolute floor, rounded conservatively above both retained maxima. Fresh
reopen and ranges each pass timing only when:

```text
arm-median increase <= max(5% of control median, 200,000 ns)
paired-median increase <= max(5% of paired control basis, 200,000 ns)
at least 4/5 pairs satisfy candidate-control <= max(5% of control, 200,000 ns)
```

Exact returned bytes, authenticated object/canonical-byte counters, head,
closure, and errors remain hard. Fresh scrub and reconstruction retain the
ordinary +5% arm/paired/at-least-4/5 rule. No tiny-phase extension is
authorized because the envelope is fixed from pre-v3 evidence.

### Campaign and terminal gates

Assert before preparation or invocation:

```text
pair0 warmup   AB
pair1 measured AB
pair2 measured BA
pair3 measured AB
pair4 measured BA
pair5 measured AB
```

Prepare each pair once and byte-copy/hash-equal database, authority, and
expectations to both arms. Retain raw JSONL, preflight, commands, resource
observations, environment/toolchain/test/static custody, source/binaries,
primary Python analysis, independently implemented non-Python analysis,
storage/schema audit, versioned manifest, and final read-only audit. No row
replacement, optional extension, or threshold amendment is permitted.

F2-v3 is **PASS / retain; F3 eligible** only if every exact identity/work/
authority/durability/pager/storage/Q/M4.5 gate, both >=5% durable and
combined-tail gates, CPU/resource gates, tiny-phase envelope, two-analyzer
agreement, manifest, and final audit pass. Otherwise stop **FAIL / REVISE**
with the smallest remaining cause. No commit or F3 work is authorized.

Current state: acceptance contract frozen; acceptance root/bases/rows do not
yet exist.

## F2-v3 terminal acceptance result

Disposition: **PASS / retain; F3 eligible for a separate reviewed task**.
No F3 work, profile selection, production integration, schema/backend change,
commit, reset, cleanup, or sibling-repository operation was performed.

### Frozen custody and execution

The acceptance campaign used the exact preregistered source and executables:

| Item | SHA-256 |
|---|---|
| candidate source | `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158` |
| sealed F1-v3 A/control | `732171041ea25684399d308af1d4682bb9fc58b2a3c79e16080b39d0cb32b805` |
| frozen F2-v3 B/candidate | `68b599b819da9f05c76d35efd807c5d5f03266dfb7d4ed0cc78da269c4b891c0` |
| retained 100-MiB fixture | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| fixture manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| raw JSONL | `0452726e74b207dabd77f70aab04ef8be4a3aa81162dd3fdcdb094b13d6de46e` |
| preflight TSV | `a7750651d80d00b86404529e3b63a839ed097d16db97e026ecb412008763a06d` |
| primary / independent summaries | `af20a9505181aab319ea6bab73966e5dc6b073a0fd1bde544c28d2573c404e0b` / `6b7481c8c075e8c313d3567af4a2eacb0a6448dd8678b7d1fd18918a6f58181d` |
| storage audit | `e8767c172d48adcbf4fdbb4233625edbf4a09f50f04989d59d52645a4d721765` |

The exact schedule was `AB/AB/BA/AB/BA/AB`: pair 0 was warmup and pairs
1–5 were measured. Each pair was prepared once. Database, authority, and
expectation arm images were physical byte copies with equal hashes in all 12
rows. There was no row replacement, extension, or threshold amendment.

Fresh validation passed 13 focused F2 tests and all 113 workspace tests (44
core + 4 engine + 48 private benchmark + 12 parity + 5 eval), offline Clippy
with `-D warnings`, rustfmt, `git diff --check`, and the debug self-test. The
candidate executable is byte-identical to v2, so the copied v2 release M4.5
campaign remains the applicable protected regression proof; its raw SHA-256
is `054af4a4a4287f256d352071860289497b447638268e0ec8773e40f93b102d7a`
and it remains PASS (`433.194708 -> 8.422917 ms`).

### Exact semantic, authority, write, and Q result

Every A/B row has the frozen source fingerprint, 5,284-entry CDC sequence,
root, transition, and fresh root-first ordered-closure digest. Reconstruction
and all range returned-byte/authentication counters are exact. Both arms have
5,372 created objects, 105,291,554 new canonical bytes, 365,262 rewritten
mapping bytes, the same SQL execute/changed-row and BLOB-write counts, one
transaction, one COMMIT dispatch/return, complete-head publication, no
reconciliation call, and a successful fresh reopen/scrub/reconstruction/range
sequence.

Runtime durability is unchanged `FULL` (`synchronous=2`) + `DELETE`. Final
main-DB dirty writes are exactly A/B `26,676 / 26,676`; final spills are
`6,676 / 6,675`, matching the prospectively frozen pager equations. All 12
databases have the same schema SHA-256
`e83baa3550f5f58d974bf67c19c4ae59da354260155588f5ccef21f7318ad162`,
5,372 object rows, one meta row, one visible-head row, 109,268,992 logical/
apparent DB bytes, a 32-byte authority endpoint, and no journal/WAL/SHM
residue. No schema or serialized endpoint was added.

Candidate construction counters are exact in every row:

```text
put evidences / covered edges                  = 5,372 / 5,371
leaf / branch / file / workspace / transition = 83 / 2 / 1 / 1 / 1
source fingerprint bytes / hashes              = 104,857,600 / 1
required raw ChunkId bytes / hashes             = 104,857,600 / 5,284
CDC accumulator entries                         = 5,284
proof consumptions                               = 1
```

The candidate performs one pre-COMMIT head query and zero returned rows,
row-BLOB reads, object authentications, or raw hashes. Control-to-candidate
reductions are SQL queries `5,373 -> 1` (`99.981388%`) and BLOB reads/object
authentications `5,373 -> 0` (`100%`). Total candidate Q is exactly 55,325
bytes on every row, below the authorized 73,728-byte cap, and `q_current=0`
on success and all injected failures. The proof-owned equation remains 21,952
bytes with `O(K + F*(H+1) + bounded buffers)` live memory.

### Frozen performance gates

All values below are five-pair measured medians. Negative change is favorable.

| Metric | A/control | B/candidate | Arm change | Paired median | Wins / protected result |
|---|---:|---:|---:|---:|---|
| mapping/proof construction | 400.461 ms | 492.777 ms | +23.052% | +22.292% | diagnostic |
| pre-COMMIT qualification | 387.465 ms | 0.051 ms | -99.987% | -99.987% | 5/5 |
| standalone outer COMMIT | 126.054 ms | 168.426 ms | +33.614% | +32.254% | 0/5; reported phase-coupled diagnostic |
| combined qualification + COMMIT | 512.861 ms | 168.477 ms | -67.150% | -67.513% | **5/5 PASS** |
| durable capture | 916.310 ms | 659.593 ms | -28.016% | -27.725% | **5/5 PASS** |
| complete lifecycle | 1,607.986 ms | 1,353.841 ms | -15.805% | -15.772% | 5/5 favorable |
| total CPU | 1,620 ms | 1,360 ms | -16.049% | -15.951% | 5/5 PASS |
| maximum RSS | 93,683,712 B | 93,323,264 B | -0.385% | -0.420% | 5/5 PASS |
| peak footprint | 92,488,136 B | 92,111,256 B | -0.407% | -0.425% | 5/5 PASS |
| allocated-store delta | 117,764,096 B | 109,248,512 B | -7.231% | -7.231% | 5/5 PASS |
| fresh scrub | 268.613 ms | 267.886 ms | -0.271% | -0.197% | 5/5 within +5% |
| reconstruction | 423.207 ms | 422.261 ms | -0.224% | -0.180% | 5/5 within +5% |

System CPU is `240 -> 270 ms`: the arm and paired-median increase are both
30 ms and all 5/5 pairs are within the prospectively frozen +60-ms ceiling.
Fresh reopen is `0.904500 -> 0.957791 ms`; its arm/paired increase is 53,291
ns and 4/5 pairs meet the 200,000-ns floor. Ranges are
`0.705000 -> 0.670416 ms`; the arm/paired changes are favorable and 5/5 meet
the envelope. Exact tiny-phase outputs and work counters match in every row.

The primary Python and independently implemented Ruby analyzers produce the
same canonical statistics SHA-256
`3500674f05ee0a99b5a3762c8ce23b0f3e46582e0c0424c47206e19598e93823`
and both return `PASS / retain`, no failed hard gate, and `f3_eligible=true`.

### Severity and causal disposition

- P0 implementation/correctness defect: none demonstrated. Standalone
  transaction-local authority, exact incumbent authentication, cleanup,
  identity, topology, write shape, durability, and independent verification
  all pass; the proof was not redesigned for v3.
- P1 engine/acceptance investigation: closed for this prospectively frozen v3
  contract, not erased. Same-binary C0/C1 evidence proves verifier-dependent
  pager/filesystem state and work redistribution across qualification and
  COMMIT boundaries; standalone COMMIT remains +33.614% and is reported.
  VFS calls/bytes, xSync calls/wall, journal true peak, and physical media I/O
  remain **Unavailable**, so no narrower physical cause is claimed.
- P2 resource policy: resolved prospectively by the user-authorized absolute
  Q contract. The +18,025-byte measured control/candidate difference is small,
  exact, bounded, under cap, and not hidden by a relative percentage.
- P2 tiny phases: resolved by the pre-row 200-us absolute-plus-relative
  envelope while keeping exact outputs/work hard.
- P2 diagnostic orchestration: the prior idle outer timer remains invalid and
  unused; the same-binary parser interruption and primary-analyzer status-label
  correction are retained explicitly. Neither changed or reran benchmark rows.

### Before/after algorithm and final decision

F1 performs one source/CDC/CAS construction pass and then a second full
SQLite root/file closure replay before COMMIT. F2 issues private evidence only
after canonical insertion or exact incumbent authentication, folds bounded
occurrence/edge summaries through the existing `FileBuilder`, and consumes a
single-use proof bound to open/store/authority/epoch/profile/transaction/
mutation state plus source fingerprint, CDC sequence/count, total raw bytes,
root, workspace, and transition. The flat root-first closure digest is not
treated as bottom-up composable; it is freshly recomputed after COMMIT.

Time remains honestly `Theta(B + N)`: F2 removes a duplicate linear database
pass but cannot remove the source/CAS lower bound. Live construction memory is
`O(K + F*(H+1) + bounded chunk/SQL/encoding/output buffers)`. Durable space
remains `Theta(B_u + N)`. There is no all-reference/object/event list, map,
cache, visited set, source spool, table, sidecar, public framework, dependency,
or serialized metadata.

Every prospectively frozen v3 hard gate passes. Retain F2 as the next private
full-create control. F3 is eligible only as a separate user-reviewed task;
this work stops before F3 and remains uncommitted.
