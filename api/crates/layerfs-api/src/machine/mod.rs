//! The `machine` family: 14 calls, one file each (O11: update; #535: platforms, start, stop;
//! layerfs-project#164: stats; layerfs-project#178: files).
mod create;
mod files;
mod get;
mod images;
mod list;
mod load;
mod platforms;
mod prune;
mod remove;
mod start;
mod stats;
mod status;
mod stop;
mod update;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        list::CALL,
        platforms::CALL,
        get::CALL,
        images::CALL,
        load::CALL,
        prune::CALL,
        status::CALL,
        stats::CALL,
        files::CALL,
        start::CALL,
        stop::CALL,
        create::CALL,
        remove::CALL,
        update::CALL,
    ]
}
