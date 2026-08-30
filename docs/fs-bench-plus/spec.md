# fs-bench-plus specification

Status: draft binding benchmark contract
Protocol version: 0.3
Drafted: 2026-08-31
LayerFS source at revision: 0970008668f54bae841797dafd57acab191fba7f

## 1. Purpose and authority

fs-bench-plus measures the public, single-agent filesystem experience of
LayerFS V2 against pinned upstream Cloudflare Computer. It covers real
filesystem execution, immutable checkpoint creation, authority publication,
storage amplification, transfer, FUSE materialization, and fresh-process
recovery.

The protocol is designed to answer all of these questions without mixing
evidence layers:

1. How quickly does a user-visible filesystem operation become authoritative
   and durable?
2. Does work for a small edit stay independent of complete file size?
3. How many canonical or blob bytes are newly retained because of an edit?
4. Does Reference placement avoid copying authority-owned base content?
5. Does transfer send and verify only the required canonical frontier?
6. What does cold, warm, and incremental FUSE materialization cost?
7. Does every acknowledged state survive process, Workspace, FUSE, and
   connection replacement?

The binding LayerFS product architecture remains docs/v2/spec.md. This
document may prescribe benchmark instrumentation and public-SDK orchestration,
but it may not create a benchmark-only product architecture.

In this protocol, **materialization** means making an immutable root available
through a real container FUSE Workspace projection, from public Workspace
creation through mount readiness and exact first access. It does not mean the
separate Host-placement `WorkspaceProjection::Materialize` directory-export
path. That path is out of scope for fs-bench-plus.

The protocol name is fs-bench-plus. Until explicitly renamed, its
implementation and result paths remain:

~~~text
benchmark/fs-bench                 # frozen base FUSE/fs-bench suite
benchmark/fs-benchmark-pro         # fs-bench-plus SDK/E2E implementation
benchmark-results/fs-bench         # base-suite evidence
benchmark-results/fs-bench-plus    # fs-bench-plus raw evidence and history
~~~

The Cloudflare CAS/CDC/COW article supplies workload inspiration and evidence
discipline. C3 and its recorded results are not benchmark candidates and are
not imported into LayerFS conclusions.

## 2. Normative language

MUST and MUST NOT are validity requirements.

SHOULD and SHOULD NOT are requirements that may be waived only by a written,
evidence-backed protocol exception frozen before execution.

MAY identifies an optional capability or measurement.

A campaign that violates a MUST requirement is invalid even if every test
process exits successfully.

## 3. Scope

### 3.1 Product candidates

There are exactly two product candidates:

1. Computer upstream.
2. LayerFS Reference.

LayerFS Workspace Commit is a required measured phase inside LayerFS
Reference. It is not a third candidate because Commit alone does not establish
authority visibility.

The primary comparison is:

| Candidate | Required durable boundary |
|---|---|
| Computer upstream | Public Workspace execution, real FUSE, completed synchronization to authoritative SQLite, matched durability barrier, and authoritative visibility |
| LayerFS Reference | Public SDK Workspace execution, real FUSE, Workspace Commit, Branch Push, matched two-Store durability barrier, and authoritative visibility |

### 3.2 Explicit exclusions

The following MUST NOT appear as candidates or be pooled into headline
statistics:

- C3;
- LayerFS Replica;
- OverlayFS;
- multi-agent or concurrent-writer workloads;
- branch fan-out or conflict races;
- synthetic in-memory storage engines;
- Host-placement `WorkspaceProjection::Materialize` directory export;
- direct internal content or Store APIs substituted for the public SDK path;
- an engine-only splice substituted for a real editor/FUSE prepend;
- a TUI;
- article result values treated as samples from this benchmark.

### 3.3 Evidence layers

The report MUST keep these layers separate:

| Evidence layer | Question answered |
|---|---|
| Content mechanism | How much byte, CDC, tree, hashing, and canonical-object work occurred? |
| Public Workspace SDK | What latency and behavior does an SDK user observe? |
| Workspace Commit | How does captured Workspace state become an immutable Branch Commit? |
| Authority checkpoint | How long until the Commit is durably visible at the authority? |
| Transfer | What was announced, avoided, sent, admitted, and verified? |
| Semantic storage | Which canonical or blob bytes are unique, shared, inserted, reused, reachable, or retained? |
| Physical storage | What DB, WAL, SHM, scratch, projection, and allocated bytes exist? |
| FUSE materialization | What do cold mount, warm mount, next-root projection, and first access cost? |
| Recovery | Can a fresh process reproduce every acknowledged oracle? |

An engine-only result MUST NOT be called user-facing performance. Workspace
Commit MUST NOT be called equivalent to Computer authority durability.
Semantic bytes MUST NOT be called physical storage.

## 4. Frozen provenance

### 4.1 Article method source

The article is frozen as method input, not as candidate evidence:

~~~text
repository: https://github.com/agent-infra-foundation/agent-infra-book
commit:     fa934484e5041ec83a8ab0d38d7a12512d58b0ed
path:       cloudflare/computer/chapters/PART-III-X-ARTICLE.md
~~~

Its workload shapes and evidence separation may be reproduced. Its C3 source,
multi-agent result, and recorded ratios are not samples in this benchmark.

### 4.2 Computer source

Formal Computer runs MUST use unmodified upstream source:

~~~text
repository: https://github.com/cloudflare/computer
commit:     de87919a4fd37242e960e13b7b3ba802d1eef0a0
tree:       4fb409d7e1356e1098439293d77d2fdc2dbf2190
~~~

The formal image MUST be a sealed source build. The harness MUST reject:

- another commit or tree;
- patched Computer product source;
- a source override from the environment;
- an unverifiable prebuilt distribution;
- mismatched source archive, lockfile, adapter, or OCI labels.

A diagnostic prebuilt image MAY run one smoke pair but MUST NOT support a
publishable performance or storage claim.

### 4.3 LayerFS source

Every LayerFS image MUST record:

- commit and tree;
- clean or dirty status;
- a SHA-256 source seal over production crates, SDK, FUSE, Workspace,
  manifests, lockfile, benchmark package, and container definition;
- staged and unstaged diffs;
- the inventory and hashes of admitted untracked source files;
- image labels tying the binary to that source seal.

A dirty candidate is allowed only when the evidence can reconstruct its exact
source. Any source change requires a new image, run ID, schedule, and campaign.

### 4.4 Environment

Both candidates in a pair MUST use the same:

- physical host and Docker daemon;
- architecture;
- CPU quota and affinity policy;
- memory and swap limits;
- PID limit;
- network policy;
- privilege and FUSE policy;
- temporary-filesystem limits;
- fixture and workload helper;
- trial-adjacent scheduling policy.

Host, kernel, Docker, storage, CPU, memory, and candidate cgroup information
MUST be captured before the first trial.

### 4.5 Frozen container images and runtime

fs-bench-plus runs both candidates in Docker with real `/dev/fuse`. The
LayerFS measurement image MUST be built from:

~~~text
Dockerfile: benchmark/fs-benchmark-pro/Dockerfile.layerfs
recommended tag: layerfs-fs-benchmark-pro:local
build base: rust:1.85.1-bookworm
runtime base: rust:1.85.1-bookworm
~~~

It contains the release `fs-benchmark-pro` controller, the release production
`layerfs-fuse` proxy helper, Docker CLI for the production Container placement
path, and the sealed neutral workload helper. Its OCI labels MUST match the
admitted LayerFS commit, tree, dirty state, and source-seal SHA-256.
`containers/layerfs-fuse/Dockerfile` is the product/base-fs-bench image and
MUST NOT replace this public-SDK controller image in fs-bench-plus.

The Computer measurement image MUST be built from:

~~~text
Dockerfile: benchmark/fs-benchmark-pro/Dockerfile.computer
recommended tag: layerfs-fs-benchmark-pro-computer:de87919a
build base: node:22.22.0-bookworm
runtime base: node:22.22.0-bookworm-slim
upstream commit: de87919a4fd37242e960e13b7b3ba802d1eef0a0
upstream tree: 4fb409d7e1356e1098439293d77d2fdc2dbf2190
~~~

The Computer build context contains the exact sealed upstream source archive
and admitted adapter files only. Both image architectures MUST match.

Every candidate arm uses a fresh container with this common envelope:

~~~text
--privileged
--device /dev/fuse:rwm
--cap-add SYS_ADMIN
--security-opt apparmor=unconfined
--security-opt seccomp=unconfined
--network none
--cpus 1
--memory 1g
--memory-swap 1g
--pids-limit 512
--tmpfs /tmp:rw,nosuid,nodev,size=256m
~~~

