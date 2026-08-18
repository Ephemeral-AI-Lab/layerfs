# WP4-M M4.5 independent read-only audit

## Final checkpoint-quality audit — 2026-08-19

- **PASS; ready for a separate F0 freeze.** No P0/P1 checkpoint blocker
  remains. The v3 terminal campaign and all historical sections below remain
  preserved.
- Authority wording: §13.5A now states the actual one-executable C0
  complete-closure versus C1 changed-spine comparison. It explicitly leaves
  the accepted v3 measured-spec hash, §13.3/§13.3A identities, raw rows, and
  timing unchanged; retained M3 is continuity evidence only.
- Deep topology: the new canonical K64/F64 fixture derives `H=2` from 262,145
  references without a source-sized buffer. It covers a leaf-boundary run,
  first leaf of a second inner branch, final partial leaf, and two root
  ancestor branches. Exact union/counters are four leaves, five branches, 376
  covered edges, 14 new/different edges, 11 prior/11 replacement spine
  objects, four authenticated new chunks, 43,488-byte C1 Q high-water, and
  terminal zero. C1 performs 34 SQL queries and zero complete-closure
  occurrences versus C0's 266,318/266,309. Both modes reject the deep
  malformed cumulative summary as typed `LengthMismatch`.
- Capacity portability: `from_exact_builder` rejects a returned `Vec` whose
  capacity exceeds its declared/precharged capacity even when the length
  matches. The regression proves typed `AllocationFailed` and Q cleanup. The
  96/256/64 semantic constants and serialized formats are unchanged.
- Validation: 98 tests pass; warnings-denied clippy, format, diff check, and
  debug self-test pass.
- Release custody: the release-path guard changed bytes, so one fresh v4
  campaign was required. Independent recomputation gives C0/C1
  `446.457042 -> 8.540708 ms` (`-98.087003%`, 5/5), exact Q 2,222,803, and
  12/12 byte-identical arm copies. RSS arm median is -0.175%; peak is +0.129%,
  so §13.6 does not trigger an extension. The 61-entry complete manifest
  verifies.
- V4 artifact root:
  `target/wp4m-m45-checkpoint-k64-20260819-v4/`; release SHA-256
  `7c395935457f99acfd3b02e08cdba976b971c61a407aeb65c243f2f26ddaf1a2`;
  complete-manifest SHA-256
  `1b1621735ad949abe4755e94dcd2487699af5502479dd99b707cc4d4a20e99c1`.
- Qualification, promotion, profile selection, production integration, F0
  source work, and later Phase 4 work remain not started.

## Final post-repair audit verdict — 2026-08-19

- **PASS.** A final read-only review of authority/publication, exact
  CDC/COW/closure, durability/provenance, logical-Q/resources, and benchmark
  custody/performance found no remaining P0 or P1 M4.5 acceptance blocker.
- The prospective §13.3A amendment is now the experiment authority. It
  preserves the old count-changing §13.3 row verbatim as withdrawn and
  freezes the exact XOR edit and its independently derived 5,284-reference
  sequence/root/transition/closure identities before accepted timing.
- The BEGIN ownership gap is closed; exact counter/identity values are
  precomputed before SQLite BEGIN. The overflow regression proves no live
  writer, exact typed failure, unchanged prior head, and reusable connection.
- Logical Q is pre-admitted across canonical, SQLite, decoded, CDC/range,
  delta, expectations, receipt, SQL, and output ownership. Its independently
  checked terminal high-water is 2,222,803 bytes and every exit returns to
  zero; the exact 1-GiB boundary is admitted and the next byte is rejected
  before allocation.
- The COMMIT matrix crosses the real dispatch boundary: prior-visible is a
  rejected COMMIT; requested-visible is a successful COMMIT followed by lost
  acknowledgement; different-head is a successful COMMIT followed by a
  valid complete successor; ambiguous makes the independent read genuinely
  unavailable. `Store::publish` and capture retain requested-visible first
  diagnostics without relabeling committed publication.
- All 96 tests pass, as do warnings-denied clippy, format, diff check, and the
  debug self-test. The terminal campaign's 12 official arm copies and 30
  memory-extension arm copies match their pair bases for database, authority,
  and expectations; all retained manifests verify.
- C0/C1 durable-edit medians are `440.023209 -> 9.134334 ms`
  (`-97.924124%`, 5/5). The 20-pair adjudication finds neither a >5% paired
  RSS/peak median nor the required 16/20 repeatable regression count.
