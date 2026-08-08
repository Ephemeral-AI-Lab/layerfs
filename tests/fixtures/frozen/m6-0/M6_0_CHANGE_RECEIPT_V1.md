# ChangeReceiptV1 — frozen Phase 1 binary contract

```text
DOCUMENT_ROLE: NORMATIVE_M6_0_CHANGE_RECEIPT_V1_CONTRACT
CONTRACT_REVISION: 2026-08-06.1
CONTRACT_STATUS: FROZEN_M6_0_CONTRACT_COMPLETE
SCHEMA_VERSION: 1
DIGEST: BLAKE3-256
AUTHENTICATION: REGISTERED_KEYED_BLAKE3_256
BYTE_ORDER: LITTLE_ENDIAN
DECODER: PURPOSE_BUILT_BOUNDED_EXACT_EOF
M6_0_STATUS: COMPLETE
M6_1_STATUS: NOT_STARTED_AUTHORIZED_TO_BEGIN_LATER_EXPLICIT_EXECUTION_TASK
```

This is the sole byte-level authority for `ChangeReceiptV1`. The receipt is
bounded authenticated **construction evidence** for one immutable source
snapshot relative to one store-held `AcceptedBinding`. It is not logical
filesystem truth, an authority capability, a payload checksum, a read-time
dependency, or permission to traverse a Workspace.

The mechanically regenerated structural and receipt vectors are
[M6_0_GOLDEN_VECTORS.md](M6_0_GOLDEN_VECTORS.md). The documentation-only
reference generator is
[`m6-vectors`](m6-vectors/README.md). Later product implementation must pin:

> **DOCUMENTATION-ONLY VERIFICATION TOOLING — NOT PRODUCT IMPLEMENTATION.**
> The generator is independent of, and imports no code from, the read-only
> product checkout.

```toml
blake3 = { version = "=1.8.5", default-features = false }
```

No JSON, CBOR, Protobuf, bincode, Serde-layout, self-describing value tree, or
unbounded generic decoder is permitted for this codec.

## 1. Primitive grammar

All integers are unsigned little-endian. `u8`, `u16`, `u32`, and `u64` have
their ordinary 1-, 2-, 4-, and 8-byte widths. IDs and digests are opaque fixed
byte arrays; they are never textual UUIDs or hex in the binary form.

| Name | Exact bytes |
|---|---:|
| `sandbox_id` | 16 |
| `issuer_id` | 16 |
| `custody_id` | 16 |
| `source_snapshot_id` | 16 |
| `AcceptedBindingTokenV1` | 32 |
| BLAKE3-256 digest/authenticator | 32 |

Paths are exact strict OD-01 UTF-8 relative paths: nonempty, `/` separated,
at most 4,096 bytes and 256 components, each component at most 255 bytes, with
no NUL, leading/trailing/repeated slash, empty component, `.` or `..`.
Canonical path comparison is component-aware unsigned-byte comparison, not
host, locale, or Unicode-normalized ordering.

Every checked addition, multiplication, range end, sequence successor,
capacity computation, and offset computation uses mathematical integer
semantics and must fit its destination type. Overflow is a typed failure;
wrapping and saturation are forbidden.

## 2. Exact envelope

The envelope domain is the 12 bytes `"ESV2-CHGREC\0"`. The exact field order
is:

```text
offset  width  field
0       12     domain = "ESV2-CHGREC\0"
12      2      schema_version:u16 = 1
14      4      total_encoded_bytes:u32
18      16     sandbox_id
34      16     issuer_id
50      8      issuer_instance_epoch:u64
58      16     custody_id
74      16     source_snapshot_id
90      32     base_accepted_binding_token:AcceptedBindingTokenV1
122     8      source_generation:u64
130     8      sequence_first:u64
138     8      sequence_final:u64
146     1      finality:u8
147     1      overflowed:u8
148     4      facts_count:u32
152     4      facts_encoded_bytes:u32
156     N      exact concatenated PathFactV1 records
156+N   32     coverage_digest
188+N   1      authentication_scheme:u8
189+N   4      issuer_key_id:u32
193+N   32     issuer_authentication
225+N   0      exact EOF
```

