# Issue #192 acceptance closure run — identity set v9

> **Status:** Research; informative and not a product contract. Evidence for the
> owner's acceptance matrix; nothing here is a release claim and nothing is committed.

Date: 2026-09-20. Run on the current worktree source (my bridge transport slice on
top of the owner's uncommitted work) with the owner's own harnesses, unmodified.
Identity set: `evidence/optimization-release-artifacts-v9-20260920.json`
(183 pinned runtime files, 7 pinned binaries, scratch daemon image
`layerfs-issue192-daemon-v9` = `sha256:4781bcdf2f8a71d…`, profile `release`).

## What passed

| harness | selection | cases | result |
| --- | --- | --- | --- |
| `docker_route.py` | `--telemetry off` | 11 | **PASS** — V02(×2), V03, V04, V05(×2), V06, V07, V14, V16, V17 |
| `docker_route.py` | `--telemetry forward` | 12 | **PASS** — the above plus **ENV03** |
| `docker_route.py` | `--telemetry both` | 12 | **PASS** — the above plus **ENV04** |
| `docker_faults.py` | `errors` | 16 | **PASS** |
| | `additional` | 10 | **PASS** |
| | `cleanup` | 4 | **PASS** |
| | `envelope` | 3 | **PASS** |
| | `admission` | 2 | **PASS** (C-plus-one, A-plus-one-Q-zero) |
| | `writers` | 2 | **PASS** |
| | `blocked-stderr` | 1 | **PASS** |
| | `lost-response` | 1 | **PASS** |
| | `large-distinct` | 1 | **PASS** |
| | **`slow`** | 0 | **FAIL — pre-existing, see below** |

75 case records PASS with `cleanup: PASS` on every passing receipt. The fault
selections supply the substance of V08–V13 and V15 under their own case names
(`truncated-header`, `oversized-frame`, `unsupported-opcode`, `operation-denied`,
`store-denied`, `untrusted-key`, `unsupported-profile`, `slow-upload-deadline`,
`blocked-result-consumer`, `early-refusal-unread-stdin`, `disconnect-before-end-input`,
`lost-result-after-confirmed-save`, `maximum-connected-native-roles` = one service /
four real daemons / two active mutations with response isolation, and the cleanup
family). A case↔row mapping still needs to be recorded in the receipt itself.

## The one failure, and its bisect

`docker_faults.py --selection slow` fails with
`TimeoutError: blocked stdout did not close at deadline` (the `blocked-result-consumer`
case: a 2 MiB read with a 250 ms deadline whose consumer never drains, expected to
stop the container within 3 s).

**It is not caused by this slice.** Two bisect runs, each rebuilt, re-imaged and
re-manifested:

| identity set | bridge sources | result |
| --- | --- | --- |
| v7 (bisect) | `pipe.rs` restored to the owner's 512-byte cap; rest of the slice present | **FAIL** |
| v8 (bisect) | **all three** bridge sources byte-identical to the owner's pre-edit baseline (hashes 194b25dc / 9ee4a5e5 / 3306bf44) | **FAIL** |

The same error on the owner's own bridge sources places the cause outside this
slice — in the current tree's service/daemon/telemetry changes or the environment.
It needs its own investigation before O10/T10-style resource claims.

## One hardening kept, with its limits stated

`Pipe::write` is now whole-slice **and** sets `O_NONBLOCK`, so a full pipe returns
`EAGAIN` and is retried under `ready()`'s deadline instead of blocking past it. The
old 512-byte cap had the same unbounded-blocking property (a full pipe blocks a
512-byte write too), so this is a genuine tightening — but the `slow` case cannot
validate it, because that case fails on the baseline as well. No claim is made that
this fixes `blocked-result-consumer`.

## What this closes and what remains

```text
 closable on this evidence:  M2 (bridge + real handlers + direct parity: V17)
                             M3 (real container daemon, route, mount-free: V14/V16/ENV04)
                             M4 (five operations, fault families, multi-daemon isolation)
                             M6 (tests/fmt/clippy/boundary/identities/cleanup)
                             ENV03, ENV04 re-established on current identities
 still open:                 M0  commit + published wording amendment (Noise suite/profile)
                             M1/M5  telemetry T01–T16 and ENV01/02/05/06 — still zero receipts
                             O01–O11 — still zero receipts
                             D01–D05 — still unfrozen
                             the pre-existing `blocked-result-consumer` failure above
```

## Amendment 2026-09-20 — `slow` FAIL closed (append; the FAIL row above stays)

`docker_faults.py --selection slow` now PASSes all three of its cases in **1.650 s**
against these same v9 identities, with `blocked-stderr` restored and re-measured
separately (0.618 s PASS, 200 calls). The registered `slow` FAIL row above is retained
as history: it aborted at the 3 s stop-wait inside the daemon's 5 s idle window, and
only its first case was reachable. The full-pipe hazard that the `slow` case could not
reach end to end at any payload that fits the 50 s command budget is now proven
deterministically by `layerfs-bridge/tests/pipe_deadline.rs` (201 ms against a 200 ms
deadline). Nothing is proved by a payload size that Docker attach buffering absorbs.

Details, hashes, both evidence JSONs and the harness diff:
`evidence-f5-batching/slow-selection-speedup-v1.md`,
`slow-selection-before-v1.json`, `slow-selection-after-v1.json`,
`blocked-stderr-after-v1.json`, `docker_faults-slow-speedup.patch`.
