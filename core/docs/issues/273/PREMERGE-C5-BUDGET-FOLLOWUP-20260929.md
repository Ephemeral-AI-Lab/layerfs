# #273 → #264 ordered live SDK C5 and fixed-Budget follow-up

> **Status:** Dated planning checkpoint; not release evidence or a product contract.
> Functional source `b45ed6d491610d965b43595e345d8d44d94dc7a1`;
> product source still `f00644479a9b7dfe0b74e02438eed6d60e30dc0a`.
> This follows [the previous ordered handoff](PREMERGE-ORDERED-LEASE-FIXES-20260929.md)
> without rewriting its older attempts. **NO MERGE; no numeric/release admission.**

## 3. Held public SDK G1 lease through known C1/local C5 failure — PASS, scoped

Test-only `ff660d34cbe5449caeaf43e654d58d71bb634b03`, with lint-only
follow-up `b45ed6d49`, adds a live authenticated SDK Commit and a *separate
host Service relay* bound on the host LAN address at the same port as the real
Service's 127.0.0.1 listener. The relay forwards checked encrypted records
unaltered. After the test arms it, a read-only SQLite history query detects
that this exact Branch's canonical head was published; it gates the Service's
reply ciphertext **before** the daemon consumes it. The test applies 2 KiB
`RLIMIT_FSIZE` to PID 1 of *only* its labeled owned container, releases the
gate, then restores the original unlimited limit. A test-only Docker CLI
wrapper starts that immutable daemon with SIGXFSZ ignored so an actual short
file write reports failure rather than killing the daemon. There is no product
fault hook, second Commit, changed quota/deadline, custom daemon image, or
third-party patch. The static fault helper source is under SDK tests; this
run used the independently compiled and SHA-recorded owned helper from the
earlier Linux capability probe. The helper is removed from the volume.

At clean `b45ed6d49`, public SDK `WorkspaceApi::commit` returned an actual
`WorkspaceCommitFailureWire`: `phase=Reconcile`,
`disposition=KnownCommitLocalFailure`, `cause=Io`, `unknown=false`,
`installed_revision=None`, with equal known/observed `Committed` canonical
outcomes. The committed head bytes exactly matched the relay's independent
published history-head query. The pre-Commit lease retained its original
G1 generation/revision and returned exact old `g1-note` after the failure;
read-only status reported one held lease. Checked release and unmount then
completed, but daemon shutdown reported retained `Busy` after the physical
C5 fault: **this is not a clean backing-close or refund proof**. Owned Docker
container/volume deletion and complete log capture were checked separately.
Clean local-only `core/target/issue273-next-c5-functional-final-01/result.json`
SHA256 `a1a213053bd12fd23bbe781456d2bc186c7705245e3f98f699f48a5d6a960d27`,
including raw relay/SDK logs and `SHA256SUMS`.

The earlier dirty diagnostic attempts remain distinct: first Docker cp could
not write through the read-only rootfs; next `/tmp` was noexec; the third
reached a typed known local failure but the relay misclassified its idle socket
timeout. After bounded test-only corrections the dirty diagnostic passed and
was **not** promoted to clean evidence. No original FAIL is reclassified.

## 4. Fixed 16 MiB public SDK response Budget — PASS, scoped

Test-only `51e62fa3528a040bb641cc2c809a2f1ad3f3ab64` plus lint-only
`b45ed6d49` builds a standalone static POSIX writer, then executes it through
**public `WorkspaceApi::exec` and the real mounted FUSE path**. In an unchanged
16,777,216-byte sandbox Workspace Budget, real accepted file creations and
writes occupy charged namespace/index state; a second public lease then issues
1,541 actual existing long-name entries until the *next* lookup returns
`Capacity` (unknown=false). The checked lease's registered entry count is
identical before/after this refusal: no partially registered entry. At
16,580,845 bytes charged (196,371 remaining), a 131,072-byte first lease
`view_read` returns `Capacity`, unknown=false, with **no partial response**;
the one-byte read still succeeds. The larger response and its necessary remote
call cannot both reserve under the unchanged Budget, unlike the one-byte
response. This is a response-size-dependent Budget refusal, **not** the 33rd
lease/count gate or a fabricated response. It does **not** isolate the first
reserve site (`view_reads.rs` buffer versus the remote-call scratch); a
reserve-site-specific claim needs a separate diagnostic. Both leases release,
unmount and owned sandbox deletion complete.

