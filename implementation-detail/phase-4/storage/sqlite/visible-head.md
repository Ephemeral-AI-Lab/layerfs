# Phase 4 SQLite visible-head migration specification

- Status: narrow migration authority for WP4-M and WP7; implementation pending
- Date: 2026-08-17
- Branch: `codex/empty-worktree`

## 1. Purpose and authority

This specification is the single exception permitted by
`../../rollback/spec.md` section 10.2 to the
rule that Phase 4 preserves the current SQLite schema.

The exception is necessary because schema version 1 stores only
`layerfs_store_meta.visible_root`. The frozen logical mapping requires one
atomic complete value:

```text
VisibleHead {
  generation: u64,
  child: RootId,
  transition: DeltaId,
  validation_receipt: [216]byte,
}
```

Version 1 also stores `parent_root` in a row keyed only by `root_id`. That is
incompatible with the Phase 3 rule that root identity is content-only and the
same root may be reached by different transitions.

This document authorizes only the minimum schema revision needed to store the
promoted mapping. It does not authorize a migration framework, migration CLI,
format negotiation API, multiple production profiles, or a database change.

## 2. Authorized schema revision

WP7 may replace SQLite schema version 1 with one schema version 2 after WP4-P
has selected exactly one mapping profile. Version 2 must:

1. retain the immutable canonical-object table and its unique `ObjectId`;
2. store root identity as a content handle with no parent in root identity;
3. store ancestry only in the authenticated Genesis/Change transition;
4. persist the store's exact 16-byte `store_instance_id`, 32-byte
   `validation_authority_id`, and checked `u64` `integrity_epoch`;
5. store either no visible head or all four `VisibleHead` fields together;
6. represent `generation` and `integrity_epoch` without narrowing signed
   conversion, for example as exact 8-byte big-endian blobs;
7. constrain IDs to 32 bytes and the receipt to exactly 216 bytes at both the
   schema and decode boundaries; and
8. change the complete head tuple in the same SQLite COMMIT that publishes its
   root and transition.

The protected validation key is engine-private authority, not public schema.
Until WP7 proves its custody and mutation/epoch rules, cross-reopen receipt
reuse remains disabled: reopen performs a full scrub or returns
`ValidationAuthorityUnavailable`. Same-open receipt use does not grant
cross-reopen authority.

WP4-M may use an isolated candidate-only version-2-shaped database solely for
profile measurement. Each candidate database and receipt must carry a private,
domain-separated candidate profile ID. The production engine must not open a
candidate database, and candidate rows remain `qualification=false`.

## 3. Version-1 handling

Version-1 roots and opaque delta payloads do not contain enough information to
reconstruct the frozen Phase 3 mapping. They must not be guessed, normalized,
or silently relabeled as version 2.

The only in-place version-1 upgrade permitted is an empty store for which, in
one transaction, all of the following are proven:

- `visible_root IS NULL`;
- `layerfs_objects`, `layerfs_roots`, and `layerfs_deltas` contain zero rows;
- the format marker, schema version, and durability profile are exactly the
  expected version-1 values; and
- the new version-2 store authority and no-visible-head state can be created
  completely.

Any non-empty version-1 store returns the exact typed
`SchemaMigrationRequired` result before mutation. Its bytes remain unchanged.
LayerFS does not yet have a semantic export/import path that can translate
those provisional roots and deltas, so this specification does not pretend to
provide one.

An empty-store upgrade either commits the entire new schema and authority once
or leaves version 1 authoritative. A failure after COMMIT dispatch follows the
same requested/prior/different/unresolved reconciliation rule as normal head
publication; it is never reported as a successful rollback without proof.

### 3.1 Exact open and migration classification

Version classification stops at the first applicable row. A failed open or
pre-dispatch migration never substitutes a guessed value, starts a scrub to
hide malformed structure, or mutates the store.

