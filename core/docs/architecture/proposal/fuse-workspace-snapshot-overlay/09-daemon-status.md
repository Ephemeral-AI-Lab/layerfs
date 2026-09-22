# Authenticated daemon Status

> **Status: Proposal; target LayerFS v0.1.7; not a released contract.**
> R1-C first operation, implemented against R1 commit
> `598d8405f1168f5f8409807c4f3b6e95d1a110c9`, 2026-09-21.

This adds one daemon-targeted observation to [the R1 readable mount](08-readable-implementation.md).
It does not complete all R1-C lifecycle controls or expose writable operations.

## Contract and authority

`Operation::WorkspaceStatus` uses profile 3/opcode 8 with an exact managed
Workspace ID (1-63 ASCII letters/digits/underscore/hyphen) and nonzero 32-byte
incarnation. Store and outer generation are zero, no input/result body is
permitted, and the request deadline is at most 5,000 ms. Maximum encoded request
is 124 bytes. The fixed bounded result is at most 139 bytes: echoed identity,
mounted/stopping/closed flags, active operations, nodes, handles, cookies and
aggregate consumer-accounted Workspace bytes. This is a present local
observation; it never proves the outcome of an earlier operation.

The existing Noise authentication, `Client::call_until`, Source, framing,
`server::serve` and three-byte non-history failures are reused. The service
refuses opcode 8 before Store lookup/admission. No Store permission bit maps to
it, even for a grant mask of 255. The daemon rejects content/history operations.

Control is opt-in, with both settings required together:

```text
LAYERFS_CONTROL_LISTEN=<explicit socket address; no default>
LAYERFS_CONTROL_PEERS=<selector>,<public-key-hex>,<expiry-unix>,<mask>[;...]
```

At most 16 peers; mask 1 grants Status and mask 0 does not. Unknown bits and
duplicate selectors or public keys fail configuration. Headless mode rejects
control configuration. The existing daemon private identity is the control
server identity; the external controller pins that public key. Its independent
peer list grants access only to the Workspace/incarnation already launched by
this daemon. Wrong target, incarnation, grant or expiry yields Denied without
returning an observation. Expiry is checked on every request and again after
complete empty input, including on an established session.

The listener binds before attach, then starts accepting only after mount.
Readiness names its actual endpoint. One acceptor and at most one session worker
have 2 MiB configured stacks each; one owned socket clone supports shutdown.
Excess accepted sockets close immediately (Q=0), so excess clients can receive
transport I/O failure rather than an invented authenticated Busy result. These
threads and bridge buffers are separate from Workspace's working-byte account.

On shutdown, control admission stops, the owned socket closes, and workers join
before mount teardown, sharing the same absolute 10-second shutdown allowance.
Status does not use the Workspace remote-operation permit or hold state across
network I/O. No retry, second transport codec, history algorithm or manager was
introduced.

## Verified route and evidence

[The actual control receipt](evidence/r1c-status-20260921/status-01/result.json)
records authenticated host Client -> Linux container daemon -> live Workspace.
The host uses the production headless binary's existing Client binding, pointed
explicitly at the daemon control endpoint. That endpoint executes the new local
Status handler; it does not forward the query to the content service.

The R1 fixture was closed and reused through independent byte copies of its
Store/catalog, with master/copy SHA256 equality recorded. The service opens the
history copy read-only. No original fixture, earlier receipt or performance
sample was reused as a new operation. No cold-cache claim is made.

| Proof | Actual result |
| --- | --- |
| Exact target / observation | Echoed Workspace/incarnation, mounted state and bounded counts match the launched mount |
| Real handle lifetime | Opening one FUSE file changes handles to 1; close returns it to 0; mounted bytes remain correct |
| Local progress | While a FUSE read waits on the paused service with an established TCP connection, Status completes and observes one active operation |
| Session capacity | A second authenticated connection is refused while the first remains usable; released session capacity is reclaimed |
| Independent authorization | Wrong Workspace/incarnation and authenticated peer without Status grant return Denied |
| Service boundary | The service refuses Status even with all Store grant bits |
| Established-session expiry | 93 explicit observations keep one session active across actual grant expiry; the expired request is denied, not replayed |
| Shutdown | An accepted silent handshake is interrupted by owned socket shutdown; control joins, mount unmounts and its child path is removed |

Complete functional-command wall was **29.148174209 seconds**, under the
60-second hard budget. This is verification wall time, not Status latency,
throughput or a performance sample. The receipt retains source/product-input,
host/Linux binary, image and driver identities.

Observed Workspace accounting was 1,387,904 bytes idle and 1,535,360 during the
held read: a 147,456-byte difference, comprising the declared 131,072-byte call
reservation plus the 16,384-byte read buffer requested by that kernel callback.
These are two working-allocation observations, not a peak, RSS/cgroup result or
an allowance for the separate FUSE/control/bridge memory. A subsequent demand-
grown registry correction changes idle bookkeeping and requires its own source
and count result; this receipt is not relabelled after that change.

Checks: locked Rust 1.85.1 bridge/service/daemon tests; host whole-workspace
examples/bins build and all-target Clippy with warnings denied; Linux daemon
build and all-target daemon/FUSE/Workspace Clippy; formatting, product-boundary
guard and its six self-tests. Codec tests cover malformed identities, fields,
flags, profiles, deadlines, wire maxima, exact result association and forbidden
ResultData. These codec/native-unit checks do not impersonate a malformed
ciphertext campaign against the running daemon.

Reproduce with fresh output after building the host examples/binaries and Linux
binary exactly as in 08:

```sh
LAYERFS_CONSTRUCTION_WORKERS=1 python3 core/crates/layerfs-daemon/tests/control_status.py \
  --linux-daemon "$PWD/core/target-linux/debug/layerfs-daemon" \
  --fixture "$PWD/core/target/pair1-evidence/mounted-07/result.json" \
  --output "$PWD/core/target/pair1-evidence/NEW-STATUS-OUTPUT"
```

Remote Attach, Mount, Unmount, CloseClean, edits, Stage/Commit controls, malformed
authenticated frames against the daemon, save-overlap qualification, npm and R6 remain
open. No issue is closed by this observation-only operation.

## Production source comparison

Production LOC: **98,831 -> 99,288 (delta +457)**. Reference: 65,417 unchanged;
replacement core: 33,414 -> 33,871 (+457). The comparison is first-parent R1
`598d8405f` to the exact staged Status tree, using `tools/production_loc.py`
(blob `b5b9617d08204977176302311e0b2c72a811b420`) on Git archives of
`crates` and `core/crates`. Product Rust and runtime SQL count; tests, examples,
fixtures, docs, tools, manifests, blank lines and comments do not. The staged
product-input seal matches the actual Status receipt. Reference code remains;
there is no legacy retirement or measured improvement claim.
