# LayerFS Implementation Details

This directory contains detailed phase specifications, experiments, audits,
handoffs, and optimization evidence. The repository root intentionally keeps
only the stable project entry documents:

- [Restart specification](../SPEC.md)
- [Architecture](../architecture.md)
- [Implementation plan](../IMPLEMENTATION_PLAN.md)
- [Mem9/Drive9 layered-filesystem distillation](../research/mem9-drive9-layered-filesystem-distilled.md)

The sealed benchmark and custody evidence under `target/` remains at its
historical paths and is not reorganized here.

## Reading order

1. [Phase 1 — canonical objects](phase-1.md)
2. [Phase 2 — CDC, CAS, and logical content](phase-2/handoff.md)
3. [Phase 3 — copy-on-write trees and authenticated deltas](phase-3.md)
4. [Phase 4 — durable storage and WP4-M](phase-4/README.md)

The general evaluation specification is [evaluation.md](evaluation.md).

## Phase 2

- [Handoff](phase-2/handoff.md)
- [Full-ingest findings](phase-2/findings.md)
- [Rejected packed-CAS optimization](phase-2/opt-2-packed-cas.md)

## Phase 4

Phase 4 has its own [navigation index](phase-4/README.md) because it contains
multiple storage candidates, rollback records, mapping/algorithm contracts,
and the complete WP4-M milestone history.

Current controlling status:

- WP4-M: CP-0006 `PASS / COMPLETE`; its rows remain `promotion=false`.
- WP4-P: `COMPLETE / PASS`; WP4 complete; WP5 eligible/pending; overall Phase 4
  not complete.
- Post-promotion CP-0007: `PASS / RETAIN`; 100-MiB `+1` early/middle durable
  publication is 7.868417/6.946583 ms with unchanged canonical identities and
  honest suffix-linear scaling. WP4-P remains closed.
- CP-0008 scale diagnostic: `PASS`; 500-MiB `+1` early/middle publication is
  27.140916/15.102042 ms. K64/F64 remains retained under the current <=50-ms
  suffix-linear policy, not under a hypothetical 8–10-ms scale-independent
  SLA.
- CP-0009 current product baseline: `PASS / BASELINE`; the exact control binary
  covers durable write, same-count edit, warm/fresh logical materialization,
  tiny and returned-1-MiB ranges, reopen, and CP-0008-linked count-change
  guards in a 51-second package.
- K64/F64 + DIR256K: one compatibility-promoted profile; DIR256K remains an
  unmeasured fallback; production profile ID is `b0ebb845...f4ba1`; final
  TSV/golden-test hashes are `6de8c752...a7330` / `727fe668...49701`.
- Routine evidence: 27 rows at 1/10/100 MiB under a 120-second ceiling; no
  512-MiB or 100-GiB runtime fixture.
- F2-v3 construction proof: retained.
- F3 insertion grouping and causal diagnostic: terminal `NO-GO`; F2-v3 remains
  the accepted implementation.
- F4-A: terminal `NO-GO` from documentation checkpoint
  `83d085bd80e82ae22b4a9766f2fc8aed03501fb8`; no isolated removable mechanism
  passes the 33-ms/4-of-5 gate and no optimization is authorized.

See the [WP4-M rolling progress ledger](phase-4/wp4m/progress.md), the
[terminal F3 report](phase-4/wp4m/f-series/f3/report.md), and the
[Cursor Git research note](../research/cursor-git-at-any-scale.md), and the
[F4-A diagnostic](phase-4/wp4m/f-series/f4/report.md).

## Organization rules

- A phase with one detailed document remains one file.
- A phase or substep with multiple documents receives one folder.
- Each document has one canonical live path; there are no duplicate root
  copies or compatibility stubs.
- Historical `target/` evidence and custody snapshots remain immutable.
- Moving a historical report does not change its original disposition.
- Indexes summarize navigation only; milestone reports remain authoritative.

The one-time old-to-new path and hash record is
[path-map.tsv](path-map.tsv).
