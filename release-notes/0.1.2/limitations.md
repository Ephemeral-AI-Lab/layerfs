# LayerFS 0.1.2 limitations

> **Status:** Prerelease limitations; LayerFS 0.1.2 is not published.

LayerFS remains a Developer Preview. Keep independent copies of important data;
live-process acknowledgement is not a crash- or power-loss durability
guarantee. Keep the SQLite Store outside imported or projected trees.

The active edit-performance claim is not yet admitted. Historical POSIX/FUSE
same-count and count-changing rows cannot support SDK edit latency or memory
claims. Any eventual v0.1.2 claim is limited to the exact 1/10/100/500 MiB
families and environment admitted by issue #20; it cannot be extrapolated to
100 GiB or to a different operation surface.
