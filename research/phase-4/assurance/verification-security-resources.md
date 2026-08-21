# Local verification, security, and resource optimization research

Status: direction-finding only. No implementation, format, schema, durability,
authority, or benchmark change is authorized by this report. Local code and
sealed evidence are controlling; external systems are precedent, not proof.

## 1. Evidence vocabulary and implementation boundary

- **Observed:** directly in current code, sealed raw evidence, or a terminal
  evidence-backed report.
- **Derived:** arithmetic or complexity from observations; equation shown.
- **Hypothesis:** a proposed mechanism that still needs a prospective test.

The repository contains two relevant layers:

- The current production-shaped engine is
  `crates/layerfs-engine/src/lib.rs` (schema version 1, no validated-snapshot
  receipt integration).
- The accepted F2-v3 K64/F64 candidate and its receipt/construction-proof logic
  are preserved in
  `target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs`.
  It is benchmark evidence, not production behavior.

## 2. Current verification behavior

### 2.1 Identity domains

**Observed:** Phase 1 `ObjectId` is BLAKE3 over
`"layerfs/object\0" || complete bytes` (`crates/layerfs-core/src/identity/digest.rs:5-25`,
`:39-65`). `ChunkId` is an alias for `ObjectId`, and raw chunk identity hashes
raw bytes in that same domain (`crates/layerfs-core/src/identity/mod.rs:9-13`).

The durable file reference stores both:

```text
raw ChunkId          32 bytes
raw length            4 bytes
canonical ObjectId   32 bytes
```

(`crates/layerfs-core/src/content/persistence.rs:20-21`, `:38-65`). The canonical
chunk object is `LFSO || kind || lengths || raw bytes`, so both hashes cover the
raw payload under different messages.

**Observed:** `validate_identity` hashes the complete canonical object and then
decodes its complete grammar (`crates/layerfs-core/src/object/codec.rs:153-165`).
That order determines error precedence: an unauthentic malformed object is
`IdentityMismatch` before a grammar error.

### 2.2 Full-create construction proof

The accepted F2 path issues one transaction-local `PutEvidence` for every
authenticated immutable put and folds leaf/branch/file/workspace/transition
summaries into a single-use proof. It binds store instance, authority, profile,
open, transaction, authority serial, mutation serial, topology, source length,
reference count, root, and transition. See accepted F2 source `:3192-3254`,
`:3305-3551`, `:3581-3663`.

**Observed:** the proof also maintains:

- a BLAKE3 source hasher updated with all 104,857,600 raw bytes;
- a BLAKE3 sequence hasher updated with every raw length and raw `ChunkId`;
- expected fixture fingerprints checked before publication.

See accepted F2 source `:3192-3209`, `:3332-3367`, `:3542-3543`,
`:3655-3685`, `:9841-9880`.

**Observed:** F2 removed the former full pre-COMMIT SQL/BLOB closure replay.
The accepted result reduced pre-COMMIT qualification from 387.465 ms to
0.051 ms and durable full create from 916.310 ms to 659.593 ms while preserving
root/transition/closure and one COMMIT
(`implementation-detail/phase-4/wp4m/f-series/f2/report.md:1434-1488`).

### 2.3 Post-COMMIT verification

**Observed:** after COMMIT, the candidate reopens independently, verifies the
complete visible head and receipt, performs a fresh full scrub, reconstructs the
file, and verifies exact ranges. Full scrub and reconstruction deliberately
remain byte-linear (accepted F2 source `:10315-10399` and
`implementation-detail/phase-4/algorithm/complexity-analysis.md:631-751`).

**Observed duplicated-pass optimization hypothesis:** `authenticate_blob` opens
and streams the same SQLite BLOB once to compute `ObjectId`, then opens and
streams it again for canonical grammar validation
(`crates/layerfs-engine/src/lib.rs:968-1017`). `read_object_range_on_connection`
calls that two-pass authenticator and then opens the BLOB a third time for the
requested range (`:912-965`). Its wall is unmeasured. A one-pass replacement may
need a deferred-error streaming parser that continues hashing after a grammar
failure to preserve `IdentityMismatch`-first precedence; if exact precedence
requires rereading, the one-pass hypothesis is killed. The accepted F2
benchmark uses a different private read path and supplies no performance proof.

