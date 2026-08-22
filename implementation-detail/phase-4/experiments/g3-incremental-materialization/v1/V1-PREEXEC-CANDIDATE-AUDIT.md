# G3-v1 pre-execution candidate audit

Disposition: **REVISE_BEFORE_MEASUREMENT**

The independent post-implementation audit rejected the first candidate bytes
before the frozen campaign. At classification time both the v1 result root and
v1 lock were absent; no measured v1 row existed. This record is append-only and
does not amend or relabel the zero-row v1 dry run.

## Rejected candidate custody

| Input | SHA-256 before repair |
|---|---|
| `Cargo.lock` | `70c7f1079b6dcff927932d6e0072e5cd169cd2f49ea51c72f7f108d950adb8d8` |
| `crates/layerfs-engine/Cargo.toml` | `c08efcca5cd604ffd3aa03dabcf076666292cd309095c5e2b98b7226f32a5865` |
| `phase4_create_edit_benchmark.rs` | `c78738ab213c7438544abdf2a37131652813873e30077469d578624f86ce3cdb` |
| `phase4_g3_materialization.rs` | `85c6e455aca6c1a78ef9bd44d5ad0a2e61d121dedc9b5d5548412f884eec1cf7` |
| release candidate executable | `0517721f30ebe07c6f379e1edaaad6a0d783cf40e597e9916d7a9267b35d1e00` |

These bytes were never measured by a G3 campaign.

## Exact audit findings

The first repair pass closed the original retry, missing-authority, basic
cleanup, descriptor-kind, and early-consumption defects described below, but a
fresh re-audit still returned **NOT PASS** with these exact blockers:

- `P0-EVIDENCE-RECONCILIATION-COUNTERS`: full target/prior canonical
  authentication and raw comparison in reconciliation were left out of direct
  work counters, while temporary comparison buffers were left out of Q. v2
  separates six exact reconciliation fields—SQL queries/rows, BLOB reads,
  canonical bytes authenticated, source bytes compared, and reconciliation Q
  high-water—so the primary one-byte patch remains bounded without hiding the
  fault-recovery full-file work.
- `P0-RECONCILIATION-UNSTABLE-OBSERVATION`: reconciliation observed native
  identity only before streaming. A concurrent same-inode writer could change
  an already-read prefix, or a concurrent rename could replace the destination
  name after its descriptor was opened, and produce a false target/prior result.
  The repaired operation must require stable pre/post descriptor identity plus
  a final no-follow name observation proving the name still resolves to that
  descriptor; instability maps to `AmbiguousDurability`.
- `P1-RECONCILIATION-TERMINAL`: neither-target-nor-prior mapped to
  `PublicationConflict` where unreadable/wrong-kind/identity-unstable evidence
  must be `AmbiguousDurability`; the prior result was also rejected by the row
  and analyzers. v2 accepts exactly target/new or prior/old, while exact stable
  different bytes remain `PublicationConflict`.
- `P1-RENAME-TARGET-CLEANUP`: reconciliation target after a rename error
  unconditionally disarmed temp cleanup even when rename had not consumed the
  temp; prior reconciliation also replaced the original rename error with a
  generic publication conflict. Cleanup ownership must follow observed name
  consumption, and prior must preserve the original exact error.
- `P2-GLOBAL-FAULT-INJECTION`: the process-global clone-miss `AtomicBool` could
  be consumed by a parallel test or survive an unrelated native clone failure.
  Fault injection must be operation-local and single-use.

### P0 — retry accumulated mutations

`P0-RETRY-BASE`: patch retry preparation did not restore a fresh authenticated
parent candidate before each attempt. A retry could therefore apply the edit to
already mutated bytes rather than recomputing the exact parent-to-target result.
The output oracle might catch a particular final mismatch, but the mechanism
itself did not establish the required invariant. The repair recopies the
authenticated parent for every attempt and streams a complete equal-length
proof that all differing bytes lie within the single declared range before any
permit is minted.

### P1 — fallback, publication, cleanup, and precedence

- `P1-LOST-ACK`: the fault point did not model loss of acknowledgement at the
  rename/directory-durability boundary, and reconciliation did not enforce the
  complete (`target`,`new`) / (`prior`,`old`) truth table. The repair injects
  immediately after rename, before directory sync, performs fresh nonblocking
  no-follow complete canonical comparison against target then prior, reconciles
  rename/directory-sync ambiguity, and directory-syncs a reconciled target.
- `P1-AUTHORITY-FALLBACK`: missing seed, permit, key, or seed-stat authority
  could escape as a candidate error instead of an honest complete authenticated
  fallback. The repair makes every such qualification miss fall back and mint
  no authority.
- `P1-CLEANUP-OWNERSHIP`: named seeds and clone candidates did not acquire
  cleanup ownership immediately; clone reopen/identity failures could therefore
  leak reserved artifacts or escape instead of becoming a clean clone miss.
  The repair installs guards at creation and proves zero residue on every path.
- `P1-GATE-PRECEDENCE`: some qualification decisions followed scenario labels
  rather than observed filesystem/authority gates, so a real missing,
  symlink/wrong-kind, invalidated, count-changing, or clone-failure state could
  take the wrong route. The repair orders actual gates: no-follow kind
  preflight; logical authority; destination continuity including missing;
  length/reference-count; then clone availability.

### P2 — consumption and descriptor races

- `P2-PERMIT-CONSUMPTION`: a permit could be consumed before a usable clone was
  reopened and identity-verified. The repair consumes only after that proof;
  clone failure consumes zero.
- `P2-CAPABILITY-RECHECK`: the operation did not freshly revalidate all retained
  directory and seed descriptor facts. The repair rechecks directory
  device/inode and requires the seed to match its bound identity, be regular,
  read-only, unlinked, and exact-length.
- `P2-CLONE-REOPEN-RACE`: a reopened clone was not fully tied to the no-follow
  directory entry. The repair requires descriptor and entry device/inode
  equality and regular kind before patching.
- `P2-PUBLISH-NOFOLLOW`: publication lacked the strongest available
  `RENAME_NOFOLLOW_ANY` traversal rejection. The repair uses it.
- `P2-ORACLE-KIND-RACE`: independent hash/reconciliation opens could block on or
  follow an unexpected object before kind rejection. The repair opens
  nonblocking/no-follow and rejects nonregular descriptors before reading.

## v2 consequence

All repairs stay within the preregistered Attempt-B protected-seed/clone-patch
mechanism and do not introduce a new optimization variable. v2 may freeze only
after the re-audit blockers above are repaired; it then binds the repaired
four-file source set, once-built executable, every row, and final record. Any
further defect requires a fresh append-only version; v1 remains permanently
zero-row.
