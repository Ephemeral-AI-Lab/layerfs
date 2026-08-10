### Milestone 0 repaired exit

- Commit: `f1c4ebd111eee269a2a4f2d0a786f41d4cae63b2`
- Date: 2026-08-10
- Checklist complete: yes; only M0 checkboxes are checked in the implementation plan
- Commands and environment: Windows 11, Node 24.11.1, pnpm 10.32.1;
  `pnpm install --frozen-lockfile`; `pnpm validate:m0`
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the repository-foundation milestone
- Validation duration and operation counts: 37.07 seconds; 4 M0 harness tests, 0
  failures; 45 core files; 4 negative import-bypass fixtures; 118 linted source/config
  files
- Packaging evidence: 6 clean publishable packages, 200 generated dist files, 9 public
  entrypoints, 96 exported symbols, and 6 clean-consumer tarballs with 206 approved
  files
- Resource high-water: not applicable to the repository-foundation milestone
- Known deviations: M2-and-later source remains present but provisional. It is excluded
  from M0 tests and is neither accepted nor represented as safe. M1 must be rerun after
  this repaired baseline before M2 may be reconsidered.
- Approved to begin next milestone: yes, M1 only

The graph gate parses static imports and exports, dynamic `import()`, import types,
TypeScript `ImportEquals`, CommonJS `require`/`require.resolve`, and resolves every
relative source edge by real path. It proves the exact sixteen core areas, rejects
source/package cycles and relative cross-package escapes, confines SQL ownership,
restricts transformation composition to operations, and allows SQLite implementations
only through explicit storage ports at the two approved composition roots. Four
committed negative fixtures prove the dynamic-import, ImportEquals, require, and
realpath package-escape paths fail.

Every publishable package removes a planted stale-output sentinel before building. Its
dist tree must correspond exactly to current source, and its packed file list must
correspond exactly to that clean dist plus approved assets. All six tarballs are
installed together in a clean consumer, executed, and type-checked. The four documented
core subpaths work, internal deep imports covering all core areas fail, and committed
symbol plus declaration snapshots cover every public entrypoint.

CI invokes only `validate:accepted`, which is pinned to `validate:m0` in this commit.
Every later milestone has an explicit owned suite and sequential gate; empty or absent
future suites fail if someone advances the pointer early.
