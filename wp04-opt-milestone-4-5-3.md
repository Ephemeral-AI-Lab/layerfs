# WP4-M M4.5-3 — COMMIT reconciliation and failure provenance

- Verdict: **PASS for debug correctness and durability classification**.
- Release performance: **NotRun**.
- Decision: retain the shared publication/reconciliation path and advance only
  to exact accounting.  M4.5-5 remains blocked on M4.5-4.
- Scope: private benchmark `Store` shadow only.  This is not production
  `Engine` integration, promotion, profile selection, or a performance claim.

## Audit correction and publication invariant

The audited publication sequence is now one SQLite writer transaction:

```text
BEGIN IMMEDIATE
  -> authenticate the exact complete prior visible head in that snapshot
  -> establish and consume the transaction-owned same-open witness
  -> prepare and qualify the exact changed result
  -> stage exactly one complete visible-head row
  -> dispatch one COMMIT
```

The expected prior state is the complete tuple `(generation, root,
transition, validation_receipt)`, not only a parent/root pair.  Genesis uses
an insert-only statement.  Update predicates on every member of the complete
prior tuple and must change exactly one row.  The receipt binds store,
authority, epoch, profile, generation, root, and transition.  These rules
close same-generation/root ABA substitution, and the M4.5-2 mode-preserving
decoder/COW path remains unchanged.

All checked counter increments and authority-serial overflow checks needed by
the current publication path are computed before COMMIT dispatch.  A known
successful COMMIT is not changed into failure by later instrumentation.  The
only synthetic post-dispatch fault represents a missing acknowledgement and
is classified from independently visible durable state.

## Real COMMIT-error reconciliation

An actual SQLite COMMIT error is induced with SQLite's native commit hook.  It
uses the same production candidate branch as an ordinary COMMIT error; it is
not the synthetic after-success fault.  On every actual COMMIT error the code:

1. records the first failure before cleanup;
2. opens a fresh connection with `SQLITE_OPEN_READ_ONLY`;
3. performs no DDL and validates the database authority metadata and complete
   receipt-backed head;
4. classifies the observed state as requested-visible, prior-visible,
   different-head, or unknown/ambiguous;
5. preserves cleanup failure separately from the first failure;
6. clears the active writer-transaction identity and invalidates same-open
   authority; and
7. never issues a changed-spine witness during reconciliation.

The real rejected-COMMIT test observes `PriorVisible`, retains `Io` as the
dominant failure, leaves no visible head, and proves the same state after a
fresh reopen.  The classifier test independently exercises all four
post-dispatch outcomes:

| Fresh observation | Reconciliation | Dominant result |
|---|---|---|
| exact requested complete head | `RequestedVisible` | committed success |
| exact complete prior head | `PriorVisible` | original COMMIT error |
| another valid complete head | `DifferentHead` | `PublicationConflict` |
| unreadable/invalid authority or receipt | `Ambiguous` | `AmbiguousDurability` |

A pre-dispatch injected failure performs rollback and records
`NotAttempted`; it cannot be mistaken for a COMMIT ambiguity.  The first and
cleanup errors remain separate in `FailureProvenance`, while the reconciliation
outcome alone selects the dominant public result.  Object reads continue to
return `CandidateError::MissingObject(exact ObjectId)`; reconciliation does
not erase or relabel that provenance.

## Fingerprints and custody

| Item | SHA-256 / value |
|---|---|
| branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| cumulative tracked implementation diff | `f13a525c05623436062cec5c6b393c575fb616b606862a5ad1172003d46a556c` |
| benchmark source | `a19e64f7be4c57e12229de9331205cac2962c58d00f81a143e377408e5b73b42` |
| engine manifest | `f2f17cf5d302dfeaab12c4b1d0b6af660c229cd737c773f3a5d417dcb2eb1242` |
| debug executable | `0f4f721665340ab2555a5b16959173f37c8e91f07a4872099a1d1c81b97a60c3` |
| retained 100-MiB source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained fixture manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |
| independently prepared debug expectation | `81b2eaf5b5c0144fe945e2bd17228cee046e85ba021970eeeb0653cf8042a316` |

The tree remained dirty throughout.  Preflight reconfirmed branch, HEAD,
status, and no Cargo/rustc writer before Cargo.  No file in the sibling
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs` worktree was modified.  The sole
manifest change enables rusqlite's already-vendored `hooks` feature for the
real SQLite failure test; it adds no dependency, schema, database, WAL, VFS,
worker, pool, async path, or public API.

## Commands and debug correctness evidence

```text
git branch --show-current
git rev-parse HEAD
git status --short
pgrep -af 'cargo|rustc'

cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  actual_commit_error_uses_fresh_reconciliation -- --nocapture
  -> 1 passed; 0 failed

cargo test -p layerfs-engine --bin phase4_create_edit_benchmark
  -> 15 passed; 0 failed

cargo build -p layerfs-engine --bin phase4_create_edit_benchmark
  -> PASS (debug only)

target/debug/phase4_create_edit_benchmark --self-test \
  /tmp/layerfs-m45-m3-selftest.aHrrPZ
  -> PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
     objects=20; auth_bytes=1,054,836

cargo fmt --all -- --check
git diff --check
  -> PASS
```

The temporary self-test SQLite image, authority sidecar, and input were
removed after the test.  No release executable was built and no timing row was
collected.

## Equations, resource classifications, and bounds

Publication equations exercised by the tests are:

```text
writer transactions = 1
COMMIT dispatches    = 1 for every publication attempt reaching dispatch
visible-head rows    = 0 or 1
genesis changed rows = exactly 1 (INSERT only)
update changed rows  = exactly 1 under complete-prior-head predicate
requested generation = prior generation + 1 (or 1 for genesis)
```

For an actual rejected COMMIT:

```text
first_error      = Io
fresh_head       = exact prior head (None in the genesis test)
reconciliation   = PriorVisible
dominant_error   = Io
active_txn       = None
authority_serial = pre-dispatch serial + 1
```

Release edit latency, paired deltas/wins, CPU, RSS, Q, W, D, SQLite native
prepare counts, physical I/O/cache/sync/temp/journal observations, and storage
endpoint comparisons are **NotRun**.  Nothing is represented by an invented
zero.  Exact live Q and split SQL accounting remain an explicit M4.5-4 gate.

The algorithmic claims are unchanged:

```text
same-count mutation          O(Xb + Xc + K + F*H)
C1 qualification             O(K + F*H + A_delta + V_delta + H^2)
resident candidate memory    O(H + K + bounded pages/chunks/SQL/output)
C0 full closure              linear in the complete closure
first authority/full scrub   linear in the complete closure
fresh scrub/reconstruction   linear in the complete closure/source
+1                           suffix-linear
```

Reconciliation adds one bounded complete-head read on a fresh read-only
connection only after ambiguous COMMIT dispatch; it neither walks closure nor
mints authority.

## Defects and retain/revise/revert decision

The pre-audit implementation reconciled only synthetic after-success faults
and read through the writer connection.  It could not prove how a real COMMIT
error was classified and did not establish independent snapshot custody.
This milestone fixes that shared root path and tests the real SQLite failure.

Decision: **retain** the revised publication/reconciliation implementation.
M4.5-3 is complete for debug correctness.  Do not advance to release M4.5-5:
M4.5-4 must first replace the old max-local/fixed-envelope Q diagnostic,
separate SQL acquisition/query/execute/row counters, preserve higher-authority
W/D while adding named changed-work counters, and structurally parse campaign
JSON.
