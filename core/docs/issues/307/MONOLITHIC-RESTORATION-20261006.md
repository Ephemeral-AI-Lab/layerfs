# Monolithic restoration and cluster-one-end regression

> **Status:** Owner-selected scoped withdrawal and full final-source campaign.
> S7/S8/S9 remain incomplete. No further layout mechanism is selected.

The owner explicitly withdraws the payload-segment design and restores product
behavior from `9b74ac035faaf271edd01726913f63f084adaaa2`, tree
`29e6b463c2cec61ee645e20ac452364426fc1494`. Product files under `core/crates`
are byte-equal to that source, including the acquisition entry batching and pack
reservation correction. S7 overlay/daemon accounting and S9 authenticated/runtime/
Bridge work are preserved. This is a scoped withdrawal of the newer Persistence
format/provider/configuration/counters and its driver/harness options, not a
repository reset, old Service revival or discarded per-Init SQLite prototype.

Active default Monolithic creation and old supported schemas1–6 remain; schemas7/10
are refused without migration or fallback. The new-layout external tests/examples
are preserved as inactive [historical source](checks/monolithic-restoration-20261006/withdrawn-source/)
and in their original commits. The VFS/SQL diagnostics, proposals, original results
and every FAIL/NOT_RUN receipt stay retained. Unrelated owner notes, side documents,
closed S5/S6 handoff, four containers, other checkout state and root reference remain.

The latest payload source `b1277cf1f` is retained before the withdrawal checkpoint:
91,055,834ns and5,308,416B. Its old competitive speed/joint FAIL, storage/root/cold/
proof/cleanup/caps PASS are finalized in the [barrier ledger](checks/durable100-barriers-results-20261006/ledger.json).
The observed83,626,792ns prior Monolithic value is historical evidence; comparing
it with79,759,708ns is a4.85% arithmetic difference, not a retroactive regression PASS.

The main regression control is **the cluster-one-end public Project Init**, source
`197d2fb7d0a141d7a9350852022febeec3255bf2`, tree
`dbbe49212b26294024be28986e868c27f4de4825`. Restored handbook source
`8cbeadef07dc9ac1e79cd59eaee3dca494e2ff87` has the identical `core/crates` tree
`c558ee85e1e751d03378b91cbd950df5fb673872`. Its Monolithic public Project driver
uses real ordered-run acquisition, Durable WAL/FULL/fullfsync or Disposable
MEMORY/OFF, four constructors and environment workers1. The restored candidate
uses the later real backed-acquisition Project route. The older7edddb Service
MEMORY/OFF split-Store comparison is a separately disclosed competitive control.

All eight unchanged controls passed the [identity/closure audit](checks/monolithic-restoration-20261006/cluster-one-end-baseline-audit.json):
exact source/tree/product/compilation/dependency/config seals, actual Project driver
source, immutable release driver/verifier/helper binaries, original raw receipt and
complete manifest custody, workload/seed, profiles, cold zero-residency attestation,
root/sample proof, cleanup and30/19s caps. Original candidate rows in #302 become
this new context's regression controls. Their old pair FAIL/PASS labels, dates,
case IDs and seals are preserved. No unchanged qualified baseline is resampled.

| Same-profile control | Complete product ns | Final allocated B |
| --- | ---: | ---: |
| Durable100 | 79,759,708 | 5,255,168 |
| Durable1000 | 201,566,000 | 20,545,536 |
| Durable10000 | 2,492,429,625 | 305,070,080 |
| Durable100000 | 7,724,523,333 | 514,965,504 |
| Disposable100 | 38,747,750 | 5,222,400 |
| Disposable1000 | 129,258,375 | 20,537,344 |
| Disposable10000 | 1,645,276,292 | 305,074,176 |
| Disposable100000 | 5,558,569,958 | 514,940,928 |

The prospective [full matrix](../../../../docs/roadmap/0.1/0.1.7/monolithic-cluster-one-regression-20261006.md)
selects all eight restored candidates at one clean final product source. Lead with
same-profile latency/allocation versus these controls. Speed is
`10*candidate_ns <= 11*cluster_one_end_ns`; strict storage is
`candidate_final_DB+WAL+SHM <= same_profile_control_final_total`. No allocation
allowance is invented. Show raw growth/deltas/percentages and speed/storage separately;
joint PASS requires every required condition. Keep the older competitive gate
separate and never relabel its historical failures.
