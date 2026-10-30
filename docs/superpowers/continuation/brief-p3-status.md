# P3 — status note (NOT a work brief)

Issue: layerfs-connect#54 (CLI history / push / pull).
Parent: layerfs-connect#14.

## Status

- History half: DONE via P1 (`branch.graph`,
  `branch.changed_paths`, bounded traversal).
- Push/pull half: BLOCKED by R7 #42 (external dependency).
  The blocker is not resolved by any work in this repo.

## Rule

Do NOT implement, simulate, stub, or fake push/pull. No
test doubles that pretend to push or pull, no "demo" path
that looks like a transfer, no local-only transfer dressed
as push/pull. If R7 #42 closes, the commander writes a real
implementation brief at that time; until then this item stays
open and blocked, and the parent Done-when line for push/pull
stays unmet — reported honestly, never papered over.
