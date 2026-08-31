# LayerFS documentation policy

> **Status:** Current documentation policy.

## Documentation classes

- `docs/index.md` is the documentation entry point.
- `docs/versioned/<release>/` is the immutable user and operator manual for a
  release.
- `docs/next/<release>/` contains non-binding proposals for a named future
  release.
- `docs/general/` contains maintained project practices and concepts.
- `docs/development/` contains maintainer tooling and benchmark contracts.
- `docs/research/` contains informative comparative and conceptual work.
- `docs/archive/` contains implementation history and handoff material.
- `release-notes/<release>/` contains release evidence and announcements.
- benchmark and research directories contain supporting evidence or design
  exploration, not released product contracts.

## Status banners

Every maintained document begins with an explicit status banner. Use one of:

```text
Status: Released for LayerFS <release>.
Status: Release candidate for LayerFS <release>.
Status: Current general guide.
Status: Proposal; target LayerFS <release>; not a released contract.
Status: Research; informative and not a product contract.
Status: Archived; retained for historical evidence only.
```

The banner must make authority clear without requiring repository history.

## Writing rules

- Describe the current product positively and directly.
- Put release-specific commands, APIs, schemas, and limitations in the
  versioned manual.
- Keep examples executable and use public SDK or CLI entry points.
- Use typed entity names exactly as the source defines them.
- Link benchmark summaries to raw evidence rather than copying unattested
  numbers into several files.
- Use repository-relative links and verify them before review.
- Avoid treating an experiment, handoff prompt, or planning target as a
  released capability.

## Review checklist

For every documentation update, verify:

1. all local links resolve;
2. command grammar matches the CLI parser;
3. Rust examples match public reexports and method signatures;
4. schema counts and runtime settings match the Store source;
5. limitations do not imply unsupported guarantees;
6. the document has exactly one clear status.
