# fs-bench-pro

Selected Docker/Linux benchmarks with real LayerFS daemon/FUSE execution.
The host orchestrates; fixtures, Stores and product processes stay inside
Docker. No project bind mounts, data volumes or Docker socket are used.

## Source layout

```text
benchmark/fs-bench-pro/
  Dockerfile.layerfs
  verify-selected.py
  shared/
    runner.py
    runtime.py
    daemon-entrypoint.sh
    test_runner.py
    test_runtime.py
    test_layout.py
  families/<family>/
    mod.rs
    setup.sh
    perf.sh
    verify.sh
  src/
    main.rs
    infra.rs
    ... shared operation and oracle implementations
```

The 18 current family folders are the public entrypoints. Their shell scripts
only bind the family and forward arguments to shared infrastructure. Shared
Rust workload helpers remain at the benchmark root where required by the
coordinator and standalone workload build.

## Build and run

Run these commands from this directory:

```bash
export LAYERFS_BENCH_IMAGE="$(python3 shared/runner.py --build-image)"

families/init_namespace/setup.sh --smoke
families/init_namespace/perf.sh --smoke

families/directory_construction_traversal/perf.sh \
  --case directory-construct-1-compact-v2 --seed 1 \
  --setup clone --perf-samples 3
```

Preparation is optional prewarming: a performance run acquires a compatible
prepared input or creates it when missing. Use `--image` instead of the
environment variable when selecting an image explicitly.

| Entry point / option | Meaning |
| --- | --- |
| `setup.sh` | Prepare the selected input without running performance. |
| `perf.sh --perf-fast` | One full selected sample; the default mode. |
| `perf.sh --perf-samples N` | N independent sequential samples of the same selected input. |
| `perf.sh --setup fresh\|clone` | Rebuild the initial Store or copy an authenticated prepared Store, outside operation timing. |
| `verify.sh` | One identity-pinned verification invocation; never a performance campaign. |

Initialization cases reuse source fixtures but always create a fresh output
Store. They reject `--setup clone`. Proof-only families reject performance.
`--smoke` excludes high operation tiers and requires no more than 50,000,000
fixture bytes and 1,000 regular paths. The 500 MiB capped-result replacements
are large-only, not automatically reduced or executed as smokes.

## Verification

Supply the exact case, seed/repetition, image, source and input identities
recorded in the performance header:

```bash
families/init_namespace/verify.sh \
  --case namespace-100-compact-v3 --seed 1 \
  --image "$IMAGE" --source "$SOURCE_SEAL" --input "$INPUT_ID"
```

Inherited SDK cases use `--repetition` instead of `--seed`. Include the
matching `--setup` strategy for post-initialization cases. All family
launchers use the one `verify-selected.py` implementation and existing
independent Rust oracles.

Verification has a 45-second work allowance within a hard 59-second
end-to-end deadline, including setup, cleanup and receipt publication.
Outcomes are PASS, FAIL, TIMEOUT or INCOMPLETE. Unsupported long proofs
are not shortened and called passed. Verification never runs concurrently
with performance.

## Results and retention

Each invocation creates a unique output directory by default; `--output`
selects an explicit directory. Normal retained output is one `perf.jsonl`
or `verification.json`, plus a bounded `failure.log` when needed.
Performance logs contain identities, sample timers/resources, status and a
summary. A performance PASS does not imply verification PASS.

Prepared data is a bounded disposable Docker cache. Independent sample
Stores and containers are removed; no input tree or database is exported
by default. Cached/copied input is not claimed to be cold.

Compact-v2 ordinary/reuse fixtures and compact-v3 namespace profiles have
distinct identities from historical lower tiers. Do not mix unlike profiles
in scaling comparisons.

## Historical entrypoints

Root-level shell wrappers and the obsolete host-based runner/custody/report
pipeline have been removed. There is no compatibility or archive directory.
Use the corresponding frozen Git revision to reproduce historical
commands and reports; current family scripts are the supported interface.
Historical roadmap/results documents remain historical records, not
instructions to run deleted scripts on this revision.

See the [current infrastructure specification](../../docs/roadmap/0.1/0.1.3/benchmark-infrastructure-optimization-spec.md)
and [issue #45](https://github.com/Ephemeral-AI-Lab/layerfs/issues/45).
