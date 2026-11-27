//! `fs.glob`.
use crate::families::args::{optional_path, paging, snapshot_in, snapshot_target, text, Answer};
use crate::filesystem::cache::Cached;
use crate::{
    filesystem::{
        paged::{query, results},
        text::search,
        tree::{positional, visibility},
    },
    StoreService,
};
use layerfs_content::compare::memo::Memo;
use layerfs_protocol::sdk::SdkValue;

/// `glob(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: paths matching a `/.lww`-syntax pattern.
pub fn glob(service: &StoreService, args: &SdkValue) -> Answer {
    let (stack, reader, name) = snapshot_target(service, args)?;
    // The call's caches, from the root's open on (#818 R4).
    let cached = Cached::new(&reader)?;
    let (snap, saved) = snapshot_in(service, &stack, &cached, name)?;
    let pattern = text(args, "pattern")?;
    let path = optional_path(args)?;
    let seen = visibility::Visibility::load(&service.store, &stack, &snap, saved.removals)?;
    // One path walk for the call (#818 R3), through its caches and memo.
    let memo = Memo::new(&cached);
    let entry = visibility::checked(&memo, &snap, &path, &seen)?;
    // The page-independent `removed_count`: the hidden entries this pattern
    // would return anywhere under `path`, not only direct children.
    let removed = seen.hidden_matches(&memo, &snap, &path, pattern)?;
    let filters = search::Filters::of(args)?;
    if args.get("page").is_some() {
        if filters.any() {
            return Err(crate::families::args::invalid(
                "page cannot be combined with include, exclude or ignoreCase",
            ));
        }
        return results::pin(
            &service.store,
            &snap,
            &saved.record,
            &saved.recorded,
            results::glob(
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
    results::pin(
        &service.store,
        &snap,
        &saved.record,
        &saved.recorded,
        positional::glob(
            memo,
            &snap,
            pattern,
            &path,
            entry,
            &seen,
            &filters,
            limit,
            pos,
            // Wide windows where a demand costs a round trip (#818 R6).
            cached.wide(),
        )
        .map(|value| visibility::settle(value, &seen, removed)),
    )
}