`total_encoded_bytes` is the entire envelope including domain, length field,
coverage digest, authentication framing, and authenticator. It must equal
`225 + facts_encoded_bytes`, equal the received slice length, be at least 225,
and be at most 1,048,576. `facts_encoded_bytes` must equal the exact sum of all
declared `fact_encoded_bytes` values and end exactly where the coverage digest
begins. The declared count must be consumed exactly. No padding, extension,
unknown field, omitted trailer, or trailing byte is legal.

`finality` has two recognized values: `0x00 = NON_FINAL` and `0x01 = FINAL`.
`overflowed` has two recognized values: `0x00 = COMPLETE` and
`0x01 = PRODUCER_OVERFLOW`. Other values are malformed. Only
`FINAL/COMPLETE` is eligible for the incremental path; authenticated
`NON_FINAL` or `PRODUCER_OVERFLOW` produces a typed
`FullEnumerationRequired`, not incremental acceptance.

`authentication_scheme` has one V1 value:
`0x01 = REGISTERED_KEYED_BLAKE3_256`. `issuer_key_id` is nonzero. Unknown
schemes and key ID zero are rejected before source effects.

## 3. AcceptedBinding token and authority law

`AcceptedBindingTokenV1` is 32 cryptographically random opaque bytes issued by
the catalog owner after the complete acceptance outcome is durable. All-zero
is reserved and invalid. It is not `VersionIdV1`, a serialized
`AcceptedBinding`, a MAC, a bearer authorization for application operations,
or a reconstructible value.

Receipt validation resolves the token only against store-held accepted
authority under the receipt's exact `sandbox_id`. Resolution returns an
internal non-serializable `AcceptedBinding` capability only if the catalog
record is accepted, unrevoked, closure-authenticated, and under current
locator/custody proof. A caller that possesses or guesses the raw 32 bytes
cannot construct, deserialize, or mint that capability. Token resolution is
revalidated immediately before admission effects. Missing, foreign-sandbox,
revoked, stale-locator, unaccepted, or raw caller-created tokens fail closed.

## 4. PathFactV1 grammar

Each fact begins with its total byte length, including the length field:

```text
fact_encoded_bytes:u32
fact_kind:u8
before_entry_kind:u8
after_entry_kind:u8
presence:u8
path_len:u32
path[path_len]
if presence & 0x01: prior_path_len:u32 || prior_path[prior_path_len]
if presence & 0x02: before_generation:u64
if presence & 0x04: after_generation:u64
if presence & 0x08: before_size:u64
if presence & 0x10: after_size:u64
if presence & 0x20: metadata_digest[32]
range_count:u16
ordered range_count * (start:u64 || length:u64)
exact fact EOF
```

Presence bits are exactly:

| Bit | Meaning |
|---:|---|
| `0x01` | `prior_path` present |
| `0x02` | `before_generation` present |
| `0x04` | `after_generation` present |
| `0x08` | `before_size` present |
| `0x10` | `after_size` present |
| `0x20` | `metadata_digest` present |
| `0x40`, `0x80` | reserved; must be zero |

Entry kinds are `0x00 = ABSENT`, `0x01 = REGULAR_FILE`,
`0x02 = DIRECTORY`, and `0x03 = SYMLINK`. Fact kinds are:

| Tag | Kind | Exact before/after law |
|---:|---|---|
| `0x01` | `CREATE` | before `ABSENT`; after non-absent |
| `0x02` | `MODIFY` | before and after `REGULAR_FILE` |
| `0x03` | `DELETE` | before non-absent; after `ABSENT` |
| `0x04` | `RENAME` | same non-absent before/after kind; `prior_path` required |
| `0x05` | `METADATA` | same non-absent `REGULAR_FILE` or `DIRECTORY` kind |
| `0x06` | `TRUNCATE` | before and after `REGULAR_FILE` |
| `0x07` | `SPARSE_MAP` | before and after `REGULAR_FILE` |
| `0x08` | `REPLACE` | both non-absent; kinds differ, or both are `SYMLINK` |

