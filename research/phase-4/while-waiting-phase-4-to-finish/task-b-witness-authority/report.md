# F2 whole-source witness authority under ordered authenticated canonical evidence

Status: research disposition only. This report authorizes no implementation,
format, profile, benchmark, migration, or production change. `Observed`,
`Derived`, `Hypothesis`, and `Unavailable` are used literally. The concurrent
WP4-M work had not begun any measured release row while the local evidence was
read; none of its partial code, setup failures, amendments, or artifacts is
evidence here.

## 1. Executive disposition

**Derived:** the private F2 `source_hasher` supplies no independently required
LayerFS publication authority once the proof has all of the following before
COMMIT:

1. a fixed, domain-separated, unambiguously framed ordered commitment over
   every `(raw_length, canonical_object_id)` occurrence;
2. complete canonical-byte authentication (or exact authenticated incumbent
   equality) before transaction-local `PutEvidence` is issued;
3. exact one-for-one binding of each committed tuple and its `PutEvidence` to
   the same ordered file-leaf occurrence;
4. checked count, total, K/F topology, root, workspace, transition, authority,
   profile, transaction, mutation-serial, and expected-head checks; and
5. one-shot proof consumption before the one publication COMMIT.

Under collision and second-preimage resistance of the 256-bit canonical
`ObjectId`, equality of that authenticated ordered tuple stream implies equality
of every canonically framed chunk payload and therefore equality of their
concatenated source bytes. The whole-source digest's unique operational feature
is different: it is a partition-independent comparison with an externally
declared fixture/source fingerprint. In the accepted F2 benchmark that is useful
custody and defense in depth, but it is not a canonical object, mapping field,
root, transition, receipt field, or independently established product
publication authority.

This conclusion does **not** say that the proposed canonical transcript can
blindly replace the current raw-ID transcript in the current 68-byte v1 file
reference. V1 still contains a separate `raw_id`. A wrong v1 `raw_id` is caught
pre-COMMIT by the current ordered `(length, raw_id)` golden but is caught by
neither the whole-source digest nor a canonical-ID-only transcript. A
canonical-only transcript is complete for the proposed single-canonical-ID
profile, where the raw-ID field does not exist; a v1 witness-only substitution
would still need an independently authenticated per-occurrence raw-ID binding.
That is a proposed-transcript scope condition, not a property supplied by the
whole-source digest.

## 2. Evidence boundary and controlling observations

- **Observed:** the accepted F2-v3 source has SHA-256
  `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
  The inspected sealed copy has that exact hash: [sealed accepted F2
  source](../../../../target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs).
- **Observed:** the sealed F4 raw artifact has SHA-256
  `5241b106a9d1d841e124d73ff247f2abadb2bf27759ef54d62a3ab3af3eb212f`.
  It is measurement custody, not a product content identity.
- **Observed:** `ObjectId = BLAKE3("layerfs/object\0" || complete_canonical_bytes)`.
  A canonical chunk is the exact Bytes object
  `LFSO || kind=Bytes || u32be(payload_length) || u32be(raw_length) || raw_bytes`.
  Identity is checked before canonical grammar is trusted. See the
  [canonical identity study](../../core/canonical/identity-and-hashing.md) and
  [single-identity study](../../core/canonical/v2-single-identity.md).
- **Observed:** current v1 leaves serialize
  `raw_id[32] || u32be(raw_length) || canonical_object_id[32]`. The committed
  codec at `d781173` fixes this at 68 bytes per occurrence.
- **Observed:** the private F2 construction path issues `PutEvidence` only after
  insertion or complete incumbent authentication/equality. Evidence carries
  canonical `object_id`, kind, canonical length, open identity, transaction
  identity, authority serial, and mutation serial.
- **Observed:** `ConstructionState::observe_chunk` checks that evidence against
  `reference.object_id`, `Bytes`, and exact canonical length; checks
  `bytes.len() == reference.raw_length`; updates both construction hashers; and
  then the caller appends the same copyable `FileReference` value to the
  `FileBuilder`.
- **Observed:** each leaf fold checks its observed occurrence count and raw
  total against the actual leaf; branch and root folds zip ordered child
  descriptors with ordered proof summaries and check IDs, levels, cumulative
  ends, counts, totals, and transaction identity. Workspace and Genesis
  transition puts are then evidenced and folded.
- **Observed:** one-shot consumption checks active transaction, issuance and
  prior consumption, open/store/validation-authority/epoch/profile/authority
  serial, final mutation serial, and an empty prior head. Only afterward are
  source/sequence/count/total/root/transition qualifications compared and the
  single publication COMMIT dispatched. See the [accepted F2 report](../../../../implementation-detail/phase-4/wp4m/f-series/f2/report.md).
- **Observed:** fresh post-COMMIT reopen, complete scrub, reconstruction, and
  ranges remain independent, linear verification phases. See the
  [full-create pipeline](../../core/pipeline/full-create-pipeline.md) and
  [verification/security study](../../assurance/verification-security-resources.md).
- **Unavailable:** a production LayerFS API contract in which a caller-supplied
  whole-source fingerprint is a public precondition. The inspected mechanism is
  private benchmark code; the [algorithm specification](../../../../implementation-detail/phase-4/algorithm/spec.md)
  does not serialize or publish a whole-source digest.

## 3. Exact current authority graph

The following is the accepted private F2-v3 graph. Solid arrows are current
pre-COMMIT authority; the final dashed portion is post-COMMIT reconciliation or
verification and cannot retroactively authorize publication.

```text
external retained fixture bytes
  -> out-of-timer raw fingerprint + CDC-sequence custody preflight

