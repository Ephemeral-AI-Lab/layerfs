mod support;

use support::Scenario;

#[test]
fn child_merge_and_rollback_publish_as_ordered_branch_history() {
    let mut scenario = Scenario::new();
    scenario.qualify_publication();
    scenario.qualify_checkout_and_finalization();
    scenario.qualify_child_merge_and_rollback();
    scenario.cleanup();
}
