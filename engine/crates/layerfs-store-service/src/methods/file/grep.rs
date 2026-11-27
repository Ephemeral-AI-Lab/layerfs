//! `fs.grep`.
use crate::methods::args::{
    invalid, optional_path, paging, snapshot_in, snapshot_target, text, Answer,
};
use crate::read::cached::Cached;
use crate::{
    read::{paged_reads, query, reads, search, visibility},
    StoreService,
};
use layerfs_content::compare::memo::Memo;
use layerfs_protocol::sdk::SdkValue;

/// `grep(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: literal matches per line in text files.
pub fn grep(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, name) = snapshot_target(service, args)?;
    // The call's caches, from the root's open on (#818 R4).
    let cached = Cached::new(&reader)?;
    let (snap, saved) = snapshot_in(service, &stack, &cached, name)?;
    let pattern = text(args, "pattern")?;
    if pattern.is_empty() {
        return Err(invalid("pattern is empty"));
    }
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    // One path walk for the call (#818 R3), through its caches and memo.
    let memo = Memo::new(&cached);
    let entry = visibility::checked(&memo, &snap, &path, &seen)?;
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
            &saved.recorded,
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
        &saved.record,
        &saved.recorded,
        reads::grep(
            &cached, memo, &snap, pattern, &path, entry, &seen, &filters, limit, pos,
        ),
    )
}
