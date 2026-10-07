# Pre-S8 Init WAL decision: one prospective sample

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Registered after F1–F4 `8c66eb8b4`, 2026-10-07, under owner assignment #307.

**Owner supersession2026-10-07:** WAL throughout is the selected design and the
existing WAL observation is accepted as the baseline for its exact scope.
The private-import route/conditional decision below is historical and withdrawn;
original receipts and the speed FAIL remain unchanged. See the
[controlling decision](PRE-S8-WAL-BASELINE-DECISION-20261007.md).

## Deepest-file plan

| File | Requirement |
| --- | --- |
| `core/benchmark/fs-bench-pro/families/phase7_sqlite.py` | Register one new Disposable WAL case and delegate its distinct receipt schema; preserve all old cases/profile expectations. |
| `core/benchmark/fs-bench-pro/families/serverless_init.py` (new) | Reuse existing fixture, cold helper, process supervisor, release build/archive and oracle. Bind the retained memory receipt and record the one owner decision. |
| `core/benchmark/fs-bench-pro/tests/test_serverless_init.py` (new) | Product-free registration, identity, field/cardinality and gate arithmetic checks. |
| `core/crates/layerfs-project/examples/benchmark_init.rs` | Count the actual public Init invocation at the external call site and retain lifecycle remainder arithmetic. No production hook. |
| `core/crates/layerfs-project/examples/verify_namespace.rs` | Open the provisioning write handle before its read set, retaining the same independent oracle; seal after readers close. The macOS limitation is recorded in F1–F4. |
| This plan and `docs/roadmap/0.1/0.1.7/pre-s8-store-init.md` | Prospectively freeze scope, comparison and decision before collection. |

## Frozen selection

Exactly one candidate sample: `phase7-sqlite-disposable-init-1000-serverless-wal-v1`,
seed1, `namespace-1000-compact-v3`, 1,000 regular files, 11 directories and
20,000,000 logical bytes. Reuse the existing local prepared input; output Store
is fresh because Init itself is measured. No baseline is rerun and no other tier
is selected. Durable is NOT_RUN — deferred by owner. Strict-allocation selections
are NOT_RUN — mechanism removed. All eight historical speed and eight
strict-allocation failures remain unchanged.

The latest incumbent-restored memory receipt is the prospective control:
`checks/incumbent-restoration-20261007/raw/phase7-sqlite-disposable-init-1000-incumbent-restored-v1/candidate/receipt.json`,
SHA-256 `eedfd8a8a45e8a59fdae59317cee05471f99e2e118b78ffd344ceb007c21e1a8`,
source `2fced797d14f9d4f6b72c9ad00574976a4dfd5f0`, tree
`9bb5da6f863ecc12b79d465aa2d735e00692a581`. Its complete product time is
155,291,459ns and allocated Store total is20,590,592B. Its manifest is
`e4c484767163117b3c846b6d846cc8157fed052e0b15c70a908ad977f59d881c` and root
`a71b9151bb269cdc3e71d9424e36669edd7d78ed39f9055bc355cebd4d93ec2c`.
The control keeps its original labels/verdicts; it is not a new matched arm.

## Operation, timing and limits

Public `Handles::create` → `layerfs_project::init` → bounded acquisition cleanup
→ `Handles::seal`, through the existing `benchmark_init` example on macOS.
One Project Init call; four supported Init constructors with environment
`LAYERFS_CONSTRUCTION_WORKERS=1`. No daemon, FUSE, SDK transport or install is
part of this component operation. No product branch depends on this case ID.

The inner monotonic `operation_ns` covers fresh create through checked seal and
close, including required cleanup. The external complete performance command
also pays native cold attestation, child launch/exit and output/storage accounting. Final report serialization is
external supervision, separate from performance and verification.
The inherited owner limits remain30s complete performance command and19s
separate verifier. Build/setup are separate; release/locked artifacts only.
The verifier inventories every path/kind/directory metadata and the existing
70 selected files/6,430,827 bytes. It is not a full-content oracle of20MB.
Its writer handle only establishes sidecars before read-only handles and seal;
all this work belongs to the independent verifier, never performance.

Cache contract stays `phase7-cold-content-fresh-database-complete-lifecycle-v1`:
the native two-pass helper invalidates source content and requires zero resident
pages before Init. Metadata residency remains unobserved. The same declared
control contract applies. Lifetime RSS is labelled lifetime, never phase peak;
phase/resource limits remain unavailable and release admission is false for
this selected development decision. Report all raw speed/storage operands.

The existing1.10× arithmetic is reported as `10*candidate_ns <= 11*control_ns`.
Separately, the owner's import-route decision is exact: when eligible complete
WAL time is greater than155,291,459ns, implement a fresh private memory-journal
Init build that changes to WAL at seal. Otherwise keep WAL throughout. Do not
repeat either arm or change a limit. Invalid/incomplete evidence makes this
decision unresolved, not a pass. Compare storage too; a roughly50% slowdown
for roughly5% storage benefit cannot justify accepting the slower route.

## Custody and output

Use the primary checkout's measurement lock, targets and immutable binary
archive. A committed scoped product/harness seal is required. The three inherited
untracked handoff/planning Markdown files remain untouched and unstaged; their
exact paths/hashes are declared non-inputs. Preserve actual global Git dirtiness
in the receipt and separately verify no tracked change or any other untracked
file. Never label the whole working tree clean.

Record source/tree/product/SQL/compilation/harness/dependency/binary/cold-helper/
fixture/oracle/report identities, observed interference, commands, every outcome
and original cleanup in a fresh `benchmark-results/fs-bench-pro/pre-s8-wal-init-*`
directory. Append compact/raw evidence under
`checks/pre-s8-wal-init-20261007/`; preserve any failure or unrun selection.
No measurement has run at this registration.

## Registration checks and source size

Receipts01/03 pass the four product-free selection/control/decision checks;04
passes19 retained-registry/arithmetic/supervision checks. The verifier builds
in02. Receipt05 preserves Clippy's unnecessary non-Drop release failure;06
passes after removing that release, and07 passes formatting. Harness syntax
and local documentation links pass. No production source changes in this
registration; no measurement was run.

Production LOC:170469 ->170469 (delta0). Core105052; reference65417; active
core61887; excluded predecessor36325; excluded integration6840, all unchanged.
Receipt08 uses the pinned counter on exact parent and staged snapshots.
