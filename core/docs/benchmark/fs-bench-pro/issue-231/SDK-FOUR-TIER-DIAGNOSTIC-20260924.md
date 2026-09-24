# #231: four-tier SDK Init diagnostic selection

> Frozen before extending the selector or collecting this cohort. This is an
> explicit follow-up to the two-case [#236 v2 SDK contract](../issue-236-sdk-init/SPEC.md),
> requested for #231 on 2026-09-24. It does not replace the historical
> daemon-host rows or establish #231 performance admission.

Run the exact seed-1 `core-sdk-init-fixture-v2` shapes for
`namespace-100-compact-v3` (100 files/5 MB),
`namespace-1000-compact-v3` (1,000/20 MB), `namespace-10000`
(10,000/300 MB), and `namespace-100000` (100,000/500 MB). The repeated
“10,000” in the request is interpreted as the issue's fourth mandatory
100,000-file case. The four structured-text variants remain optional and
unrun. Use one fresh result directory and one sample for each explicit case,
in this order; retain failures and do not repeat an unchanged arm.

The existing `runner.py` and `families/init_namespace.py` own all four calls.
Keep the default `--family init_namespace` selection at 100 and 1,000 files.
Permit explicit `--case` selection of 10,000 and 100,000 as unregistered
diagnostics, with their own receipt label; do not promote either historical
`NOT_RUN` receipt. Build the SDK driver and independent verifier with locked
Cargo's **default debug** profile in this worktree. The driver makes one
public `Client::init_project` call. Its timer encloses source scan/read,
construction, Saves and C5 publication. The verifier reopens Store/history
after the timed child and checks every path, mode, mtime, size and SHA-256
against the sealed manifest. Prepared masters may be reused outside the
timer; every case uses a fresh Store and History.

Keep the existing 15-second complete-command limit, 5-second verifier
limit and 30-second build limit. No timeout, worker count, fixture or
cache-policy relaxation is authorized. The declared source cache for every
row is `source-cache-uncontrolled-v1`: this selector does not invalidate
and prove whole-input source residency. It has no frozen numeric SDK
latency target. Thus even a functionally successful row is
`admission_eligible=false`, `performance_gate=NOT_FROZEN` and
`performance_status=INELIGIBLE`. Report raw operation and complete-command
times only as diagnostics, with verifier, cleanup, Store geometry, source,
product, harness, binary and fixture identities. Do not pool these SDK
observations with #231's daemon-host route or the old release diagnostic.

The issue's original completion gate remains all four **eligible
performance PASS**, independent verification PASS, cleanup PASS and exact
custody. This cohort cannot satisfy that gate by construction. Therefore
its results may inform the next qualifying contract, but cannot alone
authorize merging the treatment PR or closing #231. A future admission
campaign needs a prospectively committed cache and numeric contract and new
samples; no receipt from this cohort may be relabeled later.
