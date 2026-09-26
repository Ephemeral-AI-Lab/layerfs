# #241 bounded mounted verifier diagnostic: second frozen case

**Status: FAIL, retained.** This was one new functional diagnostic, not a
performance sample. The frozen manifest selected `1mib-delete-band01` at
byte offset **96,547**, deleting 4,096 bytes from a 1,048,576-byte pristine
file. The attempt used source commit
`d037e501ea74f02c021dbe4a0e25531c9a1a685d` (tree
`1b5ed14abd044c0cd5722f3198d8aac0fc902a89`), host test executable
SHA-256 `41be6b63a4cec35c79e0f8594b467a0019a437c10e331f5fc29219b6633b4bb8`,
the same frozen manifest SHA-256
`e12509411e225e6eea497dc08e3cc0730f1c2ab7ce3f104cfc232eb16c7f2b6a`,
qualified 1 MiB v3 master manifest SHA-256
`cfe5cbdcb6416e6238c7b5eab0ade0fb8754d03253fa7877a0ebc7985ed2c5ed`,
and immutable Linux image
`sha256:238c3a69e89a8ab5ef89dfc18e739d520a7b43e6b3b50c5b47721795b86124ce`.
The original case was never rerun. This second case was run once and was not
rerun for a pass.

The full private state and first output remain at
`core/target/issue241-position-diagnostic-1mib-delete-band01-01/` in the
originating worktree. Compact raw [run](attempts/1mib-delete-band01-01/run.tsv),
[case](attempts/1mib-delete-band01-01/case.tsv),
[summary](attempts/1mib-delete-band01-01/summary.tsv), and
[daemon logs](attempts/1mib-delete-band01-01/daemon.stderr) were copied
unchanged and sealed in [SHA256SUMS](SHA256SUMS).

The aggregated error now identifies the primary failure: the public SDK Exec
for the mounted verifier returned `Failure(Unknown, unknown=true)`, followed
by `WorkspaceApi::unmount` returning `Failure(Io, unknown=false)`. The verifier
was a single shell command with three `dd bs=1 count=4096` window reads and a
one-byte EOF read. From that command, it requests 12,288 one-byte `dd` input
records before EOF; this is **source-derived user-space I/O count**, not a
measured FUSE callback count. The Exec's five-second silent-progress limit is
a plausible explanation for `Unknown`, but callback count and daemon liveness
were unavailable after the error, so the precise cause is not proved. The
daemon log capture succeeded before SDK Sandbox deletion; it contains only
`sandbox control ready 0.0.0.0:23456`. SDK deletion returned `Ok(())`, and
SDK Sandbox list was empty.

The retained Store/history were inspected independently, without repeating
the mounted case. The existing `verify_edit` example exited 0 and verified
the published 1,044,480-byte file's full SHA-256
`e68a3ba600a2fe6140b63787da01535552a8a691f99e3d9102a6e1f53dee1302`.
Its canonical file root was
`fcb0c85637ce6b1e15f200c0c9aaf0d48992006fd47b75501fe3985dec407067`,
with 55 extents; the pristine genesis root/count also matched. The Branch
head was `1232bc9760498c710eec0b140400e7a60cbb3f2b72012fd016ef02ca820d76e97f`.
The [diagnostic output](attempts/1mib-delete-band01-01/read-only-diagnostic.stdout)
and [input](attempts/1mib-delete-band01-01/read-only-diagnostic-case.txt)
are retained. This confirms the product published the intended bytes but does
not turn the failed mounted verifier into a functional PASS.

Before any third mounted check, the prospective verifier changes to a
block-positioned `dd bs=4096`: at most two 4 KiB file blocks per head, seam,
or tail window, then a bounded in-pipe trim; EOF uses one block. That command
shape was checked against the sealed Alpine image on an ordinary 16-byte file,
not treated as mounted proof. The next check uses a different frozen ID and
records actual read callback delta when status remains available. Both earlier
FAILs remain on record.
