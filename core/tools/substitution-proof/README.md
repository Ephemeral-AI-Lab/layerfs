# The unchanged-consumer substitution proof

> **Status:** Proof harness, not product source and not a released contract.
> Evidence: [`docs/roadmap/0.1/0.1.7/evidence/stage-7-substitution-20260920T000000Z/`](../../../docs/roadmap/0.1/0.1.7/evidence/stage-7-substitution-20260920T000000Z/README.md).

This answers the Stage 7 acceptance scenario: can integration develop against one
qualified C1/C2 revision, then adopt an independently optimized C1/C2 revision by
**dependency selection and rebuild, without rewriting integration logic**?

Four arms, one frozen consumer, four dependency selections:

| arm | C1 | C2 |
| --- | --- | --- |
| `baseline` | baseline revision | baseline revision |
| `c1-only` | candidate | baseline |
| `c2-only` | baseline | candidate |
| `combined` | candidate | candidate |

```sh
python3 core/tools/substitution-proof/run.py --baseline 66bce8378 --candidate HEAD
```

The run materializes one revision tree per arm under `--root` (default `/tmp/subst`),
generates the four arm manifests, and then, per arm, builds and runs:

- `core_pipeline` — the repository's own integrated C1 → C2 external test, frozen;
- `filesystem_primitives_candidate` — a C1 driver that prints deterministic
  identities, so canonical output can be compared across arms;
- `cross_revision` — `create` then `read` across arms, so a persisted-data refusal
  is recorded as a typed result rather than hidden.

Results, including every hash and the per-arm manifests, are written to
`matrix.json`.

Rules this harness keeps:

- **No `[patch]`, no vendoring, no fork, no third-party modification.** The arms
  differ only in path selection.
- **The consumer is never edited between arms.** One copy, hashed once.
- **No timing claim.** The example's `elapsed_ns` field is normalized out of the
  cross-arm comparison, and the profile is `debug`.
