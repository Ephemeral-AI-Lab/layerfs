# Ephemeral AI FS

A branchable SQLite filesystem for multi-agent workspaces.

Ephemeral AI Computer will use this library as its default production filesystem.
Computer retains `@cloudflare/dofs` as an explicitly selected, isolated comparison
engine for tests and benchmarks. Durable Object SQLite remains a supported database
backend; it is not the component being replaced.

The project is in its specification phase. Start with the [`PRD.md`](./PRD.md) product
boundary and the [`SPEC.md`](./SPEC.md) technical specification. The release gates are
the [`correctness test plan`](./docs/testing/correctness-tests.md) and
[`benchmark plan`](./docs/benchmarks/release-benchmarks.md). Implementation packages
have not been created yet. Delivery order, milestone checklists, and acceptance criteria
are in the [`implementation plan`](./docs/implementation/implementation-plan.md). The
ready-to-paste [`implementation prompt`](./docs/implementation/implementation-prompt.md)
starts a coding-agent task with the required architecture and release gates.
