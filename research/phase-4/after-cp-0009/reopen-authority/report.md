# Cross-process reopen authority after CP-0009

Status: adversary-driven research; no implementation or benchmark authority  
Date: 2026-08-21  
Scope: current K64/F64 + DIR256K product profile; WP4-P remains COMPLETE and closed

## Executive answer

`Observed(source/evidence)`: CP-0009's fresh-process reopen/head-ready boundary
is **3.007750 ms** at 100 MiB. It authenticates the fixed-size store/head/receipt
state; it does not establish authority over the complete file closure. The
separate same-open authority establishment is **245.330416 ms**, and the
subsequent same-count durable edit is **9.737250 ms**. The actual first edit
after reopen is therefore **255.067666 ms** by direct addition. CP-0008 measures
the same independent authority cost at **1,235.301 / 1,213.208 ms** for the
500-MiB early/middle arms, before their **27.140916 / 15.102042 ms** publication
work.

`Observed(source/evidence)`: the current receipt authenticates a logical tuple
containing store, validation-authority, integrity-epoch, generation, root,
transition, and profile. The CP-0009 benchmark-private authority additionally
binds an open identity, transaction identity, authority serial, and mutation
serial. That same-open authority is deliberately discarded on reopen. The
production `Engine` still opens schema version 1 and has no integrated store
authority, integrity epoch, complete visible-head receipt, or cross-process
permit.

`Derived(equation)`: a persisted receipt proves that a named closure was valid
when the receipt was issued. It cannot prove that the database is the freshest
copy, that a copied sidecar is unique, or that skipped object bytes have not
been deleted, replaced, restored, or corrupted since validation. A database
and sidecar can be copied bit-for-bit; every local O(1) observation is then
identical in the original and the copy. If one unvisited object in the copy is
mutated, a sublinear verifier that does not read it still sees the same receipt
and head. Therefore no authority composed only of copyable database/sidecar
state can provide adversarial cross-process closure authority.

`Hypothesis(test needed)`: sound sublinear reopen is possible in principle only
if LayerFS adds a non-replayable authority outside the database/sidecar rollback
domain, binds it to one fenced writer and the exact logical head/profile, and
either (a) provides an immutable authenticated storage snapshot or (b)
completely mediates every object-invalidating physical mutation with a trusted
epoch. A crash-safe prepared/committed authority protocol is also required.
No such provider, mutation boundary, or recovery protocol exists in the
current product.

The evidence is sufficient to retain the complete first-edit scrub. It is not
safe to implement or benchmark a scrub bypass.

## Evidence method and controlling scope

This report uses only these labels:

- `Observed(source/evidence)`: a value or behavior directly present in the
  accepted checkpoint, specification, current source, or an authoritative
  upstream description;
- `Derived(equation)`: a conclusion whose premises and equation are stated;
- `Hypothesis(test needed)`: a prospective mechanism requiring a separate
  proof before implementation or timing; and
- `Unavailable(reason/source)`: information that the current authority or
  evidence does not provide.

The current-product control is [CP-0009](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline.md),
with its [machine-readable analysis](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0009-dirty-b073a7e04c7a-current-product-baseline-analysis.json).
The affected-operation scale authority is [CP-0008](../../../../implementation-detail/phase-4/test-checkpoint-report/cp-0008-dirty-4f1c97f81f7c-count-change-scale.md).
F2/F4 remain historical optimization and attribution evidence; they do not
replace CP-0009. The current [decision map](../../decision-map.md) and
[hypothesis ledger](../../foundations/hypothesis-ledger.md) are routing context,
not evidence. WP4-P is complete, K64/F64 + DIR256K remains the product profile,
and no profile or count-changing decision is reopened here.

No build, test, filesystem experiment, SQLite invocation, benchmark, or scrub
bypass was run for this report.

## 1. The two boundaries must remain separate

