//! `fs.grep`.
use crate::methods::args::{invalid, optional_path, paging, snapshot_full, text, Answer};
use crate::{
    read::{paged_reads, query, reads, search, snapshot, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `grep(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: literal matches per line in text files.
pub fn grep(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap) = snapshot_full(service, args)?;
    let pattern = text(args, "pattern")?;
    if pattern.is_empty() {
        return Err(invalid("pattern is empty"));
    }
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap)?;
    if snapshot::entry(&reader, &snap.tree, &path)?.is_some() {
        if let Some(refusal) = visibility::refuse(&path, &seen) {
            return Err(refusal);
        }
    }
    let filters = search::Filters::of(args)?;
    if args.get("page").is_some() {
        if filters.any() {
            return Err(crate::methods::args::invalid(
                "page cannot be combined with include, exclude or ignoreCase",
            ));
        }
        return paged_reads::pin(
            &service.store,
            &snap,
            paged_reads::grep(
                &service.store,
                &reader,
                &snap,
                pattern,
                &path,
                &seen,
                query::result_page(args)?,
            ),
        );
    }
    let (limit, pos) = paging(args)?;
    paged_reads::pin(
        &service.store,
        &snap,
        reads::grep(&reader, &snap, pattern, &path, &seen, &filters, limit, pos),
    )
}