| Condition | Exact first and dominant result |
|---|---|
| filesystem or SQLite open/read failure | preserve the exact classified I/O cause |
| unknown format marker or schema version | `SchemaMismatch` |
| known schema with the wrong durability or mapping profile | `ProfileMismatch` |
| missing, partial, wrong-size, or noncanonical store ID, validation-authority ID, or integrity epoch | `InvalidRecord("store_authority")` |
| mixed-NULL visible head, malformed generation, wrong child/transition length, or receipt length other than 216 bytes | `InvalidRecord("visible_head")` |
| structurally complete receipt with invalid canonical grammar, bound tuple, or authenticator | `InvalidValidationReceipt` |
| well-formed store whose required protected validation authority is unavailable | `ValidationAuthorityUnavailable`, unless the caller separately requests a full scrub that does not use receipt authority |
| nonempty version 1 | `SchemaMigrationRequired` |

For these prepublication structural/open failures,
`FailureProvenance.first` is the row's exact cause and `reconciliation` is
absent. With no cleanup failure, `cleanup_first` is absent and `dominant`
equals `first`; an actual cleanup failure fills `cleanup_first` and dominates
only where the lifecycle contract explicitly permits it. The visible head,
schema bytes, rows, and authority remain unchanged. A structurally valid store
may be reopened after the missing external authority is restored or may run the
separately requested full scrub. Malformed structure remains fail-closed until
externally repaired; the failed open does not repair it.

## 4. Atomicity and failure rules

For every version-2 capture:

1. canonical objects, root, transition, and closure evidence are staged;
2. the complete new `VisibleHead` tuple is staged;
3. one COMMIT is the atomic visibility and durability boundary; and
4. a lost acknowledgement is reconciled against the complete tuple, including
   the byte-identical receipt; the frozen operation key is recomputed from the
   retained prior/request tuple and is not stored as a fifth head field.

Before COMMIT dispatch, a failure guarantees no visible-head change, although
authenticated immutable objects may remain unreachable residue. After
dispatch, the only outcomes are:

| Authoritative observation | Outcome |
|---|---|
| exact requested head | success; retain the first transport cause as diagnostic |
| exact prior head | original exact failure; publication proven absent |
| a different complete head | `PublicationConflict` |
| requested/prior/different cannot be established | `AmbiguousDurability`; only the identical idempotency key may retry |

The bounded failure record preserves `first`, `cleanup_first`,
`reconciliation`, and `dominant` exactly as frozen by
`../../mapping/logical-persistence.md` section 10.

## 5. Required tests

Before WP7 may use schema version 2 in production, direct tests must prove:

- a fresh version-2 store and an empty version-1 upgrade;
- a non-empty version-1 store returns `SchemaMigrationRequired` without any
  file or row mutation;
- every row in section 3.1, including unknown version/profile, partial
  authority, mixed-NULL head, malformed generation, wrong ID/receipt length,
  invalid receipt binding/authenticator, and unavailable validation authority;
- exact `first`, `cleanup_first`, `reconciliation`, and `dominant` provenance,
  zero mutation/publication, and the specified reopen/full-scrub behavior for
  every section 3.1 row;
- all-or-none NULL handling and exact byte lengths for every head field;
- generation and epoch values at `0`, `i64::MAX`, `i64::MAX + 1`, and
  `u64::MAX`, plus checked `u64::MAX + 1` rejection before mutation;
- empty-root -> nonempty-root -> the identical empty `RootId` under a different
  transition without a root-row conflict;
- one COMMIT publishes the complete tuple and no partial tuple is observable;
- injected failure before dispatch and each of the four post-dispatch
  reconciliation outcomes;
- exact genesis/prior idempotency-key vectors from the mapping, per-field
  divergence, invalid prior-tag rejection, and proof that the key is not a
  persisted head column;
- reopen with valid authority, unavailable authority, stale epoch, wrong
  store/profile/key, changed receipt bytes, and exact 215/216/217-byte receipt
  boundaries;
- preserved first/dominant causes and honest unreachable-residue custody; and
- the original DELETE-journal, `synchronous=FULL`, `temp_store=FILE`, and
  `mmap_size=0` profile remains unchanged.

## 6. Completion boundary

This specification authorizes the schema shape and migration behavior only.
It does not select K/F or page sizes, authorize candidate compatibility, or
claim performance. WP4-P must first select one mapping profile and delete all
candidate selectors. WP7 then implements this single production schema and
runs the tests above.
