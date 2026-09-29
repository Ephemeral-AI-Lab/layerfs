# #273 follow-up: real SDK lease deadline fault and bounded native fix

> **Status:** functional-only continuation of
> [the `96d196dec` checkpoint](PREMERGE-FOLLOWUP-20260929.md), not a new
> numeric admission, historical-receipt rewrite, owner disposition or PR-head
> update. Final functional source `31b49905bdfa71703c7580bc696c20446e25e832`;
> production fix `f00644479a9b7dfe0b74e02438eed6d60e30dc0a` (first parent
> `8fc801a7cb9c2de296e3b10d006fcd5c618e037c`).

## Fault, diagnosed failure, fix

The SDK public live test added in `8fc801a7` obtains a real held G1 view,
reads exact G1 bytes, pauses **only its own** Docker container selected by the
owner/sandbox-name labels, calls public `WorkspaceApi::view_read` at its normal
5 s deadline, always resumes the container, then checks original custody,
old/new bytes across a later Commit, checked release and unmount. No test-only
product hook, sleep, altered deadline, alternate Store, response replay or
increased Budget was introduced. This is an external control-session-deadline
falsifier: the paused sandbox can prevent the session's checked Hello, so the
SDK attempt need not have reached the view-read operation. It must **not** be
called proof of a daemon-side view-read cancellation.

The first append-only live attempt at `8fc801a7`, using the preceding daemon
image, **FAILED**: the SDK returned `Failure(Io)` instead of `Deadline` after
22.18 s complete test command. The panic skipped checked lease release;
cleanup captured `sandbox shutdown retained: Busy`. This failed attempt is
local-only `core/target/issue273-next-sdk-deadline-01/result.json` SHA256
`0db50fac810cdf3e439b9984aedfa16394df51c6a297827e9dff91619144067b`.
It was **not** converted to PASS or treated as a clean close.

Root cause: a socket read's `TimedOut` was collapsed to `Io` by the bridge's
native record reader. The SDK session, having lost its retained Hello, could
try a fresh handshake even after the original call deadline. `f00644479`
conservatively maps **actual socket read timeouts** to `Deadline` and propagates
that error on read-only calls; a missing mutation result still reports
**Unknown**. After a checked Hello consumes the same SDK call deadline, the
session is discarded and its original error returned without a further
handshake. Before-deadline I/O errors remain `Io`. Architecture source and
this boundary were updated in `14-service-runtime.md` with the production
commit. No quota, worker count, deadline, format, durability promise or
third-party code changed.

At `f00644479` the live test **PASSed** using the rebuilt product daemon image
`sha256:65122998e92b125d0ec0b10f9826be5c74f064800f57199dd02aa4e290cdc22d`
(daemon binary SHA256 `2f0b4305488a862687b09eb9664e773cdd79fa143899a43ab7767209dcf5fe57`).
It emitted `VIEW_LEASE_DEADLINE read=Deadline g1=exact g2=exact held=1
release=Completed unmount=Completed`. At final `31b49905` the two SDK live
selections (deadline and the existing pinned G1/G2 / forged-token / 32-lease
proof) ran serially **2/2 PASS** on the same image, owned sandbox deleted;
`core/target/issue273-next-sdk-view-31b-01/result.json` SHA256
`2e8cc29549467e7a124e2d2e135048ff16a31daebf0e7e9d2a29b586018e931b`.
Those paths and the image build `core/target/issue273-next-sdk-deadline-image/result.json`
are **gitignored local custody only**, not published GitHub raw evidence.

A separate final-source native authenticated integration test holds the peer's
terminal response **after it accepts BEGIN**, under a bounded test deadline:
read-only missing reply returns `Deadline` without a retry; an uncertain
`WorkspaceReleaseView` reply returns `Unknown` (`unknown=true`), not a
fabricated successful retirement or known refusal. All six
`native_connections` tests PASS. This proves transport classification; it
**does not** hold a *live SDK lease* across an uncertain release, so that SDK
falsifier remains open. The older unprinted token-refusal FAIL at `c0ab3e5c8`
still has **unknown cause**; a later clean-source PASS does not diagnose it.

## Frozen gate and admission after the product change

Because the transport/SDK product inputs changed, the original registered
`issue248-separated-4097-v1` public selection was exercised **once at the new
clean source** `31b49905`: exact 4,097 WRITE callbacks, C1 emitted
`LFS_C1_EDIT_LOAD v=1 nodes_read=0 stored_nodes_read=0 draft_nodes_read=0`,
two independent verifiers PASS, clean unmount/delete, 6,491,784,042 ns complete
wall within the unchanged 25 s gate. Receipt is local-only
`core/target/issue273-next-248-gate-31b-01/receipt.json` SHA256
`aad10a9d039efc15da0649d959301823ab19eab4306efebf3fca8edd9c37f1c5`.
Its **functional status PASS, numeric row INELIGIBLE, resource PARTIAL**, and
control NOT_RUN are unchanged. The earlier `96d196dec` attempt stays at its
own source and is not re-labelled or pooled.

The final-source locked package test commands for bridge, sandbox, SDK,
Workspace, Server and daemon each PASS on macOS; `cargo +1.85.1 build
--manifest-path core/Cargo.toml --locked --workspace --examples`, warning-denying
workspace/all-targets Clippy, fmt `--check`, product boundary (359 files), its
nine self-tests, and diff whitespace check also PASS. The Linux FUSE
namespace/stage inventories, all 12 Core packages, all ignored cases and the
full locked-release suite were **NOT_RUN on `31b49905`**; earlier-source
functional results retain their original scope. The SDK live selection
requires an immutable image ID explicitly supplied through
`LAYERFS_TEST_IMAGE`; the macOS package command alone does not run it.

**Remaining:** SDK exhausted response Budget, held lease across known C1/local
C5 failure and uncertain *checked* SDK release are NOT_RUN. The SDK deadline
call is now proved at the control-session boundary, but the independent
stopping/cancellation route is NOT_RUN. The original token FAIL's precise
cause remains unresolved. Lowering/headroom remain retained FAIL at their
unchanged quotas; explicit owner dispositions requested in
[#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5881598410)
are still pending. There is no matched VM/backend/device/host cache profile,
verifiably phase-local peak or owner numeric ruling on this host: numeric
**INELIGIBLE**, matched control **NOT_RUN**, #256/#270 NOT_PROVED. #283 defers
only the Docker Desktop peak-reset observation. **NO MERGE / NO RELEASE
ADMISSION**; no PR or historical receipt was updated.

Production LOC by first-parent commit, reproducibly counted with
`python3 tools/production_loc.py --json --root <parent/staged scope archive>`
over `crates/` and `core/crates/` (excluding tests/docs/tools):

| Commit | Reference | Core | Combined | Signed delta |
| --- | ---: | ---: | ---: | ---: |
| `8fc801a7` (test only) | 65,417 → 65,417 | 70,653 → 70,653 | 136,070 → 136,070 | +0 |
| `f00644479` (product/architecture) | 65,417 → 65,417 | 70,653 → 70,670 | 136,070 → 136,087 | +17 |
| `31b49905` (native regression test) | 65,417 → 65,417 | 70,670 → 70,670 | 136,087 → 136,087 | +0 |
| This docs-only report | 65,417 → 65,417 | 70,670 → 70,670 | 136,087 → 136,087 | +0 |
