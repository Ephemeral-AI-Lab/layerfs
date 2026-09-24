# #232 1 MiB Exec/FUSE round-trip iteration — retained evidence

Status: **counts improved in instrumentation, not in crossings.** This package
retains one labelled final sample, one independently verified receipt, the
daemon's own log stream, and the diagnosis of a lever that was attempted and
withdrawn. It does not change the #232 baseline report, the registry, the
targets or the frozen receipts.

## 1. What this round added

`core`'s public post-timer status route now reports **projection data-callback
byte totals and a bounded request-size histogram** beside the existing callback
counts. The specification's §2 already declared byte totals a product
prerequisite before a case can be qualified; the histogram is what distinguishes
a large request answered short from a large transfer the caller split.

A labelled audit of that route also found and fixed a defect it had introduced:
the daemon compared the workspace's combined 36-row histogram against 18
declared buckets and failed closed, so **every** mounted workspace returned
`Integrity` from `WorkspaceApi::status`. The frozen campaign's receipts predate
the defect and still show their real counts; they were not rewritten.

## 2. Identity of the retained final sample

| item | value |
|---|---|
| source commit | `6c7d8eda008b9bdbc2e8b4b47384794fbd885ee0` |
| source tree | `49ee2491db9b2816934f0223099cbfb6bd2a31a7` |
| source dirty | `False` |
| product seal | `a9917820c34ce0cfe46163078f6aba18b602e3df707fdd364840b5dd1e7d0716` |
| harness seal | `86a88760c505ceb63038b34abf063571cbe759418668045eaaeb2227d14dad49` |
| Cargo lock | `d6fb4800e048346f87b6c825bdc2f2d446f50e33a2ead40799147408b8dc7e9b` |
| image | `sha256:d49fb9e2e40d18346ec6debbc636138cb93c752894283e4906f9eeddc34a6eb3` |
| daemon | `31fd64371744d2b76c34ef4de752e7b6aafa7106a28f78229c5b9e64bc1f315e` |
| edit tool | `70d41e2868196bad3a70e266aaf8e20796965154aaa5da70964879d193545ff9` (unchanged) |

The label chain that follows this sample removes one `eprintln!` from the SDK
driver's status path (commit `853beedbd`). That line is emitted only when the
status call fails, it is not in the daemon, and the image and the packaged
binaries are byte-identical; the arithmetic below is therefore the same at
either label. It is stated so that no later reader has to discover it.

One sample, no rerun, `--verification skipped` during collection and a separate
`verify-edit` afterwards.

Commands, exactly as run:

```text
python3 core/benchmark/fs-bench-pro/runner.py run \
  --case overwrite-middle-4k-on-1mib-ops-1-exec-v2 \
  --out benchmark-results/fs-bench-pro/issue232-roundtrip-final-01/overwrite-middle-4k-on-1mib-ops-1-exec-v2 \
  --verification skipped
python3 core/benchmark/fs-bench-pro/runner.py verify-edit \
  --run benchmark-results/fs-bench-pro/issue232-roundtrip-final-01/overwrite-middle-4k-on-1mib-ops-1-exec-v2
```

## 3. Independent verification

`verification.json` — `status: PASS`, wall `670 595 458 ns`, coverage
`full-file`, `full_file_bytes_verified: true`, observed digest equals the
declared `f998113b…`, canonical root `83a5dadc…` differs from the pristine
`8fafdf06…`, extent count 56 from 54, published head commit present, historical
root match. `report-edit` was not run: it needs the whole 56-row campaign
directory, and this package retains one row. `report.txt` from the run is the
same information for this row.

## 4. Crossing table

`sdk-exec-fuse` op counts inside the timed window, from the retained
`telemetry.lft1`; the control crossings are the two required public calls.

| crossing | frozen v2 | this sample | floor |
|---|---:|---:|---:|
| control host→sandbox `WorkspaceApi::exec` | 1 | 1 | 1 |
| control host→sandbox `WorkspaceApi::commit` | 1 | 1 | 1 |
| upstream `Inspect` (baseline refresh) | 1 | 1 | 0 — R2 |
| upstream `EditFile` (content save) | 1 | 1 | ┐ |
| upstream `UpdatePortableMetadata` | 1 | 1 | ┘ 1 — R1 |
| upstream `HistoryCommand` (composite) | 1 | 1 | 1 |
| upstream `ReadFile` (base blocks) | 0 | 0 | 0 |
| **upstream total** | **4** | **4** | **3** |
| **crossings total** | **6** | **6** | **4** |

The product's own `upstream_calls` counter reports `4` in the sample's receipt
and `4` in the frozen receipt, and the retained `telemetry.lft1` shows exactly
`Inspect ×1, EditFile ×1, UpdatePortableMetadata ×1, HistoryCommand ×1` inside
the window. The crossing count is unchanged by this round.

## 5. Waterfall of the retained sample