### 2.4 Receipt and authority

**Observed candidate behavior:** `ValidatedSnapshotReceiptV1` is a 216-byte
canonical object containing store instance, authority ID, epoch, generation,
root, transition, mapping profile, and a keyed BLAKE3 authenticator
(`crates/layerfs-core/src/validation.rs:7-59`, `:62-130`). The accepted candidate
keeps the key in a mode-0600 sidecar and writes zeros to the database's
`validation_key` field (accepted F2 source `:1842-1858`, `:1984-2005`).

**Observed security problem in the private candidate:** the sidecar key is
derived deterministically from profile, current time, and path
(accepted F2 source `:1822-1832`). Those are not a cryptographic entropy source.
This does not describe current production, which has no receipt integration,
but it is a blocker if the candidate authority design is promoted.

**Observed limitation:** the repository correctly refuses to treat the 216-byte
snapshot receipt as proof that arbitrary bytes fetched later are still present
and authentic. Fresh scrub remains complete; cross-reopen fast authority is
unavailable without exact key/epoch/rollback custody
(`algorithm/complexity-analysis.md:682-718`).

## 3. What the sealed attribution says

The F4-A observer-heavy diagnostic reports these component medians:

| Full-create component | Median | Mapping share | Status |
|---|---:|---:|---|
| Raw `ChunkId` hash | 95.185147 ms | 18.16% | current format invariant |
| Construction source/sequence hash | 89.067215 ms | 16.99% | gross combined upper bound; whole-source sublane is not isolated |
| Canonical `ObjectId` hash | 96.068155 ms | 18.33% | current format invariant |
| All three disjoint hash intervals | 280.146626 ms | 53.45% | subtotal, not separately added |
| Canonical + mapping encode | 3.161540 ms | 0.60% | required bytes |
| Explicit row materialization copy | 0 ms | 0% | absent in mapping path |
| Mapping VDBE+pager composite | 48.853618 ms | 9.32% | inseparable |
| Mapping direct VFS | 24.281657 ms | 4.63% | required I/O |

Source: `implementation-detail/phase-4/wp4m/f-series/f4/report.md:272-315`.

The same diagnostic's medians are 524.111750-ms mapping, 112.144334-ms
standalone COMMIT, 636.836792-ms durable create, 280.250583-ms fresh scrub, and
438.069792-ms reconstruction (`f4/report.md:215-226`). The remaining diagnostic
gap to 500 ms is 136.836792 ms, or 21.49% (`:238-255`).

### Resource observation

All five measured F4 rows report:

- application-owned logical Q high-water 65,417--65,421 bytes and terminal zero;
- SQLite page-cache snapshot maximum 87,049,984 bytes;
- process maximum RSS approximately 93.9--94.1 MB;
- 6,675 SQLite cache spills.