- Accepted evidence:
  `wp04-opt-milestone-4-5-v3-terminal-benchmark.md` and
  `target/wp4m-m45-repair-k64-20260819-v3-terminal/`. Release executable
  SHA-256 is
  `f84e6b0f656e03ba3c537dbce08b085c3b52094a229b6df29593082e1d745ef1`;
  complete retained-tree manifest SHA-256 is
  `60887e2a4245fd3358f2242eac06b88e11051beacd3fc0bd0a2d7a7115f28cfd`.
- The earlier audit verdicts and candidate campaigns remain below unchanged
  as historical evidence. Qualification, promotion, profile selection,
  production integration, and later Phase 4 work remain not started. F0 may
  begin only as a separate next task.

## Historical second-audit verdict — 2026-08-19

- **FAIL / REVISE.** The repaired PASS below is withdrawn as an acceptance
  claim after the subsequent independent audit identified unresolved P0/P1
  authority, BEGIN ownership, exact-Q, real COMMIT-boundary, and publication
  diagnostic defects.
- The retained v2 campaign and its independently recomputed
  `443.143416 ms -> 9.000667 ms` result remain credible causal-direction
  evidence only. They may not be reused as acceptance evidence because they
  predate the prospective controlling-spec amendment.
- F0 and all later Phase 4 work remain blocked. A fresh v3 campaign and a new
  five-lane read-only audit are required after the shared root causes pass.

The repaired PASS audit below is preserved as a superseded historical record.

- Date: 2026-08-18
- Historical verdict: **FAIL / REVISE**. The original M4.5 checkpoint was not
  accepted and its evidence remains invalid.
- Repaired re-audit date: 2026-08-19.
- Repaired verdict: **PASS** for the private M4.5 same-count changed-spine
  milestone. Qualification, promotion, profile selection/rejection, and
  production integration remain false.
- Scope: terminal dirty tree on `codex/empty-worktree`; no Cargo command,
  benchmark, source edit, or sibling-worktree mutation occurred during review.
- Qualification, promotion, and rejection remain `false`.

## Repaired five-lane re-audit

The historical findings and frozen hashes below are preserved unchanged. This
section records the later repair re-audit and supersedes only the old terminal
disposition.

### Repaired terminal custody

| Item | Value |
|---|---|
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| Terminal tracked diff SHA-256 | `3dccd7e6bd2fd15e0bec21bb49f11647cc79e38a7b98fc9eb5c9e1591ee7c52f` |
| Terminal benchmark source SHA-256 | `c75282070e71611c69ffea6d1b35ea3cd0efbf80caa2f330cc01c8fa11dd553f` |
| Measured implementation diff SHA-256 | `0c8d70bc6aa5944f40ead21ffefb335457df251f7df8351bef02c04acda0ac1e` |
| Measured release executable SHA-256 | `37643a4eb99a0ab8fcbeaa326ebb2ceada98a9716c9dbe677c6f4a53e7320d02` |
| Raw JSONL SHA-256 | `be708e3ccd4a5b5ed16f53e816543a7c88ab303c370c1078c698b5ef2903a8a6` |
| Preflight SHA-256 | `e88bc7f241615f31b400a3ebc841e97eb0b6b9de81de0b2750bf8282d70e9912` |
| Structural summary SHA-256 | `22da16ef5eb6a5dd3e7103a10a647494161bbde3616027748e9511674c8bd887` |
| Fixture / manifest SHA-256 | `63b3695b...eff4` / `8c64b5f4...d4ca` |
| Expectation SHA-256 | `70520375af87d5227e28775a59879067d3b942cd82eb3f2fd2e15bb942b169ff` |

The measured diff and executable predate one terminal `cfg(test)`-only
COMMIT-error matrix addition. That addition is absent from release builds and
does not change the measured production binary. The exact measured patch and
binary are retained, while the terminal hash honestly covers the later tests
and controlling complexity record.

### Lane 1 — authority and publication: PASS

- `establish_same_open_file_witness` uses the exact singleton namespace
  resolver, validates transition plus complete child/prior closure, rereads
  the complete head, and issues authority only inside the active writer
  transaction.
- Witness/permit bindings cover open, transaction, store, authority, epoch,
  profile, generation, root, transition, 216-byte receipt, authority serial,
  and single use.
- Reopen, mismatch, mutation, reuse, publication, rollback failure, and
  unresolved durability invalidate authority. Persisted receipt bytes cannot
  create cross-reopen authority.
- Complete prior-head predicates, insert-only genesis, and direct ABA tests
  pass. No root-only or receipt-only comparison authorizes publication.

### Lane 2 — exact CDC, COW, and closure: PASS

- Exact full FastCDC proves the former uniform-`0x5a` operation has 5,283
  references and is count-changing. It is excluded rather than forced through
  same-count COW.
- The predeclared repaired `old_byte XOR 0x5a` operation has 5,284 references,
  exact source/sequence/root/transition/closure identities, and differs from
  the withdrawn old callback-substitution sequence.
