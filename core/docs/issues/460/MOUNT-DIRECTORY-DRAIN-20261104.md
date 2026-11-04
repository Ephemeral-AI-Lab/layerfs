# Remove a detached private mount directory before joining

Normal unmount could wait for the five-second Join observation deadline even
after plain detach succeeded and active requests reached zero. Receive loops
remained in `fuse_dev_do_read`; the existing single bound fusectl abort then
completed the connection. A copied mount namespace reproduces this without
product hooks. Camp tracks its consumer investigation in
[issue 118](https://github.com/cofy-x/camp/issues/118).

The previous order waited for connection termination before removing the empty
private mount directory. Linux `vfs_rmdir` detaches mounts on the removed dentry
in other namespaces too. Leaving that removal until after Join therefore kept
the very connection reference that Join needed to end. The precise external
holder on the observed OrbStack engine remains unidentified; this does not
attribute an engine defect.

## Change and boundaries

After acknowledged normal detach and zero active requests, attempt the existing
private-directory removal once, before observing receive-loop joins. Preserve
the original removal result in the session through every stop and hand it to
the final drain receipt. Forced teardown that has not detached still quiesces
before its one detach and removes the directory during release.

Busy detach remains reversible and removes nothing. Every created loop must
still join, the owner and watcher must return, active and retained requests
must reach zero, and request-owner release must succeed. No timeout, abort
authority, wire API, third-party source, dependency, Store policy, commit or
recovery boundary changes. Failed removal is neither retried nor relabeled
clean; the original bounded Join/abort fallback remains available.

## Verification

- Native Linux ARM64, real `/dev/fuse`, Rust 1.85.1, locked dependencies and the
  repository ARM64 flags: `stalled_unmount`, seven tests, all passed in 21.45 s
  under a 100-second invocation timeout after a separate compilation.
- The copied-namespace test requires a clean receipt with no abort, absent
  mount/directory/control entry, and a still-live copied namespace that can no
  longer access that directory.
- A calling-thread-only seccomp denial stages genuine directory-removal
  failure. Existing denied/short abort, second drain stop and later Revoke/Close
  custody cases remain covered. A new case preserves the original `EACCES`
  through a successful abort fallback and rejects a clean receipt.
- Formatting, product-boundary and unchanged authorized fuser provenance
  checks passed. Consumer image/performance/recovery qualification belongs to
  Camp and is not implied by this component proof.

No tests were deleted. The former copied-namespace automatic-abort test now
checks successful retirement without abort; the fallback tests retain their
original assertions and stage failed removal through the kernel.
