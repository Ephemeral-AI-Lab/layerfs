# Single authenticated pack-read boundary

Status: concrete product treatment; matched53measurement pending.

The prior53candidate atd8b1df33f needs80.446455584s versus68.721138333s reference;
save/custody+15.517267632s and candidate proof9.51200425s timeout. Returned payload
7653915810B plus metadata-pack5621630117B (work counters, not unique/device bytes).
Source shows each SQLite BLOB checked with ObjectKey SHA256 and C2 Fetch hashes
that same body again. All active production read consumers are Fetch's two paths.
Direct public persistence callers also rely on digest refusal, so neither boundary
can simply stop validating untrusted mutable carriers.

PersistedPack becomes an immutable length/digest-verified read result with private
fields, authenticated constructor, read-only getters and consuming transfer.
SQLite uses that C2-owned constructor for its existing body verification. Fetch
uses the immutable verification guarantee without another full-body SHA256, while
retaining every descriptor/membership/cardinality/byte/framing/domain/group and
canonical-chain check. Rebuilding from altered consumed bytes fails authentication.
The read port source API changes from public fields/struct literals to constructor
and accessors; active callers/implementors updated and all-targets checked. Raw
PublishedPack API, persistence schema, budgets, caches and durability unchanged.
No third-party modification/dependency. Reference product remains unmodified.

Validation:2external read-carrier tests PASS (identity/length/consumed-byte refusal,
hash-valid wrong-domain C2refusal). Domain fixture originally assumed every emitted
pack was Payload; corrected from actual object locator to flip that specific pack.
5existing read/pool/order/custody tests PASS;13publication+9transaction tests PASS;
public Init fullnamespace100/1000bothprofiles oracle PASS. Storage/persistence/
project all-target Clippy-Dwarnings, formatting, boundary443 and23self-tests PASS.
No full-workspace test claim. A source-level private-field compiler refusal caught
one persistence wrapper getter update; corrected before verification.

Prospective matched ordinary53v2 one sample per arm at this frozen product/harness,
release/locked binaries, current cold contracts,170s complete/9.5s proof, strict
storage<70427034B, <=1.10 time. All earlier failures/pass artifacts remain their
original identities; no unchanged speed retry or receipt promotion. New verified
carrier does not itself prove latency benefit; measure before claiming it.


## Matched53v2 at6b631aad2

| Scope | Reference | Candidate Disposable |
| --- | ---: | ---: |
| Product lifecycle |69.065836125s|74.695010625s|
| Complete performance command |86.579863250s|93.272690833s|
| Full independent proof |8.539828542s PASS|9.507007292s TIMEOUT|
| Acquisition |23.061718212s|23.622823708s|
| Construction |3.209201083s|3.138084211s|
| Filesystem |14.337590080s|10.730440788s|
| Save/custody |27.804237295s|36.364801786s|

TimePASS1.081504471904x, integer746950106250<=759724197375. Storage candidate
62611456B<70427034B, source/statecold/root/canonical/cleanup PASS; original manifests
rechecked. JointINCOMPLETE because candidate proof times out; no root/census
substitute or original failure promotion. Returned-read/encoding work preserved;
source authentication now once per acquired pack. Historical d8candidate save
43.393351043s is a different source/window, not a pooled speed comparison.

Fresh labelled candidate native53count diagnostic on retained closed store at same
product completes44states thenTIMEOUT9.508180250s (observers/cold/native-only scope),
owner preservation/cold PASS.44walk82.309916ms versus lengths/sample332.275792ms;
2231body acquisitions/280255666requestedVFSbytes/107.567ms SQLstep in44. Metadata
memo18259hits/3120misses, peak2MiB/437rows. File/dependency acquisition remains
next mechanism; no additional hash or frame check removed. Ordinary shared proof
is unchanged in scope; diagnostic cannot promote it.157/DurableNOT_RUN; prior
Init/17qualifications remain separate pinned identities. Goal ACTIVE.
