# Handoff: Mid-M4.5 to Phase 4 Finalization

Status: restart-safe handoff captured while task
`codex://threads/01a014bc-45b6-7e31-825e-c92d57012124` is actively
implementing WP4-M M4.5. This note is an orientation and review contract. It
does not declare M4.5, WP4-M, profile selection, production integration, or
Phase 4 complete.

Repository scope: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty` only, branch
`codex/empty-worktree`. Never modify the sibling `layerfs` worktree. Preserve
the dirty tree and do not commit unless the user explicitly asks.

## 1. Why this midpoint matters

Phase 4 establishes the durable algorithm and identity foundation for later
LayerFS work:

```text
exact Phase-2 CDC
  -> canonical immutable CAS objects
  -> authenticated file/directory mappings
  -> Phase-3 root + delta semantics
  -> path-local copy-on-write
  -> one atomic durable publication
  -> fresh independent authentication and reconstruction
```

Errors here become persistent identity, compatibility, closure, durability, or
scaling errors. A fast benchmark with the wrong authority is worse than a slow
correct baseline because it can publish a head whose closure was never proven.
Conversely, repeatedly authenticating or reconstructing the entire snapshot
for a tiny edit defeats the purpose of COW even when the bytes are correct.

The project succeeds only when these are true at the same time:

- **CAS:** canonical bytes and immutable IDs are exact, incumbent reuse is
  authenticated, and duplicate encode/hash/copy/database work is removed.
- **CDC:** one bounded streaming pass preserves the frozen chunk sequence.
- **COW:** fixed-size small edits rewrite and qualify only the changed region
  and ancestor spine, while count-changing edits report their honest suffix
  cost.
- **SQLite:** one synchronous caller-thread transaction and one durable COMMIT
  publish the complete head without weakening closure, reconciliation, or
  typed failures.
- **Memory:** the semantic lane produces the same logical identities and acts
  as a shared-core performance ceiling; it is not presented as durable
  storage.
- **Resources:** CPU, live memory, RSS, metadata, database allocation, and
  physical-I/O observations are exact or explicitly `Unavailable`.

## 2. Evidence checkpoint at note creation

| Item | Current evidence |
|---|---|
| Branch | `codex/empty-worktree` |
| Documentation HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| Retained measured implementation checkpoint | `c96b5396e98db523b9a983df4ec80fdedfa971c1` plus dirty diff |
| Retained M3 diff SHA-256 | `e7d0940cd8457523d34de2bbfc5fac702124396826cda6f95b202439e05440eb` |
| Retained M3 release executable | `ff4f7206acbdff06bf9052550b3841e989f3cab603b509f9482c3d40b949213c` |
| Retained 100-MiB durable capture | `953.829 ms`, `104.841 MiB/s` |
| Retained 100-MiB complete lifecycle | `1,663.449 ms`, `60.116 MiB/s` |
| Qualification | `qualification=false`; no selection or promotion |

The benchmark was measured from a dirty tree. Never attribute these numbers to
a clean commit alone. Use the complete `(HEAD, diff hash, executable hash,
fixture hash)` evidence tuple.

At capture time, the active task had:

- M4.5-0 frozen-evidence custody recorded as PASS;
- M4.5-1 authority/witness code implemented and revised;
- a rolling ledger that had just labeled M4.5-1 PASS after revision, while the
  active task's latest commentary still said the full debug gate was being
  rerun before restoring PASS; and
- M4.5-2 through M4.5-6 still pending.

This transient ledger/commentary disagreement is not a defect by itself; the
other task was still running. A successor must inspect its terminal result,
final reports, exact test output, and final diff before accepting either label.
Do not continue editing the same files concurrently with the active task.

## 3. Non-negotiable algorithm and format contracts

### CDC chunking remains frozen

The Phase-2 FastCDC contract remains:

```text
minimum chunk = 8 KiB
target chunk  = 16 KiB
maximum chunk = 32 KiB
```

Do not change these sizes, the boundary algorithm, seed/domain, raw `ChunkId`,
or ordered CDC sequence to win a benchmark. File-page `K`, branch fanout `F`,
and directory page ceilings are mapping-profile candidates; they are not CDC
chunk-size authority.

### Identity and publication remain frozen

- Preserve canonical Bytes and Directory identity from Phase 1.
- Preserve raw chunk identity and CDC semantics from Phase 2.
- Preserve Phase-3 COW, workspace-root, delta, parent, and replay semantics.
- Use checked `u64` arithmetic and exact malformed-input rejection.
- CAS remains immutable and authenticated; no overwrite-on-conflict shortcut.
- Publish root, delta, receipt, generation, and authority tuple as one complete
  visible head in one SQLite transaction and one COMMIT.
- Reopen, full scrub, reconstruction, and range checks remain independent.
- Never resurrect append-only/pack storage, add a new database/WAL mode, or
  introduce workers, async, pools, VFS, public multi-profile APIs, or hidden
  source-sized staging.

## 4. What M4.5 must prove before review can pass

M4.5 recovers the valuable changed-spine algorithm from rejected M4 without
reusing M4's invalid cross-process receipt authority.

The accepted same-count edit path must have:

```text
mutation:
  O(changed CDC bytes + changed references + K + F*H)

