//! `fs.glob`.
use crate::methods::args::{optional_path, paging, snapshot, text, Answer};
use crate::{
    read::{paged_reads, query, reads},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `glob(pattern, { branch, commit?, path?, pageSize?, pagePos? })`: paths matching a `/.lww`-syntax pattern.
pub fn glob(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let pattern = text(args, "pattern")?;
    let path = optional_path(args)?;
    if args.get("page").is_some() {
        return paged_reads::pin(
            &service.store,
            &snap,
            paged_reads::glob(
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
        reads::glob(&reader, &snap, pattern, &path, limit, pos),
    )
}
