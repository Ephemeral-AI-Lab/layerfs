//! The `file` family: 14 calls, one file each (O13: was fs).
mod apply;
mod blame;
mod cp;
mod diff;
mod edit;
mod glob;
mod grep;
mod list;
mod mkdir;
mod mv;
mod read;
mod rm;
mod stat;
mod write;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        read::CALL,
        stat::CALL,
        list::CALL,
        glob::CALL,
        grep::CALL,
        blame::CALL,
        diff::CALL,
        write::CALL,
        edit::CALL,
        rm::CALL,
        mkdir::CALL,
        mv::CALL,
        cp::CALL,
        apply::CALL,
    ]
}
