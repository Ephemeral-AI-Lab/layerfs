# Canonical-v2 single-identity fast exploration findings

Status: `SHADOW PASS / EXPLORATORY BREAKTHROUGH / PROMOTION CAMPAIGN WORTH AUTHORIZING`

Date: 2026-08-21

This report ends the fast canonical-v2 task. It does not integrate production,
promote a mapping profile, rewrite v1 history, reopen H05, or claim Phase 4
complete.

## 1. Decision

The semantically viable canonical-v2 direction is fixed-radix K64/F64 with one
canonical chunk identity and one ordered occurrence commitment:

```text
v2 occurrence
  u32be(raw_length) || canonical ObjectId[32]
  = 36 bytes

v2 ordered commitment
  BLAKE3 derive-key
    context "layerfs/canonical-v2/ordered-occurrence/v1"
    input repeated(u32be(raw_length) || canonical ObjectId)
```

The canonical `ObjectId` remains the locator and complete-byte authenticator of
the unchanged canonical Phase-1 Bytes object. Every mapping-specific Bytes
record uses mapping version 2. K64/F64, CDC boundaries, canonical chunk BLOBs,
caller-thread execution, one transaction, one COMMIT, and FULL+DELETE remain
unchanged.

This design passed the nonpersistent authority/format shadow and one frozen
benchmark-private exploratory screen. Against sealed CP-0009 it won all three
measured pairs with a **37.810977% paired-median durable improvement**. The
candidate median was **398.756250 ms / 250.779768 MiB/s**, earning the frozen
research label **BREAKTHROUGH**.

That result is strong enough to authorize a promotion-grade canonical-v2
campaign after the format/migration blockers in section 11 are closed. It is
not itself promotion evidence.

## 2. Evidence authority and custody

CP-0009 remains the accepted control:

```text
HEAD                 febc20f046bba84ccdce1256363d77799eabf2db
control source       3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a
control executable   9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7
fixture SHA-256       63b3695b8c117b5bc39885e0df0dcd0af1d49e575482bab16577d84b4f40eff4
fixture bytes         104857600
```

The rejected H05 live source was restored narrowly to the exact CP-0009 source
before candidate work. Its main-file HEAD-diff SHA-256 reverified as
`b073a7e0...50f84`. Frozen H05 remained preserved at
`e675d2fc...d031`; H05/H05b/H05c conclusions were not relabeled.

Canonical-v2 candidate custody:

```text
candidate main source  e8b721013308bcd1ccce54e35f40026e12df067107b72431b00536e8328edd4a
candidate codec        5b098248d0d88adb0b09d1309b2328b54f98c89c64e4b7c389a8d4f836e2b574
candidate executable   7419acc21672cc92c698675db2e68f3b0281282c26623744d2d5c1be495a9b82
shadow source          4d02a68c39130ce87d6040b951559c025f6f44824c9de84265f2134e9e8bced0
screen runner          8af277c364e609a2f60ecfbb12b2e2fff0d8b30746ef836d28bcc9693c66d8e4
screen analyzer        9ad91c5f478001dc24fd3b4fe096b8373c886c8faa87b1f653cbc250e89b293f
screen raw             347f4795471037b92608d912fa21bb6796877328b0e67bdd4dff1f9a1fabcdeb
protected smoke        cdbf135d5dd6b878102db15cab41e02da65753f353c117614b4ad6c2a8cfb1a9
independent analysis   01b43e9560f99b7340756c6ca41d3f1afe424ac349c70f114d514ca00eced93b
```

Primary evidence is under
`target/phase4-canonical-v2-exploration-20260821-v1`. The preregistration is
`canonical-v2-exploration-preregistration.md`.

## 3. Code/evidence and parallel-lane synthesis

Three disjoint read-only lanes inspected actual source and sealed artifacts:

- Identity/authority traced every raw and canonical producer/consumer and
  identified the v1-normalization authority boundary.
- Format/migration/errors derived the exact v2 grammar, profile, receipt/error
  matrix, and the smallest honest transition policy.
- Execution/primary research traced the complete 100-MiB path, recomputed
  ceilings, and mapped Git, Venti, Xet, RFC 9162, Nix, and Bao precedents back
  to LayerFS invariants.

The lanes agreed on compact K64/F64 v2. The important corrections to the prior
research were:

