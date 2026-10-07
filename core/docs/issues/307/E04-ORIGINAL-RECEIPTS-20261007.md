# E04 original receipts and allocation failure

> Status: implementation and diagnostic checkpoint; S7/S9 remain incomplete.

This checkpoint preserves the pending attachment/collector/resource work at
parent `a08bbe39deb7d892a7a5a69b0daa001ab7e53774` and executes the first actual
E04 diagnostics. No complete 1,000-write window passed. All 27 E1 selections
remain NOT_RUN/count zero, qualification NOT_EVALUATED and admission false.
E05 and a Durable E04 execution remain NOT_RUN. The owning
[reconciliation](S7-S9-EXIT-RECONCILIATION-20261007.md) and
[file plan](S7-S9-IMPLEMENTATION-OWNER-20261007.md) retain the independent exits.

## Source and checks

The original attachment API returns the successful Open Completion and original
Binding/Policy Messages with their actual credits. Existing four host and four
Linux cases are retained at unchanged attachment source. The external E04 vehicle
uses real Project Init/Store/Save/fork/runtime, authenticated Upstream, Workspace
mutations and the existing Owner. It has a closed-input independent all-byte/EOF/
portable-facts/two-inode oracle, but none of these failed invocations reached its
complete oracle. Captured Binding equality is not a new Branch-history query.

The resource helper preserves primary read/output failures, acknowledged prefixes
and secondary close failures with no retry. E04 validation preserves absent
pre-Open routing, exact response namespace and actual executed Docker command,
CID-file, image, inspected process/environment and bind paths. It preserves E04's
actual NOT_RUN Stop on startup failure.

| Check | Retained result |
| --- | --- |
| Resource syntax and final 18 cases | PASS, receipts 30/31; original dual read/close failure in 24 |
| Validator syntax and 87 cases | PASS, 36/37; three original Open/Docker witness failures in 25 |
| Additional startup-failure case | Original FAIL 41, correction PASS 46/47; 88 distinct cases, no second full-suite run |
| Final host/Linux example builds | PASS 63/64, source inventory 62 and binary/build witness 65 |
| Host all-target Clippy | Reused unchanged attachment/collector check 21; affected final host example PASS 66 |
| Linux all-target Clippy | PASS 29; subsequent host-only source is cfg-excluded, consumer binary unchanged |
| Formatting | PASS 32 and affected final package 67 |
| Product boundary / self-tests | PASS 736 files / 41 bodies, receipts 33/34; no later production change |

Original build15/Clippy20 failures remain retained. Read-only summarizer failure
61a is also preserved. Tests and diagnostics all have explicit wall stops; no
command reached the repository 120-second test ceiling. There is no CI claim.

## Actual diagnostic outcomes

All rows use debug functional binaries, uncontrolled caches, one ordinary
construction producer and the separately supported four-worker native Init.
They reuse the closed base and replacements through independent byte copies;
no cold or measured-performance claim follows.

| Original selection | Complete external wall / bound | Actual outcome | Custody |
| --- | --- | --- | --- |
| Receipt 50, initial host | 16,588,374,791 ns / 90 s | FAIL before attachment; 0 writes; accepted socket remained nonblocking, host WouldBlock and consumer UnexpectedEof | Both processes exit 1; original container and artifacts retained |
| Receipt 60, blocking accepted socket | 20,291,636,667 ns / 90 s | FAIL: host independent input worker's inherited read timeout expires while no RPC is admitted; 67 Applied results, 66 complete write traces | Consumer explicitly stopped after host failure; exit 137 without OOM; final reply/release for publication 67 unavailable; no terminal consumer manifest/oracle |
| Receipt 68, persistent host input | 76,385,835,625 ns / 90 s | FAIL: Namespace attempt index 94 returns original pre-BEGIN ENOSPC; 95 attempts including failure, 94 successful writes | Original failed Completion, operation/source/Owner retained through process error exit; Stop NOT_RUN, oracle absent; no guessed Close/cleanup |

The host corrections do not retry failures. The final profile keeps the existing
per-I/O handshake read/write timeouts, active consumer read timeout, job/fence
limits and 80/90-second product/external watchdogs. Only the host input worker's
idle read timeout is cleared after successful authentication; it waits for the
next independent request until explicit fencing. No whole-handshake wall is
inferred from a per-I/O socket option.

Raw run roots under `core/target/cluster2-307/` are:

- `e04-owner-disposable-20261007/`
- `e04-owner-disposable-socket-fix-20261007/`
- `e04-owner-disposable-persistent-input-20261007/`

Each retains exact original commands, image/toolchain/source copies, pre-consumer
fixture bundle, host and consumer outputs, startup/job/probe/trace prefixes and
actual container inspect. The first failed retained validator returns FAIL in
51. Later prefixes have no complete successful mapping/oracle; no narrow
write_window_consistency PASS is claimed. Eleven source-scoped gaps remain.

## Physical allocation finding and approved disposition

