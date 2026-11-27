//! The Files walker (layerfs-project#178): one page of a folder or one
//! window of a file, read beneath a base folder and nowhere else. The
//! inspect kind `sandbox_files` is its first view, a Sandbox's home, and
//! `machine_files` (`machine.rs`) the views of a machine; a view is a
//! [`Scope`] with its [`Rules`], and [`answer`] serves any of them. A
//! namespaces Sandbox's whole root (`root.rs`) is the third.
//!
//! A folder page is a flat list in tree order: names in byte order, a
//! folder's entries after it, to `depth` levels. Its cursor is the path of
//! its last entry. A page reads the names of each folder on the cursor's
//! path once, keeps the smallest it has room for after the cursor, and
//! `stat`s only the entries it answers: a page costs the names of those
//! folders, `limit` `stat`s and `limit` names of memory, and a walk goes on
//! from its cursor whatever was added or removed meanwhile. A file window
//! is read with one `pread` of at most `length` bytes.
//!
//! Every open and `stat` is `layerfs_sandbox_backend::shared::confined`'s:
//! no link leads out of the base, no mount is entered but one the view
//! crosses, no special file is read. A mount point is told from the mount
//! table and what LayerFS keeps private from `roots::kept`, both before
//! any `stat`.
use super::roots::{self, Kept};
use layerfs_sandbox::deploy::Side;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::home::with_field;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::tail;
use layerfs_sandbox::backend::confined::{self, Base, Kind, Mount, Opened, Refused, Stat, View};
use std::cell::OnceCell;
use std::collections::BinaryHeap;
use std::ffi::OsStr;
use std::ops::ControlFlow;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// The call the Sandbox view serves.
pub(super) const CALL: &str = "sandbox.files";
/// The inspect kind of [`sandbox_files`].
pub(super) const SANDBOX_FILES: &str = "sandbox_files";

/// The levels a page walks when a call names none.
pub const DEPTH: i64 = 1;
/// The most levels a page walks.
pub const MAX_DEPTH: i64 = 32;
/// The entries a page answers when a call names no `limit`.
pub const ENTRIES: i64 = 500;
/// The most entries a page answers.
pub const MAX_ENTRIES: i64 = 1000;
/// The bytes a file window holds when a call names no `length`.
pub const WINDOW: i64 = 256 << 10;
/// The most bytes a file window holds.
pub const MAX_WINDOW: i64 = 1 << 20;

/// How many names a listing reads between two looks at its budget.
const NAMES_PER_LOOK: usize = 1024;

/// What a files call asks for: nothing (the roots), a folder page or a
/// file window.
#[derive(Clone, Debug, Default)]
pub struct FilesRequest {
    /// The folder or file, an absolute path; none for the roots.
    pub path: Option<String>,
    /// How many levels of a folder a page walks.
    pub depth: Option<i64>,
    /// A page's `next`: the entries after it.
    pub cursor: Option<String>,
    /// At most this many entries.
    pub limit: Option<i64>,
    /// Where a file window starts.
    pub offset: Option<i64>,
    /// At most this many bytes of a file.
    pub length: Option<i64>,
}

impl FilesRequest {
    /// The request `args` name.
    pub fn of(args: &SdkValue) -> Self {
        let text = |name: &str| args.get_text(name).map(str::to_owned);
        Self {
            path: text("path"),
            depth: args.get_int("depth"),
            cursor: text("cursor"),
            limit: args.get_int("limit"),
            offset: args.get_int("offset"),
            length: args.get_int("length"),
        }
    }

    /// Its refusal before any read: an option outside its range, a path
    /// that is not absolute or holds a `..`, or a cursor with no folder.
    /// An option that does not apply to what the path turns out to be (a
    /// `limit` for a file, an `offset` for a folder) is not used.
    pub(super) fn check(&self, call: &str) -> Result<(), SdkRefusal> {
        let invalid = |what: String| Err(refused("InvalidInput", format!("{call}: {what}")));
        let within = |name: &str, value: Option<i64>, least: i64, most: i64| match value {
            Some(value) if value < least || value > most => {
                invalid(format!("`{name}` is {least} to {most}"))
            }
            _ => Ok(()),
        };
        within("depth", self.depth, 1, MAX_DEPTH)?;
        within("limit", self.limit, 1, MAX_ENTRIES)?;
        within("length", self.length, 1, MAX_WINDOW)?;
        if self.offset.is_some_and(|offset| offset < 0) {
            return invalid("`offset` is 0 or more".to_owned());
        }
        for (name, path) in [("path", &self.path), ("cursor", &self.cursor)] {
            let Some(path) = path else { continue };
            let plain = (Path::new(path).components())
                .all(|part| matches!(part, Component::RootDir | Component::Normal(_)));
            if !path.starts_with('/') || !plain {
                return invalid(format!("`{name}` is an absolute path with no `..` in it"));
            }
        }
        if self.cursor.is_some() && self.path.is_none() {
            return invalid("`cursor` continues a folder: name its `path`".to_owned());
        }
        Ok(())
    }