raw source bytes read inside capture
  -> frozen FastCDC 8/16/32-KiB boundaries
  -> for each ordered chunk occurrence i:
       raw_length_i = checked u32(len(raw_i))
       raw_id_i = BLAKE3("layerfs/object\0" || raw_i)
       canonical_i = LFSO/Bytes framing(raw_i)
       canonical_id_i = BLAKE3("layerfs/object\0" || canonical_i)
       immutable INSERT, or complete authenticated equal incumbent
       -> PutEvidence(canonical_id_i, Bytes, canonical_len_i,
                      open, transaction, authority_serial,
                      next mutation_serial)
       -> ConstructionState::observe_chunk(
            same FileReference(raw_id_i, raw_length_i, canonical_id_i),
            same raw_i, exact PutEvidence)
       -> current source_hasher += raw_i
       -> current sequence_hasher += u32be(raw_length_i) || raw_id_i
       -> same FileReference appended to current K leaf
  -> authenticated canonical leaf object + exact next PutEvidence
  -> ordered leaf proof(count, total, leaf ObjectId)
  -> authenticated canonical branches + exact next PutEvidence values
  -> ordered branch proof IDs, levels, cumulative ends, counts, totals
  -> authenticated file root + exact next PutEvidence
  -> file proof(expected count, total, root)
  -> singleton workspace Directory + exact next PutEvidence
  -> workspace root proof
  -> Genesis transition + exact next PutEvidence
  -> transition proof
  -> one-shot consume under live open/store/authority/epoch/profile/
     transaction/authority-serial/final-mutation-serial and empty head
  -> compare source fingerprint, raw sequence, count, total, root, transition
  -> stage complete head/receipt
  -> exactly one FULL+DELETE SQLite COMMIT
  -- post-COMMIT only --> fresh reopen and complete-head/receipt check
  -- post-COMMIT only --> fresh root-first full scrub
  -- post-COMMIT only --> reconstruction and exact ranges
```

**Derived:** the current proof's construction authority is not either digest in
isolation. It is the conjunction of authenticated puts, their serial order, the
same occurrence values entering the builder, exact topology summaries, live
scope, one-shot consumption, and head publication. The digests compare that
same-capture result to external benchmark expectations.

## 4. Four commitments that must remain distinct

### 4.1 Unambiguous definitions

Let `H(x)` be ordinary 32-byte BLAKE3; `OID(x) = H("layerfs/object\0" || x)`;
`raw_i` be the ith CDC chunk; `len_i = |raw_i|`; `RID_i = OID(raw_i)` under the
current alias; and `CID_i = OID(canonical_Bytes(raw_i))`.

Current commitments:

```text
F_fixture = H(complete external fixture bytes)

S_inner   = H(raw_0 || raw_1 || ... || raw_(n-1))

R_current = H((u32be(len_0) || RID_0) ||
              (u32be(len_1) || RID_1) || ... ||
              (u32be(len_(n-1)) || RID_(n-1)))
```

`R_current` has fixed 36-byte entries and is therefore entry-decodable, but it
uses ordinary unkeyed BLAKE3 without a purpose domain. Count and checked raw
total are separate qualification scalars.

For this report the proposed commitment is exactly:

```text
context = UTF-8(
  "LayerFS 2026-08-20 full-create ordered canonical evidence v1"
)

