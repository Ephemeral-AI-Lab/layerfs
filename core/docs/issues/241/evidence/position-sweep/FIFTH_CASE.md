# #241 mounted lifecycle diagnostic: fifth frozen case

**Status: PASS for this one functional diagnostic.** `1mib-delete-band04`
at frozen byte offset **283,084** was run once, deleting 4,096 bytes from the
sealed 1,048,576-byte pristine file. No prior case was rerun or relabelled.
The four earlier attempts retain their FAIL status; the complete 264-case
functional sweep has **not** run, and this is not a performance or cold-cache
sample.

| Identity | Value |
| --- | --- |
| Source commit/tree | `99fa97e7eae39d9efb977ae829e69b5bb61a4e63` / `d7edff868367bf062c53567720325a22b662d3fb` |
| Host functional test executable SHA-256 | `930c5070eb1fb325886831a1db3568b45553f27dd67d6c18df6dc2d30ed35590` |
| Manifest SHA-256 | `e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a` |
| Qualified v3 master manifest SHA-256 | `cfe5cbdcb6416e6238c7b5eab0ade0fb8754d03253fa7877a0ebc7985ed2c5ed` |
| Linux image ID | `sha256:238c3a69e89a8ab5ef89dfc18e739d520a7b43e6b3b50c5b47721795b86124ce` |

The exact command was `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked -p layerfs-sdk --test range_position_sweep
mounted_public_sdk_position_sweep -- --exact --nocapture`, with
`LAYERFS_POSITION_CASE_ID=1mib-delete-band04` and the qualified master/image
identities above. The cursor key was read from a private 0600 test file and
is omitted from receipts. The full private state remains at
`core/target/issue241-position-diagnostic-1mib-delete-band04-01/` in the
originating worktree. Compact raw [run](attempts/1mib-delete-band04-01/run.tsv),
[case](attempts/1mib-delete-band04-01/case.tsv), and
[summary](attempts/1mib-delete-band04-01/summary.tsv) receipts are unchanged
and sealed in [SHA256SUMS](SHA256SUMS). The test process exited 0.

The public mounted splice Exec returned its complete canonical PASS line,
then explicit SDK Commit published new Commit
`12a47524bdaa2eaf8aa5eb62455170896a762fe8948d5201d28de0927d455e4cd8`.
The retained baseline old Commit was
`126501ded1d29c660eb3e1bfc18af6eeec21964e7270d9b77956b5d0bbae37e36f`.
The independent copied-Store C1 reader found **1,044,480 final bytes** with
full SHA-256
`9be2b5bd95a4cef4c1c78b2e13160345ad2c400a5cdecc5e69a5678e779f7c63`,
matching the prospective byte model. It validated file root
`b0fcdb30bcc3466701d5349d6102da348db66565ecc555c2ed69748a3775036e`
and 55 extents. The old Commit's payload retained the pinned pristine root,
length and count. Fresh new-Branch, old-Commit and pristine-source Branch
views each passed bounded mounted head/seam/tail, `stat` size and EOF checks.
The pristine source Branch head remained absent. The edit view's bounded
mounted verifier caused **5 actual FUSE `read` callbacks**.

Each of the edit, fresh, old and source views used a different public SDK
Sandbox. The `with_mount` path returned success only after SDK unmount; all
four SDK Sandbox deletions returned `Ok(())`, each ID was absent from the SDK
list, and the final list was empty. This attempt's raw receipt records the
delete/list results; a later test-only logging refinement records unmount
results as explicit lines for the final campaign. That refinement was not part
of this PASS source identity and has not had its own live sweep.