| Boundary | Size / operation | Result | Evidence meaning |
|---|---:|---:|---|
| Fresh-process reopen/head ready | 100 MiB control | **3.007750 ms** median | `Observed(source/evidence)`: process launch excluded; fixed-size head/receipt readiness, not complete closure authority |
| Same-open authority establishment | 100 MiB same-count | **245.330416 ms** median | `Observed(source/evidence)`: complete authenticated closure traversal before the edit permit exists |
| Same-open durable edit after authority | 100 MiB same-count | **9.737250 ms** median | `Observed(source/evidence)`: path-local edit/publication boundary after authority |
| First edit after reopen | 100 MiB same-count | **255.067666 ms** | `Derived(equation)`: `245.330416 + 9.737250` |
| Authority, early / middle | 100 MiB count change | **240.164 / 240.711 ms** median | `Observed(source/evidence)`: CP-0008 independent authority lane |
| Authority, early / middle | 500 MiB count change | **1,235.301 / 1,213.208 ms** median | `Observed(source/evidence)`: approximately linear complete-closure lane |
| Publication, early / middle | 500 MiB count change | **27.140916 / 15.102042 ms** median | `Observed(source/evidence)`: separately accepted suffix-linear publication lane |
| First after reopen, early / middle | 500 MiB count change | **1,262.771917 / 1,228.564417 ms** median | `Observed(source/evidence)`: row-wise authority plus publication |

`Derived(equation)`: the 100-to-500-MiB authority ratios are **5.144x early**
and **5.040x middle**, while file size grows 5x. This is the measured complete
authenticated-closure cost identified by the [complexity analysis](../../../../implementation-detail/phase-4/algorithm/complexity-analysis.md),
not reopen bookkeeping. Removing it requires a proof of continued authority,
not a faster head query.

The [algorithm specification](../../../../implementation-detail/phase-4/algorithm/spec.md)
already names fast reopen and fresh scrub as different operations. A receipt
can make the initial head/root work bounded and defer authentication to
accessed paths only under a valid store/epoch trust model. A fresh scrub
deliberately authenticates the complete closure. Replacing one row with the
other changes the security question unless equivalent authority is proved.

## 2. What the current code actually authorizes

### 2.1 Production receipt codec

`Observed(source/evidence)`: [the production receipt codec](../../../../crates/layerfs-core/src/validation.rs)
defines the exact 216-byte `ValidatedSnapshotReceiptV1`. Its keyed authenticator
binds:

```text
store_instance_id
validation_authority_id
integrity_epoch
head_generation
child_root_id
transition_id
mapping_profile_id
```

Decoding checks fixed framing and length, the keyed authenticator, the expected
authority, and the expected profile. This protects tuple integrity and detects
wrong keys or changed receipt fields.

`Derived(equation)`: receipt integrity is not receipt freshness:

```text
valid_mac(receipt, key)
  => receipt was created by key authority for the encoded tuple

valid_mac(receipt, key)
  != tuple is the newest tuple
  != skipped objects still equal their previously validated bytes
  != this database/sidecar copy is the unique live store
```

### 2.2 CP-0009 benchmark-private same-open authority

`Observed(source/evidence)`: [the current benchmark source](../../../../crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs)
implements a stronger but process-local authority chain:

1. `Store::begin` obtains `BEGIN IMMEDIATE` and creates a transaction identity.
2. `establish_same_open_file_witness` reads the exact head, authenticates the
   transition, runs `scrub_file` over the complete file closure, rechecks that
   the head did not change, and then issues a witness.
3. `SameOpenValidationWitness::consume` is single-use and checks open identity,
   transaction identity, authority serial, store ID, validation authority,
   epoch, profile, generation, root, transition, and byte-identical receipt.
4. The edit proof binds each authenticated `PutEvidence` to the same open,
   transaction, authority serial, and monotonically advancing mutation serial.
5. Successful proof consumption authorizes carry-forward. Publication first
   increments the authority serial, then carries only the exact requested head
   after acknowledged COMMIT or exact `RequestedVisible` reconciliation.
6. Rollback increments the authority serial and drops both transaction and
   carried authority. Failed rollback still clears them before SQLite cleanup
   is attempted. Reopen creates a new open identity and initializes carried
   authority to `None`.

The direct tests prove single use, tuple mismatch rejection, mutation/authority
invalidation, failed-rollback invalidation, zero-authentication carry into the
next same-open transaction, and loss of carry on reopen. This is why the
9.737-ms edit is safe after same-open authority and why receipt bytes alone do
not mint cross-reopen authority.

### 2.3 Production integration gap

`Observed(source/evidence)`: [production `Engine`](../../../../crates/layerfs-engine/src/lib.rs)
still declares schema version 1. `Engine::open` configures SQLite and validates
the provisional schema, whose store metadata contains one `visible_root`.
The `Engine` struct has a path, connection, counters, journal observation, and
SQLite profile; it has no `store_instance_id`, `validation_authority_id`,
`validation_key`, `integrity_epoch`, complete `VisibleHead`, receipt authority,
open fence, or cross-process permit.

