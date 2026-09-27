# Issue 265 follow-up: new-page owner ledger finalization

> **2026-09-27 evidence entry.** Source `7221e177be9ebbe0baf4d51e8f7067a7dc66b37e`;
> control `ebfa8f242eaa37a224b6e30469744011503cd419`. All three new
> latency rows are `INELIGIBLE` because ordinary host/container cache state was
> uncontrolled. The count and functional conclusions below do not promote them.

The [prospective owner treatment](../../../../docs/roadmap/0.1/0.1.7/issue265-owner-finalize-treatment.md)
was committed at `12fbe4897` before this product change, runner change or new
sample. The only new product mechanism is in the Workspace backing: a new keyed
or extent page writes `edges=true` in its first authenticated owner record,
after recording `edge_progress=(page,0)`. The existing progress cursor records
each acknowledged edge prefix, so cleanup still releases only advanced edges
after a definite failure. Successful completion no longer reads and rewrites
the same 4 KiB ledger page to flip that flag. No physical format, page identity,
quota, custody edge, root publication, deadline or construction worker changed.

## Route and exact identities

The retained balanced controls and new rows use the same closed, full-oracle
10 MiB old head, the same static 100 one-byte-write shell binary, an independent
writable byte-copy Store/history clone for each attempt, and the same public
Mount → Exec → explicit Commit → independent old/new-head verifier →
Unmount/Delete route. The writer source SHA-256 is
`dc21c66ddb85be7c5d27c164b292cbf19a82f4e5a3352a8197050cfcae15d8f0`.
The source tree is `01a6d1b746ea1ca6c078358e8c2d2c549aa960db`, product
seal `3931a1c4ab9c59bc0f0dbdcaf83dc85f37dfc18fce030b4c087d052d9508a93a`,
harness seal `701a00b34424ad9c05e947da86fdcd7262e0525ce253e402f5927aded634bd46`,
Cargo.lock SHA-256 `09b880a18e1c221ba830908467f0985b0419ae0bf3c80c90ede7f7c5178272d6`,
and Docker image `sha256:80049a44e8ee3879b29ef12abfa6aad3fe5eb2944322da981dd3153a72e5484e`.
The locked Cargo release host SDK/verifier and aarch64 daemon were rebuilt in
this worktree; the sealed writer binary was reused. The prior closed master
was copied and verified by SHA-256 and the rebuilt verifier before sampling.
`clone_method` is `shutil.copyfile` independent writable byte copy, never a
cold-cache claim. One construction worker, the 15 s complete-command limit
and separate 9 s verifier limit were unchanged. No build overlapped a timed row.

Each pattern had exactly one attempt at this frozen source, in
append/dispersed/repeated order. The [append-only evidence](evidence/owner-finalize-v1/)
retains the three original receipts, raw driver/verifier stdout and stderr,
cases, source `SHA256SUMS`, preparation/build logs and manifests. Its public
prepared copy redacts the cursor key. The original prepared master, binary
archive and mutated SQLite clones remain under worktree-local ignored
`benchmark-results/fs-bench-pro/`; `EVIDENCE.json` records their hashes.

## Page-work result

The owner-finalization counter is a saturating product count read at the
existing 25/50/75/100 backing checkpoints. For **every checkpoint in every
case**, the difference from the retained balanced control is exactly one
ledger read **and** one ledger write per finalization. Extent splice totals,
cleanup edge totals and metadata page reads match their respective controls.
These are 4 KiB ledger API operations, not measured physical device traffic.

| Case | Finalizations at 25 / 50 / 75 / 100 writes | Control → new ledger reads at 100 | Control → new ledger writes at 100 | Saved read/write operations |
| --- | ---: | ---: | ---: | ---: |
| Append | 50 / 100 / 150 / 200 | 2,647 → 2,447 | 1,470 → 1,270 | 200 / 200 |
| Dispersed | 50 / 100 / 165 / 240 | 3,156 → 2,916 | 1,756 → 1,516 | 240 / 240 |
| Repeated | 50 / 100 / 150 / 200 | 3,059 → 2,859 | 1,489 → 1,289 | 200 / 200 |

The retained per-checkpoint read/write differences are 50/50, 100/100,
150/150 and 200/200 for append and repeated; dispersed is 50/50, 100/100,
165/165 and 240/240. At write 100, the avoided logical ledger transfer is
1,638,400 bytes for append/repeated and 1,966,080 bytes for dispersed
(read plus write); there is no device-byte claim. Balanced dispersed still
has seven live metadata pages and 101 leaf/39 branch writes, with the same
child/custody edge counts as its control. The final C1 edit and FileInput
counts also remain the prior recorded result; no C1 or C2 algorithm changed.

