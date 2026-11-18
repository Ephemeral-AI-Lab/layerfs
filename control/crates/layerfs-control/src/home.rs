//! `home.init` and `home.get`: setting a home up on purpose, and reading what
//! this build would use with it without changing anything. `init` opens the
//! home as every call does (created when empty, converted from layout 2 or
//! 1.1.0, an interrupted conversion finished) and says which of those it did.
//! `get` reads files only: no process, Docker, network, lock or log line, so a
//! client can poll it, and it answers for a home that does not exist yet.
//! What it answers is the call surface's (2.0): the home's `state` with
//! its `reason` and `fix`, the build's `version`, the home's `generation`,
//! and, only when asked, the platforms this computer supports (a probe)
//! and the tree of everything in the home ([`crate::home_tree`]).
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::layout::{detect, Layout};
use layerfs_home::home::{generation, read_record, with_field};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::state::{Reason, State};
use std::path::Path;

/// The identity of the build making the call (`LAYERFS_BUILD_COMMIT` and
/// `LAYERFS_BUILD_DIRTY` when it was compiled), compared with the Linux
/// bundle's and the native daemon's; `commit` is `None` when it was built
/// without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Build<'a> {
    /// The 40-digit commit, if known.
    pub commit: Option<&'a str>,
    /// Whether the tree had uncommitted changes.
    pub dirty: bool,
}

/// The text a binary carries to name its build (`layerfs-daemon`):
/// the marker, then `<commit>:clean`, `<commit>:dirty` or `unknown`.
pub const BUILD_MARKER: &[u8] = b"layerfs-build-identity:";

/// What a home is, before anything opens it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Found {
    Missing,
    Current,
    NeedsConversion,
}

/// The sections `home.get` adds when asked (`--with`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct With {
    /// `platforms`: what this computer supports, probed.
    pub platforms: bool,
    /// `tree`: everything in the home, from its records.
    pub tree: bool,
}

/// The one command that makes a missing or older home current.
const INIT: &str = "layerfs home init";

/// The home's state and layout, read only; a `layerfs.json` this build
/// cannot read (or of a newer layout) is refused, as `detect` does.
fn state(home: &Path) -> Result<(Found, Option<i64>), layerfs_bridge::control::SdkRefusal> {
    Ok(match detect(home)? {
        Layout::Empty => (Found::Missing, None),
        Layout::Old => (Found::NeedsConversion, None),
        Layout::Previous(file) => (Found::NeedsConversion, Some(file.layout)),
        Layout::Current(file) if crate::migrate::layout_3_pending(home) => {
            (Found::NeedsConversion, Some(file.layout))
        }
        Layout::Current(file) => (Found::Current, Some(file.layout)),
    })
}

/// `home.init`: the home ready for calls, as the first call of any other
/// kind makes it. Refused inside a machine's worker root, which is not a
/// home, and `InUse` while a layout 2 home has live Sandboxes.
pub fn init(home: &Path) -> Outcome {
    if layerfs_home::home::guest::is_root(home) {
        return Err(refused(
            "InvalidInput",
            "a machine's worker root is not a home; run `layerfs home init` on the host",
        ));
    }
    let (before, _) = state(home)?;
    let file = crate::migrate::open(home)
        .inspect_err(|refusal| crate::migrate::note_failure(home, refusal))?;
    #[cfg(feature = "machine")]
    layerfs_machine::ensure_built_ins(home, &file.id)?;
    Ok(SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("created", SdkValue::Bool(before == Found::Missing)),
        (
            "converted",
            SdkValue::Bool(before == Found::NeedsConversion),
        ),
        ("layout", SdkValue::Int(file.layout)),
    ]))
}

