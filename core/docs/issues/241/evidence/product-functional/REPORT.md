# #241 projected range product functional receipts

> Functional Linux evidence only. These commands do not time the registered
> Edit→Commit selection, establish a cold cache state, or qualify 1–500 MiB
> position coverage. All eight attempts below are retained, including failures.

The test harness acquired one closed 64 MiB Store master
([receipt](fixture-64m.json), SHA-256
8ee7bbbe9879395cc346ea84f6851f0911a83b2deb73830e71f6acf45f097cd7)
through an independent writable byte copy per selection. A fresh live
producer made the mounted data.bin/alias namespace. The edited file was
8 KiB for the mounted insert and variants checks; the read-only check used
the pristine fixture file. The master Store hash in the successful receipts is
7e8a68dd7a34d24eb3260496919f586d486cbbb8347f1063cc1e9b84ba883795.
The Linux kernel was 6.12.76-linuxkit aarch64; backing was ext4; the
functional container image was
sha256:9babee938c8c9c7a6678331c98f76a88a40594569b2f6a726c08d3677d836026.
Each receipt pins its source, test, binary and driver hashes. [SHA256SUMS](SHA256SUMS)
seals the copied raw receipts and test stdout/stderr. The original private
Store copies remain under the run's ignored core/target/pair1-evidence/
directories; these tracked records do not archive those database bytes.

| Attempt | Result | Observation |
| --- | --- | --- |
| [kernel insert preflight 01](kernel-insert-preflight-01/result.json) | FAIL before test | Shared product-input hashing assumed the layerfs-api grouping directory had its own Cargo.toml. All case checks were NOT_RUN. Fixed by discovering actual nested manifests in 2a23cd198; the failed receipt remains. |
| [kernel insert 02](kernel-insert-02/result.json) | PASS | Actual Linux STATE/EDIT on an existing writable descriptor inserted 4 KiB at offset 4093. Same descriptor and hard-link alias agreed on length 12,288, mtime, boundary bytes and EOF; status recorded one range edit, 4,096 accepted bytes and zero physical shifted suffix bytes. Explicit Workspace Commit produced the exact canonical bytes. |
| [kernel variants 03](kernel-variants-03/result.json) | FAIL | The append-open descriptor refused EDIT with EOPNOTSUPP (95), contradicting frozen ABI EBADF (9). Test stopped at that assertion; later checks were not credited. |
| [kernel variants 04](kernel-variants-04/result.json) | PASS | After c1308a57f, malformed tail returned EINVAL without mutation; equal-length overwrite succeeded; stale stamp returned ESTALE unchanged; append-open handle returned EBADF; pure delete, alias read/EOF, later ordinary WRITE and canonical Commit all agreed. |
| [kernel read-only 05](kernel-readonly-05/result.json) | PASS | Actual read-only FUSE mount delivered STATE and refused EDIT with EROFS before publication; stamp and accepted-byte counter remained unchanged. |
| [Workspace semantics 01](workspace-semantics-01/result.json) | FAIL at cleanup | Mutation, stale and zero-edit checks had passed, but close_clean returned Busy because test-local OwnedPayload values still held backing pins. No clean-close proof was credited. |
| [Workspace semantics 02](workspace-semantics-02/result.json) | PASS | After test-local payloads were dropped in 83ef85956, projected handle state, insert/delete, stale and empty refusals, exact bytes, counters, Commit and clean close passed. |
| [Workspace notification 03](workspace-notification-03/result.json) | PASS | Injected notifier failure after publication returned WorkspaceError::Coherence with the accepted mutation receipt and revision; status retained the failure and 4 accepted bytes, with zero physical suffix bytes. This is a lower-layer custody test, not a live kernel reply-loss certificate. |

The mounted selections use kernel_range_ioctl_route.py with the closed fixture,
release binaries, the sealed image, one named case and a fresh output path.
The lower-layer selections use projected_range_route.py and the semantics or
notification_failure case. Each receipt records the exact binary hash because
the target filename is reused across source identities. The fixture's
preparation command and source seal are in its own receipt.

The complete functional command walls of passing selections were 0.89–1.00 s,
including local container and service lifecycle. Those are not product
Edit→Commit samples. The cache contract is undeclared here, so no latency gate
is passed. Public SDK Exec caller confirmation, the four registered 1–500 MiB
selections, independent old-Commit verification, and the frozen position sweep
require separate receipts.