Sources: sealed
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/f4a.raw.jsonl` and
`target/wp4m-f4a-residual-attribution-k64-20260820-v1/resources.stderr`.

**Derived:** `87,049,984 / 65,419 ~= 1,330.6`. Exact application Q is useful but
is not a whole-process or SQLite-memory bound. The report already labels
SQLite cache high-water unavailable because the API specifies zero high-water
for `SQLITE_DBSTATUS_CACHE_USED`; these are observed snapshots, not invented
high-water values.

## 4. Primary-source lessons that apply locally

### 4.1 BLAKE3 is already a tree hash with SIMD

The official [BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs)
defines its 1-KiB-chunk tree and domain-separated modes. The official
[BLAKE3 implementation](https://github.com/BLAKE3-team/BLAKE3) includes runtime
SSE/AVX/NEON implementations; Rayon is an optional multithreaded feature.

Applicable lesson: do not invent another hash implementation or assume that
parallel workers are required for SIMD. The retained single-threaded library is
already hardware-aware. The large opportunity is eliminating an unnecessary
*semantic digest*, not changing one `Hasher::update` call.

Distinct existing digests cannot be derived from each other without changing
their messages and frozen outputs. “Fuse three BLAKE3 hashes into one” is not a
valid format-preserving claim.

### 4.2 Merkle proofs narrow authenticated reads, not full scrub

[RFC 9162](https://www.rfc-editor.org/rfc/rfc9162.html) specifies domain-separated
Merkle inclusion and consistency proofs and requires exact index/tree-size/root
checks. [Bao's primary specification](https://github.com/oconnor663/bao/blob/master/docs/spec.md)
shows verified streaming and range proofs rooted in BLAKE3.

Applicable lesson: LayerFS's radix/Merkle DAG already gives path-local mapping
authentication under a valid receipt. Bao-like within-object proofs could make
ranges sub-object, but ordinary chunk objects are at most 32 KiB, so the proof
metadata and format complexity are unlikely to pay. It cannot replace a named
fresh full scrub, whose purpose is complete reauthentication.

### 4.3 SQLite supports per-connection hardening and honest memory counters

SQLite documents approximate connection memory through
[`SQLITE_DBSTATUS_CACHE_USED`, `SCHEMA_USED`, and `STMT_USED`](https://sqlite.org/c3ref/c_dbstatus_options.html)
and statement memory through
[`SQLITE_STMTSTATUS_MEMUSED`](https://www.sqlite.org/c3ref/c_stmtstatus_counter.html).
It documents per-connection runtime bounds for BLOB/row length, SQL length,
variables, VM operations, parser depth, attached databases, and worker threads
through [`sqlite3_limit`](https://www.sqlite.org/c3ref/limit.html).

[`SQLITE_DBCONFIG_DEFENSIVE` and `TRUSTED_SCHEMA`](https://www.sqlite.org/c3ref/c_dbconfig_defensive.html)
disable dangerous schema features; SQLite recommends disabling trusted schema
where possible. These are defense-in-depth controls, not throughput claims.

SQLite's official [atomic commit description](https://www.sqlite.org/atomiccommit.html)
explains that rollback-journal `FULL` durability writes and syncs the journal
before database pages and that cache spill can force extra write/sync work. This
makes spill/cache-size profiling legitimate, but not free: avoiding 6,675
spills can move dirty pages and memory into COMMIT rather than remove them.

### 4.4 Authentication keys require OS entropy

Apple's official [`SecRandomCopyBytes`](https://developer.apple.com/documentation/security/secrandomcopybytes%28_%3A_%3A_%3A%29)
and the cross-platform [`getrandom` documentation](https://docs.rs/getrandom/latest/getrandom/)
describe cryptographically secure system randomness and explicit failure. A
keyed BLAKE3 authority key must come from such a source, not timestamp/path
hashing. This is mandatory security work if receipt authority is promoted; it
is not a performance optimization.

## 5. Ranked directions

| Rank | Direction | Target workload | Kind | Plausible upside / ceiling | Risk |
|---:|---|---|---|---|---|
| 1 | Isolate the whole-source and ordered-sequence hash sublanes, then decide whether raw-source fingerprinting is product work or benchmark-only work | 100-MiB full create | Potential semantic work elimination; format-preserving if root/reference identities stay unchanged | 89.067 ms is only the gross combined upper bound (14.0% durable); source-only upside is unknown | Proof may stop detecting a bad raw reference before COMMIT unless typed evidence binds the once-computed raw ID |
| 2 | Fuse production BLOB identity hashing and grammar validation into one streaming pass; capture the requested range during that pass | Scrub, reconstruction, `load_object`, range reads | Constant-factor, no format change | Removes one complete BLOB pass, and range's third read; local wall unmeasured | Must preserve `IdentityMismatch`-first error precedence and never expose bytes before authentication |
| 3 | Future format: use canonical chunk `ObjectId` as the sole chunk identity and remove raw `ChunkId` from each file reference | Full create and every mapping | Algorithmic/representation simplification | Gross raw-hash lane 95.185 ms (14.95% durable ceiling); saves `32*5,284 = 169,088` mapping bytes on fixture | Changes Phase-2/Phase-4 identities, goldens, mapping format, rejoin fingerprint, migration |
| 4 | Stream current `DeltaRecord` identity input through `ObjectId::from_reader` instead of allocating `prefix || parent || child || payload` | Large delta write/load | Constant-factor and Q reduction | Removes one payload-sized temporary copy; zero on genesis/empty delta | Exact byte-stream/domain parity |
| 5 | Generation/authority/locator-scoped verified-work receipt for repeated reads in one operation/open | Repeated local ranges and reused subtrees | Constant-factor cache with strict bound | Can remove duplicate reads/hashes only when repetition exists | Stale locator, epoch rollback, unbounded cache, misuse during fresh scrub |
| 6 | Explicit SQLite cache/resource profiles with DBSTATUS/STATEMENT memory accounting | Large writes under memory pressure | Resource/performance tradeoff | Mapping direct-VFS gross ceiling 24.282 ms; any larger claim needs evidence | RSS/COMMIT expansion, spill merely shifted, process-global controls |
| 7 | Bao/outboard proofs within large canonical objects | Large directory pages only | Format/storage change | Sub-object range authentication | No useful payoff for <=32-KiB chunk objects; added durable bytes |

## 6. The strongest full-create direction

The decisive semantic question is:

> Is the complete raw-source BLAKE3 fingerprint part of the product's capture
> acceptance contract, or is it only a benchmark/golden assertion duplicating
> the durable root's commitment to the ordered authenticated chunk stream?

The mapping root already commits, transitively, to ordered raw lengths, raw
`ChunkId`s, canonical chunk `ObjectId`s, and canonical chunk bytes. The source
hasher does not create a serialized identity. Fixture preflight is explicitly
outside the headline timer (`implementation-detail/phase-4/algorithm/spec.md:294-307`).

The 89.067-ms construction source/sequence lane is the only observed gross,
format-preserving full-create upper bound in this report with strategic
magnitude. It is not an isolated source-hasher measurement: a later diagnostic
must split whole-source hashing from the ordered length/`ChunkId` transcript
before assigning removable milliseconds. The current proof also uses the
expected sequence fingerprint to reject a wrong raw-ID/reference sequence before
COMMIT. Removing it without replacing that binding would weaken validation.

The smallest later specialist question is therefore not “can we delete a hash?”
It is:

```text
Can a typed, transaction-local construction evidence value bind the exact
once-computed raw ChunkId, raw length, canonical ObjectId, canonical length,
mutation serial, and source ordering without a second full-source hash pass?
```

If yes, benchmark source SHA/CDC preflight can remain outside capture while the
product proof preserves self-consistency. Only the separately isolated
whole-source sublane may then be claimed as removable. If no, the required
construction hashing remains.

## 7. Amdahl ceilings

Using descriptive F4 component medians (not an additive same-row prediction):

```text
durable full create                    636.836792 ms
target                                 500.000000 ms
gap                                    136.836792 ms (21.49%)

