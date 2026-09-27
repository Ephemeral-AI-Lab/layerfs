# Issue 266: higher-count mounted WRITE refusal

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

This branch starts at #261 draft PR #262 head
`d128d3fa574d4dadce758dd35b9c4af4ecc9c87d` and changes only its own
managed worktree. The historical [#261 512 FAIL](../261/evidence/512-v1-fail/receipt.json)
remains unchanged. The [prospective #266 selection](../../../../docs/roadmap/0.1/0.1.7/issue266-fuse-refusal-spec.md),
[post-reply cause selection](../../../../docs/roadmap/0.1/0.1.7/issue266-post-reply-diagnostic-spec.md),
[treatment](../../../../docs/roadmap/0.1/0.1.7/issue266-reply-wait-treatment-spec.md),
and [shared-count clarification](../../../../docs/roadmap/0.1/0.1.7/issue266-shared-count-route-correction.md)
are separate committed records. The clarification does not revise older
specifications or receipts.

## Shared public route and diagnosis

The 100, 512 and 4,097 selections use the same
[`separated_writes.py`](../../../benchmark/fs-bench-pro/separated_writes.py),
[`benchmark_shell`](../../../crates/layerfs-api/sdk/examples/benchmark_shell.rs),
and [generic one-fd writer](../../../benchmark/fs-bench-pro/writers/write-separated.c).
The public driver makes one `WorkspaceApi::mount`, one
`WorkspaceApi::exec(&mount.id, "/fixtures/bin/write-separated data.bin N")`,
and one explicit `WorkspaceApi::commit` only after successful Exec. The writer
issues one-byte positional writes at offsets `2*i`, `i=0..N-1`. The expected
full 8,194-byte file and exact changed-run count derive from `N`; actual FUSE
WRITE callbacks are counted independently by Workspace Status. No 512-only
mutation route or per-write Service call was added. The old 100 functional
PASS and 512 FAIL were not rerun.

The source audit found Busy exits at projection admission, projected
publication/coherence checks, backing admission/window state, revision/root
validation and shutdown. The first changed-source 512 diagnostic put its
optional snapshots before reply and completed without EBUSY. That did not
identify the historical callback-258 refusal. A second, prospectively
specified cause diagnostic restored the historical optional *post-reply*
snapshot interval while keeping the writer and real mutation path identical.
It returned EBUSY at callback **129**, immediately after 128 accepted writes.
The bounded daemon records name
`begin_projection_mutation` / FUSE `admission`, with revision 128,
`CoherenceStatus::Ready`, bound delivery, `mutation_held=true`, one reply
permit and 9,999,999,083 ns left in the existing callback deadline. The
one-fd writer's next syscall could start only after the preceding WRITE reply
attempt. The source therefore identifies a healthy prior origin permit held
during the post-reply snapshot as this diagnostic's precise refusal cause.
It is compatible with the old callback-258 failure, whose exact branch was
never logged; the old receipt is not retrospectively relabelled.

The fix marks a projected WRITE just before its reply attempt. If and only if
the preceding mutator is at that boundary, is the sole permit holder and has
Ready, bound coherence, the next projected mutation waits on a condition
signal until that permit drops or the original callback deadline expires.
State and permits are checked again after waking. Failed/unbound coherence,
an old observation and an active mutation not yet at reply still refuse
immediately. The permit still covers reply and the optional snapshot. No
mutation, notification, failed outcome or unknown reply is replayed. The
[coherence architecture amendment](../../architecture/proposal/fuse-workspace-snapshot-overlay/24-mounted-sdk-coherence.md#issue-266-amendment-a-healthy-write-reply-gap)
records the fixed per-mount condition allocation and unchanged publication
path.

## Append-only public attempts

All commands used locked release binaries, the same sealed 8,194-byte old
head, a one-process/one-fd writer, one construction worker and an independent
read-only verifier. This worktree initialized its own master once; every later
case reused that closed master via `shutil.copyfile` and verified the byte
copy's hashes. Clone is setup reuse, not a cold claim. Ordinary host/container
cache was uncontrolled, so even a functional PASS has latency
`INELIGIBLE`, not a performance PASS. Each row was taken once at its frozen
source; no number was replaced or pooled with another source.

| Attempt | Source | Actual WRITE callbacks | Exec / Commit / complete command | Functional, cleanup, latency | Evidence |
| --- | --- | ---: | ---: | --- | --- |
| Pre-reply 512 diagnosis | `14859869f` | 512 | 3.000673 / 0.090577 / 4.202082 s | PASS, PASS, INELIGIBLE | [receipt](evidence/pre-reply-512/receipt.json) |
| Historical-order cause diagnosis | `4006d9893` | 129; 128 accepted | 0.583191 / no Commit / 6.477324 s | FAIL, FAIL, INELIGIBLE | [receipt](evidence/post-reply-512-fail/receipt.json), [refusal and retained shutdown](evidence/post-reply-512-fail/driver.stderr) |
| Reply-wait treatment, same post-reply snapshot | `a2e986a06` | 512 | 2.659247 / 0.088065 / 3.905690 s | PASS, PASS, INELIGIBLE | [receipt](evidence/treatment-512/receipt.json), [independent oracle](evidence/treatment-512/verifier.stdout) |
| Ordinary unsampled #248 4,097 gate | `a2e986a06` | **unknown** | Exec no driver result / no Commit / **25.006537 s** | **FAIL**, cleanup FAIL, latency not eligible | [receipt](evidence/gate-4097-fail/receipt.json), [postmortem](evidence/gate-4097-fail/orphan-after.stderr) |

The actual commands are pinned in each receipt. To reproduce at the named
source identity, use a **fresh** output path for each prepare/run, preserving
the same selection and prior prepared master. For example, the treatment
used:

```sh
python3 core/benchmark/fs-bench-pro/separated_writes.py prepare \
  --output benchmark-results/fs-bench-pro/issue266/treatment-prepared-v1 \
  --reuse-prepared benchmark-results/fs-bench-pro/issue266/postreply-prepared-v1/prepared.json \
  --reuse-selection fuse512
python3 core/benchmark/fs-bench-pro/separated_writes.py run \
  --prepared benchmark-results/fs-bench-pro/issue266/treatment-prepared-v1/prepared.json \
  --output benchmark-results/fs-bench-pro/issue266/treatment-512-v1 \
  --selection fuse512
```

The gate selected `gate` in the same runner with its own fresh preparation and
run paths. Reproduction is for diagnosis; neither failed arm is resampled to
replace these retained results.

The treatment oracle checked the original head
`1225c7e67c60589eb484199abbead21976228e75e2e73cca4eeb483761a8e77094`
and returned head
`12e5b2ae216f9074948314d73d660eacf6626d334322e737f8deb7b98ff0cc5197`:
exact parent relationship, old-head immutability, path inventory, both full
8,194-byte files, mode/length and **512 separated changed runs**. The daemon
lowered **1,024 final pieces**, **512 changes** and the Service declared 512
replacement bytes. The independent verifier took **14.359 ms**. This proves
one public Commit's exact old and new heads and final bytes. The writer did
not read through its fd between writes, so current-source read-your-writes,
concurrent G1/G2 history and an unknown reply outcome remain separate proofs.

At WRITE 512 the treatment's diagnostic snapshot recorded **512 retained
private payloads**, **4,288,512 allocated backing bytes** (including 94,208
metadata bytes), 14 live metadata pages, 22,015 direct 4 KiB ownership-ledger
reads, 13,212 writes, 4,408 metadata-page reads, 511 routine record scans and
zero registry lookup scans. These are cumulative diagnostic I/O counts and a
live allocation snapshot, not storage-device bytes or a phase-local cgroup
peak. C1 declared zero nodes created for this small-file save; that counter
does not count every Store SQL operation. Store size was 618,496 → 897,024
bytes; history file size remained 86,016 bytes. Driver-reported consumer
accounting was 1,876,920 bytes. Sampled host-operation maximum RSS was
31,997,952 bytes, not a peak memory gate. The daemon printed `sandbox closed`
with no retained-shutdown line; the SDK reported successful Unmount and
Docker deletion. Each copied attempt folder has an [identity manifest](evidence/treatment-512/EVIDENCE.json)
and a redacted prepared receipt; original sealed Store/history and binaries
remain in this worktree's ignored result directory.

The #248 gate was attempted once only after the 512 repair passed. Its
25-second complete-command timeout killed the host driver before it emitted
a receipt, WRITE count, Exec status or verifier result. The daemon's retained
log later recorded a failed `WorkspaceExec` after **29.108651 s**. No Commit
ran; the cloned Store remained 618,496 bytes. Docker deletion did not occur
inside the driver. Its identified container (the exact gate image) remained
running, then reported `sandbox shutdown retained: Busy` when signalled and
exited 137 after Docker's 10-second stop bound. The container and its one
named volume were removed only as explicit postmortem cleanup; this does
**not** establish clean Workspace custody. The saved [inspect](evidence/gate-4097-fail/orphan-before.inspect),
[resource snapshot](evidence/gate-4097-fail/orphan-before.stats),
[daemon log](evidence/gate-4097-fail/orphan-after.stderr) and
[cleanup outputs](evidence/gate-4097-fail/EVIDENCE.json) retain the failed
attempt. A later Docker snapshot showed 42.39 MiB used against its 512 MiB
limit; that is neither a phase peak nor proof of bounded memory. The #249
30-second product Exec timer remains a separate dependency. No deadline,
worker count, workload or cache policy was changed to turn this gate into a
PASS.

## Checks and open qualifications

At the fixed product source, the locked aarch64 release daemon build and the
Linux `kernel_write` external test-target build passed. The 512 public
Exec/Commit and independent oracle passed. Focused host Clippy for
`layerfs-workspace`, `layerfs-fuse` and `layerfs-daemon` libraries with
`-D warnings` passed in 12.14 s. The product boundary guard passed on 317
files and its nine self-tests passed; the shared runner self-check and Python
compile passed. Changed Rust files passed rustfmt and `git diff --check` was
clean.

Full Core fmt remains **FAIL** on the unchanged formatting at
`runtime/state.rs:267`. Linux aarch64 warning-denying Clippy remains **FAIL**
on the unchanged `useless_conversion` at `backing/segments.rs:192`; neither
file was edited here. The full Core test suite was **NOT_RUN** under the
under-30-second focused-test rule. The new Linux reply-gap test target
compiled but its native execution was **NOT_RUN**: reusable 64 MiB fixture
preparation first [failed looking for the default debug daemon](evidence/native-fixture-setup/native-fixture-v1/result.json),
then with an explicit release profile [failed `InvalidInput` during prepared
alias creation](evidence/native-fixture-setup/native-fixture-v2/result.json)
from this worktree's separate Store. Both setup failures
are retained. The existing concurrent append, notifier-failure, unknown
reply, and G1/G2 overlap native selections were not rerun at this source;
their current-source coverage remains open. The treatment's real sequential
FUSE callbacks and exact Commit oracle must not be presented as those wider
proofs. No CI or retired preflight result is claimed.

## Exact first-parent production LOC

Method for product-changing commits: `tools/production_loc.py --root
<snapshot>` on `git archive` of the first parent and final staged tree,
restricted to `crates`, `core/crates` and the same counter. Nonblank,
noncomment first-party product Rust/runtime SQL counts; external tests,
benchmark tooling, docs, fixtures, examples, comments and blanks do not.
Reference stays at **65,417** production lines and adapter stays **0** in
every row. There is no legacy retirement or product relocation; `open_flags.rs`
is a move within Core FUSE. Core and combined totals are:

| Commit | Change | Core before → after | Combined before → after |
| --- | --- | ---: | ---: |
| `8b6b15f4b` | Prospective diagnosis | 58,532 → 58,532 (+0) | 123,949 → 123,949 (+0) |
| `14859869f` | Failure-only instrumentation and shared runner | 58,532 → 58,595 (+63) | 123,949 → 124,012 (+63) |
| `bfed8a438` | Prospective cause diagnostic | 58,595 → 58,595 (+0) | 124,012 → 124,012 (+0) |
| `4006d9893` | Post-reply optional snapshot | 58,595 → 58,595 (+0) | 124,012 → 124,012 (+0) |
| `387845be9` | Prospective reply-wait treatment | 58,595 → 58,595 (+0) | 124,012 → 124,012 (+0) |
| `e81899198` | Condition wait, reply marker, architecture and external test | 58,595 → 58,640 (+45) | 124,012 → 124,057 (+45) |
| `a2e986a06` | Sealed rebuild reuse selection | 58,640 → 58,640 (+0) | 124,057 → 124,057 (+0) |
| `42845d306` | Shared count-route correction | 58,640 → 58,640 (+0) | 124,057 → 124,057 (+0) |
| `b039b221f` | Register focused reply-gap test | 58,640 → 58,640 (+0) | 124,057 → 124,057 (+0) |

This report and its copied evidence also change no production source. Their
commit records the same first-parent total and delta zero.

## Follow-up count analysis

After this report, a separate [prospective 1,024 count diagnostic](../../../../docs/roadmap/0.1/0.1.7/issue266-1024-count-diagnostic-spec.md)
used the same public route and product seal. Its one retained
[receipt](evidence/scaling-1024/receipt.json) has 1,024 actual FUSE WRITE
callbacks, independent old/new-head oracle PASS and positive daemon close;
Exec was 6.497280 s and Commit 0.154572 s, with latency still
`INELIGIBLE`. The [algorithm, space and time analysis](SCALING-ANALYSIS.md),
prepared with three independent read-only subagent audits, shows that the
ownership-ledger read/write counters follow an exact quadratic fit at the
observed 128-to-1,024 checkpoints, while live payload/metadata allocation
grows approximately linearly. This is a finite-range count finding, not a
qualified time exponent or a measured 4,097 count. The original 4,097 FAIL
remains unchanged and was not rerun.

The follow-up spec commit `e2bbd908c`, runner commit `a8f3514c4` and this
analysis/evidence commit each change only documentation or benchmark tooling:
Core **58,640 → 58,640 (+0)**, reference **65,417 → 65,417 (+0)**,
adapter **0 → 0**, combined **124,057 → 124,057 (+0)** for each first-parent
comparison. The same `tools/production_loc.py --root .` product scope and
exclusions used in the table above apply; the staged product snapshots are
identical to their respective first parents.

## Native reply-gap proof follow-up

The earlier native fixture `NOT_RUN` qualification above records the state at
that report's source. Its two setup FAIL receipts remain intact. A third
[failed setup](evidence/native-fixture-setup/native-fixture-v3/result.json)
used a correctly provisioned C2 Store and still received `InvalidInput` at
the prepared-alias Commit. The shared native driver was sending the alias row
inside the request, while the current prepared-stream protocol requires seven
declared row totals in the request and a versioned row body. The driver now
encodes that body once for the three existing alias-fixture callers. A fresh
[fixture preparation](evidence/native-fixture-setup/native-fixture-v4/result.json)
then passed in **0.636303 s** with closed producer processes and no cache or
performance claim. The earlier default-debug-daemon failure also explains why
the commands below explicitly select the release profile.

The first [focused route attempt](evidence/native-reply-gap/native-reply-gap-v1/result.json)
was a **FAIL** at **0.731125 s**: its registered `reply_gap` case selected zero
tests because the Rust function had a `_wait` suffix. The selection was fixed
and the retained container and volume were removed in recorded postmortem
cleanup. The next [reply-gap test execution](evidence/native-reply-gap/native-reply-gap-v2/result.json)
selected **one** Linux test and passed: **0.21 s** test, **1.107362 s** complete
command, test exit 0, service exit 0 and cleanup PASS. Rustfmt then changed
only whitespace in the external test. A [final-source proof](evidence/native-reply-gap/native-reply-gap-v3/result.json)
rebuilt that test binary and passed the same single selection in **0.22 s**,
with a **1.092133 s** complete command, test exit 0, service exit 0 and
cleanup PASS. A separate call first
showed that an admission attempt with its original 100 ms deadline returns
`Deadline` while the healthy origin permit is held. A second thread then
announced its admission attempt, produced no result while that origin was held,
and returned successful admission from that same attempt after the origin
permit dropped. This closes the focused native projection API proof gap; it is
not another public 512/4,097 performance sample or a kernel-mounted concurrency
proof. The 4,097 FAIL and its missing callback/Commit result remain unchanged.

The worktree-local commands were:

```sh
cargo +1.85.1 build --release --manifest-path core/Cargo.toml --locked -p layerfs-server --example prepare_store
core/target/release/examples/prepare_store benchmark-results/fs-bench-pro/issue266/native-r1-store-v1/store.sqlite
LAYERFS_PROOF_PROFILE=release python3 core/crates/layerfs-workspace/tests/prepare_large_edit.py \
  --store-master benchmark-results/fs-bench-pro/issue266/native-r1-store-v1/store.sqlite \
  --binaries core/target/release --producer-source 15302704bf6d985f1f29c484f2baabeb97f5843a \
  --producer-seal c0d856c04d62cbb41b9d1d02f92dfa16f7e11817b32637d29590a7fd6f1a82b9 \
  --output benchmark-results/fs-bench-pro/issue266/native-fixture-v4
cargo +1.85.1 zigbuild --release --target aarch64-unknown-linux-musl \
  --manifest-path core/Cargo.toml --locked -p layerfs-fuse --test kernel_write
LAYERFS_PROOF_PROFILE=release python3 core/crates/layerfs-fuse/tests/kernel_write_route.py \
  --fixture benchmark-results/fs-bench-pro/issue266/native-fixture-v4/result.json \
  --binaries core/target/release \
  --test-binary core/target/aarch64-unknown-linux-musl/release/deps/kernel_write-502106b9f7dc1ff2 \
  --case reply_gap --output benchmark-results/fs-bench-pro/issue266/native-reply-gap-v3
```

These output paths name the retained attempts; reproduction needs fresh output
paths. Both actual test commands were below the requested 30-second focused bound and
the fixture was setup outside it. No public WRITE selection was repeated. The
three historical fixture FAILs, zero-test FAIL and both focused PASSes are all
retained with copied-file hashes. This follow-up changed external tests and
their drivers, not product code: Core **58,640 → 58,640 (+0)**, reference
**65,417 → 65,417 (+0)**, adapter **0 → 0**, combined
**124,057 → 124,057 (+0)**, using the same first-parent production counter
and source scope described above. The final test source hash is
`d6d28dcbc676ab5479653117428a5ea209bf4c646da858c455cb4bb55b8d1fd2`;
the receipt's `source` field names the pre-commit parent and its independent
test-source hash names the exact executed external test. Locked release host
fixture and Linux test builds passed, as did focused Rustfmt, Python compile
and source/document whitespace checks. The unrestricted staged whitespace
check flagged only the literal trailing blank lines in three preserved test
stdout files. Full Core test and Clippy were not run for this external-test
follow-up; no CI or retired preflight result is claimed.