`Observed(source/evidence)`: the benchmark-private `Store` persists IDs and an
epoch in its private `wp4m_meta`, reads the validation key from a sibling
`.authority` file, and validates the receipt on open. The epoch is initialized
and compared, but the current source contains no production repair/deletion
path that advances it. A database plus `.authority` file remains a copyable,
rollbackable pair.

The [visible-head migration specification](../../../../implementation-detail/phase-4/storage/sqlite/visible-head.md)
therefore keeps cross-reopen receipt reuse disabled until key custody and
mutation/epoch rules are proved. The [mapping specification](../../../../implementation-detail/phase-4/mapping/logical-persistence.md)
is explicit that the current SQLite profile does not authorize adversarial
cross-reopen reuse.

## 3. Required security property

Let `C(H)` be the complete strong-edge closure of exact visible head `H`, and
let `Valid(C(H), t)` mean every occurrence in that closure was present,
identity-authenticated, and semantically valid at time `t`. A sublinear reopen
permit accepted at later time `u` must justify:

```text
accept(permit, u)
  => exact_authoritative_head(u) = H
  && Valid(C(H), validation_time)
  && no closure-invalidating event occurred in (validation_time, u]
  && one fenced writer owns validation-through-use
```

The receipt proves the first historical validation claim only when its issuer
was sound. Exact head equality proves logical tuple equality at the current
read. The missing term is the universal negative over all closure-invalidating
events between validation and use.

`Derived(equation)`: that missing term can be established in only two general
ways without rereading `C(H)`:

1. bind `H` to a trusted immutable authenticated physical snapshot whose
   contents cannot change; or
2. trust a complete mutation mediator that observes every possible invalidating
   write and advances a non-replayable epoch before the write becomes visible.

The current mutable SQLite file on APFS plus a copyable sidecar supplies
neither property against the required adversaries.

## 4. Copy/replay impossibility for local-only authority

Consider a valid local state:

```text
S = database bytes || journal/sidecar endpoint state || authority sidecar
```

After a complete scrub, copy `S` byte-for-byte to `S'`. A reopened process
restricted to local database/sidecar observations sees the same store ID, key,
authority ID, epoch, profile, generation, head, receipt, schema, and durability
fields in both states. Any deterministic O(1) or sublinear acceptance function
that accepts `S` must accept `S'` when it reads the same subset.

Now change or delete one object in `S'` that the acceptance function does not
read. The persisted head and receipt remain byte-identical. The function still
accepts, but `C(H)` is no longer complete and valid.

```text
local_observations(S) = local_observations(S')
accept(S)              = accept(S')
Valid(C(H), S)         != Valid(C(H), S')
```

`Derived(equation)`: local-only acceptance is unsound under copy/restore plus
unvisited-byte mutation. Adding more copyable fields only enlarges `S`; it does
not break the indistinguishability. Path, inode, size, timestamp, receipt,
database change counter, and a database-local epoch are not non-replayable
authority unless a separately trusted layer makes them so.

