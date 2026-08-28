mod support;

use support::Scenario;

#[test]
fn published_history_reconstructs_backups_and_compacted_reopens() {
    let mut scenario = Scenario::new();
    scenario.qualify_publication();
    scenario.qualify_checkout_and_finalization();
    scenario.qualify_child_merge_and_rollback();
    scenario.qualify_recovery();
    scenario.cleanup();
}
