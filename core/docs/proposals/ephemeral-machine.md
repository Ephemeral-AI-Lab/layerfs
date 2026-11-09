# Ephemeral Machine Proposal

Status: proposal, not committed  
Scope: LayerFS 3.0 cloud execution; first implementation target E2B  
Related: `layerfs-project#23/#24/#25/#29`, `layerfs#547`, `layerfs#553/#554`, `compute-plane-2.0.md`, `control-plane-2.0-contract.md`

## 1. Definition

An **Ephemeral Machine** is:

```text
ephemeral execution lease
+ durable Cloud Store
```

It is **not** a durable machine:

```text
no authoritative local disk
pause/resume optional, never a correctness dependency
no assumption that the sandbox outlives a tool call
```

The durable source of truth is the Cloud Store. The E2B sandbox is disposable.
Terms are defined in §2; the 2.0 machine they are contrasted with is specified in
`control-plane-2.0-contract.md`.

## 2. Terminology

| Term | Meaning | In 3.0 phase 1 |
|---|---|---|
| machine | Durable execution host with a kernel and local filesystem; authority for its local Workspace state. | not used (an E2B sandbox is not a machine) |
| worker | Long-lived per-machine process that holds the Store lease/cache and executes kernel-bound work. | no |
| worker_server | The TCP accept loop inside `worker`; machine-local. | no equivalent |
| endpoint | Machine-local address (loopback + secret) through which control reaches `worker_server`. | none (§5) |
| ephemeral machine | Ephemeral execution target bound to a Cloud Store lease; a per-call execution unit, not durable. | yes |
| lease | Time-bounded, idempotent grant to execute work against the Cloud Store. | one per tool call |
| machine-run / lease runner | One-shot process in the sandbox, started through the E2B command API: mount branch, execute tool call, commit, unmount, exit. | yes |
| command API | E2B `create` / `commands.run` / `kill`; the phase-1 transport and broker. | yes |
| Cloud Store | Durable authority: PostgreSQL or a Store gateway. Holds branches and commits. | yes |

`worker`, `worker_server` and `endpoint` are 2.0 control-plane concepts. They are
listed only so the 3.0 model is not mistaken for them.

## 3. Invariants

1. Cloud Store is the only durable authority.
2. No remote SQLite file. Cloud Store is PostgreSQL or a Store gateway.
3. Workspace is local and disposable; it is recreated from a branch.
4. One tool call has at most one commit.
5. A commit is idempotent by `tool_call_id`.
6. Commit success before lease expiry is durable.
7. Expiry before commit is `Unknown`; retry the tool call, never replay the commit.
8. External side effects of a tool call are not transactional and are never replayed blindly.
9. A lease has a TTL; expiry is normal, not an error.
10. The control plane does not own data-plane state.
11. Phase 1 has no worker, no `worker_server`, and no machine-local endpoint.
12. Pause/setTimeout may extend a lease, but correctness must not depend on pause.

## 4. Architecture

```text
Durable control plane
  Agent / SDK
  layerfs control
    branch
    lease
    commit policy

Ephemeral E2B machine
  E2B sandbox lease
  machine-run one-shot process (not worker / worker_server)
  Workspace (overlay + FUSE)
  Remote Store client
  local cache only

Durable Cloud Store
  PostgreSQL / Store gateway
  base branch + commits
  object storage / backup
```

Diagram: https://app.excalidraw.com/s/9Zq6XgRe98z/9OnpNbh7jpv

## 4.1 Reference diagrams

- LayerFS 2.0 local machine architecture: https://app.excalidraw.com/s/9Zq6XgRe98z/7WOIOh2EsS4
- LayerFS 2.0 local machine sequence: https://app.excalidraw.com/s/9Zq6XgRe98z/8wNF6eq7Os7
- LayerFS 3.0 ephemeral machine architecture: https://app.excalidraw.com/s/9Zq6XgRe98z/9OnpNbh7jpv
- LayerFS 3.0 ephemeral machine sequence: https://app.excalidraw.com/s/9Zq6XgRe98z/1OLYoujxArN

## 5. Execution model

First phase uses **one lease per tool call**, not one long-lived machine:

```text
1. control -> E2B: create sandbox (idempotencyKey = tool_call_id)
2. E2B: start the machine-run helper from the template
3. machine-run: mount branch from Cloud Store
4. machine-run: execute the tool call
5. machine-run: workspace.commit(expected head, tool_call_id)
6. machine-run: unmount
7. control -> E2B: kill sandbox
```

`machine-run` is a one-shot process invoked by the E2B command API. It is not
`worker`, not `worker_server`, and it opens no listener. Phase 1 has no
machine-local worker endpoint: control talks to the provider API and never dials
the sandbox. A later optimization may reuse one sandbox for several tool calls
within its TTL, but correctness must not depend on reuse.

## 6. 2.0 vs 3.0

| Aspect | 2.0 machine | 3.0 ephemeral machine (phase 1) |
|---|---|---|
| process lifetime | one long-lived `worker` per machine | one `machine-run` per tool call; sandbox killed after commit |
| transport | TCP to machine-local endpoint, framed, protocol 3 | E2B command API (`create` / `commands.run` / `kill`); no inbound socket |
| endpoint | machine-local loopback + secret | none; control only calls the provider API |
| Store authority | worker holds the Store lease/cache; local machine is durable | Cloud Store only (PostgreSQL / gateway); sandbox cache is replaceable |
| timeout behavior | explicit close/cancel; liveness != progress; `Unknown` after send | lease TTL (300 s); setTimeout/pause optional extension; timeout after commit safe, before commit `Unknown`; retry tool call, never the commit |
| reuse | worker reused across calls on its machine | correctness independent of reuse; same-TTL sandbox reuse is a later optimization |

