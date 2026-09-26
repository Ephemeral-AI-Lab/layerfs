# #245 E/F checkpoint: live successor, known-outcome retry, and frozen comparison

> **Status: Dated planning checkpoint; not release evidence or a product contract.**
> Source of the initial proof below: `ff3dbcfe0eb0269e39981372b23bd84c0f9a1ccc`.
> The dated continuation at the end records product source `f9de81520`.

The frozen F comparison passes at both recorded product sources. E's stable
routes pass; a deterministic observation of B accepted *inside* the successor
build remains open, as the continuation records.
The #232 structural-shift and 56-shape gates, the load-bearing selection, and
the separate namespace ceilings remain open. The numbers below are one attempt
per registered case at each named source, not a latency or release claim.

## E: implementation and functional gates

`reconcile_commit` pins a live G+1 root, builds its successor outside the
metadata writer gate, and checks whether the root/revision moved before it
seals and installs the result. A moved frontier is rebuilt within the original
deadline; stale builds remain temporary pages on the attempt root. Mounted
write/read acquisitions wait for a short gate holder up to the caller's
deadline. A retained staged submission with a validated known C5 outcome and
reconcile-phase local failure resumes on the same selector without repeating
the consumed C5 command or finishing its fund twice. A known pending metadata
page is identity-checked and cleaned before retry, with its slot claim retained;
incomplete ownership remains blocked. The frozen root and old readers stay
pinned. [The snapshot architecture](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/02-overlay-snapshot.md)
and [CommitStaged architecture](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/17-commit-staged.md)
were updated with the source.

The final functional fixture is
`benchmark-results/fs-bench-pro/issue245-route-harness/fixture-e-05/result.json`
(closed 64 MiB Store, `shutil.copyfile` independent byte clone, fresh live
history producer, release binaries, one construction worker). The final route
summary is
`benchmark-results/fs-bench-pro/issue245-route-harness/e-final-routes-summary-01.json`.
All ten registered selections below are **PASS** at `ff3dbcfe0`, with the raw
stdout/stderr and `result.json` in each named directory. The route hard budget
was 60 s per case; no timeout or worker count was raised.

| Selection (`issue245-route-harness/` suffix) | Wall s | Result |
| --- | ---: | --- |
| `e-final-write-envelope-01` | 1.064 | PASS, 9 MiB replacement |
| `e-final-write-frontier-01` | 17.625 | PASS, 512-edit streaming Commit |
| `e-final-resize-envelope-01` | 1.227 | PASS |
| `e-final-fresh-stream-replay-01` | 1.034 | PASS |
| `e-final-stage-lowering-01` | 0.908 | PASS |
| `e-final-stage-semantics-01` | 0.993 | PASS |
| `e-final-commit-successor-01` | 0.922 | PASS |
| `e-final-commit-reconcile-retry-01` | 0.961 | PASS: exact C5 outcome retained, retry used no second C5 call, G2 bytes saved by a later Commit, clean close succeeded |
| `e-final-mounted-successor-01` | 1.145 | PASS: mounted shell A/B and racing append sequence, B0/B1/B2 native reads, live G3 bytes, no write `Busy` |
| `e-final-composite-successor-01` | 1.020 | PASS |

The mounted test releases B after the first C5 reply is held and races later
shell appends with the second reconciliation. It verifies accepted byte order
and old heads, but does not timestamp a B write against an internal individual
builder page operation; that narrower overlap is not independently observed.

All earlier non-passing diagnostic receipts remain under the same harness:
`e-reconcile-failure-01` stopped before the test because the pinned Docker
image was missing; `e-reconcile-failure-02` reached retry and exposed the
quarantined known pending page (`Capacity`); `e-reconcile-failure-03` proved
retry/install but the test incorrectly demanded clean close while G2 remained
dirty; `e-mounted-successor-01` failed a helper assertion that assumed no G3
write could advance revision after B2 installed. The changed source/test
identities were run in fresh directories; none of those failures was relabeled
or removed. The corresponding final gates above pass.

## F: frozen comparative target and candidate identities

