# Canonical-v2 fast-iteration agent prompt

Copy the prompt below into a new Codex task. It authorizes research, a
nonpersistent shadow, focused implementation, and short exploratory screens.
It does not authorize production integration or a promotion-grade campaign.

---

`/goal` Explore, design, and rapidly test the best canonical-v2 single-identity
direction for LayerFS. Use parallel subagents freely for deep code/evidence and
primary-source research, build the smallest nonpersistent shadow first, and run
only short focused experiments needed to learn the achievable speed. Stop with
an evidence-backed canonical-v2 recommendation before production integration or
a long acceptance campaign.

## User intent

The user wants ambitious local Phase-4 optimization, especially durable full
create, without spending the discovery phase on long test suites or overly hard
promotion gates. Explore creatively and use subagents, but do not weaken
identity, authentication, bounded-memory, atomic-publication, exact-error, or
durability guarantees.

This is fast iteration:

```text
understand actual code/evidence
  -> parallel research
  -> nonpersistent shadow
  -> focused tests
  -> one small benchmark-private candidate at a time
  -> <=120-second exploratory screen
  -> report how good it can plausibly become
```

Do not run a full five-pair campaign, full workspace suite, 512-MiB campaign,
multi-host study, production migration, or integration during this task.

## Repository and custody

Work only in:

```text
/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty
```

Expected branch and committed HEAD:

```text
branch  codex/empty-worktree
HEAD    febc20f046bba84ccdce1256363d77799eabf2db
```

Never touch sibling `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`.

The worktree is intentionally dirty. Preserve CP-0007/8/9, the research tree,
H05/H05b/H05c, and every unrelated user file. Do not commit, reset, clean,
delete, overwrite historical evidence, or run a broad checkout.

## Accepted control and current source warning

CP-0009 remains the accepted control:

```text
control source SHA-256
3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a

control source diff from HEAD
b073a7e04c7a7a2b17671f80c42aee598cc5d8039e4ba83d63b7cac89d150f84

control release executable
9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7

fixture size
104857600

fixture SHA-256
63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4

durable create              640.109209 ms
construction               504.215417 ms
proof consumption            0.038542 ms
COMMIT                      135.855250 ms
```

The live benchmark-private source is currently the rejected H05 candidate,
not CP-0009. Before modifying candidate source:

```text
live and frozen H05 source
e675d2fc7646745eaf709f61703ff84098949ce4319cb4e6882b96698d95d031

frozen H05 source
target/phase4-h05-canonical-witness-screen-20260821-v1/candidate/phase4_create_edit_benchmark-h05.rs

frozen CP-0009 source
target/phase4-h05-canonical-witness-screen-20260821-v1/control/phase4_create_edit_benchmark-cp0009.rs

frozen CP-0009 executable
target/phase4-h05-canonical-witness-screen-20260821-v1/control/phase4_create_edit_benchmark-cp0009
```

1. freeze `pwd`, branch, HEAD, status, toolchain and relevant hashes;
2. verify the sealed H05 candidate remains preserved under `target/`;
3. restore only
   `crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs` to the exact
   frozen CP-0009 source using a narrow patch;
4. verify the CP-0009 source/diff hashes above; and
5. build every canonical-v2 variant from that control.

Do not use H05 source as the v2 base and do not relabel its screen rows.

## Historical H05 conclusion

Read [H05 terminal findings](h05-terminal-findings.md) first.

The immutable conclusions are:

```text
H05 v7       MEASURED NO-GO / REVERT
performance  3/3 wins; 16.655343% paired median
blocker      frozen exact allocated-storage equality
H05b         allocation observer NOT JUSTIFIED / STOP
H05c         H05 CLOSED / A/A EXACT-EQUALITY STABLE
control      CP-0009 retained
```

H05 cannot be reopened or promoted. Its authority proof, ordered canonical
commitment, tests, and measured cost are useful priors for canonical-v2. H05
rows are not canonical-v2 acceptance evidence.

## Read first

Read these completely before deciding what to build:

