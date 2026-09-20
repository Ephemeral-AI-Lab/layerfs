# Issue 192 M0 implementation decisions v1

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Specification pin: `21f6af702919c23fafc88890361bef7bfb831140`, published on
`codex/pair3-foundation`. Custody is in `spec-custody.json`; its 19 files were
byte-compared before and after copying. C1/C2 basis is
`10b9d4a6cf9d88267d508cb010cc82950e080d77`; no uncommitted optimizer source is
included. Core lock SHA-256: bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991. Rust 1.85.1; native arm64 macOS and
arm64 Linux Docker. Dependency additions require a separately recorded final lock.

## Selected carrier and trust

Native ordered TCP with Noise_KK_25519_ChaChaPoly_BLAKE2s, using published
`snow = 0.9.6`. Both static public keys are provisioned out of band. The bounded
initial principal selector only selects a configured key; successful KK authentication
creates trusted authority. No bearer secret or caller-asserted authority goes on
wire. AEAD authenticates every complete frame and strictly increasing per-direction
nonces prevent replay/reordering. Failed authentication closes without diagnostic
key material. Static credentials have configured Unix expiry, checked at handshake
and operation admission; revocation replaces configuration and restarts the service.
No dynamic trust registry or key recovery. Secret config is operator-owned and
excluded from committed evidence. Host listener explicitly configured; Docker connects
to `host.docker.internal` (never its own localhost).

## Vocabulary, schema and bounds

Protocol v1 uses big-endian integers, fixed 32-byte object identities, fixed numeric
Store/principal identifiers, nonzero monotonically increasing u64 correlation IDs,
u64 input-generation labels. Frame header: `LFB1`, kind u8, flags u8=0, reserved
u16=0, correlation u64, payload length u32. Kinds HELLO=1, BEGIN=2, BODY=3,
END_INPUT=4, RESULT_DATA=5, SUCCESS=6, FAILURE=7. Native AEAD messages additionally
carry a u32 ciphertext length; maximum plaintext 32788 bytes, ciphertext +16.
Metadata encoding is a closed checked binary schema documented with the codec.
Unsupported versions/profile/flags/kinds fail closed, without fallback.

F=16384 body bytes; P=32768 encoded metadata bytes; C=4 live connections including
handshaking/closing; A=1 active service handler process-wide; Q=0. One service
construction producer. Frame-count ceiling 8192 per operation; zero-length BODY
is invalid. Maximum file/input/read response 64 MiB; edit replay 8 MiB; edits 256;
prepared binding/inode records 128 each; path 4096 bytes; name 255 bytes; listing
128 entries/16384 bytes. All request sums and counts checked before allocation.
No application upload/download queue beyond one active frame in each direction;
W=2*F per direction covers current encrypted/plaintext representation. Metadata
and decoded representations are charged separately (<=256 KiB per connection).
Socket buffers, native thread stacks, C1/C2 memory and service replay are additional.
These are selected implementation limits, not measured RSS or performance promises.
Operation deadline 10 seconds, configurable downward only; handshake/idle 5 seconds.
Absolute deadlines are rechecked at each I/O/core boundary. Core SQL/hash calls are
not universally preemptible; elapsed deadline after completion cannot imply rollback.

Store schema 6, application ID 1279677261, physical profile 1, frozen default policy;
MEMORY journal/synchronous OFF unchanged. Only Store-derived construction capacities
and fixed FilesystemResources are accepted. Store paths never enter requests.
Configured principal/Store/op bitmasks are checked in the same direct/remote handler.

ReadFile: file root + start/end; provisional DATA until exact terminal length.
Inspect: File (length/representation), Stat (resolved identity, roots and portable
mode/mtime), List (path/after/count/bytes with explicit continuation), Readlink.
ConstructFile: exact length and streaming body, including empty/cutoff cases.
EditFile: base root/length and ordered current-result start/end/replacement lengths;
replacement bodies remain separate capped parts, no sorting/coalescing.
UpdatePreparedFilesystem: base filesystem root, scope, root serial, sorted changed
bindings and typed existing-inode values. No new inode allocation; references must
already be retained. N saves plus one attachment are N+1 operations.
Mutation success requires complete exact END_INPUT and confirmed C2 finish.
Failure keeps typed original/cleanup disposition; lost terminal is unknown and never
replayed. StoreProvider's collapsed ProviderFailure remains collapsed.

## Execution and telemetry

A bounded native connection owner performs authentication and frames. The service
handler runs synchronous core work on one owned context; TimingScope and StoreProvider
never cross threads. Full-duplex client input/output must observe early refusal and
cancel the local submission without draining it. Pending/closing owners remain counted.

Portable telemetry stays std-only and forbids unsafe. Native feature will use
published safe macOS nix getrusage (CPU) plus libproc pidrusage (RSS/start), and
Linux procfs plus safe nix sysconf/poll;
qualification and exact lock pins precede enabling these capabilities. One monitor
per process, 100 ms cadence, 600 raw slots, 32 fixed active summaries, skip missed
ticks. CPU cumulative user/system and RSS belong to process windows, not exclusive
operations. Incarnation combines configured run identity, PID and native start ID.

Master-off creates no telemetry allocations/probes/workers/files/exports. Timing
uses static labels and a validated <=1024-node/32-depth/128-label-byte profile;
conservative per-node ownership charge includes arena/tree conversion overlap.
Producer ceiling 8 MiB includes 4 MiB timing pool, 2 MiB/128-record queue, 16 KiB
encoded maximum and fixed monitor/bookkeeping; exact layout arithmetic must pass
before enablement. Forward/local/both count copies/in-flight bytes and aggregate
64 KiB/s, 256 KiB burst. Collector: 16 producers including closing, 8 MiB queue,
16 MiB owned budget, 1 MiB/s and burst. Local segments 4*16 MiB; collector 16*16 MiB;
active included, 24h expiry checked at <=60s cadence, exclusive owned namespace.
Shutdown allowance <=2s, no guaranteed blocking-syscall preemption or durability.
Required envelope overflow omits diagnostics, never truncates JSON or product output.

