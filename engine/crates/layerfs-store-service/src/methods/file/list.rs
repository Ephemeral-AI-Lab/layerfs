//! `fs.list`.
use crate::methods::args::{optional_path, paging, snapshot_full, Answer};
use crate::read::cached::Cached;
use crate::{
    read::{paged_reads, query, reads, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;
use layerfs_content::compare::memo::Memo;

/// `list(path?, { branch, commit?, pageSize?, pagePos? })`: one window of a directory.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap, saved) = snapshot_full(service, args)?;
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    // One path walk for the call (#818 R3), through its caches and memo.
    let cached = Cached::new(&reader)?;
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
    paged_reads::pin(&service.store, &snap, &saved.record, answer)
}