construction source/sequence hash       89.067215 ms (combined gross bound)
  gross mapping share                    16.99%
  gross durable share                    13.99%

raw ChunkId hash                         95.185147 ms
  gross mapping share                    18.16%
  gross durable share                    14.95%

canonical ObjectId hash                  96.068155 ms
  gross mapping share                    18.33%
  gross durable share                    15.08%
```

**Derived planning bounds:**

- Even removing the entire combined construction-hash upper bound cannot reach
  500 ms in ideal arithmetic: `636.837 - 89.067 ~= 547.770 ms`. The removable
  whole-source sublane is smaller and currently unknown.
- A future single-identity format plus removal of auxiliary construction hash
  has a gross descriptive ceiling of about 184.252 ms, enough in arithmetic to
  cross the target. It is not a prediction because component medians differ by
  row and the format candidate changes other work.
- Canonical ObjectId hashing cannot be removed while preserving current CAS
  identity and incumbent authentication.
- SQLite bind, explicit-copy, and mapping VFS lanes are individually too small
  to close the 136.8-ms gap.

For read verification, no valid percentage can be calculated from the F4 rows
for the production two-pass `authenticate_blob`, because the accepted benchmark
uses a different path. Its algorithmic pass count, not a guessed wall time, is
the evidence:

```text
current authenticate + range = 2*Theta(b) full BLOB scans + Theta(r) range read
fused candidate               = 1*Theta(b) scan, capturing r while scanning
```

## 8. Security and resource requirements

### Required before receipt promotion

1. Generate authority keys from a checked OS CSPRNG; fail closed on entropy
   failure. Never derive a secret from path/time/profile.
2. Define sidecar creation durability, parent-directory synchronization, backup,
   restore, permission, and rotation/epoch custody.
3. Preserve fresh independent COMMIT reconciliation. A valid MAC cannot decide
   whether an unacknowledged COMMIT became visible.
4. State the rollback threat model. A copied old database plus copied old key
   can validate unless an external monotonic epoch authority exists.
5. Use constant-time authenticator comparison if the authority can cross a
   timing-observable trust boundary; do not claim that ordinary slice equality
   provides it.

### Honest resource envelope

1. Continue application Q charge/decharge and terminal zero, but label it
   application-owned only.
2. Add `DBSTATUS_CACHE_USED`, `SCHEMA_USED`, `STMT_USED`, lookaside, spill, and
   `STMTSTATUS_MEMUSED` snapshots where supported. Do not report their mandated
   zero high-water fields as real high water.
3. Freeze per-connection `sqlite3_limit` values for maximum BLOB/row, SQL,
   parameters, VDBE operations, parser depth, attached databases, and worker
   threads. These are safety bounds, not measured speedups.
4. Prefer an explicit byte-based page-cache profile and measured spill/RSS/COMMIT
   tradeoff. Do not use SQLite's process-global hard heap limit inside a library
   as though it were per-engine isolation.
5. Keep RSS/peak footprint and temporary/journal allocation separate from Q.

### SQLite defense in depth

Enable defensive mode and disable trusted schema before processing an existing
database, checking every return code. Disable unused extension/trigger/view/
attach capabilities where the SQLite build/API permits. These controls should
be tested for schema-open compatibility and treated as security hardening.

Do **not** run `PRAGMA integrity_check` on every open as a fast-reopen
substitute. SQLite documents `quick_check` as `O(N)` and full
`integrity_check` as `O(N log N)` in its official
[PRAGMA documentation](https://www.sqlite.org/pragma.html); both would duplicate
the product's cryptographic object/closure checks and destroy `O(1)` head reopen.
Use them in explicit repair/audit workflows or after a corruption signal.

Any optimization must preserve `IdentityMismatch`-first error precedence,
complete incumbent authentication, fresh independent scrub, checked limits,
no output before authentication, one transaction/publication COMMIT, fresh
ambiguous-outcome reconciliation, and honest `Unavailable` labels. A receipt
proves only its exact bound tuple, never current byte presence or rollback
freshness.

## 9. Recommendations and anti-recommendations

### Recommend

1. Assign a specialist to answer the construction-evidence question in section
   6. It is the strongest format-preserving full-create direction and has an
   observed 89-ms gross lane.
2. Independently implement the one-pass production BLOB authenticator; it is a
   clear local read-path defect and requires no architecture change.
3. Treat single canonical chunk identity as a later format research branch, not
   a current optimization patch. Its gross 95-ms lane makes it worth analysis.
4. Harden receipt key generation before any production promotion.
5. Extend resource accounting to SQLite/statement memory while preserving the
   distinction from application Q.

### Do not recommend

- Do not remove or truncate cryptographic hashes merely because hashing is the
  largest lane.
- Do not infer that root equality authenticates newly fetched corrupt bytes.
- Do not cache across fresh scrub, authority epoch, rollback, or ambiguous
  publication.
- Do not introduce Bao/outboard data for ordinary 8--32-KiB chunks now.
- Do not enable worker threads/Rayon: current execution and caller-thread
  invariants forbid it, and SIMD is already present.
- Do not change `FULL + DELETE`, weaken COMMIT, or hide durability under a
  receipt.
- Do not use packfiles, a remote manifest, or distributed consensus to solve a
  local verification pass-count problem.

## 10. Disposition

**Current bottleneck:** full-create mapping is hash-dominated, not delta-,
encoding-, bind-, or explicit-copy-dominated. The large format-preserving
question is how much of the combined 89-ms construction timer belongs to the
whole-source fingerprint, whether that sublane is product-required, and how
typed evidence prevents bad raw references from publishing if it is removed.
The current production-shaped read path has duplicated BLOB passes worth a
one-pass hypothesis, but its wall is unmeasured and exact error precedence may
kill the elimination; it targets scrub, reconstruction, and ranges rather than
durable write.

**Direction:** investigate construction proof semantics first, fuse production
BLOB verification second, and keep the single-identity representation as a
separate high-risk format study. Resource and authority hardening are mandatory
for correctness but should not be sold as throughput work.
