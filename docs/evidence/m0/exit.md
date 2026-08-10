### Milestone 0 repaired exit

- Candidate commit: `937b72e84349d3371ca5d75399f5a30e0307d06c`
- Validation date: 2026-08-10
- Checklist complete: yes; this record accepts M0 only
- Primary environment: Microsoft Windows NT `10.0.26200.0`, x64, Node `24.11.1`, pnpm
  `10.32.1`
- Primary command: `pnpm validate:m0:pre-evidence`
- Primary result: pass in 40.895 seconds; foundation tests 5 passed, 0 failed
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the repository-foundation milestone
- Hosted-CI deviation: no GitHub Actions run exists for this unpushed branch
- Validation scope: one actually executed Windows x64 / Node 24.11.1 cell; no Linux or
  Node 22 result is claimed
- Approved next gate: M1 evidence recording only. M2 remains paused and unaccepted.

The exact candidate verified the deterministic 1 MiB fixture with SHA-256
`37fcc2662466658ff1c3345de0dd5454764eded6ea1019a701563f359ab8c086`, linted 18 Markdown
files and 158 source/config files, parsed 46 core source files, and passed 24 negative
architecture/reflection/package bypass fixtures. The foundation runner executed five
tests, all passing.

The package gate built six publishable packages into 204 clean `dist` files. Six
isolated declared-dependency tarball closures containing 210 approved files passed
runtime/type parity for all nine public entrypoints and 96 exported symbols, while
forbidden internal deep imports and sentinel-only builds were rejected.

This M0 candidate intentionally advances from the earlier foundation commit because the
M1 repair updated M0-owned reachable API rollups. Its M0-owned tree digest is
`5498e5c1f9c486ee2e44fca844b29724a2b037802d524aeb1de9e0a1eda851fb`.

The pre-evidence command was required because this refreshed evidence commit did not yet
exist. After the directly parented M0 evidence commit and the M1 evidence record exist,
the complete `pnpm validate:m1` command runs `check:evidence` and revalidates this M0
gate. No hosted or unexecuted platform/runtime result is inferred.
