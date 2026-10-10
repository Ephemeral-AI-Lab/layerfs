# Timing baseline at the R8/R9 close

> **Status:** Measured result, 2026-10-10. One sample per cell; exploratory,
> not admission eligible, not a release claim.

The owner's instruction after R8c and R9c closed: **"run the final timing rows,
record them as baseline then push. it does not matter if they are slower or
faster than a2, do not matter anymore, because a2 was experimental"**. This
answers OWNER-1: A2 is historical, with no gate and no arm. Nothing below is
judged against it.

**Twelve of the 282 timing rows were run, each once, and all twelve passed
their verifier.** They are the twelve rows that have a closed workload, a
closed oracle and a prepared input: cells C01 to C12, class B, arm L. The
other 270 rows stay `NOT_RUN`; the reasons are in the last section.

## Baseline

Class B (fresh mount on a warm daemon, after one untimed identical call on an
earlier mount and its terminal unmount), arm L, Disposable/WAL/OFF, one
construction producer, deployed `serial_low_water` 256. The command time on
the in-container clock is the number to compare a later sample with; the host
clock adds the launch outside the container.

| Cell | Command | Command ms, in-container clock | Command ms, host clock | Mount ms | Unmount ms | Requests | Owner jobs | Statements | Overlay logical bytes | Store logical bytes | Regime | Verifier / custody / cleanup |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| C01 | create 1000 files | 279.7 | 315.8 | 8.8 | 5.6 | 5002 | 5002 | 44004 | 430080 | 221312 | MIXED | PASS / KNOWN_STOP / Gone |
| C02 | create 1000, stat 1000 | 842.5 | 881.6 | 7.6 | 6.4 | 6002 | 6002 | 46004 | 430080 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C03 | create 1000, remove them | 549.5 | 594.0 | 9.6 | 3.8 | 9022 | 9038 | 102540 | 430080 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C04 | 100 directories, 1000 files | 716.7 | 750.5 | 8.9 | 5.4 | 5342 | 5342 | 42644 | 376832 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C05 | that tree, then `find` | 744.0 | 785.1 | 8.8 | 6.3 | 5890 | 6000 | 46637 | 376832 | 221312 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C06 | write 64 MiB | 99.5 | 136.3 | 8.2 | 5.1 | 518 | 518 | 9766 | 68468736 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C07 | write 64 MiB, copy it | 234.5 | 268.8 | 8.0 | 7.1 | 1551 | 1550 | 23133 | 136708096 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C08 | write 64 MiB, read it | 92.7 | 129.6 | 10.9 | 5.6 | 521 | 521 | 9783 | 68468736 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C09 | read a 64 MiB base file | 60.5 | 93.0 | 8.8 | 6.2 | 517 | 517 | 1055 | 229376 | 204800 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C10 | copy a 64 MiB base file | 147.4 | 187.5 | 7.5 | 8.4 | 1548 | 1547 | 14408 | 68468736 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C11 | overwrite a 64 MiB base file | 95.2 | 132.4 | 8.1 | 7.0 | 517 | 517 | 9760 | 68468736 | 204800 | SEPARATE | PASS / KNOWN_STOP / Gone |
| C12 | `git init`, 100 files, add, commit | 207.7 | 243.2 | 7.7 | 5.7 | 3436 | 3439 | 24146 | 335872 | 213072 | SEPARATE | PASS / KNOWN_STOP / Gone |

Raw nanoseconds, the complete-command and verifier walls, allocated bytes and
the daemon's resident high-water for every row are in
[baseline.json](checks/timing-baseline-20261010/baseline.json), copied from the
receipts by [make_baseline.py](checks/timing-baseline-20261010/tools/make_baseline.py).

How to read it:

- **One sample cannot carry a small margin.** R7 measured up to 23 % between
  two samples of identical work with the scheduler regime unchanged and 30 %
  when it changed. A later sample inside that band is not a change.
- **The counts are the stable part.** Requests, owner jobs and statements are
  exact and repeat; use them first when comparing a later source.