`prior_path` is forbidden except for `RENAME`, where it is required and must
differ from `path`. `before_generation` is required exactly when the before
kind is non-absent; `after_generation` is required exactly when the after kind
is non-absent. Both generations are nonzero. When both are present,
`after_generation` must be strictly greater than `before_generation`, including
for `RENAME`. V1 deliberately has no equal-generation pure-rename exception:
one after-metadata digest and optional range hints cannot independently prove
unchanged before-content and before-metadata, so permitting equality would make
the canonical validator depend on unstated external evidence. This stricter
monotone rule is safe, deterministic, and executable.

`before_size` is required exactly when the before kind is `REGULAR_FILE` and
`after_size` exactly when the after kind is `REGULAR_FILE`; each is at most
8,589,934,592. The metadata digest is always required. It describes the after
entry, or the before entry for `DELETE`, using:

```text
BLAKE3-256(
  "ESV2-CHGMETA\0" || u16-LE(1) || entry_kind:u8 ||
  mode_present:u8 || [portable_mode:u16-LE]
)
```

`mode_present` is `0x01` exactly for regular files and directories and `0x00`
exactly for symlinks. A present mode is `0x0000..0x0fff`; the structural root
sentinel `0x1000` is never legal receipt metadata. No other byte is legal.
For `DELETE`, `entry_kind` and mode refer to the before entry; otherwise they
refer to the after entry. This digest is event/custody coverage, not logical
truth. Validation obtains the selected kind and portable mode from the
independently held immutable-source fact (the authenticated deletion witness
for `DELETE`, otherwise the immutable after-source view), rejects an illegal
kind/mode combination or `0x1000`, recomputes the exact `ESV2-CHGMETA` digest,
and requires equality with the receipt. The receipt's 32 bytes never supply
their own authority. Construction and validation therefore both require the
same independently held portable source fact before any effect.

`fact_encoded_bytes` is at least 47, no more than 16,384, must fit wholly
inside `facts_encoded_bytes`, and must equal the exact bytes consumed by the
legal field combination. The 47-byte defensive minimum is below the smallest
currently legal semantic fact; the exact parser, not padding, determines the
actual minimum. The largest semantically reachable V1 fact is exactly 9,298
bytes: a regular-file rename with two 4,096-byte paths, both generations, both
sizes, metadata, and 64 ranges. The 16,384-byte limit is therefore a defensive
framing ceiling, not a promise that padded or extension bytes are legal.
Unknown tags, illegal presence combinations, or fact-local trailing bytes
fail.

## 5. Fact order, duplicates, renames, and coalescing

Facts describe the net change from the exact base binding to the exact source
snapshot. Producers must coalesce repeated events for one logical path before
encoding. Creation followed by deletion disappears. Deletion followed by
creation becomes `REPLACE` when kinds differ, `REPLACE` for a same-kind
symlink target change, or `MODIFY`/`METADATA` as appropriate for same-kind
regular files and directories. A same-kind regular-file replacement is
represented by `MODIFY`, `TRUNCATE`, or `SPARSE_MAP`; a same-kind directory
change is represented by child facts and, when its portable mode changes, a
`METADATA` fact. Rename chains are reduced to original source and final
destination.

The fact sequence is strictly increasing by this tuple:

```text
(path components under OD-01 unsigned-byte order,
 fact_kind numeric tag,
 prior_path components under the same order, with absent before present)
```

Primary `path` values are unique, so the second and third tuple members are a
canonicality check rather than permission for duplicate primary paths. Rename
source paths are also unique. A non-rename primary path must not equal any
rename source. A rename source may also be a rename destination only when the
entire connected component is a closed permutation cycle; this permits swaps
without permitting an uncoalesced chain. Each cycle is encoded once through
its destination-sorted facts. Any other duplicate, source fan-out,
destination fan-in, intermediate rename, or ambiguous create/delete/replace
composition fails closed.