The LayerFS controller mounts the Docker socket only because the production
`WorkspacePlacement::Container` implementation uses Docker to copy, start,
control, and remove the FUSE helper in the target container. The measured
request points at that fresh container and uses:

~~~rust
WorkspacePlacement::Container { container_id, root: mount_root }
WorkspaceProjection::Fuse
~~~

One outer `docker exec` MAY launch the sealed benchmark controller inside the
already admitted measurement container. It MUST NOT execute the registered
filesystem workload. Every registered read or mutation MUST be launched by
`Client::exec_workspace_session`, consumed through
`Client::workspace_output` and `OutputReader`, and performed through the real
FUSE mount. Docker calls made internally by the production Container
projection to attach/control the FUSE helper are product work and remain timed.

Each LayerFS trial uses a fresh measurement container. Fresh-process recovery
uses a second fresh container over the retained Store files. Each Computer
trial also uses a fresh container. No candidate container, mount, Workspace,
application cache, or Store crosses trial boundaries.

## 5. Public product paths

### 5.1 LayerFS public SDK requirement

All publishable LayerFS performance operations MUST be driven through the
public layerfs-sdk surface.

The harness MAY use public SDK-reexported Store constructors to create and
connect databases. It MUST NOT invoke private Store SQL or private mutation
helpers.

The admitted setup sequence is:

~~~rust
LayerStackStore::create(authority_path)
BranchStore::create(branch_path, authority_store_id)

Client::connect(ConnectionContext {
    layerstack: LayerStackEndpoint::local(authority),
    branches,
})

Client::initialize_layerstack(name, initialization)
Client::pull_layer(layer_id, RemotePlacement::Reference)
Client::fork_branch(name, LocalForkSource::Layer { layer_id })
~~~

Every primary comparable mutation checkpoint MUST use this current public SDK
lifecycle:

~~~rust
let workspace_id = client.create_workspace_session(CreateWorkspaceSession {
    branch_id,
    placement: WorkspacePlacement::Container { ... },
    projection: Some(WorkspaceProjection::Fuse),
})?;

let argv = NonEmpty::new(argv)?;
let execution = client.exec_workspace_session(workspace_id, argv)?;
let output = client.workspace_output(execution.id)?;
let mut after = 0;
let receipt = loop {
    let page = output.read(after, true)?;
    after = page.next_sequence;
    if page.exited {
        break page.receipt.ok_or(MissingTerminalReceipt)?;
    }
};

for checkpoint in checkpoints {
    let execution = client.exec_workspace_session(workspace_id, checkpoint.argv)?;
    // Read OutputReader through the terminal receipt, then:
    let commit = client.commit_workspace_session(workspace_id)?;
    let push = client.push_branch(branch_id)?;
}
client.end_workspace_session(workspace_id, EndWorkspaceMode::Clean)?;
~~~

Protocol 0.3 requires a successful Commit to rebase the existing active
Workspace in place. The rebase preserves visible NodeIds, open-handle identity,
the branch lease, and the same FUSE mount; advances the pinned head, base root,
reader, and exact COW base; clears committed dirty/spool state; and resumes the
same projection. A reload under a retained mount or a simple state change is
invalid. EDIT16-K1, EDIT16-K4, and EDIT16-K16 each create one public Workspace,
run every checkpoint on that Workspace, and end it once. Create/attach and
End/unmount remain inside the complete scenario total.

The execution receipt MUST prove:

~~~text
exit_code = Some(0)
stopped = false
terminal receipt present
no truncated required oracle output
~~~

Evidence and recovery MAY use:

~~~rust
Client::monitor_snapshot()
Client::analyze_dedup()
Client::query(...)
LayerStackStore::inventory_page(...) // read-only, after timer
BranchStore::inventory_page(...)     // read-only, after timer
BranchStore::root_complete(...)      // registered-root proof, after timer
Client::add_layer(...)   // diagnostic and excluded from the primary boundary
~~~

A formal authority-to-stable result requires the existing Commit/Push lifecycle
to complete the matched two-Store durability work before acknowledgement. Do
not add a new public operation. Existing receipts and Monitor evidence MUST
identify both bound Store IDs and record each WAL checkpoint, database fsync,
and parent-directory fsync outcome. Private SQL, opening the databases behind
the SDK, and benchmark-only checkpoint hooks are not substitutes.

Fresh-process verification MUST reconnect the exact Stores through public
constructors, create a public SDK Workspace, execute the verifier through
Client::exec_workspace_session, consume its OutputReader, and end the
Workspace through the SDK.

The benchmark MUST NOT use any of these as a headline shortcut:

~~~text
docker exec for the registered filesystem workload
BranchStore::commit_changes directly
ContentChange::Splice directly
ObjectBuffer directly
StoreDb SQL directly
test-only mutation or admission hooks
~~~

Direct content tests MAY verify an algorithmic oracle, but their time does not
enter product comparison tables.

### 5.2 Computer public path

Computer MUST run through the pinned public Workspace, computerd, RPC
synchronization, and real FUSE path.

The timer MUST include the public execution result through completed
post-command synchronization. The harness MUST reject a row unless:

~~~text
execution completed
exit code = 0
sync status = complete
skipped entries = empty
~~~

The harness MUST NOT patch the Computer representation, emulate
Workspace.runtime.exec, or replace computerd/FUSE with a host directory.

### 5.3 Shared workload helper

Both candidates MUST execute the same byte-identical sealed helper binary.
The helper MUST use the same syscalls, flags, buffer sizes, fsync calls, close
ordering, and rename sequence in both candidates.

Its SHA-256 and build provenance MUST be recorded and verified inside both
candidate environments.

The helper and fixture MUST be mounted read-only outside both measured
Workspace namespaces. They MUST never be synchronized, captured, chunked,
canonicalized, or counted as candidate storage. Only the helper's target file
and registered namespace are inside the measured FUSE mount.

Candidate adapters may establish public sessions around the helper. They MUST
NOT reimplement the mutation differently per arm.

## 6. Fairness and anti-cheating contract

### 6.1 Forbidden specialization

Production code MUST NOT inspect or special-case:

- fs-bench-plus names;
- target or temporary paths;
- fixture digests;
- edit offsets;
- edit marker bytes;
- operation indexes;
- expected roots, sizes, or final digests;
- smoke, pilot, or formal mode;
- trial number or candidate order.

The harness MUST scan measured product diffs for benchmark-specific names,
paths, hashes, offsets, and markers before admitting an image.

### 6.2 Forbidden storage shortcuts

The benchmark MUST NOT use:

- zero-filled or compression-friendly substitutes for the registered
  high-entropy fixture;
- sparse holes in place of fixture bytes;
- hard links or reflinks to create a measured result unless the scenario is an
  explicitly named clone/reflink test;
- precomputed final files;
- fixture-specific dictionaries;
- disabled or delayed durability;
- background persistence left incomplete when the timer stops;
- an expected final root injected into candidate state;
- a hidden pre-mounted FUSE projection or pre-read target;
- durable candidate state carried from another trial.

Ordinary product CAS, CDC, canonical objects, persistent extents, SQLite page
cache, and kernel page cache are allowed when reached naturally through the
public path.

### 6.3 No post-result tuning

All production configuration and benchmark thresholds MUST be frozen before
the formal schedule is generated.

Per-trial tuning, profile selection after seeing results, favorable retry, and
best-configuration selection from the formal population are prohibited.

### 6.4 No result laundering

The report MUST NOT:

- combine engine, SDK, and E2E latencies into one unnamed metric;
- call a LayerFS-only operation a Computer comparison;
- call the single-agent metric the article's branch-storage result;
- replace missing data with zero;
- remove slow valid rows;
- sum independent medians to manufacture an aggregate;
- use the best trial as a headline;
- compare a warmed arm with a cold arm without an explicit cache-state label.

## 7. Preparation and cache policy

### 7.1 Preparation cache

Preparation caches MAY contain only candidate-independent or source-sealed
setup artifacts:

- compiled binaries;
- Docker image layers;
- Cargo and package-manager downloads;
- dependency installation outputs;
- benchmark scripts;
- the sealed helper binary;
- source archives and environment inspection.

Preparation MUST NOT cache candidate workload state:

- Store databases;
- canonical objects or Computer blobs;
- manifests, extents, roots, or changed-object lists;
- Workspace spools;
- pre-mounted FUSE projections or candidate mount state;
- path, inode, membership, or completeness lookups;
- prior operation receipts used to skip product work;
- expected final state.

The fixture is registered immutable input, not candidate cache. It MUST be
generated and hashed outside candidate timers after images are sealed, mounted
read-only, and never embedded inside either image.

