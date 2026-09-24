# #241 functional position sweep: first mounted attempt

**Status: FAIL, retained.** This was one functional check, not a timed
performance sample or a cold-cache claim. The full [frozen manifest](../../position-manifest-v1.tsv)
selected `1mib-delete-band00`, byte offset **45,395**, `delete_len=4,096`,
`insert_len=0`, from a 1,048,576-byte pristine file. The attempt was run once
on a real mounted Linux FUSE route. It was **not rerun** for a passing result.

| Identity | Value |
| --- | --- |
| Manifest SHA-256 | `e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a` |
| Harness source commit/tree | `dc4ec52a5a56bc2ad01fd461552033d50b6ef211` / `2105f0ea742afd135d6b3b931341049549068b92` |
| Host test executable SHA-256 | `ddf23d3374eb33d67c04172cef3e3c2fa3fcf92a9b1804462e423ed94c7be995` (debug functional build) |
| Qualified v3 master manifest SHA-256 | `cfe5cbdcb6416e6238c7b5eab0ade0fb8754d03253fa7877a0ebc7985ed2c5ed` |
| Master Store / history SHA-256 before copy | `34b21cc3b51d066451e97e271db2aea9c4cf1f5d9c1b8468a5c8b256ffdd1d65` / `9c796ad09bb6a79e4f78b82372927941c76ad0839c7366dfea6b89436050104b` |
| Linux image ID | `sha256:238c3a69e89a8ab5ef89dfc18e739d520a7b43e6b3b50c5b47721795b86124ce` |

The command was `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked
-p layerfs-sdk --test range_position_sweep mounted_public_sdk_position_sweep
-- --exact --nocapture`, with `LAYERFS_POSITION_CASE_ID=1mib-delete-band00`,
the sealed image/master identities above, the private cursor-key file, and a
fresh output directory. The private Store/history and full first-run output
remain at
`core/target/issue241-position-live-1mib-delete-band00-01/` in the originating
worktree. The compact raw [run](attempts/1mib-delete-band00-01/run.tsv),
[case](attempts/1mib-delete-band00-01/case.tsv), and
[summary](attempts/1mib-delete-band00-01/summary.tsv) receipts are copied here
unchanged; [SHA256SUMS](SHA256SUMS) seals them. The test process exited 101.

The case receipt's terminal error is `WorkspaceApi::unmount` returning
`Failure(Io, unknown=false)`. SDK Sandbox deletion returned `Ok(())`; the
subsequent SDK list was empty. This harness version returned the unmount error
ahead of any inner read/status failure, so the primary mounted verification
outcome is **unknown**. No PASS is inferred from the private Store result.

Read-only diagnosis of the retained private Store/history found two Commits on
the test Branch and no head Commit on the pristine sibling Branch. The new
head is `123b75ee829badb9c6e3185a3065259fbb6e5609540825de4628c03968eadf99a6`;
its parent is the baseline old Commit
`126875bd06183bb32898dfdfc9b1dbf6cf09d923d1d5c9cec1549f83cb096d2a8e`.
The existing independent `verify_edit` example, used only against this
retained state, exited 0 and reported full-file SHA-256
`8452c0ab2cde73ccc4060d4c2b0b3c345bd90b7d281275155d15b102a31a1201`,
matching the byte-granular deletion recipe. It reported 1,044,480 final
bytes, canonical file root
`aa14036ff34ffefe4257c3e45ca64474c0360eebe11839318ec62b7b550996a1`,
55 extents, and the pristine genesis file root/count. Its executable SHA-256
was `50aa15ad91a3311f120680ba356c0a18f6458a81796ec14441291ae2500296fb`.
The [diagnostic stdout](attempts/1mib-delete-band00-01/read-only-diagnostic.stdout)
and its [case input](attempts/1mib-delete-band00-01/read-only-diagnostic-case.txt)
are retained. This establishes published bytes and Branch history in the
private copy, but does not prove the failed mounted coherence/cleanup path.

The next harness identity records a primary case error and unmount result
separately, and captures available daemon logs before SDK Sandbox deletion on
failure. Any next mounted diagnostic uses a **different frozen case ID**;
this first attempt remains FAIL.
