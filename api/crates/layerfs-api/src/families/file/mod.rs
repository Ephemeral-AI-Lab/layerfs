//! The `file` family: 14 calls, one file each (O13: was fs). Reads answer
//! content at a Branch's head or at `--at`; each write is one Commit.
mod apply;
mod blame;
mod cp;
mod edit;
mod glob;
mod grep;
mod history;
mod list;
mod mkdir;
mod mv;
mod read;
mod rm;
mod stat;
mod write;

use crate::dispatch::Calls;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::file::Guard;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[
        read::CALL,
        stat::CALL,
        list::CALL,
        glob::CALL,
        grep::CALL,
        history::CALL,
        blame::CALL,
        write::CALL,
        edit::CALL,
        rm::CALL,
        mkdir::CALL,
        mv::CALL,
        cp::CALL,
        apply::CALL,
    ]
}

/// The text argument `name`, when given.
fn text(args: &SdkValue, name: &str) -> Option<String> {
    args.get_text(name).map(str::to_owned)
}

/// A write's `--expected` and `-m`.
fn guard(args: &SdkValue) -> Guard {
    Guard {
        expected: text(args, "expected"),
        message: text(args, "message"),
    }
}