### 7.2 Natural product state

Natural state created by earlier registered operations in the same scenario is
part of the product and may remain available. The sixteen-edit sequence
intentionally observes repeated checkpoints of the same file.

The harness MUST NOT reset one candidate between registered edits while
allowing the other to retain natural state.

Each new trial MUST use fresh candidate durable directories, runtime
directories, Workspaces, mount points, and containers. No candidate
application cache crosses trial boundaries.

### 7.3 Dedicated warm tests

Warm behavior is allowed only in explicitly named warm scenarios. The warm
state MUST be created by a registered candidate-local setup operation whose:

- elapsed time;
- bytes read and written;
- immutable root and mount identity;
- process and connection state;
- allocation;
- inclusion or exclusion from the timed phase

are all recorded.

Warm state MUST NOT come from the other candidate or another pair. Warm and
cold samples MUST never be pooled.

### 7.4 Cache-state axes

Every FUSE-materialization and executor-start row MUST record these independent
axes:

| Axis | Values |
|---|---|
| Product durable state | empty, seeded, history-retained |
| Candidate process | fresh-process, same-process |
| SQLite connection | fresh-connection, reused-connection |
| Workspace/projection | new-workspace, existing-active-workspace, new-FUSE-mount, existing-FUSE-mount |
| Mount/root | mount-absent, same-root-mounted, next-root-remount, process-cold-remount |
| OS page cache | host-evicted, proven-warm, uncontrolled |
| Fixture source | host-evicted, proven-warm, uncontrolled |

A single cold boolean is forbidden.

### 7.5 OS page cache claims

A new process, new container, new mount namespace, posix_fadvise call, or a
large filler read does not prove an evicted host page cache.

host-evicted may be claimed only when:

1. the native Linux host or Docker VM is controlled;
2. all candidate processes and descriptors are stopped;
3. sync completes;
4. the documented host eviction operation succeeds;
5. its timestamp is between trials;
6. fault and storage-read counters are captured where possible.

Otherwise the row MUST say os_page_cache=uncontrolled.

If the shared fixture cannot be evicted between adjacent arms, the harness
SHOULD prewarm it identically immediately before each arm and label the source
proven-warm. It MUST NOT allow the first arm to warm the fixture silently for
the second.

## 8. Fixture and oracle

The core input is deterministic AES-256-CTR over zero bytes:

~~~text
key: 0x07 repeated 32 times
IV:  0x03 repeated 16 times
~~~

The registered core fixture is:

~~~text
bytes:   33,554,432
SHA-256: 3d2fadd86ea3d8c52f8f3255bec470f2da7e31b7ed809cc0e97e1e9dc894cd8c
~~~

The sixteen edits use:

~~~text
edit bytes: 10
offset(i):  ((i + 1) * 2,654,435,761) mod (file_size - 10)
i:          0 through 15
markers:    E000000001 through E000000016
~~~

For 32 MiB:

~~~text
post-edit SHA-256:
30e8b6c71ab635057c32f0e509e6e0037b5781f94bf1b4c88fb438f41d76ca26

prepend bytes:
PREPEND010

final bytes:
33,554,442

final SHA-256:
7b86abcd0e9d2016bbb8b16722e1439475feff84e31fe9801a4ec74e99dc74c3
~~~

Fixture generation, neutral oracle construction, setup seeding, and final
independent verification are outside candidate operation timers. Their elapsed
time, size, and hashes remain evidence.

Every mutating helper command MUST fsync and close its changed file before
exiting. A temp-file replacement MUST fsync and close the temporary file before
atomic rename.

## 9. Scenario topology

Cold creation and Reference editing MUST use separate fresh contexts so cold
creation does not copy the base into the edit BranchStore.

### 9.1 Cold-create topology

~~~text
Computer:
  fresh authority
  target absent

LayerFS:
  fresh empty LayerStackStore
  fresh BranchStore
  empty genesis Layer
  Reference Pull
  zero-copy local Fork
  target absent
~~~

The measured operation creates the file through public execution and real FUSE,
then reaches the comparable durable boundary.

### 9.2 Reference-seeded topology

~~~text
Computer:
  seed deterministic target outside scenario timer
  reach authority durability
  close and reopen

LayerFS:
  initialize authority through LayerStackInitialization::Directory(seed)
  Reference Pull the resulting Layer
  zero-copy Fork a local Branch
  close and reopen the exact Store pair
~~~

Before the timed scenario:

~~~text
BranchStore base object count = 0
BranchStore base object bytes = 0
Fork canonical object delta = 0
Reference fallback active
authority Layer root authenticated
~~~

A violation invalidates the row. The harness MUST NOT silently continue with a
local base copy or Replica behavior.

## 10. End-to-end timing

All clocks MUST be monotonic. Nested phases MUST be non-overlapping or marked
overlapping.

### 10.1 Computer

Required phases:

~~~text
setup_ns
workspace_create_ns
runtime_exec_sync_wait_ns
wal_checkpoint_ns
database_fsync_ns
directory_fsync_ns
api_ns
persistence_ns
to_stable_ns
workspace_end_ns
complete_scenario_ns
reopen_ns
~~~

Required equation:

~~~text
to_stable_ns
  = api_ns
  + wal_checkpoint_ns
  + database_fsync_ns
  + directory_fsync_ns

complete_scenario_ns
  = workspace_create_ns
  + sum(to_stable_ns for registered checkpoints)
  + workspace_end_ns
~~~

Unsupported directory fsync is unavailable with the exact error; it is not
zero.

### 10.2 LayerFS SDK and FUSE

Required user-observed phases:

~~~text
sdk_workspace_create_ns
sdk_exec_dispatch_ns
sdk_output_to_terminal_ns
sdk_workspace_commit_ns
sdk_branch_push_ns
sdk_workspace_end_ns
authority_api_ns
authority_to_stable_ns
complete_checkpoint_ns
complete_scenario_ns
reopen_ns
~~~

Required production fragments:

~~~text
root_pin_ns
projection_attach_ns
fuse_fence_ns
spool_fsync_ns
pause_quiesce_ns
capture_ns
candidate_plan_ns
dirty_compare_ns
content_mutation_ns
namespace_mutation_ns
local_admission_ns
completeness_verify_ns
commit_publish_ns
workspace_reload_ns
projection_transition_ns

push_history_ns
push_frontier_ns
push_membership_ns
push_object_admission_ns
push_fact_admission_ns
push_authority_verify_ns
push_publish_ns
push_durability_ns

branch_store_checkpoint_ns
layerstack_store_checkpoint_ns
branch_store_database_fsync_ns
branch_store_directory_fsync_ns
layerstack_store_database_fsync_ns
layerstack_store_directory_fsync_ns
durability_unattributed_ns
~~~

Required equations:

~~~text
sdk_workspace_commit_ns
  = pause_quiesce_ns
  + capture_ns
  + candidate_plan_ns
  + dirty_compare_ns
  + content_mutation_ns
  + namespace_mutation_ns
  + local_admission_ns
  + completeness_verify_ns
  + commit_publish_ns
  + workspace_reload_ns
  + projection_transition_ns
  + commit_unattributed_ns

sdk_branch_push_ns
  = push_history_ns
  + push_frontier_ns
  + push_membership_ns
  + push_object_admission_ns
  + push_fact_admission_ns
  + push_authority_verify_ns
  + push_publish_ns
  + push_durability_ns
  + push_unattributed_ns

push_durability_ns
  = branch_store_checkpoint_ns
  + layerstack_store_checkpoint_ns
  + branch_store_database_fsync_ns
  + branch_store_directory_fsync_ns
  + layerstack_store_database_fsync_ns
  + layerstack_store_directory_fsync_ns
  + durability_unattributed_ns

authority_api_ns
  = sdk_exec_dispatch_ns
  + sdk_output_to_terminal_ns
  + sdk_workspace_commit_ns
  + sdk_branch_push_ns

authority_to_stable_ns
  = authority_api_ns

complete_checkpoint_ns
  = sdk_workspace_create_ns
  + authority_to_stable_ns
  + sdk_workspace_end_ns

complete_scenario_ns
  = sum(complete_checkpoint_ns for registered checkpoints)
~~~

`authority_to_stable_ns` equals `authority_api_ns` because the existing Push
operation does not return before `push_durability_ns` completes. The same
durability fragments MUST NOT be added again outside the Push timer.

The report MUST show authority_to_stable as a diagnostic for each checkpoint.
The primary public-path boundary is complete_checkpoint for a single
checkpoint and complete_scenario for an aggregate row. Lifecycle time may not
be removed from the primary ratio.

