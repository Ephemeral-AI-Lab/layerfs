# Ephemeral AI FS M6-M10 implementation prompt

Paste the complete block below into Goal mode. The authorization boundaries in this
prompt are intentional: the same goal may continue after the user supplies a required
approval, but credentials already present in the environment never imply permission to
deploy or publish.

```text
Complete every mandatory checklist item and acceptance criterion for Ephemeral AI FS
milestones M6 through M10. Treat the accepted M5 implementation as the baseline; change
M0-M5 code only when a later milestone requires it or a regression proves it incorrect.

Authoritative requirements, in precedence order:
1. docs/spec/*.md
2. docs/testing/correctness-tests.md
3. docs/benchmarks/release-benchmarks.md
4. docs/implementation/implementation-plan.md

If those documents conflict, stop and report the exact conflict instead of choosing a
weaker interpretation. Never weaken, delete, skip, relabel, or replace a mandatory test
to make a gate pass. Never mark a checklist item complete without executable evidence.

Before M6:
- Verify the final M5 gate remains green.
- Preserve all unrelated user changes and untracked performance artifacts.
- You are authorized to create local git commits. Make one scoped baseline commit that
  contains the accepted M5 changes and these reviewed M6-M10 specification/prompt
  clarifications, but excludes unrelated performance artifacts. Then make one separate
  scoped commit for each completed later milestone.
- You are not authorized to push any commit or mutate any remote resource.

Execute these gates in dependency order:

M6 — Cloudflare Durable Object SQLite parity
- Replace every Node-backed Durable Object conformance mock with the pinned
  @cloudflare/vitest-pool-workers faithful local runtime using an actual local
  SQLite-backed Durable Object binding, ctx.storage.sql, and transactionSync.
- Complete the shared storage, filesystem, branch, maintenance, recovery, resource,
  restart, and eviction suites plus the 60-second smoke profile in that runtime.
- Produce and locally exercise the exact deployable preview Worker bundle,
  compatibility date, bindings, and SQLite migration.
- pnpm test:m6 and pnpm validate:m6 must require no Cloudflare account, credentials,
  deployment, network access, or remote resource and must create no external state.
- Do not deploy the preview fixture during M6.

M7 — Node VFS and real FUSE
- Complete the Node VFS bridge and all bounded session, backpressure, durability, and
  error contracts.
- Run the mandatory smoke and restart/remount evidence on privileged Linux with a real
  mounted FUSE filesystem. A shim, mock, or bridge-only test does not count.
- If no suitable runner is available, finish all safe local work, leave M7 unchecked,
  and report the missing runner as a blocker.

M8 — replication
- Implement the bounded host-neutral protocol and all restart, replay, fault, lease,
  policy, and resource cases.
- Run Node-to-Node and Node-to-Durable-Object suites. The Durable Object side may use
  the M6 faithful local runtime; no hosted deployment is required at M8.

M9 — version 0.1 integration candidate
- Run the complete local correctness, fault, architecture, packaging, API, migration,
  resource, smoke, and B01-B09 benchmark gates on their specified reference runners.
- Run the isolated DOFS comparison exactly as specified; never make DOFS an automatic
  fallback or share its database with Ephemeral AI FS.
- A hosted Cloudflare preview is mandatory at M9, not M6. Before deploying, stop and
  request explicit user authorization. Do not ask the user to paste a secret. Ask them
  to authenticate Wrangler themselves or provision CLOUDFLARE_API_TOKEN in the
  environment. After authorization, require EFS_ALLOW_CLOUDFLARE_PREVIEW=1 and verify
  the intended non-production account/environment before deploying. Credentials alone
  are not authorization. Never target production. An unclaimed temporary deployment,
  local Miniflare, or a Node-backed mock does not satisfy the hosted gate.
- Run the unchanged 60-second Durable Object smoke and required release benchmarks on
  the hosted preview, and record non-secret environment and bundle identity evidence.
- Package publication is mandatory for M9. Prepare and verify the exact package set,
  then stop and request explicit publication authorization before changing a registry.
  Never infer publication permission from available registry credentials.

M10 — Ephemeral AI Computer integration
- Use the actual Computer integration target and the exact package versions published
  at M9. A local surrogate does not satisfy the integration gate.
- Before modifying a repository outside the current authorized workspace, stop and
  request explicit authorization for that repository and scope.
- Complete the authoritative Durable Object, local Node/FUSE, replication, shell, Git,
  restart, reconnect, collection, verification, default-selection, isolation, rollback,
  and 60-second end-to-end gates. DOFS remains explicit and isolated.

At each milestone exit:
1. Run the complete milestone gate and all earlier regression gates affected by the
   diff.
2. Launch independent subagents for adversarial correctness, crash-safety/resource,
   and evidence/spec reviews.
3. Fix every substantiated finding and rerun the affected gates.
4. Update API snapshots, normative docs, the implementation-plan checklist, a handoff
   report, and machine-readable evidence with exact commands, commits, environments,
   seeds, fault positions, and measured limits.
5. Create a scoped local milestone commit only after its mandatory gate is green.
6. Do not begin a dependent milestone while its prerequisite is incomplete.

Optional 10 GiB, millions-of-rows, and load-10m diagnostics remain optional unless a
normative requirement explicitly promotes one to a gate. Missing credentials,
infrastructure, external repository access, deployment authorization, or publication
authorization never permits simulated evidence or a checked box. Finish all safe work,
identify the exact remaining gate, and keep the milestone incomplete until the user
provides the missing authority or environment.

The goal is complete only when every mandatory M6-M10 checklist item and acceptance
criterion has passing evidence from its required environment, all adversarial findings
are resolved, the documented default validation budgets remain satisfied, the final
working tree is reproducible, and the implementation plan accurately records the
result.
```
