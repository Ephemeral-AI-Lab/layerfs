# Issue #192 bounded load profile

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.

D04 selection for implementation after the schema 7 C2 checkpoint, based on
`0819f3f39833d477d9ed6d878a50691c3c046a83`. These are admission/ownership
limits, not a measured maximum file size, process-RSS ceiling or throughput claim.
O06 and O10 require the actual native/Docker proofs below before qualification.

| Resource | Selected limit |
| --- | --- |
| Configured logical Stores | 4 per service |
| Persistent, handshaking or closing sessions C | 4 |
| Acceptor's additional synchronous refusal owner | 1 socket, no operation buffers |
| Active operations W | 2 across the entire service, reads included; Q=0 |
| Private C2 saves per Store | 2, including unresolved persisted ownership |
| Construction producers | 1 per admitted operation |
| Construct input / read range | 4 GiB per operation |
| Edit base/result length | 4 GiB; replacement replay remains 8 MiB |
| Metadata / body frame | 32 KiB / 16 KiB |
| Frame-count budget | ceil(declared bytes / 1024) + 256 boundary frames + 1 terminal |
| Edit / prepared directory / inode counts | 256 / 128 / 128; at most 128 changed names |
| Overall requested duration | 1..=600,000 ms; caller may select less |
| No-progress I/O allowance | 5 seconds, additionally bounded by the overall deadline |
| Native handshake / idle session | 5 seconds |
| Executable shutdown collection | 2 seconds; unresolved owners survive until explicit process exit |

Requests select their actual byte/range/replay counts and a shorter overall
duration within this profile. The profile and protocol version remain 1; the frame
layout and Noise selection are unchanged. Older binaries can explicitly refuse
the wider profile's requests; mixed-version qualification is not implied. No
existing fault-test or verification command timeout changes with the product's
600-second maximum requested duration.

## Aggregate ownership ledger

Admission reserves complete operation ownership, not just a connection or a
writer flag. An operation keeps its reservation through finish/abort/cleanup.
Closing session owners keep their C reservation through worker completion or
process exit. A request cannot create a third operation with a smaller declared
payload. Buffers owned by diagnostics after an operation returns remain charged
to telemetry's separate bounded pools.

The memory admission profile is the following vector of byte and count ceilings.
Counts for collection nodes and native connections are stated as counts; they are
not converted into an invented universal physical byte size. Every per-operation
dimension below has multiplicity at most W, the Store's configured writer budget
(two by default, 1..=64). This preserves hard aggregate
ownership limits without treating socket options as a persistent memory cap.

