# E04 original write receipts

Status: Current general guide. This describes implemented external diagnostic
source; execution and qualification
require their own retained receipts. This note does not record a passed sample;
E05 remains NOT_RUN. The owning requirements are the
[E2 job receipt contract](../../../docs/roadmap/0.1/0.1.7/cluster-two-e2-job-receipts-v1.md).

## Actual route and independent input

The named macOS `e2_writes_host` initializes the supported global Store through
Project Init, retains the initialized SDK Runtime, authenticates one native
connection, and serves it through Sessions and Supervisor. The Linux
`e2_writes` consumer uses `Upstream::attach_with_receipts`, its production
Workspace, and an actual fresh local OwnerClient database. No legacy product,
substitute SQL, reconnect, provider retry or failed operation replay is used.

The fixture contains exactly the root directory and `dense.bin`, a 16 MiB
ordinary file. Independently closed base input uses registered seed 1, identity
0; replacement input contains 1000 independently generated 4 KiB payloads with
identities 1 through 1000. Fixture attachment verifies both complete files
against the frozen generator. The manifest binds their SHA256s, lengths,
assignment and actual acquisition sidecar. The caller separately prepares the
registered fixture bundle and identity before the collector; final output
hashes do not replace that prepared input bundle.

For index i in 0..1000 the ordinary write is 4096 bytes at
`4096 * ((104729 * i) % 4096)`, with mtime seconds -7 and nanoseconds 42. Each
operation uses a fresh production UpstreamOperation while initialized Calls
persist. Workspace resolves actual Needs through the authenticated client,
publishes through OwnerClient, attempts the exact publication reply ticket and
releases the original BaseSource. There is no growing job/write collection.
The existing bounded namespace-command clone remains real work; exclusive copy
attribution is unavailable.

The host makes a fresh native source from the closed base, records portable
facts and allocated blocks, imports it through public Init/fork, then removes
that owned native source before the consumer. `source_copied_bytes` is that
native copy count, not an exposed Project read counter. Namespace Init retains
its actual four-worker profile even when the ordinary construction environment
is one. Fixture attachment uses client canonical cache capacity zero; this is
not OS cache enforcement or a cold-state qualification claim.

## Original observation custody

Recorder creation captures identity, binary and closed-input hashes and retains
startup/attempt/owner IDs before Owner startup. Each original job's record and
external-job IDs precede its single admission. Owner/client probe IDs precede
the snapshot. Each public write ID precedes caller input and operation demand.

Successful attachment returns the original Open Completion and Binding/Policy
Messages. Actual work, bytes and hashes are recorded while credits are held,
then those originals are explicitly dropped. The Open wall boundary includes
attachment work; its JobWork is exact original Open work rather than a
substituted diagnostics subtraction.
Its input route is absent because the namespace does not exist before Open.
The retained validator binds the namespace through that original Opened result;
later jobs must carry the exact allocated route.

Observing ports execute the same typed commands as production ports and record
the original Completion before release. Unattempted admission retains the
original command and deciding error. A pending error or external wait bound
retains Pending in a Mutex only to satisfy WorkspaceError's Send/Sync custody
contract; no I/O or wait occurs under that lock. Record failure retains the
original Completion; wrong response shape makes the attempted operation
terminal. The driver retains its actual Owner, operation, source token,
bootstrap replies and native fence on the first failure. It also retains the
first secondary reporting/append failure beside that cause. A terminal writer
is never reopened. Failure exits with those owners held, without guessing
Close, release or remote publication disposition.

Original JobWork supplies one aggregate StatementWork, payload/allocation,
parked turns, queue wait and service time. It does not supply fourteen SQL
families per job. Owner diagnostics separately supply actual foreground and
maintenance DatabaseWork families, class counts/waits and credit endpoints.
The external validator may compare original-job aggregate sums to exact owner
aggregate deltas; it cannot redistribute those totals across statement families.

Needs retains the actual requested facts. A subsequent base-fact interval binds
the same source and original Needs job ID and records actual shared client
counters before/after supply. The private BaseFacts provider/per-ID trace stays
unavailable. Applied records contain the original publication; ReplyAttempted
records include that exact publication as their input. Trace binds serial,
source, offset, length, replacement hash, times and publication. `write_attempts`
counts IDs allocated before caller input; `write_records` counts acknowledged
trace rows. They can differ on failure, so unrun indices start after the actual
attempt prefix rather than treating a published but unrecorded write as unrun.

