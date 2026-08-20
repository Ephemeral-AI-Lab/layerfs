# LayerFS Implementation Details

This directory contains detailed phase specifications, experiments, audits,
handoffs, and optimization evidence. The repository root intentionally keeps
only the stable project entry documents:

- [Restart specification](../SPEC.md)
- [Architecture](../architecture.md)
- [Implementation plan](../IMPLEMENTATION_PLAN.md)
- [Note to read after Phase 4](../NOTE_TO_READ_AFTER_PHASE_4.md)

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

- F2-v3 construction proof: retained.
- F3 insertion grouping and causal diagnostic: terminal `NO-GO`; F2-v3 remains
  the accepted implementation.
- F4: not started or authorized by this reorganization.

See the [WP4-M rolling progress ledger](phase-4/wp4m/progress.md), the
[terminal F3 report](phase-4/wp4m/f-series/f3/report.md), and the
[post-F3 planning note](phase-4/wp4m/f-series/f3/read-after.md).

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
