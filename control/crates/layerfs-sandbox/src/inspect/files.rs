//! The Files walker (layerfs-project#178): one page of a folder or one
//! window of a file, read beneath a base folder and nowhere else. The
//! inspect kind `sandbox_files` is its first view, a Sandbox's home; a
//! view is a [`Scope`], and [`answer`] serves any of them.
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
//! Every open and `stat` is `layerfs_sandbox_backend::local::confined`'s:
//! no link is followed, no other mount entered, no special file read. A
//! mount point is told from the mount table and what LayerFS keeps private
//! from `roots::kept`, both before any `stat`.
use super::roots::{self, Kept};
use super::Side;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::with_field;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::tail;
use layerfs_sandbox_backend::local::confined::{self, Base, Kind, Opened, Refused, Stat, View};
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
    fn check(&self, call: &str) -> Result<(), SdkRefusal> {
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

    /// The inspect request for this read of Sandbox `sandbox`.
    fn request(&self, sandbox: &str) -> SdkValue {
        let mut fields = vec![("sandbox".to_owned(), SdkValue::text(sandbox))];
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
    let record = crate::family::sandbox_of(home, name)?;
    let mut args = request.request(name);
    // the Workspace records of a Sandbox whose daemon the home reaches
    // directly are here, and its run folder is in its machine: they go
    // with the request
    if request.path.is_none() && crate::direct::forwarded(&record) {
        args = with_field(args, "workspaces", roots::recorded(&record.dir(home))?);
    }
    super::ask(home, record.machine(), SANDBOX_FILES, args)
}

/// [`sandbox_files`] on the side that holds the Sandbox's run folder.
/// `spent` says when the request's soft budget is used up: a page then
/// answers the entries it has, with `has_more` and `reason: "deadline"`.
pub fn sandbox_page(side: &Side<'_>, args: &SdkValue, spent: &dyn Fn() -> bool) -> Outcome {
    let request = FilesRequest::of(args);
    request.check(CALL)?;
    let name = args.get_text("sandbox").unwrap_or("");
    let run = super::run_folder(side, name)?;
    let Some(path) = request.path.as_deref().map(Path::new) else {
        return roots::sandbox(side, name, &run, args.get("workspaces"));
    };
    let (Side::Home(root) | Side::Worker(root)) = side;
    let home = run.join(roots::HOME);
    let Ok(relative) = path.strip_prefix(&home) else {
        let why = match roots::kept(root, path) {
            Kept::Workspace => "a Workspace's files are read through its mount, by its commands",
            Kept::Log => "a log is read with `logs.sandbox`",
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
    let scope = Scope::open(CALL, root, &home, owner)?;
    answer(&scope, relative, &request, spent)
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
    /// The mount points under the base, each relative to it, with its
    /// filesystem type.
    mounts: Vec<(PathBuf, String)>,
    /// Whose files may have more than one name: a regular file with
    /// several names owned by anyone else is not read. `None` where the
    /// rule does not apply.
    owner: Option<u32>,
}

impl<'a> Scope<'a> {
    /// The view of folder `shown` beneath itself, on the side whose root
    /// is `root`.
    pub(super) fn open(
        call: &'static str,
        root: &'a Path,
        shown: &Path,
        owner: Option<u32>,
    ) -> Result<Self, SdkRefusal> {
        let base = Base::open(shown, View::Beneath).map_err(|why| refusal(call, shown, why))?;
        // the table names real paths, as the base's own descriptor does
        let real = base.path().map_err(failed)?;
        let mounts = (confined::mounts().map_err(failed)?.into_iter())
            .filter_map(|mount| {
                let under = mount.point.strip_prefix(&real).ok()?;
                (!under.as_os_str().is_empty()).then(|| (under.to_owned(), mount.fs))
            })
            .collect();
        Ok(Self {
            call,
            root,
            shown: shown.to_owned(),
            base,
            mounts,
            owner,
        })
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
        if let Some((_, fs)) = self.mounts.iter().find(|(point, _)| point == relative) {
            return Class::Mount(Some(fs.clone()));
        }
        let kept = roots::kept(self.root, &self.shown.join(relative));
        if kept == Kept::Workspace {
            return Class::Mount(None);
        }
        let stat = match folder.stat_of(name) {
            Ok(stat) => stat,
            Err(_) => return Class::Gone,
        };
        match kept {
            Kept::Restricted => Class::Restricted(stat),
            Kept::Log => Class::Log(stat),
            // on another device than its folder: a mount the table did not name
            _ if stat.device != folder.stat.device => Class::Mount(None),
            _ => Class::Entry(stat),
        }
    }

    /// Refuses a regular file with several names that its view's owner
    /// does not own: a link a command made to a file that is not its own.
    fn own_link(&self, path: &Path, stat: &Stat) -> Result<(), SdkRefusal> {
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
    /// A mount point, with its filesystem type when the table gave it.
    Mount(Option<String>),
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
        if scope.mounts.iter().any(|(point, _)| *point == above) {
            return Err(scope.invalid(&path, "is in a mount; a mount is listed, not read"));
        }
        if roots::kept(scope.root, &scope.shown.join(&above)) != Kept::Open {
            return Err(scope.invalid(&path, "is in a folder that is listed, not read"));
        }
    }
    if let (Some(parent), Some(name)) = (relative.parent(), relative.file_name()) {
        let folder = (scope.base.open_at(parent)).map_err(|why| refusal(scope.call, &path, why))?;
        match scope.class(&folder, relative, name) {
            Class::Mount(_) => {
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
    let opened = (scope.base.open_at(relative)).map_err(|why| refusal(scope.call, &path, why))?;
    match opened.stat.kind {
        Kind::Folder => page(scope, &opened, relative, request, spent),
        _ => {
            // what was opened, not what was listed before the open
            scope.own_link(&path, &opened.stat)?;
            window(&opened, request)
        }
    }
}

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
            );
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
                Class::Mount(fs) => {
                    let mut shown = entry(&path, level, "mount", None);
                    shown.extend(fs.clone().map(|fs| ("fs".to_owned(), SdkValue::Text(fs))));
                    (shown, false)
                }
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
                        stat.kind == Kind::Folder,
                    )
                }
            };
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
        match self.scope.base.open_at(relative) {
            Ok(folder) if folder.stat.kind == Kind::Folder => {
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