**The figures below are the W=2 instance of that rule, not a ceiling on W.**
[#216](https://github.com/Ephemeral-AI-Lab/layerfs/issues/216) makes the budget an
operator setting, so a raised budget multiplies every per-save dimension stated
here - one 16 MiB encoder workspace per save is the largest of them - and no
resource qualification exists for a raised budget. The default stays 2, and a
larger setting is an explicit operator decision with that memory consequence
attached, not a free throughput change.

| Owner | Bound per owner and aggregate multiplicity |
| --- | --- |
| Bridge session | Two 256 KiB directional allowances plus 256 KiB metadata/conversion allowance; C=4 gives 3 MiB of conservative application staging allowance. Frame/record lengths enforce allocation demands; Vec capacity, header overhead, previous/current frames and plaintext/ciphertext overlap are included. |
| Edit replay | 8 MiB per operation, 16 MiB across W. No spool or whole-result buffer. |
| C1 input/read waves | Existing bounded chunk, mapping and traversal windows; file read waves request at most 32 payload objects / 1 MiB, with fixed navigation bounds. |
| C1 edit frontier | Existing 8 MiB−1 deferred-state bound per operation. |
| C1 filesystem update | Existing 4 MiB scratch and 64 MiB ordering profile per operation; ordering pending rows/runs/merge output remain inside the existing accounting. Native metadata conversion is also bounded by the request's 128-record/name limits. |
| C2 encode/decode workspace | One fixed 16 MiB encoder and 1 MiB decoder per save; at most 34 MiB for two saves. One additional 1 MiB read-session decoder per operation is separately owned. |
| C2 pending canonical data | 512 objects / 512 KiB per batch, with the pre-existing single-object exception bounded by 16 MiB; the drained wave and next pending batch are both counted. |
| C2 open groups and tails | Five lanes per save; each multi-record group also seals at 512 KiB canonical data. Framing/count limits remain 65,536 bytes / 8,191 records. Ordinary tails are at most 256 KiB; the single-record lane has the explicit 16 MiB + 4 KiB bound. Prepared writes and retained tails are distinct simultaneous owners. |
| C2 returned canonical data | 32 MiB per read wave, including repeated IDs; same-save pending/stored results share this cap before copying/acquisition. At most two operation readers. |
| C2 content index | Fixed 8,192 slots / 65,536 references, 688,128 payload bytes per index. Up to four published Store indexes, two private copies and two temporary publication copies. |
| C2 pooled index | At most 131,072 entries per published/private index. Up to four published Store indexes, two private copies and two temporary publication copies. Entry charges are not exact BTreeSet heap/RSS; node counts and allocator observations remain explicit. |
| C2 dependency and group caches | Existing 4 MiB pack-cache budget and 512 KiB decoded group/value budgets per owning reader. The permitted one-large-pack acquisition is separately bounded by the singleton limit, including admission before BLOB materialization. Collision validation owns a separate bounded resolver and at most two canonical comparison buffers. |
| C2 metadata lookup | At most 128 requested IDs per SQL page and two candidates per ID, with an extra row only to detect violation. Depth caches retain at most 4,096 entries. Pooled cold synchronization scans only the current 131,072-reserved-value ordinal interval. |
| Native SQLite state | One save connection and at most one provider read connection per admitted mutation; read-only operations use a bounded read session. Short transactions and explicit row/blob limits bound demands. The engine's page-cache/journal/native allocations are separately observed and are not equated to Rust entry charges. |
| Telemetry | Existing shared monitor, eight bounded recording slots under the 4 MiB recording pool, 600 observation slots and 32 live summaries. The selected forward queue is 2 MiB / 128 records, including the current write; records are at most 16 KiB. Published/closing diagnostic ownership remains charged. |

The earlier 96 KiB directional arithmetic omitted capacity/header details; the
allowance above replaces that accounting claim without changing frame sizes or
allocating larger transport windows. No whole-file collection, per-frame task,
unbounded queue, shared private-candidate map or uncharged detached writer is
introduced.

Do not add a process RSS reading to these ownership dimensions, or add enclosing
cgroup totals to RSS/kernel/file fields. The vector is a structural application
bound. OS socket/backlog state, thread stack mappings, native engine allocations,
allocator overhead and process/cgroup observations retain their own scopes and
availability. O10 must report allocator and OS observations as observations;
sampled maxima are not exact peaks, and unavailable domains are not zero.

The external Rust allocation observer selects two simultaneous 1 MiB, then two
32 MiB constructions/readbacks, four native service sessions, four co-hosted
client endpoints and the configured native telemetry runtime. Its prospective
256 MiB Rust-allocation ceiling is an instrumentation guard for this selection;
it is not an RSS ceiling, does not include SQLite's native allocator, and does
not substitute for filesystem-ordering or singleton worst-case proofs. Native
OS observations and the real Docker large-input rows remain separate evidence.

## Prospective functional qualification

Native executable and Docker product-route proofs select the unmodified Cargo
release profile, Rust 1.85.1, two build jobs and one construction producer per
operation. Host and aarch64-unknown-linux-musl artifacts are rebuilt from the same
runtime inventory; each run validates their hashes and the immutable image ID.
Initial release compilation has a separate 600-second build budget. This does
not change the functional command budget or qualify a prior debug executable.

The baseline release artifact failed the 4 GiB complete proof; its diagnostic
sample showed substantial time in the locked ChaCha software backend. A later
prospective ARM artifact selects the published `chacha20_force_neon` configuration
through `RUSTFLAGS=--cfg=chacha20_force_neon`, with the same Rust 1.85.1, release
optimization, source, lock and Noise suite. Both selected ARM targets declare
NEON. The stable-toolchain capability check passed despite the dependency's older
nightly-only documentation note. This does not change source in that dependency,
add a provider, add workers or relax a workload/timeout. Host/Linux build receipts
and a new image manifest must bind this compilation selection. Qualification
requires current-artifact functional tests and software-provider interoperability;
the baseline artifact remains a named reference, never an executable fallback.

Build and fixture acquisition are separately recorded. Each functional command
has a fresh output directory and an unchanged 60-second hard maximum; targeted
fault tests retain their shorter declared limits. No throughput comparison is
selected here.

1. Two native threads and independent handles write the same Store, including
   identical objects, reverse publication order, foreign dependency refusal,
   abort isolation, capacity/reclamation and ordinary reopen.
2. Real separate Docker daemons drive two successful overlapping operations with
   equal numeric request IDs in their separate sessions. One extra operation
   receives prompt Capacity while both admitted sources are barrier-stalled.
3. Stream generated input and incrementally validate returned data. Exercise
   1 MiB, 256 MiB and the selected 4 GiB boundary, plus over-limit declarations.
   Preserve source-pattern identity. Compressible and distinct-data proofs are
   labelled separately; neither is a claim about every data distribution.
4. Observe bridge/C1/C2/telemetry ownership with two writers and four session
   owners, then slow input/output, failed diagnostics and bounded shutdown. Record
   the actual host/Linux resources and independently scoped native observations.

A boundary which cannot complete within its verification command budget is
NOT_RUN/unqualified with its actual wall and reason retained. Do not reduce the
case, increase its timeout, silently select an old executable or move that missing
product proof into #193. #193 remains the separate later performance campaign.