The issuer's certified event interval is authoritative for complete path
coverage. `facts_count` need not equal the number of raw events because the
facts are the deterministic coalesced net result. Zero facts is valid for an
authenticated no-change checkpoint and does not create a new logical
authority by itself.

An issuer may be registered for incremental authority only when its event
interval completely observes every mutation channel that can affect the
snapshot: buffered writes, direct I/O, memory-mapped writes, create/unlink,
rename, truncate, hole punching and sparse-map changes, metadata changes, and
custody transitions. The issuer must either close that interval against the
immutable named snapshot or emit `NON_FINAL`/`PRODUCER_OVERFLOW`; a journal or
interceptor that can miss any such channel is ineligible. Sparse/range events
never replace the required changed-path fact. Crash recovery may declare an
interval complete only from durable registered-issuer state bound to the same
issuer epoch, custody, snapshot, generation, and sequence interval.

## 6. Range law

Ranges are non-authoritative optimization hints. They may be present only
when the after kind is `REGULAR_FILE` and the fact kind is `CREATE`, `MODIFY`,
`RENAME`, `TRUNCATE`, `SPARSE_MAP`, or `REPLACE`. `DELETE` and `METADATA` have
zero ranges. Each range is half-open `[start, start + length)`:

- `length` is nonzero;
- `start + length` is checked and must fit `u64`;
- the end is no greater than `after_size`;
- ranges are ordered by increasing `start`;
- overlap is forbidden; and
- adjacency is forbidden because adjacent ranges must be coalesced.

Thus for consecutive ranges, `previous_end < next_start`. Empty range lists
are valid. Receipt acceptance proves changed-path coverage, not sub-file byte
coverage; ranges remain hints until a separately frozen and qualified issuer
protocol proves an exact old-to-new mapping and produces `ResyncWitnessV1`.
Missing or unusable hints take the counted full-changed-file path, never an
unsafe partial-file assumption.

## 7. Coverage digest and authentication

The coverage preimage is exactly:

```text
"ESV2-CHGCOV\0" || u16-LE(1) ||
exact envelope bytes at offsets [14, 156 + facts_encoded_bytes)
```

The slice starts at `total_encoded_bytes` and ends after the final fact; it
therefore includes total length, every identifier and generation/sequence
field, finality/overflow, both fact framing fields, and every fact byte. It
does not include the receipt envelope domain/schema a second time, the
coverage digest, or authentication trailer. `coverage_digest` is the ordinary
unkeyed BLAKE3-256 digest of that preimage.

The authentication preimage is exactly:

```text
"ESV2-CHGMAC\0" || u16-LE(1) ||
exact envelope bytes at offsets [0, 193 + facts_encoded_bytes)
```

That slice includes the envelope domain/schema, `total_encoded_bytes`, every
fact, `coverage_digest`, `authentication_scheme`, and `issuer_key_id`, but
ends before `issuer_authentication`. Authentication is BLAKE3 keyed mode with
the registered 32-byte key; the output is all 32 bytes. V1 does not truncate.

The registry lookup key is
`(sandbox_id, issuer_id, issuer_instance_epoch, issuer_key_id)`. The registry
record binds the exact 32-byte secret key, allowed custody authority, enabled
state, and epoch history. A key is never selected from receipt content alone.
The computed and received authenticators are compared over all 32 bytes with a
vetted constant-time primitive or an XOR/OR accumulation that has no
data-dependent early return. Authentication failure disclosure does not say
which byte differed.

## 8. Limits, allocation, and one-pass processing

| Limit | Exact V1 value |
|---|---:|
| Total encoded receipt | 1,048,576 bytes |
| Facts | 4,096 |
| One path or prior path | 4,096 bytes |
| One fact | 16,384 encoded bytes |
| Ranges per fact | 64 |
| Total ranges | 16,384 |
| Decoder-owned allocated capacity | 2,097,152 bytes |
| Error disclosure | 4,096 bytes |

