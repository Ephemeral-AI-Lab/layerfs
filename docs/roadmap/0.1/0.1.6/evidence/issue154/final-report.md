# #154 final report — all 33 regular cases and 3 extensions terminal at seed 1

Identity (one chain for the whole matrix): source seal
`86f14b2d68ece2ae368f8aec29070520505a61c7aa69b445414b64561640e954`, product seal
`970964e9af43a8bf57f0d7bec70736a94171f7beb62fc3378ea5cc4797500ebd` (**unchanged**:
only the harness, the oracle and the producer declarations moved), compilation seal
`679b17f0e17636ae419144edb7377be03dd1709ac362d72ea0dff61d3b89306b`, dependency seal
`374f4dfa08f98ced734e540f62dd4a8b0edb5a88c090c051ae704fbf7f67faf1`, image
`layerfs-bench-infra:86f14b2d68ece2ae` = `sha256:af6a400356e36b2f58e5eb9df07a291d8567cb975adbb183ed5d76b0c5e7268d`,
host binary `fcaa14d8decf96a6247d95a037e83eea7322aeb66a51d418acbee2e21fdd6aa7`,
harness identity `42ace192351e57ee429e09dbb6480dfffb43e65045c154f72131a99ac7626ada`.
One performance and one separate verification invocation per case and mode, seed 1,
fresh append-only outputs; superseded attempts retained on disk.

## Result

| verdict | rows |
| --- | ---: |
| PASS inside the 15 s target | **29** |
| declared ≤25 s exception (mixed / workspace / branch `…500mb-30000-k100-v1` verify) | **6** |
| performance `N/A` (the six declared verify-only access cases) | **6** |
| **TIMEOUT** — `v016-branch-mixed-500mb-30000-k100-v1` verification | **1** |

Extensions: `v016-mixed-exhaustive-100mb-5000-k100-v1` verify PASS 15.42 s / 120 s;
`v016-mixed-exhaustive-500mb-30000-k100-v1` verify **PASS 62.68 s** / 300 s (was a
298.28 s TIMEOUT in L8); `v016-workspace-four-100mb-5000-k100-v1` perf PASS 8.04 s /
verify PASS 9.24 s. Performance stays `N/A` for the two verify-only cases.

## The ten cases that did not exist before

`v016-history-namespace-inode-k{10,100}-v1` — perf PASS 1.97 s / 3.28 s, verify
PASS 4.10 s / 9.80 s. 100 Created commits, 101 retained roots, all parent edges,
states `[0, 1, 2, 49, 50, 94, 95, 99, 100]`, envelope S+2 names and S+8192 bytes.

`v016-branch-convergent-content-v1`, `v016-branch-fork-descendant-v1` — perf PASS
2.28 s / 2.09 s, verify PASS 2.45 s / 2.09 s. 30/40 Created commits, ancestry
15/20, convergent children sharing one first-medium-file content root, descendant
B/C branch-salted, every sibling head preserved.

`v016-access-{boundary-before,boundary-after,inode-before,inode-after,fork-point,divergent-head}-v1`
— performance `N/A`, verification PASS 1.69–2.10 s. Each mounts one selected
retained state of a sealed producer, creates no commits, and compares its two
declared read passes byte for byte with the producer's own declaration.

## Non-passing rows, stated plainly

* `v016-branch-mixed-500mb-30000-k100-v1` **verification TIMEOUT at 22.99 s**: the
  declared complete-command window stopped the host command after the 210-commit
  replay, both published heads and two of the three complete branch-state proofs.
  Re-taken once on the same identity (22.92 s, then 22.99 s; host load 8.4–8.8 on
  14 CPUs) and retained as `TIMEOUT`. L10 measured the same row at 22.83 s PASS, so
  it sits on the ceiling; no oracle, coverage, limit or workload was changed to
  move it. **Escalation: a declared larger verification exception with this measured
  wall, or `NOT_RUN`.**
* `LAYERFS_SOURCE_DIRTY = true`: another writer's uncommitted
  `docs/roadmap/0.1.7` study material is in the tree. It is outside the source
  seal's scope; the seal-covered content is byte-identical to the committed source.

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