[The target](../phase1-f-target/TARGET.md) was frozen before D/E. The control
at `fdfc41032` was not rerun. The second candidate preparation is
`benchmark-results/fs-bench-pro/issue245-shell-package-f-candidate-prepared-02/prepared.json`:
source `ff3dbcfe0`, product seal
`9c364e2f2a8469914b1caa430246a406f06417eff3d9911d41161530b9443397`,
harness seal `aae7081d526d85c2b225aa6eacefbae5e964c6453d3216afcd228dde779e4d1f`,
registry SHA-256
`1a7e1a3f7ea40ea14ed9f97865260c936df53601cfd5d0082c0db4041849cd3c`,
image `sha256:cd5f2f186ca88d9c3d090822479acee2af316e8374a4ae47e2507de1137f1f9c`.
The Store/History masters were closed and validated once outside the timed
attempts. Every case cloned its matching master with an independent
`shutil.copyfile` byte copy and exported `LAYERFS_CONSTRUCTION_WORKERS=1`.
Container FUSE backing and Edit-written cache state remain uncontrolled, so
**every latency cell is INELIGIBLE**. The complete-command wall comparison is
the separately frozen F-1 criterion, not a cold-cache speed claim.

One candidate attempt per registered case was collected at that identity in
`benchmark-results/fs-bench-pro/issue245-shell-package-f-candidate-02/`.
Each row has a `receipt.json`, raw driver/verifier output and `SHA256SUMS`.
All four have functional **PASS**, cleanup **PASS**, sealed verifier **PASS**
under the 9 s limit, and the frozen wall envelope **PASS**. The failure case
made zero Commit calls and retained the old head.

| Case | Complete wall s | Frozen maximum s | Verifier s | Comparative cell | Latency cell |
| --- | ---: | ---: | ---: | --- | --- |
| `mixed-refresh-v1` | 2.151699 | 4.391426 | 0.093513 | PASS | INELIGIBLE |
| `overwrite-4k-v1` | 0.931182 | 1.632815 | 0.159241 | PASS | INELIGIBLE |
| `repeated-one-byte-v1` | 1.145657 | 2.176802 | 0.093257 | PASS | INELIGIBLE |
| `failed-command-no-commit-v1` | 5.869259 | 11.764124 | 0.050949 | PASS | INELIGIBLE |

The preceding `candidate-01` attempt at `84ecbc753` also remains retained:
all four functional/cleanup/verifier and wall-envelope cells passed (2.308763,
0.923698, 1.111846, 5.913190 s respectively), while all four latency cells
were INELIGIBLE. Clippy then found a redundant return binding in production
source. Commit `ff3dbcfe0` removed that binding, changing the source seal;
the final arm was freshly prepared and attempted once per case. Neither arm
was selected by its measured wall.

## Verification and the one historical shift diagnostic

At the final product source, these checks passed:

- `cargo +1.85.1 test --release --manifest-path core/Cargo.toml --locked --all-targets`
  (raw log: `benchmark-results/fs-bench-pro/issue245-route-harness/core-test-final.log`);
- `cargo +1.85.1 clippy --release --manifest-path core/Cargo.toml --all-targets --locked -- -D warnings`;
- `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check`;
- `python3 core/tools/check_product_boundary.py` (296 production files);
- `python3 -m unittest discover -s core/tools -p 'test_*.py'` (9 tests);
- all 24 external `layerfs-workspace` Linux musl test suites, release profile,
  `aarch64-unknown-linux-musl`, `alpine:3.22`, one test thread per binary:
  `benchmark-results/fs-bench-pro/issue245-route-harness/musl-workspace-final-01/result.json`.

The required 2026-09-26 release-binary direction was applied to host, musl and
test binaries. The first warning-denying Clippy run at `84ecbc753` **FAIL**ed
on one redundant binding in `commit/reconcile.rs`; it was fixed in
`ff3dbcfe0`, and the covering Clippy command then passed. No root preflight or
CI claim was used.

One separately labeled historical #232 diagnostic used the registered
`prepend-head-4k-on-10mib-ops-1-exec-v2` command through the public SDK and
mounted `/bin/sh` route at `ff3dbcfe0`, with the pinned release image and
unchanged 5 s Exec progress deadline. It **FAIL**ed: Exec returned silent
`Unknown` after 5.027931 s, Commit was not called, unmount returned `Io`,
and the independent verifier failed because no Branch head was published.
The complete command took 11.642644 s. The runner's receipt records
`sample_count=0` because the command did not complete, even though this one
attempt ran. Receipt and raw output:
`benchmark-results/fs-bench-pro/issue245-historical-prepend-10mib-01/` and
`benchmark-results/fs-bench-pro/sdk-exec-fuse/edit_length_changing/prepend-head-4k-on-10mib-ops-1-exec-v2/`.
The original v2 failures remain unchanged;
the 5 s Exec Unknown is still an open defect, with no second arm or raised
deadline.

