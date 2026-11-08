//! `fs.grep`.
use crate::methods::args::{invalid, optional_path, paging, snapshot, text, Answer};
use crate::{
    read::{paged_reads, query, reads},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `grep(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: literal matches per line in text files.
pub fn grep(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let pattern = text(args, "pattern")?;
    if pattern.is_empty() {
        return Err(invalid("pattern is empty"));
    }
    let path = optional_path(args)?;
    if args.get("page").is_some() {
        return paged_reads::pin(
            &service.store,
            &snap,
            paged_reads::grep(
                &service.store,
                &reader,
                &snap,
                pattern,
                &path,
                query::result_page(args)?,
            ),
        );
    }
    let (limit, pos) = paging(args)?;
    paged_reads::pin(
        &service.store,
        &snap,
        reads::grep(&reader, &snap, pattern, &path, limit, pos),
    )
}
