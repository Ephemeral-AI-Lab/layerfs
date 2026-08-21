/goal Decide whether the private F2 full-create whole-source digest is
independently required product authority or redundant beside ordered
authenticated canonical evidence, and write one adversary-driven report.

## Scope and sole write authority

Work only in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`. The active WP4-M
task is concurrent and nonterminal. Do not edit, message, interrupt, steer, or
wait on it, and do not treat partial WP4-M code or rows as evidence.

You may create exactly one file:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/research/phase-4/while-waiting-phase-4-to-finish/task-b-witness-authority/report.md`

If the assigned `report.md` already exists, stop and report the collision; do
not overwrite it.

Everything else is read-only. Do not run Cargo, rustc, tests, SQLite,
benchmarks, compression, or commands that write `target/`. Use committed
sources through `git show d781173a08ab4092eb539c3a0870056e6c6a77ff:<path>`
when the live file is dirty, and prefer the sealed accepted F2 source for the
private construction proof.

If WP4-M has begun measured release rows, do not run local shell commands or
write the report until those rows are quiet or the active task is terminal;
web research and reasoning may continue.

## Authority and known evidence

- Accepted F2-v3 source SHA-256:
  `c8ac86be3a97bbcc6b980e93bc7539532e2093c0e6fe741429ef4a26cb3cc158`.
- Sealed accepted source:
  `target/wp4m-f4a-residual-attribution-k64-20260820-v1/custody/sources/phase4_create_edit_benchmark-f2-accepted.rs`.
- Sealed F4 raw SHA-256:
  `5241b106a9d1d841e124d73ff247f2abadb2bf27759ef54d62a3ab3af3eb212f`.
- Measured construction source+sequence lane median: `89.067215 ms`.
- That lane combines complete raw-source hashing and a much smaller ordered
  `(length, raw_id)` transcript; the source-only wall is unavailable.
- The external fixture fingerprint remains custody evidence regardless of the
  product-proof decision.

## Read first

1. `research/phase-4/core/canonical/v2-single-identity.md`
2. `research/phase-4/core/canonical/identity-and-hashing.md`
3. `research/phase-4/core/pipeline/full-create-pipeline.md`
4. `research/phase-4/assurance/verification-security-resources.md`
5. `research/phase-4/foundations/invariant-matrix.md`
6. `implementation-detail/phase-4/wp4m/f-series/f2/report.md`
7. `implementation-detail/phase-4/algorithm/spec.md`
8. `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md`
9. `implementation-detail/phase-4/algorithm/complexity-analysis.md`
10. the sealed accepted F2 source named above;
11. `crates/layerfs-core/src/identity/`, `object/`, `content/persistence.rs`,
    and `validation.rs` from commit `d781173` when live files are dirty.

External research may use only primary cryptographic papers, RFCs, official
specifications, or original implementations. External designs are precedent,
not proof of LayerFS equivalence.

## Research question

Construct an explicit authority graph for:

```text
raw source bytes
  -> CDC boundaries
  -> raw/canonical chunk identity
  -> transaction-local PutEvidence
  -> ordered file references and K/F topology
  -> file/workspace root and transition
  -> proof consumption and one COMMIT
  -> fresh post-COMMIT verification
```

Then determine what unique property, if any, is supplied by the inner
whole-source digest.

Do not collapse these four commitments. Define and compare them in a dedicated
table, including input, framing/domain, producer, consumer, lifetime, and
failure authority:

1. the external fixture hash used for benchmark custody;
2. the inner complete-source `source_hasher` result;
3. the current ordered `sequence_hasher` over `(length, raw_id)`;
4. the proposed domain-separated ordered commitment over
   `(length, canonical_id)`.

At minimum analyze these adversaries/failures:

- omitted, duplicated, or reordered occurrence;
- wrong raw length;
- wrong raw ID;
- wrong canonical ID or valid object in the wrong occurrence;
- canonical kind/framing mismatch;
- unequal or forged incumbent;
- stale, duplicated, skipped, or reordered PutEvidence;
- mutation after evidence;
- wrong store/open/authority/epoch/profile/transaction serial;
- rollback, commit, reopen, second consumption, or cross-store replay;
- incorrect K/F topology, count, total, root, transition, or expected head;
- collision-assumption boundaries;
- corrupted external fixture expectation versus product publication authority.

Compare the current proof with the proposed domain-separated ordered transcript
over repeated `(u32be(raw_length), canonical_object_id)` values. Do not assume
the proposed transcript is sufficient; show which layer detects every case.
Keep pre-COMMIT publication authority distinct from post-COMMIT reconciliation
and verification: later detection cannot excuse publishing a head without the
required pre-COMMIT proof.

## Evidence rules

Use `Observed`, `Derived`, `Hypothesis`, and `Unavailable` literally. Keep
cryptographic equivalence distinct from implementation equivalence and test
coverage. Never call the full `89.067215 ms` removable; ordered replacement
work remains mandatory and its isolated wall is unavailable.

## Required report

Write only the assigned `report.md`, containing:

1. executive disposition;
2. exact current authority graph;
3. four-commitment distinction table and current/proposed definitions with
   unambiguous framing;
4. adversary matrix with one row per case and one column per detecting layer;
5. product authority versus external benchmark-custody distinction;
6. equivalence argument or exact counterexample/gap;
7. error-precedence and typed-error consequences;
8. what focused tests a later implementation would require, without
   implementing them;
9. honest performance interpretation of the combined 89-ms lane;
10. limitations and collision assumptions;
11. linked local and primary sources.

End with exactly one disposition:

- `REDUNDANT_WITH_ORDERED_CANONICAL_EVIDENCE`
- `WHOLE_SOURCE_DIGEST_REQUIRED`
- `INSUFFICIENT_AUTHORITY_PROOF`

If insufficient, name the smallest missing proof obligation; do not recommend
implementation.

## Completion

Complete only after all adversary rows have a detection result, all local links
resolve, no other file changed, and the final response gives the report path,
disposition, decisive property or gap, limitations, and SHA-256.