The third file has 626,688 logical bytes but 76,504,104,960 allocated bytes.
Startup plus all successful original-job allocation requests equals that exact
physical total. The final failed Namespace has zero changed rows and zero payload
copy work; earlier publications remain real state. Source calls Linux
KEEP_SIZE for the required range on every admission. Current readback checks
minimum capacity, so additive allocation is not rejected before ENOSPC.

The observed `/work` host bind is `fakeowner`, filesystem magic `0x6a656a63`.
The prospectively declared small [range diagnostic](checks/e04-writes-20261007/74-range-diagnostic-selection.json)
makes exactly two successful 65,536-byte KEEP_SIZE calls to the same fresh range:

| Actual path domain | After first / second call | Observation |
| --- | --- | --- |
| Host share, magic `0x6a656a63` | 65,536 / 131,072 allocated bytes, EOF 0 | Additive: identical range consumes new allocation |
| Guest overlayfs, magic `0x794c7630` | 65,536 / 65,536 allocated bytes, EOF 0 | Equal in this finite primitive diagnostic |

[Raw receipt 75](checks/e04-writes-20261007/75-exact-range-cause.json) and inspect 76
retain the exact command, image, CID, mounts, observations and files. This is not
a full-device capacity qualification. Prior S6 ext4 proof remains separately
scoped: repeated requests did not grow its established allocation. Neither
overlayfs nor the host share is silently called ext4. Do not bypass the Linux
range primitive, replay E04 on the failed backing, or infer positions from blocks.

The three failed E04 files together reported 131,130,720,256 allocated bytes.
The human explicitly approved replacing only those originals after independent
logical-byte copies and hashes were retained in
[receipt 69](checks/e04-writes-20261007/69-failed-allocation-custody.json).
[Receipt 70](checks/e04-writes-20261007/70-authorized-physical-replacement.json)
records atomic byte-identical replacements, original/new identities and the
131,129,397,248-byte reduction in allocation charged to the current paths.
This number is a path-allocation delta, not proof that the filesystem reclaimed
the bytes: the virtualization process still holds read-only descriptors to the
three unlinked originals. Physical reclamation remains pending those releases.
All Store/input/receipt/container data is preserved; no unrelated owner or VM
was interrupted. The original allocation high-water evidence is historical and
is not relabeled as the smaller replacement.

## Identity and remaining exit

Source is base commit plus exact uncommitted inventories; no sealed arm is claimed.
The Linux binary remained SHA256
`3ab78cc85fa1a5e05ee25329818abe27957adc6b7d076898f081188fc506ab6d`.
Final host binary is
`7b5e6e32ac69e7992d1f6a368c391a6179bd4d676864cf4787d4b1f9e21cd87b`.
Image is `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
Base input is 16,777,216 bytes, SHA256
`a813c22ec9b6429809beafdeed2903207e31f2c68c4ab5795c11a857d26e6f17`;
replacements are 4,096,000 bytes, SHA256
`c9440baeb12b6f5d6fe142092fe3f862611f8c4ab36788a078922ea764489290`.

[Artifact observation 77](checks/e04-writes-20261007/77-retirement-and-binary-availability.json)
records an unexpected later change: current Cargo outputs and the archived Linux
binary copies disappeared, while source and receipts remained. This task issued
no target/archive deletion, and the external cause is unavailable. The first two
archived host binaries remain; the final host archival copy could not be made.
Recorded execution hashes remain historical observations, not currently reusable
binary artifacts. New checks require a new accurate build identity. Global free
space increased separately; open-original descriptor evidence still governs any
claim of reclaiming the failed files' physical allocation.

The next selected correction is explicit admission refusal for the evidenced
unsupported backing before bulk allocation, then a prospectively declared native
guest backing witness and E04 mapping successor. This is engineering work, not
authority for an automatic fallback or a waived failed sample. A new file plan
precedes implementation. E2 family attribution and actual receipt/queue capacity,
E1 registration, E3 calibration, E4 service/debt, S8 native ownership and S9
R1–R4/Q1 all remain open. Q1's owner-approved dense execution is now 500,000,000
bytes; >4 GiB execution is NOT_RUN — waived by owner, with large-offset correctness
still required. S10 full Commit, S11–S13 and reference retirement are not selected.

Production LOC is recorded from exact parent/final staged/committed snapshots in
the checkpoint commit and separate comparison receipt. Examples, tests, tools and
these diagnostics are excluded from that headline. The reference remains retained.

Exact [comparison 79](checks/e04-writes-20261007/79-production-loc-comparison.json):
**Production LOC: 170041 -> 170068 (delta +27)**. Core 104624 -> 104651 (+27),
root reference 65417 -> 65417 (+0), active core 61459 -> 61486 (+27), excluded
predecessors 40321 and excluded integration 2844 unchanged. Content remains
18613 (its earlier +5113 over the #303 baseline is unchanged). The addition is
original attachment-result custody, with no relocation or retirement credit.

Local links/anchors pass in receipt 78. Whitespace checking reports trailing
blank lines in the original raw attachment stdout receipts 02 and 06; those
bytes are preserved rather than edited. Source/document whitespace is checked
separately, and no aggregate clean/CI claim is made.
