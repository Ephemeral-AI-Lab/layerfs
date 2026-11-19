//! The `machine` family: 13 calls, one file each (call surface 2.0;
//! `machine.status` is `machine get`, `machine.space` is `machine stats
//! --with space`, `machine.platforms` is `home get --with platforms`,
//! `logs.machine` is `machine log`). `stats` also holds the small helpers
//! that rename what a library function answered to the row's keys.
mod create;
mod doctor;
mod files;
mod get;
mod images;
mod list;
mod load_image;
mod log;
mod prune;
mod remove;
mod start;
mod stats;
mod stop;
mod update;

use crate::calls::Calls;

/// A machine call in the guest worker's build (no `machine` feature): no
/// machine adapter runs inside a machine, so it is refused before any
/// effect.
#[cfg(not(feature = "machine"))]
fn host_only(
    _context: &mut crate::calls::Context<'_>,
    _args: &layerfs_bridge::control::SdkValue,
) -> layerfs_home::outcome::Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "the machine calls run on the host",
    ))
}

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        get::CALL,
        images::CALL,
        load_image::CALL,
        prune::CALL,
        stats::CALL,
        files::CALL,
        log::CALL,
        start::CALL,
        stop::CALL,
        create::CALL,
        remove::CALL,
        update::CALL,
        doctor::CALL,
    ]
}
