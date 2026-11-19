//! `layerfs.json`: the home's layout version and its id (#533 step 3).
//!
//! A home holds a `layerfs.json` of this build's layout (#536 §5, Sandboxes
//! at `sandbox/<name>/`). The id is 16 random bytes in hex, written once;
//! the `docker` machine names its containers with it (O33) and backups
//! record it as where they were exported from. A home of an older layout
//! (layout 2, or 1.1.0's records with no `layerfs.json`) is refused with a
//! fix that moves it aside: this build converts nothing. Reading never
//! changes anything.
use super::records::{private_dir, read_record, write_record};
use crate::outcome::{refused, shell_word, with_fix};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

/// The layout this build writes.
pub const LAYOUT_VERSION: i64 = 3;

/// What `layerfs.json` holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeFile {
    /// The layout version.
    pub layout: i64,
    /// The home's id, 32 hex digits.
    pub id: String,
}

/// The 1.1.0 records at the top of a home; `<p>/project.record` marks one
/// too. A folder with one of them and no `layerfs.json` is an older home,
/// never written over; one with none of them is new, whatever else is in
/// it (such as the `layerfs.lock` the first call takes).
const OLD: [&str; 9] = [
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
    /// An older LayerFS's home (layout 2, or 1.1.0's records): refused
    /// ([`older`]).
    Older,
    /// The current layout, with its `layerfs.json`.
    Current(HomeFile),
}

/// The home's layout, read only. A `layerfs.json` this build cannot read,
/// or of a newer layout, is refused.
pub fn detect(home: &Path) -> Result<Layout, SdkRefusal> {
    let file = home.join("layerfs.json");
    match read_record(&file)? {
        Some(value) => parse(&value).map(|file| match file.layout {
            LAYOUT_VERSION => Layout::Current(file),
            _ => Layout::Older,
        }),
        None => match old_records(home) {
            Ok(true) => Ok(Layout::Older),
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

/// The fix for an older home: move it aside, then make a new one at its
/// path.
pub fn older_fix(home: &Path) -> String {
    let shown = home.display().to_string();
    format!(
        "mv {} {} && layerfs home init",
        shell_word(&shown),
        shell_word(&format!("{shown}.old"))
    )
}

/// The refusal of an older home: `Unsupported`, with [`older_fix`].
pub fn older(home: &Path) -> SdkRefusal {
    let refusal = refused(
        "Unsupported",
        format!(
            "{} is the home of an older LayerFS; this build does not convert it: move it aside and make a new home",
            home.display()
        ),
    );
    with_fix(refusal, older_fix(home))
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
    // `layerfs.json` began at layout 2 (2.0)
    if layout < 2 {
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

/// The home's `layerfs.json`, written for an empty home; an older home is
/// refused ([`older`]).
pub fn ensure(home: &Path) -> Result<HomeFile, SdkRefusal> {
    match detect(home)? {
        Layout::Current(file) => Ok(file),
        Layout::Older => Err(older(home)),
        Layout::Empty => create(home),
    }
}

/// The home ready for calls, as every call opens it first: created under
/// the home lock when empty, an older home refused ([`older`]). Answers its
/// `layerfs.json`. A worker's root is not a home: it answers the id its
/// caller sent, and nothing is written.
pub fn open(home: &Path) -> Result<HomeFile, SdkRefusal> {
    if crate::home::guest::is_root(home) {
        return crate::home::guest::file(home);
    }
    let file = match detect(home)? {
        Layout::Current(file) => file,
        Layout::Older => return Err(older(home)),
        Layout::Empty => {
            let _lock = crate::home::lock::take(home)?;
            ensure(home)?
        }
    };
    crate::home::guest::opened(&file.id);
    Ok(file)
}

/// Writes a new `layerfs.json` with a new id.
pub fn create(home: &Path) -> Result<HomeFile, SdkRefusal> {
    private_dir(home)?;
    let random = crate::helpers::random()?;
    let file = HomeFile {
        layout: LAYOUT_VERSION,
        id: layerfs_bridge::sdk_values::hex(&random[..16]),
    };
    write(home, &file)?;
    Ok(file)
}

/// Writes `layerfs.json` as `file` says, replacing it whole.
pub fn write(home: &Path, file: &HomeFile) -> Result<(), SdkRefusal> {
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([
            ("layout", SdkValue::Int(file.layout)),
            ("id", SdkValue::text(&file.id)),
        ]),
    )
}

/// The `layerfs.json` of a home a call has already opened: a worker's root
/// answers its caller's id (`guest`), a current home its file. The entities
/// read their home's id here; opening (creating) a home is [`open`], which
/// every call runs first (layerfs-project#104).
pub fn opened(home: &Path) -> Result<HomeFile, SdkRefusal> {
    if crate::home::guest::is_root(home) {
        return crate::home::guest::file(home);
    }
    match detect(home)? {
        Layout::Current(file) => Ok(file),
        _ => Err(refused(
            "InvalidInput",
            format!(
                "{} is not an open LayerFS home; a call opens (creates) it first",
                home.display()
            ),
        )),
    }
}
