//! `layerfs.json`: the home's layout version and its id (#533 step 3).
//!
//! A home holds a `layerfs.json` of this build's layout (#536 §5, Sandboxes
//! at `sandbox/<name>/`). The id is 16 random bytes in hex, written once;
//! the `docker` machine names its containers with it (O33) and backups
//! record it as where they were exported from. A folder that is not such a
//! home is refused: one holding a home's own data with no `layerfs.json`
//! ([`Layout::DataWithoutMarker`]) with the fix that adopts it and never
//! touches the data, another home's folder with a fix that moves it aside,
//! and a machine's worker root ([`Layout::WorkerRoot`],
//! [`worker_root_shape`]) by naming the real home instead. This build
//! writes nothing in either. Reading never changes anything.
use super::paths;
use super::records::{private_dir, read_record, unreadable, write_record};
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

/// Which layout a home has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    /// No folder, or one holding only what 2.0's layout allows before its
    /// `layerfs.json` (the home lock, the temporary a marker is renamed
    /// from, the integration folders, `.DS_Store`): a new home.
    Empty,
    /// A folder holding a home's own data (`store/`, `machine/`,
    /// `sandbox/`, `backup/`, `secret/`, `trace/`, `log/`, `incoming/`,
    /// `generation`) and no `layerfs.json`: a home that lost its marker.
    /// It is refused ([`data_without_marker`]) until
    /// `layerfs home init --adopt` writes one, and never written before
    /// (layerfs-project#752, c6).
    DataWithoutMarker,
    /// Not a home of this build: a `layerfs.json` of another layout or one
    /// that cannot be parsed, or anything else in a folder with no
    /// `layerfs.json` that is neither a home's data nor a machine's worker
    /// root. Refused ([`foreign`]) and never written.
    Foreign,
    /// The current layout, with its `layerfs.json`.
    Current(HomeFile),
    /// A machine's worker root, recognised by its content
    /// ([`worker_root_shape`]), not a home of this build. Refused
    /// ([`worker_root`]) and never written.
    WorkerRoot,
}

/// What a folder with no `layerfs.json` is, from one listing of it
/// (layerfs-project#752, c6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unmarked {
    /// Nothing but what a new home may hold before its marker.
    Empty,
    /// A home's own data beside nothing foreign.
    Data,
    /// Anything else.
    Foreign,
}

/// The home's layout, read only. A `layerfs.json` that cannot be parsed
/// (not JSON, no layout number, or this layout with no 32-hex id) or that
/// names another layout is not a 2.0 home's: [`Layout::Foreign`] (MIG,
/// RC-8). A folder with no `layerfs.json` whose content is a machine's
/// worker root is [`Layout::WorkerRoot`], told before any other decision.
/// A folder with no `layerfs.json` that holds a home's own data is
/// [`Layout::DataWithoutMarker`], told before the empty/foreign one; only
/// what a new home may hold before its marker is [`Layout::Empty`]. Refused
/// only when the system refused the read of `layerfs.json` or the home
/// folder's listing: `Failed` with `unreadable` in its detail
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
            Some(LAYOUT_VERSION) => parse(&value).map_or(Layout::Foreign, Layout::Current),
            _ => Layout::Foreign,
        }),
        // a worker's root has no `layerfs.json` either: tell it from a
        // folder of another home by what a worker keeps, before deciding
        // it is empty, a home's data or foreign (layerfs-project#752, c5)
        None if worker_root_shape(home) => Ok(Layout::WorkerRoot),
        None => match classify(home) {
            Ok(Unmarked::Empty) => Ok(Layout::Empty),
            Ok(Unmarked::Data) => Ok(Layout::DataWithoutMarker),
            Ok(Unmarked::Foreign) => Ok(Layout::Foreign),
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

/// The top-level entries a home of this build keeps: the data the layout
/// owns. A folder with no `layerfs.json` holding any of them is a home
/// that lost its marker, not an empty or a foreign folder
/// (layerfs-project#752, c6). `generation` is a file; the rest are folders.
const DATA: [&str; 9] = [
    "store",
    "machine",
    "sandbox",
    "backup",
    "secret",
    "trace",
    "log",
    "incoming",
    "generation",
];

/// What a folder with no `layerfs.json` holds, from one `read_dir` pass:
/// [`Unmarked::Empty`] when every entry is one a new home may hold before
/// its marker ([`own_name`]); [`Unmarked::Data`] when the entries are those
/// or a [`DATA`] entry, at least one data entry is there, and every data
/// entry is a folder (`generation` may be a file); [`Unmarked::Foreign`]
/// otherwise. Any IO error is the caller's (a home listing the system
/// refuses is [`unreadable`]).
fn classify(home: &Path) -> std::io::Result<Unmarked> {
    let mut data = false;
    for entry in std::fs::read_dir(home)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if own_name(&entry, &name) {
            continue;
        }
        if DATA.contains(&name.as_ref()) {
            if name != "generation" && !entry.path().is_dir() {
                return Ok(Unmarked::Foreign);
            }
            data = true;
            continue;
        }
        return Ok(Unmarked::Foreign);
    }
    Ok(match data {
        true => Unmarked::Data,
        false => Unmarked::Empty,
    })
}

/// Whether `entry` is one a new home may hold before its `layerfs.json`:
/// the home lock [`open`] takes first, `layerfs.lock`; the temporary
/// `layerfs.json` is renamed from (`.layerfs.json.<pid>.<n>.tmp`,
/// [`write_record`]), which another process may see mid-write or a killed
/// one may leave; the [`INTEGRATION`] folders; and macOS's `.DS_Store`.
fn own_name(entry: &std::fs::DirEntry, name: &str) -> bool {
    name == "layerfs.lock"
        || name == ".DS_Store"
        || (name.starts_with(".layerfs.json.") && name.ends_with(".tmp"))
        || (INTEGRATION.contains(&name) && entry.path().is_dir())
}