## Production size and open gates

Method for **each** commit: `git archive <first parent> | tar -x` for before,
`git checkout-index -a --prefix=<staged snapshot>/` for after, then the same
`python3 core/tools/production_loc.py --root <snapshot> --json` counter on both.
It excludes tests, documentation, tooling, comments and blank lines. Legacy
reference production remains 68,728 LOC throughout; the core and combined
totals below report migration coexistence, not an algorithmic LOC claim.

| Commit | Core before → after | Combined before → after | Signed delta |
| --- | ---: | ---: | ---: |
| `bc3aaae1c` | 56,506 → 56,617 | 125,234 → 125,345 | +111 |
| `f21df2448` | 56,617 → 56,642 | 125,345 → 125,370 | +25 |
| `fcf4acbd8` | 56,642 → 56,642 | 125,370 → 125,370 | 0 |
| `84ecbc753` | 56,642 → 56,642 | 125,370 → 125,370 | 0 |
| `ff3dbcfe0` | 56,642 → 56,641 | 125,370 → 125,369 | −1 |
| `d6bc594f2` | 56,641 → 56,641 | 125,369 → 125,369 | 0 |
| `f9de81520` | 56,641 → 56,650 | 125,369 → 125,378 | +9 |

Open work: #232's 56 ordinary-shell shapes still need a separately committed
exact-command/oracle amendment before a new campaign; the load-bearing cases
remain research; the 128-dirty/128-name/32 KiB namespace ceilings belong to a
separate namespace-lane package; and the 5 s Exec Unknown above persists.
These are not silently promoted by the E/F receipts.

Reproduction (each output must be a **fresh** worktree-local path; commands
below name the retained attempts for identification):

```sh
python3 core/benchmark/fs-bench-pro/shell_package.py prepare --output benchmark-results/fs-bench-pro/issue245-shell-package-f-candidate-prepared-02
python3 core/benchmark/fs-bench-pro/shell_package.py run --prepared benchmark-results/fs-bench-pro/issue245-shell-package-f-candidate-prepared-02/prepared.json --output benchmark-results/fs-bench-pro/issue245-shell-package-f-candidate-02
python3 core/crates/layerfs-workspace/tests/commit_staged_route.py --fixture benchmark-results/fs-bench-pro/issue245-route-harness/fixture-e-05/result.json --binaries benchmark-results/fs-bench-pro/issue245-route-harness/binaries --test-binary core/target/aarch64-unknown-linux-musl/release/deps/commit_staged-1abfb5a82f37531c --output benchmark-results/fs-bench-pro/issue245-route-harness/e-final-mounted-successor-01 --case mounted_successor
python3 core/benchmark/fs-bench-pro/runner.py run --case prepend-head-4k-on-10mib-ops-1-exec-v2 --out benchmark-results/fs-bench-pro/issue245-historical-prepend-10mib-01 --verification inline
```

## 2026-09-26 continuation: metadata-window collision and strict overlap gate

The first mounted observation above wrote B after capture while the C5 reply
was held. That proves generation separation but does **not** prove B was
accepted between the successor builder's input snapshot and its install. A
stricter, labelled functional diagnostic attempted to release an already-open
mounted writer after `CommitPhase::Reconcile` became observable. These were
new test schedules, **not** F performance samples. Every output remains under
`benchmark-results/fs-bench-pro/issue245-route-harness/` with its own
`result.json`, stdout and stderr:

| Diagnostic directory suffix | Retained result |
| --- | --- |
| `e-build-overlap-diagnostic-01` | FAIL before the test: pinned `rust:1.85.1-bookworm` image absent; restored for later attempts |
| `e-build-overlap-diagnostic-02` | FAIL: a combined phase/page poll never caught its narrow observation window |
| `e-build-overlap-diagnostic-03` | FAIL: B's mounted `printf` returned I/O error while reconciliation was active |
| `e-build-overlap-diagnostic-04` | FAIL: FUSE diagnostic mapped that write to `WorkspaceError::Busy` |
| `e-build-overlap-diagnostic-05` | FAIL: waiting at the metadata writer acquisitions alone did not remove B's `Busy` |
| `e-build-overlap-diagnostic-06` | FAIL: B still returned `Busy`; Workspace status showed phase `Reconcile`, revision 130, dirty frontier 65 and no installed revision |
| `e-build-overlap-diagnostic-07` | FAIL: count-driven diagnostic identified `LFS_WINDOW_BUSY first=3 end=4` before the FUSE `Busy` response |
| `e-build-overlap-diagnostic-08` | FAIL: B was accepted after the window fix; a test assertion incorrectly used shell-process exit as the Commit install boundary |
| `e-build-overlap-diagnostic-09` | FAIL: the stress fixture's 64 fresh G2 identities led to a known local reconcile `Io`; this is outside the existing-file E selection |
| `e-build-overlap-diagnostic-10` | FAIL: with 32 existing G2 files, B was accepted after the first install (report revision 35, pre-B revision 34) |
| `e-build-overlap-diagnostic-11` | FAIL: with 100 existing G2 files, B was again accepted after the first install (report revision 103, pre-B revision 102) |