C_proposed = BLAKE3-DERIVE-KEY(
  context,
  (u32be(len_0) || CID_0) ||
  (u32be(len_1) || CID_1) || ... ||
  (u32be(len_(n-1)) || CID_(n-1))
)[0..32]
```

The context is hard-coded, globally unique, application-specific, and contains
no runtime value. Each entry is exactly 36 bytes because `CID` is fixed at 32
bytes; `u32be` is the canonical network-order length. Empty input is the empty
occurrence sequence. The proof must continue to compare the separate checked
`n` and `sum(len_i)` scalars, so an implementation cannot omit count/total work
while claiming transcript equality. BLAKE3's official derive-key mode provides
the distinct domain; fixed-width tuple encoding provides message framing. NIST
TupleHash is precedent that tuple elements must be encoded unambiguously; it is
not a proposal to replace LayerFS's hash primitive.

### 4.2 Distinction table

| Commitment | Exact input | Framing / domain | Producer | Consumer | Lifetime | Failure authority |
|---|---|---|---|---|---|---|
| External fixture hash | Complete retained fixture bytes read independently | Ordinary BLAKE3 over raw bytes; fixture name/size/manifest frame the custody record, not the digest message | Fixture preparation/preflight | Manifest checker, benchmark runner, auditors | Cross-process, cross-run retained custody | Aborts or invalidates the benchmark before an admissible row; never authorizes a LayerFS head |
| Inner `source_hasher` | Concatenation of raw bytes passed to `observe_chunk` in capture order | Ordinary BLAKE3, no LayerFS purpose domain | Transaction-local `ConstructionState` | `validate_full_create_qualification` against the decoded external expected fingerprint | Move-only transaction proof, then discarded | Current private code returns pre-COMMIT `PublicationConflict`; no durable identity is created |
| Current `sequence_hasher` | Repeated `u32be(raw_length) || raw_id[32]` | Fixed 36-byte entries; ordinary BLAKE3; no explicit purpose domain | Independent fixture CDC preflight and transaction-local `ConstructionState` | Pre-COMMIT qualification; manifest/row custody | External retained golden plus transaction proof | Detects wrong current raw-ID/length/order/count relative to the external golden; current mismatch is `PublicationConflict` |
| Proposed canonical commitment | Repeated `u32be(raw_length) || canonical_object_id[32]` | Fixed 36-byte entries under the exact BLAKE3 derive-key context above | Independent benchmark oracle when custody is needed; transaction proof only after exact canonical `PutEvidence` acceptance | Pre-COMMIT qualification together with count/total and graph proof; external manifest only for benchmark custody | Transaction-local product proof; optionally a separately named external golden | Must fail before COMMIT on transcript mismatch; it has no authority over store/open/epoch/profile/head by itself |

**Observed:** for the retained fixture, the first two commitments have equal
digest values when both read the identical complete bytes, but they are still
different evidence: different producers, failure boundaries, lifetimes, and
consumers. The external fixture hash remains necessary custody even if the
inner source digest is removed from product proof.

## 5. Adversary and failure matrix

Legend: `pre` = detects/rejects before publication; `post` = detects only after
COMMIT; `custody` = benchmark/fixture admissibility only; `cond` = only when an
independent expected transcript/value exists; `scope` = not applicable after
the proposed single-ID profile removes the named v1 field; `—` = no detection
authority. Columns are intentionally separate detecting layers.

| Adversary / failure case | Fixture custody `F` | Inner source `S` | Current raw sequence `R` | Proposed canonical sequence `C` | CAS + `PutEvidence` | Ordered K/F graph, root, transition | Scope, consume, head, COMMIT | Fresh post-COMMIT verification |
|---|---|---|---|---|---|---|---|---|
| CDC omits an occurrence before observation | custody (independent CDC count/sequence) | pre, cond on external raw fingerprint | pre, cond | pre, cond | — | pre through expected count/total | prevents publish after mismatch | post reconstruction mismatch |
| Occurrence is observed/evidenced but omitted before leaf append | — | — | — | — | serial proves the put, not placement | pre: observed leaf count/total differs from actual leaf | rollback/no COMMIT | post if wrongly published |
| CDC duplicates an occurrence | custody | pre, cond unless duplicated bytes leave identical full message (they do not for a true extra occurrence) | pre, cond | pre, cond | authenticates both puts but not desired multiplicity | pre through expected count/total | prevents publish after mismatch | post reconstruction mismatch |
| Downstream duplicate after observation | — | — | — | — unless the exact appended tuple also enters `C` | valid evidence alone is insufficient | pre when exact same tuple is jointly committed/appended; otherwise count/total gap | rollback/no COMMIT | post |
| Occurrences reordered before both digest and graph construction | custody | pre, cond when source byte order changes | pre, cond for unequal tuples | pre, cond for unequal tuples | each object may remain valid | graph is self-consistent but represents a different file | external qualification must reject | post against expected bytes/root |
| Graph occurrence reordered after transcript observation | — | — | — | — unless transcript and leaf share the exact value/ordering operation | valid objects do not reveal ordinal intent | **required pre binding**: same tuple must be serialized and folded; root alone only commits the wrong order | must not COMMIT without that binding | post is too late |
| Wrong raw length | custody sequence | — | pre, cond | pre, cond | canonical framing authenticates actual payload length but not a separate wrong reference length | pre: `observe_chunk` and checked totals/cumulative ends | no COMMIT | post `ChunkLengthMismatch`/length failure |
| Wrong v1 raw ID | custody sequence | — | pre, cond | —; `scope` in single-ID v2 | canonical evidence does not bind the separate v1 raw ID | current topology commits the wrong field without proving it equals raw bytes | current raw sequence must reject v1 before COMMIT | post `ChunkIdentityMismatch`; too late as sole authority |
| Wrong canonical ID for the raw bytes | — | — | — | pre, cond | pre: evidence ID must equal reference ID and canonical bytes must hash to it | pre through exact occurrence-to-leaf binding | no COMMIT | post `IdentityMismatch`/missing object |
| Different valid canonical object in the wrong occurrence | — | — if source hasher saw the intended bytes | maybe, if corresponding raw ID also changes | pre, cond for unequal tuples | object is valid, so CAS alone does not reject | required same-tuple occurrence binding; expected root also detects when independently known | no COMMIT after qualification failure | post expected-byte/root mismatch |
| Canonical kind mismatch | — | — | — | CID changes, cond | pre: evidence kind and authenticated incumbent kind must be `Bytes` | pre role checks in leaves/branches/workspace/transition | no COMMIT | post `WrongLogicalRole` |
| Canonical framing, declared length, trailing-byte, or mapping-tag mismatch | — | — | — | CID changes, cond | pre: identity first, then canonical grammar/length/role | pre structural codec/topology checks | no COMMIT | post identity/grammar failure |
| Unequal incumbent at same claimed ID | — | — | — | — | pre: complete incumbent identity, kind, stored length, and byte equality | — | rollback/no COMMIT | post defense in depth |
| Forged/tampered incumbent under claimed ID | — | — | — | — | pre `IdentityMismatch` unless a real hash collision | — | rollback/no COMMIT | post defense in depth |
| Stale `PutEvidence` | — | — | — | — | pre: next mutation serial and live scope mismatch | proof cannot fold it | no COMMIT | — |
| Duplicated `PutEvidence` | — | — | — | — | pre: second use carries the previous mutation serial | proof cannot fold it twice | no COMMIT | — |
| Skipped `PutEvidence` | — | — | — | — | pre: following evidence serial is greater than exactly next | missing occurrence/edge proof | no COMMIT | — |
| Reordered `PutEvidence` | — | — | — | — | pre: mutation serial is not exactly next | ordered proof fold fails | no COMMIT | — |
| Store mutation after evidence through the governed API | — | — | — | — | subsequent mutation increments serial | final proof serial no longer equals store serial | pre consume rejection | post defense in depth |
| Out-of-band object mutation after evidence but before COMMIT | — | — | — | — | not detected if it bypasses the mutation serial | not detected without a fresh pre-COMMIT read | **pre authority depends on transaction/single-writer isolation; later verification cannot excuse a reachable bypass** | post full scrub detects ordinary corruption |
| Wrong store instance | — | — | — | — | evidence/proof carries store identity indirectly through construction scope | — | pre `InvalidValidationReceipt` at consume; cross-store replay fails | — |
| Wrong open identity / reopen replay | — | — | — | — | evidence has open identity | — | pre `ValidationAuthorityUnavailable`/`InvalidValidationReceipt` | fresh open establishes its own authority |
| Wrong validation authority | — | — | — | — | construction scope carries authority ID/serial | — | pre `InvalidValidationReceipt` | receipt decode also rejects |
| Wrong integrity epoch | — | — | — | — | construction scope carries epoch | — | pre `InvalidValidationReceipt` | receipt decode/open rejects |
| Wrong mapping profile | — | — | — | a profile-specific expected transcript may differ but must not be relied on | construction scope carries profile | graph codec/profile checks | pre `InvalidValidationReceipt` | receipt binds profile |
| Wrong transaction identity/serial | — | — | — | — | evidence and node proofs carry transaction identity | every summary checks transaction identity | pre `ValidationAuthorityUnavailable` | — |
| Rollback before publication | — | — | — | — | proof/evidence becomes stale; authority is invalidated | no authoritative graph is published | prior head remains; no COMMIT success | fresh reopen sees prior head |
| Attempt to consume after COMMIT | — | — | — | — | active transaction is absent or scope serial changed | — | pre rejection; proof cannot authorize a second publication | — |
| Reopen then reuse old proof | — | — | — | — | open identity differs | — | pre rejection | fresh verification uses new state |
| Second proof consumption | — | — | — | — | — | — | pre: move-only `consumed` and transaction `construction_proof_consumed` flags | — |
| Cross-store proof replay | — | — | — | — | evidence/scope differs | — | pre store/authority/open mismatch | — |
| Incorrect K leaf fullness/partition | — | — | maybe count/total only | maybe count/total only | authenticates bytes, not canonical partition policy | pre `NonCanonicalPagePartition` and expected profile | no COMMIT | post codec/topology verifier |
| Incorrect F branch fan-out, level, or nonminimal height | — | — | — | — | authenticates bytes, not topology policy | pre partition/level/role checks | no COMMIT | post verifier |
| Wrong reference count | custody CDC count | — | separate expected count | separate expected count | — | pre file proof expected count | no COMMIT | post root/reconstruction count |
| Wrong total raw length | custody size | pre, cond when byte stream differs | tuple lengths and separate total | tuple lengths and separate total | canonical payload lengths are authenticated | pre checked leaf/branch/root cumulative totals | no COMMIT | post length/reconstruction |
| Wrong file/workspace root substituted for qualified root | — | — | — | — | referenced objects may be valid | pre proof derives file and workspace roots from exact folded edges | qualification/publish tuple mismatch; no COMMIT | post head/closure mismatch |
| Wrong transition, parent, child, kind, or operation | — | — | — | — | transition bytes are authenticated | pre exact Genesis transition fold and child edge | qualification/publish tuple mismatch; no COMMIT | post transition verification |
| Wrong expected head / concurrent publication | — | — | — | — | — | — | pre empty-head check and atomic publish conflict; ambiguous dispatch is freshly reconciled | fresh reopen distinguishes requested/prior/different/unknown |
| Collision in ordinary whole-source BLAKE3 only | custody may also collide if same primitive/message | cannot detect | `R`/`C` and graph may still distinguish | canonical path may distinguish | canonical auth remains authoritative if its messages do not collide | graph remains authoritative | normal publication rules | fresh verification cannot repair a collision in the authority it trusts |
| Collision/second preimage in canonical `ObjectId` | — | source digest may distinguish the raw messages | raw sequence may distinguish in v1 | cannot distinguish equal forged CIDs | cannot authenticate uniqueness under broken assumption | graph aliases the colliding object | scope checks do not help | fresh verification uses the same broken identity |
| Fixture bytes corrupted while frozen expectation remains correct | custody rejects before row | pre if capture is somehow attempted | pre | pre with independent canonical golden | product graph may still correctly represent the corrupted bytes | product graph is internally valid for those bytes | benchmark must not publish/admit the row | post expected-byte comparison rejects |
| Fixture expectation corrupted while fixture bytes remain correct | custody rejects when independent manifest/hash is retained | current private compare rejects correct capture | current private compare may reject | proposed external compare may reject | product canonical evidence remains correct | product root correctly represents actual bytes | benchmark is inadmissible; not a product-authority failure | product verification may pass actual bytes |
| Fixture and expectation maliciously changed together | custody chain is defeated, not the hash equation | matches attacker-selected fixture | matches attacker-selected sequence | matches attacker-selected sequence | authenticates the supplied bytes | publishes the supplied bytes correctly | cryptography cannot recover external intent | verifies supplied bytes, not historical intent |

### Matrix consequence

**Derived:** no row has “whole-source digest” as the only product layer that
prevents a bad head. It helps only when an independently expected complete raw
fingerprint exists and the corruption changes the byte stream fed to it. It
does not bind canonical IDs, raw IDs, evidence freshness, object ordinal,
topology, authority, transaction, or head. Conversely, later fresh verification
is mandatory defense and reconciliation, but it cannot be cited to justify a
head whose required pre-COMMIT occurrence/evidence binding was absent.

## 6. Product authority versus external benchmark custody

The external fixture fingerprint answers:

> Did this campaign consume the exact predeclared source artifact?

The product construction proof answers:

> Is the head about to be published the exact graph built from this operation's
> ordered, authenticated, transaction-local occurrences under this authority?

Those questions overlap on bytes but have different principals and failure
semantics.

- **Observed:** fixture generation and both raw/CDC preflights run outside the
  capture timer, are retained in a manifest, and can reject the campaign before
  any publication-bearing row.
- **Observed:** the private F2 path then repeats the expected raw fingerprint
  comparison inside the transaction. This is a benchmark qualification choice,
  not a serialized LayerFS identity.
- **Derived:** removing the inner repetition does not permit deleting the
  external fixture hash, manifest, source size, CDC golden, artifact hashes, or
  independent audit. They remain custody evidence regardless of product proof.
- **Derived:** corruption of the expected custody value is not repaired by
  making that same value a product precondition. If the expectation and fixture
  are changed together, all hashes can be internally consistent while external
  intent is lost.
- **Unavailable:** evidence that LayerFS users require an API operation
  “publish this file only if its raw whole-source digest equals X.” If such a
  public compare-and-publish contract is later specified, it is a distinct
  caller assertion and should not be inferred from this benchmark witness.

## 7. Equivalence argument and exact gaps

### 7.1 Cryptographic implication

Assume:

1. the proposed transcript encoding and domain are fixed exactly as section 4;
2. its expected digest is independently supplied when an external expected
   sequence is claimed;
3. every `CID_i` is accepted only after complete canonical-byte authentication
   or exact authenticated incumbent equality;
4. canonical Bytes encoding is injective and strictly decoded;
5. the tuple committed at ordinal `i` is the tuple serialized at ordinal `i` in
   the file graph and is covered by the exact transaction-local evidence;
6. count, total, topology, root, transition, scope, and head checks all pass
   before COMMIT; and
7. BLAKE3 has the required collision and second-preimage resistance for both
   the transcript and `ObjectId` messages.

Suppose two accepted constructions have the same `C_proposed`. Under the
transcript collision assumption, their fixed 36-byte entry streams are equal.
They therefore have the same occurrence count, and for every ordinal `i` the
same `u32be(len_i)` and `CID_i`. Under canonical `ObjectId` collision/second-
preimage resistance and injective Bytes framing, equal `CID_i` values identify
the same raw payload bytes. Concatenating equal payloads in equal ordinal order
gives equal complete source bytes. Thus authenticated ordered canonical
evidence implies the exact byte property that `S_inner` compares.

The converse is false. The same raw source can be split as two canonical chunks
`[a, b]` or one canonical chunk `[a || b]`; the whole-source digest is equal,
while the ordered canonical transcripts, reference counts, topology, and roots
are different. The source digest is therefore partition-independent but
strictly weaker about the CDC/mapping construction that LayerFS publishes.

### 7.2 Implementation-equivalence finding

**Observed:** the sealed path currently supplies the required occurrence link
without retaining an O(N) transcript: `push_bytes` constructs one local
`FileReference`; `observe_chunk` authenticates its canonical ID/evidence; the
same value is immediately passed to `push_reference`; leaf count/total is
compared; and ordered parent proof summaries match ordered descriptors. A
proposed `C_proposed` update at that exact observation boundary can therefore
refer to the canonical ID already accepted by `PutEvidence`.

**Hypothesis:** replacing the two current hasher updates with the exact
domain-separated canonical tuple update preserves that link. This is not test
coverage yet, and this report does not implement it.

Two gaps must not be hidden:

1. **Current-v1 raw-ID gap.** `C_proposed` does not authenticate the separate
   `raw_id` in a 68-byte v1 reference. `S_inner` does not authenticate it
   either. The current external raw sequence does. The smallest proof
   obligation for any v1 witness-only substitution is:

   ```text
   for every published v1 occurrence i,
   reference.raw_id_i == OID(exact raw bytes authenticated by CID_i)
   ```

   In the proposed single-canonical-ID profile this obligation disappears with
   the field, while legacy v1 readers retain their existing raw-ID validation.

2. **Out-of-band pre-COMMIT mutation gap.** Mutation serials cover governed
   Store calls. If another actor can alter an evidenced row inside the
   publication transaction without incrementing that serial, only later scrub
   may notice. Neither source digest solves this. Pre-COMMIT authority therefore
   depends on the current one-writer/transaction isolation boundary or on an
   independently specified fresh check; post-COMMIT discovery cannot be used as
   publication authority.

Neither gap gives the whole-source digest a unique required property.

### 7.3 Cryptographic versus implementation versus tests

| Claim class | Result |
|---|---|
| Cryptographic equivalence | **Derived:** authenticated ordered canonical tuples imply exact concatenated source bytes under the stated assumptions; equality of whole-source hashes alone does not imply the tuple stream |
| Current implementation linkage | **Observed:** the sealed F2 path uses the same local reference across evidence observation and builder append and folds exact ordered topology; v1 raw ID remains separately protected by the current raw sequence golden |
| Proposed implementation | **Hypothesis:** the exact derive-key transcript can replace only the inner complete-source comparison while retaining every other pre-COMMIT binding |
| Focused test coverage for the proposal | **Unavailable:** no implementation or test was written or run in this task |

## 8. Error precedence and typed-error consequences

The digest decision cannot flatten typed failures into “hash mismatch.” Current
precedence and consequences are:

1. **Canonical authenticity before grammar.** `validate_identity` recomputes
   `ObjectId` before decoding. Tampered malformed bytes therefore return
   `IdentityMismatch` before `WrongLogicalRole`, `UnexpectedEof`,
   `TrailingBytes`, or a mapping-tag/version error.
2. **Authenticated incumbent detail.** After identity succeeds, wrong stored or
   decoded kind is `WrongLogicalRole`; wrong stored canonical length is
   `LengthMismatch`; authenticated but byte-unequal incumbent is
   `IdentityMismatch`.
3. **Evidence order/scope.** Wrong object/kind/length/open/transaction/
   authority/next-mutation evidence currently collapses to
   `ValidationAuthorityUnavailable` in `accept_put`. Broader proof scope
   mismatch at consume is `InvalidValidationReceipt`.
4. **Reference length.** The sealed `observe_chunk` currently returns
   `ChunkIdentityMismatch` when `bytes.len()` differs from
   `reference.raw_length`, while post-COMMIT mapping reconstruction has the
   separate `ChunkLengthMismatch`. A later change must freeze or deliberately
   version this existing asymmetry; this report does not rename it.
5. **Topology.** Empty/nonfull/nonminimal/unequal partition failures use
   `NonCanonicalPagePartition`; ordering uses `NonCanonicalOrdering`; role and
   level confusion use `WrongLogicalRole` or the exact mapping-depth error;
   cumulative totals use `LengthMismatch`.
6. **Qualification.** The current compound source/sequence/count/total/root/
   transition comparison returns `PublicationConflict`. Removing the source
   operand must not change the first error for an independent canonical
   transcript, count, total, root, or transition mismatch.
7. **One-shot/head/publication.** Second use or missing live transaction is
   `ValidationAuthorityUnavailable`; a nonempty unexpected head is
   `PublicationConflict`. After dispatch, requested/prior/different/unknown
   authority is reconciled. `PublicationConflict` and `AmbiguousDurability`
   may dominate only under that lifecycle rule.
8. **Post-COMMIT failures.** The accepted harness wraps later verification
   failure as committed-publication failure with the committed root/transition.
   It must never be reported as though publication had been rejected before
   COMMIT.

**Derived consequence:** a v1 wrong-raw-ID case cannot be allowed to move from
current pre-COMMIT `PublicationConflict` to post-COMMIT
`ChunkIdentityMismatch` merely because `C_proposed` authenticates canonical
objects. That would weaken publication even though the source digest decision
itself is sound.

## 9. Focused tests required for a later implementation

No implementation is made here. A later change would require, at minimum, the
following focused tests before any performance row:

1. **Fixed transcript vectors:** empty, one entry, two unequal entries,
   repeated equal entries, maximum legal chunk length, and multi-update
   fragmentation produce exact frozen `C_proposed` bytes under the exact
   context string; little-endian, native-endian, missing-length, added-NUL, and
   ordinary-BLAKE3 variants differ.
2. **Independent oracle equality:** an out-of-operation oracle and the
   transaction-local streaming construction produce the same canonical
   sequence, count, and total for the retained fixture.
3. **Occurrence adversaries:** individually omit, duplicate, and reorder first,
   middle, final, repeated-equal, cross-leaf, and cross-branch occurrences;
   change one raw length; change one canonical ID; and place a different valid
   object at one ordinal. Every non-equivalent case must fail before COMMIT.
4. **Transcript-to-leaf binding:** mutate a leaf reference after transcript
   observation but before fold, and mutate transcript observation while leaving
   the leaf unchanged. Both directions must fail pre-COMMIT, proving the
   implementation link rather than merely two internally valid digests.
5. **V1 raw-ID counterexample:** retain correct raw bytes, length, canonical ID,
   and canonical transcript but inject a wrong v1 raw ID. The current profile
   must still fail before COMMIT; the test must not rely on post-COMMIT scrub.
   The single-ID profile separately proves that no raw-ID field remains.
6. **Canonical authentication:** wrong kind, wrong outer kind, wrong payload
   length, trailing bytes, wrong mapping tag/version, claimed canonical ID,
   forged incumbent, equal incumbent, and byte-unequal incumbent preserve
   identity-first typed precedence.
7. **Evidence lifecycle:** stale, duplicated, skipped, and reordered
   `PutEvidence`; unrelated intervening put; mutation after evidence; wrong
   open/store/authority/epoch/profile/transaction; rollback; COMMIT; reopen;
   second consumption; and cross-store replay all fail with current typed
   authority errors and terminal resource zero.
8. **Topology boundaries:** empty, one, exact K, K+1, exact K*F, K*F+1,
   temporary unary collapse, H=2, wrong child/order/level/cumulative end,
   wrong count/total, wrong file/workspace root, wrong transition, and wrong
   expected head all fail at the protected layer.
9. **External custody separation:** corrupted fixture with frozen expectation,
   corrupted raw expectation with correct fixture, corrupted canonical-sequence
   expectation, and optional absence of an external product golden demonstrate
   that benchmark admissibility and product graph authority are not conflated.
10. **Pre/post-COMMIT boundary:** every pre-COMMIT injection leaves the prior
    head authoritative; requested-visible, prior-visible, different-head, and
    ambiguous dispatch outcomes retain exact reconciliation provenance; fresh
    verification failures remain explicitly committed failures.
11. **Shadow equivalence:** current accepted proof and the later candidate agree
    on canonical objects/bytes, reference count/total, K/F topology, root,
    workspace, transition, closure, one transaction/COMMIT, post-COMMIT bytes,
    ranges, and every unaffected typed error. Digest values are intentionally
    different and must be separately named/versioned.
12. **Collision-boundary harness:** where dependency injection is already
    available, a deliberately colliding fake identifier must show which claims
    cease to hold. No real-collision test or weaker production hash is implied.

These are focused proof tests, not authorization for a new broad framework or
campaign.

## 10. Honest performance interpretation

- **Observed:** the sealed F4 source+sequence construction lane median is
  `89.067215 ms`.
- **Observed:** that interval contains the complete raw-source
  `source_hasher.update(raw)` work **and** the ordered
  `u32be(length) || raw_id` sequence updates and their surrounding measurement.
- **Unavailable:** the source-only wall, the current sequence-only wall, the
  exact wall of the proposed canonical sequence, and the net durable saving.
- **Derived:** the entire `89.067215 ms` is not removable. An ordered
  replacement remains mandatory, and its isolated wall is unknown.
- **Derived:** replacing `raw_id` with the already computed canonical ID changes
  only the small per-occurrence transcript input, not the need for ordered
  accumulation, count/total checks, canonical ID hashing, canonical encoding,
  CAS authentication, topology construction, COMMIT, or fresh verification.
- **Derived:** `89.067215 ms` is a gross combined ceiling, not a forecast and
  not a number that may be subtracted from an accepted durable row as achieved
  speedup.

The [complexity analysis](../../../../implementation-detail/phase-4/algorithm/complexity-analysis.md)
therefore remains accurate: full create stays `Theta(source bytes +
references)`, bounded proof memory stays `O(K + F*(H+1) + bounded buffers)`,
and ordered replacement work stays mandatory.

## 11. Limitations and collision assumptions

1. **Private benchmark boundary.** The accepted proof is private benchmark
   code, not the production engine. This report decides authority semantics,
   not production implementation equivalence.
2. **No partial WP4-M evidence.** Concurrent WP4-M code and setup artifacts
   were excluded completely.
3. **Hash assumptions.** A 32-byte BLAKE3 output is intended to provide
   256-bit first/second-preimage resistance and 128-bit collision resistance.
   Real collisions are outside normal detection authority. NIST likewise
   describes the generic collision strength of an L-bit digest as L/2.
4. **Not independent primitives.** The whole-source, raw-ID, canonical-ID, and
   transcript mechanisms all use BLAKE3 in the current/proposed design, though
   their messages/domains differ. Retaining more than one is defense in depth,
   not cryptographic independence between algorithms.
5. **Canonical encoding assumption.** The implication proof requires strict,
   injective Bytes framing and complete identity-before-grammar authentication.
   A truncated ID, unauthenticated locator, ambiguous encoding, or partial BLOB
   hash invalidates it.
6. **Tuple/domain assumption.** Domain separation alone does not frame tuples;
   fixed 4+32-byte entries do. Tuple framing alone does not separate purposes;
   the hard-coded derive-key context does. Both are required.
7. **Repeated-equal occurrences.** Reordering two byte-identical equal tuples
   is unobservable because it does not change the logical file. Reordering
   unequal tuples is committed.
8. **Caller assertion distinction.** A future public “expected whole-source
   digest” precondition would be a distinct product feature. It is
   **Unavailable** today and must not be invented from benchmark custody.
9. **Transaction isolation assumption.** Mutation serials cover governed
   Store operations. Authority against untracked concurrent mutation rests on
   the exact SQLite transaction/single-writer boundary; whole-source hashing
   does not cover that gap.
10. **Tests are not proof.** The test list can establish implementation
    conformance and adversarial coverage but cannot demonstrate BLAKE3
    collision resistance.

## 12. Linked sources

### Local controlling sources

- [Canonical v2 single-identity study](../../core/canonical/v2-single-identity.md)
- [Identity and hashing study](../../core/canonical/identity-and-hashing.md)
- [Full-create pipeline study](../../core/pipeline/full-create-pipeline.md)
- [Verification, security, and resources study](../../assurance/verification-security-resources.md)
- [Invariant matrix](../../foundations/invariant-matrix.md)
- [Accepted F2 report](../../../../implementation-detail/phase-4/wp4m/f-series/f2/report.md)
- [Algorithm specification](../../../../implementation-detail/phase-4/algorithm/spec.md)
- [Tests and benchmarks specification](../../../../implementation-detail/phase-4/algorithm/tests-and-benchmarks.md)
- [Complexity analysis](../../../../implementation-detail/phase-4/algorithm/complexity-analysis.md)
- [Sealed accepted F2 source](../../../../target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs)

Committed core sources were inspected at
`d781173a08ab4092eb539c3a0870056e6c6a77ff` with `git show`, including
`identity/`, `object/`, `content/persistence.rs`, `validation.rs`, and the typed
error enum. This avoids treating the live dirty persistence file as evidence.

### Primary cryptographic and original-design sources

- [BLAKE3 specification and domain-separated modes](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.tex)
- [Official BLAKE3 C API: hard-coded globally unique derive-key contexts](https://github.com/BLAKE3-team/BLAKE3/blob/master/c/README.md)
- [RFC 9162 Merkle-tree definition and leaf/node domain separation](https://www.rfc-editor.org/rfc/rfc9162.html#section-2.1.1)
- [NIST SP 800-185 TupleHash: unambiguous tuple hashing](https://csrc.nist.gov/pubs/sp/800/185/final)
- [NIST SP 800-107 Rev. 1: hash collision/second-preimage security terminology](https://csrc.nist.gov/pubs/sp/800/107/r1/final)
- [Official Git SHA-256 transition design: object type/length/content framing](https://git-scm.com/docs/hash-function-transition#_object_names)

REDUNDANT_WITH_ORDERED_CANONICAL_EVIDENCE