1. H05 still retained the v1 raw sequence and every raw-ID hash/reference, so
   it did not prove canonical-only authority.
2. Full create already carries the canonical ID returned by CAS admission; a
   separate carried-ID full-create variant would duplicate the same mechanism.
3. V1 reference normalization is always syntactically payload-free, but it is
   authority-preserving without payload fetch only when a valid v1 closure
   receipt/same-open witness covers the legacy raw-ID check.
4. The current transition and store formats cannot honestly publish a
   v1-parent/v2-child edge; the initial durable policy must reject it.

## 4. Raw-ID consumer and authority graph

| Surface | Current v1 authority | Canonical-v2 treatment |
|---|---|---|
| Digest | `ObjectId = BLAKE3("layerfs/object\0" || bytes)`; `ChunkId` is only an alias | Retain the canonical-object digest; delete the raw-payload alias from v2 occurrences |
| Canonical chunk | Exact `LFSO/Bytes/length/raw` framing commits to role, length, payload, and EOF | Unchanged bytes and ID |
| Memory CAS | Keyed by raw `ChunkId`; put/get/reuse rehash raw bytes | Production integration must adapt this lane; shadow/durable candidate do not pretend it is already migrated |
| `LogicalFile` | Stores raw ID + length; ranges/rejoin compare raw IDs | End-to-end v2 must store/compare length + canonical ID |
| Provisional COW identity | Transitively hashes raw IDs and lengths | Changes during eventual core integration |
| Durable v1 leaf | `raw_id[32] || length[4] || object_id[32]` | `length[4] || object_id[32]` |
| Durable CAS | Canonical ID is already SQLite locator; incumbent is fully authenticated/equal-compared | Unchanged equal-only authority |
| Full create | Raw hash, canonical encode/hash, CAS, dual-ID reference, whole-source hash, raw sequence | Canonical encode/hash once, CAS, compact reference, one canonical ordered commitment |
| Construction proof | Put evidence + count/total/topology/root/transition + source/raw transcript | Same scope/topology proof with canonical transcript; raw/source hash lanes zero |
| Same-count rejoin | Two `(length, raw_id)` confirmations; selected chunks are rehashed during persistence | Two `(length, canonical_id)` confirmations; eventual production candidate should carry the computed ID into persistence |
| Scrub/reconstruction | Canonical ID authentication, decode/length check, then redundant raw hash | Retain complete canonical auth and length; remove raw hash |
| Range | Authenticated mapping path and complete selected chunk, then raw hash before output | Same no-output-before-authentication boundary; remove raw hash only |
| Changed spine | Equal full references are covered; changed chunk canonical-authenticates then raw-hashes | Equality is `(length, canonical ID)`; changed chunk canonical-authenticates and length-checks |
| Delta | Parent/child roots and operations; no raw IDs | Profile-specific roots change; current codec lacks cross-profile fields |
| Receipt | Binds one expected profile/root/transition/store authority | Existing 216-byte receipt format can bind v2, but v1 receipt cannot be accepted as v2 |

The decisive implication is:

```text
ordered authenticated (length, canonical ObjectId)
  -> each ID authenticates exactly one complete framed Bytes object
  -> each object uniquely decodes one raw payload of that length
  -> ordered occurrences uniquely determine the concatenated source
```

This rests on canonical BLAKE3 collision resistance plus injective framing. The
removed raw check used the same hash algorithm/domain over a different
preimage; it was redundancy, not an independent cryptographic primitive.

## 5. Nonpersistent shadow specification and results

The shadow is test-only and adds no public trait, registry, selector, bridge,
or persistent migration format. It implements:

- exact 36-byte reference encode/decode with 32-KiB hard limit;
- mapping version 2 leaves, branches, and roots under K64/F64;
- two independent byte builders;
- v1 structural normalization;
- canonical-BLOB equal-only reuse;
- ordered commitment;
- scrub, reconstruction, and authenticated path/range routing;
- same-count and `+1/-1` canonical rejoin;
- profile/receipt/downgrade decisions; and
- checked overflow plus terminal-Q cleanup.

Focused boundaries are empty, 1, K, K+1, K×F, and K×F+1. Adversaries include
wrong length, wrong role, short/trailing bytes, omitted/duplicated/reordered
occurrences, legacy wrong raw ID, v1/v2 confusion, receipt mismatch, downgrade,
and failed/overflowing Q charges.

Frozen vectors:

