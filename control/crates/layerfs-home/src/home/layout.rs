//! `layerfs.json`: the home's layout version and its id (#533 step 3).
//!
//! A home holds a `layerfs.json` of this build's layout (#536 §5, Sandboxes
//! at `sandbox/<name>/`). The id is 16 random bytes in hex, written once;
//! the `docker` machine names its containers with it (O33) and backups
//! record it as where they were exported from. A folder that is not such a
//! home, and is not empty, is refused with a fix that moves it aside: this
//! build writes nothing in it. Reading never changes anything.
use super::records::{private_dir, read_record, unreadable, write_record};
use crate::outcome::{refused, shell_word, with_fix};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// The layout this build writes.
pub const LAYOUT_VERSION: i64 = 4;

/// What `layerfs.json` holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeFile {
    /// The layout version.
    pub layout: i64,
    /// The home's id, 32 hex digits.
    pub id: String,
}

/// Which layout a home has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    /// No folder, or one holding only what 2.0's layout allows before its
    /// `layerfs.json` ([`own_only`]): a new home.
    Empty,
    /// Not a home of this build: a `layerfs.json` of another layout or one
    /// that cannot be parsed, or anything else in a folder with no
    /// `layerfs.json`. Refused
    /// ([`foreign`]) and never written.
    Foreign,
    /// The current layout, with its `layerfs.json`.
    Current(HomeFile),
}

/// The home's layout, read only. A `layerfs.json` that cannot be parsed
/// (not JSON, no layout number, or this layout with no 32-hex id) or that
/// names another layout is not a 2.0 home's: [`Layout::Foreign`] (MIG,
/// RC-8). Refused only when the system refused the read of `layerfs.json`
/// or the home folder's listing: `Failed` with `unreadable` in its detail
/// ([`unreadable`]), which may be a real 2.0 home.
pub fn detect(home: &Path) -> Result<Layout, SdkRefusal> {
    let read = match read_record(&home.join("layerfs.json")) {
        Ok(read) => read,
        Err(refusal) if refusal.detail.get_bool("unreadable") => return Err(refusal),
        // [`record_of`]'s parse failure, which never sets `unreadable`
        Err(_) => return Ok(Layout::Foreign),
    };
    match read {
        Some(value) => Ok(match value.get_int("layout") {
            Some(version @ (3 | LAYOUT_VERSION)) => match parse(&value, version) {
                Some(file) => {
                    if version == 3 {
                        super::legacy_host::check(home)?;
                    }
                    Layout::Current(file)
                }
                None => Layout::Foreign,
            },
            _ => Layout::Foreign,
        }),
        None => match own_only(home) {
            Ok(true) => Ok(Layout::Empty),
            Ok(false) => Ok(Layout::Foreign),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Layout::Empty),
            Err(error) => Err(unreadable(home, &error)),
        },
    }
}

/// The top-level folders the home reserves for integrations (the Home
/// README's top-level names). LayerFS creates none of them, but an
/// integration may make them before the home's first call, as
/// `@layerfs/runtime` does with `runtime/`.
const INTEGRATION: [&str; 4] = ["runtime", "job", "stat", "preset"];

/// Whether `home` holds nothing but what 2.0's own layout allows before its
/// `layerfs.json`: the home lock [`open`] takes first, `layerfs.lock`; the
/// temporary `layerfs.json` is renamed from (`.layerfs.json.<pid>.<n>.tmp`,
/// [`write_record`]), which another process may see mid-write or a killed
/// one may leave; the [`INTEGRATION`] folders; and macOS's `.DS_Store`.
fn own_only(home: &Path) -> std::io::Result<bool> {
    for entry in std::fs::read_dir(home)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let own = name == "layerfs.lock"
            || name == ".DS_Store"
            || (name.starts_with(".layerfs.json.") && name.ends_with(".tmp"))
            || (INTEGRATION.contains(&name.as_ref()) && entry.path().is_dir());
        if !own {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The fix for a folder that is not a home of this build: move it aside,
/// then make a new home at its path.
pub fn foreign_fix(home: &Path) -> String {
    let shown = home.display().to_string();
    format!(
        "mv {} {} && layerfs home init",
        shell_word(&shown),
        shell_word(&format!("{shown}.old"))
    )
}

/// The refusal of a folder that is not a home of this build:
/// `Unsupported`, with [`foreign_fix`].
pub fn foreign(home: &Path) -> SdkRefusal {
    let refusal = refused(
        "Unsupported",
        format!(
            "{} is not a LayerFS 2.0 home; this build writes nothing in it: move it aside and make a new home",
            home.display()
        ),
    );
    with_fix(refusal, foreign_fix(home))
}

/// A `layerfs.json` of this layout: its id must be 32 hex digits.
fn parse(value: &SdkValue, layout: i64) -> Option<HomeFile> {
    let id = value.get_text("id")?;
    (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| HomeFile {
        layout,
        id: id.to_owned(),
    })
}

/// The home's `layerfs.json`, written for an empty home; any other folder
/// is refused ([`foreign`]).
pub fn ensure(home: &Path) -> Result<HomeFile, SdkRefusal> {
    let initial = detect(home)?;
    let expected = match initial {
        Layout::Current(file) if file.layout == LAYOUT_VERSION => return Ok(file),
        Layout::Current(file) => Some(file.id),
        Layout::Foreign => return Err(foreign(home)),
        Layout::Empty => None,
    };
    let _lock = crate::home::lock::take(home)?;
    match detect(home)? {
        Layout::Current(mut file) if expected.as_ref().is_none_or(|id| id == &file.id) => {
            if file.layout != LAYOUT_VERSION {
                file.layout = LAYOUT_VERSION;
                write(home, &file)?;
            }
            Ok(file)
        }
        Layout::Empty if expected.is_none() => create(home),
        Layout::Foreign => Err(foreign(home)),
        _ => Err(refused(
            "Unavailable",
            "home identity changed while opening it",
        )),
    }
}

/// The home ready for calls, as every call opens it first: created under
/// the home lock when empty, any other folder refused ([`foreign`]).
/// Answers its `layerfs.json`. A worker's root is not a home: it answers the id its
/// caller sent, and nothing is written.
pub fn open(home: &Path) -> Result<HomeFile, SdkRefusal> {
    if crate::home::guest::is_root(home) {
        return crate::home::guest::file(home);
    }
    let file = ensure(home)?;
    crate::home::guest::opened(&file.id);
    Ok(file)
}

/// Writes a new `layerfs.json` with a new id.
pub fn create(home: &Path) -> Result<HomeFile, SdkRefusal> {
    private_dir(home)?;
    let random = crate::helpers::random()?;
    let file = HomeFile {
        layout: LAYOUT_VERSION,
        id: layerfs_protocol::sdk::hex(&random[..16]),
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