    /// The inspect request for this read of what `field` names: the
    /// `sandbox` or the `machine` called `name`.
    pub(super) fn request(&self, field: &str, name: &str) -> SdkValue {
        let mut fields = vec![(field.to_owned(), SdkValue::text(name))];
        let texts = [("path", &self.path), ("cursor", &self.cursor)];
        for (name, value) in texts {
            fields.extend(value.clone().map(|v| (name.to_owned(), SdkValue::Text(v))));
        }
        let ints = [
            ("depth", self.depth),
            ("limit", self.limit),
            ("offset", self.offset),
            ("length", self.length),
        ];
        for (name, value) in ints {
            fields.extend(value.map(|v| (name.to_owned(), SdkValue::Int(v))));
        }
        SdkValue::Map(fields)
    }
}

/// `sandbox.files(sandbox, path?, { depth, cursor, limit, offset, length
/// })`: with no path, the roots of Sandbox `name` (its Workspaces, `home`
/// and `log`, each with a `role`); with a path under its home, one page of
/// that folder or one window of that file. It is read where the Sandbox's
/// run folder is, with no lock, lease, daemon or write. A Sandbox whose
/// daemon ended keeps its home readable while its run folder is there; a
/// machine whose worker does not answer is `Unavailable`.
pub fn sandbox_files(home: &Path, name: &str, request: &FilesRequest) -> Outcome {
    request.check(CALL)?;
    let record = layerfs_sandbox::family::sandbox_of(home, name)?;
    let mut args = request.request("sandbox", name);
    // the Workspace records of a Sandbox whose daemon the home reaches
    // directly are here, and its run folder is in its machine: they go
    // with the request, for the roots and for a listing that meets a
    // Workspace mount
    if layerfs_sandbox::direct::forwarded(&record) {
        args = with_field(args, "workspaces", roots::recorded(&record.dir(home))?);
    }
    layerfs_sandbox::inspect::route::ask(home, record.machine(), SANDBOX_FILES, args)
}

/// [`sandbox_files`] on the side that holds the Sandbox's run folder.
/// `spent` says when the request's soft budget is used up: a page then
/// answers the entries it has, with `has_more` and `reason: "deadline"`.
pub fn sandbox_page(side: &Side<'_>, args: &SdkValue, spent: &dyn Fn() -> bool) -> Outcome {
    let request = FilesRequest::of(args);
    request.check(CALL)?;
    let name = args.get_text("sandbox").unwrap_or("");
    let run = layerfs_sandbox::inspect::route::run_folder(side, name)?;
    let sent = args.get("workspaces");
    // a namespaces Sandbox has a root of its own, which is a second view
    let daemon = roots::daemon(side, name);
    let rooted = (daemon.as_ref()).filter(|daemon| daemon.runtime() == Some("namespaces"));
    let Some(path) = request.path.as_deref().map(Path::new) else {
        return roots::sandbox(side, name, &run, sent, rooted.is_some());
    };
    let (Side::Home(root) | Side::Worker(root)) = side;
    let home = run.join(roots::HOME);
    let Ok(relative) = path.strip_prefix(&home) else {
        if let Some(daemon) = rooted {
            let sandbox = super::root_view::Sandbox {
                name,
                run: &run,
                daemon,
            };
            return super::root_view::page(root, &sandbox, path, &request, sent, spent);
        }
        let why = match roots::kept(root, path) {
            Kept::Workspace => "a Workspace's files are read through its mount, by its commands",
            Kept::Log => "a log is read with `sandbox.log`",
            Kept::Restricted => "LayerFS keeps it private",
            Kept::Open => "only paths under the home are read",
        };
        return Err(refused(
            "InvalidInput",
            format!(
                "{CALL}: {} is outside Sandbox {name}'s home {}: {why}",
                path.display(),
                home.display()
            ),
        ));
    };
    let owner = Some(roots::command_uid(side, name));
    let scope = Scope::open(CALL, root, &home, owner, Rules::beneath())?;
    answer(&scope, relative, &request, spent)
}

/// The filesystem types whose mounts a view that crosses mounts enters:
/// local disks and memory, which answer without another host or process.
/// A mount of any other type (`fuse.*`, `nfs`, `cifs`, `9p`, `virtiofs`,
/// `fakeowner`, `sshfs`, one this list does not know) is listed as a
/// `mount` with its type and never entered.
const CROSSED: [&str; 7] = ["ext4", "xfs", "btrfs", "zfs", "f2fs", "overlay", "tmpfs"];

