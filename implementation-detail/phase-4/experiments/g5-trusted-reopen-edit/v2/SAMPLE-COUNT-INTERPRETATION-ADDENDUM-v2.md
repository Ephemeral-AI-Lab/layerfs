# G5-1 v2 sample-count interpretation addendum

This append-only note corrects one sentence in `REVIEW-SYNTHESIS-v2.md`
without changing `PREREGISTRATION-v2.md` or its schedule.

The handoff requires at least 20 matched observations for the primary class and
five adjacent pairs for secondary shapes while also requiring two distinct
comparisons. It does not unambiguously say that the full minimum repeats
separately inside each comparison. V2 nevertheless prospectively froze the
stricter interpretation: 20 primary pairs plus five pairs for each of six
secondary shapes in both comparisons, for 200 arm observations total.

That is a deliberate v2 method choice, not an exact restatement of an
unambiguous user minimum. It will not be reduced after observation. Before any
screen, the zero-row dry-run must produce a conservative complete-wrapper
forecast covering all 200 arms, operand preparation/copying, semantic cases,
analyzers, cleanup, manifests, and terminal verification, and must prove the
forecast is `<=120 s`. If it cannot, v2 is preserved as `PREMEASUREMENT_REVISE`;
only a prospectively created v2 may adopt a narrower interpretation.
