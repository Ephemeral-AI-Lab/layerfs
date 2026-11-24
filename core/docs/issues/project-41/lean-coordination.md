# Lean model coordination

## Dependency

The Lean model (`formal/lean/gc-admission/`, issue #769 / #74) must follow the **final
`p4-closure` behavior**, not only the p4-integration tree.

`p4-closure` (subagent `5bcb81cc`) adds:

| Gap | Behavior the Lean model must include |
| --- | --- |
| G1 | the optional `home` path on the Store record's `machines.<m>` entries; the bounded, ordered cross-home `PlaceView` merge; an unreachable home contributes no proof |
| G2 | the `store.repair_mount_holders` `archive` transition (+ `--prune`): export done rows to `gc-audit.jsonl`, no automatic delete |
| G4 | `preview` writes a retained `gc_intent` done row (`action = "preview"`) |

## Status

- The Lean plan (`3a7650ad`, `p4-lean-proof`) is in progress; it records this dependency
  and is **blocked on `5bcb81cc`** for the final `State`/`Transitions`/`Invariants`.
- When `p4-closure` is final and, if merged, the PR #73 tree is rebuilt, update the Lean
  model (the `machines.<m>.home` state, the `archive` transition, the preview done row)
  before starting the proofs.
- The invariant set (8) and the `step_preserves` shape stay; only the state/transitions
  gain the new fields/actions.