```text
v2 profile ID
94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b

one-abc leaf (64 canonical bytes)
4c46534f0100000037000000334c4653344d415000000202000000010000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b

empty v2 file root (49 canonical bytes)
4c46534f0100000028000000244c4653344d41500000020100000000000000000000000000000000000000000000000000

left/right/left v2 root
d618ebc7b5309eb8cb3c777f4de134759e165e2d36d7b8004c7b2c51ac3d9031

left/right/left commitment
1f680efae4a11ed98904fec40cd2e28fa0e607b48e43942f77d8c42e82310a2d
```

All three shadow tests pass in approximately 0.08 seconds. Both builders agree
at every required topology boundary. The retained file-only mapping model is
196,055 bytes; adding the unchanged surrounding 119 bytes gives the exact
196,174-byte row model.

## 6. Migration, error, and receipt decision matrix

### Initial transition policy

| State | Requested action | Result |
|---|---|---|
| Authenticated v1 leaf | Structural shadow normalization | Success without payload fetch |
| Unauthenticated/unreceipted v1 leaf | Authoritative normalization | Full legacy validation or `ValidationAuthorityUnavailable` |
| Fresh isolated store | v2 genesis | Supported by private candidate |
| Exact v2 parent | v2 child | Supported by private candidate |
| V1 parent/history | Durable v2 child | `SchemaMigrationRequired` before mutation |
| V2 parent | v1 writer/downgrade | `ProfileMismatch` before mutation |
| V2 head with v1-profile receipt | Receipt verification | `InvalidValidationReceipt` |
| Existing nonempty v1 store | Automatic rewrite/relabel | Forbidden; bytes remain unchanged |

The current delta index has parent/child roots but no parent/child profile IDs,
and current production SQLite has neither the complete profile-bound visible
head nor receipt integration. A cross-profile publication would therefore be a
new product decision, not a codec detail.

### Version-specific exact errors

V1 keeps `ChunkIdentityMismatch` for a canonical-authenticated chunk whose raw
payload hash disagrees with the stored legacy raw ID. V2 has no raw-ID field,
so that error is not representable. V2 retains:

- missing object -> ID-bearing engine `MissingObject` (core is still unit);
- bytes under wrong expected canonical ID -> `IdentityMismatch` before grammar;
- authentic wrong outer/mapping role -> `WrongLogicalRole`;
- decoded length mismatch -> `ChunkLengthMismatch`;
- short/trailing/version/tag/partition/count/aggregate errors unchanged;
- wrong expected root/commitment/transition -> publication qualification
  conflict; and
- receipt/profile/store/generation/epoch mismatch ->
  `InvalidValidationReceipt`.

A different valid same-length canonical chunk defines a different valid v2
file/root. Expected root, commitment, transition, and receipt authority reject
it; a second raw hash is unnecessary.

## 7. Direct counters and structural result

All scheduled candidate rows had the exact preregistered work:

```text
source / CDC bytes                         104857600 / 104857600
references / chunks                             5284 / 5284
raw-ID hash bytes / hashes                           0 / 0
whole-source construction bytes / hashes             0 / 0
canonical commitment bytes / entries / hashes   190224 / 5284 / 1
mapping bytes                                    196174
canonical new bytes                           105122466
objects created / reused                         5372 / 0
leaves / branches                                  83 / 2
transactions / COMMITs                              1 / 1
candidate Q high-water                              86045
terminal Q                                               0
```

Control Q was 88,093; compact v2 reduced the exact high-water by 2,048 bytes.
The mapping reduction was exactly 169,088 bytes and canonical new bytes fell
by the same amount.

## 8. Exploratory performance result

Frozen schedule:

```text
warmup  AB
measured AB / BA / AB
```

The complete runner passed 7 protected smokes plus 8 scheduled rows in 78
seconds.

| Pair | Order | CP-0009 | Canonical-v2 | Durable improvement | Mapping-stage improvement |
|---:|:---:|---:|---:|---:|---:|
| 1 | AB | 641.200375 ms | 398.756250 ms | 37.810977% | 42.379733% |
| 2 | BA | 597.742250 ms | 443.858000 ms | 25.744248% | 38.192177% |
| 3 | AB | 647.740000 ms | 391.869292 ms | 39.502070% | 43.143203% |

