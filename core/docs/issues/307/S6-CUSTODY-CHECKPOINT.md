# S6 independent custody checkpoint

> **Status:** Implemented checkpoint on local main after `1775fdf98`, dated
> 2026-10-06. S6 is still IN PROGRESS; this is not an exit audit or release receipt.

Regular-file last unlink now creates independent mutable orphan state without a
payload copy. Exact engine-minted opens and file-read windows retain it separately
from namespace generations. Descriptor write/truncate revalidates writable custody
inside every Workspace round and its atomic apply. Reads preserve the old immutable
root while namespace installs advance; maintenance composes at most two retained
lower domains into the orphan one cell at a time. The effective orphan has at most
three layers while migrating, then one, independently of Commit count.

Captured-reader tokens retain their original root/generation/floor after install;
they park definite-failure composition until exact release. Logical close refuses
new captured acquisitions but preserves existing consumer custody. Engine-minted
operation tokens and independent backed scratch prevent old cleanup from deleting
new processing state, even when a caller request key is reused after release.
Last-owner release drives live and idle maintenance; terminal cleanup waits for
all independent owners. Primitive caller-ID leases/sources retain their trusted
lifetime-uniqueness precondition and are not minted capabilities.

Removed regular-file payload/steps without owners are garbage immediately, deleted
in bounded live jobs. Their namespace tombstone remains while the immutable base
could still bind the serial. Failed composition retains that tombstone when an
orphan holds old source rows, without copying orphan payload back into namespace
state. Serial retirement may delete obsolete metadata only below installed floor
or when a newer live row supersedes it. The maximum serial has explicit cursor
completion rather than overflowing its successor.

The [architecture](../../architecture/33-independent-custody.md),
[identity](checks/s6-orphans/identity.json) and
[append-only failure ledger](checks/s6-orphans/FAILURES.md) carry actual scope.
Retained raw logs contain failed compiler/assertion/quota runs and their corrected
covering checks; no prior failure is relabelled. Every test invocation has an
explicit wall ceiling no greater than 120 seconds after `--no-run` build. No test
reached that ceiling in this checkpoint. The erroneous attempted test after an
unsuccessful build is explicitly recorded in the ledger.

The actual real-owner proof uses a content-constructed canonical root and prepared
actor installs: 24 captures, 12 known installs and 12 definite failures preserve an
inherited open-unlinked log, truncate/regrow and a read window beyond descriptor
close. Idle maintenance completes after the last read releases. Engine proofs
cover 20 alternating outcomes at the maximum serial, three-layer cutoff
composition with an independent pre-unlink reader, sealed read/install/failure
ownership, stale tokens, reused caller request keys, released scratch and live
removed-payload cleanup. Native FUSE/kernel references/output are not established.

At fresh, identical file states beside 128/1024/4096 unrelated processing owners,
complete engine jobs have equal `(statements, VM steps, changed rows)` on macOS:
open `(11,274,4)`, unlink plus reply attempt `(19,791,8)`, read acquisition
`(13,351,6)`, local orphan read `(8,200,0)`, descriptor append plus reply attempt
`(19,559,5)`. Actual custody/maintenance/payload templates have indexed EXPLAIN
plans. Complete Workspace/owner overwrite, shrink/regrow/read/append and namespace
profiles retain their unrelated-scale comparisons; owner foreground and automatic
maintenance counters are separate. These are deterministic work diagnostics,
not cold latency, residency, sustained-rate or physical device admission claims.

Current required S6 work: non-file lookup custody and a symlink view that survives
failed composition; exact whiteout lower-binding facts; physical reservations,
cleanup headroom, actual device ENOSPC and complete resource/debt evidence. Existing
MEMORY/OFF SQLite and page-quota FULL proofs do not satisfy that physical row.
P3 remains explicitly carried to backed S10 editing under the S5 audit. The external
native fuser timestamp blocker remains scoped to S0/S8/S12; no third-party source,
pin or package was patched. Root reference and excluded predecessors remain intact.

## Verification and source size

Final retained verification and exact parent/staged comparison are recorded below
before the local checkpoint commit. Unchanged source families may reuse earlier
pinned evidence; this checkpoint does not claim aggregate CI or release acceptance.

- macOS: overlay 37, Workspace 28, daemon 9 and SDK 6 tests pass (80 total);
  the final added three-layer case is retained separately without rerunning
  unchanged cases.
- Linux ARM64: the selected overlay/Workspace/daemon/SDK build passes; 74 tests
  pass, with SDK global-provider cases compiled but macOS-only in execution.
- Scoped four-package all-target Clippy with `-D warnings`, fmt and product
  boundary guard pass. Tool unit tests: 26 pass. Current-guide relative links
  pass; raw logs retain original output/whitespace.
- Linux retains the pre-existing persistence `BackendError::Filesystem` unused
  variant warning in its selected platform build. No dependency was changed.

Production LOC: **148327 ->150019 (delta +1692)**. Core (including excluded
predecessors) 82910 ->84602; root reference 65417 ->65417. No source relocation
or retirement is claimed in this delta. This growth implements custody, separate
orphan/processing state and required validation; tests/docs/logs are excluded.
Counter: unchanged `tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`.
Method: exact first-parent and staged-tree archives of `core/crates` and `crates`,
same nonblank/noncomment product-src and shipped-SQL scope/exclusions, including
inline-test exclusion. Receipt: `core/target/cluster2-307/loc/s6-custody-staged.json`;
recomputed after the final staged documentation and confirmed against the commit.
