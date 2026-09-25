# #232 repeated-128 progress fix and proof

**Disposition:** the public SDK Exec→128 mounted EDIT ioctls→Commit route now
completes. One changed-product attempt delivered typed Exec, Commit, Status,
Unmount and Sandbox Delete; seven `daemon.exec_progress` LFT1 children record
authenticated markers sent after published Workspace revisions advanced. An
independent verifier subsequently passed on that attempt's unchanged
Store/history. The attempt's original combined receipt remains **FAIL** because
its first verifier rejected the new scenario ID before reading the Store. No
performance or cold-latency PASS is claimed.

The [proof contract](REPEATED128-PROGRESS-PROOF-CONTRACT.md) was committed before
sampling. The [pre-run manifest](REPEATED128-PROGRESS-PRE_RUN.json) pins the
closed prepared master, independent writable clone, binaries, payload, source
seals and image. The one [raw attempt](progress-proof-01/) used source
`7ee723ca6663d5c5d3b6c6e7b75be7069b281da1`, product seal
`683c9b510400880ce6e739f33a68cb809fb10df02920456ba3b108c0c60ff6c9`,
image `sha256:01dfab4c41e7ab4fa46c5db4316f3a851b587f300921c9ae16800c1ef15f5ca9`
and scenario `repeated-128-progress-proof-v1`. Its Store/history and private
cursor key remain at the ignored local path in `POST_RUN.json`; retained hashes
bind them. The raw files have a `SHA256SUMS` manifest. The historical Phase 1C
and earlier liveness-diagnostic failures remain unchanged.

## What changed

The daemon observes the mounted Workspace revision every 200 ms while its Exec
child runs. When a revision advances, it may send the existing one-byte native
progress marker, at most once per second. The client accepts this marker only
for Exec and within the request's exact progress-byte budget. It still requires
the final typed Exec response; the marker does not acknowledge an ioctl or
Commit. The 5 s native no-progress boundary and 30 s Exec absolute deadline
are unchanged. A child with no published Workspace progress still reaches the
existing no-progress failure. No worker was added, and no operation is retried.

## Retained outcomes

| Boundary | Result |
| --- | --- |
| Public SDK Exec | Typed COMPLETE; caller LFT1 7,087.828 ms, daemon LFT1 7,086.483 ms |
| Native progress | 7 `daemon.exec_progress` LFT1 child spans |
| Mounted route | 256 STATE, 128 EDIT, 0 WRITE callbacks; 4,096 accepted bytes; 0 shifted suffix bytes |
| Piece diagnostic | 128 splice count lines and 128 piece-page count lines; no piece algorithm change |
| Public SDK Commit | COMMITTED, 168.243 ms; recorded head and revision 130 |
| Public cleanup | Unmount PASS; Sandbox Delete PASS |
| Complete command | 10.067 s, under 15 s; original receipt **FAIL** because first verifier failed |
| First independent verifier | **FAIL**, 0.578 s: `complexity scenario and operation contract differ` |
| Repaired independent verifier | **PASS**, 1.901 s, same Store/history and Commit; no second Exec or Commit |

The verifier repair was declared separately in the
[repair contract](REPEATED128-VERIFY-REPAIR-CONTRACT.md) and pinned in its
[pre-run manifest](REPEATED128-VERIFY-PRE_RUN.json). It added the exact new
scenario ID to the existing complexity-contract check, with no product change.
The second verifier's source was `4def9a3f6`; its binary and the original
case, Store, history, performance receipt and failed verifier hashes were
checked before invocation. Its raw output and receipt are retained under
`progress-proof-01/verification-repair-01/`. It verified all 1,052,672 result
bytes and the expected SHA-256
`9895e8e40eb779da8c84625c77bed9a335fbe06a62c85cdb03c5846592949153`,
published and historical mode/mtime, retained old content and head, reopened
Branch, chunked representation and canonical root shape. As in the frozen
Phase 1C contract, the exact new canonical root was observed rather than
predeclared; the full-content digest and historical identities were checked
independently.

The pre-sample macOS Store cache check passed with zero resident pages. Linux
FUSE backing remains **INELIGIBLE** for cold latency because invalidation and
whole-input residency checks are absent. These timings are route diagnostics,
not admission numbers. The completed 56-case single-edit campaign, cold-cache
admission question and capped-500 MiB raw aim were not changed.
