# #232 all-ioctl v3: 56 retained SDK Exec/FUSE/Commit cases

**Functional result: 56/56 completed, independently verified, and cleaned
up. Cold latency admission: 0/56 eligible.** Every row is `INELIGIBLE` under
the frozen Linux FUSE backing-cache contract, because Commit may consume
bytes Edit just wrote from resident pages. The capped-500 MiB middle insert
also missed its prospective 70 ms **raw engineering aim** at 87.607 ms; that
miss remains visible even though the row is not eligible for a cold
performance verdict. This campaign is a complete one-attempt functional and
diagnostic rollout of the frozen 56-case registry, **not #232 release
admission or a v0.1.6 G2 speedup claim**.

## Frozen source and method

The [prospective contract](../../PHASE1_ALL_IOCTL_CONTRACT.md), immutable
[56-case registry](../../../../../benchmark/fs-bench-pro/registry/workspace-exec-edit-v3.json)
and [pre-run manifest](PRE_RUN.json) preceded every live attempt. The
execution source is `46cf957abf97ad5c340c8faf0dfab7e9fb426658`, product
seal `0945f62bbd3a8532b2fe08cac8b7962d9774c9d66f23e1ab61f7b315e8402aa9`,
harness seal `83af2b85b21db55110580b03ec68efe115a52726c45d800aa67e004dca0030cf`
and v3 registry SHA-256
`469bf23363083980cf2e424394eba994ed7a6009116d215b3d96c379e66b975c`.
The distinct release image is
`sha256:4da3cfabe2ea3bd0df986d5c578671f6623a6a0e8bd5cd94cb0aa179ccca8fa8`.
The target kernel was `6.12.76-linuxkit`, with unmodified published fuser
0.18.0 (`core/Cargo.lock` checksum
`b82b6597d216503555ead6b358f341ef748869bf5c6fbae6a0cb9dd231baecfd`).
`PRE_RUN.json` pins the exact release host binaries, daemon/editor binary
hashes, image, payloads, six closed prepared masters, source tree, fresh
outputs, one construction worker, cache domains and budgets. The historical
v2 registry SHA-256 remains
`05a133b543db33729d60cc321cc88682046c16bfb1761daec6a57c5df3b11823`;
none of its rows or receipts was relabelled.

Every case independently copied one validated closed prepared Store/history
master outside the timer, created a Sandbox and mounted Workspace through
public SDK, ran one `WorkspaceApi::exec` with a cooperating mounted editor,
then called public `WorkspaceApi::commit` once. The tool obtained STATE on
its open descriptor, issued either one inline EDIT or a private
BEGIN/ordered DATA/APPLY sequence, and confirmed exactly one revision,
fstat, bounded open-FD bytes and EOF. No POSIX write substituted for a
registered mutation. The host then read public Status, unmounted, deleted
the Sandbox and ran an independent identity-matched read-only verifier.
The timer begins before Exec and ends at the typed Commit result; setup,
cache checking, Status/cleanup and verification are outside it. Operation
wall and resource observations come only from `layerfs-telemetry` LFT1.

## Gates and route counts

| Registered family | Cases | Functional, verifier and cleanup | Cache-latency status |
| --- | ---: | ---: | ---: |
| Length preserving | 12 | 12 PASS | 12 INELIGIBLE |
| Length changing | 32 | 32 PASS | 32 INELIGIBLE |
| Canonical chunk count | 12 | 12 PASS | 12 INELIGIBLE |
| **Total** | **56** | **56 PASS** | **56 INELIGIBLE** |

All 56 complete performance commands were at most **4.159 s**, under the
15 s limit. All 56 independent verifiers were at most **3.119 s**, under
10 s. Each verifier checked full result bytes/digest, published head,
canonical root and count where prospectively pinned, old pristine root,
mode/mtime, and reopened Branch. Status counters matched each row's exact
two STATE calls and either one inline EDIT, one BEGIN + one Zero DATA + one
APPLY, or one BEGIN + sixteen Bytes DATA + one APPLY. Each row reported
the expected literal-byte total, zero FUSE WRITE callbacks, **zero shifted
suffix bytes**, one Workspace revision, Unmount PASS and public Sandbox
Delete PASS. No `FAIL`, `INCOMPLETE`, or `NOT_RUN` v3 selection was dropped;
there were none among these 56. The separate Phase 1C 128-edit FAIL remains
in its own report and is not counted as a v3 single-edit case.

The macOS Store-domain pre-sample check saw zero resident pages in every
independent copy. The Linux FUSE backing domain cannot be invalidated and
checked between Edit and Commit without a benchmark-only eviction that the
contract forbids. Thus even a raw number under an aim is **not** a cold
`GOAL_MET`. Four insert aims were frozen prospectively; 52 other rows had
no numeric target and receive no invented target comparison. Raw LFT1
caller CPU and RSS samples were retained, but all 56 roots lacked a
resource sample at the close boundary. Their process-shared CPU intervals
and sampled RSS are partial windows, **not exclusive operation CPU or exact
phase peak memory**. No cgroup lifetime peak or host wall substitutes.