- Local scanning starts at the authenticated predecessor and stops at the
  first two exact suffix confirmations. The retained rows inspect 143,709 CDC
  bytes and store five changed chunks, not the 1-MiB maximum window or the
  100-MiB source.
- Same-count COW rewrites only the affected ordinal leaves and ancestor union.
  Individual changed chunk lengths and intermediate cumulative ends may
  redistribute; final count and total length must match.
- C1 covers only exact equal child ObjectIds under authenticated paired
  parents, follows eight different edges, and fully authenticates five new
  objects / 103,363 bytes. The root, transition, receipt, closure, fresh scrub,
  reconstruction, and ranges agree with C0 and the independent oracle.

### Lane 3 — durability and errors: PASS

- The measured same-middle path has one transaction owner. Every failure while
  the transaction remains active records the exact first `FailureCause`,
  invalidates authority, attempts ordered rollback, and records cleanup
  separately.
- `MissingObject(ObjectId)` survives through provenance. Generic SQLite errors
  do not replace a more precise core/missing-object cause.
- COMMIT dispatch is counted before dispatch. Active authority is invalidated
  before fallible post-error reconciliation.
- Actual SQLite commit-hook errors exercise requested-visible, prior-visible,
  different-head, and ambiguous outcomes through the fresh independent
  read-only reconciliation function. Wrong requested keys classify ambiguous,
  not different.
- A successful or requested-visible COMMIT cannot be relabeled by later
  verification/JSON/counter work: postpublication failures retain the exact
  committed root, transition, and cause.

### Lane 4 — resources and accounting: PASS

- Exact logical Q is a checked sum of simultaneously live owned capacities,
  charged through scoped guards and capped at 1 GiB before admitted dynamic
  allocation. Fixed-size head/meta fields are stack arrays; mapping payloads
  and row BLOBs borrow where their lifetimes permit.
- Prepared expectations are capped at 128 KiB. CDC/rejoin, decoded page,
  ancestry, bounded SQL, range output, and range measurement owners are
  included. Parent decoded/canonical buffers are dropped before recursive
  changed-child descent.
- Real-path overlap and error tests, 1-GiB overflow, expectations overflow,
  and every measured row end at exact `q_current=0`. C0/C1 Q is exactly
  2,278,037 bytes.
- W and D are `Unavailable`; narrower write/auth/rewrite/CDC/SQL/BLOB/output
  counters retain exact labels. Native prepares, sync/fsync, page-cache bytes,
  peak journal/temp, and byte-level physical I/O remain `Unavailable`.
- Main/journal/authority apparent and allocated bytes are separately labeled.
  Endpoint storage is identical between arms.

### Lane 5 — benchmark custody and performance: PASS

- The versioned repaired directory retains the fixture, manifest, measured
  patch, executable, commands, environment, scripts, base images, expectation
  files, raw rows, preflight, external observations, summary, and hashes.
- Each pair was prepared once; `/bin/dd` physical byte copies supplied the
  same DB, authority, and expectation bytes to C0/C1. Preflight equality is
  12/12 for all three artifacts. No clone/reflink claim is made.
- All 12 rows are release `PASS`, one transaction/COMMIT, exact identities,
  exact timer equations, Q zero, and W/D Unavailable.
- C0 `443.143 ms` to C1 `9.001 ms` is `-97.969%` with 5/5 wins. CPU passes;
  RSS/peak arm medians remain below the 5% extension trigger; endpoint storage
  is identical.
- Same-open authority, post-COMMIT verification, same-open lifecycle, and
  first-open lifecycle remain separate. The 9.001-ms value is never presented
  as 100-MiB throughput or the complete authenticated lifecycle.

## Repaired audit decision

No P0 or P1 acceptance blocker remains in the five repaired lanes. The private
M4.5 changed-spine implementation is **retained and accepted for M4.5**.

F0 may begin as the next separate work item. This audit does not itself start
F0 and does not authorize production integration, final profile selection,
promotion, full-create optimization, the 198-row campaign, or later Phase 4
work.

## Frozen custody

| Item | Value |
|---|---|
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| terminal tracked diff SHA-256 | `b001f0088234b2bd03300890d4195ea0c28d167e6e677df32144fe24f13490a3` |
| measured release executable SHA-256 | `f0ba1c2423161cc2f79a0e7378408141eecfed30d4e65aceab3c8c667e5570af` |
| raw 78-row JSONL SHA-256 | `f6f1e698b7e50272cb993897c6ecff0c53fa4ba6bbd72c742f99de513f6e6165` |
| structural summary SHA-256 | `8ff8ad020e348904c3c89a539b1c299dfa6b718e87860ce9ecd8f0d14a84cce3` |
| prepared-base preflight SHA-256 | `6168b8be546b25a504321340bafd6e0b9659aa79bd5c55da9b6ef2ab069634a3` |

