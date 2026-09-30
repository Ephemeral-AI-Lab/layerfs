# Phase B owner acceptance and main integration — 2026-09-30

> **Status: Dated planning checkpoint; not release evidence or a product contract.**

The owner accepts the completed Phase B scope and authorizes integration into
remote `main` and closure of its completed tickets: “with phase b finished, we
can merge into the remote main and close tickets”. This later direction replaces
the earlier draft/unmerged/open-ticket hold in the handoff and checkpoints. It
does not change any historical receipt or confer release admission.

The [seven-family checkpoint](SEVEN-FAMILY-CHECKPOINT-20260930.md),
[exact proof reuse map](experiments/20260930-seven-family-reuse.json),
[Family 7 checkpoint](FAMILY7-CHECKPOINT-20260930.md) and
[append-only experiment log](EXPERIMENT-LOG.md) define the accepted scope. All
seven families have their stated functional, command-budget, independent-oracle
and cleanup/custody coverage. History storage passes the prospectively recorded
owner profile allowing up to 10% allocation deviation; strict historical misses
remain misses. Init and Workspace numeric latency qualification remains
INELIGIBLE because of the declared cache limitations. Those observations are
retained with their original source identities and are not release speed claims.

## Source and integration method

- Reviewed Phase B handoff: `2135e2dc186c0bad7fdd1b7f42ca8e5bcc26cc27`,
  tree `284faeb731589310316e9aaf7436f108d97c3e55`.
- Final product change: `a54cda244f4ad4f99b87a852c6dbe0395633c3d3`.
- Remote main before integration: `f74dbe77da12fa533587be8a578375bce3f19373`.
  It is an ancestor of the reviewed handoff: 0 main-only and 148 handoff-only
  commits before this documentation publication.
- PR [#285](https://github.com/Ephemeral-AI-Lab/layerfs/pull/285) is advanced by
  fast-forward from its Phase A head and retargeted to `main`. Main integration
  uses a non-forced fast-forward, retaining every source/evidence commit and
  rejecting a concurrent incompatible main update. This record adds only docs;
  the checked product tree is unchanged. Actual remote/PR results are reported
  in the closing issue comments after integration.

## Ticket disposition

| Ticket | Accepted disposition and evidence |
| --- | --- |
| [#284](https://github.com/Ephemeral-AI-Lab/layerfs/issues/284) | Close the active-backing/component-namespace integration. [Phase A report](../284/PHASE-A-IMPLEMENTATION-REPORT.md) and all seven Phase B families cover the implemented scope. |
| [#286](https://github.com/Ephemeral-AI-Lab/layerfs/issues/286) | Close the owner-accepted seven-family iteration. Preserve all cache-INELIGIBLE, failed, diagnostic, reused and deferred receipts and the up-to-10% storage profile. |
| [#258](https://github.com/Ephemeral-AI-Lab/layerfs/issues/258) | Close inherited-directory moves/replacements, stable held identities, nested/deep traversal and atomic refusal. Phase A and the [Family 5 checkpoint](FAMILY5-CHECKPOINT-20260930.md) carry the independent old/new namespace/byte proofs and recorded 3/67 descendant controls. |
| [#264](https://github.com/Ephemeral-AI-Lab/layerfs/issues/264) | Close component-only mounted namespace and charged complete ancestry. Mounted moves avoid descendant copy-up/path rewriting. Phase A and Family 5 cover deep paths, handles, G1/G2 and resource refusal. Existing C1 full-base alias/cycle scans retain their explicit separate scope. |

Open work stays open: [#276](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276)
owns the deferred 10240 frontier, 270-component Commit/ReserveInodes cost and
dirty-discard/close policy; [#256](https://github.com/Ephemeral-AI-Lab/layerfs/issues/256)
owns general namespace/handle/encoding/streaming scale beyond the finite 1025
case; [#283](https://github.com/Ephemeral-AI-Lab/layerfs/issues/283) owns the host
memory capability. Broader numeric/cache qualification and the distinct #261
performance gate remain open; this acceptance does not certify them. Other
independent tickets and uncontained donor/optimization PRs are not dispositioned
by this record.

The historical exit-7/unmount-discard case remains FAIL with its
[owner deferral](https://github.com/Ephemeral-AI-Lab/layerfs/issues/276#issuecomment-5903530127).
The separate explicit recovery profile passes. The original failed-command
Family 7 profile remains OWNER-DEFERRED/NOT_RUN. No discard API was invented and
no cleanup guard was weakened to close these tickets.

## Validation and source size

The [final owning checks](experiments/20260930-family7-checks.json) are reused for
the unchanged product: 4 authenticated Commit transport tests, 26 applicable
host owning tests, host workspace/all-target Clippy, Linux release SDK/native
test Clippy, fmt, 357-file product boundary, 9 boundary self-tests and 6 package
Python guards passed. Darwin excludes Linux-only test bodies; the actual Linux
native/SDK proofs cover the changed behavior. Earlier unchanged component and
family proofs retain their explicit reuse scopes. No earlier benchmark group,
including Family 2, is rerun for this documentation-only integration. Local
links and the documentation commit's whitespace check pass. The full main diff
reports four trailing-blank-line warnings in unchanged archived raw outputs
introduced by `5a9a9fc5f`; those evidence bytes are preserved:

- `core/docs/issues/245/evidence/post-phase1-extent-v1/pieces-sequence.stdout:37`
- `core/docs/issues/245/evidence/post-phase1-ownership-v1/after-count-trace.txt:25`
- `core/docs/issues/245/evidence/post-phase1-ownership-v1/before-count-trace.txt:22`
- `core/docs/issues/245/evidence/post-phase1-ownership-v1/payload-route/test.stdout:24`

`git diff --check` over the integration outside these exact four raw outputs
passes. The full command is reported as a known evidence-file whitespace miss,
not an unrestricted PASS. No product fix or receipt rewrite is needed. No CI or
retired aggregate preflight runs; no release tag is created.

Production LOC uses `tools/production_loc.py --json --root <snapshot>` from blob
`c7dd2b9c6aa9db63393a4ff3ebca9327529d146e`, with identical production-only scope,
inline-test handling and exclusions on archived Git snapshots. Tests, examples,
benchmarks, tools and docs do not enter these totals.

| Comparison | Reference | Core | Combined | Signed combined delta |
| --- | ---: | ---: | ---: | ---: |
| Main before integration | 65,417 | 56,168 | 121,585 | — |
| Reviewed integrated product | 65,417 | 70,279 | 135,696 | +14,111 versus main |
| Phase B baseline | 65,417 | 70,022 | 135,439 | — |
| Phase B final | 65,417 | 70,279 | 135,696 | +257 versus Phase B baseline |

This documentation publication has production delta +0. Every retained commit
carries its own exact first-parent comparison. The main delta includes the
earlier implementation stack as well as Phase B; reference code is unchanged
and remains separately counted. No migration scope was removed to reduce LOC.
