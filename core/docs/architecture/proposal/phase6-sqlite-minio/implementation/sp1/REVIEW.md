# SP1 packet review and documentation checks

> **Status: Dated planning checkpoint; not release evidence or a product contract.**

2026-10-02, source audit parent bfbf48ea7694e8450bd47a3bf2c297fdac36badc.
The implementation/test authors had nonoverlapping files; an independent
read-only source reviewer checked the combined packet. Root integrated findings.
No runtime build, product test or measurement was performed by this review.

## Findings incorporated

| Finding | Source evidence | Resolution |
| --- | --- | --- |
| The suggested165-value public pool fixture did not exist | `metadata_pool.rs` documents the100-row public leaf cap and groups at1/101/201 | Test plan reuses the actual named fixture conditionally;165 remains policy capacity. |
| Writer base reads need self charging | C2 selector calls `resolve_dependency`; ordinary reads exempt only the requested object | Proposed seam has explicit dependency resolution; both routes share the existing bounded resolver. |
| Publication scope alone omits captured pack ceiling | C2 lookup/resolver also bound pack order before cache use | Scope carries a separate captured ceiling and private save/generation identity. |
| Private pooled groups need actual visibility/cache witness | Existing same-save8shared+2new and unrelated-reader pool tests | Fold these into writer/refusal witnesses, with checks before cache answers and no per-value network request. |
| Refactored connections could accidentally change persistence | Core policy and prototype catalog MEMORY/OFF/zero busy profile | State noWAL/noSync/noBusyRetry directly in the implementation contract. |
| Depth2 permits two edges; it does not test that boundary's refusal | Existing role depth admission | Reader cap1 rejects the two-edge stored fixture; writer cap2 third-edge candidate selects FULL. |
| Wall-clock metadata prevents presealed live FS roots | Engine now() and current FUSE timestamp surface | Independent content roots/partitions retained; fullFS root proof requires deterministic supported metadata, otherwise INCOMPLETE. |

The checked profile retains128KiBexclusive, CDC8/16/32KiB, delta8/4/8,
payload/group levels3/1, payload dependency budgets512/256KiB and16/1MiB codec
arenas. The34MiB-2 simultaneous writer subtotal exposes a charging/admission
gate under current Workspace16MiB; it is neither a measured peak nor automatic
permission to increase that budget. Allocation/profile decisions precede source
mutation and are independently required before supported physical claims.

## Handoff boundaries

Implementation signatures/new witness IDs are prospective. Current C2 SQLite
owner behavior is retained; MinIO catalog/wire allocations must be recorded at
SP1.0 before dependents. Four coalesced witnesses precede stride collection;
new MinIO history profile is unregistered and unsupported by existing CLI flags.
The three original schedule/budget/oracle identities and their historical verdicts
remain unchanged. No new benchmark framework or whole-workspace speed claim.

Root documentation validation checks all local links in the staged packet,
existing command/parser/target references, source settings, historical receipt
SHA256 and arithmetic, whitespace and exact owned staging. Full product checks
remain prospective because this checkpoint changes documentation/evidence only.
Exact production LOC is measured separately on first-parent/final staged trees;
the issue checkpoint records resulting publication and checks.

## Strict-split owner revision — 2026-10-02

Source audit parent: `8c926b9392f3636ae156236dc26d0510ee069d8d`. The preceding
review records the original proposal; it is not strict-split runtime evidence.
Implementation/test authors revised separate owned files, and a read-only
reviewer checked the new storage/provenance/visibility boundary against source.

- Global SQLite now stores all canonical filesystem metadata and pooled values/
  groups; MinIO uploads regular-file whole payloads and CDC chunks only.
- Attribute values emit Chunk/ExtentLeaf/FileState. Logical producer/reader
  provenance, dual-domain locations and per-use mapping references preserve
  exact IDs without SQL file-payload fallback or role-only classification.
- The SQL metadata pack representation retains complete bounded pack envelopes
  for the existing decoder; pooled value groups keep their distinct existing
  framing. Bare group bytes cannot substitute for a pack envelope.
- Domain/body/generation cache identities and placement checks before canonical
  cache hits prevent SQL bytes from satisfying a missing payload location.
  The dual-use vector uses an actual sealed CDC chunk from a large regular file
  as an attribute value; manually encoding a small Chunk is not ordinary file
  producer provenance. Both cache-read orders are covered prospectively.
- Old S2-first wording was corrected: clean strict SP1 can proceed without
  importing unfinished S2 or the stopped old SP1 implementation. Published old
  fixture-only evidence may be reused by its actual source/hash/scope.
- Payload chains retain intermediate/final canonical hashing. Existing pooled
  reads authenticate group digests/edges and the requested final canonical leaf;
  witnesses independently read/hash every retained pool leaf. The packet does
  not claim existing pooled decoding hashes every intermediate canonical leaf.
- The four revised witnesses require isolated metadata/pool MinIO GET delta0,
  exact dual-use IDs, private/public cache visibility, payload chain parity and
  public generic SDK/FUSE history. They remain prospective NOT_RUN.
- Resource accounting separates SQL metadata, payload and shared codec/index
  ownership. Existing quotas are unchanged; physical fit remains open.

Documentation validation checks local links against the final staged snapshot,
all historical receipt-copy hashes/lengths and ratio arithmetic, owned diff
whitespace and exact staging. No product tests, builds, provider measurements
or performance qualification are claimed by this documentation revision.
