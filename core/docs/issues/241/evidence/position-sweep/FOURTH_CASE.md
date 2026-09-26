# #241 corrected byte oracle: fourth frozen case

**Status: FAIL, retained.** `1mib-delete-band03` at byte offset **206,873**
ran once as a functional diagnostic, deleting 4,096 bytes from the sealed
1,048,576-byte fixture. It used source commit
`de8d6f1f5b11f0df5f9e35c91ecf5dffdb916507` (tree
`12806084bcc8e136f59bc9a0c4dd94da23c84a2d`), host test executable
SHA-256 `cbb2b41e7a0d50a3e9c0372896c8c17250f8a211d091ccef6ff7d0924a55601a`,
the unchanged 264-position manifest SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`,
qualified v3 master manifest SHA-256
`cfe5cbdcb6416e6238c7b5eab0ade0fb8754d03253fa7877a0ebc7985ed2c5ed`,
and Linux image
`sha256:238c3a69e89a8ab5ef89dfc18e739d520a7b43e6b3b50c5b47721795b86124ce`.
No earlier case was rerun. The [raw case](attempts/1mib-delete-band03-01/case.tsv),
[run](attempts/1mib-delete-band03-01/run.tsv),
[summary](attempts/1mib-delete-band03-01/summary.tsv), and
[daemon log](attempts/1mib-delete-band03-01/daemon.stderr) are sealed in
[SHA256SUMS](SHA256SUMS); the private state remains under
`core/target/issue241-position-diagnostic-1mib-delete-band03-01/` in the
originating worktree. The test process exited 101.

The splice tool's confirmed Exec and explicit Commit completed. The
block-positioned mounted verifier completed with **5 actual FUSE `read`
callbacks** between its before/after status observations. The independent
Store byte SHA-256 matched its prospective prefix/delete/suffix model; the
published C1 file root was valid and differed from pristine. SDK unmount of
that edit Workspace succeeded. The next step, mounting a fresh Workspace
for readback **on the same Sandbox**, returned definite `Busy`, so the case
failed and later old-Commit/pristine-source mounted checks were not reached.
SDK Sandbox deletion returned `Ok(())` and list confirmed absence.

An independent read-only diagnostic of the retained private Store/history
also exited 0: final SHA-256
`242328453842bd551b04a2812b72fc28cd4f60490cb0182112d4710c159410fe`,
published file root
`0b1232b12fb9c64bfe16f3d4e1ecc7c2eed7e3ff0f7eddff39fa6be2745e84d2`,
55 extents, and head Commit
`126f303cf6119d9c72e02e8a54762e986b76a10dad47f6153083ecff4c01757d49`.
Its [stdout](attempts/1mib-delete-band03-01/read-only-diagnostic.stdout) and
[input](attempts/1mib-delete-band03-01/read-only-diagnostic-case.txt) are
retained. This does not promote the mounted case to PASS.

The [daemon lifecycle](../../../../../crates/layerfs-daemon/src/lifecycle.rs)
permits one selected Workspace per Sandbox until closure. The public SDK has
mount/unmount but no separate close; `SandboxApi::delete` is the available
closure boundary. Reusing one Sandbox for successive mounts was therefore a
harness error. The next source identity creates and deletes a fresh SDK
Sandbox for each mounted view and records each unmount/delete/list result.
It will use a **fifth distinct frozen ID**; this fourth outcome remains FAIL.
No latency claim follows from the functional command duration.
