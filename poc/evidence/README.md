# Stage One portable evidence

This directory makes the compact controlling evidence inspectable from a fresh
repository checkout instead of depending only on ignored `target/` paths.

```text
stage1-pre-repair-campaign-20260824/
  immutable portable copy of the 61-row A01–A17 REVISE campaign
a02-diagnostic-20260824/
  immutable portable copy of the preregistered 300-offset attribution
stage1-post-repair-closure/
  post-adversarial source manifest, preserved full-workspace foundation,
  exact-source touched closure, release and zero-row-readiness custody; no
  measured campaign rows
stage1.1-apple-edge-20260825/
  compact terminal receipt for the independently audited 47-row/51-edit
  attempt-014 PASS; includes exact source, executable, fixture, raw-artifact
  hashes, closure commands, decisive counters, performance and resource gates
stage1.1m-current-source-closure-20260825/
  compact Verified M0-M15 terminal receipt, closure failures and repairs,
  frozen 0/24/96 MiB result, owner-audit disposition, and independently audited
  current-source attempt-015 regression receipt
stage1.1t-trusted-20260826/
  preserved attempt-001 result plus append-only supersession: materialize-only
  Trusted open did not yet mark trusted_history
stage1.1t-trusted-20260826-attempt-002/
  preserved second TrustedLocalDev result; product-operation diagnostics remain
  useful, but its v1 row-timer and build-custody claims are superseded
stage1.1t-trusted-20260826-attempt-003/
  final audited v2 TrustedLocalDev 0/24/96 MiB population, source/build custody,
  attempt-002 supersession, independent raw recomputation, and current-source
  Stage 1.1 attempt-020 regression receipt
```

The performance campaign and A02 diagnostic predate the adversarial correctness
repairs. Their source bindings and `REVISE`/accepted-exception status remain
unchanged. The post-repair closure never promotes those measurements to the new
source. Stage 1.1 attempt 014 is the distinct measured campaign for exact
source commit `f3dd4a32273a4c5cbe5e7ca2287c945ba4434c30`.
Stage 1.1M performance remains bound to clean operand `9800f865`; its separate
current-source correctness closure is reclosed at clean commit `36d05d8`.
The Verified performance population was not rerun and remains
`REVISE_NO_AUTHORIZED_OWNER`; the Trusted attempt-003 PASS is a separately
labeled weaker-trust class.
