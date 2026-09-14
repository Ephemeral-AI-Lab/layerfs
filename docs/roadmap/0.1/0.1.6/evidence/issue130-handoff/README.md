# Cleanup and #130 execution handoff preservation

This directory records preservation, not product or performance qualification.
The owner requested one local branch (`main`) and cleanup of completed migration
branches, while explicitly preserving unrelated open-PR branches.

Before cleanup, all local/remote-tracking refs, both historical stashes and two
stale detached-worktree heads were saved in a verified Git bundle outside the
checkout. The archive location, bundle checksum and retained file identities are
in [preservation-manifest.json](preservation-manifest.json). The archive contains
the original ref/worktree/stash inventory, stash patches and unfinished source
patch. It must not be treated as disposable benchmark output.

The four unfinished source files exactly match
[the cancelled Step 5 patch](../step5-non-commit-consumers/UNVERIFIED-cancelled-work.patch)
at SHA-256 `8a50da256a3292530fe91e60ae87b88eec1da5557e796daa635dcb21f40ca837`.
They are preserved here through that unchanged historical artifact, but restored
to committed product source in the working tree. Do not apply the patch wholesale:
its post-publication metrics failure and unbounded reconciliation identity work
need review/repair, and it has no complete passing qualification.

Whitespace validation of the import reports seven existing whitespace-only added
lines inside that raw patch. Preserve its exact bytes/checksum; do not rewrite
historical source evidence to make an artifact hygiene check green. Whitespace
checking of the remaining publication passes. This is not a product/test waiver.

The trajectory branch's capacity logs, cancelled probe sources/raw output and
Step 5 notes are copied byte-for-byte to their existing evidence paths; historical
L28–L36 are appended to the verification ledger with explicit source/validity
context. L38's corrections remain authoritative. The original debug/census timing
is not production throughput, and the interrupted suite failure is not erased.

[DEFERRED-scale-harness.patch](DEFERRED-scale-harness.patch) preserves the
trajectory-only `host_overlay.rs` harness changes without enabling them. The
25k/two-second and million-file cases remain deferred. Probe sources are retained
historical first-party evidence; no third-party source or dependency import is
patched, and no probe is run by this publication.

To inspect preserved branch history without recreating working branches:

```sh
git bundle list-heads /absolute/archive/path/repository.bundle
git bundle verify /absolute/archive/path/repository.bundle
```

Use the actual absolute archive path from the manifest. Recovery is an explicit
inspection/selection task; do not restore every archived branch, stash or patch
as part of the new implementation.

The successor starts with
[the execution handoff](../../overlay-minimal-overhead-execution-handoff.md)
and the current implementation plan. Previous branch names and owner-stop notes
inside historical evidence do not override current scope or imply an active run.
