# Issue 261 public mounted-write workload and evidence contract

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This prospective specification is based on repair commit
`6af2c5c59a48d0b6c85d656e55aecc353e346728` and issue #261. It is committed
before implementing the runner or executing either workload. The question is
where separated writes spend time on the real mounted public route and whether
a source change removes the demonstrated cost. A diagnostic does not support a
latency or release-admission claim.

## Selection and public operation

| Field | Frozen value |
| --- | --- |
| Family | `workspace_mounted_separated_writes` |
| Scenario/version | `issue261-separated-100-v1` diagnostic; `issue248-separated-4097-v1` subsequent public gate |
| Selection | One attempt per scenario, per source identity; 100 first; 4,097 only after diagnosis and a meaningful frozen fix |
| Operation surface | Public `WorkspaceApi::mount` → one `WorkspaceApi::exec` → one explicit `WorkspaceApi::commit` |
| Orchestration/mutation | Host SDK driver; Linux daemon, FUSE, one shell-launched writer and kernel file syscalls mutate the mounted Workspace |
| Acknowledgement | Successful Exec exit followed by a committed head returned from the single Commit |
| Forbidden driver calls | Direct Workspace payload/write methods, FUSE callbacks, Bridge, Service, C1 or Store mutation methods to create writes |

The SDK driver reuses the release `benchmark_shell` lifecycle and case grammar.
There is one sandbox and mount, one Exec, one Commit, then status, unmount and
sandbox deletion. The Exec command is `/fixtures/bin/write-separated data.bin
100` or the identical command with `4097`. It starts one writer process outside
the mount. The writer opens `data.bin` once, issues exactly `N` successful
one-byte `pwrite` calls on that fd at offsets `2*i` for `i=0..N-1`, then closes
it. It writes byte `X` over initial byte `A`. It neither launches `dd` nor
emits per-write output or status RPC. It emits four cumulative progress records
after writes 25/50/75/100 for the diagnostic; gate progress is recorded at
prospectively fixed quartiles. Each progress record has the completed count and
monotonic elapsed time; output transport is counted. Every write is product
work inside Exec, including atomic Workspace publication and charging.

## Fixture, image and cache

`data.bin` exists in the old committed head and contains exactly 8,194 `A`
bytes, mode 0644. The expected new image has `X` at offsets `2*i` for the
selected `N`, `A` everywhere else, unchanged length and mode, and exactly `N`
separated one-byte changed runs. Initial namespace/Store/history preparation,
branch creation and old-head verification are outside measurement. Use one
closed, validated prepared master and an independently writable byte copy of
the Store and history for each attempt (`shutil.copyfile`), never a mutated
sample or a hard link. Record master and clone digests and copy method. The
same file contents, image writer and case grammar apply to both counts.

The SDK, SQLite, Store and Service run on the macOS host; only the Linux
daemon, FUSE and writer run in Docker. Pin the locked Cargo **release** SDK
driver, verifier and daemon binaries, writer executable, image ID, fixture,
case, source commit/tree, product/compilation/dependency/harness/oracle seals,
build flags and worker setting. `LAYERFS_CONSTRUCTION_WORKERS=1`; neither the
worker count nor the product deadline is adjusted for a result.

Cloning does not establish cold residency. This diagnostic declares ordinary
uncontrolled host and container page cache, with no warming or priming;
`admission_eligible=false`, `cache_status=INELIGIBLE` for numeric latency.
Any later cache-qualified performance arm requires a separately committed
scenario/cache contract and matching arms. The 4,097 public gate retains this
cache declaration, so it can prove function and report raw wall, not claim a
cache-qualified speed PASS. No cold and warm rows are pooled.

## Timers, limits and retention

Use monotonic clocks. `exec_ns` starts immediately before the public Exec call
and ends on its response; `commit_ns` analogously wraps the public Commit call.
`complete_command_wall_ns` is the external driver launch through process exit,
including sandbox lifecycle, cleanup and receipt emission. Record mount,
cleanup, verifier and fixture preparation separately. The independent verifier
runs after the complete command, never within either product timer. Record
driver exit and timeout even if a partial receipt is unavailable.

The 100-write diagnostic and 4,097 gate each have a 15 s complete-command
performance budget; a prospective 25 s exception is declared for the 4,097
gate only. The functional harness ceiling for that gate remains 60 s, and the
current 30 s product Exec deadline remains owned by #249. A miss or timeout
remains FAIL/NOT_RUN; no limit is enlarged after observation. Every focused
test command has a 30 s ceiling. Never repeat an unchanged performance arm or
discard an attempt. Use unique append-only directories for preparation,
diagnostic, gate, verifier, failures and `NOT_RUN` rows. Retain exact commands,
stdout/stderr, process status, raw events, receipt, source/environment
identities, cleanup outcome and SHA-256 manifest.

## Required count attribution

The 100-write attempt is a labelled count-driven diagnostic. Record cumulative
elapsed at 25/50/75/100 and cumulative work counters; the four checkpoints
are within the one run, not four samples. Distinguish observed counts, source
derived bounds and unavailable fields with explicit provenance. Required scopes:

* Public/control: mount/Exec/Commit/status/unmount calls, control frames,
  shell/writer launches, status/progress polls, output frames and any per-write
  RPC. Expected public calls are one mount, one Exec and one Commit.
* Kernel/FUSE: exactly `N` writer `pwrite` syscalls and `N` supplied bytes;
  actual LOOKUP, OPEN, WRITE, FLUSH, RELEASE, GETATTR and READ callbacks,
  callback sizes and elapsed work. Never substitute syscall count for WRITE
  callback count.
* Private payload: acquisitions, segment create/allocate/write/close calls,
  allocated and retained bytes, owner counts, reclaim candidates/scans,
  ledger reads/writes and lock/wait time. Attribute fixed per-write work and
  work whose count grows with prior writes.
* File extent index: before/after live extents, B+ fanout/height, per-splice
  page visits/reads/writes/allocations/reuse, root publications and split
  work. Keep namespace and canonical C1 trees separate.
* Commit/Store: frozen-cursor visits/reseeks, final descriptors/replacement
  bytes, Bridge/Service trips, C1 visits, Store writes/transactions and one
  head publication. Record host/container CPU, RSS/anonymous/file cache,
  private disk, Store/history bytes, resource charges, custody and cleanup.

The source model uses `N` accepted writes, `E` live file extents, `H` extent
tree height, `R` final changed runs and `S` final replacement bytes. For this
fixture `E` and `R` grow with `N`, with `R=N` and `S=N`. Intended write index
work is `O(H + touched)` per accepted write and `O(N log E)` in total, plus
real `O(N)` callback, payload and ledger I/O. Commit traverses final state
with bounded monotone cursors. Report any deviation with loop/call site and
count evidence; one elapsed value cannot establish order. Reconcile an
independent source audit with the retained counts before selecting a fix.

## Independent oracle and gates

The verifier opens the cloned Store/history read-only after driver exit. It
checks old branch head identity and old `data.bin` bytes, new head parent and
root identity, exactly one new head publication, new length/mode/all bytes,
exactly `N` separated changed runs and no extra paths. It must not trust the
driver's returned bytes as its oracle. Verification target is under 10 s and
its wall is separate. PASS requires exact route, counts, bytes, resource and
cleanup evidence; unavailable mandatory evidence is `INCOMPLETE`, a budget
miss is `FAIL`, and uncontrolled cache makes numeric latency `INELIGIBLE`.
The 4,097 gate is attempted once at a frozen prospective source identity only
when the diagnostic mechanism and source repair make the attempt meaningful.
