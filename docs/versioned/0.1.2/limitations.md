# LayerFS 0.1.2 limitations

> **Status:** Released limitations for the `v0.1.2` Developer Preview.

The [0.1.1 limitations](../0.1.1/limitations.md) remain in force: LayerFS is a
Developer Preview, does not claim crash- or power-loss durability at every
acknowledgement, requires independent backups for important data, supports one
live local authority per Store, and provides no cross-host synchronization,
automatic repair, or hostile-code security boundary.

Additional 0.1.2 qualifications:

- edit and Store measurements describe exact synthetic fixtures on one retained
  MacBook/Docker Desktop/Linux-FUSE environment, not universal guarantees;
- owner-side range-edit batches must be non-empty and target one Workspace and
  one regular file; namespace and metadata changes remain filesystem operations;
- a newly created FUSE file uses direct I/O until its create handle closes;
  mmap on that handle returns `ENODEV`, while close/reopen restores the ordinary
  retained-cache and read-only mmap behavior;
- the retained SQLite Store misses the 600 MB primary footprint goal at
  662,831,104 median bytes; authenticated physical packs are deferred to issue
  #18;
- the metadata-cardinality verifier is an explicitly tolerated 63.356-second
  result under the frozen 60/66-second policy; and
- prebuilt executables, crates.io packages, and runtime images are not
  published for v0.1.2.
