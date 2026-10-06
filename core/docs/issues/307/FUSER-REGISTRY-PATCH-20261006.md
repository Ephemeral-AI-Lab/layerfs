# crates.io fuser 0.18.0 authorized timestamp correction

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

The owner requested "use fuser 0.18.0 from crates io and apply patch". This
supersedes the earlier no-patch and corrected-published-release-only prerequisite
for this one correction. The [prior decision and official Git investigation](FUSER-OFFICIAL-CANDIDATE-20261006.md)
remain historical records. No Git dependency is adopted. S0 and S8 remain unchecked;
the patch fixes the library bugs but does not complete the full native milestone.

## Reproducible source and selection

The [patch and provenance record](../../../patches/fuser-0.18.0/README.md) pin the
original official crates.io archive at version0.18.0 and SHA-256
`b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd`.
The checked-in copy preserves all85 published files, with only `src/time.rs`
changed. Its correction and regression tests match the upstream file at
[e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7](https://github.com/cberner/fuser/commit/e48279fab0ddbe4a6e22aefad3cbfdf796d0b0b7).
The unchanged published manifest/version/license/public API and other84 files
are verified by file hashes. Both owning roots use the exact `[patch.crates-io]`
path/version entry. Cargo records the local patched source without a registry
checksum; the separate archive/diff/file pins authenticate the changed dependency.

The correction uses unsigned magnitude for signed minimum and the timespec rule
`seconds + nanoseconds/1e9` for negative fractional values. The original release
instead subtracted the fractional nanoseconds and negated `i64::MIN`, producing
wrong values and a debug overflow panic before the LayerFS callback. These are
real library bugs, preserved in [the original native failures](FUSE-TIME-BLOCKER-20261005.md).

A [fresh reconstruction](checks/fuser-registry-patch/reconstruction-receipt.json)
extracts the pinned official archive, applies the exact diff once and verifies
all85 result files against the checked-in package. Shared Cargo registry source
is unchanged. Cache deletion and a clean checkout retain this fix. The focused
integrity guard rejects other patches, altered inventory/diff/source, redirects,
floating/Git/custom registry sources and owning locks that bypass the patch.
The core active graph does not yet include the replacement FUSE crate; its patch
is currently unused. The independent native harness actually builds the patched
package. No empty FUSE scaffolds or root-reference changes are introduced.

## Exact qualification

[Source identities](checks/fuser-registry-patch/source-identities.json) pin the
base commit, archive/diff/time.rs, harness, manifest/lock and ARM64 config hashes.
Linux aarch64 kernel6.12.76-linuxkit, Rust1.85.1, original native-proof image
`sha256:e51d0265072d2d9d5d320f6a44dde6b9ef13653b035098febd68cce8fa7c0bc4`.
Builds use `--locked`, test compilation uses `--no-run`, overflow checks remain on,
and `LAYERFS_CONSTRUCTION_WORKERS=1` is set. No graph package changes except the
fuser0.18.0 source/checksum removal in the actual harness; every core package pin
and every other harness package pin remains identical to the first parent.

The [frozen case plan](checks/fuser-registry-patch/frozen-proof-plan.json) selects
one attempt per distinct case, with8s explicit complete-command ceilings. Fresh
synthetic mounts/sockets isolate correctness. Cargo caches are reused and declared;
these results have no cold speed, RSS or sustained-rate eligibility. The four
unrelated containers are preserved; their presence is declared interference,
without a comparative timing claim. [Raw receipts](checks/fuser-registry-patch/proof-receipts.json)
contain commands, walls, binary hashes/sizes, limits and all outcomes.

| Exact case | Complete wall ns / bound | Actual verdict |
| --- | ---: | --- |
| fuser-time-unit | 279783750 / 8s | PASS |
| native-negative | 234247333 / 8s | PASS |
| native-minimum-whole | 228672333 / 8s | PASS |
| native-minimum-fractional | 228891250 / 8s | FAILED |
| public-session-five-cases | 343899375 / 8s | PASS |
| native-platform-smoke | 338944500 / 8s | PASS |

Four fuser time unit tests pass. The parser/reply fixture verifies five exact
inputs: negative fraction, signed minimum whole/fraction, minimum+1 fraction and
maximum fraction. It uses the public Session boundary, explicit DESTROY and join,
bounded sockets and client panic cleanup. It is an external fixture, not a
product raw-wire workaround or a replacement for mounted native qualification.
The mounted negative and whole-minimum cases now return exact values without
panic. Native mount/read/unmount/join also passes. Every native attempt, including
the failed minimum-fractional case, reports normal unmount/join; none times out.

The mounted `(i64::MIN,200000000)` case remains **FAILED**: the callback and returned
value have nanos0. [Diagnosis](checks/fuser-registry-patch/native-minimum-fractional-diagnosis.json)
retains this outcome. The pinned Linux
[timestamp_truncate](https://github.com/gregkh/linux/blob/v6.12.76/fs/inode.c#L2455)
zeros nanoseconds at the filesystem's minimum/maximum, and
[notify_change](https://github.com/gregkh/linux/blob/v6.12.76/fs/attr.c#L358)
applies it before setattr. Exact200M nanos survive when actually presented to
fuser's public Session parser/reply boundary. This evidence separates the corrected
library conversion from Linux VFS canonicalization. No full native timestamp PASS,
smaller contract, kernel patch or relabeled old failure is claimed.

## Scoped checks and retained failures

The initial workspace selection of fuser dependency unit tests failed compilation
because Cargo requires its dev-dependencies and own package workspace. No test ran.
The [selection diagnosis](checks/fuser-registry-patch/unit-build-selection-diagnosis.json)
then selected the copied package's unmodified own manifest/lock; its `--no-run`
build and four time tests pass. The initial offline core lock resolution selected
13 unrelated compatible upgrades, all discarded before builds. Only unused-patch
metadata remains; [package comparisons](checks/fuser-registry-patch/core-lock-package-comparison.json)
verify unchanged core pins and the [harness comparison](checks/fuser-registry-patch/proof-lock-package-comparison.json)
verifies only the intended fuser source change. Original logs are retained.

The pinned base image lacks Clippy. Its first lint attempt and an invalid bare-ID
Dockerfile build are retained with diagnoses. An owned image built from the exact
base adds the official unmodified Rust1.85.1 Clippy component; the
[qualified warning-denying harness check](checks/fuser-registry-patch/proof-clippy-qualified-receipt.json)
passes, image
`sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
These toolchain setup failures are not timestamp failures or benchmark samples.
The core-tools covering run passes39 tests; the final affected integrity suite
passes13 after the archived own-lock context was included. Changed-fixture
formatting passes, the core boundary scans568 production Rust/SQL files, and
locked daemon/SDK all-targets `--no-run` compilation passes with the unchanged
core dependency graph. No actual test exceeds8s for the Rust timestamp cases or
the explicit120s ceiling for Python tooling.

Local documentation links pass110 checks. First-party whitespace passes; full
Git whitespace checking reports retained publisher whitespace, unified-diff
context markers and raw failed build output. Those exact bytes are preserved.

All existing first-party core production source is unchanged by this patch;
unchanged S7/S9 component evidence remains scoped to those implementations.

## Milestone disposition and next-ready work

The fuser source incompatibility is corrected by the explicitly allowed patch.
The full native signed-minimum fractional requirement still has a hard owning
Linux boundary gate; a fuser patch cannot restore nanoseconds removed before
its callback. Keep S0/S8 open and preserve the exact timestamp requirement until
an allowed owning-platform resolution is qualified. Other S8 product gaps remain:
native mount/requests/dispatch/ownership/coherence and daemon registry/lifecycle/
execution/control/upstream, including ordinary Bash, coherent caches and writeback
off. A prerequisite proof does not implement or qualify those operations.

Independent next-ready batch work remains the full S7 cost gate (complete SQL/
request/page/byte/copy/queue/residency/debt, entire256MiB reservation/high-water/
freelist and worst-case/amortized/cumulative evidence), S8 implementation independent
of the boundary gate, and S9 fair authenticated transport, disconnect fences and
faithful complete-root acquisition. Keep P3/P6/P7/P13/P14 as later Commit prerequisites;
S10–S13 remain outside this batch. Preserve the S5/S6 chat/handoff, unrelated state,
root reference and containers. No push, release or deployment occurs.

This checkpoint's third-party correction, tools and harness do not change
first-party production LOC. Exact first-parent/staged-tree counts and committed
source correspondence are recorded in the separate commit/handoff receipt.