The raw rows and reported statistics remain preserved as nonqualifying
mechanism-direction evidence. They are not accepted M4.5 evidence.

## Reconciled blockers

### P0 — exact Phase-2 CDC was not executed for the edited stream

The measured mutation creates one replacement reference and swaps it into the
old ordinal. Expected observations and the purported full-rebuild oracle both
scan the original source and substitute bytes only after the old boundary has
already been emitted. Their agreement is circular and does not prove the
frozen edited CDC sequence or bounded exact rejoin. C1 also rejects different
children whose authenticated cumulative lengths legitimately redistribute.

Classification: implementation bug plus invalid correctness assumption and
evidence gap. The 99.435% row currently describes a synthetic one-reference
swap, not the specified durable same-count CDC edit.

### P0 — witness issuance did not authenticate the complete prior closure

`scrub_file` resolves the first entry named `file` without requiring the exact
singleton namespace role. A canonical namespace with a valid file edge and an
additional missing/corrupt strong edge can therefore reach witness issuance
without complete closure authentication. Later C1 checks may reject it, but
issuance itself is not authoritative.

Classification: implementation bug at the shared witness-establishment path.

### P0 — pre-COMMIT cleanup and provenance are incomplete

After `BEGIN IMMEDIATE`, most edit, oracle, closure, malformed-input, missing-
object, and counter failures propagate directly. They do not pass through one
owner that records the exact first cause, attempts ordered rollback, captures
`cleanup_first`, invalidates transaction authority on every exit, and releases
the writer. The current tests manually issue `ROLLBACK` around several cases.

Classification: implementation bug in transaction-attempt ownership.

### P0 — committed-success and reconciliation custody are incomplete

Fallible post-COMMIT counter/JSON work can make the child exit as a generic
failure without preserving that publication was already committed. Fresh
reconciliation collapses precise read/authority/receipt failures into a coarse
enum; an exact requested head with the wrong retained request key is
misclassified as a different head; and a COMMIT error reconciled as requested
visible can report zero COMMIT dispatches.

Classification: implementation and counter bugs. Only prior-visible was
tested through an actual SQLite COMMIT error; the remaining outcomes were
synthetic or direct classifier calls.

### P0 — exact logical Q was not established

The tracker charges only selected capacities after allocation, does not apply
the frozen 1-GiB pre-admission limit, and omits live prepared expectations,
mapping payload copies, returned head receipts, range results/measurements,
delta paths, and other owned buffers. `q_current=0` proves registered guards
balanced, not that every live allocation was registered. The claimed
`Q=48,133` is therefore not exact.

Classification: implementation/resource-accounting bug and missing real-path
overlap/overflow evidence. Recursive verification also retains parent page
pairs across depth, giving an unreported `O(F*H)` live shape.

### P0 — C0/C1 prepared inputs were not byte-identical

The campaign reran `--prepare-row` separately for each arm. Fresh store and
authority identities changed every time: zero of 20 measured C0/C1 pairs had
equal database hashes and zero of 20 had equal authority hashes, although all
expectation hashes matched. The frozen protocol requires copied byte-identical
database plus authority images so the qualification algorithm is the only
variable.

Classification: benchmark-orchestration bug. Existing timing remains useful
directional evidence but cannot carry a causal PASS.

### P1 — counters and evidence need revision

- W/D do not implement the governing cumulative work/output definitions.
- Reconciliation/open SQL and several BLOB values are uncounted.
- Same-middle source/CDC fields count constructed replacement bytes while CDC
  work is zero and still label CDC as nested.
- The expectation reader has no input-size or range-count cap.
- The first-open lifecycle equation is not emitted per row.
- Rows trust an executable hash supplied by the environment instead of
  comparing it with the running executable.
- Exact pre-edit images, measured-source patch, and summary-generation command
  were not retained.

## Required repair order

1. Add narrow failing regressions for exact edited-stream CDC/rejoin, complete
   witness closure, rollback invalidation/provenance, reconciliation key/cause/
   COMMIT accounting, and real-path Q admission/overlap.
2. Repair the smallest shared causes; do not alter frozen CDC/CAS identity,
   durability, receipt bytes, schema, or profile constants.
3. Rerun only affected focused tests, then the required all-target/clippy/fmt
   gates.
4. Build release once from the terminal validated source.
5. Run a new focused C0/C1 campaign from retained, copied, byte-identical
   database and authority inputs; preserve old rows separately.
6. Repeat all five independent read-only lanes. F0 remains blocked until they
   agree.
