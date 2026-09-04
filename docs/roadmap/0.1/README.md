# LayerFS 0.1.x roadmap

> **Status:** Current compatibility-preserving patch line.

## Problem statement

LayerFS 0.1.0 established a working real-FUSE and Docker product path plus an
immutable payload benchmark baseline. The benchmark surface is not complete,
and known optimization opportunities still need reproducible public-path
evidence. Adding new architectures before finishing this loop would make
regressions and improvements difficult to attribute.

## Goal

Use the 0.1.x line to complete LayerFS benchmarks and make evidence-driven
correctness, resource, and performance improvements against the existing
real-FUSE and Docker environment.

The fixed product path is:

```text
one host-native LayerStackStore
  -> public Rust SDK
  -> capability-authenticated container daemon
  -> real Linux FUSE
  -> fresh-process execution
  -> explicit Commit
  -> explicit End
  -> Store reopen and verification
```

Container and fixture preparation remain outside timed regions. Optimizations
must improve the ordinary production path; benchmark-only APIs, persistent
workers, prewarmed Workspaces, hidden caches, skipped integrity checks, weaker
acknowledgement, moved timing boundaries, and discarded valid samples are not
allowed.

## Files to read

- [Roadmap architecture](../architecture.md)
- [Benchmark contract](benchmarking.md)
- [Development guide](development.md)
- [v0.1.1 released history](0.1.1/README.md)
- [v0.1.2 completed release plan](0.1.2/README.md)
- [v0.1.3 draft](0.1.3/README.md)
- [v0.1.4 draft](0.1.4/README.md)

## Release sequence

- [0.1.1 released history](0.1.1/README.md)
- [0.1.2 completed release plan](0.1.2/README.md)
- [0.1.3 Workspace and single-Branch deduplication plan](0.1.3/README.md)
- [0.1.4 multi-history operation draft](0.1.4/README.md)
- [Development guide](development.md)
- [Benchmark contract](benchmarking.md)

| Release | Benchmark completion and optimization scope |
| --- | --- |
| v0.1.0 | Frozen payload baseline: create, small edit, EDIT16, prepend, and read. |
| v0.1.1 | Existing-directory initialization and namespace scaling through localized Commit and exact reopen. |
| v0.1.2 | Adapt `fs-bench-pro`, implement the universal edit engine, complete same-count and count-changing Docker/FUSE performance families, and measure total durable Store footprint. |
| v0.1.3 | Twelve families covering Workspace workloads, CAS/CDC, bounded single-Branch retained-history storage growth, and reliability; reuse the four-tier benchmark infrastructure. |
| v0.1.4 | Multi-Layer and multi-Branch Commit history, Fork, Add, Diff, conflict, and query scaling. |

Benchmark each admitted operation, but optimize only measured defects or
material opportunities. A passing operation may close as measured with no code
change.

## Append-only benchmark freeze

The registry grows from v0.1.0 through v0.1.4, but every admitted row is frozen
through 1.0.0. Its scenario ID, fixture generator and digest, public operation
sequence, timed boundary, acknowledgement semantics, correctness/reopen oracle,
sample rules, resource envelope, and result schema must not change in place.

Each release adds its rows and reruns all earlier registered rows. If a harness
defect requires a semantic correction, retain and deprecate the old row, add a
new scenario ID or schema version, and run both once when practical. A later
versioned campaign may change repetition count but cannot pool with or replace
the earlier distribution. Freeze workloads and evidence—not one machine's
observed latency as a universal value.

## Compatibility boundary

Every 0.1.x change preserves the released five-table Store schema, canonical
bytes and identities, CDC profile, public SDK and CLI behavior, daemon protocol,
visibility and acknowledgement semantics, and resource bounds. Only the item
that fails this boundary moves to 0.2.0.

## Acceptance criteria

The 0.1.x phase is complete when:

- [ ] The v0.1.0-v0.1.4 registered matrices pass with exact reopen proof.
- [ ] CPU, RSS, FUSE I/O, Store growth, object reuse, transaction maxima, and
  cleanup evidence are retained for every registered lifecycle.
- [ ] Every admitted scenario remains byte-for-byte and boundary-for-boundary
  compatible with its frozen definition, or has an explicitly versioned
  replacement that retains the earlier evidence.
- [ ] Every accepted optimization has a focused regression check and improves
  the production SDK/FUSE path.
- [ ] Selected failure paths leak no mount, container, process, output reader,
  spool, Workspace, or Branch lease.
- [ ] The final LayerFS-only campaigns have no unexplained regression.
- [ ] Remaining limitations and incompatible proposals are documented under
  the correct later release.
- [ ] v1.0.0 can adopt the accumulated registry as benchmark contract v1.

Do not add code merely because a speculative micro-optimization is imaginable.
