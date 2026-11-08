//! `fs.diff_files`.
use crate::methods::args::{project, selector, Answer};
use crate::{
    read::{file_compare, query},
    StoreService,
};
use layerfs_bridge::control::SdkValue;

/// `diff_files({ left, right }, { branch })`: two files compared line by line.
pub fn diff_files(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let branch = args.get_text("branch").unwrap_or("main");
    let boolean = |name| match args.get(name) {
        None | Some(SdkValue::Null) => Ok(false),
        Some(SdkValue::Bool(value)) => Ok(*value),
        _ => Err(crate::methods::args::invalid(format!(
            "{name} must be boolean"
        ))),
    };
    file_compare::diff(
        &service.store,
        &stack,
        selector(args, "left", branch),
        selector(args, "right", branch),
        file_compare::Options {
            include_attribution: boolean("includeAttribution")?,
            allow_missing: boolean("allowMissing")?,
            page: query::result_page(args)?,
            extended: ["page", "pageSize", "includeAttribution", "allowMissing"]
                .iter()
                .any(|name| args.get(name).is_some()),
        },
    )
}
