# WAL throughout: accepted host Init baseline

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Owner supersession received2026-10-07 from the explicitly dispatched side-chat
> handoff `01a1152b-e93c-72b2-bac1-4d5b210d2830`.

The owner directs: “honestly use the simplified design and honestly report the
regression and record them as the new baseline.” Use WAL throughout Project
Init and Workspace Commit. This supersedes the earlier conditional rule that
selected a private MEMORY import after a WAL regression. Retire the private
creation/promotion path in a new commit; preserve its earlier code history and
all measurement/proof receipts. Ordinary sole-owner seal remains: checkpoint,
checked close, one-file/sidecar verification and the approved narrow macOS
persistent-WAL file-control wrapper. Overlay remains separate MEMORY/OFF.

## Exact accepted baseline scope

The accepted baseline is the existing Disposable1000 WAL-throughout observation
at `c5fae7e3aa783dcdb1c09bf977c0395a5b066dd5`, not a new sample:

| Value | WAL observation | Retained MEMORY control | Delta |
| --- | ---: | ---: | ---: |
| Complete host Init product time | 198720291ns | 155291459ns | +43428832ns (+27.9660145379%) |
| Final allocated Store bytes | 20574208B | 20590592B | −16384B (−0.0795703203%) |

The original1.10× speed comparison remains **FAIL**. Owner acceptance of the
design/baseline does not turn that failure into PASS and establishes no speedup.
This is one WAL observation against retained evidence, not a fresh matched
pair. Allocation machinery and lifecycle changed too; the whole difference is
not conclusively attributed to WAL. The sample covers macOS host Init of1000
files/20MB at the registered seed, source/build/binary/fixture/cache/profile
identities. It establishes no daemon Commit speed, other-size baseline or
complete phase/resource qualification. No additional timing run is requested.

The [original result](PRE-S8-INIT-WAL-RESULT-20261007.md),
[raw receipt](checks/pre-s8-wal-init-20261007/10-wal-sample/receipt.json) and
[identity/operands](checks/pre-s8-wal-init-20261007/11-result.json) retain exact
custody, cold-content proof, oracle scope, cleanup and limitations. The original
eight speed failures, eight strict-allocation failures and all Durable deferrals
remain unchanged. No new numeric ceiling is inferred from accepting this point.

## Shared implementation and separate operation state

Init and Commit reuse Content construction primitives/canonical formats,
Storage Save algorithms and the WAL persistence/history implementation. Their
inputs and mutable producer state remain separately owned. Init performs native
acquisition and initial namespace construction. Commit consumes a stable overlay
capture plus immutable base, builds incremental changes in bounded batches,
conditionally publishes history and installs the new base while retaining later
mutations. Init acquisition/abandoned-operation cleanup is never used by Commit.

Storage permits one active Save per Storage handle. Overlapping producers need
separate Storage/Save state over the same Store; no global mutable Save or mutex
spans an entire Commit. Combined stage/publication, multiple-producer correctness
and live-Workspace Commit remain implementation/proof obligations. WAL selection
alone does not close them.

## Deepest-file withdrawal plan

- Remove `create_for_init`/private selection from Persistence `store/open.rs`.
- Remove private-state/profile branches in `backend/sqlite/connection.rs` and
  `profile.rs`, and private conversion in `seal.rs`. Keep the ordinary WAL seal
  and macOS file-control module exactly as already proved.
- Retire `tests/private_init.rs`; return Project `tests/init_sqlite.rs` to
  ordinary shared WAL creation. Preserve every earlier private-route receipt.
- Update the handbook, core guide and shared-Store architecture, and add an
  explicit supersession note to the dated private-route report/plan.
- Close the single WAL case against resampling and update the pure decision
  reporter/test to the new owner disposition; raw historical decisions remain.
- Preserve R2's independent counter-custody correction and all unrelated work.
  Verify source equality of the restored Persistence scope against the earlier
  WAL checkpoint, build/check the affected scope, and reuse unchanged proofs
  by exact scope. Record the per-commit production LOC; no timing resample.

## Restoration verification

At parent `1e5f5033ff2a265b94cb5ebab66e9f613b8d2400`, the selected Persistence
files and Project Init test are restored byte-for-byte from `c5fae7e3` (receipt01).
The independent R2 counter-custody correction remains. The new evidence directory
is [checks/pre-s8-wal-restoration-20261007](checks/pre-s8-wal-restoration-20261007/).
Receipts02/09 build all targets of Persistence, Project, SDK and Daemon on
macOS and the pinned Linux image; both persistence profiles compile. Receipts03/04
pass4 decision and19 registry tests;05–08 pass scoped warning-denying Clippy,
formatting, the743-file boundary scan and42 guard self-tests. Each test has a100s
wall stop. Receipt10 records only the new owner disposition over unchanged raw
measurement bytes, retaining `speed_gate=FAIL` and the historical conditional.

Unchanged functional Store behavior reuses the exact restored F1–F4 scope and
its host/Linux proofs, recorded in [the foundation report](PRE-S8-F1-F4-20261007.md).
No old campaign or product timing was rerun. Linux uses image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`;
the actual host/container source hashes match before compilation. All new commands
export `LAYERFS_CONSTRUCTION_WORKERS=1`, use locked Cargo1.85.1 and retain root
ARM64 inputs. Durable execution remains **NOT_RUN — deferred by owner for
Disposable-only development**. There are no new product executions, cleanup
attempts or terminal unknowns in this restoration. No F5–F15 claim is added.

Production LOC:170526 ->170470 (delta-56). Core105109 ->105053;
active61944 ->61888; reference65417, excluded predecessors36325 and excluded
integration6840 unchanged. Receipt11 uses the pinned counter over exact parent
and staged product snapshots. This removes the superseded private-import route;
it is not a speed improvement or transport retirement.