## Finding dispositions and evidence mapping

Stage 7 audit findings: schema compatibility is explicitly pinned (no migration);
C2 native engine/path/provider types stay service-local; mutable capacities never
cross the bridge; filesystem resources are fixed below known bounds; one C1 package
identity is used throughout core; no same-save chaining or new inode allocation.
Independent algorithm substitution remains #172, performance driver/campaign #193.

Every implementation matrix V01-V16 requires actual Docker/macOS route evidence;
V17 is secondary direct parity. External malformed peers are labelled as endpoint
coverage. T01-T16 map to telemetry tests and actual native output/retention runs;
ENV01-ENV06 require OS/process evidence. Case results start NOT_RUN and will be
recorded individually with test names, commands, identities and cleanup in the
acceptance record. No copied Stage 6 evidence or direct smoke can promote a row.

Native package/compiler qualification, exact schemas and allocation layout remain
M0 dependent checks. This decision record does not claim M0 complete until those
checks are recorded. Any failed choice is retained with its reason and superseded
explicitly before dependent implementation.

## Qualified implementation profile and checkpoint clarification

The preceding early decisions were development inputs. The concrete implemented
profile uses four persistent/handshaking/closing sessions plus one synchronous
accept/refusal slot: C=5 total application resources, with four admitted sessions.
The payload/codec overlap allowance is 96 KiB per direction per persistent
connection, plus 256 KiB metadata/decoded ownership. The original two-frame
estimate did not include all plaintext/ciphertext/relay overlap and is not a
qualified memory claim. Socket send/receive buffers are requested at 128 KiB and
checked at <=256 KiB each. Kernel backlog/metadata and stacks are separate.

The native recorder selects eight 256-node recordings (depth 32, label 128 bytes)
under a 4 MiB reservation pool. Native C3 labels are static and no imported timer
tree is accepted from peers; the collector retains bounded encoded envelopes.
External caller-owned clones/imported trees require their own caller budget and
are not an unlimited global-memory promise. Runtime validates all selected limits
and checked aggregate arithmetic before native allocation. Default recorded
producer ownership is below 8 MiB; this is not total RSS. Cgroup observations in
acceptance come from the host Docker Engine stats API, separately from native
process RSS and without a reset phase-peak claim.

Inspect paths are canonical relative paths; root is empty. File Saved and
FilesystemSaved are distinct terminal types. List returns the actual explicit
C1 continuation name. The exact field order and big-endian encoding are in
`layerfs-bridge/src/adapters/native/protocol/{metadata,response,frame}.rs`.
There is one version and no fallback. Body response allowance excludes the fixed,
separately bounded terminal metadata envelope.

The first complete Docker off-mode/direct parity selection passed at the original
C1/C2 baseline; its receipt is retained as pre-checkpoint evidence. At M3-to-M4,
[optimizer-checkpoint.json](optimizer-checkpoint.json) records selective byte-exact
integration of committed C1 source/tests/architecture from
`e54af84653cd1a22b3f26631dbd19eafb895a7b4` (+49 upstream production LOC). C2 stays
at the original basis. No #190 benchmark files or historical receipts were copied.
Subsequent qualification must use this C1 checkpoint. The daemon has no C1/C2
linkage, so its image can be reused when its own compilation inputs match.

Safe capability qualification: the standalone Snow KK probe passed authenticated
roundtrip, ciphertext tampering and nonce-mismatch checks. The real native monitor
tests exercised libproc. Linux musl daemon builds include the native procfs path.
The real full-disk test used a disposable 32 MiB HFS+ image, reached ENOSPC, retained
the original Err(42), and recorded one output failure; detach cleanup passed.
These are functional/platform facts, not overhead or performance measurements.

## Fast functional verification selection

Stalled-input/output cases request the supported 250 ms operation deadline, with
a 3 s controller watchdog. This exercises deadline behavior without waiting for
the maximum 10 s profile. It is functional verification, not a latency claim or
qualification of the maximum-duration case. Reuse existing build artifacts and
passed proofs where identities remain applicable; rerun affected checks only.
The macOS CPU correction and disposition of earlier CPU observations are in
[evidence/macos-cpu-unit-disposition.json](evidence/macos-cpu-unit-disposition.json).

## Final candidate identity and schema clarification

The final runtime-source seal is
`1f226f29cd85875159921d670e92526887e466374a2da88fe0d437053cbf274c`;
[qualification-index-20260920.json](evidence/qualification-index-20260920.json)
records its exact source scope, files, image and native binary. Final lock SHA-256:
`48d2c6f7ef507a5052e5f7377a9b203c7b4c6f3cad26bf38a66a6a4a4928437b`.
Resource reports carry selected-measurement bits (CPU=1, RSS=2), local clock/source
and probe duration; disabled differs from unavailable. Run summaries include
namespace and loss-counter overflow. Mac CPU comes from checked POSIX microseconds,
not raw libproc Mach ticks. The Docker coordinator uses distinct product and
diagnostic attachments so unread diagnostics cannot stall CLI product demultiplexing.
The [acceptance record](07-acceptance.md) states the remaining ENV05 and full-workspace
proof gaps; implemented limits are not promoted to a measured maximum RSS claim.
