# #241 post-fix 10 MiB position diagnostic v1

> **Status:** Dated planning checkpoint; not release evidence or a product
> contract. Frozen before the first attempt on 2026-09-25.

One diagnostic-only selection covers all 66 existing frozen 10 MiB
positions, from a fresh independent writable byte copy of the previously
qualified v3 master. The product source includes the composed host Service
closed-stdin polling fix and daemon LFT1 connect/Hello/call child spans; the
immutable Linux image is rebuilt from the changed daemon. The manifest,
baseline Exec command, range EDIT route, case order, stop-on-first-failure
rule, 30-second Exec deadline, five-second native progress rule, one
construction worker, and cache policy are unchanged. Enable the existing
diagnostic telemetry run and retain first and six-second-later Docker
snapshots on any failure known before Sandbox deletion.

Freeze source commit/tree, host test binary, image, fixture and master
receipt identities, telemetry run ID and a fresh output path before the
attempt. Record every PASS, FAIL and NOT_RUN row. There is no retry or
replacement of any earlier result. Cache is uncontrolled and
`admission_eligible=false`; no elapsed value is a performance sample.
A successful selection would be evidence that the new source completed this
diagnostic once, not proof that the host spin caused either historical
`Unknown` or that the full 264-position Phase 3 gate is complete.