pre-COMMIT qualification:
  O(K + F*H + changed/new authenticated closure + H^2)

resident LayerFS memory:
  O(H + K + F + bounded chunk/page/SQL/output buffers)

where H = O(log_F(reference_count / K)).
```

The bounded `H^2` ancestry check is initially acceptable. It may be replaced
only if counters prove it material; do not add a visited map or cache merely
because a lower asymptotic expression looks attractive.

M4.5 cannot pass until all of the following are directly evidenced:

1. A private, move-only, single-use same-open witness is issued only after a
   complete in-transaction authentication of the exact current head.
2. The witness binds transaction/open/store/authority/epoch/profile,
   generation, root, transition, and receipt; reopen, mutation, reuse,
   mismatch, rollback, and unresolved durability invalidate it.
3. Persisted receipt bytes alone never authorize cross-reopen skipping.
4. `C0` complete-closure and `C1` changed-spine controls differ only in the
   qualification algorithm.
5. Every new/different subtree and summary—including file-root mode—is fully
   authenticated; only witness-covered equal immutable edges may be skipped.
6. Before COMMIT, an independently prepared oracle binds the exact operation,
   edited source fingerprint, ordered CDC sequence, root, transition, and
   closure expectation.
7. Publication compares the complete prior head, rejects genesis overwrite
   and ABA, and cannot return a new fallible counter error after successful
   COMMIT dispatch.
8. Actual ambiguous COMMIT outcomes reconcile as requested/prior/different/
   unresolved through a fresh independent connection.
9. `MissingObject(ObjectId)` and all provenance remain exact.
10. Checked summed-live `Q` charges every simultaneously live owned capacity,
    returns to zero on every exit, and is not replaced by max-local buffers or
    RSS.
11. SQL acquisitions, native prepares, executions, rows, BLOB operations,
    W/D, CPU, RSS, and storage are labeled honestly.
12. Focused tests, full regressions, one release build, retained 100-MiB
    same-middle `C0/C1` A/B, milestone report, and rolling ledger all pass.

The rejected M4 result—about `2.195 ms` durable same-middle latency and a
`99.965%` pre-COMMIT reduction—is motivation and causal-direction evidence,
not accepted performance evidence.

## 5. Independent M4.5 review procedure

After the active task stops, the successor's first action is a read-only audit.
Do not immediately repair or benchmark. Freeze status, hashes, and command
outputs first, then launch at least these five independent review lanes:

| Lane | Required review |
|---|---|
| A — authority/publication | Same-open witness custody, transaction snapshot, complete-head compare, one use, invalidation, ABA/genesis, no cross-reopen authority |
| B — algorithm/closure | `C0/C1` semantic equivalence, changed/new subtree authentication, summary/mode checks, exact changed-spine counters, honest Big-O and no hidden full scan |
| C — durability/errors | Pre/post-dispatch failure provenance, real lost-ack reconciliation, exact `MissingObject` ID, cleanup, no post-COMMIT fallible relabeling |
| D — resources/counters | Summed-live Q, overlap tests, CPU/RSS, SQL/BLOB labels and equations, W/D, metadata and database/sidecar physical bytes |
| E — benchmark custody | Release/debug rejection, frozen fixture and prepared bases, isolated balanced `C0/C1`, timer equations, raw JSONL, paired statistics and no throughput mislabeling |

Reconcile the reviews into one ranked blocker list. Distinguish:

- an implementation bug;
- a benchmark-orchestration or counter bug;
- an invalid authority/correctness assumption; and
- a genuine algorithm limitation.

Then run only the narrow tests needed to reproduce findings. If any P0
authority, identity, closure, publication, delta, malformed-input, or exact-Q
gate fails, M4.5 is not accepted regardless of speed. Repair the smallest
shared cause, rerun affected tests and affected rows, update the M4.5 report,
and repeat read-only review. Do not silently edit a report from FAIL to PASS.

Only after all five lanes agree may M4.5-6 freeze terminal HEAD/diff/executable
and fixture hashes and stop for the user's promotion decision.

## 6. Resource policy, including controlled relaxation

The hard safety rules cannot be loosened:

- no `O(source bytes)` or `O(all references)` resident structure;
- no unbounded object map, cache, SQL statement, parameter list, or output;
- no unchecked length/count arithmetic;
- no skipped mandatory authentication or durability;
- no hidden second transaction/COMMIT;
- no memory improvement obtained by moving required state to uncontrolled
  persistent metadata.

Numeric constant limits may be relaxed later if the user wants more aggressive
speed, but only prospectively:

1. Predeclare the exact bounded buffer/batch change and absolute byte cap.
2. Preserve the same asymptotic memory class at 100 MiB, 512 MiB, and the
   analytical 100-GiB projection.
3. Add exact live-Q charges and an overlapping-allocation test before timing.
4. Run balanced A/B and report CPU, Q, external RSS, peak footprint, and
   database/journal allocation.
5. Require a material affected-phase gain and no correctness/storage drift.
6. If a previously frozen `<=5%` protected gate must change, amend the
   controlling experiment before running it; never loosen a gate after seeing
   an unfavorable result.

Good controlled tradeoffs are bounded—for example, a fixed leaf-sized query
or a fixed row-and-byte insertion group. A 1–4 MiB operation-local buffer may
be evaluated if evidence says crossings dominate; a 100-MiB reference or byte
cache may not. The exact cap must come from a preregistered milestone, not this
example.

CPU is a primary optimization resource, not merely a guard. Remove duplicate
hashing, encoding, decoding, copying, SQL crossings, and closure passes. Do not
remove a cryptographic check unless an equal or stronger transaction-local
proof covers the exact bytes and authority.

## 7. Metadata and storage-overhead policy

M4.5 and the first post-M4.5 optimizations should add **zero new serialized
metadata**. Their witnesses, receipts of work, and bounded batches are private,
transaction-local, and nonserializable.

The existing mapping metadata remains controlled by page/tree bounds:

```text
file leaves       = ceil(reference_count / K)
next level        = ceil(file_leaves / F)
subsequent levels = repeated ceil(previous_level / F)
total metadata    = exact canonical bytes of all leaves, branches, roots,
                    directory pages/indexes, delta and receipt
