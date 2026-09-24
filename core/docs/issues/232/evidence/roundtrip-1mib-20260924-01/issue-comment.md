## 1 MiB Exec/FUSE round-trip iteration: instruments added, one lever withdrawn, crossings unchanged

Worktree `/Users/yifanxu/.codex/worktrees/issue232-roundtrip-1mib/layerfs`, branch
`codex/issue232-roundtrip-1mib`, 28 commits from `b0bd10422`. **The branch is
local and has not been pushed.** The baseline report, the registry, the targets,
the SPEC and every frozen v2 receipt are untouched (`git diff b0bd10422 HEAD`
over those paths is empty).

This round answered the owner's ask for the 1 MiB case by driving the crossing
count and the work behind each crossing, not by chasing a wall. It did **not**
reduce the crossing count. It is reported as a miss.

### What the row measures

| crossing | frozen v2 | this round | floor |
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

The product's own `upstream_calls` counter reads `4` in both receipts, and the
retained `telemetry.lft1` shows exactly `Inspect ×1, EditFile ×1,
UpdatePortableMetadata ×1, HistoryCommand ×1` inside the timed window.

### Final sample and verification

Source `6c7d8eda0`, tree `49ee2491d`, clean; product seal `a9917820c`, harness
seal `86a88760c`, image `sha256:d49fb9e2e`, daemon `31fd64371`. One
performance sample with `--verification skipped`, then a separate identity-
matched `verify-edit`: **PASS**, `full-file` coverage, observed digest equals
the declared one, canonical root differs from the pristine root.

`edit_commit_ns` **35.23 ms** against `g2_target_ms` **5.63** — `TARGET_MISS` as
recorded. The row stays `INELIGIBLE` under the frozen cache contract. The target
was not moved, this sample is not pooled with G2, and the 34.37 → 35.23 ms
difference is spread between two single observations at different identities,
not a regression or an improvement.

Of the 35.23 ms, 17.41 ms (49.4 %) is named upstream Service span and 17.82 ms
is sandbox-side and still unmeasured. The frozen row's split is 16.92 / 17.45 ms.
**Fixing the two redundant crossings is worth roughly 5 ms of ~35 ms, not the
whole distance to target**; that ceiling is now measured rather than assumed.

### Instruments added

**D3 — projection byte totals and request-size histogram.** The public
post-timer `WorkspaceApi::status` route now reports how many bytes the kernel
asked the projection to move, how many it moved, and an 18-bucket power-of-two
histogram of the request sizes. On this row: `read_request=0 read_returned=0
write_request=4096 write_returned=4096`, write in the `2k-4k` bucket.

This also settles an open question from the handoff: **the 1 MiB overwrite
performs no base read at all.** R3's term in this row is zero by construction,
so R3 can only be measured on a shift case, where the frozen campaign already
shows 7–8 projection requests for 4 declared 128 KiB blocks.

**D1 — the sandbox's own log stream.** The harness now attaches
`docker logs --follow` for the sandbox's life and retains it as `daemon.log`
beside each sample. Until now the daemon's `role=2` LFT1 never reached the host:
the sandbox launch consumed the daemon's first output and later records went to
a container log nobody read. The sandbox's `daemon.exec_spawn`,
`daemon.exec_output` and per-request spans are now retained with the sample.

**An audit with D3 found and fixed a defect.** The daemon compared the
workspace's combined 36-row histogram against the 18 declared buckets, failed
closed, and the status route returned `Integrity` for **every** mounted
workspace. It changed no number in this round — nothing in the crossing path
depended on it — and it is fixed with an external test covering the combined
list, both directions, a short list and an unknown label.

### R1: implemented, measured, withdrawn

`Operation::EditFileWithMetadata` was implemented to publish one dirty file in
one crossing — the edit and the portable fields it stamps under one save owner,
with `Response::Saved` carrying the metadata root the same save produced. The
Workspace used it for every published file, keeping the separate metadata
request only for a fresh file, which has no base root to patch.