/// What a view judges beyond what LayerFS keeps private.
pub(super) struct Rules {
    /// How an open resolves a path from the base, and whether it may end
    /// in another mount.
    pub(super) view: View,
    /// The entries of the base that are the kernel's own (`proc`, `sys`):
    /// listed as `system`, never `stat`ed, entered or read.
    pub(super) system: &'static [&'static str],
    /// Only what every user of the machine may read is opened: a file
    /// others may read, a folder others may list and search. Anything
    /// else is `unreadable`: listed, never opened.
    pub(super) world: bool,
    /// The LayerFS folders in the base, each relative to it, with its
    /// role: an entry that is one is listed with it.
    pub(super) roles: Vec<(PathBuf, &'static str)>,
    /// Those folders are read as bases of their own: this view lists
    /// them and enters none.
    pub(super) apart: bool,
}

impl Rules {
    /// A folder read beneath itself, with no rule of its own.
    pub(super) fn beneath() -> Self {
        Self {
            view: View::Beneath,
            system: &[],
            world: false,
            roles: Vec::new(),
            apart: false,
        }
    }

    /// Whether `relative` is one of the base's system entries.
    fn system(&self, relative: &Path) -> bool {
        let mut names = relative.components();
        match (names.next(), names.next()) {
            (Some(Component::Normal(name)), None) => {
                self.system.iter().any(|system| name == *system)
            }
            _ => false,
        }
    }

    /// The role of the LayerFS folder at `relative`, if it is one.
    fn role(&self, relative: &Path) -> Option<&'static str> {
        let found = self.roles.iter().find(|(folder, _)| folder == relative);
        found.map(|(_, role)| *role)
    }

    /// Whether this view may open `stat`'s entry at all.
    fn opens(&self, stat: &Stat) -> bool {
        !self.world
            || match stat.kind {
                Kind::File => stat.mode & 0o004 != 0,
                Kind::Folder => stat.mode & 0o005 == 0o005,
                _ => true,
            }
    }
}

/// One mount point under a view's base.
struct Point {
    /// Where, relative to the base.
    at: PathBuf,
    /// Its filesystem type.
    fs: String,
    /// The view enters it: its entries are listed and read as its
    /// folder's are.
    crossed: bool,
}

/// One view: a base folder, how its paths are shown, and what is judged
/// in it. Another view (a machine's folders, a Sandbox's whole root) is
/// another `Scope`.
pub(super) struct Scope<'a> {
    /// The call, for its refusals.
    call: &'static str,
    /// The root of the side that serves it, which `roots::kept` judges
    /// paths against.
    root: &'a Path,
    /// The base as paths name it: an entry's `path` is this joined with
    /// its path under the base.
    shown: PathBuf,
    base: Base,
    /// The mount points under the base: at each, the mount a path
    /// reaches.
    mounts: Vec<Point>,
    /// Whose files may have more than one name: a regular file with
    /// several names owned by anyone else is not read. `None` where the
    /// rule does not apply.
    owner: Option<u32>,
    /// Where that rule applies: under this folder only. `None`: in the
    /// whole view.
    owned: Option<PathBuf>,
    rules: Rules,
    /// The Workspace records a request carried, `[{ id, path }]`.
    sent: Option<&'a SdkValue>,
    /// Each recorded Workspace's mount path and id, read when a listing
    /// first meets a Workspace mount.
    workspaces: OnceCell<Vec<(String, String)>>,
}

impl<'a> Scope<'a> {
    /// The view of folder `shown` under `rules`, on the side whose root
    /// is `root`, with this process's mount table.
    pub(super) fn open(
        call: &'static str,
        root: &'a Path,
        shown: &Path,
        owner: Option<u32>,
        rules: Rules,
    ) -> Result<Self, SdkRefusal> {
        let base = Base::open(shown, rules.view).map_err(|why| refusal(call, shown, why))?;
        // the table names real paths, as the base's own descriptor does
        let real = base.path().map_err(failed)?;
        let table = confined::mounts().map_err(failed)?;
        Ok(Self::over(
            call, root, shown, base, &real, table, owner, rules,
        ))
    }

