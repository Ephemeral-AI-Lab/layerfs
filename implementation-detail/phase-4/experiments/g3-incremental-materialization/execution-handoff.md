# G3 autonomous execution handoff

> **Historical handoff — do not execute.** This prompt predates the controlling
> **G3 PASS / G4 READY — v13 STATICALLY CLOSED AND TERMINALLY SEALED** result.
> The immutable v11 package is historical REVISE, v12 is PREEXEC REVISE with
> zero rows, and v13 is the fresh controlling terminal. G4 is planning-only and
> UNSTARTED; Phase 4 remains incomplete. See [G3-REPORT.md](G3-REPORT.md).

Copy the prompt below into the next Codex task.

---

```text
/goal Complete Phase 4 G3: design, implement, validate, and freeze the smallest
correct destination-authority-gated incremental materialization prototype. Work
autonomously through implementation defects, evidence-protocol defects, and
negative iterations until G3 reaches an honest terminal PASS or every
authorized G3 mechanism is exhausted by a precise evidence-backed blocker.
Stop before G4. Do not commit.

AUTONOMY / DO NOT PAUSE FOR ROUTINE AUTHORIZATION

The user explicitly authorizes you to work through all of G3 while they are
away. Within the scope below, do not stop merely because an attempt reports
REVISE, NO-GO, a benchmark protocol defect, a failed test, or a candidate
performance miss. Do not ask the user to authorize the next fresh version,
dry-run, short screen, or repair campaign.

Instead:

1. preserve the failed/rejected attempt append-only;
2. identify the smallest shared root cause;
3. use subagents for independent semantic, authority/security, code-path, and
   evidence review when helpful;
4. create a fresh versioned attempt;
5. preregister it before measurement;
6. run the shortest falsifying check first; and
7. continue until the G3 close rule is satisfied.

This is standing authorization for G3 source edits, focused tests, builds,
dry-runs, versioned evidence attempts, short measured screens, repair attempts,
and the final static closure. It does not authorize destructive Git commands,
rewriting historical evidence, committing, G4, WP5, broad OS/application
integration, or weakening correctness/authority to obtain speed.

If a true external or semantic blocker remains after all authorized mechanism
classes below have been tried, do not wait for user input. Finish a terminal
G3 FAIL/DEFER report that proves the blocker, restores the accepted product
source, preserves all evidence, and states the exact missing external
authority. That is the only acceptable non-PASS stopping condition.

REPOSITORY / CUSTODY

Work only in:

  /Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty

Branch:

  codex/empty-worktree

Starting committed HEAD:

  d79f0e0e2582d1bc491410224fec2b6cef7482e9

The working tree intentionally contains uncommitted G2 documents and
methodology. Preserve them. Never touch the sibling `layerfs` repository.
Never reset, clean, delete, overwrite, chmod, relabel, or selectively rerun
sealed G0/G1/G2 evidence. Never use `git reset`, `git clean`, or checkout to
discard work. Revert a failed candidate only with a narrow reviewed patch that
restores the accepted source bytes while leaving evidence intact.

Current accepted product runtime remains G1:

- Canonical-v2;
- exact-boundary FastCDC contiguous-region kernel;
- SQLite writer policy `PRAGMA cache_spill=2000`;
- 100-MiB durable create: 308.884052 ms / 323.746076 MiB/s;
- writer maximum RSS: 12.48 MiB;
- SQLite cache snapshot maximum: 8.35 MiB.

G2 is closed. It did not change product source.

G2 authoritative result:

  G2 PASS / INSUFFICIENT_EVIDENCE FOR A CONSTANT-FACTOR CANDIDATE

G2-v5 evidence:

- result root:
  target/phase4-g2-materialization-decomposition-20260822-v5/results-v5
- payload manifest SHA-256:
  12f74b88188c1a22babe129c4b1d5d0e1889ba55d2cf0046ae55af6803709399
- terminal SHA-256:
  09a5948a2c6a31c55811d50459c24cf72c4d2e3ff61ea5773754bf5c6c1a60a2
- terminal verification SHA-256:
  41447453a34b1933850e6e090a2bc59628d58f7d585e7c394e937cfe03250af0
- raw JSONL SHA-256:
  c64a4f7b4d1a831fd7406251f0de2ab44cfbf390d07188d55298fdbbfefb0eeb
- normalized ledger SHA-256:
  5de0586cdcb80932b503458c0b74e1983b3b2b5179adc6ba5ed4480aa7af33b9

Post-G2 static closure already passed:

- `cargo test --workspace --offline --all-targets`:
  142 passed, 1 ignored, 0 failed;
- `cargo clippy --workspace --offline --all-targets -- -D warnings`: PASS;
- `cargo fmt --all -- --check`: PASS;
- `git diff --check`: PASS.

READ FIRST — COMPLETELY

Before changing source, freeze pwd/branch/HEAD/status and hashes, then read:

1. `implementation-detail/phase-4/2026-08-21-phase-4-full-grind.md`,
   especially sections 1-5;
2. `implementation-detail/phase-4/experiments/g2-materialization-decomposition/
   G2-POST-PASS-STATIC-CLOSURE-20260822.md`;
3. the sealed G2-v5 terminal, verification, primary analysis, independent
   recomputation, raw rows, cleanup, and proxy-custody evidence;
4. `implementation-detail/phase-4/baseline/current-benchmark-scoreboard.md`;
5. `implementation-detail/phase-4/baseline/canonical-v2-baseline-v1.md`;
6. `research/phase-4/handoffs/hot-cold-materialization.md`;
7. `research/phase-4/assurance/verification-security-resources.md`;
8. `research/phase-4/foundations/invariant-matrix.md`;
9. `research/phase-4/foundations/benchmark-and-evidence.md`;
10. `research/phase-4/decision-map.md`;
11. the Phase-4 algorithm spec, lifecycle, complexity analysis, canonical
    object/receipt code, current logical reconstruction/range path, and every
    caller you may change.

Treat older statements that G2 is active or that pre-G1 baselines control as
historical. The roadmap plus G2 post-PASS closure are current.

G2 FINDING / WHY G3 EXISTS

The current 100-MiB complete authenticated reconstruction remains roughly:

- warm same-open logical reconstruction: 338.776 ms / 295.180 MiB/s;
- fresh-process logical reconstruction: 366.357 ms / 272.958 MiB/s;
- G2 decomposition control center: 328.897 ms;
- G2 instrumented center: 332.405 ms, +1.067% observer overhead.

G2 measured these median component families:

- canonical authentication: 94.817 ms;
- closure commitment: 88.483 ms;
- source/output fingerprint: 87.890 ms;
- SQLite BLOB acquisition: 59.404 ms;
- secondary byte decode: 0.141 ms.

Only the 0.141-ms decode was directly removable under the current authority.
Do not reopen it as a headline candidate. G3 must avoid full-file work under an
explicit authority, not make the full-file loop microscopically faster.

G3 OBJECTIVE

Build a production-shaped but narrowly scoped prototype that makes repeated
materialization proportional to proven changed work when authority permits,
while preserving a complete authenticated fallback.

The prototype must address these operations:

1. receipt/authority-valid no-op materialization;
2. one-byte same-size parent-to-child update;
3. same-size 1-MiB replacement;
4. stale/invalid receipt or authority;
5. external destination mutation or substitution;
6. before-publication, publication, and lost-ack/fault behavior;
7. count-changing edit through an honest complete fallback only.

G3 is a mechanism/prototype stage, not the final 1/10/100-MiB acceptance
matrix. Stop before G4.

AUTHORITY IS THE DESIGN, NOT A DETAIL

Do not treat any of the following alone as proof that destination bytes still
equal a root:

- a MACed receipt saying what was previously published;
- inode, size, mtime, ctime, mode, path, or sidecar metadata;
- a database-local epoch or visible-head generation;
- an FSEvents/watch notification stream without a proved no-gap contract;
- file permissions or an ordinary hard link.

A valid fast path must either:

A. establish and test an explicit exclusive LayerFS custody/mutation model with
   protected, non-replayable continuity and fail-closed invalidation; or
B. avoid trusting the mutable user destination by using a protected verified
   native seed, then clone/copy-on-write and patch it before atomic publication.

Bind every authority to the exact store instance, validation authority,
profile, epoch/generation, parent root, target root, destination identity,
mutation continuity, publication serial, and operation. Use OS randomness for
secrets. A time/path-derived secret is forbidden. A watcher gap, process death,
rollback, downgrade, wrong store/profile/root, external mutation, symlink/
wrong-kind substitution, or missing authority must reject the fast path and
run the exact complete fallback or return the prospectively declared typed
error. Never mint authority from a fast-path miss.

For an ordinary user-editable destination, if exact no-op qualification still
requires hashing all bytes, say so honestly. Do not fake O(1). Pivot within G3
to the protected verified-seed design if it can safely avoid trusting those
bytes.

IMPLEMENTATION LADDER

Use the first rung that is safe and demonstrates the mechanism. Do not build a
framework.

Attempt A — managed destination authority:

- specify the smallest exact custody and invalidation state;
- bind it to current canonical-v2 roots/deltas and existing validation
  authority;
- implement no-op and same-size changed-range application;
- make every unqualified case use the existing complete authenticated path.

If Attempt A cannot prove ordinary destination continuity without Theta(S)
verification, preserve that result and immediately try Attempt B.

Attempt B — protected verified native seed plus atomic clone/patch:

- create a private seed only after full canonical authentication and exact
  output verification;
- key it by an authenticated file-level identity, never by mutable path,
  metadata, harness-only fingerprint, or compressed representation;
- on the same APFS volume, clone the verified seed to a unique temporary file,
  patch only authenticated changed ranges, apply exact metadata, sync under the
  declared durability contract, and publish atomically;
- use a complete streaming copy/reconstruction fallback for clone failure,
  cross-volume output, missing/corrupt seed, or unsupported platforms;
- never hard-link a mutable workspace to a trusted seed.

You may choose an even smaller mechanism discovered by code tracing or
subagent review, but it must satisfy the same authority and work-avoidance
contract. Do not pivot to compression, Git-style packs/deltas, Bao, a second
durable carrier, a global cache, broad worker concurrency, or lazy hydration;
those are outside G3 or already unsupported by evidence.

NATIVE OUTPUT SAFETY / DURABILITY

For every path that touches native output:

- preflight destination path/name admissibility, including case and Unicode
  collision behavior relevant to the target volume;
- use descriptor-relative/no-follow operations where available and reject
  symlink or wrong-kind substitution;
- keep temporary output on the same destination volume;
- expose no candidate bytes before their required authentication;
- publish old-or-new atomically;
- separately define data sync, file metadata sync, rename/publication, and
  containing-directory durability;
- reconcile ambiguous publication from a fresh independent observation;
- clean every temporary/seed/receipt artifact on error;
- preserve exact typed error precedence;
- never infer durability, sync, cache warmth, or physical I/O from wall time,
  logical length, allocation, RSS, or Q.

If the current codebase lacks a production native materializer, begin in a
benchmark-private module with production-shaped types and exact filesystem
calls. Move only the minimum shared logic needed for truthful semantics into
library code. Do not integrate a broad layerfs-os/application surface in G3.

ONE-VARIABLE / PROTECTED BASELINE

G3's one variable is the qualified incremental materialization mechanism and
its minimum authority state. Preserve:

- Canonical-v2 IDs, object bytes, roots, deltas, mapping profile, and current
  storage format;
- FastCDC boundaries;
- current SQLite schema, durability, one-writer/transaction/COMMIT rules;
- authenticated full reconstruction and range semantics;
- exact errors and no-output-before-authentication;
- bounded Q/RSS and caller-visible deterministic ordering;
- G1 writer-memory policy;
- full fallback results and identities.

Do not combine G3 with page-size changes, CDC changes, new chunk sizes,
compression, packfiles, SQLite batching, concurrency, or reopen-authority
optimization.

DIRECT COUNTERS / OBSERVABILITY

Before measurements, preregister exact counters and equations for:

- qualification outcome and reason code;
- destination/seed authority reads and validations;
- mapping/object/SQL queries and rows;
- canonical BLOB reads and authenticated bytes;
- source bytes reconstructed;
- destination bytes read, cloned, copied, patched, and written;
- changed ranges, changed bytes, and metadata operations;
- temporary/seed files and logical/apparent/allocated bytes;
- sync calls, rename/publication calls, and reconciliation;
- user/system CPU where supported;
- RSS, Q high-water, terminal Q, and cleanup;
- complete-fallback work;
- exact timer equations.

Each field must be Observed, Derived with its equation, NotApplicable, or
Unavailable with source/reason. Do not report unsupported values as zero.

The mechanism signal is direct avoided work, not a heroic wall-time gate:

- a qualified no-op must write zero payload bytes and avoid complete payload
  SQL/BLOB/authentication/reconstruction work;
- a same-size one-byte or 1-MiB update must show payload work bounded by the
  authenticated changed ranges plus fixed metadata/authority overhead, not by
  the 100-MiB file size;
- invalid authority and unsupported cases must execute the complete fallback
  with exact baseline identities and no trust laundering;
- wall time must be directionally consistent and must not materially regress,
  but G4—not G3—owns the final performance acceptance matrix.

FAST ITERATION PROTOCOL

Keep iteration fast. Do not run the full workspace suite after every edit.

For each attempt:

1. trace actual callers and write a short prospective preregistration;
2. add the smallest focused semantic/authority/fault tests;
3. build the relevant target once;
4. run a deterministic mechanism/parity screen whose measured portion is
   strictly under 20 seconds total;
5. use 1-MiB fixtures for fault/semantic edges and 10-MiB or a minimal 100-MiB
   row only when needed to prove work is not file-size-linear;
6. analyze direct counters before spending time on more rows;
7. retain, revise, or revert immediately.

No individual command should intentionally run longer than 120 seconds.
Measured attempts use fresh versioned namespaces, one-shot schedules, hard
timeouts, zero selective reruns, adjacent balanced ordering when comparing two
executables, and independent recomputation. Preparation/build time must not be
hidden inside an operation timer, but the campaign must record its complete
global wall.

Do not run a broad 1/10/100 matrix in G3. That is G4. A final retained G3
confirmation may still use one compact screen under 60 seconds total if the
initial sub-20-second screen has a strong direct-counter signal.

After the final candidate is selected, run the full static closure exactly
once:

- focused G3 tests;
- `cargo test --workspace --offline --all-targets`;
- `cargo clippy --workspace --offline --all-targets -- -D warnings`;
- `cargo fmt --all -- --check`;
- `git diff --check`;
- tracked/untracked custody and product-source diff review.

REVISE / NO-GO ITERATION RULE

Do not stop at the first REVISE or NO-GO.

Classify the failure:

1. orchestration/evidence defect:
   preserve the attempt, fix only the protocol in a fresh version, and rerun
   the minimum missing evidence with unchanged candidate bytes when possible;
2. implementation correctness defect:
   repair the shared root cause, extend a focused regression test, and run a
   fresh version;
3. authority/security defect:
   do not weaken the gate; redesign the authority or pivot from Attempt A to
   Attempt B;
4. performance/mechanism miss:
   retain the negative evidence, restore accepted source, inspect counters and
   codegen, then try the next authorized G3 mechanism only if it removes a
   different measured work family;
5. environment/custody failure:
   preserve the failed attempt, repair operands/modes/locking in a fresh
   version, and continue without deleting history.

Use subagents to challenge any universal conclusion such as “incremental
materialization cannot be optimized.” A failed implementation proves only that
implementation unless an actual lower bound or exhausted authority contract is
demonstrated.

Do not endlessly tune magic thresholds or repeat statistically equivalent
rows. Two consecutive attempts that exercise the same mechanism and fail for
the same measured reason require a design change, not another rerun.

G3 PASS CLOSE RULE

Mark G3 PASS and freeze a G3 baseline only when all of the following hold:

- the authority/trust mode is explicit and cannot be replayed across the wrong
  store/profile/epoch/root/destination;
- no-op and same-size qualified paths show the prospectively predicted avoided
  full-file work;
- one-byte and 1-MiB outputs are byte/metadata exact;
- invalid/stale/mutated destinations fail closed into the complete fallback;
- count-changing edits use the exact fallback without a false locality claim;
- fault/publication/reconciliation behavior is old-or-new and cleanup-complete;
- identities, errors, full fallback, durability, Q/RSS, and storage gates pass;
- raw evidence, independent analysis, manifests, environment, commands, and
  limitations are complete;
- final static closure passes;
- accepted product source is left on the retained G3 candidate, or restored to
  G1 if G3 terminates FAIL/DEFER.

If PASS, update the roadmap to `G3 COMPLETE; G4 READY` and stop before G4.

G3 EXHAUSTIVE FAIL/DEFER CLOSE RULE

FAIL/DEFER is terminal only after both managed-destination authority and the
protected verified-seed/clone-patch alternative have been honestly evaluated,
or a shared semantic/platform fact proves both impossible in current scope.
The final report must state:

- the exact authority or platform capability that is missing;
- why full byte verification remains required;
- which short experiments were run and their direct counters;
- why another iteration would repeat the same mechanism rather than test a new
  hypothesis;
- confirmation that accepted G1/G2 source and evidence were restored/preserved;
- what later OS/application primitive would reopen the direction.

Do not ask the sleeping user for permission to write that terminal report.

ARTIFACT ORGANIZATION

Keep G3 documents under:

  implementation-detail/phase-4/experiments/g3-incremental-materialization/

Use one folder per attempt (`v1`, `v2`, ...). Keep raw campaign artifacts under
fresh versioned `target/phase4-g3-*` roots. Never modify earlier attempt roots.
Create/update:

- prospective preregistration per attempt;
- runner and independent analyzer;
- dry-run schedule/custody record;
- raw JSONL and direct-counter schema/dictionary;
- primary and independent analysis;
- terminal manifest and verification;
- final `G3-REPORT.md`;
- accepted `baseline/g3-incremental-materialization-baseline-v1.md` only on
  PASS;
- Phase-4 roadmap status.

FINAL RESPONSE

Return only after G3 is terminal. Include:

- PASS or exhaustive FAIL/DEFER;
- exact trust/authority model;
- implementations attempted and why each was retained/revised/reverted;
- no-op, one-byte, 1-MiB, fallback, mutation, and fault direct-counter table;
- descriptive wall/RSS/Q/storage results with honest cache/I/O limitations;
- identities, durability, errors, and cleanup results;
- focused/full test results;
- source, binary, raw, analysis, manifest, and terminal hashes;
- changed files and retained target roots;
- historical evidence preservation;
- final accepted source state;
- explicit G4 eligibility.

Do not commit. Do not start G4.
```
