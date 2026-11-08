//! `fs.list`.
use crate::methods::args::{optional_path, paging, snapshot, Answer};
use crate::{
    read::{paged_reads, query, reads},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `list(path?, { branch, commit?, pageSize?, pagePos? })`: one window of a directory.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let (reader, snap) = snapshot(service, args)?;
    let path = optional_path(args)?;
    let answer = if args.get("page").is_some() {
        paged_reads::list(&reader, &snap, &path, query::result_page(args)?)
    } else {
        reads::list(&reader, &snap, &path, paging(args))
    };
    paged_reads::pin(&service.store, &snap, answer)
}
