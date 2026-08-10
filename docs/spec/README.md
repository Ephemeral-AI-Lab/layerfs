# Ephemeral AI FS specification

The detailed technical specification is organized into six normative
contracts:

- [`storage-and-data-model.md`](./storage-and-data-model.md) defines database,
  content, manifest, migration, recovery, and garbage-collection behavior.
- [`filesystem-api.md`](./filesystem-api.md) defines paths, metadata,
  operations, errors, adapters, and conformance behavior.
- [`branches-and-publication.md`](./branches-and-publication.md) defines branch
  views, lifecycle, conflicts, publication, idempotency, and retention.
- [`replication.md`](./replication.md) defines host-neutral negotiation,
  bounded batches, cursors, staging, retry, and import/export.
- [`node-vfs.md`](./node-vfs.md) defines the Node virtual filesystem provider,
  range I/O, bounded write sessions, flush, errors, and metrics.
- [`performance-and-resource-limits.md`](./performance-and-resource-limits.md)
  defines aggregate memory accounting, backpressure, benchmark methods, and
  release gates.

[`design-rationale.md`](./design-rationale.md) records the non-normative
evidence, tradeoffs, and rejected alternatives behind those contracts.

The executable release plans are:

- [`../testing/correctness-tests.md`](../testing/correctness-tests.md) for the
  required correctness, fault, integrity, and integration matrix; and
- [`../benchmarks/release-benchmarks.md`](../benchmarks/release-benchmarks.md)
  for fixtures, measurements, thresholds, DOFS comparisons, and go-live gates.

Start with the repository-level [`SPEC.md`](../../SPEC.md) for scope and
normative language.

For Ephemeral AI Computer, these contracts define the default production
filesystem. Computer may retain DOFS as an isolated, explicitly selected
comparison engine. Durable Object SQLite remains the database backend through
`@ephemeralai/fs-sqlite-cloudflare`.
