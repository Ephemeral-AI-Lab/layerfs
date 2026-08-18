# WP4-M M4.5-1 — private same-open validation witness

- Verdict: **PASS after independent-audit revision**.  The original Store-open
  witness was insufficient; the retained witness is now owned by the active
  `BEGIN IMMEDIATE` transaction/snapshot and cannot be issued beforehand.
- Release performance: **NotRun**.
- Decision: retain the revised transaction-owned witness.  Release timing
  remains blocked on the later C0/C1 shadow, durability, and exact-accounting
  gates.
- Scope: private state in the benchmark `Store` shadow only.  The production
  `Engine` is unchanged and no public profile/receipt API was added.
- Labels remain `qualification=false`, `promotion=false`, and
  `rejection=false`.

## Fingerprints and custody

| Item | SHA-256 / value |
|---|---|
| branch | `codex/empty-worktree` |
| HEAD | `f3df30a80172131b74b5949a6a55234c962dac67` |
| cumulative tracked implementation diff | `a60a07079e4e4cc8fb12479dcb73b8d11d8839e41e5edf61ed166ab353910328` |
| benchmark source | `a2e25cb30c7b49aa73388979705ef6d10f7060c914dfe9a74108f1e0881f56d6` |
| shared file-root decoder | `1e1803250fe91493c26844c35ed20c5979c2d27a85b7411799da6606ed5b5d03` |
| parity test source | `2798b4973697e13deab8a45bfb1200118adc250d4568f6bac3b72450544ed47c` |
| debug self-test executable | `03e5b22c38e64e72ca0ed08ab47dc2e0cf2a2144ad3f4c2e733d66f3081d9f91` |
| retained 100-MiB source | `63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4` |
| retained fixture manifest | `8c64b5f49a10651e71fd52df3959cae22d291af4d95f47e43f7456308baad4ca` |

The cumulative diff includes the already-retained M3 work.  The audit revision
also changes the shared candidate file-root decoder to expose `mode` and its
parity callers so same-count COW can preserve nonzero mode instead of
hardcoding zero.  The production SQLite engine, schema, database profile,
receipt bytes, fixture, and frozen M3/M4 artifacts are unchanged.  No commit
was created and `/Users/yifanxu/Ephemeral-AI-Lab/layerfs` was not modified.

## Minimal implementation and authority model

The private nonserializable witness binds exactly:

```text
open identity
store instance ID
validation authority ID
integrity epoch
candidate profile ID
head generation
root ID
transition ID
exact 216-byte persisted receipt
same-open authority serial
single consumed bit
```

`open identity` is a checked process-local atomic sequence assigned on every
`Store::open`; it is never encoded or persisted.  The same-open authority
serial is also process-local and invalidates existing witnesses after an
engine-authorized authority mutation.  The existing persisted receipt remains
unchanged and cannot be decoded into a witness.

Issuance is routed through `establish_same_open_file_witness`, which reuses the
existing receipt decoder, transition verifier, and complete file walker in
this order:

```text
authenticate current complete head/receipt
  -> authenticate transition and its pages/child
  -> full scrub child file closure
  -> for a Change, full scrub the parent file closure
  -> reread and byte-compare the complete head
  -> issue private witness for that exact open and tuple
```

Consumption burns the witness before checking it.  Therefore an open mismatch,
tuple mismatch, authority mutation, or adversarial failed attempt cannot be
repaired and retried with the same witness.  A successful consumption moves
the exact binding into a private permit; the original witness cannot be reused.
The permit is not serialized and `covers` requires every bound field to equal
the current `Store` and complete head.

There is deliberately no constructor, encoder, decoder, schema field, public
method, cache, map, worker, pool, alternate database, or source-sized state.

## Direct tests and adversary cases

Commands and results:

```text
cargo fmt --all
cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  tests::same_open_witness_requires_full_scrub_and_is_exactly_single_use \
  -- --exact --nocapture
  -> 1 passed; 0 failed

cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  tests::publication_faults_record_reconciliation_and_require_private_authority \
  -- --exact --nocapture
  -> 1 passed; 0 failed

cargo test -p layerfs-engine --bin phase4_create_edit_benchmark
  -> 10 passed; 0 failed

cargo build -p layerfs-engine --bin phase4_create_edit_benchmark
  -> PASS, warning-free

target/debug/phase4_create_edit_benchmark --self-test <mktemp directory>
  -> self-test PASS; root f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
     objects=20; auth_bytes=1,054,836

cargo fmt --all -- --check
git diff --check
  -> PASS
```

The focused witness test directly proves:

- a same-open complete scrub issues a witness for the exact current tuple;
- the permit binds all ten semantic fields and the exact receipt bytes;
- a consumed witness cannot be reused;
- a witness issued before close cannot be consumed after reopen, even though
  the persisted receipt and head are byte-identical;