The decoder makes one forward bounded pass over one borrowed receipt slice.
It parses and validates framing, paths, fact semantics/order, ranges, coverage,
and authentication without an unbounded value tree. Paths, fact bodies, and
authentication inputs remain borrowed slices where safe. The authentication
hasher is initialized before offset 0; the coverage hasher is initialized
before offset 14; each consumed byte is fed once to every applicable live
hasher. After the final fact the coverage output is checked and then its
received bytes plus the authentication tag/key ID are fed to the continuing
authentication state. No second facts scan or Workspace scan is permitted. A
bounded rename index may retain offsets/lengths, not path
copies. No receipt path opens source payload.

Before receipt transport allocation, decoder-owned map/index allocation, or
authentication, the shared `B_s` ledger precharges the exact applicable
capacities. Cancellation and deadline are checked as part of this reservation.
No parser collection or MAC work starts until it succeeds. The worst-case
combined receipt reservation is:

```text
owned receipt/input capacity       1,048,576
decoder-owned allocated capacity   2,097,152
bounded error disclosure               4,096
                                  ---------
RECEIPT_PROCESSING_MAX             3,149,824 bytes
```

If the input is caller-borrowed and already charged to the same ledger, its
capacity is not charged twice; the original live charge remains. Every actual
allocation is reconciled to allocator capacity before visibility and the
sum of decoder-owned capacities must not exceed 2,097,152. Failure to reserve
returns `ReceiptResourceRefused` before allocation/authentication/source or
durable effects. The 3,149,824-byte maximum fits the 33,554,432-byte minimum
`B_s` profile and grants no right to consume non-borrowable recovery headroom.

Queue descriptors are fixed bounded IDs/counters only: sandbox/issuer/custody/
snapshot IDs, binding token, epoch/generation/sequences, receipt byte count,
fact/range counts, and receipt digest. They never retain receipt bytes, path
slices, path copies, decoded fact lists, range lists, or source handles.

Receipt CPU is `O(total_encoded_bytes)` and decoder memory is bounded by the
table above. It performs no workspace traversal, file enumeration, payload
open/read/hash, materialization, or payload digest. The ordinary valid
incremental path therefore preserves zero opens, reads, and hashes for every
unchanged file.

## 9. Generation, sequence, restart, ABA, replay, and custody

The store retains issuer acceptance state per `(sandbox_id, issuer_id)`:
current registered epoch, last accepted source generation, last accepted
sequence final, accepted receipt digest/outcome records, and custody handoff
state.

For a new accepted receipt in one epoch:

- `issuer_instance_epoch`, `source_generation`, `sequence_first`, and
  `sequence_final` are nonzero;
- the epoch equals the currently registered epoch and is never reused;
- `source_generation` is strictly greater than the last accepted generation;
- `sequence_first` equals checked `last_sequence_final + 1`, or equals 1 for
  the first receipt of a newly registered epoch;
- `sequence_final >= sequence_first`; and
- the exact source snapshot and custody handle remain immutable and live
  through final revalidation immediately before admission effects.

Sequence wrap is forbidden. When the prior final sequence is `u64::MAX`, the
issuer must enter a separately registered epoch through a durable custody
handoff; it cannot wrap to zero. Restart creates a strictly newer registered
epoch, new nonreused custody and snapshot identities, and a key/epoch binding.
Registering the epoch requires a durable handoff from the old owner or an
independent complete-source custody establishment. Process identity, PID,
wall-clock time, a reused identifier, or an unregistered key cannot establish
continuity.

