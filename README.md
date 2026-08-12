# Ephemeral AI FS

> Branch-aware, content-addressed storage for multi-agent workspaces.

[![M2 accepted](https://img.shields.io/badge/M2-accepted-2ea44f)](./docs/evidence/m2/exit.md)
[![M3 accepted](https://img.shields.io/badge/M3-accepted-2ea44f)](./docs/evidence/m3/exit.md)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

Ephemeral AI FS gives Ephemeral AI Computer a durable workspace layer where agents can
read and edit files independently, share unchanged content, and publish changes through
one authoritative SQLite-backed workspace.

[Quick start](#-quick-start) · [How it works](#-how-it-works) ·
[Benchmarks](#-m3-benchmark-progress) · [Milestones](#-milestone-progress)

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

| C3 mechanism                     | Repository implementation                                                             | Status                                               |
| -------------------------------- | ------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| CAS: share unchanged bytes       | `packages/fs/src/cas/` and `packages/fs/src/sqlite/content-repository.ts`             | ✅ M1/M2 accepted                                    |
| CDC: reconnect after local edits | `packages/fs/src/cdc/fastcdc.ts` and `packages/fs/src/operations/local-rebuild.ts`    | ✅ M3 durable local-rebuild path                     |
| Authenticated manifests          | `packages/fs/src/manifests/` and `packages/fs/src/sqlite/manifest-tree-repository.ts` | ✅ M2 accepted                                       |
| COW pages and patches            | `packages/fs/src/cow/pages.ts` and `packages/fs/src/sqlite/overlay-repository.ts`     | 🧱 Foundation exists; efficient branch editing is M5 |
| Branch bases and conflict checks | `packages/fs/src/branches/`, `branch-repository.ts`, and `branch-engine.ts`           | 🚧 Full publication is M4/M5                         |
| One durable authority            | `packages/fs/src/sqlite/driver.ts` and transaction-scoped repositories                | ✅ M2 accepted                                       |

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
M4 branches         ⏳
M5 maintenance      ⏳
M6–M10 integration  ⏳
```

| Milestone | Scope                                                                       | Status          |
| --------- | --------------------------------------------------------------------------- | --------------- |
| M0        | Repository and test foundation                                              | ✅ Accepted     |
| M1        | CAS, CDC, COW, patches, and manifests                                       | ✅ Accepted     |
| M2        | Transactional SQLite storage and Node driver                                | ✅ **Accepted** |
| M3        | Filesystem namespace, revisions, and I/O                                    | ✅ **Accepted** |
| M4        | Branches and publication                                                    | ⏳ Planned      |
| M5        | Maintenance, recovery, and bounded scale                                    | ⏳ Planned      |
| M6–M10    | Cloudflare parity, Node VFS, replication, release, and Computer integration | ⏳ Planned      |

M3 is accepted with read-path batching, durable bounded local reconnection, and async
write-path hashing. Its A6 benchmark gate is 500 scattered edits in 20 seconds; the
latest clean run completed 500/500 in 9.975 seconds.

See the [implementation plan](./docs/implementation/implementation-plan.md),
[M2 exit record](./docs/evidence/m2/exit.md), and
[M3 improvement plan](./docs/benchmarks/m3-improvements.md).

## 📊 M2 benchmark

M2's mini-benchmark measures the file-backed Node SQLite engine directly. It does not
include FUSE, page-cache effects, or the complete Computer execution path. Results below
compare the pre-improvement M2 candidate with the accepted M2 implementation after
native host hashing, statement batching, and FastCDC copy reduction.

### Headline results

| Metric                                   |                              Result |
| ---------------------------------------- | ----------------------------------: |
| Cold 100 MiB write                       |  **2.5× faster**: 17.9 → 44.2 MiB/s |
| Cold 100 MiB read                        | **2.7× faster**: 43.8 → 118.1 MiB/s |
| Warm 100 MiB read                        | **2.7× faster**: 44.4 → 118.6 MiB/s |
| A1 write statements                      |      **4.3× fewer**: 12,472 → 2,880 |
| M2 storage/integration/maintenance tests |             **99 passed, 0 failed** |

### Workload results

| Workload                     |        Before |     M2 result |            Change |
| ---------------------------- | ------------: | ------------: | ----------------: |
| 4 KiB random read            |    2.85 ms/op |    1.17 ms/op |       2.4× faster |
| 100 MiB materialization      |    33.8 MiB/s |    67.9 MiB/s |       2.0× faster |
| 100 × 1 MiB materialization  |    33.7 MiB/s |    64.6 MiB/s |       1.9× faster |
| 100 one-byte edits           |        8.95 s |        4.44 s |       2.0× faster |
| Mixed workspace, cold / warm | 5.99 / 5.65 s | 2.28 / 2.33 s | About 2.5× faster |

The 100,001-entry closure test completed with 4,655 reconciliation statements, or 0.0465
statements per manifest entry. Exact quota accounting and deduplication behavior were
preserved.

<details>
<summary>🧪 Benchmark methodology and caveats</summary>

- Runtime: Windows x64, Node 24.11.1, file-backed SQLite, WAL, `synchronous=FULL`.
- Fixtures: deterministic 100 MiB, 100 × 1 MiB, and mixed-workspace workloads.
- The matrix is a single-trial engineering benchmark with approximately 10–20% variance.
- The A6 workload of 1,000 scattered one-byte edits still uses an O(file) fallback on
  large leaves and is not an M2 pass criterion. Bounded local reconnection is an M3
  goal.
- Read-cell resident-memory peaks are conservative proxies because the harness currently
  samples the stream before consumption completes.

Full methodology and raw artifacts are in the
[M2 mini-benchmark report](./docs/benchmarks/m2-minibench.md).

</details>

## 📈 M3 benchmark progress

M3's focused benchmark work improved the 100 MiB edit and read paths while retaining
authenticated manifests, exact reconciliation checks, transaction and memory limits, and
acknowledged SQLite durability.

| Metric                      |                     M3 result |
| --------------------------- | ----------------------------: |
| Cold 100 MiB read           |                   259.6 MiB/s |
| Warm 100 MiB read           |                 2,921.5 MiB/s |
| A5: three one-byte edits    |               70.676 ms total |
| A6: scattered edits         | **500/500 in 9.975 s — pass** |
| Workerd async write hashing |   383.5 MiB/s, 5.53× baseline |

The revised A6 acceptance gate is 500 edits in ≤20 seconds. The former 1,000-edit target
remains a documented SQLite WAL/fsync floor on the validation hardware, not an M3
acceptance requirement. See the [M3 exit record](./docs/evidence/m3/exit.md),
[benchmark report](./docs/benchmarks/m2-minibench.md), and raw
[A6 artifact](./tests/performance/artifacts-m3-final/A6-scattered-edits.json).

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

Run the engine benchmark:

```bash
node tests/performance/mini-bench.mjs
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
tests/performance/            Mini-benchmark harness and artifacts
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

The next milestone is M4 branch publication and conflict handling.

## 📄 License

Ephemeral AI FS is released under the [MIT License](./LICENSE).