- **Regime** is where the kernel placed the caller and the daemon's receive
  thread, read from that thread's context switches. C01 ran MIXED and every
  other cell SEPARATE; a request round trip costs about half as much when the
  two share a CPU, and the product does not choose.
- **Storage** is the overlay's logical bytes after the command and the Store's
  logical bytes; the overlay's 256 MiB reservation is unchanged and is why its
  allocated bytes are larger.
- Every row is `DIAGNOSTIC` and `admission_eligible=false` by the harness's
  own contract. Class B was enforced: measured-phase object demands are zero
  in all twelve.

## Identity and custody

| Field | Value |
| --- | --- |
| Family, cases, order | R7 exploratory matrix, `C01:B:L` to `C12:B:L` in that order, one sample each |
| Source | `5969ecaf8e7799f51e299b632748419fbaabf63f`, tree `bbd165887312`, product tree `core/crates` `10bd82154437`, harness tree `d9f1434b9651` |
| Binaries | release; daemon SHA-256 `f44900cf…8c0e`, runtime `fdd962b7…0573`: the same two binaries the R8c examination used |
| Image | `sha256:378b799e…2cd6` |
| Build | locked, offline, repository ARM64 flags; Cargo reused unchanged artifacts, no clean-recompile claim (receipts 1341 to 1343) |
| Profiles | Global Store Disposable/WAL/OFF selected explicitly; overlay MEMORY/OFF/EXCLUSIVE; `LAYERFS_CONSTRUCTION_WORKERS=1`; Durable `NOT_RUN — disabled by owner until explicit reauthorization` |
| Setup | fresh volume and an independent byte copy of the sealed master Store for every cell (`cp --reflink=never`, compared with `cmp`); masters `layerfs-r7-empty-master-20261009-2ac7cc762` and `layerfs-r7-big-master-20261009-b8d76c0a3`; closed oracle packages from 2026-10-09, re-hashed at configuration |
| Command | `core/target/r7-iterate-cell.sh <cell> <first receipt number>`, which runs `python3 -B -m r7.runner --config <sealed config> --selection <cell>:B:L` under the checkout lock |
| Receipts | [1340 to 1391](checks/r7-optimization-20261009/) in the R7 harness's receipt directory, names ending `-5969ecaf8`; four per cell: volume, clone, configuration, sample |
| Verifier | the runner's independent comparison against the closed oracle, separate from the timers; 237 to 456 ms against a 9.5 s stop |
| Budgets | complete command 0.34 to 1.12 s against the 15 s stop |
| Interference | no other build, test or measurement ran in this checkout; other containers on the machine were running and were not stopped |

Containers and volumes this run created are removed; see the
[custody record](checks/timing-baseline-20261010/resource-custody.txt).

## Rows not run

| Rows | Count | Why |
| --- | ---: | --- |
| Arm N and arm P, every case | 148 canonical, 40 scenario | Not LayerFS. The passthrough arm is still refused by the provenance checker (OWNER-3), and neither arm has its fresh copy and residency proof prepared. A baseline of the product does not need them |
| C01 to C12, class C, arm L | 12 | No closed class-C oracle or same-mount warm-up recipe is prepared; C12's second identical call cannot repeat |
| E01 to E19, arm L | 43 | No closed recipe, deployed helper or expected output exists for these cases on the full fixture; E08 and E09 are owner-excluded |
| K01 to K05 and W01, W02, arm L | 7 | Need Commit checkpoint manifests and concurrent schedules that have not been authored |
| SC01 to SC13, arm L | 20 | The scenarios are named but have no executable workload |

Running any of these means authoring and sealing a workload and its oracle
first. That is preparation work, not a run, and it was not started.

## Not verified

- Anything about cold-cache cost: class A was not run.
- Commit time: these cells do not commit, and the K rows were not run.
- Repeatability: one sample per cell, by rule.
- Any resource bound: resident and storage figures are observations.
