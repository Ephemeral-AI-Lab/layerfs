# fs-bench-pro

See the [benchmark quick note](QUICKSTART.md) for the current environment and fast iteration commands.

Selected benchmarks with real LayerFS daemon/FUSE execution. The default
`host-store` topology runs SDK/Workspace/SQLite on macOS and daemon/FUSE/workloads
inside Docker Linux. The explicit `docker` topology keeps product work and
Stores in Linux. No project bind mounts, data volumes or Docker socket are used.

## Source layout

```text
benchmark/fs-bench-pro/
  Cargo.toml
  Dockerfile.layerfs
  README.md
  QUICKSTART.md
  verify-selected.py
  shared/
    runner.py
    runtime.py
    daemon-entrypoint.sh
    test_*.py
  families/<family>/
    mod.rs
    setup.sh
    perf.sh
    verify.sh
  src/
    main.rs
    infra.rs
    ... shared operation and oracle implementations
  workload/
    main.rs
    workspace_common.rs
    workspace_registry.rs
    ordinary_workloads.rs
    dedup_workloads.rs
    reliability_workloads.rs
    sdk_edit_common.rs
```

The 18 current family folders are the public entrypoints. Their shell scripts
only bind the family and forward arguments to shared infrastructure. Every
family uses the same `mod.rs` plus `setup.sh`/`perf.sh`/`verify.sh` layout.

`src/` contains the SDK/Store coordinator and verification code. `workload/`
contains the standalone workload helper and Rust code it shares with the
coordinator. `sdk_edit_common.rs` supplies fixtures, plans, and identities for
the three current SDK edit families. The retired `edit_same_count` and
`edit_count_changing` definitions, commands, and tests have been removed.
`shared/` contains Python infrastructure and its tests.

## Build and run

Run these commands from this directory:

```bash
python3 shared/runner.py --build-host
export LAYERFS_BENCH_IMAGE="$(python3 shared/runner.py --build-image)"

families/init_namespace/setup.sh --smoke
families/init_namespace/perf.sh --smoke

families/edit_length_changing/perf.sh \
  --case insert-middle-4k-on-500mib-result-capped-v2-ops-1 \
  --repetition 1 --perf-fast
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


### Host-owned SQLite comparison

The four construction families also support `--topology host-store`. Build the host coordinator with `python3 shared/runner.py --build-host`, then use the same family `perf.sh`/`verify.sh`, selectors and seeds. Pass the frozen compatible Linux image with `--image`; host and image product seals must match. Host binary identity is sealed beside `target/release/fs-benchmark-pro`. The Linux image keeps its original source identity. Default host performance output is `benchmark-results/host-store/results/run-…`; pass an explicit host results directory for verification.

Example (from this directory):

```sh
python3 shared/runner.py --build-host
families/payload_create_read/perf.sh --topology host-store --image "$IMAGE" \
  --case payload-create-500m --seed 1 --setup clone --perf-fast \
  --output ../../benchmark-results/host-store/results/payload-500-s1
```

The host owns the SDK, Workspace manager/capture/spool and local SQLite Store. The existing authenticated ContainerBinding/ProxyHost route connects to the real Linux daemon/FUSE/workload. Docker has no data-sharing mounts or socket; the daemon has one loopback-only published TCP port. The existing container guards still validate resources, image, ownership, device and capabilities. The 2 CPU/2 GiB container limit does **not** cap the host coordinator. Report host process CPU/RSS/I/O separately; moving file cache out of the cgroup and using additional host workers is not a pure efficiency gain.

Prepared cache and samples are local to the ignored project `benchmark-results/host-store/`: `prepared/`, `fixtures/`, `samples/`, `results/`. Workspace masters are closed, self-contained and protected; every invocation byte-copies to an absent independent writable Store, fsyncs, quick-checks and verifies equal hashes/distinct inodes before timing. WAL/SHM/journal sidecars are rejected: this helper is deliberately not a live-database snapshot API. Native imports reuse source fixtures with original modes and always create a fresh output Store. Masters are checked again after samples. Cache compatibility uses schema/format/fixture/seed/initial-state identity, separately from producing and executing code/image provenance. Bump the versioned compatibility contract if canonical format or byte-generation semantics change outside the fixture descriptors. Reuse is bounded to 8 prepared/fixture entries and 10 GiB; only marked benchmark-owned data can be evicted. Completed sample data is removed.

Prepared cache entries are evictable, and samples are disposable. Git ignore is not a backup mechanism. Durable backups require separate storage and retention; ordinary product Store locations remain caller-selected and are outside these cleanup paths. Docker-only results remain evidence for their original topology and must never be relabeled as host qualification.