## Four prospective insert aims

| Pristine file | LFT1 Exec | LFT1 Commit | Raw Exec→Commit | Raw aim | Raw comparison | `EditFile` finish |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| 1 MiB | 24.150 ms | 19.355 ms | 43.513 ms | 55 ms | below aim | 1.436 ms |
| 10 MiB | 27.284 ms | 22.970 ms | 50.261 ms | 55 ms | below aim | 2.854 ms |
| 100 MiB | 26.940 ms | 28.988 ms | 55.936 ms | 60 ms | below aim | 8.990 ms |
| capped 500 MiB | 31.791 ms | 55.811 ms | **87.607 ms** | **70 ms** | **miss by 17.607 ms** | **35.222 ms** |

The capped-500 minus 1 MiB `service.finish` increase was **33.786 ms**, above
the prospective 15 ms growth aim. Its batch-drain child rose from 0.553 to
33.899 ms, an increase of **33.346 ms**. This agrees with the earlier
[Phase 1B attribution](../../../241/evidence/phase1b-finish-diagnostic/REPORT.md):
the growth sits inside batch drain, while its internal safe optimization
remains unisolated. This is a new v3 diagnostic observation, not permission
to resample an unchanged arm or silently rewrite its gate. The four old
#241 v4 samples use a different source identity, and historical v0.1.6 G2
timed a direct SDK route; neither is a matched speed arm here.

## Raw evidence and disposition

The table below lists every registered selection and its one raw caller
LFT1 duration. Each case links to its retained
[`receipt.json`](attempt-01/overwrite-head-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json)
folder; the [derivation script](derive.py) rechecks all copied `SHA256SUMS`,
source/image/registry identities, LFT1 root and children, callbacks,
independent verifier, cleanup and cache classification and reproduces
[derived.json](derived.json). The [raw attempt folders](attempt-01/) also
hold driver/daemon/Service logs, case inputs and verifier output. Mutated
Store/history SQLite files and the mode-0600 history cursor key remain in
the ignored local run paths pinned by `PRE_RUN.json`; their closed prepared
master hashes are in that manifest. Reproduction uses the exact row command
from the registry with
`python3 core/benchmark/fs-bench-pro/phase2_all_ioctl.py run <scenario-id>`
at a fresh output identity. That command must not be used to resample the
retained v3 attempt.

This completes the v3 all-ioctl **functional** route across all 56 cases.
#232 **latency admission is still open**: the frozen cache contract makes all
56 cold timings ineligible, and the capped-500 MiB raw insert misses its
engineering aim. A later performance decision requires a new prospective
cache-enforceable scenario and a cause-specific batch-drain change if one is
shown safe; no current receipt may be promoted or overwritten.

## Every registered row

