# #232 Phase 4 fast-lane sample and Phase 5 collection state

> **Status:** one registered case has a complete performance sample and a
> separate independent proof; every other registered case is `NOT_RUN` and the
> twenty structural cases stay `NOT_RUN`. No row is a release-admission PASS.

Source identity: commit `2ed00cc8774ffd8d9f5cbaa403e830f37aba14c6`, tree
`8677dc35b65060ecb8232aef17bf342799ee9baa`, dirty `False`.
Product seal `dec1242a6a381d3e…`, harness seal
`fdc9b05dac24de68…`, Cargo lock
`d6fb4800e048346f…`, registry
`05fb205d391d551a17bde86a00c969310b6e7406` contract pin; registry SHA-256
`e4b4d2fc67cb1630dc8f15283db3085663466071bb373c829a6026ffddb66aab`.
Image `sha256:cb51ce524b113a6aed95953df7c091dde77e29e87477271964f65aff53f15766` (release, `aarch64-unknown-linux-musl`,
base `alpine@sha256:5291449c3df73caf6ed85e649dec1b9e818b39a5d8c871e97afc13e9cd5e8fa8`, edit tool
`a755089896282a70…`, daemon `e10b3ab6303b6949…`).

## 1. Selected case

`overwrite-middle-4k-on-1mib-ops-1-exec-v1` (workspace_exec_edit_length_preserving), fixture
1,048,576 B, `pwrite` 4096 B at
522,240, declared editor `single-positional-write`,
historical G2 engineering target 5.63 ms.

## 2. Measured outcome (one sample, no resample)

| quantity | value |
|---|---:|
| caller `edit_commit_ns` (LFT1 root) | 41,451,208 ns |
| `edit` child | 19,018,375 ns |
| `commit` child | 22,428,250 ns |
| historical G2 target | 5.63 ms |
| complete command wall (fresh Sandbox create + delete included) | 932,240,000 ns (≤ 15 s budget) |
| preparation wall (recurring, byte copy + fork + Sandbox + Mount) | 385,256,625 ns (≤ 5 s goal) |
| cleanup wall (status + unmount + sandbox delete) | 421,268,667 ns |
| verifier wall (separate command) | 41,308,000 ns (≤ 15 s cap, < 10 s goal) |
| samples for this case/arm | 1 |

Raw LFT1: `17` records, SHA-256
`1d9f0ead563f041315a817c6de6df4c0f186039d3914c13eb877346f7f6bb428`; caller root
`sdk.edit_commit.fuse` with `success=true`, `resource_status=sampled`, and the
`edit` and `commit` children both present. CPU is the crate's shared-process
window and memory is its sampled process RSS; the daemon is a separate process
and the shell edit child is not included in it.

Product route evidence after the timer (public `WorkspaceApi::status`):
`lookup=1,getattr=5,read=0,write=1,readdir=0,open=1,setattr=0,rename=0,other=0`, upstream host Service calls
4. Unmount and `SandboxApi::delete` both
succeeded with no orphan container or volume.

## 3. Terminal status

`INELIGIBLE` — fuse-backing-domain-served-by-resident-pages-not-invalidatable. The raw
number is retained as a declared-warm diagnostic: the Linux FUSE/backing
domain is created fresh per case, cannot be invalidated without a
benchmark-only eviction between Edit and Commit, and is therefore not a cold
claim. The macOS Store domain was invalidated and checked non-faultingly and
is `PASS` with
0 resident pages of
171 checked. Against the historical
absolute target the same number would be `TARGET_MISS`
(41,451,208 ns > 5,630,000 ns); it is
recorded as Ineligible rather than promoted, and the arm is not rerun.

## 4. Independent proof (separate command, fresh reopen)

| check | observed | declared | match |
|---|---|---|---|
| final size | 1,048,576 | 1,048,576 | yes |
| content digest | `f998113b31421539…` | `f998113b31421539…` | yes |
| coverage | `full-file`, `full_file_bytes_verified=true` | declared full-digest bound 100 MiB | — |
| canonical root | `83a5dadc083bfef9…` | differs from fixture root | yes |
| extent count | 56 | 54 → changed | yes |
| published Branch head | `12e418b8644fe9b8…` | receipt head | yes |
| retained pristine genesis root | `8fafdf06fac9dbdf…` / 54 extents | pinned fixture root / 54 | yes |

Verification binding: performance run `/Users/yifanxu/.codex/worktrees/bb50/layerfs/benchmark-results/fs-bench-pro/exec-fuse-edit-phase4-04`, image
`sha256:cb51ce524b113a6aed95953df7c091dde77e29e87477271964f65aff53f15766`, fresh reopen `true`, coverage `full-file`.

## 5. Retained attempts and omissions

| attempt | retained path | outcome |
|---|---|---|
| 01 | `benchmark-results/fs-bench-pro/exec-fuse-edit-phase4-01` | build-selection defect, no sample |
| 02 | `.../exec-fuse-edit-phase4-02` | harness-defect row: LFT1 was forwarded on stderr and the harness only scanned stdout; 58,527,917 ns retained, no LFT1 |
| 03 | `.../exec-fuse-edit-phase4-03` | refused: the prepared master was incompatible with the changed harness seal (fail-closed) |
| 04 | `.../exec-fuse-edit-phase4-04` | the gate sample above, plus a parser re-derivation from its own raw LFT1 and the separate proof |

Registered cases with a sample: 1 of 36.
Registered cases `NOT_RUN`: 35.
Structural cases `NOT_RUN`: 20 (the frozen
`structural-shift-algorithm-unfrozen` reason). No registered case was dropped,
shrunk, retried for a better number, or omitted from this report; the
remaining 35 registered rows were not collected because the
session's remaining wall time did not cover 35 further case preparations and
samples (each needs its own writable byte copy, Sandbox lifecycle and, for the
500 MiB sizes, a 500 MiB copy), so they are recorded `NOT_RUN` with that exact
reason rather than sampled partially or reported as a subset.

Verification of the remaining registered rows is likewise `NOT_RUN`: it is a
separate identity-matched command per retained performance receipt, and only
one such receipt exists.

## 6. Harness identity of the proof

The performance sample and its raw LFT1 were taken under harness seal
`fdc9b05dac24de68…`. The verifier binary is the immutable
archived one from that same build receipt
(`binary-archive/<sha256>/verify_edit`), and the product source, Cargo lock,
registry and image identities are unchanged. The verification command itself was
re-run once after a harness-only defect in the verifier's case-file binding (the
published Branch identity was not passed to it); that change touched no product
source, no timer, no cache contract and no oracle definition, and the performance
receipt and its raw telemetry were not rewritten. A future revision of the
measured route or the oracle needs its own scenario version and fresh receipts.

## 7. What is not complete

* 35 of the 36 registered cases have no performance sample: `NOT_RUN`, exact
  reason "session wall time did not cover the remaining preparations and samples".
* The 20 structural cases: `NOT_RUN` under the frozen
  `structural-shift-algorithm-unfrozen` reason.
* No row is `GOAL_MET`. The one sampled row is `INELIGIBLE` under the frozen
  cache contract, and its raw number exceeds its historical target.
* Issue #232 is therefore **not** complete and must not be closed.