```

For every milestone, report separately:

- source/raw bytes;
- canonical chunk bytes;
- canonical mapping/root/delta/receipt metadata bytes;
- objects created/reused/authenticated;
- logical, apparent, and allocated SQLite main/journal/sidecar bytes; and
- `W` newly written canonical bytes and `D` duplicate/obsolete/unreachable
  bytes under the controlling definitions.

No optimization may add metadata proportional to source bytes beyond the
already frozen chunk-reference/mapping representation, duplicate source data,
or an unbounded durable index/cache. Any future serialized metadata proposal
requires a separate format/profile amendment, an exact overhead equation,
100/512-MiB measurements, an analytical 100-GiB projection, independent
goldens, and explicit user approval. It must not be smuggled into an
optimization milestone.

## 8. Post-M4.5 optimization program

M4.5 optimizes same-count edit qualification. It does not optimize full create.
After independent M4.5 acceptance, proceed one measurable variable at a time.

### F0 — Freeze the accepted M4.5 checkpoint

Freeze HEAD, complete dirty diff, source files, executable, fixture, prepared
bases, `C0/C1`, raw rows, reports, commands, environment, and toolchain hashes.
No source optimization belongs in F0.

### F1 — Make COMMIT and physical I/O observable

Separate COMMIT dispatch, acknowledgement, reconciliation, sync time where
available, page-cache/spill state, dirty pages, main/journal writes and bytes,
CPU, and endpoint allocation. Report unsupported observations as
`Unavailable`. Do not weaken `synchronous=FULL`, rollback-journal durability,
or one-COMMIT publication.

### F2 — Bounded transaction-local full-create construction proof

Carry forward proof already established while streaming source and inserting
or authenticating each object:

```text
authenticated chunks
  -> exact leaf summaries
  -> exact branch summaries
  -> mapping root
  -> workspace root + transition
  -> complete publication expectation
