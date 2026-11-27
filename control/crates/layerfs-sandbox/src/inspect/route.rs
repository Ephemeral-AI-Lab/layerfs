//! The inspect route: where a request runs ([`ask`]), the worker's side of
//! it ([`in_worker`]) and how a side finds a Sandbox's run folder
//! ([`run_folder`]). The one match on a request's kind ([`Serve`]) is
//! `layerfs-inspect`'s, passed in by the caller.
use super::owner::{admit, on_owner, Budget};
use crate::deploy::Side;
use crate::record::SandboxRecord;
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::place::{self, Executor};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// The worker operation every inspect request is sent as.
pub const OP: &str = "inspect";

/// One admitted request `{ kind, .. }` on a side, with its budget: the
/// match on its kind.
pub type Serve = fn(side: &Side<'_>, request: &SdkValue, budget: &Budget) -> Outcome;

/// Runs inspect `kind` with `args` where machine `machine`'s run folders
/// are: in this process under its own admission, or in the machine's
/// worker under the worker's, waited for at most `place::READ`. A machine
/// whose worker is not recorded or does not answer in time is
/// `Unavailable`; nothing starts it.
pub fn ask(
    home: &Path,
    machine: &str,
    kind: &'static str,
    args: SdkValue,
    serve: Serve,
) -> Outcome {
    let request = layerfs_home::home::with_field(args, "kind", SdkValue::text(kind));
    match on_owner(home, machine)? {
        Executor::Local => {
            let home = home.to_owned();
            admit(move |budget| serve(&Side::Home(&home), &request, budget))
        }
        Executor::Worker(mut worker) => worker.read(OP, request, place::READ),
    }
}

/// The worker's `inspect` operation: one request under this process's
/// admission, on the worker's `root`.
pub fn in_worker(root: &Path, args: SdkValue, serve: Serve) -> Outcome {
    let root = root.to_owned();
    admit(move |budget| serve(&Side::Worker(&root), &args, budget))
}

/// The run folder of Sandbox `name` on `side`: the `directory` of the
/// daemon record in `<root>/sandbox/<name>/record.json`, used only when it
/// is a folder directly under this side's run root, so a damaged or edited
/// record never points a read elsewhere. The record is read as a file: no
/// lock, no registry, no daemon. A side with no such record or folder
/// answers `NoSuchSandbox`, as a Sandbox's own calls do there.
pub fn run_folder(side: &Side<'_>, name: &str) -> Result<PathBuf, SdkRefusal> {
    layerfs_home::names::SandboxName::parse(name)?;
    let (Side::Home(root) | Side::Worker(root)) = side;
    let lost = || {
        refused(
            "NoSuchSandbox",
            format!("no Sandbox {name} runs on this machine"),
        )
    };
    let record = SandboxRecord::read_in(&paths::sandbox_dir(root, name), name)?.ok_or_else(lost)?;
    let directory = (record.daemon())
        .and_then(|daemon| daemon.directory())
        .map(PathBuf::from)
        .ok_or_else(lost)?;
    let named = matches!(
        directory.components().next_back(),
        Some(Component::Normal(_))
    );
    if !named || directory.parent() != Some(paths::run_root(root).as_path()) {
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {name}'s record names a run folder outside {}; it is not read",
                paths::run_root(root).display()
            ),
        ));
    }
    match std::fs::symlink_metadata(&directory) {
        Ok(found) if found.is_dir() => Ok(directory),
        Ok(_) => Err(lost()),
        Err(error) if error.kind() == ErrorKind::NotFound => Err(lost()),
        Err(error) => Err(failed(error)),
    }
}
