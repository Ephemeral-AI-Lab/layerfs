# layerfs-connect integration (REQUIRES SEPARATE USER APPROVAL)

## Scope boundary (read first)

This brief is NOT authorized by the Area P pipeline alone.
Work in the layerfs-connect repo starts only after the user
explicitly approves it. Until then this file is a plan, not
a work order. The product changes stay in the layerfs repo;
the connect repo holds no product code (its @layerfs/*
dependencies are `link:` deps to a layerfs worktree).

## Integration spec (from P9 v2, §Connect integration / §11.6)

1. SSE: do NOT extend the existing RunEvent union. On the
   same `/api/events` endpoint, carry a parallel HostEvent
   union: ProjectHostEvent (`project.state` /
   `project.events-error`) and InitHostEvent (`init.progress`).
2. Invalidation (pinned): when the host receives a P9 event,
   it re-fetches the FULL `projects.state()` and emits that;
   the renderer replaces its state wholesale. A P9 snapshot
   must NEVER be spliced directly into ProjectState.
3. Subscription ownership: the host holds one P9 subscription
   per project, with a pump. Renderer SSE reconnects must NOT
   disturb host subscriptions.
4. Polling points to replace (named): live-tab `POLL_MS`
   2000, run-project-tabs progress tick, run.js `get_init`
   loop. The run-call bridge is mandatory for separate
   demo-agent clients (they run in their own subprocess with
   their own client, invisible to the host queue): use
   RunManager's existing finished-call events as a refresh
   fallback.
5. Explicitly OUT of this step: traces.js 200 ms tail removal
   (needs P9-W1 cross-process watch — a separate slice);
   macOS fine-grained init phases (need P9-I1).
6. P11 storage stats and P12 summary are natural fits for
   the host API contract; wire them only after their product
   slices land, under the same invalidation rule.
7. Connect-side tests for every replaced polling point and
   for the invalidation rule (stale snapshot must never reach
   the renderer).

## Preconditions

- P9 product slice landed (done, PR #9).
- The connect checkout is read-only until approval; the
  layerfs worktree it links must be at a tip containing the
  P9 slice.
- P2 constructor options replace LAYERFS_* env injection in
  host config (see host/config.js + settings.json).
