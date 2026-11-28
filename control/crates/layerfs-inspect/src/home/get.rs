//! `home.get`: reading what this build would use with a home without
//! changing anything (`home.init`, which sets one up, is `init.rs`).
//! `get` reads files only: no process, Docker, network, lock or log line, so a
//! client can poll it, and it answers for a home that does not exist yet.
//! What it answers is the call surface's (2.0): the home's `state` with
//! its `reason` and `fix`, the build's `version`, the home's `generation`,
//! and, only when asked, the platforms this computer supports (a probe)
//! and the tree of everything in the home ([`crate::home_tree`]).
use layerfs_home::disk::layout::{detect, Layout};
use layerfs_home::disk::{read_record, with_field};
use layerfs_home::names::HOST_MACHINE;
use layerfs_home::record::generation;
use layerfs_home::reply::outcome::Outcome;
use layerfs_home::reply::state::{Reason, State};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
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
pub(super) enum Found {
    Missing,
    Current,
    Foreign,
    Container,
}

/// The sections `home.get` adds when asked (`--with`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct With {
    /// `platforms`: what this computer supports, probed.
    pub platforms: bool,
    /// `tree`: everything in the home, from its records.
    pub tree: bool,
}

/// The one command that makes a missing home.
const INIT: &str = "layerfs home init";

/// The home's state and layout, read only; a home the system refuses to
/// read is refused, as `detect` does.
pub(super) fn state(
    home: &Path,
) -> Result<(Found, Option<i64>), layerfs_protocol::sdk::SdkRefusal> {
    Ok(match detect(home)? {
        Layout::Empty => (Found::Missing, None),
        Layout::Foreign => (Found::Foreign, None),
        Layout::Container => (Found::Container, None),
        Layout::Current(file) => (Found::Current, Some(file.layout)),
    })
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
        // a `layerfs.json` this build cannot parse, or of no layout it
        // knows, is not a 2.0 home's (MIG): `outdated` with the fix that
        // moves the folder aside (C11 gives a broken state its fix; RC-8)
        Ok((Found::Foreign, layout)) => (
            State::broken(
                Reason::Outdated,
                Some(&layerfs_home::disk::layout::foreign_fix(home)),
            ),
            layout,
            Some(layerfs_home::disk::layout::foreign(home).message),
        ),
        // a LayerFS folder holding `home/` and its clients' folders: not
        // a home, and never moved aside; the fix names its `home/`
        Ok((Found::Container, layout)) => (
            State::broken(
                Reason::Unsupported,
                Some(&layerfs_home::disk::layout::container_fix(home)),
            ),
            layout,
            Some(layerfs_home::disk::layout::container(home).message),
        ),
        // `detect` refuses only a read the system refused (permissions,
        // I/O), which may be a real 2.0 home: `failed` with the OS error,
        // never the move aside (RC-8)
        Err(refusal) => (
            State::broken(Reason::Failed, None),
            None,
            Some(refusal.message),
        ),
    };
    let ready = state == State::Ready;
    let cache = match layerfs_machine::setup::bundle::cache_root() {
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
            true => crate::home::tree::tree(home)?,
            false => SdkValue::Null,
        };
        value = with_field(value, "tree", tree);
    }
    Ok(value)
}

/// `--with platforms`: `{ host, items }`, one row per platform this build
/// knows with what it is missing here ([`layerfs_machine::platforms`]).
#[cfg(feature = "machine")]
fn platforms(home: &Path) -> Outcome {
    layerfs_machine::platforms(home)
}

