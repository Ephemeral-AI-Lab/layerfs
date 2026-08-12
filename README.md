# Ephemeral AI FS

> Branch-aware, content-addressed storage for multi-agent workspaces.

[![M2 accepted](https://img.shields.io/badge/M2-accepted-2ea44f)](./docs/evidence/m2/exit.md)
[![M3 accepted](https://img.shields.io/badge/M3-accepted-2ea44f)](./docs/evidence/m3/exit.md)
[![M4 accepted](https://img.shields.io/badge/M4-accepted-2ea44f)](./docs/implementation/implementation-plan.md#7-milestone-4-branches-and-publication)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

Ephemeral AI FS gives Ephemeral AI Computer a durable workspace layer where agents can
read and edit files independently, share unchanged content, and publish changes through
one authoritative SQLite-backed workspace.

[Quick start](#-quick-start) · [How it works](#-how-it-works) ·
[Benchmarks](#-benchmark-progress) · [Milestones](#-milestone-progress)

## ✨ At a glance

| Shared content                                                               | Local edits                                                                           | Durable publication                                                                                      |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| SHA-256 CAS objects and authenticated manifests keep unchanged bytes shared. | FastCDC and bounded local-rebuild paths keep edits local when the workload allows it. | SQLite transactions, leases, revisions, quotas, and conflict checks protect the authoritative workspace. |

## 🧭 How it works

The C3 storage pipeline described in
[Part III of the Agent Infra Book](https://github.com/agent-infra-foundation/agent-infra-book/blob/main/cloudflare/computer/chapters/PART-III.md)
is represented directly in this repository. The pipeline is easier to read as three
small flows:

### 1. Write and store

```text
Agent edit
    |
    v
EphemeralFS operation
    |
    +--> FastCDC chunks -----> SHA-256 CAS objects
    |
    +--> Ordered entries ----> Authenticated Merkle manifest
                                      |
                                      v
                              SQLite transaction
                                      |
                                      v
                              Immutable revision
```

### 2. Read a workspace

```text
Main head or branch head
          |
          v
Authenticated manifest
          |
          v
Bounded manifest cursor
          |
          v
Shared CAS objects
          |
          v
Bounded file read or stream
```

### 3. Publish a branch

```text
Private branch
      |
      v
Verify expected base revision
      |
      +---- base unchanged ----> Atomic publish to main
      |
      +---- base changed -------> Explicit conflict
```

An edit follows this path:

1. Split changed bytes into content-defined chunks.
2. Store new chunks by hash and reuse existing CAS objects.
3. Build and authenticate a new ordered manifest.
4. Commit revision, lease, quota, and head changes atomically in SQLite.
5. Read from a branch head or publish after verifying the expected base revision.

### C3 mechanisms implemented here

| C3 mechanism                     | Repository implementation                                                             | Status                                        |
| -------------------------------- | ------------------------------------------------------------------------------------- | --------------------------------------------- |
| CAS: share unchanged bytes       | `packages/fs/src/cas/` and `packages/fs/src/sqlite/content-repository.ts`             | ✅ M1/M2 accepted                             |
| CDC: reconnect after local edits | `packages/fs/src/cdc/fastcdc.ts` and `packages/fs/src/operations/local-rebuild.ts`    | ✅ M3 durable local-rebuild path              |
| Authenticated manifests          | `packages/fs/src/manifests/` and `packages/fs/src/sqlite/manifest-tree-repository.ts` | ✅ M2 accepted                                |
| COW pages and patches            | `packages/fs/src/cow/pages.ts` and `packages/fs/src/sqlite/overlay-repository.ts`     | ✅ M4 branch overlays and bounded publication |
| Branch bases and conflict checks | `packages/fs/src/branches/`, `branch-repository.ts`, and `branch-engine.ts`           | ✅ M4 accepted                                |
| One durable authority            | `packages/fs/src/sqlite/driver.ts` and transaction-scoped repositories                | ✅ M2 accepted                                |

The result is a filesystem foundation for C3-style execution: shared immutable data,
private agent state, bounded edits, and explicit publication into one durable main view.

## 💡 Why this design

Full workspace copies make branch creation and repeated edits scale with workspace size.
Ephemeral AI FS separates immutable content from workspace state instead:

- **CAS** shares identical content across files, revisions, and agents.
- **CDC** helps insertions and deletions reconnect with unchanged content.
- **Merkle manifests** provide authenticated, ordered file content.
- **SQLite** supplies transactions, quotas, leases, and recovery boundaries.
- **Explicit fallbacks** preserve correctness when an edit exceeds a bounded local path.

The system may use a streamed rebuild for a large or unsupported edit, but it must not
trust a stale derived index or return incorrect bytes.

## 🚦 Milestone progress

```text
M0 foundation       ✅
M1 content engine   ✅
M2 SQLite storage   ✅  latest accepted milestone
M3 filesystem I/O   ✅  accepted milestone
M4 branches         ✅
M5 maintenance      ⏳
M6–M10 integration  ⏳
```

| Milestone | Scope                                                                       | Status          |
| --------- | --------------------------------------------------------------------------- | --------------- |
| M0        | Repository and test foundation                                              | ✅ Accepted     |
| M1        | CAS, CDC, COW, patches, and manifests                                       | ✅ Accepted     |
| M2        | Transactional SQLite storage and Node driver                                | ✅ **Accepted** |
| M3        | Filesystem namespace, revisions, and I/O                                    | ✅ **Accepted** |
| M4        | Branches and publication                                                    | ✅ **Accepted** |
| M5        | Maintenance, recovery, and bounded scale                                    | ⏳ Planned      |
| M6–M10    | Cloudflare parity, Node VFS, replication, release, and Computer integration | ⏳ Planned      |

M3 is accepted with read-path batching, durable bounded local reconnection, and async
write-path hashing. Its A6 benchmark gate is 500 scattered edits in 20 seconds; the
latest clean run completed 500/500 in 9.975 seconds.

See the [implementation plan](./docs/implementation/implementation-plan.md),
[M2 exit record](./docs/evidence/m2/exit.md), and
[M3 improvement plan](./docs/benchmarks/m3-improvements.md).

## 📊 Benchmark progress

The mini-benchmark measures the file-backed Node SQLite engine directly. It does not
include FUSE, page-cache effects, or the complete Computer execution path. The tables
compare the pre-improvement baseline, the accepted M2 implementation, and the accepted
M3 implementation.

### Headline results

| Metric              |   Baseline |          M2 |            M3 |
| ------------------- | ---------: | ----------: | ------------: |
| Cold 100 MiB write  | 17.9 MiB/s |  44.2 MiB/s |    60.0 MiB/s |
| Cold 100 MiB read   | 43.8 MiB/s | 118.1 MiB/s |   259.6 MiB/s |
| Warm 100 MiB read   | 44.4 MiB/s | 118.6 MiB/s | 2,921.5 MiB/s |
| A1 write statements |     12,472 |       2,880 |         1,174 |
| Evidence checks     |          — |   99 passed |    156 passed |

### Workload results

| Workload                        |      Baseline |            M2 |                            M3 |
| ------------------------------- | ------------: | ------------: | ----------------------------: |
| 4 KiB random read               |    2.85 ms/op |    1.17 ms/op |               0.57–1.02 ms/op |
| 100 MiB materialization         |    33.8 MiB/s |    67.9 MiB/s |                   108.5 MiB/s |
| 100 × 1 MiB materialization     |    33.7 MiB/s |    64.6 MiB/s |                    98.1 MiB/s |
| 100 one-byte edits              |        8.95 s |        4.44 s |                        2.13 s |
| Mixed workspace, cold / warm    | 5.99 / 5.65 s | 2.28 / 2.33 s |                 2.29 / 2.30 s |
| Three one-byte edits on 100 MiB |        18.6 s |         9.4 s |                     70.676 ms |
| A6: 500 scattered edits         |             — |             — | **500/500 in 9.975 s — pass** |
| Workerd write-path hashing      |             — |    69.3 MiB/s |                   383.5 MiB/s |

The 100,001-entry closure test completed with 4,655 reconciliation statements, or 0.0465
statements per manifest entry. Exact quota accounting and deduplication behavior were
preserved across M2 and M3.

### M4 branch-engine benchmark

The smaller branch benchmark is a non-gating public-API benchmark covering 1, 5, and 10
branches, up to 1,000 changed paths, COW pages, structural patches, conflicts, replay,
and limit rejection. Results below are from the local Windows x64 validation machine;
preparation and publication are measured separately.

| Workload                     | Preparation | Publication | Outcome                |
| ---------------------------- | ----------: | ----------: | ---------------------- |
| 5 branches × 100 paths       |      2.85 s |      0.58 s | 5 merges, 0 conflicts  |
| 10 branches × 100 paths      |      5.78 s |      1.23 s | 10 merges, 0 conflicts |
| 5 same-inode writers         |       43 ms |      6.4 ms | 1 merge, 4 conflicts   |
| 10 same-inode writers        |       61 ms |      9.0 ms | 1 merge, 9 conflicts   |
| 2 hard-link aliases          |       12 ms |     12.9 ms | 1 merge, 1 conflict    |
| 500 COW edits                |      474 ms |       51 ms | 1 merge                |
| 500 structural patches       |      4.21 s |      5.3 ms | 1 merge                |
| Replay after physical reopen |     95.6 ms |      6.9 ms | 0.26 ms replay         |

The complete reduced matrix finished with **20/20 cells passing in 18.5 seconds**; a
three-trial conflict run passed **3/3 cells**. Run it with:

```bash
pnpm bench:branches
```

Most branch cells use an in-memory database for repeatability; the replay cell uses a
file-backed database and physical reopen. These results supplement, but do not replace,
the M4 correctness and fault suites.

<details>
<summary>🧪 Benchmark methodology and caveats</summary>

- Runtime: Windows x64, Node 24.11.1, file-backed SQLite, WAL, `synchronous=FULL`.
- Fixtures: deterministic 100 MiB, 100 × 1 MiB, and mixed-workspace workloads.
- The matrix is a single-trial engineering benchmark with approximately 10–20% variance.
- M3's A6 acceptance gate is 500 scattered one-byte edits in ≤20 seconds. The former
  1,000-edit target remains a documented SQLite WAL/fsync floor on the validation
  hardware, not an M3 acceptance requirement.
- Read-cell resident-memory peaks are conservative proxies because the harness currently
  samples the stream before consumption completes.

Full methodology and raw artifacts are in the
[benchmark report](./docs/benchmarks/m2-minibench.md) and
[M3 evidence](./docs/evidence/m3/exit.md).

</details>

## 🚀 Quick start

Requirements: Node `>=22.13` and pnpm `10.32.1`.

```bash
pnpm install
pnpm validate:accepted
```

Run the M2 storage suite directly:

```bash
pnpm test:m2
```

Run the storage engine benchmark:

```bash
node tests/performance/mini-bench.mjs
```

Run the smaller branch-engine benchmark:

```bash
pnpm bench:branches
```

## 🗂️ Repository map

```text
packages/fs/                  Host-neutral filesystem core and algorithms
packages/sqlite-node/         File-backed Node SQLite adapter
packages/sqlite-cloudflare/   Cloudflare/workerd adapter
packages/node-vfs/            Node VFS integration
packages/replication/         Replication package
tests/algorithms/             M1 content and manifest tests
tests/storage/                M2 SQLite storage tests
tests/conformance/            M3 filesystem conformance tests
tests/performance/            Storage and branch benchmark harnesses and artifacts
docs/implementation/          Milestone plan and acceptance criteria
docs/evidence/                Accepted milestone evidence
docs/benchmarks/              Benchmark plans, results, and improvement targets
```

## 🛡️ Evidence and next steps

- [M2 acceptance evidence](./docs/evidence/m2/exit.md)
- [M2 benchmark details](./docs/benchmarks/m2-minibench.md)
- [M3 acceptance evidence](./docs/evidence/m3/exit.md)
- [M3 sequenced improvement plan](./docs/benchmarks/m3-improvements.md)
- [Full implementation plan](./docs/implementation/implementation-plan.md)

The next milestone is M5 maintenance, recovery, and bounded scale.

## 📄 License

Ephemeral AI FS is released under the [MIT License](./LICENSE).
