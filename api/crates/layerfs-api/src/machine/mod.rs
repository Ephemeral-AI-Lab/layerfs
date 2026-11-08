//! The `machine` family: 9 calls, one file each (O11: update; #535: platforms, start, stop).
mod create;
mod get;
mod list;
mod platforms;
mod remove;
mod start;
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
        status::CALL,
        start::CALL,
        stop::CALL,
        create::CALL,
        remove::CALL,
        update::CALL,
    ]
}
