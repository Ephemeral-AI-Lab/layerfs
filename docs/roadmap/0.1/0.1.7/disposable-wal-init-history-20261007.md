# Disposable WAL namespace Init and retained-history selection

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Issue #307. Owner request 2026-10-07: use only Disposable/WAL/OFF until explicit
reauthorization of Durable, run the complete namespace Init matrix, then history
strides 10, 3 and 1, and report every outcome. This is a new selection after the
serverless implementation, not a replay or replacement of the closed one-point
WAL decision. Work stays in the primary checkout on local main.

## Frozen operations, selection and limits

One new candidate sample per case, in this order. All use system SQLite on macOS,
locked Rust 1.85.1 release and repository ARM64 inputs. Image is N/A. The history
rows are explicitly the retained C1/C2/C5 component operation, not daemon/FUSE
or Workspace Commit performance; no host data-path product adapter is created.

| Case suffix after `phase7-sqlite-disposable-` | Operation / fixture | Complete performance / separate proof |
| --- | --- | --- |
| `init-100-owner-wal-v1` | 100 files, 5,000,000 B, `namespace-100-compact-v3` | 30 s / 19 s |
| `init-1000-owner-wal-v1` | 1,000 files, 20,000,000 B, `namespace-1000-compact-v3` | 30 s / 19 s |
| `init-10000-owner-wal-v1` | 10,000 files, 300,000,000 B, `namespace-10000` | 30 s / 19 s |
| `init-100000-owner-wal-v1` | 100,000 files, 500,000,000 B, `namespace-100000` | 30 s / 19 s |
| `history-stride10-group-rows-indexed-wal-v1` | 17 retained states, original pinned history corpus | 60 s / 12 s |
| `history-stride3-group-rows-indexed-wal-v1` | 53 retained states, same corpus | 170 s / 12 s |
| `history-stride1-group-rows-indexed-wal-v1` | 157 retained states, same corpus | 300 s / 30 s |

These are the existing owner-approved family allowances from the
[earlier selection](../../../../core/docs/issues/307/INCUMBENT-RESTORATION-PLAN-20261007.md),
not new test timeouts. Ordinary tooling tests retain a 100 s wall stop. No limit,
workload, worker count, cold contract or oracle is changed after an observation.
The complete performance command includes cold attestation, driver and observed
terminal custody; build/preparation and the independent proof are separate.

Init calls public `Handles::create -> layerfs_project::init -> Handles::seal`;
its inner clock covers create through checked seal/close, with one public Init
call, four supported Init constructors and environment construction workers=1.
Monolithic acquisition schema stays selected. Reuse identity-checked prepared
sources; fresh output Stores are part of the measured operation.

History reuses `benchmark_history`: original full corpus transitions, canonical
construction, Save and retained History, one producer, GroupRowsIndexed schema3,
all 17/53/157 states. Its complete inner lifecycle includes source/corpus open,
all states, required database cold boundaries, custody, seal/close and census.
No state is shifted to setup. Reuse the immutable corpus, not a prepared Store.
The existing per-state canonical roots and counts remain frozen.

## Cache, proof and comparison scope

Use the existing sealed native cold helper: source content must have zero
observed resident pages after invalidation. History also attests Store/extant
WAL content at every state boundary. Metadata residency and bounded in-process
buffers retain their original declared exclusions. A failed attestation is
INELIGIBLE, never a fast sample. No automatic preconditioning/retry loop.

Init's separate oracle checks every path/kind and the declared deterministic
file metadata/content sample; report sampled bytes rather than claiming a full
payload oracle. History uses all-state structure, independent retained root
pins, custody, closed SQL/canonical census and the existing five-anchor bounded
content proof (8 MiB authenticated / 32 MiB acquired maximum). WAL proof uses an
independent byte copy and a provisioning writer for read handles, preserving the
original sealed Store byte-for-byte. Proof copy/cold work is inside its proof
budget. Expected roots are derived from hash-checked retained baseline receipts,
whose complete producer roots and native proof results remain available. The
old worktree's separate census/root-pin files are unavailable; do not claim to
have revalidated those missing files or reuse the old proof as a current proof.
Run the full declared current census/namespace proof independently. Historical
root expectations get an explicit retained-receipt kind, never a current paired
harness identity.

No new baseline arm runs. Preserve and cite the previous Disposable MEMORY
Init and history observations as historical comparisons. Report raw time/storage
deltas, plus the existing 1.10 arithmetic, with the comparison explicitly
unpaired and admission-ineligible. Also report the accepted 1,000-file WAL point
as context. The historical eight Init speed and eight strict-allocation failures
stay unchanged; removed strict allocation selections remain NOT_RUN. History
allocated Store ceilings remain 54,278,964 / 70,427,034 / 92,342,273 B. No new
latency, RSS or phase-memory threshold is invented. No release speedup claim.

## Files and execution plan

| File | Required change / reuse |
| --- | --- |
| Root/core/benchmark `AGENTS.md` | Only permitted Store profile and indefinite Durable execution ban |
| `families/phase7_sqlite.py` | Seven new case IDs; reject new Durable execution; old cases unchanged |
| `families/serverless_init.py` | Reuse WAL Init path for new four-tier selection with frozen retained references |
| `families/phase7_history.py` | Reuse measured history lifecycle; current WAL readback and explicitly historical pins |
| New `shared/disposable_wal.py` and registry | Hash-bound historical inputs, profile policy, selection and honest comparison |
| `shared/phase7_history_proof.py` | Validate declared historical pin reuse; independent proof copy for WAL |
| `layerfs-project/examples/verify_history.rs` | Current profile identities and writable provisioning before read handles; seal proof copy |
| Owning harness tests | Cover registry limits, disabled Durable, evidence/profile tampering and retained-pin scope |
| Existing drivers, cold helper, observer, prepared namespace/corpus inputs | Reuse unchanged measured algorithms and workload/oracle scope |

Freeze this plan first, then freeze final harness/build identity before sampling.
Retain numbered append-only checks under
`core/docs/issues/307/checks/disposable-wal-matrix-20261007/`, raw run directories
under the worktree's ignored benchmark results, and a final campaign report.
No product source change is planned. Every local commit records exact production
LOC. Preserve the three non-input notes, four named containers and all historical
receipts. No new worktree, push, deployment or unrelated cleanup.
