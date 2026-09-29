# Phase 4.5 integration benchmark layout and plan

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Proposal dated 2026-09-29, source `64e0e81a7f702e505eae8f3f935bac42de19d312`.
> Product source remains `11a864fc133844cae7a4247b1f243d84d5763b10`.

## 1. Proposed family ownership

```text
core/benchmark/fs-bench-pro/
  families/
    init_namespace.py
    workspace_write.py
    workspace_commit.py
    workspace_namespace.py
    workspace_mutations.py
    workspace_shell_package.py
    history_retention.py
  shared/       common preparation, seals, invocation and reporting
  writers/      ordinary POSIX workload programs
  tests/        harness/registry self-tests
  runner.py     dispatch to a selected family
```

Each family owns exactly one canonical case-definition module and one thin
runner entry. Reuse existing lifecycle, supervision, verification and receipt
utilities. Add a module when its specified cases are implemented; the diagram
does not request empty future scaffolding or a new framework.

| Family | Semantic axis and operation surface | Current basis |
| --- | --- | --- |
| `init_namespace` | Source-directory initialization through the public SDK | Existing [family](../../../../benchmark/fs-bench-pro/families/init_namespace.py); keep its sole Init runner and release-only profile. |
| `workspace_write` | File-content change patterns through SDK Mount/Exec/POSIX-FUSE/Commit | Existing nine matrix cells and separate #248 gate in [checkpoint5_273.py](../../../../benchmark/fs-bench-pro/checkpoint5_273.py). Larger shifts/copies require their own declared schedule. |
| `workspace_commit` | Commit work as newly changed state and retained state vary | Existing clean/one-edit IDs, with exact same-Workspace pinned-view controls still to seal in the benchmark harness. |
| `workspace_namespace` | Namespace operation cost as width, depth, descendants and resident observations vary | Existing functional/count namespace tests; a public benchmark family is a proposed extension. |
| `workspace_mutations` | Mixed mutation sequences and their exact resulting state/custody | Existing `issue273-mutations-v1` verification selection; any timed mixed-workflow cases need prospective registration. |
| `workspace_shell_package` | Deterministic ordinary shell package refresh | Existing [package registry](../../../../benchmark/fs-bench-pro/registry/workspace-shell-package-v1.json) and [runner](../../../../benchmark/fs-bench-pro/shell_package.py). |
| `history_retention` | Retained history construction/storage across selected checkpoints, through C1/C2 | Existing [Rust family](../../../../benchmark/fs-bench-pro-storage-content/src/families/history.rs), [workload](../../../../benchmark/fs-bench-pro-storage-content/src/workload/history.rs) and [operations](../../../../benchmark/fs-bench-pro-storage-content/src/ops/history.rs). A family adapter can expose the existing driver without rewriting it or labeling it SDK/FUSE. |

`workspace_namespace` covers specific namespace operations and scaling;
`workspace_mutations` covers interactions. Crate-level correctness tests stay
in `core/crates/*/tests/`. Their runners do not become performance benchmarks
merely by appearing in this catalog. The separate C1/C2 Rust workspace stays
the history execution backend. The existing SDK Init runner must not acquire
a second implementation or silently change its operation boundary.

## 2. Naming

- Python modules and internal family IDs: `snake_case`.
- CLI family names: `kebab-case`, corresponding to the module.
- New case IDs: `<family>-<scenario>-<dimension>-<value>[-...]-v<version>`.
- Use explicit units and quantities: `writes-4097`, `files-1000`,
  `size-10mib`, `patch-4kib`, `stride-3`.
- Use operation names, not issue numbers, phases or implementation names.
  Keep issue/source/algorithm references in metadata and reports.
- File size, write count, stride and seed are case dimensions, not separate
  family modules. All three stride selections belong to `history_retention`.
- Every registry row records its actual route, operation contract, profile,
  role (`performance`, `verification` or `diagnostic`), fixture/oracle,
  limits, arms and case order. A common catalog does not make different
  routes comparable.

Examples for newly registered cases:

```text
workspace-write-append-writes-100-size-10mib-v1
workspace-write-dispersed-writes-4097-size-10mib-v1
workspace-commit-clean-retained-writes-4097-v1
workspace-namespace-rename-descendants-67-v1
workspace-mutations-mixed-files-128-v1
history-retention-stride-10-v1
```

Preserve existing registered IDs and archived receipt schemas. In particular,
keep `history-stride10`, `history-stride3`, `history-stride1`, the `issue273-*`
IDs and `issue248-separated-4097-v1` attached to their original evidence.
Organize those IDs under family ownership without renaming old results. A
layout-only move needs a new harness seal for new runs; it does not alter the
workload version or turn an old receipt into a new measurement.

## 3. Migration map

| Existing location | Proposed ownership |
| --- | --- |
| `families/init_namespace.py` | Keep in place. |
| `checkpoint5_273.py` | Write rows under `workspace_write`; clean/one-edit rows under `workspace_commit`; shared campaign mechanics under existing `shared/` utilities. Preserve the original case order and cohort contract. |
| `write_patterns.py`, `separated_writes.py` | Canonical write definitions under `workspace_write`; reusable observations/invocation helpers under `shared/`. Retain historical diagnostic entry points and seals. |
| `shell_package.py` and its registry | `workspace_shell_package`, with one canonical registry. |
| Storage/content `families/history.rs`, `workload/history.rs`, `ops/history.rs` | `history_retention` catalog adapter, reusing the existing Rust driver, profile switches and lane-specific verification. |