| Case | Family | LFT1 Exec→Commit (ms) | Complete command (s) | Verifier (s) | Raw aim | Status |
| --- | --- | ---: | ---: | ---: | --- | --- |
| [overwrite-head-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-head-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 41.512 | 0.919 | 0.022 | — | INELIGIBLE |
| [overwrite-head-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-head-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 40.819 | 0.921 | 0.077 | — | INELIGIBLE |
| [overwrite-head-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-head-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 49.864 | 0.916 | 0.631 | — | INELIGIBLE |
| [overwrite-head-4k-on-500mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-head-4k-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 74.514 | 0.925 | 3.094 | — | INELIGIBLE |
| [overwrite-middle-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-middle-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 42.921 | 0.897 | 0.025 | — | INELIGIBLE |
| [overwrite-middle-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-middle-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 48.138 | 0.913 | 0.084 | — | INELIGIBLE |
| [overwrite-middle-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-middle-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 55.779 | 0.917 | 0.622 | — | INELIGIBLE |
| [overwrite-middle-4k-on-500mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-middle-4k-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 86.399 | 0.936 | 3.101 | — | INELIGIBLE |
| [overwrite-tail-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-tail-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 44.646 | 0.919 | 0.021 | — | INELIGIBLE |
| [overwrite-tail-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-tail-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 39.349 | 0.927 | 0.076 | — | INELIGIBLE |
| [overwrite-tail-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-tail-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 49.785 | 0.925 | 0.631 | — | INELIGIBLE |
| [overwrite-tail-4k-on-500mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-tail-4k-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_preserving | 76.180 | 0.937 | 3.104 | — | INELIGIBLE |
| [insert-middle-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/insert-middle-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 43.513 | 4.159 | 0.201 | below | INELIGIBLE |
| [insert-middle-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/insert-middle-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 50.261 | 0.927 | 0.080 | below | INELIGIBLE |
| [insert-middle-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/insert-middle-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 55.936 | 0.921 | 0.626 | below | INELIGIBLE |
| [insert-middle-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3](attempt-01/insert-middle-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 87.607 | 0.950 | 3.090 | MISS | INELIGIBLE |
| [delete-middle-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/delete-middle-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 42.014 | 0.908 | 0.022 | — | INELIGIBLE |
| [delete-middle-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/delete-middle-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 44.989 | 0.918 | 0.077 | — | INELIGIBLE |
| [delete-middle-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/delete-middle-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 54.148 | 0.911 | 0.630 | — | INELIGIBLE |
| [delete-middle-4k-on-500mib-ops-1-exec-ioctl-v3](attempt-01/delete-middle-4k-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 80.803 | 0.943 | 3.107 | — | INELIGIBLE |
| [append-tail-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/append-tail-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 39.201 | 0.915 | 0.021 | — | INELIGIBLE |
| [append-tail-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/append-tail-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 38.727 | 0.900 | 0.082 | — | INELIGIBLE |
| [append-tail-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/append-tail-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 47.983 | 0.940 | 0.621 | — | INELIGIBLE |
| [append-tail-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3](attempt-01/append-tail-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 69.569 | 0.932 | 3.119 | — | INELIGIBLE |
| [prepend-head-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/prepend-head-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 42.642 | 0.916 | 0.021 | — | INELIGIBLE |
| [prepend-head-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/prepend-head-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 40.568 | 0.948 | 0.078 | — | INELIGIBLE |
| [prepend-head-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/prepend-head-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 51.035 | 0.918 | 0.625 | — | INELIGIBLE |
| [prepend-head-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3](attempt-01/prepend-head-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 67.134 | 0.932 | 3.096 | — | INELIGIBLE |
| [replace-grow-middle-2k-to-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/replace-grow-middle-2k-to-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 43.805 | 0.905 | 0.021 | — | INELIGIBLE |
| [replace-grow-middle-2k-to-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/replace-grow-middle-2k-to-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 45.571 | 0.919 | 0.085 | — | INELIGIBLE |
| [replace-grow-middle-2k-to-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/replace-grow-middle-2k-to-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 53.352 | 0.922 | 0.637 | — | INELIGIBLE |
| [replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3](attempt-01/replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 79.204 | 0.909 | 3.083 | — | INELIGIBLE |
| [replace-shrink-middle-4k-to-2k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/replace-shrink-middle-4k-to-2k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 44.801 | 0.911 | 0.022 | — | INELIGIBLE |
| [replace-shrink-middle-4k-to-2k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/replace-shrink-middle-4k-to-2k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 47.365 | 0.915 | 0.081 | — | INELIGIBLE |
| [replace-shrink-middle-4k-to-2k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/replace-shrink-middle-4k-to-2k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 52.522 | 0.935 | 0.622 | — | INELIGIBLE |
| [replace-shrink-middle-4k-to-2k-on-500mib-ops-1-exec-ioctl-v3](attempt-01/replace-shrink-middle-4k-to-2k-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 97.797 | 0.933 | 3.110 | — | INELIGIBLE |
| [truncate-tail-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/truncate-tail-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 39.973 | 0.892 | 0.022 | — | INELIGIBLE |
| [truncate-tail-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/truncate-tail-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 38.572 | 0.919 | 0.079 | — | INELIGIBLE |
| [truncate-tail-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/truncate-tail-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 46.444 | 0.933 | 0.634 | — | INELIGIBLE |
| [truncate-tail-4k-on-500mib-ops-1-exec-ioctl-v3](attempt-01/truncate-tail-4k-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 71.692 | 0.939 | 3.088 | — | INELIGIBLE |
| [zero-extend-tail-4k-on-1mib-ops-1-exec-ioctl-v3](attempt-01/zero-extend-tail-4k-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 40.416 | 0.832 | 0.021 | — | INELIGIBLE |
| [zero-extend-tail-4k-on-10mib-ops-1-exec-ioctl-v3](attempt-01/zero-extend-tail-4k-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 38.188 | 0.851 | 0.077 | — | INELIGIBLE |
| [zero-extend-tail-4k-on-100mib-ops-1-exec-ioctl-v3](attempt-01/zero-extend-tail-4k-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 48.389 | 0.929 | 0.627 | — | INELIGIBLE |
| [zero-extend-tail-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3](attempt-01/zero-extend-tail-4k-on-500mib-result-capped-v2-ops-1-exec-ioctl-v3/receipt.json) | edit_length_changing | 66.922 | 0.920 | 3.114 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 50.624 | 0.900 | 0.022 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 53.399 | 0.928 | 0.078 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 55.297 | 0.923 | 0.634 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 81.801 | 0.934 | 3.109 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 49.541 | 0.927 | 0.022 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 49.418 | 0.936 | 0.081 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 60.251 | 0.914 | 0.631 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 82.715 | 0.944 | 3.105 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 47.726 | 0.935 | 0.022 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 47.551 | 0.921 | 0.081 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 58.358 | 0.925 | 0.623 | — | INELIGIBLE |
| [overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1-exec-ioctl-v3](attempt-01/overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1-exec-ioctl-v3/receipt.json) | edit_canonical_chunk_count | 79.317 | 0.936 | 3.090 | — | INELIGIBLE |