    /// The view of `base`, shown as `shown`, whose mount namespace's
    /// `table` names it `real`. A view that crosses mounts
    /// (`View::InRoot`) enters a mount only when its type is one of
    /// [`CROSSED`], it is not in a system entry, a LayerFS folder read
    /// apart, a path LayerFS keeps or a mount that is not entered itself;
    /// `base` is told each such mount's id, and refuses what it opens in
    /// any other.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn over(
        call: &'static str,
        root: &'a Path,
        shown: &Path,
        mut base: Base,
        real: &Path,
        table: Vec<Mount>,
        owner: Option<u32>,
        rules: Rules,
    ) -> Self {
        // at one point, the last mount of the table is the one reached
        let mut tops: Vec<(PathBuf, Option<u64>, String)> = Vec::new();
        for mount in table {
            let Ok(under) = mount.point.strip_prefix(real) else {
                continue;
            };
            if !under.as_os_str().is_empty() {
                tops.retain(|(point, ..)| point != under);
                tops.push((under.to_owned(), mount.id, mount.fs));
            }
        }
        // a mount point before the mounts under it
        tops.sort_by_key(|(point, ..)| point.components().count());
        let mut mounts: Vec<Point> = Vec::new();
        for (at, id, fs) in tops {
            let first: PathBuf = at.components().take(1).collect();
            let apart = rules.apart && rules.roles.iter().any(|(role, _)| at.starts_with(role));
            let crossed = rules.view == View::InRoot
                && CROSSED.contains(&fs.as_str())
                && !rules.system(&first)
                && !apart
                && roots::kept(root, &shown.join(&at)) == Kept::Open
                && !(mounts.iter()).any(|above| !above.crossed && at.starts_with(&above.at));
            if crossed {
                base.cross(id);
            }
            mounts.push(Point { at, fs, crossed });
        }
        Self {
            call,
            root,
            shown: shown.to_owned(),
            base,
            mounts,
            owner,
            owned: None,
            rules,
            sent: None,
            workspaces: OnceCell::new(),
        }
    }

    /// Takes the Workspace records a request carried from the side that
    /// keeps them (`[{ id, path }]`), beside this side's own.
    pub(super) fn with_workspaces(mut self, sent: Option<&'a SdkValue>) -> Self {
        self.sent = sent;
        self
    }

    /// Keeps the hard-link rule to what is under `folder` (a Sandbox's
    /// run folder in its root, where an image's files keep their links).
    pub(super) fn owning(mut self, folder: &Path) -> Self {
        self.owned = Some(folder.to_owned());
        self
    }

    /// The handle of the Workspace mounted at `path` (its id, from a
    /// caller of a build before handles), from the Workspace records: those
    /// the request carried and those under this side's root, read once and
    /// only when a listing meets a Workspace mount.
    fn workspace(&self, path: &Path) -> Option<String> {
        let recorded = self.workspaces.get_or_init(|| {
            let pair = |item: &SdkValue| {
                let handle = item.get_text("workspace").or(item.get_text("id"))?;
                Some((item.get_text("path")?.to_owned(), handle.to_owned()))
            };
            let mut all: Vec<(String, String)> = match self.sent {
                Some(SdkValue::List(items)) => items.iter().filter_map(pair).collect(),
                _ => Vec::new(),
            };
            if let SdkValue::List(items) = roots::recorded_under(self.root) {
                all.extend(items.iter().filter_map(pair));
            }
            all
        });
        let path = path.to_str()?;
        let found = recorded.iter().find(|(mounted, _)| mounted == path);
        found.map(|(_, id)| id.clone())
    }

    fn invalid(&self, path: &Path, why: &str) -> SdkRefusal {
        refused(
            "InvalidInput",
            format!("{}: {} {why}", self.call, path.display()),
        )
    }

    /// What entry `name` of `folder` is, the entry at `relative` under the
    /// base. A mount point, a Workspace mount and what LayerFS keeps
    /// private are told from the path, before any `stat`; a mount point
    /// and a Workspace mount are never `stat`ed at all.
    fn class(&self, folder: &Opened, relative: &Path, name: &OsStr) -> Class {
        if self.rules.system(relative) {
            return Class::System;
        }
        let shown = self.shown.join(relative);
        let kept = roots::kept(self.root, &shown);
        let point = self.mounts.iter().find(|point| point.at == relative);
        if let Some(point) = point.filter(|point| !point.crossed) {
            // a Workspace mount the table names keeps its id
            let id = (kept == Kept::Workspace).then(|| self.workspace(&shown));
            return Class::Mount(Some(point.fs.clone()), id.flatten());
        }
        if kept == Kept::Workspace {
            return Class::Mount(None, self.workspace(&shown));
        }
        let stat = match folder.stat_of(name) {
            Ok(stat) => stat,
            Err(_) => return Class::Gone,
        };
        match kept {
            Kept::Restricted => Class::Restricted(stat),
            Kept::Log => Class::Log(stat),
            // in another mount than its folder, and the table did not name it
            _ if point.is_none() && folder.stat.apart_from(&stat) => Class::Mount(None, None),
            // a LayerFS folder read apart is judged by its own view
            _ if !self.rules.opens(&stat) && self.enters(relative) => Class::Unreadable(stat),
            _ => Class::Entry(stat),
        }
    }

    /// Opens `relative`, shown as `path`, and judges what was opened: in
    /// a view that resolves links (`View::InRoot`) a path can end where
    /// its own names do not say, so where the kernel says it ended is
    /// asked of `roots::kept` too, for the entry and every folder above
    /// it. Answers what was opened and its path as the kernel names it.
    fn opened(&self, relative: &Path, path: &Path) -> Result<(Opened, PathBuf), SdkRefusal> {
        let opened = (self.base.open_at(relative)).map_err(|why| refusal(self.call, path, why))?;
        if self.rules.view != View::InRoot {
            return Ok((opened, path.to_owned()));
        }
        // the base is shown as `/`: a path inside it is its own name
        let real = match opened.path() {
            Ok(real) if real.is_absolute() => real,
            _ => return Ok((opened, path.to_owned())),
        };
        if real != path && (real.ancestors()).any(|part| roots::kept(self.root, part) != Kept::Open)
        {
            return Err(self.invalid(
                path,
                "leads to what LayerFS keeps private under another name; it is not read",
            ));
        }
        Ok((opened, real))
    }

    /// Whether a walk enters the folder at `relative`: not a LayerFS
    /// folder this view lists as a base of its own.
    fn enters(&self, relative: &Path) -> bool {
        !(self.rules.apart && self.rules.role(relative).is_some())
    }

    /// Refuses a regular file with several names that its view's owner
    /// does not own: a link a command made to a file that is not its own.
    fn own_link(&self, path: &Path, stat: &Stat) -> Result<(), SdkRefusal> {
        if (self.owned.as_ref()).is_some_and(|folder| !path.starts_with(folder)) {
            return Ok(());
        }
        match self.owner {
            Some(owner) if stat.kind == Kind::File && stat.links > 1 && stat.owner != owner => {
                Err(self.invalid(
                    path,
                    "is a hard link to a file the Sandbox's commands do not own; it is not read",
                ))
            }
            _ => Ok(()),
        }
    }
}

