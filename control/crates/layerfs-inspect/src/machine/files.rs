//! `machine.files` and its inspect kind `machine_files`
//! (layerfs-project#178 step 4): a machine's files, read where they are.
//!
//! With no path the answer is the machine's roots (`roots::machine`): its
//! LayerFS folders ranked by role, then `/` on a machine that is LayerFS's
//! own (docker, Lima, Apple Container). A path is read in one of two
//! views:
//!
//! - in a LayerFS folder, beneath that folder and nowhere else, as a
//!   Sandbox's home is: no link is followed, no mount entered, and what
//!   LayerFS keeps private (`roots::kept`) is `restricted`;
//! - anywhere else, from `/` on a worker's side: links resolve inside the
//!   machine's root, `/proc`, `/sys`, `/dev` and `/run` are `system`, a
//!   mount is entered only when its type is a local disk's or memory's,
//!   and only what every user of the machine may read is opened; the rest
//!   is `unreadable`. A LayerFS folder is listed there with its role and
//!   read by its own path. In a home (`native`) there is no such view: the
//!   machine is its owner's own computer, and a path outside the LayerFS
//!   folders is `InvalidInput`.
use super::super::files::walk::{answer, FilesRequest, Rules, Scope};
use super::super::files::roots;
use layerfs_sandbox::deploy::Side;
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_sandbox::backend::confined::View;
use std::path::{Path, PathBuf};

/// The call this view serves.
const CALL: &str = "machine.files";
/// The inspect kind of [`machine_files`].
pub(super) const KIND: &str = "machine_files";
/// The entries of a machine's `/` that are the kernel's own.
const SYSTEM: [&str; 4] = ["proc", "sys", "dev", "run"];

/// `machine.files(machine, path?, { depth, cursor, limit, offset, length
/// })`: with no path, the roots of machine `machine` (its LayerFS folders,
/// each with a `role`, then `/`); with a path, one page of that folder or
/// one window of that file. It is read where the machine's data is, with
/// no lock, lease, daemon or write. A machine that is stopped or whose
/// worker does not answer is `Unavailable`; nothing starts it.
pub fn machine_files(home: &Path, machine: &MachineName, request: &FilesRequest) -> Outcome {
    request.check(CALL)?;
    let name = machine.as_str();
    let mut args = request.request("machine", name);
    // a folder listing names a Workspace mount by its record, and the
    // records of a Sandbox whose daemon the home reaches directly are
    // here: they go with the request
    if request.path.is_some() {
        let mut sent = Vec::new();
        for sandbox in layerfs_sandbox::record::sandboxes(home)? {
            if sandbox.machine() == name && layerfs_sandbox::direct::forwarded(&sandbox) {
                if let SdkValue::List(items) = roots::recorded(&sandbox.dir(home))? {
                    sent.extend(items);
                }
            }
        }
        if !sent.is_empty() {
            sent.truncate(roots::MOST);
            args = with_field(args, "workspaces", SdkValue::List(sent));
        }
    }
    layerfs_sandbox::inspect::route::ask(home, name, KIND, args)
}

/// [`machine_files`] on the side that holds the machine's data. `spent`
/// says when the request's soft budget is used up: a page then answers
/// the entries it has, with `has_more` and `reason: "deadline"`.
pub fn machine_page(side: &Side<'_>, args: &SdkValue, spent: &dyn Fn() -> bool) -> Outcome {
    let request = FilesRequest::of(args);
    request.check(CALL)?;
    let Some(path) = request.path.as_deref().map(Path::new) else {
        return roots::machine(side);
    };
    let (Side::Home(root) | Side::Worker(root)) = side;
    let folders = roots::folders(side);
    let sent = args.get("workspaces");
    // the outermost LayerFS folder that holds the path is its base
    if let Some((folder, _)) = folders.iter().find(|(folder, _)| path.starts_with(folder)) {
        let rules = Rules {
            roles: under(&folders, folder),
            ..Rules::beneath()
        };
        let scope = Scope::open(CALL, root, folder, None, rules)?.with_workspaces(sent);
        let relative = path.strip_prefix(folder).unwrap_or(Path::new(""));
        return answer(&scope, relative, &request, spent);
    }
    if let Side::Home(_) = side {
        return Err(refused(
            "InvalidInput",
            format!(
                "{CALL}: {} is outside this machine's LayerFS folders; on `native`, which is your own computer, only they are read",
                path.display()
            ),
        ));
    }
    let slash = Path::new("/");
    let rules = Rules {
        view: View::InRoot,
        system: &SYSTEM,
        world: true,
        roles: under(&folders, slash),
        apart: true,
    };
    let scope = Scope::open(CALL, root, slash, None, rules)?.with_workspaces(sent);
    let relative = path.strip_prefix(slash).unwrap_or(Path::new(""));
    answer(&scope, relative, &request, spent)
}

/// The LayerFS folders strictly inside `base`, each relative to it.
fn under(folders: &[(PathBuf, &'static str)], base: &Path) -> Vec<(PathBuf, &'static str)> {
    (folders.iter())
        .filter_map(|(folder, role)| {
            let inside = folder.strip_prefix(base).ok()?;
            (!inside.as_os_str().is_empty()).then(|| (inside.to_owned(), *role))
        })
        .collect()
}
