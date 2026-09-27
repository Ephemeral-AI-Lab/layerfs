# Issue 261: three mounted 100-write diagnostic cases

> **Status:** Prospective workload and evidence specification. Commit this before
> changing the writer, runner, verifier, or taking a sample.

## Question and common route

Compare the work caused by 100 true appends, 100 dispersed edits, and 100
overwrites of one byte. All three use one prepared old head with one existing
`data.bin` containing exactly 10 MiB (10,485,760 bytes) of `A`, mode 0644.
Each selection takes an independent writable byte copy of the same closed,
validated Store/history master. One public `WorkspaceApi::mount`, one
`WorkspaceApi::exec`, and one explicit `WorkspaceApi::commit` form the measured
route. The daemon launches `/bin/sh -c` once; that shell launches one writer
outside the mount. The writer opens `data.bin` once, makes 100 successful
one-byte file syscalls on that fd, closes it, and emits only progress at
25/50/75/100 writes. The driver makes no internal mutation calls.

The case order is append, dispersed, repeated. Each is one labelled diagnostic
and gets at most one attempt per frozen source identity. They are different
workloads, not replacements for the retained 8,194-byte separated-offset row
or the #248 4,097-write gate. A failure, timeout, or ineligible row stays
append-only and is never resampled to select a better number.

| Case ID | Writer syscall and offset | Final size | Expected changed runs |
| --- | --- | ---: | ---: |
| `issue261-append-100-10m-v1` | `open(O_APPEND)` then `write(fd, &byte, 1)` 100 times at EOF | 10,485,860 | 1 appended run |
| `issue261-dispersed-100-10m-v1` | `pwrite(fd, &byte, 1, (104729 + i*2654435761) % 10485760)` for `i=0..99` | 10,485,760 | 100 isolated runs |
| `issue261-repeated-100-10m-v1` | `pwrite(fd, &byte, 1, 5242880)` 100 times | 10,485,760 | 1 one-byte run |

For all cases, syscall `i` writes byte `B + (i % 24)` for `i=0..99`.
The dispersed formula is a deterministic full-period permutation of the
10 MiB byte positions (`gcd(2654435761, 10485760) = 1`); the selected 100
positions are distinct, interior, and nonadjacent. This fixed, spatially
dispersed schedule is the reproducible random-edit workload. Every repeated
write changes the currently visible byte, and the final byte is the value
from syscall 99. The append case grows the file by exactly 100 bytes.
The source model predicts final Local/Base extent counts of 101, 201, and 3
respectively; these are predictions to reconcile with observed Commit counts,
not a substitute for an independent oracle.

## Preparation, measurements, and limits

Prepare the 10 MiB fixture and its old branch head once outside all timers,
verify that master read-only, then clone its closed Store/history with
`shutil.copyfile` for each case. Clone is setup reuse and makes no cold-cache
claim. Reuse sealed release `benchmark_init`, `benchmark_shell`, daemon and
verifier binaries when their compilation inputs match; rebuild affected
binaries with locked release Cargo. The writer is a statically linked Linux
helper in the image. Pin source commit/tree, product/compilation/dependency and
harness seals, spec and writer hashes, fixture/master/clone hashes, image ID,
binary hashes, build flags, and `LAYERFS_CONSTRUCTION_WORKERS=1`.

Measure monotonic writer progress, public Exec and Commit walls, complete
command wall (driver launch through cleanup), verifier wall separately, mount
and cleanup walls, output bytes/frames where exported, actual FUSE callback
classes, private payload and ownership I/O, metadata page reads, extent-tree
height/page work, control and Service calls, Store work, resource charges,
process/cgroup resource domains where available, and clean custody release.
For count attribution, enable the existing 25-write FUSE backing snapshots and
Commit complexity logs for each diagnostic; record their observer overhead.
No per-write status/control call or output is added. Exact FUSE FLUSH/RELEASE,
transport frames, cumulative extent-only page writes, Store page writes and
physical-device bytes are unavailable unless the source exports them; never
impute zero.

Each complete diagnostic command has a 15 s limit. The independent verifier
has a 9 s limit and runs after the complete command. Each focused test command
stays under 30 s. No product Exec deadline, worker count, fixture, cache policy,
or limit changes after observing a row. The existing #249 30 s product Exec
timer and #248 4,097 gate remain separate.

Ordinary host/container cache is uncontrolled, with no priming or warm-cache
credit. Each row is `admission_eligible=false` and numeric latency is
`INELIGIBLE`; raw walls can guide diagnosis but cannot prove that one pattern
is faster. Compare within-run work counts and source-derived complexity first.
Keep all failed and ineligible receipts with exact commands, raw stdout/stderr,
status, cleanup, and SHA-256 manifests.

## Independent oracle and diagnosis

The verifier reopens each cloned Store/history read-only, identifies the old
and new Commit and exact parent, inventories all paths and metadata, reads all
old and new `data.bin` bytes, and applies the declared schedule independently
of the writer output. It checks the expected size, every byte, changed-run
count, and no extra path. A successful driver exit alone is not a proof.

After the three retained attempts, reconcile observed Exec/Commit/complete
wall and quartile counts with a source model for FUSE callbacks, payload
acquisition/reclaim, B+ extent paths and custody edges, control/Service trips,
Store work, memory/disk charging, and cleanup. Distinguish observed counts,
source predictions, and unavailable measurements. In particular, repeated
overwrites may leave only one final changed run for Commit while still paying
for 100 atomic per-write publications during Exec.