### 6.1 Worker distinction

| | 2.0 local machine | 3.0 ephemeral machine |
|---|---|---|
| worker | long-lived process, one per machine | none |
| worker_server | TCP accept loop inside worker | none |
| endpoint | machine-local loopback + secret | none; control uses the E2B command API |
| execution unit | worker handles many calls | `machine-run` handles one tool call |
| Store lease | held by worker | held by Cloud Store; sandbox has only a replaceable cache |
| pause | not a correctness mechanism | optional runtime extension, never a correctness dependency |

## 7. Transport decision

First phase:

```text
control -> E2B API (create, commands.run, kill)
no inbound raw TCP
no worker endpoint
no WSS tunnel
```

The command API carries the request and streams progress, and acts as the broker
between control and `machine-run`. This avoids the E2B raw-TCP limitation and
matches the ephemeral lease model.

A persistent worker reached over WSS / an outbound channel is a later
optimization for high call frequency. It is not a 3.0 first-phase requirement and
nothing in the commit protocol may depend on it.

## 8. Runtime extension (optional)

The default lease is 5 minutes. If a single tool call legitimately needs more time,
the lease may be extended **when the plan allows it**:

```text
setTimeout() before the deadline
or
pause() + resume()
```

These are runtime-extension tools, not correctness mechanisms.

Rules:

1. Commit before pausing whenever possible.
2. Pause drops external command streams; the `machine-run` process may survive in memory if the provider keeps memory.
3. After resume, reconnect the command stream and verify the workspace/process.
4. If the stream cannot be resumed, the result is `Unknown`; retry the tool call.
5. Never use pause as a substitute for commit.
6. Never make correctness depend on pause.
7. If the plan has a hard cap that pause cannot extend, a longer single call is unsupported; split it into committed tool calls.

## 9. Store contract

Allowed:

```text
PostgreSQL
Store gateway / HTTP Store service
object storage for packs / backups
```

Forbidden:

```text
remote SQLite file
SQLite over NFS / volume
local SQLite as the only authority
```

SQLite remains valid only on one kernel with a local filesystem, as in 2.0.

Every read/write inside E2B goes through the remote Store client. Local cache is
replaceable and never authoritative.

## 10. Commit protocol

Request fields:

```text
home_id
project
branch
tool_call_id
expected_head
message
```

Rules:

```text
commit idempotency key = home/project/branch/tool_call_id
if current head != expected_head -> Conflict; remount latest; retry tool call
if no file changes -> no-op commit
empty commit is not a checkpoint
```

If the command succeeds but the reply is lost:

```text
query commit by idempotency key
do not create a second commit
```

If the lease expires before commit:

```text
Unknown
retry the tool call
do not replay the commit
```

## 11. E2B-specific rules

```text
template: layerfs machine-run + FUSE + Store client
timeout: 300s for phase 1
onTimeout: kill default is acceptable after commit; setTimeout/pause is optional for extension
pause/resume is optional, not a correctness dependency
no local SQLite
no worker, no worker_server, no public endpoint
E2B API key only in control
Store credentials scoped to the lease
```

Blocking checks before implementation:

```text
/dev/fuse present
namespaces available
cgroup v2 if backend requires it
ext4/xfs/btrfs if any local SQLite is ever used
mount/unmount works inside the E2B kernel
```

If FUSE is unavailable, this design does not work for bash tool calls; do not
silently fall back to a different filesystem.

## 12. Failure matrix

| Event | Result |
|---|---|
| E2B create fails | no commit; retry tool call |
| command starts, lease expires before commit | `Unknown`; retry tool call |
| commit succeeds, sandbox killed | safe; commit durable |
| commit succeeds, reply lost | query by idempotency key |
| Store unavailable | fail; no local authority |
| branch head moved | `Conflict`; remount + retry |
| E2B 429/503 | no automatic replay; caller-visible `Unavailable`/`Unknown` |
| external API side effect | not rolled back; retry policy is tool-specific |
| FUSE missing | `Unsupported`; no fallback |

## 13. Open question: reusable session lease vs one-shot per call

Not decided. Phase 1 uses one-shot per call; correctness must survive it either
way.

Tradeoffs:

```text
for a reusable session lease
  amortize sandbox startup and branch mount across calls
  keep the Store lease and local cache warm

for one-shot per call
  no liveness/cancellation state between calls
  simpler timeout classification (lease == tool call)
  per-tool_call_id idempotency stays trivial
```

The reuse path reintroduces long-lived-process concerns (liveness, cancel,
capacity) and a lease that spans calls, so its idempotency and failure classes
would need a new contract. Measure startup/mount cost against cache warmth before
choosing.

## 14. Non-goals

```text
durable E2B machine
worker / worker_server / machine-local endpoint in phase 1
pause/resume as correctness
remote SQLite
longer-than-TTL tool calls in phase 1
multi-tenant fairness
automatic infinite retry
```

## 15. Acceptance

- Tool call succeeds, commit exists, sandbox killed after commit.
- Lease expires after commit: commit still readable.
- Lease expires before commit: result `Unknown`, no partial commit.
- Commit reply lost: one commit, query by idempotency key.
- Concurrent branch update: `Conflict`, retry from latest.
- Store down: explicit failure, no local write.
- FUSE check fails: explicit `Unsupported`.
- No raw TCP exposed; no worker, `worker_server`, or machine-local endpoint required.
- `machine-run` exits after commit; it never stays resident for a later call.
- E2B command logs correlate `tool_call_id`, commit id, lease id.
