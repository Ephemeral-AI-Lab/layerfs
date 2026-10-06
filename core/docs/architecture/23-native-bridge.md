# Native authenticated bridge channels

> **Status:** Current general guide; implemented primitive, not complete S0/S9 exits.

The #307 checkpoint after `d084a1a8a` activates
[layerfs-bridge](../../crates/layerfs-bridge/src/native/mod.rs) with the selected
unmodified Snow 0.9.6 pin. macOS and Linux ARM64 use KK/Curve25519/AES-GCM/SHA256
and preserve root Cargo AEAD inputs. Missing ARM64 AES/Polyval build cfgs refuse
compilation; no runtime suite fallback or third-party source/registry edit exists.
Activating this previously excluded capability adds Snow's locked dependency
closure while preserving normalized pre-existing identities/checksums/edges.
Cargo.lock dependency version suffixes disambiguate newly coexisting versions;
they do not change an existing dependency edge.

[Handshake](../../crates/layerfs-bridge/src/native/handshake.rs) takes one supplied
TCP socket and provisioned expected static keys. It attempts KK once under a fixed
cluster-two prologue, permits no handshake payload, checks completion and actual
expected peer, and only then constructs VerifiedPeer. Provisioning public-key/keypair
functions cannot manufacture that authenticated type. SDK bind requires it before
application permission/scope/profile checks. Trust/rotation remains application work.

[Channel](../../crates/layerfs-bridge/src/native/channel.rs) owns one sender and one
receiver, separate scratch/nonces and shared failure state. Each can move to its
own I/O worker; the library starts no thread. A connection owns two socket handles
after try_clone. A two-byte ciphertext length is checked before resize; Noise
authenticates the exact body. Ciphertext is at most 65,535 bytes, plaintext at most
65,519 including future logical framing. Sender retains one cipher buffer;
receiver retains one cipher and one plain buffer. Receive results borrow the
current window. Handshake transient arrays/vector are each bounded at 256 bytes
and freed before channel readiness.

Large flows use arbitrarily many bounded records with checked native nonces; no
total byte/flow/command time cap exists. Nonce exhaustion is the physical protocol
limit. No command-specific bootstrap or Bash deadline exists. Successful native
channel creation explicitly selects TCP_NODELAY; other socket options,
readiness/admission and service workers remain caller-owned. Native proof socket
deadlines are external verification fences.

The 2026-10-07 native profile correction sets TCP_NODELAY once in
`Connection::new`, after successful KK authentication and before the direction
clone or record-buffer allocations. The option applies to the original socket
and its sender/receiver clones. An option failure returns the original typed I/O
error through the existing channel result; it does not retry, reconnect, create
direction buffers or reinterpret the completed handshake. Failed authentication
does not enter this channel-construction step.

This adds one socket-option syscall attempt per post-authentication channel
construction. It is separate from `ChannelWork` wire read/write attempts/calls
and byte counts; those diagnostics do not count socket configuration. The record
grammar still sends its two-byte length prefix and sealed body in separate
writes, including their positive partial progress. Canonical/wire bytes,
nonces, buffer capacities and bounded owners remain unchanged. Handshake also
uses this two-write grammar, but this selected correction configures the
successful channel rather than changing the earlier handshake phase.

The external [native test](../../crates/layerfs-bridge/tests/native.rs) retains
caller socket clones, explicitly starts both endpoints with NODELAY disabled,
creates actual public KK channels and reads back NODELAY on both authenticated
sockets. Duplex records and record/wire counters are checked without a latency
ratio or clock threshold. The fixture's socket/readiness deadlines bound its
waits; product timeouts and reconnect behavior are unchanged. Test source is
not a passing receipt, and selecting this option does not establish numerical
latency, resource or complete S7/S9 qualification.

[I/O](../../crates/layerfs-bridge/src/native/io.rs) completes positive partial byte
progress and returns every error, including Interrupted, without retry or busy
loop. A read/crypto/write failure quarantines both directions and attempts socket
shutdown once. CloseFailed retains an original operation error plus secondary
shutdown failure. Subsequent calls refuse before another attempt. Oversized caller
records are pre-attempt admission and consume no nonce. Explicit close atomically
fences both directions before one shutdown attempt. Transport failure does not
infer whether Workspace/Save/history publication ran, or delete/resend it; owning
adapter completion fences must establish that disposition.

Fixed diagnostics expose completed records/plain bytes, actual positive I/O calls,
wire bytes including partial failures and owned capacities. Cumulative counters
saturate without imposing a data limit. Work is O(B) over B flow bytes with bounded
record scratch and no flow-sized vector or descriptor chain. Socket/kernel/crypto
residency, aggregate threads/queues and service fairness remain qualification work.

Public native TCP proofs cover full/short/empty duplex records over multiple
windows, buffer/counter observations, oversized refusal, wrong static identity,
malformed length and corrupt ciphertext. Failure quarantines both directions
without frame/nonce replay. SDK proofs now bind through real native handshakes
before real macOS Store calls. These are capability/local integration proofs;
object/history/control codecs, multiplexing, credits, fair dispatch and disconnect/
restart custody remain required. The pinned fuser timestamp gate is unaffected.

The predecessor is preserved intact in excluded layerfs-bridge-legacy, with only
manifest identity changed. Its 6,834 production LOC remain counted. Retired
deadlines, timeout retry loop and prepared-construction routes are no active
dependency/fallback. S11 removes it after replacement coverage.

The subsequent [runtime wire ownership](40-runtime-wire-ownership.md) checkpoint
adds actual contract/codec/native fragmentation and shared partial/result credits,
separate message/correlation identities, explicit worker close handles and observed
failed handshake/record costs. SDK owns operation/receipt codecs and input/output
workers. Original native APIs remain usable. Full S7/S9 resource/restart/root exits
remain open; the owner-accepted fuser correction and Docker proof supersede the old
published-package wait recorded above.
