# Canonical-v2 compact closure repair v2

The first compact attempt at
`target/phase4-canonical-v2-closure-20260821-v2/compact-results-v1` is preserved
as `CANONICAL-V2 REVISE`: `cargo check` stopped before tests, build, preparation,
or rows because an `Option::map` closure mixed `AnyResult` with `CoreError`.
Its terminal manifest SHA-256 is
`941e9d04dc07fc5552c008678be288fa03b85413fc9c6fda61ad27d5db05084f`.

Before this fresh attempt, the closure was replaced by ordinary conditional
control flow so both `?` operators propagate through the enclosing `AnyResult`.
No codec, identity, transaction, timer, correctness, schedule, performance, or
decision gate changed. The controlling prospective contract remains
`PROSPECTIVE-COMPACT-CLOSURE-v1.md` byte-for-byte. This attempt uses only the
fresh namespace
`target/phase4-canonical-v2-closure-20260821-v3/compact-results-v1` and rehashes
the complete failed-attempt manifest before validation.

The one global 119-second clock, exactly three focused tests, one release build,
29-row compact schedule, candidate-only labels, both-primary-pairs-win rule,
hard semantic gates, terminal manifest, and scope stop are unchanged.
