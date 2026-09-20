# C3 product implementation specification

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

Here **C3 means the current pair 3 / #181 service, bridge and daemon foundation**,
not an instruction to implement FUSE or rename older C3 labels. Pair 1 / #179 and pair 2 / #180
remain later Workspace/FUSE and history work. This specification makes the
[design packet](../README.md) executable as implementation tasks; it supplies no
implemented protocol, qualified defaults or passing evidence.

Use [02 — module ownership](../02-file-layout-and-boundaries.md),
[03 — transport semantics](../03-operations-and-transport.md),
[04 — resources](../04-resource-and-verification-plan.md), and
[07 — public operations](../07-public-operations.md) as the detailed contracts.
Telemetry implementation is in [02-telemetry.md](02-telemetry.md); execution and
acceptance are in [03-verification.md](03-verification.md). Resolve conflicts
against current owner direction and the repository/core rules, not old snapshots.

## 1. Deliverable and exclusions

Deliver three real crates in the independent `core/` workspace:

| Crate | Focused ownership | Required dependency boundary |
| --- | --- | --- |
| `layerfs-bridge` | `contract/` request/result/outcome/limits; `adapters/native/` connection/client/server/payload and `protocol/` frame/metadata/state | No C1, C2, Workspace, service or database dependency |
| `layerfs-service` | Concrete `owner`; `operation/` access/admission/dispatch/lifecycle/read/write/filesystem; `input/` sequential/replay/records; `native/` config/startup | Uses bridge vocabulary, existing C1/C2 public facade and telemetry; native deployment stays outside handlers |
| `layerfs-daemon` | Configuration, startup/shutdown assembly and headless bounded stdin/stdout relay using bridge Client | No service/C2/native SQLite dependency; no duplicate operation parser |

Keep one bridge library with client/server endpoints and one native frame codec.
The server accepts a supplied handler; the service assembly supplies its concrete
handler. No mandatory operation trait, registry, per-op client class or second
bridge exists for direct calls. One logical service/daemon implementation serves
configured native placements; this does not promise identical platform binaries.

Implement the responsibility folders when their code exists, not empty scaffolds.
Production files are at most **999 physical lines**; `lib.rs`/`mod.rs` are at most
**200** and declarations/delegation only. Tests/fixtures/peers stay outside `src/`.
Use compatible existing dependencies before adding one; never patch third parties.

Main acceptance is **actual Linux Docker daemon -> network -> native macOS host
service -> C1/C2 -> SQLite**, with host driver input/result validation and default
forward-mode telemetry. No FUSE,
`/dev/fuse`, bind mount, shared-data volume, container Workspace directory or
container payload/spool/result files. Normal executable/config/image files remain.
Host service scratch has declared ownership; edit replay initially uses capped
service memory. Explicit optional daemon-local telemetry segments are a separate
tested output mode under 02, never an automatic fallback. A direct call is
secondary parity, never network acceptance.

Do not implement Workspace, mount/exec control, inode allocation, history/Commit,
GC of Store objects, managed/cloud providers, durable recovery, composite-submit,
resume, automatic retry, additional carriers or an embedded-daemon feature here.
Operational telemetry retention is a distinct explicitly scoped feature in 02.

## 2. M0: concrete decisions to freeze before dependent product code

Create one versioned implementation record with these exact decisions and their
verification cases. These are technical readiness gates, not invented approvals
or permission to reopen settled scope. Missing values are missing work, not
zero/unlimited defaults. The implementer can resolve ordinary technical choices
within the stated scope and record them without asking again for authorization.

