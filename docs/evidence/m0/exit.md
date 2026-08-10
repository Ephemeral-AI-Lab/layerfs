### Milestone 0 repaired exit

- Candidate commit: `786b418d002a0bf086386bd84d053a20054ec3fd`
- Validation date: 2026-08-10
- Checklist complete: yes; M1 was reopened and is not accepted by this record
- Primary environment: Windows 11 64-bit `10.0.26200`, Node `24.11.1`, pnpm `10.32.1`
- Primary command: `pnpm validate:m0`
- Primary result: pass in 40.572 seconds; 4 tests passed, 0 failed
- Correctness artifact: [`correctness.json`](./correctness.json)
- Benchmark artifact: not applicable to the repository-foundation milestone
- Hosted-CI deviation: no GitHub Actions run exists for this unpushed branch. The exact
  candidate was instead validated locally across the workflow's complete OS/runtime
  matrix: Windows and Linux, Node 22 and Node 24.
- Approved next gate: M1 only. M2 remains paused and unaccepted.

The exact candidate passed these four local matrix runs:

| Operating system          |    Node | Validation                         | Result         |
| ------------------------- | ------: | ---------------------------------- | -------------- |
| Windows 11                | 24.11.1 | `pnpm validate:m0`                 | pass, 40.572 s |
| Windows 11                | 22.23.2 | `pnpm validate:m0`                 | pass, 45.558 s |
| Debian bookworm container |      22 | clean install + `pnpm validate:m0` | pass           |
| Debian bookworm container |      24 | clean install + `pnpm validate:m0` | pass           |

Each run verified the deterministic 1 MiB fixture and its recorded SHA-256 digest,
linted 18 Markdown files, ran Prettier and ESLint over the repository, checked 122
source/config files, parsed 45 core files, and exercised eight negative import-bypass
fixtures. The package gate built six publishable packages into 200 clean `dist` files,
packed six tarballs containing 206 approved files, tested all nine public entrypoints
and 96 exported symbols, type-checked clean consumers, and denied internal deep imports.

The architecture gate now includes static and dynamic imports, import types, TypeScript
`ImportEquals`, direct and aliased `require`/`require.resolve`, triple-slash path
references, bare host/external imports, and relative realpath escapes. It enforces the
exact sixteen core areas, zero source/package cycles, SQL ownership, only-operations
cross-composition, and a type-only SQLite-to-operations storage-port inversion.

CI remains configured for Windows/Linux and Node 22/24 and invokes only
`validate:accepted`, which is pinned to `validate:m0` at this repaired M0 exit. A hosted
run requires pushing the branch and is deliberately not claimed here.
