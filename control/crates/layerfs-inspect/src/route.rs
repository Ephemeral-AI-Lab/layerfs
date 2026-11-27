//! The one inspect route (layerfs-project#178, #179 A): [`ask`] runs a kind
//! where a machine's run folders are, [`in_worker`] is the worker's
//! `inspect` operation, and [`serve`] is the one place a kind is matched.
use crate::Side;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::inspect::owner::Budget;

/// Runs inspect `kind` with `args` where machine `machine`'s run folders
/// are ([`layerfs_sandbox::inspect::route::ask`]), matched by [`serve`].
pub fn ask(home: &std::path::Path, machine: &str, kind: &'static str, args: SdkValue) -> Outcome {
    layerfs_sandbox::inspect::route::ask(home, machine, kind, args, serve)
}

/// The worker's `inspect` operation: one request under this process's
/// admission, on the worker's `root`, matched by [`serve`].
pub fn in_worker(root: &std::path::Path, args: SdkValue) -> Outcome {
    layerfs_sandbox::inspect::route::in_worker(root, args, serve)
}

/// One admitted request `{ kind, .. }` on `side`. A kind made of many
/// units of work (a folder walk) is handed `budget` and checks it between
/// them: a folder walk between entries (`machine_space`'s too),
/// `machine_stats` between Stores and between Sandboxes. The other kinds
/// each read one window or a few sizes.
pub fn serve(side: &Side<'_>, request: &SdkValue, budget: &Budget) -> Outcome {
    use crate::files::{log, walk};
    use crate::machine::{files as machine, space, stats};
    use layerfs_home::outcome::refused;
    use layerfs_sandbox::inspect::overlay;
    match request.get_text("kind").unwrap_or("") {
        log::MACHINE_LOG => log::machine(side, request),
        log::SANDBOX_LOG => log::sandbox(side, request),
        overlay::KIND => overlay::serve(side, request),
        walk::SANDBOX_FILES => walk::sandbox_page(side, request, &|| budget.spent()),
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
