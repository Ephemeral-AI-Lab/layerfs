//! The process-wide cancel request of `store upgrade` (#804 §6) is dropped
//! once the call answers: a long-lived process (an SDK host) that took one
//! Ctrl-C is not left cancelling every later upgrade, while a request made
//! before a call still cancels that call. Its own test binary: the flag is
//! process-wide, so no other upgrade runs beside these.
#[path = "../support/root.rs"]
mod root;
#[path = "../support/worker_store.rs"]
mod worker_store;

use layerfs_home::names::StoreName;
use layerfs_home::reply::events::Events;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use worker_store::placed;

fn upgrade(home: &std::path::Path) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_| {};
    let name = StoreName::parse("shop").unwrap();
    layerfs_store::admin::with_users::upgrade(
        home,
        &mut Events::new(&mut sink),
        &name,
        &layerfs_sandbox::users::STORE,
    )
}

#[test]
fn a_cancel_stops_the_next_upgrade_only() {
    let fixture = placed("cancel-clear", |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve(listener, worker_store::SECRET.into(), root)
        });
    });
    // asked for before the call: that call is cancelled, nothing sent
    layerfs_home::cancel::request();
    let refusal = upgrade(&fixture.home).unwrap_err();
    assert_eq!(refusal.code, "Cancelled", "{refusal:?}");
    assert!(
        !layerfs_home::cancel::requested(),
        "the answered call dropped the request"
    );
    // the next call is not cancelled
    let next = upgrade(&fixture.home);
    assert!(
        !matches!(&next, Err(refusal) if refusal.code == "Cancelled"),
        "{next:?}"
    );
    assert!(!layerfs_home::cancel::requested());
    fixture.remove();
}
