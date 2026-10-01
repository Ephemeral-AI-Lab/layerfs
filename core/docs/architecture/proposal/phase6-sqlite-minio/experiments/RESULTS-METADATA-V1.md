# Daemon metadata engine v1 results

> **Status: Research; informative and not a product contract.**
> 2026-10-01; [#294](https://github.com/Ephemeral-AI-Lab/layerfs/issues/294), native sub-issue of #293.
> Prospective specification: [METADATA-V1](METADATA-V1.md), committed at `15b2ca92091188b4282a0d104ec50985933d6ea0` before implementation.
> Measured prototype source: `81ddccbba97076ab940092b5b15c5fbc7938d2cf`.

## Decision

The indexed namespace and generation representation is a useful candidate for
further design. Reusing one engine removes repeated opens/configuration from the
prototype workload. Repeated interval overwrites retain final state without a
recursive write log, and immutable application generations allow successor
updates between short database reads.

**Do not adopt the v1 bulk extent transaction unchanged.** Full-range overwrite
and truncate of 8,192 fragmented extents change 8,194 and 7,681 rows respectively
in one transaction, exceeding the predeclared 512-row target. Both byte/source
oracles pass. This is a representation FAIL independent of their elapsed time.
A staged range update or logical range invalidation with atomic visibility and
bounded retirement needs its own design/oracle experiment before implementation.
No error-driven fallback, quota increase or new worker was used.

Physical memory admission remains **INCOMPLETE**. The actual provider is
`/usr/lib/libsqlite3.dylib`, SQLite 3.51.0, compiled `DEFAULT_MEMSTATUS=0`.
Original v1 receipts contain raw zero allocation counters; those values are
**UNAVAILABLE**, not zero usage. A separate [provider observation](evidence/metadata-v1/provider-memory-diagnostic.json)
confirmed global counters zero while per-connection cache/statement allocations
were nonzero. No performance arm was repeated. OS/file-cache containment is also
unavailable in this host prototype. Future operation receipt schema 2 corrects
missing counters to null; that correction does not qualify or relabel v1 rows.

All numerical observations have uncontrolled host cache and remain **INELIGIBLE**,
`performance_claim=false`, `admission_eligible=false`. These are component
workloads, not LayerFS Commit/Exec latencies or Phase 4.5 speed comparisons.

## Complete first collection

Each case ran once. Timed work includes counter instrumentation; selected
captures include streaming result delivery. Namespace rows combine lookup,
full listing and mutation; they are not single-lookup latency. Lifecycle rows
include opens/configuration/closes; ordinary rows report opening separately.
All full-state/source-coordinate verifiers and runtime-owner cleanup passed.
No timer/worker/profile limit changed after collection.

| Case | Work ms | Complete command ms | Separate verifier ms | Max changed rows / transaction | Transaction target |
| --- | ---: | ---: | ---: | ---: | --- |
| namespace-128 | 5.142 | 12.677 | 2.274 | 5 | PASS |
| namespace-10000 | 7.173 | 15.051 | 34.625 | 5 | PASS |
| namespace-100000 | 24.303 | 37.223 | 360.028 | 5 | PASS |
| deep-270 | 175.221 | 182.700 | 1.465 | 1 | PASS |
| lifecycle-reopen-128 | 20.430 | 25.344 | 0.596 | 1 | PASS |
| lifecycle-persistent-128 | 4.512 | 9.448 | 0.608 | 1 | PASS |
| extent-repeated-4097 | 110.881 | 118.775 | 12.939 | 5 | PASS |
| extent-append-512 | 14.744 | 21.254 | 12.093 | 2 | PASS |
| extent-dispersed-512 | 16.009 | 23.402 | 14.227 | 5 | PASS |
| extent-fragmented-8192 | 15.053 | 22.949 | 27.731 | 8194 | FAIL |
| extent-truncate-regrow | 14.198 | 22.989 | 17.871 | 7681 | FAIL |
| generation-10000 | 49.646 | 59.512 | 56.949 | 128 | PASS |
| overlap-1 | 7.157 | 15.807 | 22.423 | 128 | PASS |
| overlap-2 | 13.745 | 22.013 | 44.182 | 128 | PASS |
| overlap-3 | 20.706 | 29.206 | 65.376 | 128 | PASS |

All command walls were below 15s and all separate verifiers below 9.5s. The
15-case performance command walls sum to 618.349ms; this sum excludes
setup and separate verification and is not an end-to-end daemon metric.

## Count evidence and scope

- Namespace 128/10,000/100,000 completed 833/872/1,223 instrumented statements;
  the increasing term is required keyset listing. Fullscan-step and sort counters
  were zero in every timed workload. SQL VM work still increases with required
  rows; zero fullscan does not establish global algorithmic complexity.
- Deep traversal performed 27,000 indexed component queries for 100 paths of
  270 components, followed by one changed rename row. This avoids descendant
  path rewriting in the prototype. Canonical certification/root construction
  and the actual #289 product routes remain unqualified.
- Lifecycle reopen/persistent executed identical 512 workload statements and
  4,864 VM steps. Opens were 128 versus 1. Their 20.430ms/4.512ms observations
  include real lifecycle differences; cache ineligibility prevents a product
  speedup claim or extrapolation to full Commit.
- 4,097 repeated overwrites leave exactly three current extents. Appends leave
  513 and dispersed writes 747 extents; those populations reflect final content
  fragmentation, not retained historical WRITE records.
- Eight generation rounds stream 80,000 selected inode rows, preserve exact
  captured/live values, and finish with zero obsolete inode/capture rows.
  Capture changes two Workspace/capture records; retirement batches are at most
  128 rows. The work includes 1,024 successor mutations and all result output.
- Overlap 1/2/3 uses one shared engine and 1/2/3 fixed metadata mutators, with one
  captured-state construction reader. Each mutator completes 128 updates before
  selected traversal resumes. Exact old/new rows, duplicate-capture refusal,
  release and cross-Workspace isolation passed. This proves the declared
  interleaving, not scheduler fairness, FUSE callbacks, independent Exec or
  multiple SQLite-writer throughput.
- Maximum cursor field bytes remained within 16,384 in every case. This excludes
  Rust container overhead and engine/kernel allocations and is not whole-process
  memory containment.

## Identities, reproduction and retained evidence

[Identity](evidence/metadata-v1/identity.json),
[complete summary](evidence/metadata-v1/summary.json) and
[original full-artifact hash manifest](evidence/metadata-v1/manifest.json)
retain exact raw values. Each case folder holds its receipt, operation record,
stdout/stderr and verifier result. Full databases/selected streams remain in the
owned ignored raw folder and are hashed by the original manifest; they are not
large Git inputs. Published small evidence is an exact copy, not recomputed rows.

```sh
cargo +1.85.1 build --release --locked --manifest-path core/benchmark/phase6-metadata/Cargo.toml
cargo +1.85.1 clippy --release --locked --manifest-path core/benchmark/phase6-metadata/Cargo.toml -- -D warnings
cargo +1.85.1 fmt --manifest-path core/benchmark/phase6-metadata/Cargo.toml -- --check
python3 -m unittest discover -s core/benchmark/phase6-metadata -p 'test_*.py'
python3 core/benchmark/phase6-metadata/run.py --output benchmark-results/phase6-metadata/run-v1 --masters benchmark-results/phase6-metadata/masters-v1
```

The collection command above names the actual invocation at measured source
`81ddccbba`; it is **not** permission to overwrite or rerun that evidence.
The source was clean/sealed before collection. A worktree-local nonblocking lock
covered collection; no local build overlapped a timed workload. Prepared masters
were acquired once and independently copied; no APFS clone or cold-cache claim.
The binary used repository-root ARMv8 flags and product-parity dependency versions.
The measured executable was reproduced byte-for-byte from its committed Rust
source after the memory-format correction and retained in the owned immutable
`binary-archive/cc0601c273d27f3404aab13f6fe2917e4049930b2cc0dc1e5903488fd7921603/`
folder; rebuilding did not repeat any performance operation.
An initial fresh-lockfile parity failure was corrected before any measurement by
reconciling the prototype against `core/Cargo.lock`; no third-party changes.

Checks: locked release build, warning-denying release Clippy, formatting,
independent oracle corruption/resurrection tests, local links, diff checks and
raw manifest validation passed. The memory-format correction rebuilt/Clippy
checked its changed source and used an inspection-only command to confirm null
availability; it did not repeat the v1 performance selection.

Core product build/test suites, original benchmark families, daemon/FUSE/Linux,
MinIO/CAS/delta/canonical oracles, public Commit/Exec, process-death/Unknown,
cloud/WAL/fsync and physical containment are NOT_RUN for this prototype. No
product implementation changed, and no issue or release admission is complete.

## Per-commit source-size record

| Commit | Reference before/after | Core before/after | Combined before/after | Delta |
| --- | --- | --- | --- | ---: |
| `15b2ca920` prospective specification | 65,417 / 65,417 | 70,279 / 70,279 | 135,696 / 135,696 | 0 |
| `81ddccbba` standalone prototype | 65,417 / 65,417 | 70,279 / 70,279 | 135,696 / 135,696 | 0 |

Method: `tools/production_loc.py` SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`,
same counter over Git archives of each first parent and final staged tree;
committed parent/tree equality verified. Scope includes reference/Core shipped
SQL and excludes research prototype SQL, harness/tests/tools/docs and generated
artifacts. This is research outside product source, with no product retirement
or migration. The report/counter-availability correction checkpoint records its
own exact comparison in its commit message and issue handoff.

## Next design gate

Freeze a bounded representation for bulk overwrite/truncate that preserves exact
source selection, atomic visibility, shrink/regrow zeros, captured generations
and progress while retiring large ranges in indexed batches. Independently test
that selected representation and secure real resource observation before using
this schema as the Phase 6 daemon engine contract. Preserve these v1 failures.
