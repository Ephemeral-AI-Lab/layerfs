//! `fs.list`.
use crate::methods::args::{optional_path, paging, snapshot_full, Answer};
use crate::{
    read::{paged_reads, query, reads, snapshot, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `list(path?, { branch, commit?, pageSize?, pagePos? })`: one window of a directory.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap) = snapshot_full(service, args)?;
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap);
    if snapshot::entry(&reader, &snap.tree, &path)?.is_some() {
        if let Some(refusal) = visibility::refuse(&path, &seen) {
            return Err(refusal);
        }
    }
    let answer = if args.get("page").is_some() {
        paged_reads::list(
            &service.store,
            &reader,
            &snap,
            &path,
            query::result_page(args)?,
        )
    } else {
        reads::list(&reader, &snap, &path, paging(args))
    }
    .map(|value| visibility::filter(value, &seen));
    paged_reads::pin(&service.store, &snap, answer)
}