**It worked on the daemon side.** The retained daemon log shows one save,
`service.begin_save` 1.9 / `service.edit` 1.8 / `service.finish` 1.9 ms,
building a content root and a metadata root, and the driver reported
`upstream_calls: 2` where the baseline row reports 4.

**It does not deliver the reply.** The daemon encodes and writes the 90-byte
`Response::Saved` with its metadata root and the client never observes it, so
Commit waits out the five-second silent-wait window and fails with an unknown
outcome. Reproduced at four identities, and independently of whether the
metadata base named the published sibling root or the content root. A response
whose bytes are written but not read is not a performance result and must not
be kept as one, so the change is **withdrawn unshipped**
(`revert(core): withdraw the merged content-and-metadata save`).

What the attempt still establishes, and what the handoff's ~2.6 ms estimate got
wrong: **R1's real size is one crossing plus the second save's `begin_save` and
`finish`** — the metadata objects and their role validation move into the first
save rather than disappearing.

### R2: characterised, not attempted

`serial_original`
(`core/crates/layerfs-workspace/src/filesystem/original.rs:88`) runs the
`Inspect` only when the node's baseline is stale, and it exists to learn the
published content root that `EditFile` must name as its base. A node resolved
from the published base is already canonical and takes the cached branch; this
row's node is refreshed, so the crossing is what the contract costs. Removing it
changes where the published base root comes from, which is a correctness
question the oracle would have to settle first. Recorded as an open lever and
**not** as a win.

### Stop reason

The crossing budget is **not met**. Four of the six levers the handoff named
were addressed (D3, D1, R1, R3) and one (R2) was characterised as structural;
of the three performance levers, none reduced a crossing in this row. R1 — the
only lever that does reduce them — needs the unobserved-response question
answered first, and that is a client-side transport investigation rather than a
product change. The remaining named terms are `EditFile` at 6.29 ms and the
sandbox share of `edit` at 13.23 ms, neither with a measured cause.

### Status

**#232 is not complete and must not be closed.** No row has an eligible cold
Edit+Commit sample, no row is `GOAL_MET`, and this round's row is `TARGET_MISS`
at 5.63 ms. No crossing reduction and no speed improvement is claimed.

### Evidence

`core/docs/issues/232/evidence/roundtrip-1mib-20260924-01/`:

* `NOTE.md` — full report: identity, crossing table, waterfall with unknowns
  marked, lever findings, instrument limits
* `LEDGER.md` — append-only per-iteration ledger with counts, identities,
  arithmetic and every non-passing line
* `analyze_roundtrips.py` — reads retained evidence only and reproduces the
  crossing counts, the `upstream_calls` identity check, the per-op spans and the
  projection bytes and buckets
* `HANDOFF.md` — the handoff prompt, retained because it previously lived only
  under `benchmark-results/`, which this repository excludes from version control
* `final-run.json`, `final-driver-receipt.json`, `final-receipt.json`,
  `final-telemetry.lft1`, `final-daemon.log`, `final-verification.json`,
  `final-verifier.stdout`, `final-driver.stderr`

Reproduce (`--repo` and `--results` point at this worktree):

```text
python3 core/docs/issues/232/evidence/roundtrip-1mib-20260924-01/analyze_roundtrips.py \
  --repo <worktree> --results benchmark-results/fs-bench-pro/sdk-exec-fuse --case 1mib
```

Checks at the final label: `check_product_boundary.py` PASS (290 production
files); `python3 -m unittest discover -s core/tools -p 'test_*.py'` 9 OK;
`cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` clean;
`cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets --
-D warnings` clean; `cargo +1.85.1 test --manifest-path core/Cargo.toml
--locked` 728 passed / 0 failed / 3 ignored; harness suite 37 OK (4 skipped).
No `preflight.sh`, no CI claim.