```

Keep the existing complete pre-COMMIT verifier as the shadow control until
adversarial agreement passes. Then remove only the duplicate database replay.
This removes one linear pass; full create remains `Theta(source bytes +
references)` and memory remains `O(K + F*H + bounded buffers)`.

### F3 — Bounded immutable CAS insertion groups

Only if SQL crossings dominate, group inserts under both fixed row and byte
caps inside the same transaction. Preserve per-object canonical validation,
duplicate-ID handling, conflict/reuse incumbent authentication, exact counters,
one transaction, and one COMMIT. This is a constant-factor crossing reduction,
not a sublinear full-create algorithm.

### F4 — Instrument the residual and optimize one cause

Break mapping into source read, CDC, raw hashing, canonical encoding, ObjectId
hashing, SQLite bind/execute/conflict work, and leaf/branch/root/delta encoding.
Choose only the largest observed residual. Do not bundle CDC buffering, CAS
batching, and hashing changes into one candidate.

### F5 — Reassess the retained 100-MiB target

Run release only, one warmup and five isolated balanced pairs, exact retained
fixture/prepared images, disjoint timers, raw JSONL, external observations,
and exact equations.

```text
primary durable target: <= 500.000 ms = >= 200 MiB/s
stretch durable target: <= 333.333 ms = >= 300 MiB/s
```

The retained baseline is `953.829 ms`; eliminating the retained
`388.155-ms` pre-COMMIT phase alone leaves an Amdahl planning floor near
`565.674 ms`. At least another `65.674 ms` must be removed to reach 200 MiB/s.
This is planning arithmetic, not a predicted benchmark result.

### F6 — Optimize complete lifecycle after durable capture stabilizes

Target fresh scrub and reconstruction with shared bounded walkers/batches.
Independent scrub remains linear in reachable authenticated bytes;
reconstruction remains linear in output bytes. Optimize SQL crossings, copies,
and redundant byte passes without hiding required work or weakening fresh
verification.

Each F milestone must preregister its hypothesis and counters, pass focused
correctness before timing, update its milestone report and rolling benchmark,
and receive a retain/revise/revert decision before the next starts.

## 9. Algorithm target table

| Operation | Required algorithm | Memory target | Performance interpretation |
|---|---|---|---|
| 100-MiB full create | `Theta(source bytes + references)`; one source/CDC pass, bounded mapping frontier, no duplicate closure replay | `O(K + F*H + bounded chunk/page/SQL buffers)` | Durable capture `<=500 ms` primary, `<=333.333 ms` stretch |
| Same-count small edit | Changed CDC region plus one leaf and ancestor spine; witness-covered changed-spine qualification | `O(H + K + F + bounded changed chunks/pages)` | Primary metric is edit latency and exact changed-work counters, never `100 MiB / edit wall` |
| Count-changing `+1` | Persisted-reference bounded streaming/spool with honest suffix rewrite | bounded independent of source size | `Theta(changed region + suffix)` under fixed ordinal mapping; no logarithmic claim |
| Range read | Authenticated routing plus returned bytes | bounded unless caller explicitly requests bounded output accumulation | Exact boundary bytes and latency/returned-byte rate |
| Fresh scrub | Complete reachable authenticated closure | bounded stack/page/batch state | Linear work is required; reduce crossings/copies only |
| Reconstruction | Complete ordered source output | bounded leaf/query/output window | Linear output lower bound; authenticate before exposure |
| Directory replacement | Authenticated descriptor routing, selected page rewrite, required flat wrapper/index work | bounded page/index state | Current flat index may remain linear; do not claim radix behavior |

`H`, `K`, and `F` must be derived from checked actual topology and counters.
Big-O proves scaling shape; only balanced release measurements prove throughput.

## 10. From accepted M4.5 to completed Phase 4

Do not jump directly from M4.5 to a promotion claim. The controlling sequence
is:

```text
M4.5 independent acceptance
  -> F0-F6 current-candidate optimization and honest retained-row evidence
  -> WP4-M required 100/512-MiB file + directory profile campaign
  -> WP4-P select one profile; delete losers/selectors; regenerate goldens
  -> WP5 finalize the one shared canonical mapping
  -> WP6 real Memory semantic/parity lane
  -> WP7 production SQLite integration of the promoted mapping
  -> WP8/WP9 fair unchanged-source Memory/SQLite baseline
  -> WP10 remove measured duplicate authentication/closure
  -> WP11 remove measured SQLite statement/transaction crossings
  -> WP12 remove measured shared encode/hash/copy/CDC/COW costs
  -> WP13 storage-backend compatibility audit
  -> WP14 stable-source final campaign and decision
