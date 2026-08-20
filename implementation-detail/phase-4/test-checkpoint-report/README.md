# Phase 4 test checkpoint reports

This directory is the compact append-only history of Phase-4 performance and
correctness checkpoints. It records what was measured at each Git commit or
small dirty-tree step without retaining transient databases or large fixtures.

## Files

- [TEMPLATE.md](TEMPLATE.md) is copied for each new checkpoint.
- [index.tsv](index.tsv) contains one summary row per completed checkpoint.
- A checkpoint uses the name
  `cp-NNNN-<short-sha-or-dirty>-<experiment>.md`.
- Its optional compact raw rows use the same basename with `.jsonl`.

## Identity

A clean candidate is identified by its full Git commit and executable hash. A
dirty candidate is identified by the complete tuple:

```text
HEAD + complete diff SHA-256 + executable SHA-256
```

Never attribute dirty-tree performance to `HEAD` alone. Every report names its
parent checkpoint and frozen control so the optimization chain is explicit.

## Retention

The normal checkpoint bundle is one Markdown report plus one compact JSONL and
must remain below 10 MiB. Record hashes for fixtures and binaries; do not copy
them here. Do not retain SQLite databases, generated fixtures, authority files,
expectations, output files, or release executables for ordinary screening.

One representative database may be retained elsewhere for a final accepted
checkpoint only when that retention and its byte cap were declared before the
run.

## Decisions

- `BASELINE`: accepted comparison point.
- `SCREEN-PASS`: promising directional result awaiting checkpoint validation.
- `RETAIN`: complete checkpoint gate passed.
- `REVISE`: mechanism remains plausible but implementation/evidence needs a
  new prospective attempt.
- `REVERT`: correctness, resource, or performance gate failed.
- `INCONCLUSIVE`: noise or missing causal evidence prevents a decision.

The next optimization must not build on `REVISE`, `REVERT`, or `INCONCLUSIVE`.
