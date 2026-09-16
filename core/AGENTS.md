# Agent rules for the replacement core

> **Status:** Current general guide.

Applies recursively to `core/`. Read the repository [AGENTS.md](../AGENTS.md)
first; its measurement, release, dependency and evidence rules still apply.
This file adds the owner's product-source and module-structure requirements.

## Scope and ownership

- `core/` is the replacement product workspace. Existing root `crates/` is a
  temporary reference, not a dependency, binary fallback or source include.
- Only `layerfs-telemetry` is agreed as a candidate crate. All other crate names,
  counts and boundaries remain open. Create no placeholder packages or modules.
- Read the component's approved roadmap before implementing it. For telemetry,
  use the [timer specification](../docs/roadmap/0.1/0.1.7/component-decoupling/telemetry.md)
  and [repository layout](../docs/roadmap/0.1/0.1.7/component-decoupling/repository-layout.md).
- Keep unrelated work intact. Package/source moves and legacy retirement follow
  the migration plan; they are not implicit parts of implementing a component.

## Product source contains product code only

For every package in `core/crates/`, `src/` is exclusively production code.
This includes all library, executable, included and generated implementation code.

Forbidden in product implementation:

- Inline tests: `#[test]`, `#[bench]`, test modules and equivalent test macros.
- Test-only configurations: `#[cfg(test)]`, `cfg!(test)`, test-bearing
  `cfg_attr`, test-only features such as `test-instrumentation`, and equivalent
  branches hidden behind another feature name.
- Fixtures, mocks, fake clocks, fault injection, test counters, benchmark drivers,
  demonstration mains and helper functions that exist only for tests or examples.
- Test-only public methods, visibility changes or extra abstractions introduced
  solely to let tests reach private implementation details.
- Alternate algorithms, shortcuts or tuned behavior selected for a test/benchmark.

Platform cfgs, real product features, production validation, assertions of product
invariants and actual product telemetry remain legitimate implementation code.
API documentation belongs with the API; executable examples and compile-fail
fixtures belong outside src/. Do not add executable rustdoc test blocks in src/.

Use these locations as needed; do not scaffold empty directories:

| Content | Location |
| --- | --- |
| Product implementation | `core/crates/<package>/src/` |
| Public-API integration tests | `core/crates/<package>/tests/` |
| Test helpers, fakes, fixtures, compile-fail inputs | Under that package's `tests/` |
| Runnable usage examples | `core/crates/<package>/examples/` |
| Package benchmark targets | `core/crates/<package>/benches/` |
| Product-wide benchmark harness | `core/benchmark/` when introduced |
| Build/check scripts | `core/tools/` |

`#[test]` is expected in tests/. Tests exercise the same production library that
normal consumers use. Do not include/recompile private src/ files from a test,
redirect library/binary target paths into test directories, or import test helpers
back into production. Test-only dependencies belong in dev-dependencies; follow
the component's stricter dependency limits (telemetry is std-only).

Prefer public behavior and deterministic data to test-only product hooks. For
telemetry, use synthetic completed reports to check exact composition/formatting,
and structural/ordering assertions for actual clock measurements. Do not inject
a fake clock into src/ solely for tests, add a public Clock trait for testing,
or use sleeps and narrow wall-time thresholds. Verify disabled behavior through
its public result and inspection of the clock/allocation paths; stronger external
instrumentation must not add test-only branches to the product.

## lib.rs and mod.rs are thin entry files

Every product `lib.rs` and `mod.rs` has a hard maximum of **200 physical lines**,
including comments and blank lines.

Allowed: crate/module attributes, module declarations, imports/reexports, API
documentation and thin calls that directly delegate to a dedicated implementation.
Delegation may forward arguments/results; it must not implement behavior itself.

Keep type/trait definitions, impl blocks, state, constants that encode behavior,
algorithms, branching, loops, conversions, validation, formatting and I/O in
focused named implementation files. Keep helpers there too. Do not evade the
limit with minified lines, macro expansion, includes, or a renamed god module.
File size is a ceiling, not an instruction to fill entry files to 200 lines.

## Production LOC for every commit

Follow the repository's
[per-commit production LOC rule](../AGENTS.md#production-loc-comparison-for-every-commit).
Every commit records production LOC before, after and signed delta using the
first parent and exact committed tree, with a reproducible counting method.
Prepare it from the final staged tree; exclude unstaged work.

For core, count actual product implementation and required runtime source inputs.
Exclude tests, examples, fixtures, benchmark/development tooling, documentation,
comments, blank lines and generated build artifacts. Report reference and core
subtotals separately during migration, plus the combined product total. Keep
source classification stable across moves and include new product paths.

The **200-line lib.rs/mod.rs limit is different**: it counts every physical line,
including comments/blanks. Do not use that count as production LOC. Tests remain
required, but their size does not enter the per-commit production comparison.
Record unchanged production totals with delta 0 for policy/test/docs-only commits.

## Checks and completion

Before handing off core changes, run from the repository root:

```sh
python3 core/tools/check_product_boundary.py
python3 -m unittest discover -s core/tools -p 'test_*.py'
```

The source-text guard detects prohibited test/configuration markers, oversized
entry files and common implementation constructs in them. Its checks also apply
to marker examples embedded in product-source comments/strings; put such examples
in external documentation. It is not a Rust semantic proof. Review ordinary helper
functions, macros, manifest target paths, dependencies and generated inputs for
the same ownership rules; a green scan alone does not establish product purity.

Once a core workspace exists, run its locked tests, examples, formatting and
warning-denying Clippy with an explicit core manifest. The existing root local
preflight runs the boundary guard and its self-tests; core Cargo checks must be
added when that workspace is introduced. Do not disable test
discovery or omit checks to satisfy this policy. Follow the repository pre-push
gate before publication; LayerFS has no CI.

Report exact checks and gaps. Do not present an empty source scan as a built or
tested implementation. Production behavior and required tests must both be present
before an implementation task is complete.
