# Independent review of the two product fixes

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

A reviewer that wrote none of the code read commits `c127b6d55` (P-A) and
`ea812d3f8` (P-B) with `git show` only; nothing was built or run by it.

## Held

- P-A arithmetic: no serial handed out twice, no range lost, at most one
  allocator call per call, no lock across an allocator call; zero, short and
  overflowing windows refused by the unchanged filter. Concurrent interleavings
  are reasoned from source; no test covers them.
- P-A custody: the early attempt runs on new ports with their own failure
  state, so it cannot become the request's retained Store failure; contention
  is tested on the right error in both arms; no deadlock path (the Store
  writer is a try-lock and the reservation is the mutation's first step).
- Configuration: one decode route, validated there; encode and decode agree;
  every `DaemonLimits` producer is updated.
- P-B: the maintenance failure is read under the owner's short queue-state
  mutex, registry then owner state, never the reverse; both refusals return
  before any entry or bind; duplicate and full-registry answers unchanged; the
  wire reply is `Capacity` at `mount:debt`; no other admission route exists.
- Rules: no retry, wait, sleep, timer, thread, busy handler, test cfg, unsafe,
  hook or dependency; largest touched production file 412 lines.
- Tests: public API, deadline-bounded; no assertion weakened or removed.

## Findings and disposition

| Finding | Disposition |
| --- | --- |
| A non-contention early failure makes the mount terminal, not only the create; four texts said "ends that create" | Behaviour kept (same custody as the exhausted path); comment, port contract, architecture 77 and plan corrected; alternative recorded for the owner |
| No receipt at the committed identities | Covered by the final suites at the final identity |
| The comment giving the reason for the low-water bound was false | Comment corrected; plan corrected; code unchanged |
| P-B's proof covers only a quarantined engine; headroom and Status are not built | FP-26 stays PARTIAL; stated in the plan and the completion record |
| `reserve_serial` port contract text stale | Corrected |
| A poisoned owner now answers at phase `owner submission` instead of through bind; a stopping owner with a retained maintenance failure answers `Capacity` | Recorded; same codes otherwise, no change made |
| The Mount refusal lasts for the daemon's life, since nothing clears the retained maintenance failure | Recorded; follows the existing retained-failure rule |
| For a quarantined engine the Mount answer changes from `Unknown` to `Capacity` once the maintenance turn has run | Recorded |
| With a low-water near the window every Workspace's second create makes a second Store write and about twice the serials are discarded at unmount | Recorded for the choice of a deployed value; the value stays 0 here |
| `set_serial_low_water` is public with no once-only guard; only startup enforces the bound | Recorded; assembly is its only product caller |
| Four tests are controls that also pass on the previous behaviour | Recorded; they are the low-water 0 controls |
| Test creates run `bash` without a bound of their own | Pre-existing pattern; every invocation runs under an external wall stop |
| The LOC wrapper lived only under untracked `core/target/` | Copied to [tools/rx-count.py](tools/rx-count.py) |
