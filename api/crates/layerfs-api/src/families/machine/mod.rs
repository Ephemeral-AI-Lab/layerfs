//! The `machine` family: 13 calls, one file each (call surface 2.0;
//! `machine.status` is `machine get`, `machine.space` is `machine stats
//! --with space`, `machine.platforms` is `home get --with platforms`,
//! `logs.machine` is `machine log`). `stats` also holds the small helpers
//! that rename what a library function answered to the row's keys.
pub(crate) mod lifecycle;
mod load_image;
mod prune;
pub(crate) mod status;
mod update;

use crate::dispatch::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        status::list::CALL,
        status::get::CALL,
        status::images::CALL,
        load_image::CALL,
        prune::CALL,
        status::stats::CALL,
        status::files::CALL,
        status::log::CALL,
        lifecycle::start::CALL,
        lifecycle::stop::CALL,
        lifecycle::create::CALL,
        lifecycle::remove::CALL,
        update::CALL,
        status::doctor::CALL,
    ]
}