Replay of the exact same authenticated receipt bytes after a conclusive stored
outcome is idempotent and returns the recorded outcome without repeating
source or durable effects. The store identifies it by BLAKE3-256 of the exact
receipt plus the issuer tuple. The same epoch/generation/sequence/snapshot
tuple with different bytes is `ReplayDivergence`, even if independently
authenticated. A replay whose prior outcome is unknown resumes only through
the catalog-owned typed recovery record. Stale generation, sequence gaps,
epoch rollback/reuse, snapshot reuse after custody transfer, divergent replay,
and token ABA never mint authority.

## 10. Typed pre-effect disposition

Receipt processing is a pure gate before source and durable effects:

| Class | Examples | Required disposition |
|---|---|---|
| `ReceiptMalformed` | domain/schema/length/count/path/presence/tag/order/range/EOF violation | Refuse; no fallback is automatically launched |
| `ReceiptLimitExceeded` | receipt/fact/path/count/range/owned-capacity/error cap | Refuse; no automatic traversal |
| `ReceiptUnauthenticated` | unknown/disabled key, bad coverage digest, bad authenticator | Refuse; bounded generic disclosure only |
| `ReceiptAuthorityRejected` | raw/unresolved binding token, foreign sandbox, revoked binding, epoch/replay divergence | Refuse and security-count; no automatic traversal |
| `ReceiptCustodyRejected` | missing/transferred/mutable snapshot custody or failed immediate revalidation | Refuse; no construction effects |
| `FullEnumerationRequired` | authenticated non-final/producer-overflow receipt, sequence gap, unavailable complete event coverage | Return typed reason; do not traverse inside receipt processing |
| `FullChangedFileRequired` | valid changed-path coverage but no usable authoritative ranges/resync witness | Continue only after the separately bounded full-changed-file reservation succeeds |
| `ReceiptResourceRefused` | receipt or later fallback reservation unavailable/deadline/cancelled | Retryable refusal before effects |

A caller may proceed from `FullEnumerationRequired` only through a separately
authorized complete-enumeration operation that acquires stable snapshot
custody and precharges all traversal/order/spill/changed-payload/pack/proof
resources before opening or reading the source. Invalid or unauthenticated
input never turns into attacker-triggered whole-workspace work.

`FullChangedFileRequired` is returned only after the receipt itself is valid,
authenticated, authoritative, current, and complete for changed-path coverage,
but the changed regular file has no accepted authoritative resynchronization
witness. Receipt ranges alone never suppress this result. It starts no I/O;
the caller must first acquire the separately bounded full-changed-file
reservation. `ReceiptResourceRefused` is returned before effects when either
the receipt precharge or that later fallback reservation is unavailable,
cancelled, or past deadline.

## 11. Required executable evidence

The reference vector gate must regenerate and compare:

- valid minimal/no-change and nontrivial authenticated receipts;
- exact 1-MiB, 4,096-fact, 4,096-byte-path, 64-range-per-fact, and
  16,384-total-range boundaries, plus the exact 9,298-byte maximum reachable
  legal fact;
- length/count/presence/tag/order/duplicate/rename/overlap/adjacency/range-
  overflow/sequence/generation/custody/replay/coverage/authentication/cap/EOF
  failures, plus authenticated CHGMETA failures for arbitrary digest, wrong
  source kind, wrong source mode, and reserved `0x1000` mode;
- all structural identity domains, the root sentinel and child-mode law,
  malformed/unknown/trailing typed objects, typed-edge closure, invalid UTF-8
  and NUL names/targets, and occupied same-ID/different-byte collision
  handling;
- executable `FullEnumerationRequired`, `FullChangedFileRequired`,
  `ReceiptResourceRefused`, raw-token/revocation/key/epoch/custody/ABA,
  immediate-revalidation, idempotent/divergent/unknown-outcome replay, and
  cap dispositions; and
- exact valid preimages, lengths, IDs, coverage digests, authenticators, full
  receipt bytes, mutation recipes, mutated-byte digests, and typed outcomes.

Any later byte change requires a new schema/domain or an explicit M6.0
reopening. It invalidates PRE-03 through PRE-05, PRE-10, PRE-12, and every
dependent seal; reviewers do not redesign V1 in place.
