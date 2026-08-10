### Milestone 1 repaired exit

- Candidate commit: `ff8cd5a74e3b57392ff232788e6f9244cc447aaf`
- Validation date: 2026-08-10
- Sequential predecessor: repaired M0 candidate
  `937b72e84349d3371ca5d75399f5a30e0307d06c`
- Checklist complete: yes; this record accepts M0 and M1 only
- Primary environment: Microsoft Windows NT `10.0.26200.0`, x64, Node `24.11.1`, pnpm
  `10.32.1`
- Primary commands: `pnpm test:m1` and `pnpm test:workerd`
- Primary result: pass in 11.163 seconds total; 35 Node tests and 11 named workerd
  checks passed, 0 failed
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the pure-algorithm milestone
- Hosted-CI deviation: no GitHub Actions run exists for this unpushed branch
- Validation scope: one actually executed Windows x64 / Node 24.11.1 cell; no Linux or
  Node 22 result is claimed
- Approval boundary: stop after M1. M2 remains paused and unaccepted.

The candidate is an explicit allow-empty validation marker directly above the M0
evidence commit. The accepted M0 source/API predecessor remains
`937b72e84349d3371ca5d75399f5a30e0307d06c`, matching the M0 evidence candidate and the
sequential predecessor required by the evidence checker.

The Node suite passed all 35 algorithm tests in 6.837 seconds of command wall time. It
covers SHA-256/CAS ownership, FastCDC goldens and streaming bounds, runtime admission,
COW and structural patches, segmented-manifest codecs/builders/cursors, recomputed-
digest corruption, the 100,001-entry bounded builder, authenticated fixed-capacity
diagnostic local rebuild, streamed fallback, adversarial metrics, and caller-controlled
`Uint8Array`/Node `Buffer` ownership boundaries.

The workerd command, including its clean package build, passed all 11 required named
checks in 4.323 seconds and emitted machine-readable JSON. It reproduced the shared
SHA-256, FastCDC, manifest, cursor/corruption, COW, patch, diagnostic-local, streamed,
subclass-source, and runtime-progress assertions. The no-edit subclass-source regression
preserved root identity, file size, source bytes read, and bytes hashed in both
runtimes.

The durable 100,001-entry builder used 17-record keyset reads, produced 396 nodes at
depth 3, and observed a peak of 259 retained record references with a 9,336-byte
serialized-record capacity proxy. It processed 100,396 grouping records / 3,618,996
grouping-record bytes. These are observed metrics, not loose assertion ceilings or
JavaScript heap measurements.

The M1-owned tree digest is
`4dcfa8d941c7fd40e7a697e9431bd4ac0673e987579bd548306b91412536bce1`. After this directly
parented evidence commit exists, the complete `pnpm validate:m1` command revalidates M0,
checks both evidence histories/digests, and reruns Node and workerd. No hosted or
unexecuted platform/runtime result is inferred.
