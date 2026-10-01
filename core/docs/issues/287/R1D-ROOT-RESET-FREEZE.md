# R1d profile4 known roots retirement and fixed-row reset

> Prospective source contract, 2026-10-01. Published parent
> `53b6bf741a5693f4d00ec98b914ce34645ee9ab3`; the preceding scoped graph
> layout is in the same root-coordinated implementation source. Checks are unrun.

SC-03/05/07 require exact bounded completion and retained failure custody.
Storage owns new root_retire.rs, root_reset.rs and root_session.rs, the small
session/authority/status/module integrations and external construction_reset
proofs. Content's StateLedger gains ordinary Clone for a real retained proposed
transcript. Other namespace/Store/Server/Bridge files remain independently owned.

This changes successful private profile4 completion semantics only. Original
LFCS1-4 field/schema/header/record bytes remain unchanged; profiles1-3 retain
logical completion and explicit native release. No pool/checkout/rebind is added.

Public real owner operations:

- `retire_roots_window(&StateSeal) -> RootRetirementProgress`: exact terminal
  sealed roots, one at-most128-record/header-inclusive64KiB transaction. A
  producer/cursor cannot access roots after the first retirement starts.
- `reset_completed_roots(&StateSeal)`: only after exact full retired transcript,
  empty primary/ordinal/site/graph/partial indexes, no pending attempts, no held
  admitted graph acknowledgements, known autocommit and native identity/allocation.
- Existing `complete_phase(scope)` on a successful sealed profile4 owner loops
  the first operation to exact completion, then performs the second before the
  actual C1 completion returns and canonical root emission proceeds. Unsealed
  unsuccessful construction keeps its existing known release path.

Retirement retains the original terminal scope/seal, maximum key, acknowledged
StateLedger and its count/byte/last-key continuation. Before deletion it owns a
bounded exact expected row window and proposed transcript. Rows must have exact
consecutive original ordinals and increasing keys. DELETE names key, ordinal and
root and must affect exactly one row. The final replay seal must equal the original
terminal seal before final deletion. Progress advances after known COMMIT and
native allocation observation; COMMIT Unknown preserves expected/proposed rows,
old/proposed transcript and credit, denies another mutation/reset/refund/adoption.

Each window has at most128 records and88+128*63=8,152 encoded bytes, below64KiB.
Actual Rust StateRecord capacities, proposed StateLedger and boxed retirement
metadata receive leases from the existing scoped64KiB graph working class before
allocation. Its Graph has no pending solver attempt during roots. No second
native file, independent full S reservation or native class increase occurs.

The original session_owner sealed totals/digest remain fixed through retirement;
bounded in-process continuation is the authority. There is no recovery/adoption
of leftover private files. Final reset updates only four existing fixed owner
rows (session/site/graph/solver) to their exact initial empty mutable values;
header, scopes, source subject and selected budget/declarations remain. No bulk
DELETE, schema recreation, DROP, VACUUM, retry or repair is permitted.

Known clean is a separate acknowledged terminal state and retains the original
terminal seal. Verification checks exact reset fixed rows/emptiness, rather than
reconstructing a former seal from empty SQL. All original scopes are ended.
Native close/unlink/removal/refund stays the explicit existing release operation;
reset does not refund native credit. Independent Store/C5 uncertainty neither
poisons known scratch completion nor authorizes uncertain scratch deletion.

Required exits: zero/1/128/129/full-declared root boundaries, partial continuation
and no stale read, original digest/key/ordinal corruption refusal, all indexed
projections empty, exact fixed rows and unchanged native class, real held-reader
COMMIT Unknown with exact proposed continuation retained, explicit native cleanup,
and ordinary current C1/Server known completion. Root runs coherent owning checks
once after source integration. No pooling or speed claim belongs to this delivery.