READ-SYNC-32M is read-only: its complete scenario is public Workspace create,
SDK exec/output through the full read and normal synchronization, and clean
Workspace end. Commit, Push, and mutation durability are N/A, not zero, and the
harness MUST NOT issue a synthetic no-op Commit merely to alter the result.

For healthy FUSE, capture is expected to walk zero materialized files because
mutations already live in the Workspace overlay. The benchmark MUST measure
this rather than assume it.

### 10.3 Durability symmetry

The report MUST publish:

1. product API durability according to each product contract;
2. a matched quiesced stable snapshot.

The matched Computer barrier is:

~~~text
checkpoint authority SQLite
fsync authority database
fsync parent directory when supported
~~~

The matched LayerFS barrier is:

~~~text
checkpoint BranchStore SQLite
checkpoint LayerStackStore SQLite
fsync both databases
fsync their parent directory when supported
~~~

The LayerFS barrier MUST complete inside the existing Commit/Push public
lifecycle and be exposed through existing operation receipts and Monitor data.
No new public operation or benchmark-only hook is permitted. Missing evidence
makes a formal matched-stability campaign unavailable; it does not authorize
private SQLite access.

Computer API plus barrier MUST NOT be compared with LayerFS API alone.

An acknowledged operation is valid only when exact bytes are recoverable
without the process, FUSE daemon, Workspace, or container that performed it.

## 11. Core public-SDK E2E matrix

All LayerFS rows in this section use Client::exec_workspace_session and
OutputReader to terminal receipt.

The primary boundary is each candidate's complete public workflow in an
already-prepared container. Each registered scenario creates one public
Workspace/FUSE projection, executes all scenario checkpoints, and closes that
Workspace once. Image build, container/process startup, readiness, and
prerequisite installation are setup evidence outside the headline. Public
Workspace ready/attach and close/unmount are inside the complete scenario.

| ID | Setup | Filesystem operation | Checkpoints | Purpose |
|---|---|---|---:|---|
| COLD-CREATE-32M | cold-create | Create and fsync 32 MiB | 1 | Initial indexing and write throughput |
| EDIT16-K1 | Reference-seeded | Sixteen distributed 10-byte overwrites | 16 | Headline tiny edits |
| SAME-BYTE-NOOP | Reference-seeded | Write existing ten bytes | 1 attempt | Exact no-op |
| APPEND-10B | Reference-seeded | Append ten bytes | 1 | Native count growth |
| TRUNCATE-SHRINK-4K | Reference-seeded | Remove final 4 KiB | 1 | Native count decrease |
| TRUNCATE-GROW-4K | Reference-seeded | Extend 4 KiB with zeros | 1 | Sparse count growth |
| RENAME-ONLY | Reference-seeded | Rename existing file | 1 | Namespace-only mutation |
| PREPEND-TEMP-RENAME | Reference-seeded | Prefix via temp-copy-fsync-rename | 1 | Article-compatible editor count change |
| REWRITE-32M | Reference-seeded | Replace every byte with second stream | 1 | Dense fallback control |
| READ-SYNC-32M | registered result | Read complete file through public execution | 0 mutation | Read regression |
| REOPEN | acknowledged state | Fresh process/context and public verification execution | 0 | Durability oracle |

### 11.1 Checkpoint frequency

The same sixteen edits MUST run as:

| ID | Edits per Commit and Push | Checkpoints |
|---|---:|---:|
| EDIT16-K1 | 1 | 16 |
| EDIT16-K4 | 4 | 4 |
| EDIT16-K16 | 16 | 1 |

All rows fsync each file edit and produce the same final byte oracle. They may
have different operation-history roots and Commit histories.

For checkpoint size K, both candidates execute exactly one sealed-helper
command containing K edits, and the helper fsyncs after every edit. Computer
then performs exactly one public post-command authority synchronization and
stable barrier. LayerFS performs exactly one Workspace Commit, one Branch Push,
and one public two-Store stable barrier per checkpoint group. K1 has 16 public
execution/checkpoint groups in both arms, K4 has four, and K16 has one. Every
row creates one Workspace before its first group and performs one clean End
after its final group.

### 11.2 File-size scaling

The same ten-byte overwrite sequence MUST run at:

~~~text
4 MiB
32 MiB
256 MiB
~~~

The report MUST separate execution time from Commit, Push, and barrier time.

### 11.3 Edit-size scaling

A single middle-file equal-length overwrite MUST run with:

~~~text
1 byte
10 bytes
4 KiB
64 KiB
1 MiB
~~~

Any incremental-to-full fallback threshold MUST be recorded explicitly.

### 11.4 Count-changing provenance

The public benchmark distinguishes:

1. public POSIX append and truncate, whose exact mutation is known;
2. public editor-style temp-copy-rename, whose application I/O is linear;
3. optional explicit insert/collapse-range syscalls, once supported by both
   public candidates.

The existing internal semantic Splice is an algorithmic correctness oracle. It
MUST NOT enter user-facing latency tables because the current Client has no
public splice operation.

Optional rows:

| ID | Admission condition |
|---|---|
| INSERT-RANGE-4K | Both candidates support equivalent FUSE insert range |
| COLLAPSE-RANGE-4K | Both support equivalent FUSE collapse range |
| COPY-RANGE-EDIT | Both expose equivalent copy_file_range behavior |
| REFLINK-EDIT | Both expose equivalent clone/reflink behavior |

If only LayerFS supports the operation, the result is LayerFS-only and has no
Computer speedup.

The benchmark MUST NOT infer a prepend or copy because of a temp filename,
small size difference, sampled equality, known marker, or rename-over.

## 12. Reference storage, deduplication, and transfer section

Storage snapshots MUST occur at:

~~~text
S0 fresh stores
S1 seeded authority
S2 Reference Pull and zero-copy Fork, then clean close and reopen
S3 before public execution
S4 after execution and before Commit
S5 after Workspace Commit and before Push
S6 after Push
S7 after optional Add
S8 after clean close and fresh-process reopen
~~~

### 12.1 Per-Store measurements

For each LayerFS Store:

~~~text
database apparent bytes
WAL apparent bytes
SHM apparent bytes
database allocated bytes
WAL allocated bytes
SHM allocated bytes
object count
canonical object bytes
fact rows and encoded bytes by kind
~~~

Additionally report role-specific closure evidence:

~~~text
BranchStore:
  root_complete(root) result for every registered retained root
  count of registered retained roots returning true

LayerStackStore:
  complete_roots receipt = N/A (the schema has no receipt table)
  published Layer and Commit roots whose authority closure validation succeeded
~~~

The BranchStore count is not represented as a total receipt-table row count;
unregistered roots are outside it. Authority-published roots MUST NOT be
relabeled as receipt rows.

Let:

~~~text
B = BranchStore canonical object set
A = LayerStackStore canonical object set
U = B union A
I = B intersection A
~~~

The benchmark MUST compute by streaming sorted inventories:

~~~text
authority_only_ids and bytes
branch_only_ids and bytes
shared_ids and bytes
union_cas_bytes
physical_cas_bytes = bytes(B) + bytes(A)
placement_duplicate_bytes = physical_cas_bytes - union_cas_bytes
placement_factor = physical_cas_bytes / union_cas_bytes
~~~

LayerFS inventory, Store allocation, and union/intersection values MUST use
`Client::monitor_snapshot()`, `Client::analyze_dedup()`, and, when the report
needs the per-ID partition, the SDK-reexported read-only
`inventory_page()` methods after the timer stops. Registered-root completeness
uses the SDK-reexported public `BranchStore::root_complete()`. Filesystem
metadata may measure apparent and allocated DB/WAL/SHM bytes after the timer.
The benchmark may not open LayerFS SQLite files or call a private method to
recover a counter omitted by these public surfaces.

The report MUST NOT show only a sum of SQLite files. That hides whether
Reference avoided copying the base.

### 12.2 Retention

Report:

~~~text
head_reachable_bytes
history_union_bytes
history_overhead_bytes
retained root count
oldest root readable/authenticated
latest root readable/authenticated
Layer count
Commit count
owned Commit count
pulled ancestry Commit count
~~~

Computer orphan bytes and LayerFS addressable immutable history are not
equivalent and MUST remain separate.

### 12.3 Candidate equation

Reference-aware candidate accounting is:

~~~text
candidate_ids
  = inserted_ids
  + local_reused_ids
  + source_reused_ids

candidate_bytes
  = inserted_bytes
  + local_reused_bytes
  + source_reused_bytes
~~~

source_reused means the immutable parent route already holds the canonical
object and Reference may serve it without copying it into BranchStore.

