# Simplification and speed investigation

> **Status:** Read-only investigation; no current benchmark run was launched.
> The timings below are historical Stage 6 evidence, not current-HEAD claims.

## Owner direction for the successor

- One performance observation per registered case and arm, using seed `1` or
  repetition `1` where that selector applies. No multi-sample option, average,
  n3, or best-of result in the v0.1.7 runner.
- During iteration, test one selected path immediately after a relevant edit.
  Do not repeat unrelated passing tests, proofs, or performance cells. Run the
  affected final admission checks once after the last relevant edit.
- Reuse a compatible prepared fixture, build, image, receipt, or verifier proof
  only through its existing identity/seal rules. Reuse must never serve cached
  pages to the measured operation.

These rules are normative in
[`core/benchmark/fs-bench-pro/AGENTS.md`](../../../benchmark/fs-bench-pro/AGENTS.md).

## Existing Stage 6 patterns; select only what the new runner needs

The table below describes the old Stage 6 runner, **not** the #235 first-pass
CLI. The [#231 spec](issue-231/SPEC.md) reduces that CLI to `list`, `run`,
`verify` and `report`, with lazy one-case preparation, a full verifier and no
control/candidate arms or second Cargo workspace. Do not port Stage 6's
sampled verification, proof-reuse and calibration modes merely because they
exist.

| Work | Existing fast path | Boundary |
| --- | --- | --- |
| One-case preparation | The core harness accepts `prepare --case ID`; `perf --case ID` also acquires only that case automatically. | Do not run whole-lane preparation for one target. |
| Iteration verification | An explicit case defaults to deterministic `sample` verification. | Its receipt is `INCOMPLETE`; this is a quick check, not admission proof. |
| Proof reuse | `--reuse-pass` reuses an identity-matched PASS and cleanup-PASS. | A relevant source/harness/fixture/image change invalidates it. |
| Build reuse | `--no-build` plus the worktree's locked Cargo target and matching artifact seals. | Any changed binary or product artifact must be rebuilt and resealed. |
| One selected operation | The Rust driver accepts one `--case`; the Python runner launches one selected child and writes its own receipt. | Do not wrap it in a loop or rerun it for a better number. |

Stage 6 round-5b records **143.5 s** for whole-lane preparation (129.5 s to
acquire 118 masters / 15.567 GB), with an independent run at 139.4 s. That
acquisition pays off for repeated full-lane runs; it is waste for a one-case
edit. Use the case-scoped path. The same archived run records a 220-row
verification in 1.989 s and matching proof reuse in 23.9 ms. For a selected
case, the documented quick verification is about 0.1 s and proof reuse about
0.06 s. These receipts are from Stage 6 source commit `2f8ebc90`; inspect their
identity before reuse, and do not treat them as current product measurements.

## Do not buy speed by weakening proof

The full-lane Stage 6 verification phase was 32.4 s, but only about 0.8 s was
skippable because most oracle work is part of each case's required operation or
trace. Turning those oracles off changes the claim. Keep sampled verification
for fast iteration and full verification for final admission; do not label
sampled coverage PASS.

Likewise, preserve byte-copy independence, SQLite sidecar rejection, expected
result checks, and cache invalidation/residency for cases that claim cold reads.
APFS clone appeared to copy a 136.7 MB Store in 857 μs while consuming only
36 KiB of free space; the required byte copy took 0.166 s and used 133 MiB.
That apparent shortcut would invalidate storage-allocation evidence.

## Small implementation candidates to profile

These are code-reading findings, not measured savings and not permission to
change the measurement contract:

1. **Remove duplicate Store hashing around clone.** The root runner hashes the
   source before the copy, hashes it again afterward, hashes the destination,
   and later re-hashes the prepared tree during cleanup. The global rules say to
   validate the master once and retain its sealed digest. A focused implementation
   can compare the streamed copy digest with that sealed digest and use the
   existing independent-copy, sidecar, SQLite-integrity, and master-isolation
   checks. Preserve the target's identity proof; remove only source rereads that
   repeat an already established digest. If the source was exposed to the sample,
   retain the stronger unchanged check for that case.
2. **Reuse validated image inspection.** Root selection resolves an immutable
   Docker image ID, then sample startup inspects that same ID again. Pass the
   already-validated metadata within one invocation if container validation can
   still check the actual created image, mounts, limits, and ownership. This is
   likely a small fixed overhead; measure it before prioritizing.
3. **Keep expected-value work out of repeated verification.** The core harness
   already persists expected digests during acquisition. Preserve that pattern:
   verify actual output against the pinned expected value, but do not recompute
   the same fixture-only oracle on every pass. The prior full-member rehash
   cost 4.317 s for 1 GiB on one historical case.

The first candidate is directly supported by
[`benchmark_rules.md` §6](../../../../docs/general/benchmark_rules.md):
repeated source rehashing per sample is unnecessary. Implement it only with
unchanged receipt identity and a focused test of changed-master, changed-copy,
sidecar, and master-isolation rejection. No cache invalidation or residency
check may be removed as part of that optimization.

## Legacy interface note

The root CLI still exposes `--perf-samples N`; the v0.1.7 successor should omit
it. Do not use it for current evidence. The current QUICKSTART's old #118 n3
regression screen is superseded by the owner's one-sample instruction; archived
n3 reports remain historical and are not rewritten.
