# Family 7 prospective package selection

**Status: Dated planning checkpoint; not release evidence or a product contract.**

Profile `package-many-file-functional-clone-v2`, one public SDK/POSIX-FUSE
attempt per case in the following frozen order. Host owns SDK/server, SQLite and
canonical publication; Docker owns daemon/FUSE/BusyBox workload. One construction
worker; default Workspace Budget/quota unchanged. No product/codec change.

| Current ID suffix (prefix `workspace-shell-package-`) | Workload | Complete command limit |
| --- | --- | ---: |
| mixed-refresh-sdk-v2 | Original `mixed-refresh-v1` recipe: in-place, create/delete and temporary-file replacement, fixed 9-file package | 25 s |
| overwrite-4k-sdk-v2 | Original `overwrite-4k-v1`: 4096 P bytes at 5 MiB in a 10 MiB all-A file | 15 s |
| repeated-one-byte-sdk-v2 | Original `repeated-one-byte-v1`: first 16 bytes X in 64 KiB all-dot file | 15 s |
| many-128-sdk-v2 | Create 128 `many/f<i>` files, exact bytes `new-<i>`; list/count all | 15 s |
| many-129-sdk-v2 | Same recipe with 129 files, one directory, one final Commit | 15 s |
| many-257-sdk-v2 | 257 files distributed by i%17 across 17 directories, one final Commit | 15 s |
| many-1025-sdk-v2 | 1025 tiny package files across 17 directories, one final Commit; declared scale exception | 25 s |
| many-129-live-g2-sdk-v2 | Timed G1 creates129 `old-<i>` files and commits/pins f0; G2 updates129, deletes f128, replaces f127 via rename; one G2 Commit; full held `old-0` pin | 15 s |

Three closed fixture masters (package/large/repeated) are acquired once through
**the existing** `benchmark_init`/`ProjectApi::init`, then the existing
`benchmark_shell` seeds a known branch head. Full read-only baseline verification
runs once. Every sample clones a closed master by independent writable byte copy;
masters and binaries are SHA256 sealed. Package v2 image inputs use the unchanged
original recipe. Incremental release builds, immutable binary archive and matching
image layers are reused. Setup work remains outside measured phases; no measured
mutation, lowering, publication or cleanup is moved into setup. All G1/G2 work
for the retained case is inside its complete command.

Separate existing `verify_checkpoint5` runs once per completed receipt, bound9s:
full old/new tree kinds, portable modes, every regular-file byte, absence of extra
paths, exact head/parent and unchanged core/index.js identity. Retained old/new
many-file contents plus complete pin bytes are required. No sampled-content claim.
SDK Mount/Exec/Commit/Status/unmount/delete and launch-to-exit lifecycle are timed.
Numeric latency is **INELIGIBLE** because host/daemon/input/dirty-byte cache state
is uncontrolled. Raw intervals remain diagnostic; command and proof bounds are
functional completion gates. No comparative speed claim is registered.

Historical IDs/profile/receipts remain untouched. `failed-command-no-commit-v1`
is visible as OWNER-DEFERRED / NOT_RUN: its unmount-discard cleanup assumption is
the deferred [#276 finding](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903530127).
Family6 r069 remains FAIL and r070's explicitly different recovery remains PASS.
Do not repeat dirty-close or introduce a discard API in this campaign.

Finite scale success does not close #256: arbitrary namespace size, bounded
streaming/frontier memory, overflow encodings, single-handle paged enumeration,
128 simultaneous handles and page/scan counters remain separate scope. The
whole-frontier C5/Commit architecture and10240 writes remain deferred on#276.
Families1–6 production paths are unchanged: reuse their published checkpoint
proofs, do not rerun an earlier family. Current owning harness checks cover this
adapter and shared binding/cursor plumbing; no new Rust implementation or runner.