/// Whether `dir` holds a machine's worker root by content (layerfs-project#752,
/// c5): at least one of `store/`, `sandbox/` and the local backends' run
/// root ([`paths::run_root`]); and either a `project.init` folder copy
/// (`incoming/init-*`) or the worker's own log ([`paths::worker_log`]); and
/// none of a home's `trace/` ([`paths::trace_dir`]), `backup/` or
/// `layerfs.json`. The entries are the ones [`paths`] names, so a moved
/// entry cannot make a home look like a worker's root. Any read the system
/// refuses answers `false`: `detect` then reports the folder as it would
/// without this check.
pub fn worker_root_shape(dir: &Path) -> bool {
    let worker_data =
        dir.join("store").is_dir() || dir.join("sandbox").is_dir() || paths::run_root(dir).is_dir();
    if !worker_data {
        return false;
    }
    if !paths::worker_log(dir).is_file() && !has_init_copy(dir) {
        return false;
    }
    !paths::trace_dir(dir).exists()
        && !dir.join("backup").exists()
        && !dir.join("layerfs.json").exists()
}

/// Whether `dir/incoming/` holds a `project.init` folder copy, `init-*`.
fn has_init_copy(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir.join("incoming")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_name().to_string_lossy().starts_with("init-") && entry.path().is_dir()
    })
}

/// The fix for a machine's worker root mistaken for a home: name the real
/// home instead. It never moves or initializes the root.
pub fn worker_root_fix() -> &'static str {
    "unset LAYERFS_HOME"
}

/// The refusal of a machine's worker root mistaken for a home:
/// `Unsupported`, with [`worker_root_fix`]. Nothing is written.
pub fn worker_root(root: &Path) -> SdkRefusal {
    let refusal = refused(
        "Unsupported",
        format!(
            "{} is a machine's worker root, not a LayerFS home; this build writes nothing in it: point LAYERFS_HOME at your home",
            root.display()
        ),
    );
    with_fix(refusal, worker_root_fix())
}

/// The fix for a home that lost its `layerfs.json`: write the marker for the
/// data that is already there. `--adopt` reads the records for the id, writes
/// `layerfs.json` and audits the home read only; it never moves, rewrites or
/// initializes the data.
pub fn data_without_marker_fix() -> &'static str {
    "layerfs home init --adopt"
}

/// The refusal of a home that lost its `layerfs.json`:
/// `Unsupported`, with [`data_without_marker_fix`]. Nothing is written.
pub fn data_without_marker(home: &Path) -> SdkRefusal {
    let refusal = refused(
        "Unsupported",
        format!(
            "{} holds a LayerFS home's data but no layerfs.json; this build writes nothing in it: adopt it with `layerfs home init --adopt`, or move it aside if you do not want the data",
            home.display()
        ),
    );
    with_fix(refusal, data_without_marker_fix())
}

/// The fix for a folder that is not a home of this build: move it aside,
/// then make a new home at its path. A folder holding a home's data without
/// its marker is not this case ([`data_without_marker`] comes first).
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
fn parse(value: &SdkValue) -> Option<HomeFile> {
    let id = value.get_text("id")?;
    (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| HomeFile {
        layout: LAYOUT_VERSION,
        id: id.to_owned(),
    })
}

/// The home's `layerfs.json`, written for an empty home; any other folder
/// is refused ([`foreign`], or for a machine's worker root [`worker_root`],
/// or for a home that lost its marker [`data_without_marker`]).
pub fn ensure(home: &Path) -> Result<HomeFile, SdkRefusal> {
    match detect(home)? {
        Layout::Current(file) => Ok(file),
        Layout::WorkerRoot => Err(worker_root(home)),
        Layout::DataWithoutMarker => Err(data_without_marker(home)),
        Layout::Foreign => Err(foreign(home)),
        Layout::Empty => create(home),
    }
}

/// The home ready for calls, as every call opens it first: created under
/// the home lock when empty, any other folder refused ([`foreign`], for a
/// machine's worker root [`worker_root`], or for a home that lost its
/// marker [`data_without_marker`]). Answers its `layerfs.json`.
/// A worker's root this process serves is not a home either: it answers
/// the id its caller sent, and nothing is written.
pub fn open(home: &Path) -> Result<HomeFile, SdkRefusal> {
    if crate::home::guest::is_root(home) {
        return crate::home::guest::file(home);
    }
    let file = match detect(home)? {
        Layout::Current(file) => file,
        Layout::WorkerRoot => return Err(worker_root(home)),
        Layout::DataWithoutMarker => return Err(data_without_marker(home)),
        Layout::Foreign => return Err(foreign(home)),
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
/// answers its caller's id (`guest`), a current home its file, and a home
/// that lost its marker is refused with the adopt fix. The entities read
/// their home's id here; opening (creating) a home is [`open`], which every
/// call runs first (layerfs-project#104).
pub fn opened(home: &Path) -> Result<HomeFile, SdkRefusal> {
    if crate::home::guest::is_root(home) {
        return crate::home::guest::file(home);
    }
    match detect(home)? {
        Layout::Current(file) => Ok(file),
        Layout::WorkerRoot => Err(worker_root(home)),
        Layout::DataWithoutMarker => Err(data_without_marker(home)),
        _ => Err(refused(
            "InvalidInput",
            format!(
                "{} is not an open LayerFS home; a call opens (creates) it first",
                home.display()
            ),
        )),
    }
}
