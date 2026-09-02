# LayerFS 0.1.1 limitations

> **Status:** Released limitations for the `v0.1.1` Developer Preview.

The [0.1.0 limitations](../0.1.0/limitations.md) remain in force: LayerFS is a
Developer Preview, does not claim crash- or power-loss durability at every
acknowledgement, requires independent backups for important data, supports one
live local authority per Store, and provides no cross-host synchronization,
automatic repair, or hostile-code security boundary.

Additional 0.1.1 qualifications:

- namespace benchmark results describe exact synthetic fixtures and the
  retained host; they are not universal throughput guarantees;
- the preferred 100,000-file result of 200 MB/s remains nonbinding and unmet;
- the terminal namespace campaign passes every authorized performance,
  correctness, resource, cleanup, and evidence gate, including the strict
  100-file Create ceiling;
- direct initialization is limited to shapes proven eligible before admission;
  unsupported shapes must use the canonical fallback; and
- prebuilt binaries, crates, and runtime images are official only if listed
  with immutable digests in the release artifact manifest.
