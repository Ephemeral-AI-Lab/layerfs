# LayerFS 0.1.2 release contract

> **Status:** Unreleased draft contract; SDK-only evidence is available, publication is not authorized.

LayerFS 0.1.2 preserves the documented patch-level public API, CLI, daemon,
canonical identity, and Store-format compatibility boundary. The universal
regular-file edit engine is supporting implementation work.

Issue #20's three complete SDK-only families are evaluated under the explicitly
approved latency/parity and ack-window-v1 observation policy in
[benchmark results](benchmark-results.md). The [selector](sdk-edit-evidence.json)
binds final admission to exact evidence and repository gates. Publication
additionally requires parent #12's separate release-finalization decision and
validation of the exact release source. No tag, Release, or asset is created
or authorized by issue #20 completion.
