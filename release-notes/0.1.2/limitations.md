# LayerFS 0.1.2 limitations

> **Status:** Released for LayerFS 0.1.2.

LayerFS remains a Developer Preview. Keep independent copies of important data;
live-process acknowledgement is not a crash- or power-loss durability
guarantee. Keep the SQLite Store outside imported or projected trees.

Active SDK evidence and final admission are pinned in the
[selector](sdk-edit-evidence.json). The measured result accepts 20/20/30 ms
Edit/Commit/combined medians and three disclosed Edit-parity exceptions;
Commit size spread is diagnostic, not size independent. Memory observations
are broader-window samples plus native lifetime bounds, not exact-phase or
continuous category ceilings. Sampling cannot exclude every transient swap.
One baseline verifier control error passed on retry; its failed attempt remains
preserved. The subsequent daemon close/remount race was reproduced and fixed
before the release refresh. See [results](benchmark-results.md).

The compatible SQLite Store still exceeds the original 600 MB primary
footprint target. [Issue #18](https://github.com/Ephemeral-AI-Lab/layerfs/issues/18)
is far-future, unscheduled exploration of alternative storage architecture,
not a promised optimization or a near-term release item. Physical packs may
reduce footprint but add publication, recovery, migration and indexing complexity.

Historical POSIX/FUSE rows cannot support SDK latency or memory claims.
Claims are limited to exact 1/10/100/500 MiB families and the recorded
environment; no 100 GiB or different-operation-surface extrapolation is made.
