//! `home.init`: setting a home up on purpose. It opens the home as every
//! call does (created when empty; a folder that is not a LayerFS 2.0 home
//! is refused with the fix that moves it aside) and says which it did
//! (`outcome`: `created` or `unchanged`).
use super::get::{state, Found};
use layerfs_home::reply::outcome::{refused, Outcome};
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// `home.init`: the home ready for calls, as the first call of any other
/// kind makes it: `{ home, unchanged, outcome, layout, version }`. Refused
/// inside a machine's worker root, which is not a home, and for a folder
/// that is not a LayerFS 2.0 home (`Unsupported`, with the fix that moves
/// it aside).
pub fn init(home: &Path) -> Outcome {
    if layerfs_home::disk::guest::is_root(home) {
        return Err(refused(
            "InvalidInput",
            "a machine's worker root is not a home; run `layerfs home init` on the host",
        ));
    }
    let (before, _) = state(home)?;
    let file = layerfs_home::disk::open(home)?;
    #[cfg(feature = "machine")]
    layerfs_machine::ensure_built_ins(home, &file.id)?;
    let outcome = match before {
        Found::Current => "unchanged",
        _ => "created",
    };
    Ok(SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("unchanged", SdkValue::Bool(before == Found::Current)),
        ("outcome", SdkValue::text(outcome)),
        ("layout", SdkValue::Int(file.layout)),
        ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
    ]))
}
