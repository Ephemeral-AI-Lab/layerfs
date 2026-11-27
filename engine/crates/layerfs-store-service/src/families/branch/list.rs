//! `branch.list`: one Branch by name, else one page of Branches (`branch.get`
//! sends it with the name).
use crate::families::args::{invalid, project, Answer};
use crate::history::listing;
use crate::StoreService;
use layerfs_protocol::sdk::SdkValue;

/// `list({ name?, prefix?, excludePrefix?, after?, limit? })`: the
/// one Branch `name`, else one page of every Branch and main in name order,
/// read through the name index from `after` on.
pub fn list(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let word = |name: &str| match args.get(name) {
        None | Some(SdkValue::Null) => Ok(None),
        Some(SdkValue::Text(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(invalid(format!("{name} must be text"))),
    };
    if let Some(name) = word("name")? {
        return listing::branch(&service.store, &stack, name);
    }
    let names = listing::Names {
        prefix: word("prefix")?,
        exclude: word("excludePrefix")?,
        after: word("after")?,
    };
    listing::branches(&service.store, &stack, names, listing::limit(args)?)
}
