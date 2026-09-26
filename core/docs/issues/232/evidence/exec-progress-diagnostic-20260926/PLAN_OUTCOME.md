# #232 Commit failure outcome diagnostic, 2026-09-26

> **Status: Dated planning checkpoint; not release admission.**

The release product attempt at source `563c104dd1a628cd4f345798fb38c5ff19208546`
got past the former five-second Exec silence and entered Commit. Its retained
LFT1 shows `EditFile` failed after 64.555 ms, but the complete command reached
the fixed 15-second wall during cleanup before the driver could print its final
receipt. That attempt remains FAIL with verification `SKIPPED`.

At one new clean source, the release SDK benchmark driver will print the
existing Commit result once, before cleanup, only when
`LAYERFS_EDIT_OUTCOME_DIAGNOSTIC=1`. It does not alter the product call, timeout,
worker count, cache policy, fixture, command or cleanup. Build a sealed release
Linux daemon image and release host driver, prepare an independent master and
freeze their identities before the attempt. Run the same registered 10 MiB
prepend case once with `--verification skipped` in a fresh append-only output
directory. Preserve any failure and use the pre-cleanup line only to diagnose
the cause; do not compare its wall time with the previous run or use it as a
replacement performance sample.