| Freeze item | Required recorded decision |
| --- | --- |
| Core basis | Qualified source/tree and lock/toolchain; schema/canonical profile; relevant Stage 7 findings and disposition; concurrent Stage 6 changes reconciled |
| Carrier/deployment | One selected native network carrier; TCP remains the candidate until recorded; exact host/container route, listen/connect policy and process lifecycle |
| Authentication | Selected implementation/dependency and trust boundary; credential provisioning/verification, expiry/revocation behavior, secret redaction and transport integrity/confidentiality requirements |
| Authorization | Trusted caller context construction; caller-to-configured-Store and per-operation permissions; no client-selected native path or caller-declared authority |
| Protocol/codec | Magic/version, fixed field widths/endianness, frame/opcode/error values, flags, exact byte/record encodings, correlation reuse rules and compatibility refusal |
| Operation schemas | All five typed requests/results; exact closed `Inspect` variants; root roles, ranges, listing continuation, edit-part boundaries and prepared-record encoding |
| Supported work | Intended workload sizes/operations; finite F/W/C/A/P/D, request/response/replay/record/scratch allowances and checked aggregate memory; Q=0 is fixed |
| Native execution | Bounded synchronous-core/I/O bridge, admission ownership, deadline observation points and shutdown disposition; no unbounded tasks/threads or extra construction producers |
| Telemetry seam | Explicit enabled/disabled configuration, operation-report handoff, process monitor ownership and selected bounded reporting route per implementation 02 |

Do not treat illustrative resource arithmetic as admitted defaults. A required
workload that does not fit is a recorded unsupported/failing case; do not shrink
it, raise a timeout or hide required acquisition outside its operation.
The implementer may choose helper signatures, cohesive file splits and compatible
concrete codec/carrier libraries. Scope changes, new durability promises or
unsupported dependencies are explicit blockers rather than silent substitutions.

## 3. Public entry points and five real handlers

Conceptual shapes below define ownership, not frozen Rust signatures:

```text
Client.call(request, bounded_input, bounded_output)
Server.serve(connection, supplied_handler)
Service.handle(verified_caller, request, stable_input, bounded_output)
```

Direct invocation calls that same `Service.handle`, including validation,
authorization/admission and completion. Endpoint code establishes trusted peer
identity; request bytes cannot mint a verified context. Server dispatch remains a
closed match over supported operations, with native errors retained only in
bounded service diagnostics.

| Handler | Implement using existing core APIs | Completion and smallest useful check |
| --- | --- | --- |
| `ReadFile` | `StoreProvider` + C1 `read_range` | Exact bounded bytes, terminal length/status; verify range/zero-byte read and failed partial output |
| `Inspect` | Selected C1 inspection/`FilesystemRead` variants | Bounded typed result/page; verify absent path versus missing object and checked continuation |
| `ConstructFile` | Store-derived policy/capacities + `construct_stream` + `SaveHandoff` | Saved root only after input finality and `finish`; construct/read exact bytes including empty input |
| `EditFile` | `EditStream`, capped stable `EditSource`, `apply_edits`, local provider/handoff | New saved root; old root still reads; unsupported edits/replay sizes fail explicitly |
| `UpdatePreparedFilesystem` | Checked `FilesystemInput`, `FilesystemObjects`, `update_filesystem` | Existing-inode prepared update against retained roots; verify bindings/inodes and old root preservation |

For every call derive C1 policy from the selected Store; do not trust independently
supplied mutable capacities. Validate resource arithmetic and matching supported
capabilities before exposing configuration to untrusted requests. Fix or constrain
relevant reviewed core boundary gaps explicitly, with their own evidence.

Complete-file input adapts sequential bytes to `Read`: EOF occurs only after valid
`END_INPUT` and exact declared totals. Edits retain declared replacement parts in
reserved capped memory before C1 reads them; preserve current-result coordinates
and ordering. Do not coalesce/sort edits or flatten them into a complete file as a
fallback. Filesystem records hold final bindings for changed names, not every
unchanged entry, and are checked finite slices with explicit retained root roles.
No new-inode allocation is invented by the driver.

Begin one C2 save per mutation; keep its root private until valid input finality
and successful `finish`. Use one operation-local `StoreProvider` for published
base reads. Retain the original `SaveHandoff` error and checked cleanup outcome.
Do not assume a provider for unpublished output or force intermediate publication
to conceal a missing capability. N file saves plus a tree update remain N+1
operations; no composite/atomic-save promise is added here.

## 4. Wire, admission and failure invariants