SQLite's own documentation matches the physical boundary of this derivation:
database files are ordinary files and a rogue process can overwrite them in a
way SQLite cannot prevent ([How To Corrupt An SQLite Database File](https://www.sqlite.org/howtocorrupt.html)).
SQLite serializes cooperative writers, but that is transaction isolation, not
defense against raw-file mutation ([Isolation In SQLite](https://www.sqlite.org/isolation.html)).
`PRAGMA data_version` is local to one connection and is meaningful only when
two values from that same connection are compared; it is not cross-open
freshness authority ([SQLite PRAGMA documentation](https://www.sqlite.org/pragma.html#pragma_data_version)).

## 5. Adversary matrix

| Adversary / event | Current receipt and same-open behavior | What a sound cross-process permit would require | Finding |
|---|---|---|---|
| Copy database without the authority sidecar | `Observed(source/evidence)`: benchmark open cannot load the required key or the authority ID will not match. | No additional fast-path mechanism is needed; fail closed or scrub under separately established authority. | Current private path rejects; production `Engine` has no receipt integration. |
| Copy or restore database **and** sidecar together | `Observed(source/evidence)`: store ID, key, authority ID, epoch, head, and receipt remain mutually valid. | A non-copyable authority namespace plus anti-clone/anti-rollback freshness outside the copied set. | `Derived(equation)`: persisted receipt alone accepts the replay and is insufficient. |
| Restore database while retaining the same sidecar | `Observed(source/evidence)`: the long-lived key can authenticate old receipts; an old complete head can remain internally consistent. | External monotonic head/epoch state that cannot be restored with the database. | Database-local generation/epoch cannot prove freshness. |
| Restore a different sidecar alone | `Observed(source/evidence)`: wrong key/authority rejects. Restoring the same unchanged sidecar is observationally a no-op. | Key version and authority generation must be externally monotonic if rotations are meant to invalidate old state. | Key mismatch helps confusion resistance but does not solve whole-state replay. |
| Logical head or whole-database rollback | `Observed(source/evidence)`: exact tuple checking catches a stale receipt only when a newer authoritative tuple remains visible somewhere. Whole-state rollback removes that comparator. | Rollback-resistant external sequence plus exact external binding to the accepted head. | Local generation is not rollback resistance. |
| ABA (`A -> B -> A`) | `Observed(source/evidence)`: live publication increments generation with checked arithmetic, but restoring old state recreates the old generation/head/receipt exactly. | A non-repeating external version/fencing token; equality must include it. | Content/head equality alone cannot distinguish the replayed `A`. |
| Stale receipt against a newer live head | `Observed(source/evidence)`: current tuple comparison returns `InvalidValidationReceipt`. | Preserve exact tuple comparison. | Covered only when the newer head remains authoritative and visible. |
| Store/open/authority/profile/epoch confusion | `Observed(source/evidence)`: the receipt binds store/authority/epoch/profile/head; the same-open witness also binds open and transaction identities. | Cross-process open must replace process-local identity with a trusted fencing token while retaining every existing binding. | Logical confusion checks are necessary and already modeled; they are not freshness or mutation coverage. |
| Downgrade to an older schema/profile/binary | `Observed(source/evidence)`: current open checks schema and profile, but a previously deployed binary may not know a future external-authority rule. | A trusted minimum authority/protocol version and downgrade floor outside rollbackable store bytes. | Current profile checks do not prove that all writers enforce the future protocol. |
| Concurrent cooperative SQLite writer | `Observed(source/evidence)`: `BEGIN IMMEDIATE` serializes SQLite writers; the same-open scrub and permit live inside that writer transaction. | Cross-process authority acquisition must be fenced and ordered with `BEGIN IMMEDIATE`; loss of fence invalidates the permit. | SQLite covers cooperative connections, not clones or raw-file writers. |
| Concurrent rogue/raw-file writer | `Observed(source/evidence)`: SQLite documents that an arbitrary process can overwrite an ordinary database file and SQLite cannot defend against it. | Mandatory storage isolation or a trusted authenticated snapshot/mutation monitor; advisory/cooperative locking is insufficient. | Neither receipt nor scrub proves safety after a later rogue write. |
| External object-byte mutation or deletion after validation | `Observed(source/evidence)`: the receipt does not authenticate skipped siblings; accessed objects are rehashed and fail when touched. | Immutable snapshot semantics or complete mutation mediation that advances the trusted epoch before visibility. | A first edit that reuses an unvisited corrupted sibling has no current closure authority. |
| Corruption between validation and proof use | `Observed(source/evidence)`: same-open transaction/mutation serials cover engine-mediated changes; raw physical changes bypass them. | Validation-through-use fencing over both logical writer state and the physical storage authority. | This is a TOCTOU failure for any permit, including a newly reopened one. |
| Crash before COMMIT dispatch | `Observed(source/evidence)`: current transaction cleanup retains the prior head; reopen carries no process-local authority. | External prepared state must reconcile to prior and abort without ever issuing a permit. | Full scrub is safe fallback after ordinary SQLite recovery. |
| Crash or lost acknowledgement after COMMIT dispatch | `Observed(source/evidence)`: live code reconciles exact requested/prior/different/unavailable state with a fresh read while the retained request tuple and idempotency key remain in memory. | A durable trusted `Prepared(prior, requested, request_key, fence)` record that survives process loss and is reconciled before permit issuance. | Receipt/head alone cannot reconstruct retry ownership or safely classify every crash. |
| Ambiguous COMMIT with unavailable database | `Observed(source/evidence)`: current result is `AmbiguousDurability`; carry-forward is withheld unless exact requested visibility is established. | External authority remains non-accepting while prepared/ambiguous; only exact reconciliation may finalize. | Any fast permit during ambiguity is forbidden. |
| Legitimate online backup | `Observed(source/evidence)`: SQLite's Backup API creates a consistent snapshot, which can be bit-wise identical to an earlier source state ([SQLite Backup API](https://www.sqlite.org/backup.html)). | Restore must either create a new store/authority and scrub, or coordinate with external monotonic authority under an explicit rollback policy. | Consistency is not freshness; a valid backup remains a replay. |
| Raw backup/restore while a transaction or hot journal exists | `Observed(source/evidence)`: SQLite warns that mixed old/new copies and database/journal mispairing can corrupt the backup. | Use a SQLite-safe backup boundary first; then apply the separate authority/restore rule above. | A receipt cannot repair an inconsistent physical image. |

## 6. Minimum trusted state for any sublinear permit

The following is a necessary set, not an implementation proposal. Omitting any
row leaves at least one matrix adversary unresolved.

| Trusted element | Minimum binding / behavior | Why current local state is insufficient |
|---|---|---|
| Authority namespace and store identity | One non-clonable authority namespace bound to the exact `store_instance_id` | Database and sidecar copies preserve the current store ID. |
| Protected verification authority | Non-exportable key or external verifier with explicit rotation/version | A copied `.authority` file reproduces the current MAC authority. A key alone still signs old receipts and does not prove freshness. |
| Rollback-resistant version | Monotonic version/fencing token outside database, sidecar, backup, and restore domains | Database-local generation and epoch roll back with the database. |
| Exact logical binding | Store ID, validation authority/version, integrity epoch, schema/profile/durability version, generation, root, transition, receipt, and protocol version | The receipt covers most logical fields, but production open does not integrate them and none is independently fresh. |
| Physical continuity authority | Either an immutable authenticated snapshot handle bound above, or complete mediation of every deletion/replacement/repair/raw write with epoch advance before visibility | A logical receipt cannot detect a changed unvisited BLOB. |
| Writer fencing | One live fence token shared by authority acquisition, SQLite writer transaction, proof consumption, and publication | `BEGIN IMMEDIATE` fences cooperative SQLite writers only; it does not fence clones or raw-file mutation. |
| Crash/reconciliation record | Durable exact prior head, requested head, idempotency key, fence, and `Prepared`/`Clean` state in the non-replayable authority domain | Current exact request context is process-local and disappears on crash. |
| Downgrade floor | Minimum schema/profile/authority-protocol version enforced by the trusted authority | Rollback to an older binary/store can otherwise bypass new invalidation rules. |

A minimal abstract authority state would need at least:

```text
AuthorityState =
  Clean(version, fence, store, epoch, profile, physical_snapshot, head)
| Prepared(version + 1, fence, store, epoch, profile,
           physical_snapshot, prior_head, requested_head, request_key)
```

Only `Clean` with exact equality at every binding may mint a reopen permit.
`Prepared` is never fast-authority state. Recovery may change `Prepared` to
`Clean(prior)` only after proving the prior head authoritative, or to
`Clean(requested)` only after proving the exact requested head and request key.
A different or unavailable head remains conflict/ambiguous and cannot mint a
permit.

`Hypothesis(test needed)`: such a state machine could close rollback and crash
ambiguity if backed by a real linearizable, rollback-resistant provider.
`Unavailable(reason/source)`: no provider, physical snapshot binding, fencing
API, backup/restore policy, downgrade floor, or atomic recovery implementation
is selected or present in current LayerFS. The abstract state therefore does
not constitute authority ready for implementation or screening.

## 7. Mandatory invalidation events

Any future permit must be invalidated, or refused before issuance, on all of
the following:

1. receipt, head, generation, root, transition, store, authority, epoch,
   profile, schema, durability profile, or protocol-version mismatch;
2. validation-key rotation, authority unavailability, authority rollback, or
   downgrade-floor failure;
3. writer-fence change, lease expiry, concurrent-writer conflict, or inability
   to prove that the same fence spans validation through proof consumption;
4. rollback, failed rollback, aborted transaction, proof/witness reuse, or any
   mutation-serial discontinuity;
5. COMMIT dispatch until exact requested visibility is established; any
   `PriorVisible`, `DifferentHead`, or `Ambiguous` result withholds requested
   authority;
6. crash while external authority is `Prepared`, until exact recovery finishes;
7. any engine-authorized object deletion, replacement, repair, salvage, import,
   or other operation capable of invalidating a validated immutable object;
8. any detected raw-file mutation, physical snapshot mismatch, storage event
   gap, unsupported filesystem/locking mode, or loss of mutation mediation;
9. copy, clone, rollback, backup restore, database/journal/sidecar mispairing,
   or path alias unless a trusted authority explicitly rehomes it as a new
   store and requires a scrub;
10. generation/epoch regression, same-version different tuple, or any ABA
    observation;
11. corruption, absence, wrong identity, or wrong semantic role in any object
    fetched after permit issuance; and
12. any requested transition that cannot reproduce the exact retained prior,
    requested, and idempotency-key bindings.

`Derived(equation)`: if a future platform cannot reliably distinguish one of
these events from the unchanged case, it cannot safely issue a sublinear
permit for that threat model. The fallback is the full scrub or the precise
`ValidationAuthorityUnavailable`/corruption/conflict result.

## 8. Why a persisted receipt alone is not sufficient

The receipt is necessary because it provides unforgeable logical binding. It
is insufficient for four independent reasons:

1. **Replay:** MAC validity is invariant under copying or restoring the bytes.
2. **Freshness:** generation and epoch are monotonic only within one
   non-rolled-back history; restoring their storage restores the old values.
3. **Continued integrity:** the receipt contains no commitment to current
   physical SQLite pages and no proof that an unvisited object still exists.
4. **Exclusive use:** it contains no surviving writer fence or crash-recovery
   ownership for the next process.

Moving the same receipt to a separate sidecar does not change these results if
the sidecar is in the same copy/restore domain. Keeping the key in a protected
keystore prevents key copying, but old receipts under the same key still
verify after database rollback unless the keystore also supplies a
rollback-resistant current version/head. Adding an external counter detects
rollback but still does not detect an out-of-band mutation of an unvisited
object unless storage immutability or complete mutation mediation is also
trusted.

This matches the controlling [rollback specification](../../../../implementation-detail/phase-4/rollback/spec.md):
a snapshot receipt can cover an exact validated closure, but cannot
authenticate bytes fetched later or substitute for incumbent equality. It
also matches the [invariant matrix](../../foundations/invariant-matrix.md): CAS
reuse and skipped closure work require exact authority, while physical I/O and
storage integrity cannot be inferred from logical counters.

## 9. Benchmark prohibition and counter status

`Observed(source/evidence)`: CP-0009 and CP-0008 already provide the only
admissible current counters needed for this decision: approximately 3-ms
head readiness, 245.330-ms 100-MiB authority, and 1.21–1.24-s 500-MiB
authority. These quantify the removable wall only if a sound replacement
authority exists; they do not validate one.

`Unavailable(reason/source)`: there are no candidate direct counters for
external authority calls, trusted-state bytes, fencing transitions, prepared
recovery operations, mutation-coverage events, false accepts, invalidations,
or added durable writes because no complete authority model or implementation
exists.

The requested conditional rule therefore resolves to **no prospective
benchmark shape**. Do not implement, build, or time any first-edit scrub
bypass. In particular, do not compare “receipt accepted after reopen” against
CP-0009: that would time an unauthenticated omission, not an authority
mechanism. The [benchmark and evidence method](../../foundations/benchmark-and-evidence.md)
requires semantic equality before wall evidence; this candidate fails before
the benchmark stage.

Permissible follow-up is proof-only: select an actual non-replayable authority
and physical mutation boundary, specify the crash/restore/downgrade state
machine, and close every adversary row. Only a later report that does so may
select a ready-for-screen disposition and then define direct counters and a
prospective screen.

## 10. Disposition

`Derived(equation)`: a ready-for-screen disposition is rejected because the
current product has no non-replayable external freshness state, physical
continuity/mutation coverage, cross-process fence, or durable crash-recovery
record. The insufficient-authority disposition is also unnecessary because
the available evidence is sufficient to decide the current action: every
local-only model is defeated by copy/replay plus mutation of an unvisited
object. Retain the complete first-edit scrub and forbid a bypass benchmark
until the missing authority is specified and proved.

RETAIN_FULL_REOPEN_SCRUB
