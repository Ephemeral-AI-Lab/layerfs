### Milestone 0 repaired exit

- Commit: `28b9cdf072777ff09f663aef733dc4caf85bdade`
- Date: 2026-08-10
- Checklist complete: yes
- Commands and environment: Windows, Node 24.11.1, pnpm 10.32.1; `pnpm validate:m0`
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the repository-foundation milestone
- Smoke duration and operation counts: 20.8 seconds total; 44 tests, 0 failures
- Resource high-water: not applicable to the repository-foundation milestone
- Known deviations: later milestone suites are deliberately separate from `validate:m0`; M1 and M2 remain unaccepted pending their bounded-construction and staging/driver repairs, and later GC/streaming prototypes remain provisional
- Approved to begin next milestone: yes

The repaired gate resolves the complete TypeScript import graph, rejects file
and package cycles, enforces the exact approved core directories and dependency
directions, confines SQL to SQLite repositories, and rejects cross-mechanism
composition outside operations. The export gate packs the committed package,
installs it in a clean temporary consumer, executes and type-checks all four
documented import paths, and proves that CAS, CDC, COW, manifest, schema,
repository, and transaction deep imports are blocked.

The fixture check is read-only and verifies the committed one-megabyte fixture
against seed `0x5eedc0de` and SHA-256
`37fcc2662466658ff1c3345de0dd5454764eded6ea1019a701563f359ab8c086`.
Package artifacts are generated solely by `pnpm build` and are reproducible
from the committed sources and lockfile.