The `-09` source inspection suggests the fresh-file stress reached the
`Current::counted` fresh-identity skip and dirty-count check; the receipt
establishes `Io`, not an independently instrumented exact error site. No
namespace bound was raised and no such result was promoted to an E pass.
Temporary diagnostic markers and the experimental test/harness edits were
removed before the product commit.

The `-07` receipt located an actual transient resource collision. Reconcile's
ungated build owned backing I/O window `3..4`; routine metadata maintenance,
reached by a mounted write before publication, requested that same window with
an immediate `Busy` result. Commit `f9de81520` makes maintenance wait for the
window up to its existing deadline **before** taking the writer gate. It adds
no window, worker, cache exception or longer deadline. The affected
[CommitStaged architecture](../../../../architecture/proposal/fuse-workspace-snapshot-overlay/17-commit-staged.md)
was updated in that commit. This fixed the observed window refusal; it did not
create a deterministic public barrier inside `build_ordered`.

At `f9de81520`, a newly sealed fixture
`benchmark-results/fs-bench-pro/issue245-route-harness/fixture-e-window-final-01/result.json`
and route summary
`benchmark-results/fs-bench-pro/issue245-route-harness/window-final-routes-summary-01.json`
record all ten selected routes **PASS** in fresh `window-final-*-01/`
directories, including reconcile retry and the stable three-generation
mounted observation. The 24 release-profile musl workspace suites also PASS
at `benchmark-results/fs-bench-pro/issue245-route-harness/musl-workspace-window-final-01/result.json`.
Full release-profile Core tests (`core-test-window-final.log`), warning-denying
Clippy, formatting, the product boundary check and nine tool unit tests PASS.
These passing routes do not relabel the strict `-10`/`-11` observations.

The new product seal required a new F candidate preparation and one new arm:
`benchmark-results/fs-bench-pro/issue245-shell-package-f-candidate-prepared-03/prepared.json`
and `benchmark-results/fs-bench-pro/issue245-shell-package-f-candidate-03/`.
Source `f9de81520`, product seal
`2279ea61b264fcfb38fbf33f3960565cdb358b4532c1fa0fecec54ea9406cf26`,
the unchanged harness seal and registry SHA above, image
`sha256:d6e7ddf91f0ac863b0f7a6909e4c815b5374423c4807e66894eef325f8ad8f5b`,
one construction worker, independent `shutil.copyfile` clones. The control
was not rerun. The preceding candidate arms remain in place; no wall was
selected by taking the best of them.

| Case | Complete wall s / frozen maximum s | Functional / cleanup / verifier | F comparative cell | Latency cell |
| --- | ---: | --- | --- | --- |
| `mixed-refresh-v1` | 2.226766 / 4.391426 | PASS / PASS / PASS, verifier 0.094569 s | PASS | INELIGIBLE |
| `overwrite-4k-v1` | 0.920098 / 1.632815 | PASS / PASS / PASS, verifier 0.161633 s | PASS | INELIGIBLE |
| `repeated-one-byte-v1` | 1.130367 / 2.176802 | PASS / PASS / PASS, verifier 0.098476 s | PASS | INELIGIBLE |
| `failed-command-no-commit-v1` | 5.968702 / 11.764124 | PASS / PASS / PASS, verifier 0.052225 s | PASS | INELIGIBLE |

**E/F lane disposition:** F's frozen comparative target is met at the latest
product seal. E's code, known-outcome retry and stable mounted observations
pass, and the diagnosed metadata-window `Busy` is repaired. The stricter
requirement to prove an accepted B write specifically *inside* the ungated
successor build is **OPEN**: the public phase signal preceded two writes that
landed after install, and no test-only product hook or time/budget relaxation
was introduced to force a pass. The five-second historical Exec `Unknown`,
#232's 56 shapes, load-bearing selection and namespace ceilings remain open
as stated above.
