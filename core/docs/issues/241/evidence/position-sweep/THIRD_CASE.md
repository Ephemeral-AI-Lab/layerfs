# #241 block-positioned mounted verifier: third frozen case

**Status: FAIL, retained.** A third, distinct functional diagnostic ran once:
`1mib-delete-band02` at frozen byte offset **135,511**, deleting 4,096 bytes
from the 1,048,576-byte fixture. It used source commit
`f0bb3c0810b7bc86b2684e7b5bd3efdc2a02c9f7` (tree
`8c7c4807296e8c55eaf9c8b9f55dc6eab96f8d97`), test executable SHA-256
`93c506cde8571bf1bb0d5fb054c7be3d55d6be1a5cc0c2dd1bdee38d0f037f62`,
the unchanged manifest SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`,
qualified v3 master manifest SHA-256
`cfe5cbdcb6416e6238c7b5eab0ade0fb8754d03253fa7877a0ebc7985ed2c5ed`,
and Linux image
`sha256:238c3a69e89a8ab5ef89dfc18e739d520a7b43e6b3b50c5b47721795b86124ce`.
The [raw case](attempts/1mib-delete-band02-01/case.tsv),
[run](attempts/1mib-delete-band02-01/run.tsv),
[summary](attempts/1mib-delete-band02-01/summary.tsv), and captured
[daemon log](attempts/1mib-delete-band02-01/daemon.stderr) are preserved with
checksums in [SHA256SUMS](SHA256SUMS). The full private copy remains at
`core/target/issue241-position-diagnostic-1mib-delete-band02-01/` in the
originating worktree. The test process exited 101. No earlier case was rerun.

The new block-positioned mounted verifier and Workspace unmount both
completed; the test then failed in its **own canonical oracle**. Its full
byte digest from the copied Store matched the independent byte model:
`b449fc658a5b0e5ec45e114812635d54150dd21f94faacb9ed0cd9945058213f`,
with 1,044,480 bytes. The stored file root/count were
`e438a78f1a7bd56385aec6b4bed0a8c615b0ed5dd49bb5254ca19a61a3e85aa9` /
55 extents, whereas a **fresh full-file reconstruction** of the same bytes
gave `a19785724d7631bb51c0a867f2b2ff4d8a4b06317c48239f39fcd75cba940b09` /
54 extents. That equality was an incorrect qualification rule: Core's
[`edit_transitions.rs`](../../../../../crates/layerfs-content/tests/edit_transitions.rs)
explicitly states that localized chunked edits retain old chunk boundaries
and are judged by an independent byte model, because a fresh constructor
re-chunks the whole file. The difference is not, by itself, a product defect.
This original attempt remains FAIL because its declared oracle rejected it.
SDK Sandbox deletion returned `Ok(())` and list confirmed absence.

The prospective next harness identity derives only the expected byte SHA-256
from pristine prefix, replacement and suffix; it checks the new C1 root and
extent mapping are valid and changed from pristine, and records the observed
root/count without demanding fresh-construction equality. It also writes the
mounted verifier's `read` callback difference into the case receipt before
later Store/history checks. The third run calculated that difference but did
not retain it before the oracle failure, so its actual callback count is
unavailable. The next mounted check uses a **fourth distinct frozen ID**.
This is functional proof work, with no position-latency claim.
