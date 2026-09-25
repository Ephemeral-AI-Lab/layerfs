# #232 repeated-128 liveness follow-up

**Disposition: FAIL, cause localized, no product change.** The historical
Phase 1C attempt remains unchanged. One separately labelled, prospectively
declared diagnostic ran at source `449f30bb5fb17003f74a3bd725090f49163fa0f1`
with product seal `0945f62bbd3a8532b2fe08cac8b7962d9774c9d66f23e1ab61f7b315e8402aa9`,
harness seal `3c51e92850f6a66033bec48838e130d29eaf8db6cb1d18a856c95328882f840a`,
and image `sha256:259986fc83211d81968db634c429f9485a556335b95f6f45337a5fe5e1c42037`.
This diagnostic exercises the opt-in cooperating `splice-batch` ioctl tool,
not arbitrary shell-command editing through Exec.
The [contract](REPEATED128-LIVENESS-CONTRACT.md) preceded the run; the
[pre-run manifest](REPEATED128-PRE_RUN.json) pins the host binaries, workload,
payload and reused prepared master. Its product and harness seals still matched
at execution; the intervening pre-run-manifest commit changed documentation
only. The Store/history were independent writable byte copies. The [raw
receipt, logs and hashes](liveness-diagnostic-01/) are append-only evidence.
Before the run, a Dockerfile using a bare `sha256:` image in `FROM` failed to
resolve; no measurement started. An older local image was also assembled but
discarded before sampling when its product seal differed from this branch.
The sampled image was rebuilt from this branch's daemon, and its verified base
and final image digests are in the pre-run manifest.

The caller's LFT1 Exec failed at **5,040.202 ms** with `Unknown`, while daemon
LFT1 `WorkspaceExec` completed successfully at **7,369.822 ms**. No Exec
acknowledgement reached the SDK; Commit was never called. The command's
diagnostic summary was written after all **128** checked edits, and its
7,355.579 ms covered the `run_with` calls:

| Child activity | Cumulative diagnostic elapsed | Share of checked batch |
| --- | ---: | ---: |
| 128 EDIT ioctls | 6,107.078 ms | 83.03% |
| Bounded POSIX reads | 1,219.238 ms | 16.58% |
| 256 STATE ioctls | 14.934 ms | 0.20% |
| `fstat` | 13.319 ms | 0.18% |
| Other checked-call work | 1.010 ms | 0.01% |

The local syscall intervals are **diagnostic attribution**, not separate LFT1
operation records or cold latency results. The LFT1 daemon root and caller
root are the timed end-to-end boundaries. The daemon also recorded 512
`ReadFile` operations totalling 935.905 ms inside its process; these overlap
the POSIX read intervals and must not be added to them. The bridge's fixed
5,000 ms control-response no-progress bound, while SDK Exec allows 30,000 ms,
therefore explains the `Unknown`: the 128 checked mounted operations kept the
daemon busy without producing a control response before the bridge bound.
No deadline or worker count was changed.

The daemon logged 128 `LFS_PIECE_COUNT` and 128 `LFS_PIECE_PAGES` lines:
**49,664 splice-loop visits**, **487 piece-index pages written**, and 257
final logged pieces, exactly as in the retained Phase 1C failure. This
diagnostic localizes most wall time to EDIT ioctls, but those calls also include
payload ownership, piece loading, splice, page rebuilding, publication and FUSE
reply. It does not time the splice loop within the ioctl. The counts cannot
justify a tree rewrite or identify a single narrow product algorithm fix.
The failure is the cumulative synchronous EDIT and readback route crossing
the native control no-progress boundary, with EDIT the largest measured share.

The complete command took **12.821 s**, under its 15 s wall cap but **FAIL**
because the driver exited 1. Independent verification is **NOT_RUN**, since
there was no Commit/head to verify. Unmount returned `Io`; public Sandbox
Delete passed. The pre-sample macOS Store residency check found zero resident
pages across 171 checked pages. Linux FUSE backing remains **INELIGIBLE** for
cold latency because it lacks invalidation plus full residency checks. Neither
the historical failure nor this diagnostic is an admission PASS or a new
Exec→ioctl→Commit proof.

No product source or bridge deadline was changed. The temporary workload
timing hook is preserved at commit `aaf4dde930e44cfc7994049ecbc3d8f7c48e697f`
for exact reproduction and removed from the branch after this run. A future
fix needs an independently scoped mechanism that either reduces the measured
EDIT/observation work within the unchanged response bound or carries genuine
operation progress through the control protocol. Any changed-source attempt
requires a new sealed identity and fresh public SDK Exec→ioctl→Commit proof
with independent verifier and cleanup. The completed 56-case single-edit
campaign and the separate cold-cache and capped-500 MiB raw questions are
untouched.
