//! Independent writer-reopen test: an orphaned mount row whose release
//! statement never ran.
//!
//! A mount whose reply is lost after its intent's `COMMIT` leaves a row no
//! caller holds; the writer records its op and the reopen deletes the row
//! (`session/reopen.rs` step 4). On a fresh connection the delete statement
//! is not prepared yet, so step 4 first prepares it in a round trip of its
//! own, then executes it. Here the connection is lost at the prepare's
//! first reply: the delete itself was never sent. The orphan must still be
//! deleted by a later reopen ("orphaned rows go"); otherwise its live row
//! refuses `branch.remove` and compaction with no Workspace mounted at all,
//! until the daemon exits.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::fault_proxy;
use crate::testerwr;

use fault_proxy::Fault;
use testerwr::*;

#[test]
fn an_orphan_whose_release_was_never_executed_is_still_released() {
    let Some(rig) = rig("wr_orphan_prepare") else {
        return;
    };
    let twin = rig.store.fork("twin");
    let k = mount_flight(&rig, twin);
    // Calibration: an orphan, then a mount that reopens. No Workspace is
    // mounted, so the reopen takes no keys (step 3 sends nothing) and the
    // first prepare after the startup is step 4's delete statement.
    let calibration = rig.store.fork("o_cal");
    lose_mount(&rig, twin, calibration, k);
    let probe = rig.store.fork("probe");
    rig.proxy.arm(None);
    let mounted = rig.service.bind(probe);
    let window = rig.proxy.disarm();
    assert_eq!(
        rows(&rig.store, calibration),
        (0, 0),
        "calibration orphan gone"
    );
    rig.service.unmount(mounted);
    let kinds: Vec<char> = window.log.iter().map(|(_, kind)| *kind).collect();
    let startup = kinds
        .iter()
        .position(|kind| *kind == 'K')
        .expect("a startup")
        + 1;
    let prepare = startup
        + kinds[startup..]
            .iter()
            .position(|kind| *kind == '1')
            .unwrap();
    // The prepare's reply (1 t T Z) and then the delete's execution (2 D C Z).
    assert_eq!(
        kinds[prepare..prepare + 8].iter().collect::<String>(),
        "1tTZ2DCZ",
        "the flight: {kinds:?}"
    );

    // The armed run: the same shape, the connection lost at the prepare's
    // first reply. Nothing of the delete has been sent.
    let orphan = rig.store.fork("orphan");
    lose_mount(&rig, twin, orphan, k);
    assert_eq!(
        rows(&rig.store, orphan),
        (1, 0),
        "the orphan's row committed"
    );
    rig.proxy.arm(Some((prepare as u64, Fault::Drop)));
    let failed = rig.service.try_bind(probe);
    let window = rig.proxy.disarm();
    assert_eq!(window.fired.map(|fired| fired.kind), Some('1'));
    assert!(failed.is_err(), "the reopen failed: {:?}", failed.is_ok());
    assert_eq!(reopens(&rig.service), 1, "only the calibration reopened");
    assert_eq!(rows(&rig.store, orphan), (1, 0), "the delete never ran");

    // The next call reopens cleanly.
    let mounted = rig.service.bind(probe);
    assert_eq!(reopens(&rig.service), 2, "the next call reopened");
    rig.service.unmount(mounted);
    assert_eq!(key_holders(&rig), vec![], "nothing is mounted");
    let left = rows(&rig.store, orphan);
    let removal = remove(&rig.store, "orphan");
    let compaction = compact(&rig.store);
    println!("TESTERWR_ORPHAN row={left:?} remove={removal} compact={compaction}");
    assert_eq!(
        left,
        (0, 0),
        "the orphaned row outlived the reopen (remove: {removal}, compaction: {compaction})"
    );
    rig.service.stop();
}