```text
candidate wins                    3/3
paired median improvement         37.810977%
control arm median               641.200375 ms
candidate arm median             398.756250 ms
candidate min / max              391.869292 / 443.858000 ms
candidate median throughput      250.779768 MiB/s
research classification          BREAKTHROUGH
```

The prior row-wise optimistic combined ceiling was 452.873 ms. The actual
candidate median was 54.117 ms faster than that model. This is observed local
evidence, but attribution beyond the exact removed counters is not claimed:
mapping serialization, canonical byte volume, compiler/cache interaction,
SQLite page layout, and COMMIT state all changed together as consequences of
the single representation variable.

### Evidence gaps to targets

Using the measured candidate arm median:

```text
500.000 ms target   -101.243750 ms  (101.244 ms below target)
400.000 ms target     -1.243750 ms  (1.244 ms below target)
333.333 ms stretch    65.423250 ms  remaining
```

The fastest individual candidate row was 391.869292 ms. It is not substituted
for the controlling median.

## 9. Storage and resource result

```text
control apparent store       109269024 bytes
candidate apparent store     109199392 bytes
observed apparent reduction      69632 bytes

control allocated values     109273088, 117510144
candidate allocated values   109203456, 117510144
```

Every candidate row stayed within 107.61% of its own apparent endpoint, below
the frozen 125% exploration cap. Paired candidate-minus-control allocation was
`+8,237,056 / -8,306,688 / +8,237,056` bytes. This direction-changing APFS
observation is reported, not normalized away. It is not physical I/O or
exclusive-extent evidence.

All rows retained one transaction/COMMIT, FULL+DELETE, timer equations,
terminal Q zero, and no journal/WAL/SHM residue.

## 10. Create/edit/scrub/reconstruction/range tradeoffs

The protected candidate smoke passed all seven operations. These are single
candidate diagnostics compared below with CP-0009 control medians; they are
not adjacent performance distributions and therefore do not make independent
acceptance claims.

| Operation | Canonical-v2 smoke | CP-0009 context | Interpretation |
|---|---:|---:|---|
| same-open same-count edit | 7.499 ms | 9.737 ms median | Raw hashes are zero; fixed changed-spine behavior remains |
| first authority | 137.974 ms | 245.330 ms median | Canonical-only scrub removes a raw pass, but cross-campaign comparison is descriptive |
| `+1` early publication | 4.714 ms | 7.375 ms guard | Mapping suffix remains O(N); bytes per reference shrink |
| `+1` middle publication | 3.929 ms | 5.322 ms guard | Same suffix-linear limitation |
| warm reconstruction | 325.900 ms | 425.801 ms median | Complete canonical auth remains; raw rehash removed |
| fresh reconstruction boundary | 338.839 ms | 433.513 ms median | Reopen plus complete output remains linear |
| returned 1-MiB range | 2.162 ms | 3.285 ms median | Complete selected chunks are still authenticated before output |
| reopen/head | 2.479 ms | 3.008 ms median | Profile-bound head work; no full scrub claim |

Tradeoffs:

- Full create gains the most and now clears 500 ms locally.
- Same-count rejoin is semantically correct with canonical confirmations, but
  the private candidate recomputes selected canonical IDs during persistence;
  a promotion candidate should carry the already-computed ID through a
  move-only occurrence token.
- Count-changing topology is unchanged: compact refs reduce bytes and CPU, not
  the O(suffix) worst case.
- Scrub/reconstruction/ranges keep full canonical authentication, strict role
  and length checks, no output before authentication, and byte-linear full
  operations. They only remove the redundant raw hash.
- V2 representation couples chunk equality to canonical Bytes framing and
  intentionally gives up the separate raw-message redundancy.

## 11. Variants and disposition

| Variant | Disposition | Evidence |
|---|---|---|
| Same-width duplicate-canonical bridge | `SKIP / NOT NEEDED` | The sealed 95.185-ms raw lane already isolates its ceiling; a redundant bridge would add another format obligation |
| Compact v2 retaining whole-source digest | `FALLBACK / NOT BUILT` | Raw-only row-wise ceiling was 521.486–543.916 ms, unlikely to reach 500 ms despite full migration cost |
| Compact v2 + one ordered commitment | `SELECTED / BREAKTHROUGH` | Shadow PASS; 3/3 screen wins; 37.811% paired median |
| Separate full-create carried-ID variant | `SKIP` | CP-0009 already carries the canonical ID returned by generated CAS admission |
| Carried canonical ID for edit rejoin | `NEXT IMPLEMENTATION DETAIL` | Current private edit path recomputes selected IDs; fix before promotion evidence |
| CDC/prolly/page-size/compression/workers/Bao | `OUT OF SCOPE` | Different variable or product contract; no need to answer canonical-v2 |

