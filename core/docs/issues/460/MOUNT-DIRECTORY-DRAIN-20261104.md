# Remove a detached private mount directory before joining

> **Status:** Research; informative and not a product contract.

Normal unmount could wait for the five-second Join observation deadline even
after plain detach succeeded and active requests reached zero. Receive loops
remained in `fuse_dev_do_read`; the existing single bound fusectl abort then
completed the connection. A copied mount namespace reproduces this without
product hooks.

The previous order waited for connection termination before removing the empty
private mount directory. Linux `vfs_rmdir` detaches mounts on the removed dentry
in other namespaces too. In the copied-namespace case, waiting until after Join
kept the connection reference that Join needed to end. The correction does not
depend on identifying the process or subsystem that retained the namespace;
holder investigations are tracked in
[#460](https://github.com/Ephemeral-AI-Lab/layerfs/issues/460).

## Change and boundaries

After acknowledged normal detach and zero active requests, attempt the existing
private-directory removal once, before observing receive-loop joins. Preserve
the original removal result in the session through every stop and hand it to
the final drain receipt. Forced teardown that has not detached still quiesces
before its one detach and removes the directory during release.

Busy detach remains reversible and removes nothing. Every created loop must
still join, the owner and watcher must return, active and retained requests
must reach zero, and request-owner release must succeed. Failed removal is
neither retried nor relabeled clean; the original bounded Join/abort fallback
remains available. The change is limited to mount-retirement ordering and
removal-result custody.

## Verification

- Native Linux ARM64, real `/dev/fuse`, Rust 1.85.1, locked dependencies and the
  repository ARM64 flags: `stalled_unmount`, seven tests, all passed in 21.45 s
  under a 100-second invocation timeout after a separate compilation.
- The former automatic-abort copied-namespace test now requires a clean receipt
  with no abort, no mount/directory/control entry, and a still-live copied
  namespace that cannot access the removed directory.
- A calling-thread-only seccomp denial stages genuine directory-removal
  failure. Existing denied/short abort, second drain stop and later Revoke/Close
  custody cases remain covered. A new case preserves the original `EACCES`
  through a successful abort fallback and rejects a clean receipt.
- Formatting, product-boundary and unchanged authorized fuser provenance checks
  passed. No tests were deleted; fallback tests retain their original assertions.

Downstream integration, performance, package and recovery qualification are
tracked separately from this component proof.