1. `research/phase-4/core/canonical/h05-terminal-findings.md`
2. `research/phase-4/core/canonical/v2-single-identity.md`
3. `research/phase-4/core/canonical/identity-and-hashing.md`
4. `research/phase-4/core/pipeline/full-create-pipeline.md`
5. `research/phase-4/core/cas/authenticated-reuse.md`
6. `research/phase-4/core/cdc/locality-and-algorithms.md`
7. `research/phase-4/core/cow/mapping-and-deltas.md`
8. `research/phase-4/assurance/verification-security-resources.md`
9. `research/phase-4/foundations/invariant-matrix.md`
10. `research/phase-4/foundations/benchmark-and-evidence.md`
11. `research/phase-4/foundations/hypothesis-ledger.md`
12. `implementation-detail/phase-4/algorithm/spec.md`
13. `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md`
14. `implementation-detail/phase-4/algorithm/complexity-analysis.md`
15. `implementation-detail/phase-4/mapping/logical-persistence.md`
16. `implementation-detail/phase-4/storage/sqlite/visible-head.md`
17. CP-0009 report, raw analysis and baseline manifest;
18. H05 v7, H05b and H05c terminal reports/manifests;
19. actual current `layerfs-core` identity/object/content/CAS/COW/delta/
    validation code and production SQLite engine code;
20. the frozen CP-0009 and H05 source copies for exact path comparison.

Existing research is routing context, not evidence. Verify decisive claims
against source, tests, or sealed rows.

## Parallel subagent freedom

Use available subagents aggressively during the research phase. Give them
disjoint, read-only ownership and require each to inspect actual code and
evidence rather than repeat this prompt.

Suggested lanes—not mandatory if a better split emerges:

### Lane A — identity and authority

Trace every raw `ChunkId` and canonical `ObjectId` producer/consumer. Challenge
whether canonical identity alone preserves exact source equality, CAS reuse,
rejoin, closure, scrub, reconstruction, ranges, errors and collision
assumptions. Produce an adversary matrix and minimum v2 proof.

### Lane B — format, migration and errors

Model v1/v2 codecs, mapping profile dispatch, v1-parent/v2-child transitions,
receipt binding, downgrade rejection, retained history, empty/nonempty store
migration, and exact error mapping. Find the smallest honest transition—do not
invent a general migration framework.

### Lane C — execution and performance

Trace the real 100-MiB source/CDC/raw-hash/canonical-hash/CAS/mapping/SQLite/
COMMIT path. Recompute row-wise ceilings, find where a canonical ID can be
carried rather than recomputed, and design direct counters. Inspect compiler/
BLAKE3/SQLite paths where useful.

### Lane D — creative external research

Search broadly for primary sources and original implementations of single-ID
CAS, ordered chunk commitments, history-independent mappings, authenticated
streaming and format migration. Git, Xet, Venti, Nix, Bao, Merkle standards and
modern CAS systems are precedents only; map every idea back to LayerFS code and
invariants.

Subagents may propose disruptive alternatives. The coordinator decides which
smallest variant deserves code. Do not let subagents run competing benchmarks
or edit the same files concurrently. Pause all local work during timed rows.

## Canonical-v2 target

The recommended design target is:

```text
v1 occurrence
  raw ChunkId[32] || raw_length[4] || canonical ObjectId[32]
  = 68 bytes

v2 occurrence
  raw_length[4] || canonical ObjectId[32]
  = 36 bytes

v2 ordered commitment
  BLAKE3(derive-key domain || repeated(u32be(length) || canonical ObjectId))
```

Retained-fixture structural model:

```text
references                         5284
mapping bytes v1                 365262
mapping bytes v2 model           196174
mapping reduction                169088
full K64 leaf v1                   4380
full K64 leaf v2                   2332
raw ChunkId hash gross lane        95.185147 ms
H05 canonical commitment input     190224 bytes
```

The row-wise optimistic combined ceiling is 427.084-454.849 ms, median
452.873 ms. Treat it as an upper bound, never an expected result.

## Hard semantic boundary