| term | frozen v2 | this sample |
|---|---:|---:|
| `edit_commit_ns` (driver) | 34.37 ms | 35.23 ms |
| LFT1 root `sdk.edit_commit.fuse` | 34.37 ms | 35.23 ms |
| ├ `edit` | 15.55 ms | 16.19 ms |
| │ ├ `Inspect` | 2.78 ms | 2.96 ms |
| │ └ unassigned (shell spawn, FUSE callbacks, delivery) | 12.77 ms | 13.23 ms |
| └ `commit` | 18.82 ms | 19.04 ms |
| &nbsp;&nbsp;├ `EditFile` | 5.53 ms | 6.29 ms |
| &nbsp;&nbsp;├ `UpdatePortableMetadata` | 3.71 ms | 3.87 ms |
| &nbsp;&nbsp;├ `HistoryCommand` | 4.90 ms | 4.30 ms |
| &nbsp;&nbsp;└ unassigned | 4.67 ms | 4.58 ms |

Within `EditFile`: `service.begin_save` 2.00 / `service.edit` 1.83 /
`service.finish` 1.57 ms. Within `UpdatePortableMetadata`: 1.14 / 1.18 /
1.31 ms. Within `HistoryCommand`: `history.role_file` 0.43, `history.begin_save`
0.43, `validate` 0.11, `inodes` 0.12, `history.finish` 1.05, unassigned
2.16 ms.

Named upstream spans in the windows are 16.92 ms of 34.37 ms in the frozen
sample (49.2 %) and 17.41 ms of 35.23 ms here (49.4 %). The unassigned
remainder is sandbox-side and stays unmeasured: this is the honest ceiling of
the two crossing levers, and it is why the round did not declare victory on
them.

**Wall is a single retained observation at a different identity and is not
compared to the frozen row as a regression or an improvement.** The registered
target `g2_target_ms: 5.63` did not move; the row is `INELIGIBLE` under the
frozen cache contract for the same reason as before and is not a PASS.

## 6. What the round established about the levers

* **R1 (merge content and metadata) is real and is one crossing, not three
  milliseconds.** A merged operation was implemented and measured: the driver
  reported `upstream_calls: 2` where the baseline row reports 4, and the
  daemon's own log shows one save producing a content root and a metadata root
  (`service.begin_save 1.9 / service.edit 1.8 / service.finish 1.9` ms).
  The reply never reached the client: the daemon encoded and wrote the 90-byte
  result and the client did not observe it, so the Commit waited out the
  five-second silent-wait window and failed with an unknown outcome, at four
  identities and with the metadata base naming either the published sibling or
  the content root. The change is withdrawn unshipped
  (`revert(core): withdraw the merged content-and-metadata save`) rather than
  kept as a broken fast path.
* **R1's prize is smaller than the handoff's estimate.** Collapsing the two
  requests removes one crossing and the second save's `begin_save` and
  `finish`; the `Validate inode role` read and the metadata object work move
  into the first save rather than disappearing.
* **R3 is not testable in this row.** The frozen crossing budget's R3 term is
  zero here, and the new histogram confirms it: `read_request=0`,
  `read_returned=0`, `write_request=4096`, `write_returned=4096`. The 1 MiB
  overwrite performs no base read at all. R3 has to be measured on a shift
  case, where the tool moves 128 KiB blocks.
* **R2 is structural, not a bug.** The `Inspect` is the mutation path's
  required baseline refresh: `serial_original` runs it only when the node's
  baseline is stale, and it exists to learn the published content root that
  `EditFile` needs as its base. A node resolved from the published base is
  already canonical and takes the cached branch; this row's node is refreshed,
  so the crossing is what the contract costs.

## 7. Instruments this round leaves in place

| instrument | where | what it answers |
|---|---|---|
| projection byte totals + 18-bucket size histogram | public `WorkspaceApi::status` | what the kernel asked to move and how it was sized |
| daemon log retention (`daemon.log` beside each sample) | harness (`shared/edit_route.py`) | the sandbox's own LFT1 and diagnostics, which never reached the host before |
| driver status-failure report | driver stderr | tells an empty count string from a failed status call |

The daemon log stream is the instrument D1 asks for: the sandbox's own
`daemon.exec_spawn` / `daemon.exec_output` and its per-request spans now land
beside the sample's caller telemetry. `final-daemon.log` in this package is
that stream for the retained sample, and it is where the merged save's daemon
side was diagnosed.

## 8. Diagnostic limits

* single sample at a new identity; the frozen row is a different identity and
  the two are not pooled, compared as a regression, or promoted;
* the Linux FUSE/backing domain stays `INELIGIBLE`; the raw number is a
  declared-warm diagnostic;
* the sandbox share of `edit` (13.36 ms here) is still unmeasured — the daemon
  log gives its spans but not a per-FUSE-callback breakdown;
* the merged-save diagnosis rests on four labelled attempts, each at its own
  identity, and none of them is a performance sample.

## 9. Smallest next lever

The largest named term in the window is `EditFile` (5.03 ms) and the largest
unnamed term is the sandbox share of `edit` (13.36 ms). Neither has a measured
cause yet. The cheapest next step is therefore not another product change: it
is to read the retained `final-daemon.log` for this sample's `WorkspaceExec`
children (`daemon.exec_spawn`, `daemon.exec_output`) and to run the same
labelled diagnostic on a shift case, where R3's projection requests are
non-zero and the new histogram has something to classify.