## Verification and terminal ownership

The in-process independent oracle derives expected bytes only from closed base
and replacement inputs. It checks every final 16 MiB byte with 64 KiB windows,
EOF, kind, size, mode, nlink and mtime. Actual root enumeration must contain only
`dense.bin` at its original serial with no continuation, followed by real
lookup. This proves membership only for the selected two-inode fixture.
`old_file_content_root_unchanged` checks the original immutable file content
root. `binding_unchanged` checks captured Runtime binding, not a fresh mutable
History head. Verification Owner jobs are separate from writes-window work. Numeric oracle
portable mode is `file_mode`; common receipt `mode` stays diagnostic.

After verification, one actual Close is followed by a 250 ms declared idle
interval without a maintenance/status/mutation pump, then one CleanupState
observation. This diagnostic does not prove sustained drain rate or exact
eligible debt. The consumer attachment is explicitly fenced and joined, with
original native/framing/reassembly reports held through recording. Close error,
partial messages or retained receive credit refuse success. Final owner
diagnostics precede actual stop. Database stat is post-stop on success or
scoped to the retained failed artifact; it is not exclusive physical I/O or a
claim that the database shrank.

The host records each original local Delivery before release. A failed,
refused or incomplete Delivery stays held and triggers explicit fence/join.
Expected input EOF requires zero queued/partial/undelivered inputs plus exact
non-saturated accounting: `wire_bytes = plaintext_bytes + 18 * records` and
`record_io_attempts = records + 1`. This excludes truncated encrypted prefixes
or bodies. EOF's Quarantined close refusal is recorded as the original pre-I/O
refusal; other close/frame/Noise/timeout/panic causes fail.

A final known successful Binding response can still be in AttachmentFence
when the consumer already received it. The host accepts only the exact
captured Binding with matching complete correlated output receipt and no
unattempted/unsent/refused input. Originals are recorded and explicitly dropped
before zero attachment/input/service/output credit checks. Consuming Sessions
fence must contain no Save slots. Socket receipt and local scope fencing imply
no Save, Commit, History, process or mount acceptance.

## Invocation and output

Linux argv has eight entries including executable:

```
e2_writes ENDPOINT ASSIGNMENT BASE_INPUT REPLACEMENTS DATABASE_ABSOLUTE OUTPUT_UNUSED IDENTITY_JSON
```

Inputs are canonicalized before startup. Database and output are fresh,
separate owned absolute paths. Identity has a 16 KiB input window. Append-only
startup/jobs/probes/trace/stdout/stderr streams retain acknowledged byte/record
counts and SHA256. Each JSON row has a 64 KiB encoding window; outcomes and
manifest are create-new. These bound individual records and working windows,
not total lifetime flow/output. The external complete functional watchdog is
90 seconds, with individual original-job/native-fence wait bounds of 5 seconds.
These are diagnostic vehicle bounds, not ordinary product Exec semantics.

MacOS argv has nine entries including executable:

```
e2_writes_host STORE_UNUSED SOURCE_UNUSED ASSIGNMENT_UNUSED BASE_INPUT PROFILE OUTPUT_UNUSED ADVERTISED_HOST DEADLINE_SECONDS
```

PROFILE is durable/disposable. Caller deadline is 1..90 seconds and bounds setup,
readiness, serving and joins. Fresh output includes `host.jsonl` and
`endpoint.txt`. The host uses actual fixture harness keys and assignment and
accepts one authenticated connection. WouldBlock readiness and observed park
waits do not retry a failed operation. Caller owns launch, complete wall time,
source/build/image seals, and external Docker mapping witness from inspected
mounts and executed argv. Invocation mapping stays unavailable internally;
the collector does not guess container aliases.

The first actual diagnostic failed before attachment because the accepted host
socket retained nonblocking mode. The corrected host explicitly selects blocking
mode before the bounded handshake. Its next diagnostic retained 67 Applied
Completions and 66 complete write traces before the host input worker's inherited
five-second read timeout expired during legitimate local-only activity. Both
failures and their binaries are retained in receipts 50/52 and 60/61.
The corrected persistent-input profile keeps the five-second handshake read and
socket write bounds, active consumer read bound, original-job and join bounds,
and the external functional watchdog. After successful authentication only the
host's continuously waiting input worker has no idle read timeout; an explicit
socket fence wakes it. A temporary socket-options clone is owned and dropped
before supervision. No failed read is retried, and the failed results are not
reclassified by this semantic correction. Whole E2 remains incomplete.