Fast iteration does not weaken these requirements:

- exact source bytes and frozen CDC boundaries/lengths;
- exact canonical chunk object bytes and canonical ObjectIds;
- complete canonical-byte authentication before semantic use or reuse;
- immutable equal-only CAS reuse and authenticated incumbents;
- deterministic, history-independent v2 bytes and roots for the same profile;
- exact count/total/order/topology and ordered commitment;
- bounded memory, checked arithmetic and terminal `Q=0`;
- exact failure precedence within each version and explicit v1/v2 error map;
- one caller-thread writer transaction, one publication COMMIT, atomic visible
  head, `FULL + DELETE`, and fresh ambiguous-outcome reconciliation for any
  durable private candidate;
- no journal/WAL/SHM residue or unsupported zero;
- no output before required authentication.

V2 mapping/root/transition/receipt/profile identities are expected to differ
from v1. Do not require v1 root equality. Require logical equivalence and exact
profile-specific deterministic identities.

## Phase 1 — nonpersistent shadow first

Before any durable v2 write, implement the smallest shadow that can answer:

1. Can every v1 reference normalize to `(raw_length, canonical ObjectId)`
   without fetching old payloads?
2. Can exact v2 36-byte references encode/decode canonically with hard limits?
3. Do independent construction paths produce identical v2 leaves, branches,
   root, ordered commitment and profile ID?
4. Can the same canonical chunk BLOBs back both v1 and v2 mappings?
5. Can full create, same-count edit, `+1/-1` rejoin, scrub, reconstruction and
   exact ranges produce the same logical bytes/work expectations?
6. What exact errors replace legacy wrong-raw-ID cases in v2?
7. Is v1-parent/v2-child publication supported explicitly, or rejected with a
   precise migration error?
8. Can receipts and visible-head checks reject profile/downgrade confusion?

Prefer an in-memory/test-local shadow and existing codecs/helpers. Do not add a
public provider/engine trait, registry, selector, generic migration framework,
or persistent bridge format merely to answer these questions.

Focused shadow tests should cover at least:

- empty, one, exact K, K+1, K*F and K*F+1 boundaries;
- repeated chunks, wrong length, wrong role, malformed/trailing bytes;
- omitted/duplicated/reordered references;
- v1 normalization and canonical-BLOB reuse;
- same logical content built through different mutation histories;
- same-count and count-changing rejoin;
- v1/v2 profile confusion and downgrade rejection;
- reconstruction and cross-chunk/leaf/branch ranges;
- checked overflow/Q cleanup.

## Phase 2 — explore variants cheaply

After the shadow passes, compare only variants that isolate useful questions.
You may choose a better set after code/evidence review. Plausible variants are:

1. **same-width nonpersistent bridge:** duplicate canonical ID in both v1
   slots to isolate raw-hash removal without storage shrink; never promote it;
2. **compact fixed-radix v2:** the real 36-byte reference under K64/F64;
3. **carried canonical ID:** compute once during CDC/rejoin and hand the
   authenticated value through persistence so no hidden recomputation remains;
4. **ordered-commitment placement:** reuse the proven H05 commitment boundary
   without retaining the rejected H05 candidate as the source base.

Do not stack unrelated CDC, prolly, compression, page-size, cache, worker or
VFS changes. Test one explanatory variant at a time and preserve each result.

Microbenchmarks or counter-only runs may screen variants before a complete
durable row. A microbenchmark cannot claim complete speedup.

## Fast validation policy

Discovery validation should be proportional and short:

- run only the focused package/test names affected by the current variant;
- use one small deterministic fixture for shadow/error tests;
- use `cargo check` or the narrowest build that proves compilation;
- build a release candidate once only after the shadow and focused tests pass;
- run `git diff --check` on owned files;
- do not run `cargo test --workspace --all-targets`, full Clippy, the 42-row
  package, 512-MiB scale, multi-host, or long fuzz/property suites in this
  fast-iteration task;
- record deferred validation explicitly for a later retained candidate.