- store ID, authority, integrity epoch, profile, generation, root, and receipt
  mismatches fail closed and burn the witness;
- an explicit same-open authority mutation invalidates the witness; and
- deleting an unchanged mapping object before the scrub prevents issuance and
  leaves the prior visible head unchanged.

The transition-ID mismatch is covered by the same complete-head equality path
used for root/generation/receipt; M4.5-2 adds direct expected-result and
multi-change adversary cases around that permit.

## Counter and resource equations

The authority equation checked by both the focused test and debug self-test is:

```text
permit binding
  = witness(open, store, authority, epoch, profile,
            generation, root, transition, receipt[216], serial)
  = current Store/open + current complete VisibleHead
```

The single-use equation is exact:

```text
one issued witness -> at most one consume attempt -> at most one permit
second consume -> ValidationAuthorityUnavailable
```

No persistent byte is added:

```text
schema delta = 0 tables + 0 columns + 0 indexes
receipt delta = 0 bytes (still exactly 216)
authority-sidecar delta = 0 bytes
```

For M4.5-1, CPU, RSS, peak footprint, physical I/O, Q, W, D, SQL/BLOB campaign
deltas, and endpoint storage measurements are **NotRun**.  The debug self-test
counters are correctness diagnostics, not release performance.  The old
max-local Q is not promoted to exact status; M4.5-4 remains responsible for
summed live-capacity accounting.

## Before/after path and bounds

Before:

```text
reopen -> verify persisted receipt -> caller could mistake receipt for skip authority
```

After:

```text
reopen -> no witness
same-open full scrub -> private witness -> one consume attempt -> private permit
```

Witness creation itself is constant-size after the required scrub.  The
authority-establishment path remains `Theta(A + V)` because it intentionally
authenticates the full closure.  Consumption performs one bounded complete-head
read and constant-size comparisons: `O(1)` work and memory with respect to file
size.  Witness/permit resident state is constant (`O(1)`), contains no source
or closure vector, and has no persistent storage cost.  Fresh scrub,
reconstruction, complete lifecycle, and `+1` behavior are unchanged.

## Defects and retain/revise/revert decision

No M4.5-1 correctness, authority, identity, atomicity, or test defect remains.
The existing raw missing-row translation still does not retain the exact
`ObjectId`; this is explicitly deferred to the ordered M4.5-3 milestone and is
not used to claim the M4.5-1 exit beyond fail-closed issuance.

Independent audit correction: a Store-open witness can predate the writer
snapshot and therefore observe a different head than the later transaction.
The required sequence is now explicit:

```text
BEGIN IMMEDIATE
  -> read/authenticate exact complete prior head in that transaction
  -> full same-open scrub
  -> issue move-only transaction-owned witness
  -> prepare and verify result
  -> stage complete head
  -> one COMMIT
```

The original implementation called `establish_same_open_file_witness` before
`edit_file` started the transaction and violated this P0 invariant.  The
revision now starts `BEGIN IMMEDIATE` first, assigns a checked transaction
identity, performs head/receipt/transition/full-file authentication on that
same connection snapshot, issues at most one move-only witness for that
transaction, and consumes it before result qualification.  Both witness and
permit bind the transaction identity; neither implements `Clone` or a wire
codec.

The audit's adjacent publication corrections are also present in the shared
substrate: genesis is insert-only; an update conditionally matches the exact
prior generation, root, transition, and receipt and must change exactly one
row; root-only ABA attempts fail.  `parse_file_root` exposes mode, changed COW
re-emits that exact mode, and a direct test proves mode 1 survives rewriting.

Post-revision commands/results:

```text
cargo test -p layerfs-engine --bin phase4_create_edit_benchmark
  -> 13 passed; 0 failed
cargo test -p layerfs-engine --test phase4_engine_parity
  -> 12 passed; 0 failed
cargo build -p layerfs-engine --bin phase4_create_edit_benchmark
  -> PASS
target/debug/phase4_create_edit_benchmark --self-test <mktemp directory>
  -> PASS; root=f1cfdd7fdd658506caf39ace169dd83f1a95fbb25aafcbb7f9b72864f9d2e42a;
     objects=20; auth_bytes=1,054,836
cargo fmt --all -- --check
git diff --check
  -> PASS
```

The focused suite now explicitly rejects issuance without an active writer
transaction, binds the transaction ID, rejects close/reopen and reuse, and
proves complete-head ABA and duplicate-genesis failures.  M4.5-1 is therefore
**PASS after revision**.  Initial authority establishment remains full-closure
linear and separately timed.  Release performance remains **NotRun**; C0/C1
activation and release timing are still prohibited until the later audit gates
pass.
