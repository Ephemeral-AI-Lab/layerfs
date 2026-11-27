//! `fs.list`.
use crate::methods::args::{optional_path, paging, snapshot_in, snapshot_target, Answer};
use crate::read::cached::Cached;
use crate::{
    read::{paged_reads, query, reads, visibility},
    StoreService,
};
use layerfs_content::compare::memo::Memo;
use layerfs_protocol::sdk::SdkValue;

/// `list(path?, { branch, commit?, pageSize?, pagePos? })`: one window of a directory.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, name) = snapshot_target(service, args)?;
    // The call's caches, from the root's open on (#818 R4).
    let cached = Cached::new(&reader)?;
    let (snap, saved) = snapshot_in(service, &stack, &cached, name)?;
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    // One path walk for the call (#818 R3), through its caches and memo.
    let memo = Memo::new(&cached);
    let entry = visibility::checked(&memo, &snap, &path, &seen)?;
    let removed = seen.hidden_children(&memo, &snap, &path)?;
    let answer = if args.get("page").is_some() {
        paged_reads::list(
            &service.store,
            &reader,
            &snap,
            &path,
            &seen,
            query::result_page(args)?,
        )
    } else {
        reads::list(&memo, &snap, &path, entry, &seen, paging(args))
    }
    .map(|value| visibility::settle(value, &seen, removed));
    paged_reads::pin(
        &service.store,
        &snap,
        &saved.record,
        &saved.recorded,
        answer,
    )
}