Clean local-only `core/target/issue273-next-budget-functional-final-01/result.json`
SHA256 `953eddffd4013dde2d2e27169b35e7e47694f22fdcf259f3b495a1682108ec`,
including static helper hash, compiler command, the entire public test output,
source hashes and `SHA256SUMS`. The initial 60,000 sequential append attempt
retained only 2,482,781 bytes of charge and **FAILED** to reach Budget. A
32,768 separated-write attempt also **FAILED** to reach Budget. A many-name
attempt with a fixed two-batch cap **FAILED**; another continued until
ordinary creation refused with 881,539 bytes remaining. The next dirty
attempt **FAILED** at entry-registration Capacity before its read assertion;
its no-partial-entry observation guided the bounded final route. These raw
local-only diagnostics under `core/target/issue273-next-budget-diagnostic-*`
are not clean-source PASS and have no numeric/performance claim. The trivial
run without `LAYERFS_BUDGET_PROOF` only skipped the earlier unfinished test and
is **not** Budget proof. The source changed between dirty attempts; their
original exact dirty-tree seals were not captured and must not be invented.

## Still open, in order

5. **Bounded 64 MiB lowering remains FAIL / NOT_REPAIRED.** Original unchanged
   fixture, oracle, quota and failed route:
   `core/target/issue273-next-stage-lowering-01/result.json` SHA256
   `600737f01fca1a82ae92f49a009ee88b7bc8c7bc6a2deb69c797720f22587e67`.
   The external `Fixture::edit` copies nearly the entire right tail for a
   length-changing early splice using public fixed-offset `set_len`/`write_file`.
   This acquires near a 64 MiB new payload before Stage under the existing
   64 MiB quota. The current public Workspace fixed-offset writes/truncation
   and POSIX FUSE surface have no atomic range-shift or collapse/insert
   operation; internal `Extent` *can* represent shifted Base offsets, but a
   test-only call into private extents, a fake partial oracle, or a changed
   quota is not an eligible public fix. A reviewed, generally supported
   public atomic extent-range operation **and** corresponding oracle/backing
   proofs (or an owner-disposed contract) are required before this gate passes.
6. **2 MiB headroom escrow remains FAIL / owner ruling PENDING.** Unchanged
   `core/target/issue273-next-stage-headroom-01/result.json` SHA256
   `847c7e40964ddd6e41c2256c906b1117fc4579e2212542d0596047387aaca97c`
   fails `Backing(Allocate, StorageFull)` at Stage. No committed escrow change
   or explicit owner disposition. A refund-only shortcut must not trade away
   failure custody or physical accounting.
7. **Numeric admission remains INELIGIBLE.** No independently matched
   private/VM/backend/device/host cache contract and phase-local cgroup
   observer has been proved; frozen control remains `NOT_RUN`, nine original
   numbers remain `INELIGIBLE`. #283 deferred only the Docker Desktop
   `memory.peak` reset observation, not a numeric gate or product leak ruling.

Historical unlogged token FAIL's exact cause and SDK cancellation proof still
remain respectively unknown and `NOT_RUN`. #256 and #270 remain functional-only
deferred / `NOT_PROVED`. No PR head was merged and no owner waiver inferred.

## Verification and size

At clean `51e62fa35`, locked all-12-package Core test PASS, Core examples
build PASS, boundary scanner PASS (359 production files), its nine self-tests
PASS, and Core fmt PASS. Initial warning-denying Clippy command with
`RUSTFLAGS=-D warnings` **FAILED** because it overrode the mandatory ARMv8 AEAD
repository config; that attempt is not a product defect. The corrected
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --workspace
--all-targets -- -D warnings` on the lint-only successor `b45ed6d49` PASSed.
Python syntax and diff whitespace PASS. The clean live SDK routes above both
PASSed at `b45ed6d49`; the all-12-package test precedes only the test-only
formatting correction and was not repeated on that successor. There is no CI
or preflight claim, no performance selection, and no numeric admission.

Every commit here changes only external tests or docs; production LOC, using
`python3 tools/production_loc.py --json --root` on first parent and final
staged/committed tree with identical exclusions: reference **65,417 → 65,417
(0)**, Core **70,670 → 70,670 (0)**, combined **136,087 → 136,087 (0)** for
`ff660d34c`, `51e62fa35`, `b45ed6d49` and this docs-only checkpoint.
