# Linux timestamp boundary and Docker verification

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

**Latest owner acceptance:** "docker verification is enough". The native Docker
proofs already performed are accepted verification of the fuser correction.
The remaining endpoint result below is a documented Linux platform limitation;
it does not require a QEMU/custom-kernel campaign to verify that dependency fix.
Its recorded FAIL remains unchanged. S8's unfinished product implementation and
mounted product qualification remain required; this acceptance advances no checkbox.
The earlier source/build investigation below remains historical evidence.

The owner asked what the remaining failure was, requested a fix, and selected
Docker for FUSE verification. The [S7–S9 continuation prompt](HANDOFF-S7-S9.md)
records that direction. All previously qualified mounted FUSE checks already ran
in owned Linux ARM64 Docker containers with `/dev/fuse` and `CAP_SYS_ADMIN`.
No QEMU VM was booted and no alternative Docker backend was created.

The [fuser0.18.0 correction](FUSER-REGISTRY-PATCH-20261006.md) fixes negative
fractional conversion and signed-minimum overflow. Four unit tests and five exact
public parser/reply cases pass. Actual mounted negative-fraction and whole-minimum
cases also pass. This dependency correction is implemented and committed.

The remaining mounted case requests `(i64::MIN,200000000)` and receives
`(i64::MIN,0)`, with normal unmount/join. This remains the original required native
precision failure. In the pinned Linux6.12.76,
[timestamp_truncate](https://github.com/gregkh/linux/blob/v6.12.76/fs/inode.c)
clears nanoseconds at the filesystem's minimum/maximum before checking granularity;
[notify_change](https://github.com/gregkh/linux/blob/v6.12.76/fs/attr.c)
applies that conversion before setattr. The
[default superblock range](https://github.com/gregkh/linux/blob/v6.12.76/fs/super.c)
is TIME64_MIN through TIME64_MAX. The
[FUSE initialization handler](https://github.com/gregkh/linux/blob/v6.12.76/fs/fuse/inode.c)
accepts timestamp granularity, while the owning fuser config already defaults to1ns.
Changing that granularity cannot restore a fraction cleared by the earlier rule.

A read of upstream revision `67f0943b394d920b6c142aad8c6af94340342ae7` confirmed the
[same boundary rule](https://github.com/torvalds/linux/blob/67f0943b394d920b6c142aad8c6af94340342ae7/fs/inode.c).
This is Linux's current saturation behavior, not another unresolved fuser parser
conversion. The parser/reply fixture preserves200M nanoseconds when those bytes
actually reach it. The exact ordinary syscall path loses them before fuser.
No unchanged native case was resampled in this investigation.

Docker containers share their backend's Linux kernel. Building a container image
can change fuser/userspace tooling; it cannot replace this VFS rule in the running
backend. A platform correction needs its own explicit source/build/native evidence
and Docker qualification while preserving the four unrelated containers. No
shared-backend restart, kernel change, direct-wire product substitute, guessed
fraction or smaller timestamp contract was applied. S0/S8 remain incomplete.
Useful independent S7/S8/S9 implementation remains ready.

The investigation downloaded the official checksum-verified Linux6.12.76 source
under ignored `core/target/cluster2-307/linux-timestamp-fix/` and inspected build
tools. Its Docker build-tools image failed at the Debian package index download
with HTTP500/unexpected EOF; no kernel was built or modified and no new native
proof ran. The exact failure/build receipt and source identities are retained in
[the all-state handoff checks](checks/s7-s9-all-state-handoff/). Downloaded source,
package caches and build artifacts remain ignored build inputs, rather than
new product source or qualification. The source/image/failed receipts are not
relabelled as a successful fix.
