# First real SDK/FUSE/SQL/MinIO/publication experiment

> Status: Research; informative and not a product contract.

The first complete path passed on 2026-10-02: real commands through the existing
public `WorkspaceApi`, a real Linux FUSE mount, daemon-owned SQLite final-state
extents, actual C1 construction and C2 FULL/compression/pack assembly, acknowledged
MinIO objects, global locator registration, actual C5 conditional publication,
known-result installation, explicit unmount and SDK container/volume deletion.
Both immutable published versions were then read from MinIO and checked against
literal expected bytes after the daemon had been removed. This is a scoped
correctness result, not Phase 6 completion or speed/resource admission.

Measured source: [`2aebefc14860a9c66ba0aeaf83a9dd1da69d6b0f`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/2aebefc14860a9c66ba0aeaf83a9dd1da69d6b0f).
Prospective specification: [`0475f08a3f9e53c9848cc72d6109e0a6e0c1a515`](https://github.com/Ephemeral-AI-Lab/layerfs/commit/0475f08a3f9e53c9848cc72d6109e0a6e0c1a515),
with the explicit Workspace/revision wire fields and full candidate validation
recorded with the implementation. Provider, dependency, ARMv8 configuration,
workload, binary and image hashes are in the [identity](evidence/integration-v3/identity.json)
and [provider record](evidence/integration-v3/provider.json).

## Observations

| Real command | Exec | Complete Commit API call | Generation | Semantic byte/mode proof |
| --- | ---: | ---: | ---: | --- |
| Create 4 KiB zeros; write `phase6` at offset 17; read size through FUSE | 15.653250 ms | 298.708667 ms | 1 | PASS |
| Write `SECOND` at offset 100, retaining the first write and file size | 7.389250 ms | 304.167750 ms | 2 | PASS |

The complete collector child took **6.922491 s**, including its global initialization,
SDK container/mount lifecycle, both commands/commits, cleanup and proof. Its
recorded SDK lifecycle interval was **6.604626 s**. Genesis construction/storage
was **24.489708 ms** before that SDK interval; it remains included in the full
child wall and is never credited to Commit. Separate readback/history proof was
**23.705875 ms**. There was one invocation, no failed attempt and no unchanged-arm
rerun. See the exact [receipt](evidence/integration-v3/receipt.json),
[invocation](evidence/integration-v3/invocation.json) and
[daemon log](evidence/integration-v3/daemon.stderr).

The first Commit acknowledged 11 new objects in 3,959 physical pack bytes and
reused 7 exact objects; the successor acknowledged 7 objects in 2,556 pack bytes
and reused 11. Callback count advanced from 35 to 61. Explicit unmount confirmed
zero remaining issued handles. MinIO's owned process exited normally and its
isolated disk data was retained; [provider drain](evidence/integration-v3/provider-drain.json).
The SDK deletion returned success after its normal container and volume absence
checks. The raw control-session EOF diagnostics remain in the log; they are
connection-close observations, not failed Workspace commands or publications.

All numerical observations are **INELIGIBLE for speed admission**: OS/page cache
state was unknown. There is no matched Phase 4.5/Phase 5 control, no benchmark
campaign and no claim of a faster Commit. Uploaded payloads were small and highly
compressible, so these pack-byte counts are not a repository compression ratio.

## What this proves and what remains open

SQLite is opened for the daemon mount lifetime, and global locator/history
connections live for the host authority lifetime. The Commit path does not create
another SQLite database or engine. Payloads reside in MinIO; the global object
catalog has locator rows, not pack BLOBs. Real service-side candidate validation
walks the admitted graph, portable metadata and file payloads before C5 publishes
an exact conditional Branch head. The successor's recorded parent is the first
Commit. Independent literal bytes and mode expectations pass for both versions;
canonical object authentication passes on readback. Independent canonical root
vectors, exact namespace/mtime reference comparisons and G1/G2 overlap remain
unqualified and are not inferred from these checks.

This first runtime deliberately has a finite 512-inode/binding construction
profile, 256 issued handles, serialized mutation/Commit, full metadata rebuild
and full reconstruction of changed files. It uses the explicitly experimental
ordinary FULL representation for canonical inode leaves. Shipping pooled metadata,
delta choice and dense pack batching are not integrated. It cannot yet justify
indefinite streaming or the six larger cohort profiles. Source retirement remains
open: the external SQLite check finds 3 final extents but 65 immutable sources after
64 repeated overwrites. V1's fragmented range transaction failure is unchanged.
Physical memory, healthy progress, quota, failures/Unknown/mount-death and full
ordinary-POSIX coverage remain unrun. Conservative pending custody refuses a
further Commit after an uncertain publication; no injected failure proof is claimed.

One clear source-level cost is avoidable transport setup. Each locator request
currently opens/authenticates its own native connection. From the actual finalized
object counts, each Commit makes at least 36 such requests in its consumer alone:
`new * (lookup + register) + reused * (lookup + authenticated-reader lookup)`.
That is `11*2 + 7*2` and `7*2 + 11*2`, before graph reads and final publication.
This is a count derived from source and recorded object counts, not a measured
latency attribution. Session reuse and bounded locator/pack batching are the next
integration improvements before larger cases or any speed qualification. Removing
validation is not supported by this result.

## Checks and continuation

The [check record](evidence/integration-v3/checks.json) gives exact commands and
unrun work. Three external real-SQLite tests pass; host warning-denying Clippy,
formatting, Darwin and Linux ARMv8 release builds, Python collector compilation,
357-file product-boundary scan and 9 guard tests pass. All third-party versions
and checksums match Core's lockfile; bundled SQLite 3.53.2 is observed at runtime.
Shipping product source is unchanged. Full unchanged Core suites were not rerun;
Linux Clippy is not claimed. Raw available check outputs are retained beside the
receipt. All earlier v1/v2 receipts and verdicts remain unchanged.

Each owning commit reports reference 65,417 / Core 70,279 / combined 135,696 with
signed delta 0: the runtime is research tooling outside shipped product scope.
The exact first-parent/staged/committed snapshots were counted with
`tools/production_loc.py`, SHA256
`c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`,
using Git archives and the same classification including shipped SQL, excluding
benchmarks/tools/tests/docs/generated/third-party code. This is not a zero-sized
product or a product architecture/LOC reduction claim.

Next: freeze persistent authenticated metadata sessions and bounded registration/
packing with exact ACK/Unknown custody, then run newly declared larger live cases.
Localized edits, canonical vectors and physical resource proofs remain separate
required gates. #293/#294 remain open; #288 qualification is delegated/unrun.