/// What a listing found an entry to be.
enum Class {
    /// The kernel's own (`/proc`, `/sys`): never `stat`ed.
    System,
    /// A mount point that is not entered, with its filesystem type when
    /// the table gave it and, for a Workspace's, the Workspace's id.
    Mount(Option<String>, Option<String>),
    /// Not for every user of the machine to read.
    Unreadable(Stat),
    /// Kept private by LayerFS.
    Restricted(Stat),
    /// A log a `logs` call reads.
    Log(Stat),
    /// Anything else, by its `stat`.
    Entry(Stat),
    /// Gone since its folder was listed.
    Gone,
}

/// The fields every entry has: its `path`, its `depth` below the folder
/// asked for, its `kind`, and from its `stat`, when it was `stat`ed, a
/// regular file's `size`, its `mode` (permission bits) and its `mtime`
/// (milliseconds since 1970); null otherwise.
pub(super) fn entry(
    path: &str,
    depth: usize,
    kind: &str,
    stat: Option<&Stat>,
) -> Vec<(String, SdkValue)> {
    let int = |value: Option<i64>| value.map_or(SdkValue::Null, SdkValue::Int);
    let size = stat.filter(|stat| stat.kind == Kind::File);
    [
        ("path", SdkValue::text(path)),
        ("depth", int(i64::try_from(depth).ok())),
        ("kind", SdkValue::text(kind)),
        ("size", int(size.and_then(|s| i64::try_from(s.size).ok()))),
        ("mode", int(stat.map(|stat| i64::from(stat.mode)))),
        ("mtime", int(stat.map(|stat| stat.mtime_ms))),
    ]
    .map(|(name, value)| (name.to_owned(), value))
    .into()
}

/// The refusal of a confined open or `stat` of `path` that answered
/// nothing.
pub(super) fn refusal(call: &str, path: &Path, why: Refused) -> SdkRefusal {
    let shown = path.display();
    let invalid = |what: &str| refused("InvalidInput", format!("{call}: {shown} {what}"));
    match why {
        Refused::Missing => refused("NotFound", format!("{call}: nothing is at {shown}")),
        Refused::Symlink => {
            invalid("is, or is reached through, a symbolic link; links are not followed")
        }
        Refused::Outside => invalid(
            "leaves the folder it is read in (another mount, or a folder moved meanwhile); it is not read",
        ),
        Refused::Mount => invalid("is in a mount this view does not enter; it is not read"),
        Refused::Denied => invalid(
            "may not be opened: the system refuses it (its permissions, or a link another user made in a shared folder); it is not read",
        ),
        Refused::Special(kind) => invalid(&format!(
            "is {}; only files and folders are read",
            special(kind)
        )),
        Refused::Unsupported(reason) => refused("Unsupported", format!("{call}: {reason}")),
        Refused::Failed(error) => failed(format!("{call}: {shown}: {error}")),
    }
}