The external mapping validator accepts the selected direct foreground Docker
invocation. It binds the actual outer command's image, collector arguments,
CID-file path, single writable repository mount, working directory and declared
environment to inspect's effective Path/Args and Config. Unsupported wrappers,
entrypoints, alternate CID witnesses and redirected input submounts refuse.
This proves the declared diagnostic path mapping only; it does not equate
Darwin/Linux device/inode/allocation observations or supply resource ownership.

The [87-case retained-validator check](../issues/307/checks/e04-writes-20261007/37-validator-final-tests.json)
passes after syntax compilation. It includes the original 37 E01 cases and 50
E04 cases. The first actual-Open and Docker-command/CID regressions failed in
[receipt 25](../issues/307/checks/e04-writes-20261007/25-e04-review-regressions.json),
which remains retained. Tooling consistency checks are not collector execution.
The later source-shaped startup-failure test first failed in
[receipt 41](../issues/307/checks/e04-writes-20261007/41-startup-prefix-regression.json)
because E04's actual `NOT_RUN` Stop was compared with E01's distinct spelling.
Only that branch was corrected, and its
[focused proof](../issues/307/checks/e04-writes-20261007/47-startup-prefix-corrected.json)
passes. This makes 88 distinct tested validator cases across the preserved
87-case run and the additional focused body; it is not a second full-suite run.

## Evidence limits

All receipts declare diagnostic mode, sample_count zero, admission_eligible
false and qualification_status NOT_EVALUATED. Full E2 observation_consistency
stays INCOMPLETE. An external validator can evaluate narrow write-window
aggregate/protocol consistency after independently binding actual artifacts;
that does not qualify E1 numeric admission or S7 completion. E05 is NOT_RUN.
Private base-fact/per-ID trace, per-job SQL families, whole-operation copies,
statement/bind correlation, indexed visited rows, exact eligible debt,
queue-only peaks, isolated runnable wait, phase residency, physical I/O and
cache calibration remain unavailable. Native aggregate counters and artifact
stat are source-scoped and cannot replace those missing observations.


## Native guest backing continuation

The [prospective v2 contract](../../../docs/roadmap/0.1/0.1.7/cluster-two-e04-native-backing-v2.md)
and [implementation report](../issues/307/E04-NATIVE-BACKING-20261007.md) describe
E04-only startup filesystem observations and independently owned guest volume
custody. Original pre/post guest stat domains remain distinct from host copies;
shared E01 serialization and the eleven missing whole-operation dimensions do
not change. Startup verifies a read-only reopened descriptor before refusing the
known incompatible host filesystem. It neither switches backing automatically
nor changes the exact per-job range admission.

The first actual native consumer completed the selected writes/oracle and Stop,
but host native CloseFailed and Docker source-domain validation prevented whole
success. That source identity remains a failed diagnostic. A proposed explicit
application fence must preserve the original error and refuse partial encrypted
EOF; source presence or consumer success does not qualify S7 or S9.


## Original coordinated disposal v3

[The implemented disposal checkpoint](../issues/307/E04-DISPOSAL-CONSISTENCY-20261007.md)
now supplies one complete Disposable and one complete Durable diagnostic. The
consumer holds its original final Binding Message through Close/Gone and exact
host acknowledgment. Host matches the original second Binding Delivery, fences
once through the existing Supervisor, joins/releases actual owners and verifies
zero credits/Save custody before acknowledgment. Consumer then performs its own
original native fence/join and Stop. No peer EOF/CloseFailed is reclassified.

Original pre-start input/probe hashes join actual commands/CIDs/inspect with the
opaque Docker bind source. Native volume identity and file stat/hash are compared
only in their actual guest domain. External control/configuration and one-shot
publication are explicitly diagnostic; they add no product/Bridge API, constructor,
service lane, kernel interruption capability or benchmark maintenance pump.

Both retained validators report write-window and application-disposal PASS, whole
observation INCOMPLETE, qualification NOT_EVALUATED and E1 samples0. Exact budgets,
source/binaries, failed precursors, partial validator timeout, custody and the
later prospective owner120s test direction remain in the report. This does not
close E2, R1, S7 or S9 or activate excluded application packages.