### 12.4 Transfer equations

Every transfer MUST satisfy:

~~~text
announced_ids = sent_ids + avoided_ids
announced_bytes = sent_bytes + avoided_bytes
sent_ids = inserted_ids + raced_existing_ids
sent_bytes = inserted_bytes + raced_existing_bytes
~~~

Report:

~~~text
owned Commit facts
root requests
equal-root and equal-subtree prunes
objects visited and authenticated
canonical bytes read
membership pages
payload batches
announced, missing, sent, inserted, raced, and avoided IDs/bytes
authority verifier IDs/bytes/prunes
transfer buffer peak
Seen spill state
~~~

Pulled ancestry facts or canonical payload actually sent back to the authority
are a hard failure. A bounded base ID announced for membership and avoided is
reported as transfer amplification, not mislabeled as retransmitted payload.

### 12.5 Article-inspired storage section

Because multi-agent is excluded, the report MUST NOT claim reproduction of the
article's 98.4 percent branch result.

The single-agent article-inspired metrics are:

~~~text
incremental checkpoint payload
durable storage growth
branch-only canonical bytes at S5
cross-Store union growth
physical placement growth
candidate amplification
transfer announcement and payload amplification
~~~

The following per-pair boundaries and equations are frozen. `C_seed` is the
matched durable Computer snapshot immediately after seeding and reopen;
`C_edit16` is the matched durable snapshot after the sixteenth edit and reopen.
For Computer, `retained_blob_and_manifest_bytes` is the sum of retained BLOB
payload and its serialized manifest/state representation at that snapshot.

~~~text
LayerFS incremental semantic bytes
  = history_union_bytes(S6 after EDIT16)
  - history_union_bytes(S2 Reference seed and Fork)

Computer incremental semantic bytes
  = retained_blob_and_manifest_bytes(C_edit16)
  - retained_blob_and_manifest_bytes(C_seed)

incremental semantic payload ratio
  = LayerFS incremental semantic bytes
  / Computer incremental semantic bytes

branch-private bytes
  = branch-only canonical bytes(S5)
  - branch-only canonical bytes(S2)

unique authority-result growth
  = union_cas_bytes(S6) - union_cas_bytes(S2)

physical placement growth
  = physical_cas_bytes(S6) - physical_cas_bytes(S2)

LayerFS durable allocation delta
  = sum allocated DB + WAL + SHM bytes for both Stores at S8
  - sum allocated DB + WAL + SHM bytes for both Stores at S2

Computer durable allocation delta
  = allocated authority DB + WAL + SHM bytes at C_edit16
  - allocated authority DB + WAL + SHM bytes at C_seed

durable allocation ratio
  = LayerFS durable allocation delta
  / Computer durable allocation delta

LayerFS physical write bytes
  = cgroup or device write bytes for all candidate-owned processes
    during the registered EDIT16 scenario

Computer physical write bytes
  = the identically scoped Computer counter

physical write ratio
  = LayerFS physical write bytes / Computer physical write bytes

current-head semantic storage ratio
  = LayerFS head-reachable canonical bytes at S6
  / Computer current-head referenced BLOB + manifest bytes at C_edit16
~~~

S2 and S8 are matched quiesced, clean-close/reopen snapshots. Physical-write
ratio is unavailable rather than estimated when the host cannot expose the
same counter for both arms. Semantic union, physical placement, durable
allocation, device writes, and current-head reachability remain separate
metrics and may not substitute for each other.

For like-for-like incremental semantic payload:

~~~text
payload_reduction_percent
  = 100 * (1 - incremental semantic payload ratio)
~~~

The measured value fills the result. The harness MUST NOT force a target
percentage.

## 13. Content and Workspace mechanism counters

Counters MUST be passive consequences of production work. A counter MUST NOT
trigger a complete scan merely to populate evidence.

### 13.1 FUSE and Workspace

~~~text
dirty interval count
dirty logical bytes
spool materialized and allocated bytes
zero range bytes
captured files and bytes
capture mode
projection remount count
~~~

### 13.2 Content

~~~text
comparison bytes
base and overlay bytes read
CDC bytes scanned
payload bytes read and written
chunks created
extent nodes read and created
extent count before and after
tree level before and after
FileMutationBatch peak and prunes
full-fallback reason
~~~

### 13.3 Namespace

~~~text
paths examined
dirty paths
directory nodes read and created
inode nodes read and created
structural deferred peak and prunes
inode identity preservation result
~~~

### 13.4 Candidate

~~~text
candidate IDs and bytes
inserted IDs and bytes
local reused IDs and bytes
source reused IDs and bytes
admission batches
scratch peak and bytes written
ObjectBuffer spill state
~~~

## 14. New-file and opaque replacement optimization contract

A genuinely new file and opaque temp-copy-rename may require one complete input
scan. They MUST still use a bounded one-pass candidate path.

The desired production flow is:

~~~text
FastCDC and canonical encoder
  -> bounded ID/object batch
  -> local and immutable-parent membership
  -> drop source-present candidate bytes
  -> admit only objects missing from both BranchStore and the admissible
     immutable parent route
  -> retain only required structural construction state
~~~

The path MUST NOT spill another complete file before learning that most
objects already exist through Reference.

For temp-copy prepend:

~~~text
complete input scan count <= 1
CDC bytes <= final file size
scratch peak <= 8 MiB plus one bounded batch
scratch bytes written <= 1 MiB
non-overlapping source-reused logical payload coverage
  >= final file size - 1 MiB
BranchStore inserted canonical bytes <= 1 MiB
~~~

Candidate scratch means all candidate-owned temporary file, spill, and SQLite
writes after S4, excluding the separately measured application Workspace spool.
Moving scratch into another temporary database does not remove it from this
counter. BranchStore inserted canonical bytes include payload, extent/tree,
inode, directory, and namespace objects. Source-reused logical coverage is a
non-overlapping file-range union; repeated references cannot be double-counted.

The application editor phase remains linear because it copied the file. Commit
and storage need not add another file-sized copy.

## 15. FUSE materialization and projection

All rows in this section use a real container FUSE projection. The benchmark
MUST NOT use Host placement or `WorkspaceProjection::Materialize` for these
rows.

### 15.1 Exact public request

LayerFS FUSE materialization uses:

~~~rust
client.create_workspace_session(CreateWorkspaceSession {
    branch_id,
    placement: WorkspacePlacement::Container {
        container_id,
        root: mount_root,
    },
    projection: Some(WorkspaceProjection::Fuse),
})?
~~~

The request pins the immutable Branch root, starts the production proxy,
copies/starts the production FUSE helper in the admitted target container,
waits for mount readiness, and exposes the namespace at `mount_root`. It MUST
not eagerly reconstruct, copy, hash, or CDC-scan the complete root.

### 15.2 Cold, warm, and incremental rows

These rows compare equivalent public product experiences where Computer
provides the corresponding boundary:

| ID | Starting state | Measured boundary |
|---|---|---|
| FUSE-COLD-MOUNT-32M | Fresh container, process, connection, Workspace, helper, and mount; seeded durable authority | Container start through first exact stat and registered first read |
| FUSE-WARM-MOUNT-32M | Fresh Workspace/helper/mount; same durable root; explicitly proven warm Store and OS cache state | Public Workspace creation through first exact stat/read |
| FUSE-WARM-NOCHANGE | Same active Workspace and mount; no Commit or authority change | Repeated exact stat/read with no remount |
| FUSE-NEXT-CHECKPOINT-10B | Natural product state after one durable ten-byte edit | Public route through the next root's mount readiness and exact changed-byte read |
| FUSE-INCREMENTAL-APPEND-4K | Durable root after 4 KiB append | New root projection through exact appended-byte read |
| FUSE-INCREMENTAL-TRUNCATE-4K | Durable root after 4 KiB shrink | New root projection through exact size/tail verification |
| FUSE-INCREMENTAL-RENAME | Durable namespace-only rename | New root projection through old-path absence and new-path exact read |
| FUSE-INCREMENTAL-TEMP-PREPEND | Durable editor temp-copy-rename result | New root projection through exact prefix and digest read |
| FUSE-FULL-REWRITE | Durable unrelated 32 MiB replacement | New root projection through exact first and complete read |
| FUSE-FIRST-READ-32M | Newly ready mount with registered cache axes | First complete FUSE read |
| FUSE-REPEAT-READ-32M | Same active mount immediately after the registered first read | Second complete FUSE read |
| FUSE-PROCESS-COLD | Durable state retained; all candidate processes, connections, helpers, and mounts replaced | Fresh container/reconnect through exact readable projection |