fn special(kind: Kind) -> &'static str {
    match kind {
        Kind::Fifo => "a FIFO",
        Kind::Socket => "a socket",
        Kind::Device => "a device",
        Kind::Symlink => "a symbolic link",
        Kind::File | Kind::Folder | Kind::Other => "not a file or a folder",
    }
}

/// One page of the folder, or one window of the file, at `relative` under
/// `scope`'s base (empty for the base itself). The path and every folder
/// above it are judged as a listing judges them, and the entry itself is
/// `stat`ed in its folder first, so what is refused is refused before it
/// is opened.
pub(super) fn answer(
    scope: &Scope<'_>,
    relative: &Path,
    request: &FilesRequest,
    spent: &dyn Fn() -> bool,
) -> Outcome {
    let path = scope.shown.join(relative);
    let mut above = PathBuf::new();
    for name in relative.parent().into_iter().flat_map(Path::components) {
        above.push(name);
        if scope.rules.system(&above) {
            return Err(scope.invalid(&path, "is in a system folder; it is listed, not read"));
        }
        if (scope.mounts.iter()).any(|point| point.at == above && !point.crossed) {
            return Err(scope.invalid(&path, "is in a mount; a mount is listed, not read"));
        }
        if roots::kept(scope.root, &scope.shown.join(&above)) != Kept::Open {
            return Err(scope.invalid(&path, "is in a folder that is listed, not read"));
        }
        if scope.rules.world {
            let folder =
                (scope.base.enter(&above)).map_err(|why| refusal(scope.call, &path, why))?;
            if !scope.rules.opens(&folder.stat) {
                return Err(scope.invalid(&path, NOT_FOR_ALL_ABOVE));
            }
        }
    }
    if let (Some(parent), Some(name)) = (relative.parent(), relative.file_name()) {
        let folder = (scope.base.enter(parent)).map_err(|why| refusal(scope.call, &path, why))?;
        match scope.class(&folder, relative, name) {
            Class::System => {
                return Err(scope.invalid(&path, "is a system folder; it is listed, not read"))
            }
            Class::Unreadable(_) => return Err(scope.invalid(&path, NOT_FOR_ALL)),
            Class::Mount(..) => {
                return Err(scope.invalid(&path, "is a mount point; a mount is listed, not read"))
            }
            Class::Restricted(_) => {
                return Err(scope.invalid(&path, "is kept private by LayerFS; it is not read"))
            }
            Class::Log(_) => {
                return Err(scope.invalid(&path, "is a log; read it with the `logs` calls"))
            }
            Class::Gone => return Err(refusal(scope.call, &path, Refused::Missing)),
            Class::Entry(stat) => match stat.kind {
                Kind::Folder => {}
                Kind::File => scope.own_link(&path, &stat)?,
                Kind::Symlink => return Err(refusal(scope.call, &path, Refused::Symlink)),
                other => return Err(refusal(scope.call, &path, Refused::Special(other))),
            },
        }
    }
    let (opened, real) = scope.opened(relative, &path)?;
    // what was opened, not what was listed before the open
    if !scope.rules.opens(&opened.stat) {
        return Err(scope.invalid(&path, NOT_FOR_ALL));
    }
    match opened.stat.kind {
        Kind::Folder => page(scope, &opened, relative, request, spent),
        _ => {
            // what was opened, not what was listed before the open
            scope.own_link(&real, &opened.stat)?;
            window(&opened, request)
        }
    }
}

const NOT_FOR_ALL: &str = "is not world-readable; it is listed, not read";
const NOT_FOR_ALL_ABOVE: &str = "is in a folder that is not world-readable; it is not read";