/// Without the `machine` feature (the guest worker's build) there are no
/// machine adapters to ask.
#[cfg(not(feature = "machine"))]
fn platforms(_home: &Path) -> Outcome {
    Err(layerfs_home::reply::outcome::refused(
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
    let Ok((dir, source)) = layerfs_machine::bundle_place("linux") else {
        return SdkValue::Null;
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

/// The daemon the host machine runs, as its Sandbox launch chooses it: the
/// one its record names (`recorded`), else the one beside this host's
/// bundle (`default`), with the build it names; read from files only.
pub struct HostDaemon {
    /// Where it is.
    pub path: std::path::PathBuf,
    /// `recorded` or `default`.
    pub source: &'static str,
    /// Whether it is a file.
    pub present: bool,
    /// The commit and dirty flag stamped into it, when it names them.
    pub stamp: Option<(String, bool)>,
}

impl HostDaemon {
    /// The host machine's daemon in `home` (a record that cannot be read
    /// leaves the default), or `None` when the user cache is unknown.
    pub fn of(home: &Path) -> Option<Self> {
        use layerfs_machine::record::machine::MachineRecord;
        let recorded = MachineRecord::read(home, layerfs_home::names::HOST_MACHINE)
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
            "no host machine daemon at {}; the host machine needs this build's `layerfs-daemon`: `layerfs machine update {HOST_MACHINE} --daemon <path>`",
            self.path.display()
        )
    }

    /// `DaemonMismatch`'s message, when its commit differs from `build`'s.
    pub fn mismatch(&self, build: Build<'_>) -> Option<String> {
        let theirs = self.stamp.as_ref().map(|(commit, _)| commit.as_str())?;
        let ours = build.commit?;
        (self.matches(build) == SdkValue::Bool(false)).then(|| {
            format!(
                "the host machine's daemon at {} was built from {}, this build from {}; record this build's daemon with `layerfs machine update {HOST_MACHINE} --daemon <path>`",
                self.path.display(),
                short(theirs),
                short(ours)
            )
        })
    }
}

/// `refusal` of a Sandbox start whose daemon (its detail's `daemon`) is the
/// native machine's and of another build than `build`, or names no build
/// at all (one older than the build marker): it says so, and its `fix`
/// records this build's daemon when [`own_daemon`] finds one, else runs
/// `layerfs home doctor`, which names the daemon and its build. Any other
/// refusal comes back as it is; one whose detail names no `daemon` (no
/// start failed) without reading anything.
pub fn native_start(home: &Path, build: Build<'_>, refusal: SdkRefusal) -> SdkRefusal {
    let Some(started) = refusal
        .detail
        .get_text("daemon")
        .map(std::path::PathBuf::from)
    else {
        return refusal;
    };
    let Some(ours) = build.commit else {
        return refusal;
    };
    let Some(daemon) = HostDaemon::of(home).filter(|d| d.path == started) else {
        return refusal;
    };
    let said = match &daemon.stamp {
        Some((theirs, _)) if daemon.matches(build) == SdkValue::Bool(false) => format!(
            "The host machine's daemon {} was built from {}, this build from {}: \
             it cannot be expected to read what this build writes",
            daemon.path.display(),
            short(theirs),
            short(ours)
        ),
        Some(_) => return refusal,
        None if daemon.present => format!(
            "The host machine's daemon {} names no build: it is not this build ({}) \
             and may not read what this build writes",
            daemon.path.display(),
            short(ours)
        ),
        None => return refusal,
    };
    let fix = match own_daemon(build) {
        Some(path) => format!(
            "layerfs machine update {HOST_MACHINE} --daemon {}",
            crate::shell(&path)
        ),
        None => "layerfs home doctor".to_owned(),
    };
    let message = format!("{}. {said}", refusal.message);
    layerfs_home::reply::outcome::with_fix(SdkRefusal { message, ..refusal }, fix)
}

/// This build's own `layerfs-daemon`: the one beside the running program
/// (a runtime's `bin/`), else the default beside this host's bundle, the
/// first that is a file stamped with `build`'s commit. Read from files only.
pub fn own_daemon(build: Build<'_>) -> Option<std::path::PathBuf> {
    let ours = build.commit?;
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("layerfs-daemon")));
    let bundle = layerfs_machine::bundle_dir(std::env::consts::OS)
        .ok()
        .map(|dir| dir.join("layerfs-daemon"));
    [beside, bundle].into_iter().flatten().find(|path| {
        path.is_file()
            && std::fs::read(path)
                .ok()
                .and_then(|bytes| stamp(&bytes))
                .is_some_and(|(commit, _)| commit == ours)
    })
}

/// The host machine's daemon (macOS only; null elsewhere): the one its
/// record names, else the default beside the macOS bundle.
#[cfg(target_os = "macos")]
fn daemon_value(home: &Path, build: Build<'_>) -> SdkValue {
    let Some(daemon) = HostDaemon::of(home) else {
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
