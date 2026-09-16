# #154 final report — all 33 regular cases and 3 extensions terminal at seed 1

Identity (one chain for the whole matrix): source seal
`86f14b2d68ece2ae368f8aec29070520505a61c7aa69b445414b64561640e954`, product seal
`970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` (**unchanged**:
only the harness, the oracle and the producer declarations moved), compilation seal
`679b17f0e17636ae419144edb7377be03dd1709ac362d72ea0dff61d3b89306b`, dependency seal
`374f4dfa08f98ced734e540f62dd4a8b0edb5a88c090c051ae704fbf7f67faf1`, image
`layerfs-bench-infra:86f14b2d68ece2ae` = `sha256:af6a400356e36b2f58e5eb9df07a291d8567cb975adbb183ed5d76b0c5e7268d`,
host binary `fcaa14d8decf96a6247d95a037e83eea7322aeb66a51d418acbee2e21fdd6aa7`.
One performance and one separate verification invocation per case and mode, seed 1,
fresh append-only outputs; every superseded and stopped attempt stays on disk.

## Result

| verdict | rows |
| --- | ---: |
| PASS inside the 15 s target | **29** |
| declared ≤25 s exception (`mixed` / `workspace` / `branch` `…500mb-30000-k100-v1`, verify of the first two) | **6** |
| declared ≤30 s verification exception (`v016-branch-mixed-500mb-30000-k100-v1` verify: 23.10 s) | **1** |
| performance `N/A` (the six declared verify-only access cases) | **6** |
| FAIL / TIMEOUT / NOT_RUN | **0** |

Performance: **27 PASS**, 6 `N/A`. Verification: **33 PASS**. Extensions: verify
PASS 15.01 s (`…100mb-5000-k100`, 120 s watchdog), verify PASS 59.33 s
(`…500mb-30000-k100`, 300 s watchdog — a 298.28 s TIMEOUT in L8), perf PASS 7.57 s /
verify PASS 9.88 s (`…workspace-four…`, 60 s each).

## The ten cases that did not exist before

`v016-history-namespace-inode-k{10,100}-v1` — perf PASS 2.12 s / 2.82 s, verify
PASS 3.68 s / 9.45 s. 100 Created commits, 101 retained roots, every parent edge,
states `[0, 1, 2, 49, 50, 94, 95, 99, 100]`, envelope S+2 names and S+8192 bytes.

`v016-branch-convergent-content-v1`, `v016-branch-fork-descendant-v1` — perf PASS
2.12 s / 1.98 s, verify PASS 2.09 s / 2.27 s. 30/40 Created commits, ancestry
15/20, convergent children sharing one first-medium-file content root, descendant
B/C branch-salted, every sibling head preserved.

`v016-access-{boundary-before,boundary-after,inode-before,inode-after,fork-point,divergent-head}-v1`
— performance `N/A`, verification PASS 1.59–1.96 s. Each mounts one selected
retained state of a sealed producer, creates no commits, and compares its two
declared read passes byte for byte with the producer's own declaration.

## Owner ruling recorded in this phase (L12)

`v016-branch-mixed-500mb-30000-k100-v1` verification carries a **declared 30-second
ceiling** instead of the pre-existing 25 seconds, because the L10 redundancy work
left it at 22.99–23.10 s against a 22.0 s work window (2.56 s setup, ~11.4 s
210-commit replay, ≈7.7 s proof). The exception is keyed by the exact registered ID,
reported as `EXCEPTION` with its measured wall, and asserted against accidental
widening by a harness test. Unchanged: the 15-second family target (still reported
separately), the three-second cleanup reserve, every other row's 25-second ceiling,
and every oracle, coverage and limit. The ≈7.7 s proof phase still verifies the same
published per-object results once per branch head; sharing them across branches
would remove that repetition and is recorded as the follow-up that could retire the
exception.

## Defects fixed at the root

The boundary-cycle exchange moved its byte away from the 128 KiB boundary, so the
profile never published the 131 072 B state its two frozen consumers read; the
exchange now carries the pair across the boundary and both boundary-cycle rows were
re-collected. The drive-level preparation cached its master under a key the runner
never acquires (so first-use construction stayed inside the gate) and wrote the
owner marker after computing the master identity (so its own entry failed the
identity check); both are fixed, and an access case's producer is now sealed during
preparation with its own container. The transcript oracle gained the one rule a
byte-splice model cannot express: a file that changes representation class is
transcribed from its declared content, and the mismatch report now names the path
and both decompositions.

## Non-passing rows

None. `LAYERFS_SOURCE_DIRTY = true` because another writer's uncommitted
`docs/roadmap/0.1.7` study material is in the tree, outside the source seal's scope;
the seal-covered content is byte-identical to the committed source.
