//! The `machine` family: 13 calls, one file each (call surface 2.0;
//! `machine.status` is `machine get`, `machine.space` is `machine stats
//! --with space`, `machine.platforms` is `home get --with platforms`,
//! `logs.machine` is `machine log`). `stats` also holds the small helpers
//! that rename what a library function answered to the row's keys.
mod create;
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
        crate::doctor::machine::CALL,
    ]
}