Every cache-state axis from section 7 MUST appear in these rows. Cold, proven
warm, and uncontrolled cache samples are never pooled.

Protocol 0.3 measures FUSE-NEXT-CHECKPOINT and incremental-root rows on the
same active Workspace/FUSE mount after the in-place Commit rebase. Fresh-mount
rows still create and attach a new public Workspace. Any fallback reload or
remount is reported separately and invalidates a same-mount row.

### 15.3 Required FUSE phase evidence

Record non-overlapping or explicitly overlapping timers for:

~~~text
container_start_ns
client_connect_ns
root_pin_ns
workspace_registry_ns
proxy_start_ns
helper_copy_ns
helper_start_ns
mount_ready_ns
projection_attach_ns
first_lookup_ns
first_stat_ns
first_read_ns
complete_read_ns
projection_pause_ns
projection_unmount_ns
projection_cleanup_ns
fuse_materialization_unattributed_ns
~~~

Record at least:

~~~text
selected immutable root
mount path
mountinfo filesystem type and options
helper PID and exact binary SHA-256
proxy endpoint identity
mount attempts and failures
bytes eagerly reconstructed during attach
canonical bytes copied during attach
payload bytes read before first registered access
payload bytes read by first and repeated access
complete-file hashes or CDC bytes during attach
kernel page-fault and process I/O deltas
residual helper, mount, proxy, and temporary paths
~~~

Counters MUST be passive consequences of required product work. They must not
read the tree merely to prove that the tree was not read.

### 15.4 FUSE validity requirements

A FUSE materialization row is valid only when:

1. Linux `/proc/self/mountinfo` inside the target container proves a real FUSE
   mount at the registered path;
2. the exact selected immutable root is recorded before attachment;
3. first lookup, stat, read, namespace, size, and digest oracles pass;
4. mount readiness occurs before the registered workload begins;
5. attach performs zero eager complete-root reconstruction or payload copy;
6. Reference attachment copies zero authority-owned base objects into
   BranchStore;
7. a ten-byte next-root projection performs no complete-file hash or CDC scan;
8. every helper/proxy/mount created by the row is removed or intentionally
   retained by the registered warm scenario; and
9. fresh-process recovery uses a second fresh admitted container over the
   retained Store files.

An outer `docker exec` may start the sealed benchmark controller. The
controller MUST invoke every registered filesystem command through
`Client::exec_workspace_session`. Calling the workload through a benchmark
`docker exec`, including the current `timed_docker` shortcut, invalidates the
row. Docker calls made by the production Container projection to manage the
FUSE helper remain legitimate product work and are timed.

## 16. Resource measurements

Each candidate process group SHOULD record:

~~~text
wall time
user and system CPU
maximum RSS
cgroup memory.current and memory.peak
minor and major page faults
voluntary and involuntary context switches
/proc process I/O counters
cgroup io.stat bytes and operations
temporary scratch peak
Workspace spool peak
SQLite cache configuration
~~~

All candidate-owned control, FUSE, proxy, computerd, helper, and benchmark
controller processes are in scope.

If Docker Desktop does not expose reliable physical I/O, the report MUST retain
logical counters and mark physical I/O unavailable. SQLite payload or wall time
must not be relabeled physical writes.

LayerFS optimization bounds:

~~~text
FileMutationBatch deferred bytes < 8 MiB
inode deferred bytes < 8 MiB
directory deferred bytes < 8 MiB
candidate memory <= 8 MiB before bounded spill/admission
transfer peak < 34 MiB
object batch <= 128 objects and <= 4 MiB
fact batch <= 128 facts and <= 64 KiB
history page <= 128 facts
membership page <= 512 IDs
~~~

Dirty interval metadata MUST count against the Workspace final-delta limit.
Exceeding a reported bound fails LayerFS optimization; it does not invalidate
otherwise complete empirical evidence.

## 17. Campaign protocol

| Campaign | Complete adjacent pairs | Use |
|---|---:|---|
| Self-check | 0 | Schema, oracle, helper, and verifier validation |
| Smoke | 1 | Functional and evidence-path validation only |
| Pilot | 10 | Method and variance qualification; no final claim |
| Core formal | 30 | Headline E2E latency and storage |
| Extended formal | 10 | Scale, checkpoint-frequency, and FUSE-materialization sections |

Each pair:

1. uses fresh candidate state;
2. uses the same fixture, helper, envelope, and host;
3. runs candidates adjacently;
4. randomizes order from a frozen seed;
5. retains all raw output and failures.

The formal schedule MUST be balanced and written and hashed before execution.
It cannot change because of latency, failure, temperature, or an unfavorable
result.

No valid outlier is removed. A failed arm invalidates the campaign for
publication and remains immutable evidence. A corrected attempt receives a new
run ID.

## 18. Statistics

For each trial, derive:

~~~text
cold create
sixteen-edit total
append
truncate shrink
truncate grow
rename
temp prepend
rewrite
read
registered core total
~~~

Totals are derived within each trial before aggregation. Independent medians
are never summed.

For each operation group and candidate, report:

- count;
- median;
- Q1 and Q3;
- minimum and maximum.

Quartiles use:

~~~text
index = (n - 1) * q
~~~

The primary paired values are:

~~~text
latency ratio
  = LayerFS complete_scenario_ns / Computer complete_scenario_ns

speedup
  = Computer complete_scenario_ns / LayerFS complete_scenario_ns
~~~

A latency ratio below 1 means LayerFS is faster. A speedup above 1 means
LayerFS is faster.

Report median paired ratio and speedup, Q1/Q3, deterministic paired-bootstrap
95 percent confidence intervals, pair wins/ties/losses, and all per-pair orders
and totals.

Bootstrap behavior is frozen:

~~~text
resampling unit: one complete adjacent pair
latency sample: LayerFS complete_scenario_ns / Computer complete_scenario_ns
storage sample: the exact per-pair ratio defined in section 12.5
statistic: median of the resampled paired ratios
interval: percentile 95 percent interval
resamples: 100,000
seed bytes: first 8 bytes, big-endian, of
  SHA-256("fs-bench-plus/bootstrap/v1\0" + run_id + "\0" + metric_id)
PRNG: SplitMix64
index mapping: floor(next_u64 * pair_count / 2^64), using u128 arithmetic
quantiles: linear interpolation at index (n - 1) * q
gate arithmetic: unrounded IEEE-754 binary64 values
display: decimal rounding only after verdict calculation
~~~

The canonical metric IDs used by terminal CI gates are exactly:

~~~text
latency.cold_create_32m
latency.edit16_k1
latency.same_byte_noop
latency.append_10b
latency.truncate_shrink_4k
latency.truncate_grow_4k
latency.rename_only
latency.prepend_temp_rename
latency.rewrite_32m
latency.read_sync_32m
latency.fuse_cold_mount_32m
latency.fuse_warm_mount_32m
latency.fuse_warm_nochange
latency.fuse_next_checkpoint_10b
latency.fuse_incremental_append_4k
latency.fuse_incremental_truncate_4k
latency.fuse_incremental_rename
latency.fuse_incremental_temp_prepend
latency.fuse_full_rewrite
latency.fuse_first_read_32m
latency.fuse_repeat_read_32m
latency.fuse_process_cold
latency.registered_core_total
storage.edit16.incremental_semantic_payload_ratio
storage.edit16.durable_allocation_ratio
storage.edit16.physical_write_ratio
storage.edit16.current_head_semantic_storage_ratio
~~~

They are schema enum values, not operator labels. The manifest freezes the
ordered list and its SHA-256 before execution. The reporter rejects aliases,
unknown IDs, duplicate IDs, or a post-admission ID-list change. Human-facing
labels never enter seed derivation.

Each resample draws `pair_count` pairs with replacement and then calculates
the median. The reporter records run ID, metric ID, seed, method, and resample
count. A zero or negative denominator produces null plus its exact reason and
fails the affected optimization gate; it is not silently replaced. A negative
semantic-retention delta is an invalid accounting result.

Latency-ratio confidence intervals are authoritative for gates. The displayed
speedup interval is the reciprocal transform `[1 / ratio_upper,
1 / ratio_lower]`; the reporter does not run a second independently seeded
bootstrap to seek a more favorable interval.

Smoke ratios are diagnostic. A smoke report MUST not make a statistical
superiority claim.

## 19. Correctness gates

Every report has two independent verdict axes:

~~~text
Empirical evidence: VALID or INVALID
LayerFS optimization: PASS or FAIL
~~~

Provenance, public-path, required-evidence, equation, oracle, and durability
violations may make empirical evidence INVALID. A fully measured mechanism,
latency, storage, or resource value outside an optimization gate leaves the
evidence VALID and makes LayerFS optimization FAIL.

