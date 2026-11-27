//! `branch.list_commits`.
use crate::families::args::{project, text, Answer};
use crate::{history::listing, StoreService};
use layerfs_protocol::sdk::SdkValue;

/// `list_commits({ branch, at?, after?, limit? })`: one page of the
/// Branch's Commits, newest first; on `main`, its publications. `at` starts
/// the first page at an older Commit (an id or a prefix); a later page names
/// the Commit the first one started at as `at` and its own place as `after`.
pub fn list_commits(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = text(args, "branch")?;
    let from = listing::From {
        at: args.get_text("at"),
        after: args.get_text("after"),
    };
    // the Branch is resolved before the page's limit is read
    listing::commits(&service.store, &stack, branch, from, listing::limit(args))
}
