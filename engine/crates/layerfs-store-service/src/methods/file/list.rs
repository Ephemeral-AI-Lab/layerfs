//! `fs.list`.
use crate::methods::args::{optional_path, paging, snapshot_full, Answer};
use crate::{
    read::{paged_reads, query, reads, snapshot, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `list(path?, { branch, commit?, pageSize?, pagePos? })`: one window of a directory.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap, saved) = snapshot_full(service, args)?;
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    if snapshot::entry(&reader, &snap.tree, &path)?.is_some() {
        if let Some(refusal) = visibility::refuse(&path, &seen) {
            return Err(refusal);
        }
    }
    let removed = seen.hidden_children(&reader, &snap, &path)?;
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
        reads::list(&reader, &snap, &path, &seen, paging(args))
    }
    .map(|value| visibility::settle(value, &seen, removed));
    paged_reads::pin(&service.store, &snap, &saved.record, answer)
}