## Function, raw walls and gates

All three drivers returned `COMPLETE`, observed 100 actual FUSE WRITE callbacks,
four upstream Service calls and one explicit Commit. Each independent verifier
passed full old/new bytes, exact parent, path inventory and final changed runs
1/100/1. Accounting remained complete with zero failed payloads; SDK
Unmount/Delete cleanup passed. Every complete command was below 15 s and
every independent verifier below 9 s. One native ext4 test, run through the
real keyed and Local extent page store in a Docker volume, passed in 0.01 s:
it checked first-write owner flags, old-root bytes across a successor,
shared G1/G2 custody, finalization count and clean metadata/payload reclaim.

| Case | Control → new raw Exec ms | Control → new raw Commit ms | Control → new complete-command ms | New verifier ms |
| --- | ---: | ---: | ---: | ---: |
| Append | 396.796 → 350.492 | 34.136 → 29.335 | 2,129.294 → 1,975.722 | 98.158 |
| Dispersed | 488.205 → 415.901 | 27.056 → 30.287 | 1,423.900 → 1,317.057 | 97.497 |
| Repeated | 445.027 → 430.602 | 17.743 → 20.020 | 1,318.510 → 1,317.618 | 98.883 |

The new raw Exec observations are lower, but cache residency was not declared
or enforced equally between arms. Their comparison is **not** a speed PASS or
regression screen. No case was rerun, omitted, relabelled or selected by time.

Warning-denying Clippy passed for Workspace, Content and Server; the Core
product boundary guard scanned 318 production files and its nine self-tests
passed; the runner self-check, Python compile, changed-file rustfmt and
`git diff --check` passed. `cargo +1.85.1 fmt --manifest-path core/Cargo.toml
--all -- --check` still fails only on the pre-existing unrelated
`core/crates/layerfs-workspace/src/runtime/state.rs:267` attribute wrapping.
The full Core workspace test suite was not run under this issue's under-30 s
focused-test constraint. The native 64 MiB failure-injection custody fixture
remains `NOT_RUN` under that bound; failure-path G1/G2 proof stays open. The
separate mounted 4,097-write #248 gate is `NOT_RUN` pending #266's FUSE
refusal and #249's product timer. C2 physical Store I/O counters remain
unavailable within this task's assigned source boundary. These omissions do
not change the three row statuses.

## Reproduction and LOC

The original one-shot commands and exact run inputs are in each receipt.
Preparation used `write_patterns.py prepare-reuse --prior-prepared
benchmark-results/fs-bench-pro/issue265-balanced-prepared-ebfa8f2-01/prepared.json
--output benchmark-results/fs-bench-pro/issue265-owner-prepared-7221e17-01`;
the `run` action then used that prepared receipt and a fresh append,
dispersed or repeated output directory. Inspect those receipts and the
evidence bundle for replay of counts; repeating the same frozen arm would
violate the one-attempt contract.

The focused native proof was built with
`cargo +1.85.1 zigbuild --release --manifest-path core/Cargo.toml --locked
--offline --target aarch64-unknown-linux-musl -p layerfs-workspace --test
owner_finalization`. The test command was:

```sh
docker run --rm \
  --mount type=volume,source=layerfs-issue265-owner-finalize-test,target=/stage \
  --mount type=bind,source="$PWD/core/target/aarch64-unknown-linux-musl/release/deps/owner_finalization-462508c6c2ab671f",target=/owner-test,readonly \
  -e LAYERFS_OWNER_TEST_ROOT=/stage --entrypoint /owner-test \
  layerfs-shell-package-v1:issue243 --nocapture
```

Every commit compares its first parent with its exact staged Git tree using
unchanged `tools/production_loc.py --root <snapshot> --json` over `git archive`
snapshots. The scope is nonblank, noncomment first-party production Rust and
shipped runtime SQL; tests, runner, docs, generated files and legacy inline
tests are excluded. The reference scope stayed 65,417 and adapter scope 0.

| Commit | Combined production LOC | Core production LOC |
| --- | ---: | ---: |
| `12fbe4897` prospective specification | 124,233 → 124,233 (0) | 58,816 → 58,816 |
| `7221e177b` product, runner and native test | 124,233 → 124,254 (+21) | 58,816 → 58,837 |

This evidence/report-only follow-up has a zero production LOC delta; no
reference implementation was retired. The exact source commit retained in
every new performance receipt is `7221e177b`, not this documentation commit.