Any violation invalidates the campaign:

1. Every final size and SHA-256 matches an independent oracle.
2. Namespace, mode, time, symlink, and hard-link topology match the scenario.
3. Every acknowledged state passes public-SDK fresh-process reopen.
4. Every retained LayerFS root is authenticated and readable.
5. SAME-BYTE-NOOP creates no Commit and sends no object or fact.
6. Clean truncate shrink persists even with no dirty byte range.
7. Same-path unlink and recreate uses the correct new inode identity.
8. Pure rename preserves source identity and content.
9. Rename-over preserves source identity and removes old target identity.
10. Interrupted transfer never publishes a Branch head.
11. Retry repairs an incomplete closure even if a root row exists.
12. Root-object presence is never treated as a complete-root receipt.
13. Push sends zero pulled ancestry facts and zero authority-owned base
    canonical payload bytes.
14. Candidate and transfer equations balance.
15. No durable write transaction contains network, CDC, hashing, history, or
    closure enumeration.
16. Both arms use the admitted public SDK/API and real FUSE path.
17. No unexpected FUSE helper, proxy, mount, or temporary lifecycle residue
    remains.

## 20. Mechanism gates

This section applies to LayerFS only and determines the LayerFS optimization
verdict. A present value outside a threshold is a valid measured failure, not
invalid evidence.

### 20.1 Ten-byte overwrite

Every EDIT16-K1 edit MUST satisfy:

~~~text
comparison bytes = 10
CDC bytes scanned = 10
unchanged payload bytes read = 0
dirty logical bytes = 10
file nodes read < 32
candidate IDs < 128
candidate bytes < 256 KiB
scratch bytes written < 256 KiB
projection remounts are reported
sent bytes = exact authority-missing frontier
~~~

One membership round trip is a preferred target, not a hard gate. Membership
request count, IDs, and bytes MUST be reported; pre-enumerating a complete root
or history merely to reduce round trips is forbidden.

### 20.2 File-size independence

For 4, 32, and 256 MiB:

~~~text
CDC bytes = 10
unchanged payload read = 0
candidate bytes <= 256 KiB
candidate bytes at 256 MiB <= 2 * candidate bytes at 4 MiB
announced bytes at 256 MiB <= 2 * announced bytes at 4 MiB
Commit median at 256 MiB <= 1.50 * Commit median at 4 MiB
~~~

Hard memory acceptance uses the absolute production-accounted bounds in
section 16 over Workspace interval metadata, FileMutationBatch deferred state,
candidate object buffers, transfer buffers, and Seen in-memory state. RSS,
cgroup peak, SQLite cache, and kernel page cache remain diagnostics and are not
used in a gameable small-file/large-file memory ratio.

### 20.3 No-op

~~~text
comparison bytes = 10
CDC bytes = 0
candidate IDs and bytes = 0
Commit = UpToDate
Push = NoChanges
sent bytes = 0
~~~

### 20.4 Append and truncate

~~~text
APPEND-10B:
  CDC bytes = 10
  unchanged payload read = 0

TRUNCATE-SHRINK-4K:
  CDC bytes = 0
  payload read = 0

TRUNCATE-GROW-4K:
  CDC bytes <= 4 KiB
  unchanged payload read = 0
~~~

Each LayerFS candidate remains below 256 KiB.

### 20.5 Rename

~~~text
CDC bytes = 0
payload bytes read = 0
payload bytes written = 0
candidate bytes < 256 KiB
inode identity preserved
~~~

### 20.6 Dense rewrite

~~~text
CDC bytes = final file size
complete payload scan count = 1
scratch remains bounded
~~~

Dense rewrite may be linear, but it may not scan or spool the complete payload
twice because admission is delayed.

### 20.7 Temp-copy prepend

~~~text
old editor bytes read >= input file bytes
temporary bytes written >= input file bytes + 10
complete candidate input scan count <= 1
CDC bytes <= final file size
candidate scratch bytes written <= 1 MiB
non-overlapping source-reused logical payload coverage
  >= final file size - 1 MiB
BranchStore inserted canonical bytes <= 1 MiB
~~~

The editor phase is linear and MUST NOT be claimed file-size-independent.

## 21. FUSE materialization gates

These gates apply to the Container-placement `WorkspaceProjection::Fuse` rows
in section 15. Host directory export is not an admissible substitute.

Every FUSE materialization row requires:

~~~text
admitted image labels and architecture match
registered fresh or warm container state
exact immutable root pinned before attach
real FUSE mount proven by in-container mountinfo
mount ready before registered workload
exact namespace, size, metadata, and content oracle
registered cache axes
zero workload execution through direct docker exec
no unexpected helper, proxy, mount, or temporary residue
~~~

FUSE-COLD-MOUNT-32M additionally requires:

~~~text
container/process/connection/Workspace/helper/mount all fresh
bytes eagerly reconstructed during attach = 0
canonical payload copied during attach = 0
complete-file hash bytes during attach = 0
CDC bytes during attach = 0
first stat and registered first read exact
~~~

FUSE-WARM-MOUNT-32M requires a fresh Workspace/helper/mount and the same durable
root. Store, SQLite, fixture-source, and OS page-cache warmth must each be
independently proven or labeled uncontrolled. A hidden surviving mount is an
invalid warm row.

FUSE-WARM-NOCHANGE requires:

~~~text
same active Workspace and mount
same immutable root
helper restarts = 0
projection remounts = 0
canonical or Store writes = 0
repeated exact stat/read
~~~

FUSE-NEXT-CHECKPOINT-10B requires:

~~~text
exact old and new root IDs
new public Workspace/FUSE projection when required by the SDK lifecycle
full-file hash bytes during attach = 0
CDC bytes during attach = 0
authority base canonical bytes copied into BranchStore = 0
changed bytes and complete digest exact
~~~

Append, truncate, rename, temp-prepend, and full-rewrite projection rows must
expose their exact acknowledged root and satisfy their operation-specific
oracles. Projection attachment itself must remain independent of complete
payload size; the later registered complete read is accounted separately.

FUSE-PROCESS-COLD requires a second fresh admitted container with fresh
processes, connection, Workspace, helper, and mount over retained durable Store
files. It must reproduce the acknowledged root without the measurement
container or its kernel/FUSE state.

FUSE-FIRST-READ-32M and FUSE-REPEAT-READ-32M must report distinct cache states,
page faults, process/cgroup I/O, complete bytes, and digest. The second result
must not be called a cold read.

## 22. Performance and storage gates

Performance does not determine whether evidence is valid. A slower LayerFS
campaign remains valid evidence, but its LayerFS optimization verdict is FAIL.
It MUST NOT be described as a terminal optimization pass or a performance
victory.

Define the paired latency ratio as:

~~~text
latency ratio = LayerFS complete_scenario_ns / Computer complete_scenario_ns
~~~

Lower is better. For every primary row with equivalent public boundaries, the
30-pair formal campaign MUST satisfy both:

~~~text
median latency ratio < 1.00
paired-bootstrap 95 percent CI upper bound < 1.00
~~~

There is no permitted primary-row loss hidden by a faster aggregate.

The stronger terminal optimization gates are:

| Workload class | Median latency ratio | 95 percent CI upper bound | Equivalent minimum median speedup |
|---|---:|---:|---:|
| EDIT16-K1 | <= 0.50 | <= 0.67 | >= 2.00x |
| SAME-BYTE-NOOP, APPEND-10B, both TRUNCATE rows, RENAME-ONLY | <= 0.50 | <= 0.67 | >= 2.00x |
| PREPEND-TEMP-RENAME | <= 0.67 | <= 0.80 | >= 1.50x |
| COLD-CREATE-32M, REWRITE-32M, READ-SYNC-32M | <= 0.80 | < 1.00 | >= 1.25x |
| FUSE-COLD-MOUNT-32M, FUSE-PROCESS-COLD | <= 0.80 | < 1.00 | >= 1.25x |
| FUSE-NEXT-CHECKPOINT-10B and incremental append/truncate/rename rows | <= 0.50 | <= 0.67 | >= 2.00x |
| FUSE-FIRST-READ-32M, FUSE-REPEAT-READ-32M | <= 0.80 | < 1.00 | >= 1.25x |
| Registered core total | <= 0.67 | <= 0.80 | >= 1.50x |

These are acceptance gates, not constants to tune the implementation around.
The report always prints the measured distributions and MUST NOT clamp,
replace, or selectively rerun a value to meet them.

LayerFS hard edit targets are:

