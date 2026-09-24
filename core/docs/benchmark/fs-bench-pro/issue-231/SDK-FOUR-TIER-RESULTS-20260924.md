# #231: four-tier SDK Init diagnostic, 2026-09-24

> **Status: FAIL for #231 completion; no merge or issue closure.** This is
> the explicitly selected SDK route in the prospective
> [diagnostic contract](SDK-FOUR-TIER-DIAGNOSTIC-20260924.md). The 100- and
> 1,000-file rows passed functional proof but remain performance-ineligible;
> the 10,000-file verifier timed out, and the 100,000-file command timed out.
> These are not the historical daemon-host #231 route or cold Init evidence.

## Identity and method

The exact measured source commit was `6a94a51240e248abc0ed1e54640debfd04d303fd`
(tree `f1cd2866489d1250e4a1facc9ba5896df743130f`). Product seal:
`78430b1b2cbcbda185e5995393cd519dd881b3a1e94cb93dd60c217b31dc6529`;
harness seal: `aa09d4cd4c83755a30e0343f074f8bfe9fac8080eef805ad2aa98288a23e5d1e`;
Cargo lock SHA-256: `72919f072f26a55ed8e2d3284536fe11dd0e5149fd2f9058ed96a36df2ad1a06`.
The locked **debug** SDK driver/verifier SHA-256 values were respectively
`ea105d9cd455dd4eab219025371d45a73cfe7af7cb1a23e767a607d54dc05e3e`
and `53433e212a52e7d6bff2de08020a0fb04d8c07538a0cd04fe8b1e26a4eb891f9`.
The first build took 7.604530292 s, below its 30-second limit; the later
cases reused those exact binaries. Each case launched one SDK driver with a
fresh Store/History and seed 1; the 100k public result was unconfirmed.
Prepared 100k
fixture reuse was outside the timer; the other three fixtures were newly
prepared. No operation was resampled.

All rows declare `source-cache-uncontrolled-v1`. No source-page
invalidation and whole-input residency proof was run, and there is no
frozen numeric SDK latency target. All four are
`admission_eligible=false`; raw SDK times cannot be promoted to performance
PASS or compared with release or daemon-host rows. The fixed complete-command
limit was 15 s and the separate full-verifier limit was 5 s.

## One-shot results

| Case | Raw public SDK call | Complete command | Independent full verifier | Cleanup | Result |
| --- | ---: | ---: | --- | --- | --- |
| 100 files / 5 MB | 0.150031000 s | 0.737912917 s, PASS | 0.668470208 s, PASS; 102 paths / 5 MB | PASS | Functional PASS; performance INELIGIBLE |
| 1,000 files / 20 MB | 0.414789917 s | 0.430163042 s, PASS | 0.642346792 s, PASS; 1,011 paths / 20 MB | PASS | Functional PASS; performance INELIGIBLE |
| 10,000 files / 300 MB | 6.211608666 s | 6.225564333 s, PASS | 5.004904125 s, TIMEOUT; no full oracle result | PASS | FAIL |
| 100,000 files / 500 MB | No confirmed SDK result | 15.006412917 s, COMMAND_SLOW / timeout | NOT_RUN: no confirmed root | UNKNOWN after timeout | FAIL |

The 10,000-file driver returned a typed result and clean exit, but the
independent verifier was killed at the unchanged five-second bound. Its
partial progress is retained; it does **not** prove a complete readback.
The 100,000-file child was killed at the unchanged 15-second command bound.
It produced no confirmed `operation_ns` or root, so its receipt has
`sample_count=0` despite the attempted call. Its partially written Store was
515,268,608 B apparent, and an empty ordering scratch directory remained;
the runner correctly reports cleanup `UNKNOWN`. Neither case was retried.

The raw result directories are under
`benchmark-results/fs-bench-pro/issue231-sdk-four-tier-6a94a5124-{100,1000,10000,100000}/`
in this worktree. Their `manifest.json` files hash the retained output,
including raw Store data. Read-only `runner.py verify --run` returned PASS
for **receipt integrity** in each directory; it does not override the failed
product or verifier statuses. Small receipts, stdout/stderr, manifests and
[SHA-256 inventory](evidence/sdk-four-tier-20260924/SHA256SUMS.json) are
copied to the [curated evidence](evidence/sdk-four-tier-20260924/).
The four structured-text variants were not selected.

## Decision and checks

Do not merge the treatment PR to `main` or close #231. The issue requires
four eligible performance PASS rows with full independent verification,
cleanup and custody. This SDK diagnostic has no admission-eligible row;
two larger tiers also failed functional/budget gates. A new prospectively
frozen cache/numeric contract and a complete passing campaign would be
required for closure. Do not re-label this cohort later.

The benchmark selector's focused seven Python tests and `git diff --check`
passed before collection. The unchanged Core product source had passed its
locked test, examples, warning-denying Clippy, format and boundary checks in
the preceding #237 treatment verification; those commands were not repeated
for this harness-only selector edit. No CI or aggregate preflight was run.
