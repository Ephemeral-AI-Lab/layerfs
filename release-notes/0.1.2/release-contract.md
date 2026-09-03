# LayerFS 0.1.2 release contract

> **Status:** Released source-only Developer Preview contract for LayerFS 0.1.2.

LayerFS 0.1.2 preserves the documented patch-level public API, CLI, daemon,
canonical identity, and Store-format compatibility boundary. The universal
regular-file edit engine is supporting implementation work.

Issue #20's three complete SDK-only families are evaluated under the explicitly
approved latency/parity and ack-window-v1 observation policy in
[benchmark results](benchmark-results.md). The [selector](sdk-edit-evidence.json)
binds final admission to exact evidence and repository gates. Publication
additionally requires parent #12's separate release-finalization decision and
validation of the exact release source. The user has separately authorized
publication through #12 once the missing benchmark refresh, documentation,
verification and release checks are complete. This remains a source-only
Developer Preview, not a crates.io or prebuilt-binary release.