~~~text
median EDIT16 complete_scenario / 16 <= 75 ms per edit
median EDIT16 complete_scenario <= 1.200 s
~~~

The preferred edit target is:

~~~text
median EDIT16 complete_scenario / 16 <= 50 ms per edit
median EDIT16 complete_scenario <= 0.800 s
median paired speedup >= 3.00
~~~

For the public SDK execution phase, before Commit and Push are added, the
32 MiB sequential COLD-CREATE, REWRITE, and READ rows MUST each sustain at
least 500 MiB/s. Authority-to-stable effective throughput is reported
separately as a diagnostic; the relative gates above use complete public
scenario time. The benchmark MUST NOT present execution-only throughput as
durable checkpoint throughput.

Storage gates are independent:

~~~text
Reference seed canonical bytes copied into BranchStore = 0
EDIT16 incremental semantic payload ratio CI upper bound <= 0.05
EDIT16 target median incremental semantic payload ratio <= 0.016
matched-quiesced EDIT16 durable allocation ratio CI upper bound < 0.90
EDIT16 physical write ratio CI upper bound < 0.90, when available
current-head semantic storage ratio <= 1.00
~~~

Every ratio above uses the exact per-pair numerator, denominator, and snapshot
defined in section 12.5. Favorable selection among S5 branch-only insertion,
S6 union growth, two-placement physical growth, or receipt candidate bytes is
forbidden.

The 0.016 storage target corresponds to approximately 98.4 percent less
incremental semantic payload, but the phrase may be used only when the measured
median and CI upper bound are both at most 0.016. If semantic storage passes and
physical allocation fails, the result is mixed, not a storage victory.

## 23. Required reports

### 23.1 Comparable durable latency

| Workload | Computer API | Computer stable | Computer complete | LayerFS SDK exec | LayerFS Commit | LayerFS Push | LayerFS stable | LayerFS complete | Speedup | 95 percent CI | Optimization verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|

### 23.2 LayerFS phase decomposition

| Workload | Create | Exec | Output wait | Pause | Capture | Compare | Content | Namespace | Admission | Publish | Projection | Push |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|

### 23.3 Algorithm work

| Workload | File bytes | Dirty bytes | Compared bytes | CDC bytes | Payload read | Candidate IDs | Candidate bytes | Source reused | Scratch |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|

### 23.4 Incremental storage

| Boundary | Computer blob growth | Authority-only | Branch-only | Shared | Union growth | Placement growth | Reduction |
|---|---:|---:|---:|---:|---:|---:|---:|

### 23.5 Physical allocation

| Boundary | Candidate | Store | DB apparent | WAL | SHM | DB allocated | WAL allocated | SHM allocated |
|---|---|---|---:|---:|---:|---:|---:|---:|

### 23.6 Transfer

| Workload | Owned Commits | Roots | Equal prunes | Visited | Announced bytes | Sent bytes | Avoided | Pages | Verifier bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|

### 23.7 Size scale

| File size | Computer stable | LayerFS stable | Speedup | Commit | CDC bytes | Candidate bytes | Memory peak |
|---:|---:|---:|---:|---:|---:|---:|---:|

### 23.8 Checkpoint frequency

| Edits/checkpoint | Checkpoints | Computer total | LayerFS total | Speedup | Commit bytes | Fact bytes |
|---:|---:|---:|---:|---:|---:|---:|

### 23.9 Cache and FUSE materialization

| Workload | Image | Container | Process | Connection | Workspace | Mount/root | OS cache | Attach | First stat | First read | Eager bytes | Total | Exact |
|---|---|---|---|---|---|---|---|---:|---:|---:|---:|---:|---|

### 23.10 History and recovery

| Candidate | Retained roots | Oldest readable | Latest readable | Final bytes | SHA-256 | Reopen |
|---|---:|---|---|---:|---|---|

### 23.11 Provenance

| Candidate | Repository | Commit | Tree | Source seal | Dirty | Image | Build mode |
|---|---|---|---|---|---|---|---|

### 23.12 Verdicts

| Empirical evidence | LayerFS optimization | Blocking evidence reason | Failed optimization gates |
|---|---|---|---|

The report MUST state:

~~~text
Article multi-agent branch-exclusive result:
N/A — multi-agent benchmark intentionally excluded.
~~~

## 24. Evidence custody

Every campaign uses an exclusively created results directory. Nothing is
overwritten.

Raw evidence MUST live under `raw/`; comparison inputs and reports MUST live
under `derived/`. The frozen manifest is at the run root. This separation makes
the raw inventory membership unambiguous.

Required evidence:

~~~text
manifest.json
terminal.json
fixture and helper hashes
frozen randomized schedule
environment and cgroup data
source archives, pins, seals, diffs, and image labels
candidate image tags, immutable digests, architectures, and OCI label checks
measurement and recovery container IDs and inspect records
Docker resource flags and effective cgroup limits
FUSE helper binary SHA-256, PID, proxy identity, mount path, and lifecycle log
in-container FUSE mountinfo proof
public SDK/API request and receipt records
execution output and terminal receipts
phase timings
mechanism counters
storage snapshots S0 through S8
durability barriers
fresh-process reopen proof
oracles
raw comparison input
derived JSON and Markdown reports
raw-inventory.sha256
~~~

The custody chain is:

~~~text
frozen manifest hash
  -> canonical raw-only SHA-256 inventory
  -> comparison input bound to manifest and raw-inventory hashes
  -> JSON report bound to comparison-input hash
  -> Markdown report bound to JSON-report hash
  -> terminal.json recording every hash above
~~~

The raw inventory is sealed after all candidate arms and raw verifiers finish
and before comparison input or reports are generated. It excludes the
inventory file itself, comparison inputs, reports, and terminal.json. Entries
are UTF-8 relative paths sorted by raw path bytes, one lower-case SHA-256,
single space, and path per line. Symlinks are forbidden in the evidence tree.

The reporter MUST verify the frozen manifest hash and reject every missing,
extra, or mismatched raw file before deriving a result. `comparison-input.json`
embeds the manifest and raw-inventory hashes. The JSON report embeds the
comparison-input hash; the Markdown report embeds the JSON-report hash.
`terminal.json` records all of them and the selected derived artifact paths.

A reporter bug is fixed in the reporter and rerun against unchanged sealed raw
evidence. Each rerun creates new derived artifact names and retains the earlier
derived files. Raw evidence and its inventory are never rewritten.

terminal.json has independent fields:

~~~text
run_status:
  COMPLETE
  FAILED_CANDIDATE
  FAILED_ORACLE
  FAILED_DURABILITY
  FAILED_ENVIRONMENT

evidence_verdict:
  VALID
  INVALID

layerfs_optimization_verdict:
  PASS
  FAIL
  NOT_EVALUATED
~~~

Unavailable is null plus a reason. It never means zero.

## 25. Invalid-result conditions

The campaign is invalid if:

- a required operation, counter, or receipt is missing;
- a required equation fails;
- fixtures or oracles differ;
- Reference setup copied base objects into BranchStore;
- either arm bypassed its public path or real FUSE;
- candidate order differs from the frozen schedule;
- an arm was silently rerun;
- process reopen was skipped;
- semantic and allocated storage were mixed;
- warm and cold samples were pooled;
- a cold cache claim lacks proof;
- a hidden seed or candidate cache was used;
- an internal splice replaced the public editor prepend;
- product code contains benchmark specialization;
- the source seal changed during execution.

A present, correctly measured LayerFS value outside a resource, mechanism,
FUSE-materialization, performance, or storage threshold is not on this
invalidity list. It produces `evidence_verdict=VALID` and
`layerfs_optimization_verdict=FAIL`.

## 26. Required interpretation

A valid report may conclude:

~~~text
LayerFS is faster on the registered public-SDK authority-durable workload.
LayerFS adds fewer canonical bytes per checkpoint.
LayerFS small public edits remain independent of complete file size.
LayerFS Reference avoids copying the authority base.
LayerFS exposed the authenticated root through real FUSE without eager payload
reconstruction under the named cache and container state.
~~~

It may not conclude:

~~~text
LayerFS reproduced the article's multi-agent 98.4 percent result.
LayerFS reproduced C3's 3.18x result.
Internal semantic splice equals editor prepend.
A fresh process proves an evicted OS page cache.
Low canonical payload proves low physical device writes.
A hidden surviving mount or pre-read target is a fair warm cache.
~~~

The benchmark is successful only when correctness, public SDK latency,
mechanism counters, transfer, storage, FUSE materialization, and fresh-process
durability pass together. High final CAS reuse after file-sized candidate work
is evidence of avoidable amplification, not an optimized edit.