/// `home.get`: the home's path, `state` (with `reason`, `fix` and the
/// sentence behind them as `message` when it is broken), the build's
/// `version`, the home's `generation`, `layout` and `mode`, the Linux
/// bundle and native daemon this build would use and whether they come from
/// the same build, and the cache root. A home it cannot read is answered
/// `broken`, not refused. `with` adds `platforms` and `tree`; the tree is
/// null unless the home is ready (there are no records of this layout to
/// read).
pub fn get(home: &Path, build: Build<'_>, with: With) -> Outcome {
    let shown = home.display().to_string();
    let (state, layout, message) = match state(home) {
        Ok((Found::Current, layout)) => (State::Ready, layout, None),
        Ok((Found::Missing, layout)) => (
            State::broken(Reason::Missing, Some(INIT)),
            layout,
            Some(format!("no LayerFS home at {shown}; `{INIT}` creates it")),
        ),
        Ok((Found::NeedsConversion, layout)) => (
            State::broken(Reason::Outdated, Some(INIT)),
            layout,
            Some(format!("the home at {shown} needs converting to this build's layout; `{INIT}` converts it (older LayerFS builds then refuse it)")),
        ),
        Err(refusal) => (State::broken(Reason::Failed, None), None, Some(refusal.message)),
    };
    let ready = state == State::Ready;
    let cache = match layerfs_machine::bundle::cache_root() {
        Ok(path) => SdkValue::map([("path", SdkValue::text(path.display().to_string()))]),
        Err(_) => SdkValue::Null,
    };
    let mut value = state.write(SdkValue::map([
        ("home", SdkValue::text(shown)),
        ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
        (
            "generation",
            match ready {
                true => SdkValue::Int(generation::read(home)),
                false => SdkValue::Null,
            },
        ),
        ("message", SdkValue::opt_text(message)),
        ("layout", layout.map_or(SdkValue::Null, SdkValue::Int)),
        ("mode", mode(home).map_or(SdkValue::Null, SdkValue::text)),
        ("bundle", bundle_value(build)),
        ("daemon", daemon_value(home, build)),
        ("cache", cache),
    ]));
    if with.platforms {
        value = with_field(value, "platforms", platforms(home)?);
    }
    if with.tree {
        let tree = match ready {
            true => crate::home_tree::tree(home)?,
            false => SdkValue::Null,
        };
        value = with_field(value, "tree", tree);
    }
    Ok(value)
}

/// `--with platforms`: `{ host, items }`, one row per platform this build
/// knows with what it is missing here; what `machine.platforms` answers,
/// with a row's `kernel` as `route` and an option's `name` as `option`.
#[cfg(feature = "machine")]
fn platforms(home: &Path) -> Outcome {
    let answered = layerfs_machine::platforms(home)?;
    let renamed = |value: &SdkValue, from: &str, to: &str| match value {
        SdkValue::Map(fields) => SdkValue::Map(
            (fields.iter())
                .map(|(name, field)| match name == from {
                    true => (to.to_owned(), field.clone()),
                    false => (name.clone(), field.clone()),
                })
                .collect(),
        ),
        other => other.clone(),
    };
    let row = |row: &SdkValue| {
        let mut row = renamed(row, "kernel", "route");
        let options: Option<Vec<SdkValue>> = match row.get("options") {
            Some(SdkValue::List(options)) => Some(
                (options.iter())
                    .map(|o| renamed(o, "name", "option"))
                    .collect(),
            ),
            _ => None,
        };
        if let Some(options) = options {
            row = with_field(row, "options", SdkValue::List(options));
        }
        row
    };
    let items = match answered.get("platforms") {
        Some(SdkValue::List(rows)) => rows.iter().map(row).collect(),
        _ => Vec::new(),
    };
    Ok(SdkValue::map([
        ("host", answered.get("host").cloned().unwrap_or_default()),
        ("items", SdkValue::List(items)),
    ]))
}

/// Without the `machine` feature (the guest worker's build) there are no
/// machine adapters to ask.
#[cfg(not(feature = "machine"))]
fn platforms(_home: &Path) -> Outcome {
    Err(refused(
        "Unsupported",
        "this build has no machine adapters, so it lists no platforms",
    ))
}

/// The home folder's permission bits, as `0700`; none while there is no
/// folder. Whether they are owner-only is the home doctor's to say.
#[cfg(unix)]
fn mode(home: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(home).ok()?;
    Some(format!("{:04o}", meta.permissions().mode() & 0o7777))
}

#[cfg(not(unix))]
fn mode(_home: &Path) -> Option<String> {
    None
}

/// Whether a part with `commit`/`dirty` comes from `build`: `false` when the
/// commits differ, `true` when they are equal and both trees were clean, and
/// null when either commit is unknown or a tree was dirty (it cannot tell).
fn matches(build: Build<'_>, commit: Option<&str>, dirty: Option<bool>) -> SdkValue {
    match (build.commit, commit) {
        (Some(ours), Some(theirs)) if ours != theirs => SdkValue::Bool(false),
        (Some(_), Some(_)) if !build.dirty && dirty == Some(false) => SdkValue::Bool(true),
        _ => SdkValue::Null,
    }
}

