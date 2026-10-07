# Independent source-review disposition

Main alone edited/built/tested/used Docker/proof/cleanup. Existing agents reviewed
read-only protocol/transfer, SDK/ownership and native closing/cancellation.

- Shared Store parent tar entry: FIXED by removing it; existing-Store injection
  never repairs borrowed parent metadata. Final tar oracle rejects Store entries.
- Blocking Unix connection admission outside absolute listener wait: FIXED using
  existing nix nonblocking socket/fs/poll capability, one connect, EAGAIN refusal
  and only EINPROGRESS completion observation. No reconnect or new direct crate.
- Marker inside unfinished/truncated standard frame: FIXED; marker fact survives
  while frame completion/error determines wait success. Focused test PASS.
- Final log-fence failure lost HTTP header attribution: FIXED via original Response
  metadata helper without a second shutdown attempt.
- SDK clear-wait phase after authentication: FIXED distinct Policy phase; original
  authenticated Control retained on failure.
- SDK install: same Connection/correlation; attempted failure terminal; original
  InstallFailure retained. Managed Exec selects admitted identity. Source review
  found no remaining blocking SDK custody/topology/readiness claim defect.
- Actual macOS-controller close race59: FIXED daemon final reply send -> original
  caller EOF -> one fence, retaining original Served on terminal failure. No
  ENOTCONN suppression/retry. Mac responder failed fence remains a distinct real
  outcome. PeerEndFailure now retains Admit/Read/Fence and actual fence-attempt
  facts; these distinguish read failure from EOF followed by failed fence.
- Terminal native socket scope: live socket retained while waiting; after a
  terminal failure slot retains original receipts while Connection drops. Full
  recoverable socket-owner/application close/join is NOT implemented or claimed.
- Positive example String failures: explicitly scope it to positive-path
  qualification; typed adapter failure tests/source own failure custody claims.
- Received startup-byte counts include read-ahead; selected port mapping only,
  not whole Ports-map exclusivity. Both limits documented.

Verified actual paths/pins/results govern claims, not this review alone. Exact
external process cancellation, native FUSE Ready/live namespace Commit and
application drain remain open. No old campaign/failure is relabeled.
