# LayerFS 0.1.2 limitations

> **Status:** Prerelease limitations; LayerFS 0.1.2 is not published.

LayerFS remains a Developer Preview. Keep independent copies of important data;
live-process acknowledgement is not a crash- or power-loss durability
guarantee. Keep the SQLite Store outside imported or projected trees.

Active SDK evidence and final admission are pinned in the
[selector](sdk-edit-evidence.json). The measured result accepts 20/20/30 ms
Edit/Commit/combined medians and three disclosed Edit-parity exceptions;
Commit size spread is diagnostic, not size independent. Memory observations
are broader-window samples plus native lifetime bounds, not exact-phase or
continuous category ceilings. Sampling cannot exclude every transient swap.
One baseline verifier control error passed on retry; its cause remains
unproven and its failed attempt is preserved. See [results](benchmark-results.md).

Historical POSIX/FUSE rows cannot support SDK latency or memory claims.
Claims are limited to exact 1/10/100/500 MiB families and the recorded
environment; no 100 GiB or different-operation-surface extrapolation is made.
