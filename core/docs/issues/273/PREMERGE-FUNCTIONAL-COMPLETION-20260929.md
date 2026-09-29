# #273 → #264 functional pre-merge completion

> Functional review checkpoint, not release evidence or merge permission.
> Final product: `11a864fc133844cae7a4247b1f243d84d5763b10`.
> Final test/harness checkpoint: `a8528b9757de993eb8ee73b6d183ea8ee5badefa`.
> Owned branch: `codex/issue273-finalize-owned`.

The owner directed this continuation to finish implementation/algorithm
correctness, the 3×3 and later 8192 tests, and defer further memory work to
[#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283). That functional
selection has passed at the final product source. Every old and new numeric row
remains **INELIGIBLE**; the distinct frozen control remains **NOT_RUN**. No
symmetric VM/backend/device/host-cache contract or numeric admission is claimed.
No PR was merged and no issue was closed.

## Product and boundary

The [approved internal v2 specification](SAVEFILE-V2-PROSPECTIVE-20260929.md)
is implemented. Authenticated attachment positively checks SaveFile version 2
before LocalEdit admission. SaveFile v1 keeps its opcode/grammar and forward-only
validation. Internal v2 uses bounded Service-side C1 reads for backward/duplicated
Base replacement runs; it spools declared run metadata and actual local bytes,
without a daemon-recursive RPC or result-sized Base spool. External Service tests
prove canonical identity against a literal ordered v1 control, full old/new bytes,
permission denial, unknown version and malformed/trailing framing.

Ordinary reads retain one charged record of actual returned canonical bytes,
inode and immutable root. Ordinary writes compare every supplied byte before
reusing that Base origin; nonmatches take ordinary charged backing. Intervening
G2 extents retain their actual immutable Base coordinates. The public SDK Exec
still runs caller commands through mounted POSIX/FUSE. No Workspace range-edit
entrypoint, FUSE range ioctl, command selector, test hook or SDK editing façade
was introduced.

The existing 208-page completion reserve is precharged before dirty publication,
transferred to the captured submission and used by active C5 page allocation.
Physical page entries retain their funding owner. Verified unlink returns actual
charge to the same unfinished fund; published/pinned owners remain charged.
Unused credit in a zero/partial allocation stays reserved rather than escaping
to ordinary headroom. Explicit known-outcome local resume can repair the one
fully accounted, never-ready, unpinned, same-fund failed candidate through the
existing identity/block-checked release path. Unknown/incomplete/foreign or
other stopped custody is refused. The canonical command is never replayed.

Source descriptions changed in the same product commits:
[files](../../architecture/03-files.md),
[Service runtime](../../architecture/14-service-runtime.md),
[CommitStaged](../../architecture/proposal/fuse-workspace-snapshot-overlay/17-commit-staged.md)
and [active backing](../../architecture/proposal/fuse-workspace-snapshot-overlay/60-active-backing.md).
Limits, default construction worker, ARMv8 AEAD flags and dependencies are unchanged.
Workspace backing has no new fsync operation.

## Ordered correctness and custody proofs

**Lowering — PASS.** The registered native selection uses the original 64 MiB
prepared fixture and exactly `[100,110)→abc`, insertion `12345` at 200, then
deletion `[500,700)`. The independent saved-C1 oracle checks every one of
**67,108,662 final bytes**, with exactly **8 local replacement bytes**. Verified
`st_blocks*512` is **180,224 bytes**, equal to charged allocated backing, under
the unchanged **67,108,864-byte quota**. Stage retains its **10-second deadline**;
the complete command is **29.478575 seconds**, below the unchanged 60-second bound.
Fresh local-only receipt: `core/target/issue273-retry-stage-lowering-01/result.json`.

**Public SDK reordered/duplicated copy — PASS.** Real Exec runs ordinary `dd`
read/copy/write, copies immutable later Base bytes to earlier and later positions,
then obtains two known successful SDK Commits. Full independent 512 KiB byte
oracles check original, captured, canonical and later nonmatching-write views in
16 KiB windows. Old pinned bytes survive both Commits; checked releases and
clean shutdown complete. `issue273-retry-sdk-live-four-01` also proves the 32-lease,
forged/stale/cross-lease refusals, live deadline and actual stopping cases.

**Headroom — PASS.** Registered Stage occupies ordinary headroom under the original
**2 MiB quota**, preserves the unrelated owner and completes from its charged
reserve with exact physical accounting. CommitStaged headroom separately occupies
its original **4 MiB quota** with intervening G2 and completes reconciliation.
Receipts: `issue273-retry-stage-headroom-01` and
`issue273-retry-commit-headroom-01`.

**Known C1/local C5 and checked recovery — PASS.** A real native per-process
file-size fault follows the canonical acknowledgement. Total allocated+reserved
charge is conserved; ordinary WRITE returns Busy. After restoring the original
limit, explicit same-selector local resume succeeds, preserving saved G1, live G2
and old bytes and reaching clean close with **one canonical Commit**. Receipt:
`issue273-retry-commit-reconcile_failure-02`. The public SDK physical-C5 proof
separately retains its known canonical outcome and old held lease and reports
retained Busy custody; it makes no clean-backing/refund claim.

**SDK stopping — PASS, scoped.** An external fault moves the owned mount's parent
during real public Unmount. Status observes `stopping=true`; new pin, held read
and checked release return known Busy. The restored path is detached externally;
the terminal failed-detach owner and pin charge remain explicit, with
`CleanupFailed` reported through SDK deletion. This is not the deadline or lost
release proof and makes no pin-refund/clean-Workspace-close claim.

**Other final-product functional proofs — PASS:** 16 native Stage selections;
eight successful CommitStaged selections (including headroom, successor,
repeated, lost result, mounted successor, cycles and 104-inode frontier);
eight composite selections (clean/clean successor, successor, repeated,
physical reconcile failure, lost result, denied and 104-inode frontier).
They cover mounted WRITE/Commit continuity, retained old/new bytes, exact cleanup
and pin/refund custody, ordinary quota refusal, native SaveFile progress/loss,
metadata denial, physical C5 stopping and known Budget refusal.

All seven public SDK lease test functions ran live against immutable final-product
image `sha256:3eef46e3a9fa228c2f755968ea4b7f858db18cb95c7be134d7be2deaea961f54`.
Lost-release ciphertext is withheld exactly once and its original Unknown is
not retried. The fixed **16 MiB** response-Budget proof returns Capacity before
response bytes/partial entry registration; the 1-byte read still succeeds.
It does not prove an accepted 128 KiB SDK read.

## 3×3 and 8192

The unchanged prepared 10 MiB master is reused by independent writable byte copy.
Locked worktree-local release driver, verifier and daemon are sealed in
`core/target/issue273-retry-matrix-02/prepared-candidate/prepared.json`.
Each candidate cell ran once at the final product identity, with one construction
worker, exact ordinary FUSE WRITE count, full independent byte verification and
complete cleanup. The frozen arm verifier additionally passes every supported
schedule; its existing explicit NOT_APPLICABLE is retained elsewhere.

Every cell below is **functional PASS / numeric INELIGIBLE**. Seconds are raw
complete-command diagnostics, not a matched speed comparison.

| Pattern | 100 writes, 15 s limit | 512 writes, 15 s limit | 4097 writes, 25 s limit | Pack loads at 100 / 512 / 4097 |
| --- | ---: | ---: | ---: | --- |
| Append | 1.524716 s | 1.722133 s | 6.019977 s | 2 / 7 / 52 |
| Dispersed | 1.034881 s | 1.858093 s | 10.673892 s | 2 / 7 / 209 |
| Repeated | 1.021502 s | 1.532209 s | 6.516553 s | 1 / 1 / 1 |

Source counters are complete and loads equal distinct packs for these schedules;
the grouped source retains its existing 1024-reference / 32768-byte windows.
This finite matrix proves its declared schedules, not every permutation or a
general asymptotic bound. It does not establish matched resource admission.

The separate nonregistered **8192** generic-write selection passes the full byte
oracle and checked clean-close/refund under the original **8 MiB Budget**, in
**23.649186 seconds** complete command. It uses eight grouped source windows,
824 pack loads and 7368 hits; the Commit diagnostic is 0.595642 seconds.
The distinct **10,240** refusal case passes at the same default Budget: canonical
G1 is exact, C5 Capacity has no installed revision, later G2 WRITE proceeds and
retained custody stays explicit. It is not substituted for 8192.

The #248 separated-4097 canonical regression also passes both verifiers and
cleanup in 5.917701 seconds, with `LFS_C1_EDIT_LOAD nodes_read=0`. Its raw numeric
row remains INELIGIBLE.

## Failed attempts and scope

[The committed evidence index](PREMERGE-FUNCTIONAL-EVIDENCE-20260929.json) pins
local receipt hashes and source identities. `core/target/` is gitignored,
local-only evidence; this report does not publish those raw files or turn their
paths into GitHub evidence URLs.

Original lowering/headroom FAIL hashes remain exactly
`600737f01fca1a82ae92f49a009ee88b7bc8c7bc6a2deb69c797720f22587e67` and
`847c7e40964ddd6e41c2256c906b1117fc4579e2212542d0596047387aaca97c`.
The unmerged/reverted prototype patch remains
`f7f4c5089ebf1a2d061899da6b9e16e58e0a75781468c4ad005d62fe7b0c1ec7`.
It was studied, never applied verbatim. Its nonmonotone v1 failure, recursive-RPC
timeout, full-byte dirty diagnostic and separately observed 32 KiB pinned SDK
read Io remain historical. The 32 KiB observation is unresolved; 16 KiB oracles
are not evidence that larger SDK reads work. The precise cause of the old
unlogged token FAIL cannot be reconstructed.

New failed/partial attempts are also retained: unoptimized-Service lowering
timeout; missing executable-bit SDK image; duplicate Cargo argument; missing
ext4/test-root backing invocation; two incorrect stopping assertions; three
successor assertions/schedules inherited from the retired range-edit carrier;
the real complete-accounting local-resume gap; and one assertion that mistook
the shared payload-host status for the active PageStore stop. No failed receipt
was rewritten or promoted. The corrected generic successor tests retain final
saved/live byte oracles and old replies, exercise local WRITE while the remote
slot is occupied, and shift tails after its known result. The mixed-read
nonmatch frontier is explicitly **65537 uploaded local bytes**, one 64 KiB block
plus the final byte, rather than the retired direct-range carrier's one-byte
assertion. Gate 1's separate exact 8-byte lowering profile is unchanged.

Failed native runtimes are stopped and their owned volumes retained in
`core/target/issue273-final-failure-custody-01`; no clean-close or refund claim
is made for them. Passing routes removed their owned runtime/volume and kept
cleanup results. Other owners' runtimes, worktrees and PR heads were untouched.
The invalid zero-value peak-helper diagnostic is still INELIGIBLE; the repaired
same-FD host-capability result remains separate from deferred numeric admission.
#256 many-file scale and #270 pure-move C1 Commit remain deferred NOT_PROVED.
Driver-declared R6/npm/other broader routes remain outside these scoped proofs.

## Final checks and production LOC

Locked host workspace tests, all-target Clippy with `-D warnings`, all workspace
examples, fmt, product-boundary scanner and all boundary/tool self-tests pass.
The final host test command is 78.552437 seconds. Linux ext4 active backing is
32/32 and ownership is 6/6, with checked cleanup. Linux test/release builds keep
the root `.cargo/config.toml` ARMv8 AEAD profile; no RUSTFLAGS override is used.
All command components keep their original registered limits and the three-minute
outer bound. There is no CI or preflight claim.

Every commit used `python3 tools/production_loc.py --json --root <snapshot>`
on exact first-parent/staged-tree product Rust and runtime SQL, with the same
legacy inline-test exclusions. Counter SHA256:
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.

| Commit | Reference before → after | Core before → after | Combined before → after | Signed delta |
| --- | --- | --- | --- | ---: |
| `3318e5cd0` approved spec | 65417 → 65417 | 70670 → 70670 | 136087 → 136087 | +0 |
| `40f03ed7f` v2 / escrow | 65417 → 65417 | 70670 → 71327 | 136087 → 136744 | +657 |
| `30ebb4302` peak helper | 65417 → 65417 | 71327 → 71327 | 136744 → 136744 | +0 |
| `8df36e336` stopping / selection | 65417 → 65417 | 71327 → 71327 | 136744 → 136744 | +0 |
| `74646ff87` successor custody | 65417 → 65417 | 71327 → 71327 | 136744 → 136744 | +0 |
| `ee3803c1f` POSIX schedule | 65417 → 65417 | 71327 → 71327 | 136744 → 136744 | +0 |
| `6a937d6e2` frontier / composite | 65417 → 65417 | 71327 → 71327 | 136744 → 136744 | +0 |
| `11a864fc1` checked active retry | 65417 → 65417 | 71327 → 71416 | 136744 → 136833 | +89 |
| `a8528b975` admission proof | 65417 → 65417 | 71416 → 71416 | 136833 → 136833 | +0 |
| This report/index commit | 65417 → 65417 | 71416 → 71416 | 136833 → 136833 | +0 |

The functional result is ready for owner review of Phase 4.5 integration. Numeric
qualification stays deferred under the latest owner direction; historical and
new numeric rows stay INELIGIBLE and the frozen matched control stays NOT_RUN.
