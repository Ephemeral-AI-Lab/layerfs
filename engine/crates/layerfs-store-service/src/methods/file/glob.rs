//! `fs.glob`.
use crate::methods::args::{optional_path, paging, snapshot_full, text, Answer};
use crate::{
    read::{paged_reads, query, reads, search, snapshot, visibility},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `glob(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: paths matching a `/.lww`-syntax pattern.
pub fn glob(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, snap, saved) = snapshot_full(service, args)?;
    let pattern = text(args, "pattern")?;
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    if snapshot::entry(&reader, &snap.tree, &path)?.is_some() {
        if let Some(refusal) = visibility::refuse(&path, &seen) {
            return Err(refusal);
        }
    }
    // The page-independent `removed_count`: the hidden entries this pattern
    // would return anywhere under `path`, not only direct children.
    let removed = seen.hidden_matches(&reader, &snap, &path, pattern)?;
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
            &saved.record,
            paged_reads::glob(
                &service.store,
                &reader,
                &snap,
                pattern,
                &path,
                &seen,
                query::result_page(args)?,
            )
            .map(|value| visibility::settle(value, &seen, removed)),
        );
    }
    let (limit, pos) = paging(args)?;
    paged_reads::pin(
        &service.store,
        &snap,
        &saved.record,
        reads::glob(&reader, &snap, pattern, &path, &seen, &filters, limit, pos)
            .map(|value| visibility::settle(value, &seen, removed)),
    )
}
