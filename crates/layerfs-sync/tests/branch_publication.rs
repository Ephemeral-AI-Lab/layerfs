mod support;

use support::Scenario;

#[test]
fn branch_publication_preserves_exact_cas_lost_ack_and_finalization() {
    let mut scenario = Scenario::new();
    scenario.qualify_publication();
    scenario.qualify_checkout_and_finalization();
    scenario.cleanup();
}