```

Any pre-promotion optimization used in profile comparison must be
profile-neutral or applied equivalently to every candidate. Do not compare an
optimized K64/F64 arm with intentionally unoptimized challenger code and call
the result profile selection.

WP4-P must choose exactly one file K/F profile and one directory page ceiling,
delete alternatives and private selectors, regenerate independent frozen
goldens/IDs, and pass read-only audit before production integration.

WP6 Memory and WP7 SQLite have different roles:

```text
Memory = semantic parity + shared-core ceiling; durability NotApplicable
SQLite = authoritative durable engine + reopen/physical-allocation evidence
```

They should share CAS/CDC/COW semantics and produce identical logical IDs,
roots, deltas, closure order, reconstruction, and ranges. Memory cannot satisfy
the durable throughput target by itself and is not a shortcut replacement for
SQLite.

WP14 must select exactly one controlling outcome:

1. retain SQLite after the durable 100-MiB row reaches at least 200 MiB/s;
2. retain SQLite and continue shared-core optimization because Memory/SQLite
   evidence shows the remaining limit is engine-agnostic; or
3. authorize a **new specification** for one named third backend only when
   optimized SQLite still misses the target and directly measured
   SQLite-specific cost dominates.

Do not implement a speculative third engine during this handoff. A different
backend must still provide immutable authenticated CAS, exact ranges, bounded
operations, atomic root/delta publication, generation/snapshot identity,
durability acknowledgement, reconciliation, and typed failures.

## 11. Per-milestone validation contract

Before implementing each optimization, record:

- the one changed variable and code path;
- algorithm class and before/after bound;
- expected direct-counter equation;
- affected phase and minimum useful wall effect;
- CPU, exact-Q, RSS, metadata/storage and correctness gates; and
- control/candidate executable, sample count and decision rule.

Then:

1. Run exact focused correctness, identity, malformed/tamper, range, delta,
   closure, replay, transaction, and counter tests as applicable.
2. Freeze release control and candidate once.
3. Use one warmup and five adjacent balanced `AB/BA` isolated pairs.
4. Require at least a 5% affected-metric median gain and at least 4/5 wins for
   a material performance claim, plus the predicted counter movement.
5. Preserve raw rows, hashes, commands, medians, min/max/spread, paired deltas,
   CPU, Q, RSS, SQL/BLOB, W/D, and logical/apparent/allocated endpoint bytes.
6. Update the milestone report and rolling benchmark before continuing.
7. Retain, revise, revert, or mark inconclusive; do not stack an inconclusive
   change under the next experiment.

A deterministic work reduction below 5% wall may be retained only as an
explicit mechanism/constant-factor improvement when it adds no correctness,
complexity, memory, CPU, or storage cost. It is not a throughput PASS.

## 12. Required reading and handoff artifacts

Read controlling documents in their declared authority order before edits or
Cargo. At minimum, use:

- [M4.5 authority-correct specification](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/milestones/m4-5/spec.md)
- [Retained 100-MiB lifecycle diagram](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/f-series/planning/retained-100-mib-lifecycle.md)
- [Post-M4.5 optimization note](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/f-series/planning/read-after-m4-5.md)
- [Rolling optimization ledger](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/wp4m/progress.md)
- [Phase 4 implementation plan](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/rollback/implementation-plan.md)
- [Phase 4 algorithm specification](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/algorithm/spec.md)
- [Phase 4 algorithm test contract](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/algorithm/tests-and-benchmarks.md)
- [Phase 4 complexity analysis](/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/implementation-detail/phase-4/algorithm/complexity-analysis.md)

Before any post-M4.5 implementation, the successor must also read the active
task's final message, every `wp04-opt-milestone-4-5*.md` report, raw artifacts,
terminal dirty diff, and independent audit findings. If the task ends without
M4.5-6 and its final read-only audit checkpoint, resume M4.5 at the first
unproven milestone rather than starting F0 or Phase 4 promotion.

## 13. Definition of success

Phase 4 is successful when one prwhomoted canonical profile has:

- exact frozen CDC/CAS/COW/root/delta semantics and independent goldens;
- bounded-memory shared mapping and real Memory/SQLite semantic parity;
- production SQLite one-transaction/one-COMMIT durability and reconciliation;
- path-local same-count edits with exact changed-work counters;
- honest suffix/topology bounds for nonlocal edits;
- controlled metadata and physical storage growth;
- a stable-source benchmark whose timers and counters reconcile;
- at least 200 MiB/s durable 100-MiB capture, or the controlling evidence-based
  SQLite/shared-core decision required by WP14; and
- no rejected profile, selector, shadow engine claim, append-only/pack path,
  speculative backend, or unbounded optimization state left behind.

Speed, safety, and canonical correctness are coequal. None may be traded away
silently to make the other two look better.