/// One window of `file`: `{ kind: "file", size, offset, content, binary,
/// next }`. `content` is the window as text: what is not UTF-8 is U+FFFD
/// and every control character but a line break and a tab is written as
/// its escape, as a log line's are. A window that ends inside a character
/// ends before it. `binary` says the window holds a NUL byte; its
/// `content` is then empty. `next` is the offset of the window after this
/// one, null at the end of the file.
fn window(file: &Opened, request: &FilesRequest) -> Outcome {
    let size = file.stat.size;
    let offset = u64::try_from(request.offset.unwrap_or(0)).unwrap_or(0);
    let length = u64::try_from(request.length.unwrap_or(WINDOW)).unwrap_or(0);
    // never more than the file held when it was opened
    let wanted = length.min(size.saturating_sub(offset));
    let mut bytes = vec![0u8; usize::try_from(wanted).unwrap_or(0)];
    let mut read = 0;
    while read < bytes.len() {
        match file.read_at(&mut bytes[read..], offset + read as u64) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(failed(error)),
        }
    }
    bytes.truncate(read);
    let binary = bytes.contains(&0);
    let (content, taken) = match binary {
        true => (String::new(), read),
        false => {
            let ends = offset + read as u64 >= size;
            let whole = match tail::whole_characters(&bytes) {
                cut if !ends && cut > 0 => cut,
                _ => read,
            };
            (tail::escaped(&bytes[..whole], &['\n', '\t']), whole)
        }
    };
    let end = offset + taken as u64;
    let int = |value: u64| i64::try_from(value).map_or(SdkValue::Null, SdkValue::Int);
    Ok(SdkValue::map([
        ("kind", SdkValue::text("file")),
        ("size", int(size)),
        ("offset", int(offset)),
        ("content", SdkValue::Text(content)),
        ("binary", SdkValue::Bool(binary)),
        ("next", if end < size { int(end) } else { SdkValue::Null }),
    ]))
}

/// One page of `folder`, the folder at `relative` under `scope`'s base:
/// `{ kind: "folder", entries, next, has_more, reason? }`.
fn page(
    scope: &Scope<'_>,
    folder: &Opened,
    relative: &Path,
    request: &FilesRequest,
    spent: &dyn Fn() -> bool,
) -> Outcome {
    let depth = usize::try_from(request.depth.unwrap_or(DEPTH)).unwrap_or(1);
    let limit = usize::try_from(request.limit.unwrap_or(ENTRIES)).unwrap_or(1);
    let listed = scope.shown.join(relative);
    // the cursor's names below the folder, one for each level it is down
    let after: Vec<&OsStr> = match &request.cursor {
        None => Vec::new(),
        Some(cursor) => (Path::new(cursor).strip_prefix(&listed).ok())
            .map(|below| below.iter().collect::<Vec<_>>())
            .filter(|names| !names.is_empty() && names.len() <= depth)
            .ok_or_else(|| {
                refused(
                    "InvalidInput",
                    format!(
                        "{}: `cursor` is the path of an entry of {}, at most `depth` levels down",
                        scope.call,
                        listed.display()
                    ),
                )
            })?,
    };
    let mut walk = Walk {
        scope,
        depth,
        limit,
        spent,
        entries: Vec::new(),
        last: None,
        stopped: None,
    };
    let _ = walk.folder(folder, relative, 1, &after);
    let next = (walk.last).or_else(|| request.cursor.clone());
    let mut answer = vec![
        ("kind".to_owned(), SdkValue::text("folder")),
        ("entries".to_owned(), SdkValue::List(walk.entries)),
        ("next".to_owned(), SdkValue::opt_text(next)),
        (
            "has_more".to_owned(),
            SdkValue::Bool(walk.stopped.is_some()),
        ),
    ];
    answer.extend((walk.stopped).map(|reason| ("reason".to_owned(), SdkValue::text(reason))));
    Ok(SdkValue::Map(answer))
}

/// One page in the making.
struct Walk<'a> {
    scope: &'a Scope<'a>,
    depth: usize,
    limit: usize,
    spent: &'a dyn Fn() -> bool,
    entries: Vec<SdkValue>,
    /// The path of the last entry answered.
    last: Option<String>,
    /// Why the page ended before the walk did: `limit` or `deadline`.
    stopped: Option<&'static str>,
}

