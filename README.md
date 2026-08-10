# Ephemeral AI FS

A branchable SQLite filesystem for multi-agent workspaces.

Ephemeral AI Computer will use this library as its default production
filesystem. Computer retains `@cloudflare/dofs` as an explicitly selected,
isolated comparison engine for tests and benchmarks. Durable Object SQLite
remains a supported database backend; it is not the component being replaced.

The project is in its specification phase. Start with the
[`PRD.md`](./PRD.md) product boundary and the [`SPEC.md`](./SPEC.md) technical
specification. Implementation packages have not been created yet.