- Use the frozen native `HELLO`, `BEGIN`, `BODY`, `END_INPUT`, `RESULT_DATA`,
  `SUCCESS`, `FAILURE` grammar. No in-band `CANCEL`, multiplexing, per-chunk ACK,
  canonical-object RPC, payload recompression or protocol resynchronization scan.
- Authenticate the connection and authorize every operation. Validate fixed header,
  legal state/kind/ID, lengths/counts/flags and checked totals before allocation.
  Unsupported versions/profiles are explicit failure, never a fallback attempt.
- One active operation per connection. Q=0: reject unavailable aggregate capacity
  or C2 ownership rather than queue/retry. N connections do not mean N writers.
- Pipeline bounded upload frames after `BEGIN`; read early responses concurrently.
  Slow peers propagate backpressure through bounded input/output, socket/runtime
  buffers and upstream driver. Do not launch a task per frame or collect full reads.
- An unread/desynchronized body after refusal terminates that connection and local
  stdin session unless boundaries are known synchronized. Never drain unlimited
  input for reuse or reinterpret trailing BODY as a new operation.
- Only C2 success permits successful saved-root delivery. Lost terminal delivery
  can leave the daemon with unknown outcome despite service-side success. Never
  replay, poll, delete or claim rollback on that uncertainty.
- Read bytes are provisional until terminal success; the caller must invalidate
  incomplete output. EOF is not success. Preserve logical absence, missing object,
  invalid input, access/capacity/ownership/provider and unknown-outcome distinctions
  actually available; do not reconstruct lost typed causes from error strings.
- Deadline/disconnect checks occur at safe adapter boundaries. Existing core calls
  are not universally preemptible; do not terminate mid-commit and report definite
  abort. Safe known failure gets one checked disposition; uncertainty retains the
  core's required state. Do not add fsync/WAL or a recovery path.

## 5. Milestones and reviewable outputs

| Milestone | Product tasks and required observable output |
| --- | --- |
| M0 — Contract freeze | Complete section 2 record and case mapping; unsupported work and core blockers named, not concealed by defaults |
| M1 — Telemetry foundations | Follow 02; retain actual existing Timing and a working disabled reporting path. Full native monitoring/output completion does not block the first real construct/read implementation |
| M2 — Bridge/service | Implement concrete schemas/codec/state checks, shared authorized service handler, sequential construction/read and bounded failure handling; external direct tests prove real C1/C2 save/reopen/readback, never mocks as sole proof |
| M3 — Actual daemon network path | Package real Linux daemon; host driver feeds bounded stdin and consumes stdout/stderr; verify real network peer/route, construction/readback and mount-free filesystem constraints |
| M4 — Full surface and failures | Complete all five handlers and declared Inspect variants; malformed/partial/oversized/denied/slow/disconnected cases, exact outcome taxonomy and multiple-daemon isolation/admission run on production paths |
| M5 — Native telemetry and output | Wire operation scopes/counters/resource windows without changing original results; exercise collectors, independent reporting failures, retention and disabled behavior |
| M6 — Final qualification | Run required core/feature/deployment checks and publish exact identities, results, gaps and production LOC per 03 |

Telemetry work may proceed alongside M2–M4 once its configuration/report seams
are frozen. Product outcomes never wait for telemetry persistence or become
mutation failures because diagnostics were dropped. Periodic resource reports
use the selected telemetry channel; do not invent unsolicited operation frames.

Each milestone leaves runnable external checks and a concrete handoff; passing
direct tests do not satisfy M3/M4 network evidence. Run only appropriate checks
without overlapping another owner's resource-sensitive run. Preserve all failures
and unrun cases. No benchmark/performance improvement, release admission or
Stage 7 substitution proof follows merely from these milestones.

## 6. Completion handoff

Provide implemented file/API map, frozen configuration/protocol record, exact
source/dependency/container identities, operation/telemetry verification results,
remaining limitations and reproduction commands. Include production LOC comparison
for every actual commit and the core boundary/format/test/clippy checks required
by repository rules. No aggregate preflight/CI replacement is introduced.
Do not retire root reference crates or alter unrelated Stage 6 work.
