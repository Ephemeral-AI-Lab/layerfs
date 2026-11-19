//! The inspect route: where a request runs ([`ask`]), the worker's side of
//! it ([`in_worker`]), the one match on its kind ([`serve`]) and how a
//! side finds a Sandbox's run folder ([`run_folder`]).
use super::{admit, files, log, machine, on_owner, overlay, space, stats, Budget, Side};
use crate::record::SandboxRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_store::place::{self, Executor};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// The worker operation every inspect request is sent as.
pub const OP: &str = "inspect";

/// Runs inspect `kind` with `args` where machine `machine`'s run folders
/// are: in this process under its own admission, or in the machine's
/// worker under the worker's, waited for at most `place::READ`. A machine
/// whose worker is not recorded or does not answer in time is
/// `Unavailable`; nothing starts it.
pub fn ask(home: &Path, machine: &str, kind: &'static str, args: SdkValue) -> Outcome {
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
pub fn in_worker(root: &Path, args: SdkValue) -> Outcome {
    let root = root.to_owned();
    admit(move |budget| serve(&Side::Worker(&root), &args, budget))
}

/// One admitted request `{ kind, .. }` on `side`. A kind made of many
/// units of work (a folder walk) is handed `budget` and checks it between
/// them: a folder walk between entries (`machine_space`'s too),
/// `machine_stats` between Stores and between Sandboxes. The other kinds
/// each read one window or a few sizes.
pub fn serve(side: &Side<'_>, request: &SdkValue, budget: &Budget) -> Outcome {
    match request.get_text("kind").unwrap_or("") {
        log::MACHINE_LOG => log::machine(side, request),
        log::SANDBOX_LOG => log::sandbox(side, request),
        overlay::KIND => overlay::serve(side, request),
        files::SANDBOX_FILES => files::sandbox_page(side, request, &|| budget.spent()),
        machine::KIND => machine::machine_page(side, request, &|| budget.spent()),
        // layerfs-project#164
        stats::KIND => stats::serve(side, request, budget),
        space::KIND => space::serve(side, request, budget),
        other => Err(refused(
            "InvalidInput",
            format!("this build has no inspect kind `{other}`"),
        )),
    }
}

/// The run folder of Sandbox `name` on `side`: the `directory` of the
/// daemon record in `<root>/sandbox/<name>/record.json`, used only when it
/// is a folder directly under this side's run root, so a damaged or edited
/// record never points a read elsewhere. The record is read as a file: no
/// lock, no registry, no daemon. A side with no such record or folder
/// answers `NoSuchSandbox`, as a Sandbox's own calls do there.
pub(super) fn run_folder(side: &Side<'_>, name: &str) -> Result<PathBuf, SdkRefusal> {
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