impl Walk<'_> {
    /// Walks `folder` (at `relative` under the base), whose entries are
    /// `level` levels below the folder asked for. `after` is what is left
    /// of the cursor here: its first name is the entry of this folder the
    /// walk goes on after, and the rest lead on inside that entry.
    fn folder(
        &mut self,
        folder: &Opened,
        relative: &Path,
        level: usize,
        after: &[&OsStr],
    ) -> ControlFlow<()> {
        let bound = after.first().copied();
        if let Some(name) = bound {
            // first what is left inside the cursor's own entry; one that
            // is gone, or no longer a folder, has nothing left
            let inside = relative.join(name);
            let entered = matches!(
                self.scope.class(folder, &inside, name),
                Class::Entry(stat) if stat.kind == Kind::Folder
            ) && self.scope.enters(&inside);
            if entered && level < self.depth {
                self.enter(&inside, level + 1, &after[1..])?;
            }
        }
        // one more name than there is room for says whether more follow
        let room = self.limit - self.entries.len() + 1;
        let Some(names) = self.names(folder, bound, room) else {
            return self.stop("deadline");
        };
        for name in names {
            if self.entries.len() == self.limit {
                return self.stop("limit");
            }
            if (self.spent)() {
                return self.stop("deadline");
            }
            let inside = relative.join(&name);
            let path = self
                .scope
                .shown
                .join(&inside)
                .to_string_lossy()
                .into_owned();
            let class = self.scope.class(folder, &inside, OsStr::new(&name));
            let (mut shown, entered) = match &class {
                Class::Gone => continue,
                Class::System => (entry(&path, level, "system", None), false),
                Class::Mount(fs, workspace) => {
                    let mut shown = entry(&path, level, "mount", None);
                    let text = |value: &Option<String>| value.clone().map(SdkValue::Text);
                    shown.extend(text(fs).map(|fs| ("fs".to_owned(), fs)));
                    shown.extend(text(workspace).map(|id| ("workspace".to_owned(), id)));
                    (shown, false)
                }
                Class::Unreadable(stat) => (entry(&path, level, "unreadable", Some(stat)), false),
                Class::Restricted(stat) => (entry(&path, level, "restricted", Some(stat)), false),
                Class::Log(stat) => (entry(&path, level, "file", Some(stat)), false),
                Class::Entry(stat) => {
                    let kind = match stat.kind {
                        Kind::File => "file",
                        Kind::Folder => "folder",
                        Kind::Symlink => "symlink",
                        _ => "other",
                    };
                    (
                        entry(&path, level, kind, Some(stat)),
                        stat.kind == Kind::Folder && self.scope.enters(&inside),
                    )
                }
            };
            if let Some(role) = self.scope.rules.role(&inside) {
                shown.push(("role".to_owned(), SdkValue::text(role)));
            }
            match &class {
                Class::Log(_) => shown.push(("role".to_owned(), SdkValue::text("log"))),
                Class::Entry(stat) if stat.kind == Kind::Symlink => {
                    let target = folder.target_of(OsStr::new(&name)).ok();
                    let target = target.map(|target| target.to_string_lossy().into_owned());
                    shown.push(("target".to_owned(), SdkValue::opt_text(target)));
                }
                _ => {}
            }
            self.entries.push(SdkValue::Map(shown));
            self.last = Some(path);
            if entered && level < self.depth {
                self.enter(&inside, level + 1, &[])?;
            }
        }
        ControlFlow::Continue(())
    }

    /// Walks the folder at `relative`, if it opens as one.
    fn enter(&mut self, relative: &Path, level: usize, after: &[&OsStr]) -> ControlFlow<()> {
        let path = self.scope.shown.join(relative);
        match (self.scope.opened(relative, &path)).map(|(folder, _)| folder) {
            Ok(folder)
                if folder.stat.kind == Kind::Folder && self.scope.rules.opens(&folder.stat) =>
            {
                self.folder(&folder, relative, level, after)
            }
            // gone, replaced, or not to be opened: it has no entries here
            _ => ControlFlow::Continue(()),
        }
    }

    fn stop(&mut self, reason: &'static str) -> ControlFlow<()> {
        self.stopped = Some(reason);
        ControlFlow::Break(())
    }

    /// The `room` smallest names of `folder` after `bound`, in byte order,
    /// from one reading of its names; `None` when the budget ran out
    /// before the reading ended. A folder that cannot be listed has no
    /// names. A name that is not UTF-8 is left out: a path is text in
    /// every front end, and such a name could not be asked for again.
    fn names(&self, folder: &Opened, bound: Option<&OsStr>, room: usize) -> Option<Vec<String>> {
        let mut smallest: BinaryHeap<Vec<u8>> = BinaryHeap::new();
        let (mut read, mut late) = (0usize, false);
        let bound = bound.map(OsStr::as_bytes);
        let listed = folder.names(&mut |name| {
            read += 1;
            if read % NAMES_PER_LOOK == 0 && (self.spent)() {
                late = true;
                return false;
            }
            if bound.is_some_and(|bound| name <= bound) || std::str::from_utf8(name).is_err() {
                return true;
            }
            if smallest.len() < room {
                smallest.push(name.to_vec());
            } else if smallest
                .peek()
                .is_some_and(|largest| name < largest.as_slice())
            {
                smallest.pop();
                smallest.push(name.to_vec());
            }
            true
        });
        if late {
            return None;
        }
        if listed.is_err() {
            return Some(Vec::new());
        }
        Some(
            (smallest.into_sorted_vec().into_iter())
                .filter_map(|name| String::from_utf8(name).ok())
                .collect(),
        )
    }
}