No single validation or benchmark command should intentionally run longer than
60 seconds. The complete exploratory screen has a hard 120-second wall. Stop a
slow test and replace it with the smallest focused reproduction.

## Exploratory screen, not a promotion gate

For the most promising benchmark-private variant, prospectively freeze:

- exact CP-0009 control and candidate source/binary hashes;
- exact fixture/profile and candidate v2 profile ID;
- schedule and row count;
- timer equations and direct counters;
- expected v2 mapping/root/storage equations;
- protected same-count edit, count-change, scrub, reconstruction and range
  smoke;
- Q/RSS/CPU/storage observations and unsupported reasons.

Use a short balanced screen, normally:

```text
one uncounted warmup pair: AB
three measured pairs:     AB / BA / AB
```

If a simpler counter/micro screen answers the current variant question, use it
first. Preserve every row and never selectively rerun or amend after seeing
data.

Performance is graded for exploration:

```text
semantic/authority failure             REVISE or STOP
negative median and <=1/3 wins          STOP variant
positive median with >=2/3 wins         PROMISING
>=5% median with >=2/3 wins             STRONG
>=15% median with 3/3 wins              BREAKTHROUGH
```

These are research labels, not acceptance or promotion. A promising 1-4.99%
result may remain worth combining with later independently measured core work;
do not reject it merely for missing 5%.

The primary reporting question is: how much of the 95.185-ms raw-ID lane and
the combined full-create gap did the variant actually remove?

## Exploratory storage policy

Canonical-v2 intentionally changes mapping bytes and roots, so do not require
v1/v2 endpoint or SHA equality. Instead require:

- exact source/canonical chunk BLOB equality;
- exact v2 format/mapping/root/transition equations;
- deterministic equality between independent candidate v2 constructions;
- expected logical/apparent mapping reduction;
- no unexplained serialized metadata;
- no journal/WAL/SHM residue;
- every allocated endpoint reported separately;
- no catastrophic allocation expansion: candidate store allocation must stay
  within 125% of its own expected apparent endpoint during exploration;
- paired allocation overhead and direction reported, not hidden.

A later promotion campaign must preregister tighter storage/performance gates
after the representation is selected. Do not calibrate a promotion tolerance
from exploratory outcomes.

## Autonomy and repair

Continue autonomously through ordinary source, test, harness, analyzer and
packaging defects. Use fresh versioned evidence namespaces, preserve history,
and use subagents for independent review. Do not stop merely because a shell
path, parser, fixture preparation or analyzer assertion is fixable.

Stop for user direction only if progress requires a materially different
product decision, such as:

- abandoning canonical self-contained chunk objects;
- accepting a public migration policy with destructive rewrite;
- weakening collision/authentication assumptions;
- adding concurrency/workers;
- changing CDC boundaries;
- promoting durable v2 bytes or integrating production.

## Deliverables

Write organized outputs under the canonical research topic and a new versioned
`target/` experiment root:

1. code/evidence/subagent synthesis;
2. exact raw-ID consumer and authority graph;
3. v1/v2 shadow specification and deterministic vectors;
4. migration/error/receipt decision matrix;
5. performance ceiling and direct-counter model;
6. focused tests and commands;
7. every exploratory variant and disposition;
8. short-screen raw rows and independent analysis, if run;
9. source/diff/binary/fixture hashes and limitations;
10. one recommended next action.

Terminal response must answer:

- which v2 design is semantically viable;
- which variant was fastest and by how much;
- estimated remaining gap to 500/400/333.333 ms, clearly labeled as evidence
  or model;
- create/edit/scrub/reconstruction/range tradeoffs;
- migration and exact-error blockers;
- whether a promotion-grade canonical-v2 campaign is worth authorizing;
- which long tests and integration work remain deferred.

## Scope stop

Do not commit, promote a profile, integrate production, rewrite retained v1
history, run a full campaign, start H09/prolly, WP5, materialization, SQLite
page-size, compression, carrier, workers/async, or claim Phase 4 complete.

Stop after the fast canonical-v2 exploration and recommendation.

---