Primary precedents support the shape, not the LayerFS result: Git uses one
typed/length-framed object identity; Venti uses one write-once content address;
Xet composes ordered length/chunk-hash commitments; RFC 9162 demonstrates
explicit ordered/domain-separated commitments; Git SHA-256 migration shows
fail-closed format negotiation; Nix accepts intrinsic profile-specific graph
identities; Bao shows authenticated streaming but would require a separate
range format. None supplies LayerFS migration or acceptance evidence.

## 12. Focused verification performed

```text
cargo test -p layerfs-core --test canonical_v2_shadow
  3 passed

cargo test -p layerfs-engine --bin phase4_create_edit_benchmark f2_
  14 passed

cargo test -p layerfs-engine --bin phase4_create_edit_benchmark \
  measured_edit_starts_from_an_already_published_base
  1 passed

additional exact tests:
  canonical-v2 commitment
  SQLite/range/reopen parity
  exact edited-stream expectations
  count-change construction proof
  selected-profile topology/Q
  changed-spine, deep-spine, witness lifecycle

release self-test PASS
1-MiB untimed counter row PASS
git diff --check PASS on owned files
```

No workspace suite, full Clippy, long property/fuzz suite, 512-MiB campaign,
or second performance screen was run.

## 13. Blockers and limitations

Before any production integration or promotion-grade claim:

1. Freeze a normative v2 specification and independent full corpus for every
   mapping/root/delta/receipt/error identity.
2. Replace the benchmark-private delta version adapter with a real v2 decoder.
   The current helper authenticates v2 bytes, then reuses the unchanged layout
   by translating the version byte in memory for the v1 semantic parser.
3. Add a move-only occurrence token binding canonical ID, length, role,
   canonical length, put evidence, store/open/transaction/epoch/profile, and
   mutation serial; carry its ID through edit persistence.
4. Decide the public migration policy. The recommended first policy rejects
   v1-parent/v2-child publication with `SchemaMigrationRequired`; supporting it
   later requires explicit parent/child profile authority.
5. Resolve v1 structural normalization authority: valid retained receipt/
   same-open proof, or full legacy payload validation. Never erase a potential
   v1 `ChunkIdentityMismatch` silently.
6. Integrate the single identity through `InMemoryCas`, `LogicalFile`,
   provisional COW identity, production SQLite complete visible head, receipt,
   and exact error translation. A leaf-only production change is insufficient.
7. Resolve adversarial cross-reopen key/epoch/rollback custody before claiming
   receipt-backed migration or fast authority.
8. Replace the unit core `MissingObject` with/through exact ID-bearing mapping
   error translation at the production boundary.
9. Remove historical-v1 private golden code/warnings from a clean v2 candidate;
   this exploratory binary deliberately bypassed v1 frozen-result gates after
   freezing its own v2 preregistration values.

Screen limitations remain one host, one fixture, three pairs,
warm-or-unknown OS/filesystem cache, no physical-I/O/fsync attribution, and
variable APFS allocated-block observations.

## 14. Deferred promotion work

Explicitly deferred:

- five-pair or larger promotion campaign;
- full workspace/all-target/all-feature suite and full Clippy;
- 512-MiB/100-GiB runtime or multi-host study;
- long fuzz/property/adversarial corpus;
- crash/fault campaign for a production v2 writer and migration;
- nonempty v1 history rewrite, dual reader/writer lifecycle, GC, or downgrade;
- production schema/visible-head/receipt integration;
- page-size, compression, carrier, workers/async, H09/prolly, WP5, or native
  materialization.

## 15. Recommended next action

Authorize a promotion-grade canonical-v2 campaign, but only after the narrow
format-authority work above lands in a new private candidate. Keep the selected
representation and commitment unchanged. Add the move-only occurrence/carry
proof, normative v2 codec/delta/receipt goldens, and explicit migration
rejection; then preregister a five-pair campaign with tighter storage/resource
gates and the same protected lifecycle operations.

Do not integrate production, promote the profile, rewrite v1 history, or stack
another performance variable on this exploratory result.