fn short(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

fn optional(text: Option<&str>) -> SdkValue {
    text.map_or(SdkValue::Null, SdkValue::text)
}

fn bundle_value(build: Build<'_>) -> SdkValue {
    let Ok(dir) = layerfs_machine::bundle_dir("linux") else {
        return SdkValue::Null;
    };
    let source = if std::env::var_os("LAYERFS_BUNDLE").is_some_and(|d| !d.is_empty()) {
        "env:LAYERFS_BUNDLE"
    } else {
        "cache"
    };
    let manifest = read_record(&dir.join("bundle.json")).ok().flatten();
    let origin = manifest.as_ref().and_then(|m| m.get("source"));
    let commit = origin.and_then(|s| s.get_text("head"));
    let dirty = match origin.and_then(|s| s.get("dirty")) {
        Some(SdkValue::Bool(dirty)) => Some(*dirty),
        _ => None,
    };
    let matched = matches(build, commit, dirty);
    SdkValue::map([
        ("path", SdkValue::text(dir.display().to_string())),
        ("source", SdkValue::text(source)),
        ("present", SdkValue::Bool(manifest.is_some())),
        ("commit", optional(commit)),
        ("dirty", dirty.map_or(SdkValue::Null, SdkValue::Bool)),
        ("matches", matched),
    ])
}

/// The daemon the native machine runs, as its Sandbox launch chooses it: the
/// one its record names (`recorded`), else the one beside this host's
/// bundle (`default`), with the build it names; read from files only.
pub struct NativeDaemon {
    /// Where it is.
    pub path: std::path::PathBuf,
    /// `recorded` or `default`.
    pub source: &'static str,
    /// Whether it is a file.
    pub present: bool,
    /// The commit and dirty flag stamped into it, when it names them.
    pub stamp: Option<(String, bool)>,
}

impl NativeDaemon {
    /// The native machine's daemon in `home` (a record that cannot be read
    /// leaves the default), or `None` when the user cache is unknown.
    pub fn of(home: &Path) -> Option<Self> {
        use layerfs_machine::record::MachineRecord;
        let recorded = MachineRecord::read(home, "native")
            .ok()
            .flatten()
            .and_then(|r| r.daemon().map(std::path::PathBuf::from));
        let (path, source) = match recorded {
            Some(path) => (path, "recorded"),
            None => (
                layerfs_machine::bundle_dir(std::env::consts::OS)
                    .ok()?
                    .join("layerfs-daemon"),
                "default",
            ),
        };
        // only a regular file is read (a FIFO or device would block or never end)
        let present = path.is_file();
        let stamp = present
            .then(|| std::fs::read(&path).ok())
            .flatten()
            .and_then(|bytes| stamp(&bytes));
        Some(Self {
            present,
            path,
            source,
            stamp,
        })
    }

    /// Whether it comes from `build` (as [`matches`] says).
    pub fn matches(&self, build: Build<'_>) -> SdkValue {
        let (commit, dirty) = match &self.stamp {
            Some((commit, dirty)) => (Some(commit.as_str()), Some(*dirty)),
            None => (None, None),
        };
        matches(build, commit, dirty)
    }

    /// `DaemonMissing`'s message.
    pub fn missing(&self) -> String {
        format!(
            "no native daemon at {}; the native machine needs this build's `layerfs-daemon`: `layerfs machine update native --daemon <path>`",
            self.path.display()
        )
    }

    /// `DaemonMismatch`'s message, when its commit differs from `build`'s.
    pub fn mismatch(&self, build: Build<'_>) -> Option<String> {
        let theirs = self.stamp.as_ref().map(|(commit, _)| commit.as_str())?;
        let ours = build.commit?;
        (self.matches(build) == SdkValue::Bool(false)).then(|| {
            format!(
                "the native daemon at {} was built from {}, this build from {}; record this build's daemon with `layerfs machine update native --daemon <path>`",
                self.path.display(),
                short(theirs),
                short(ours)
            )
        })
    }
}

/// The native machine's daemon (macOS only; null elsewhere): the one its
/// record names, else the default beside the macOS bundle.
#[cfg(target_os = "macos")]
fn daemon_value(home: &Path, build: Build<'_>) -> SdkValue {
    let Some(daemon) = NativeDaemon::of(home) else {
        return SdkValue::Null;
    };
    let (commit, dirty) = match &daemon.stamp {
        Some((commit, dirty)) => (Some(commit.as_str()), Some(*dirty)),
        None => (None, None),
    };
    SdkValue::map([
        ("path", SdkValue::text(daemon.path.display().to_string())),
        ("source", SdkValue::text(daemon.source)),
        ("present", SdkValue::Bool(daemon.present)),
        ("commit", optional(commit)),
        ("dirty", dirty.map_or(SdkValue::Null, SdkValue::Bool)),
        ("matches", daemon.matches(build)),
    ])
}

#[cfg(not(target_os = "macos"))]
fn daemon_value(_home: &Path, _build: Build<'_>) -> SdkValue {
    SdkValue::Null
}

/// The build a binary names after [`BUILD_MARKER`]: its commit and whether
/// the tree was dirty; `None` without the marker or for `unknown`.
pub fn stamp(bytes: &[u8]) -> Option<(String, bool)> {
    let at = bytes
        .windows(BUILD_MARKER.len())
        .position(|w| w == BUILD_MARKER)?
        + BUILD_MARKER.len();
    let rest = bytes.get(at..at + 46)?;
    let (commit, state) = rest.split_at(40);
    if !commit.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let dirty = match state {
        b":dirty" => true,
        b":clean" => false,
        _ => return None,
    };
    Some((String::from_utf8_lossy(commit).into_owned(), dirty))
}
