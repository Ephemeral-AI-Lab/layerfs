//! `layerfs.json`: the home's layout version and its id (#533 step 3).
//!
//! A home without `layerfs.json` is 1.1.0's layout; with it, the 2.0
//! layout. The id is 16 random bytes in hex, written once; the `docker`
//! machine names its containers with it (O33) and backups record it as
//! where they were exported from. Reading never changes anything.
use super::records::{private_dir, read_record, write_record};
use crate::common::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

/// The layout this build writes.
pub const LAYOUT_VERSION: i64 = 2;

/// What `layerfs.json` holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeFile {
    /// The layout version.
    pub layout: i64,
    /// The home's id, 32 hex digits.
    pub id: String,
}

/// The 1.1.0 records at the top of a home; `<p>/project.record` marks one
/// too. A home with none of them and no `layerfs.json` is new, whatever
/// else is in it (such as the `layerfs.lock` the first call takes).
pub(super) const OLD: [&str; 9] = [
    ".stores",
    "stores.json",
    "stores.v1.json",
    "stores.json.lock",
    "remotes",
    "remote-state",
    ".runtime",
    "worker",
    "traces",
];

/// Which layout a home has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    /// No `layerfs.json` and no 1.1.0 record: a new home.
    Empty,
    /// 1.1.0's layout: its records but no `layerfs.json`.
    Old,
    /// The 2.0 layout, with its `layerfs.json`.
    Current(HomeFile),
}

/// The home's layout, read only. A `layerfs.json` this build cannot read,
/// or of a newer layout, is refused.
pub fn detect(home: &Path) -> Result<Layout, SdkRefusal> {
    let file = home.join("layerfs.json");
    match read_record(&file)? {
        Some(value) => parse(&value).map(Layout::Current),
        None => match old_records(home) {
            Ok(true) => Ok(Layout::Old),
            Ok(false) => Ok(Layout::Empty),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Layout::Empty),
            Err(error) => Err(refused("Failed", format!("{}: {error}", home.display()))),
        },
    }
}

/// Whether `home` holds a 1.1.0 record.
fn old_records(home: &Path) -> std::io::Result<bool> {
    for entry in std::fs::read_dir(home)? {
        let entry = entry?;
        let name = entry.file_name();
        if OLD.iter().any(|old| name == *old) || entry.path().join("project.record").is_file() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse(value: &SdkValue) -> Result<HomeFile, SdkRefusal> {
    let layout = value
        .get_int("layout")
        .ok_or_else(|| bad("no layout version"))?;
    let id = value.get_text("id").ok_or_else(|| bad("no home id"))?;
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("the home id is not 32 hex digits"));
    }
    if layout > LAYOUT_VERSION {
        return Err(refused(
            "Unsupported",
            format!("this home has layout {layout}; this build reads layout {LAYOUT_VERSION}"),
        ));
    }
    if layout < LAYOUT_VERSION {
        return Err(bad(&format!("unknown layout {layout}")));
    }
    Ok(HomeFile {
        layout,
        id: id.to_owned(),
    })
}

fn bad(why: &str) -> SdkRefusal {
    refused("Failed", format!("layerfs.json: {why}"))
}

/// The home's `layerfs.json`, written for an empty home. A 1.1.0 home is
/// refused here: it is converted once by its migration, which writes the
/// file last.
pub fn ensure(home: &Path) -> Result<HomeFile, SdkRefusal> {
    match detect(home)? {
        Layout::Current(file) => Ok(file),
        Layout::Old => Err(refused(
            "InvalidInput",
            format!(
                "{} is a LayerFS 1.1.0 home; it is converted on its first 2.0 call that writes",
                home.display()
            ),
        )),
        Layout::Empty => create(home),
    }
}

/// Writes a new `layerfs.json` with a new id.
pub(crate) fn create(home: &Path) -> Result<HomeFile, SdkRefusal> {
    private_dir(home)?;
    let random = crate::host::registry::random()?;
    let file = HomeFile {
        layout: LAYOUT_VERSION,
        id: layerfs_bridge::sdk_values::hex(&random[..16]),
    };
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([
            ("layout", SdkValue::Int(file.layout)),
            ("id", SdkValue::text(&file.id)),
        ]),
    )?;
    Ok(file)
}
