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
- [v0.1.1 task and checklist](0.1.1/README.md)
- [v0.1.2 proposals](0.1.2/README.md)

## Release sequence

- [0.1.1 task and checklist](0.1.1/README.md)
- [0.1.2 proposals](0.1.2/README.md)
- [Development guide](development.md)
- [Benchmark contract](benchmarking.md)

v0.1.1 completes and admits the large-namespace lifecycle. v0.1.2 owns
patch-compatible prepend, range-copy, fragmented-write, sparse-growth, and
mixed-edit work that survives evidence and compatibility gates. Do not reserve
v0.1.3: create another patch only for a new measured defect.

## Compatibility boundary

Every 0.1.x change preserves the released five-table Store schema, canonical
bytes and identities, CDC profile, public SDK and CLI behavior, daemon protocol,
visibility and acknowledgement semantics, and resource bounds. Only the item
that fails this boundary moves to 0.2.0.

## Acceptance criteria

The 0.1.x phase is complete when:

- [ ] The registered payload and namespace matrices pass with exact reopen
  proof.
- [ ] CPU, RSS, FUSE I/O, Store growth, object reuse, transaction maxima, and
  cleanup evidence are retained for every registered lifecycle.
- [ ] Every accepted optimization has a focused regression check and improves
  the production SDK/FUSE path.
- [ ] Selected failure paths leak no mount, container, process, output reader,
  spool, Workspace, or Branch lease.
- [ ] The final LayerFS-only campaigns have no unexplained regression.
- [ ] The final matched Cloudflare payload campaign passes at stable candidate
  checkpoints, outside the inner optimization loop.
- [ ] Remaining limitations and incompatible proposals are documented under
  the correct later release.
- [ ] No remaining change is justified by current evidence within the existing
  architecture.

Stop the 0.1.x line when these conditions hold. Do not publish another patch
merely because a speculative micro-optimization is imaginable.
