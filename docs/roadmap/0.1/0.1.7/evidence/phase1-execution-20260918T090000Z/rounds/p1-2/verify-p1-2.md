# verify-p1-2 — pooled per-operation read session

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named tree).
> Round: [`receipt.md`](receipt.md). Tree: `d2c6dfb7f`, arm [`after/`](after/);
> before arm [`../v3/after/`](../v3/after/) (tree `aefcd95a5`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive d2c6dfb7f \| tar -x -C /tmp/verify-p12/tree` | 0 | clean P1-2 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | vehicles built from the archive |
| R3 | `measure_edits --mode pipeline --case chunked --threshold-bytes 131072 --output <fresh>` | 0 | `readback connection opens: 1`, root `ca6c30a4355c63c3…`, 262,144 bytes verified — identical to `after/logs/D21` |
| R4 | the same for `small-to-large`, `large-to-small`, `batch` | 0 ×3 | `opens: 1` each — identical to D22/D23/D24 |
| R5 | `cargo +1.85.1 test --offline --locked -p layerfs-storage --test cas_roundtrip` | 0 | 12 passed (including both new tests) |
| R6 | the two new tests against the **parent** tree's product source | 101 | the session does not exist there (see 2a) |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?**
`a_pooled_session_opens_one_connection_for_every_wave` cannot pass there: on
`1109fa853` every wave opens its own connection, so the per-wave sequence is
`[1, 1, 1]` and `connection_opens()` is 3. Reproduced by exporting the parent tree,
copying only the test file into it and running the test: it fails at the
`opens` sequence assertion. (The second new test passes on the parent — a per-wave
connection re-reads the ceiling too; it is a regression guard for the new design and
the receipt says so rather than counting it as a falsification.)

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted `O(waves) → O(1)`: measured **3 → 1** on all four
discriminating rows (D21–D24) and 1 → 1 on the single-wave rows, which cannot
discriminate. Nothing else moved: a diff of all 29 D-rows between the arms (timing
figures and paths removed) is 16 lines, all of them those four readings. A movement
anywhere else would have been the finding.

**2c — parity green and unchanged?** The test diff in this commit is
`core/crates/layerfs-storage/tests/cas_roundtrip.rs` only — not one of the seven
sealed-oracle targets. Those are untouched and were re-run green on the C1 tree
([`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1). V3's bound
test was **not** edited by this commit, which is the point of having written it as a
bound.

**2d — single-variable?** Product files: `cas/read.rs` (the session) and
`cas/provider.rs` (the provider's session slot and per-wave `opens`). Plus the two
new tests and the same-commit `05-storage.md` §6.11 update. Nothing else.

**2e — error paths.** (i) *Absence*: `provider_errors::an_object_the_store_does_not_hold_is_absence`
and `::a_corrupt_pack_is_not_reported_as_absence` are green in the workspace run —
the pooled connection does not turn absence or corruption into anything else,
because the error mapping is unchanged. (ii) *The refusal*: a demand over
`read_objects` is still refused **before** the session is opened
(`check_read_demand` runs in `Store::read_batch`... **no** — in the provider path the
session is opened first, then `read_objects` is reached through
`ReadSession::read`. The ceiling refusal therefore happens *after* an open on the
provider path, and the per-wave counter reports `opens: 1` for that wave. The
wave's demand bound is still enforced (the `READ_OBJECT_LIMIT` check inside
`read_objects`), but the "refused before a connection is opened" property that
`Store::read_batch` has is **not** preserved on the provider path. Recorded as a
finding on #178 rather than silently fixed: it is a second variable, and no
frozen row exercises it. (iii) *The empty case*: a read of zero ids still opens
the session (there is no cheaper honest answer) and returns an empty value list.

**2f — elapsed as a gate?** No: the gate is `opens`, a count. Every timing figure in
the round is diagnostic, and `X5` shows the counter reproduces while the clock does
not.

## 3. UNVERIFIED

* **The 1 MiB decode arena is held for the operation's lifetime, not measured.**
  The plan's claim "workspace 1 MiB × w transient → 1 MiB × 1 held" is a
  consequence of the ownership change and is visible in the code, but no vehicle
  prints it; no counter here proves it.
* **`contains` is not pooled.** It opens its own connection per call
  (`cas/store.rs`) and is outside this item's scope; a caller that mixes `contains`
  with waves still pays an open per `contains`.
* **The provider-path demand refusal now happens after the open** (see 2e-ii).
  Reported, not fixed.
* **No cross-process claim**: everything here is one process, one sample.