Keep ordinary workload binaries under `writers/` and use the existing compiled
SDK/independent-verifier examples. Do not duplicate the measured implementation
in Python, revive a Workspace range API/ioctl, or change execution based on a
benchmark identity.

## 4. Selections to verify at the final integrated source

### Existing 12-row Workspace campaign

The existing [checkpoint-5 registry](../HANDOFF-CHECKPOINT5.md) declares:

| Membership | Cases | Original complete-command limit |
| --- | ---: | --- |
| Append, dispersed, repeated x 100/512 writes | 6 | 15 s each |
| Append, dispersed, repeated x 4097 writes | 3 | 25 s each |
| Clean Commit with retained state | 1 | 15 s |
| One-edit Commit with unrelated retained state | 1 | 15 s |
| Original #248 separated-4097, 8194-byte fixture | 1 | 25 s |

The clean/one-edit controls must use their live same-Workspace lease and exact
retained generation. Their existence in the registry is not an execution PASS.
The matrix uses the unchanged prepared 10 MiB file and one construction worker.
Use actual public SDK Mount/Exec/Commit, full independent old/new-byte oracles,
real FUSE callback counts and complete cleanup.

### Separate functional/count regression selections

- 8192 distinct writes to the existing 8194-byte fixture, default 8 MiB Budget:
  full bytes, known successful Commit, checked close/refund. Its current route
  is a native Workspace test with a live Service, not an SDK Exec performance
  row. Retain that distinction; do not move it into the nine-cell matrix.
- Separate 10,240-write default-Budget refusal: exact known canonical G1,
  no installed C5 revision, continuing G2 and explicit retained custody.
- Original 64 MiB lowering: `[100,110) -> abc`, insertion `12345` at 200,
  deletion `[500,700)`, exact 67,108,662 final bytes and 8 new replacement
  bytes; 64 MiB quota, 10 s Stage and 60 s functional command bounds.
- Actual public SDK reordered/duplicated Base copy, two known Commits and
  independent old/new/G2/pinned-byte oracles.
- Original occupied 2 MiB Stage and 4 MiB Commit headroom, physical accounting,
  same-fund allocation/refund, old pins and failure/unknown custody.

These are named functional proofs, not extra samples of a performance arm.
Register any new SDK performance version prospectively with its own route,
workload, oracle and limits.

### Other families

| Family | Initial membership and proof |
| --- | --- |
| `workspace_namespace` | Small/large inherited subtrees, resident/uncached descendants, same-depth/depth-increasing rename, replacement/move-back, deep component access and listing. Record resident scans, descendant visits, C1 internal visits and private page work. The current active path still rewrites resident paths and scans inherited descendants on depth increase. |
| `workspace_mutations` | Existing mutation/G1-G2/32-pin/failure selections, plus a prospectively frozen mixed SDK Exec/Commit workflow. Mutation correctness is required before merge; the benchmark catalog supplements it. |
| `workspace_shell_package` | Existing `mixed-refresh-v1`, `overwrite-4k-v1`, `repeated-one-byte-v1`, `failed-command-no-commit-v1`. Freeze larger many-file create/update/delete/list cases before timing; 128/129/257/1025 counts discussed in the conversation are proposals, not registered PASS rows. #256 remains NOT_PROVED. |
| `history_retention` | `history-stride10` = 17 states, `history-stride3` = 53, `history-stride1` = 157. Use differences between consecutive selected states; preserve corpus/root/policy pins. Run-only stride1 is explicit and is not tuned or silently included in a default smoke run. Its C1/C2 surface does not qualify Workspace/FUSE timing. |
| `init_namespace` | Existing default 100/1000 files; explicit 10,000/100,000 tiers when selected. Locked release SDK driver/verifier; keep its declared complete-namespace plus deterministic sampled-content oracle distinct from full-content verification. Init retains its existing parallelism exception. |

## 5. Baseline and later execution

The [baseline inventory](BASELINE-20260929.md) selects whole retained cohorts,
not fastest rows. Frozen pre-#273 source `48b51e874` is the comparison control
and remains NOT_RUN. Candidate reference timings remain INELIGIBLE. Init and
history references keep their own source/profile/surface and are not pooled.

Before a future campaign, freeze the integrated source, baseline source,
unchanged workload/oracle, public route, host/image/build/profile, cache
declaration and measurement boundaries. Preserve each registered family and
its own limits; SDK 15/25 s limits must not be copied onto an unrelated history
lane or enlarged to turn a miss into a pass. Follow the current repository
one-attempt rule even where older planning text mentions repeat sampling.

Reuse validated prepared inputs outside timers, use a fresh writable byte copy
for each post-initialization run, and retain fresh outputs append-only. Report
Exec, Stage/Commit, cleanup and complete command separately, along with actual
WRITE count, index/pack work, C1 visits/saves, Store/backing bytes, identities,
verification scope and cleanup. Verified physical `st_blocks*512`, allocation
charges, RSS and cache are separate domains. Resource refusal/custody remains
a correctness proof while the owner defers additional memory qualification.
Raw times with an unproved cache contract stay INELIGIBLE. There is no speedup
denominator, merge approval, CI or aggregate preflight claim in this document.
