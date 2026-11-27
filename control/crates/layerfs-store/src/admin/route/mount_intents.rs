//! Placed mount intents a Store's admission compares (beside
//! [`super::place_view`]): ending the one a stopped Sandbox's Workspace left
//! and settling those of a machine incarnation proven dead.
use crate::admin::open::args::OpenArgs;
use crate::admin::open::service::with_service;
use layerfs_home::outcome::failed;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

/// Ends the mount intent Workspace `workspace` of Branch `branch` left in
/// the Store `args` names, if one remains: a forced `sandbox.remove` calls
/// it once the Sandbox whose daemon owned the intent is stopped, so the
/// intent does not refuse every later compaction (RC-B F1). Each one left
/// is released once, or marked uncertain when the Store refuses the delete
/// (`release_or_mark`, the daemon's own unmount release); answers how many
/// there were. Nothing is retried.
pub fn end_mount(
    args: &OpenArgs,
    branch: layerfs_history::BranchId,
    workspace: [u8; 32],
) -> Result<usize, SdkRefusal> {
    with_service(args, |service| {
        let catalog = service.store.history();
        let scope = layerfs_history::GcScopeKind::Branch;
        let intents = catalog
            .gc_intents(scope, branch.as_slice())
            .map_err(failed)?;
        let left = intents.iter().filter(|intent| {
            intent.kind == layerfs_history::GcIntentKind::Mount
                && intent.workspace == Some(workspace)
        });
        let mut ended = 0;
        for intent in left {
            layerfs_store_service::history::admission::release_or_mark(catalog, &intent.op_id)
                .map_err(failed)?;
            ended += 1;
        }
        Ok(ended)
    })
}

/// Settles the intents at `machine` the ordered `incarnation` proves dead,
/// where that Store's calls run: this process when the Store lives here, else
/// the machine's worker. The home recorded the incarnation in the
/// `MachineRecord` before this call. One bounded call per Store; a failure is
/// reported, never retried.
pub fn reconcile_machine(
    home: &Path,
    store: &str,
    machine: &str,
    incarnation: &str,
) -> Result<SdkValue, SdkRefusal> {
    // The host holds the Store's lease while it sends the call, as
    // `store_call` does; a Store an operation is moving is left, never retried.
    let (_lease, record) = crate::admin::open::lease::claim(home, store)?;
    let store_ref = record.store_ref()?;
    match crate::admin::route::executor::plan(home, store_ref)? {
        crate::admin::route::executor::Executor::Local => {
            let args = OpenArgs::of_record(home, store, &record)?;
            // the reconcile compares placed mount intents: it reads the view
            crate::admin::route::place_view::scoped(&args, || {
                with_service(&args, |service| {
                    layerfs_store_service::history::admission::reconcile_machine(
                        service.store.history(),
                        machine,
                        incarnation,
                    )
                    .map(|reconcile| {
                        layerfs_store_service::history::admission::reconcile_value(&reconcile)
                    })
                    .map_err(layerfs_store_service::history::lookup::history)
                })
            })
        }
        crate::admin::route::executor::Executor::Worker(mut worker) => {
            let mut fields = vec![
                ("store".to_owned(), SdkValue::text(store)),
                ("store_record".to_owned(), record.to_record().clone()),
                ("machine".to_owned(), SdkValue::text(machine)),
                ("incarnation".to_owned(), SdkValue::text(incarnation)),
            ];
            if let Some(place) = crate::admin::route::place_view::encoded_from_home(home) {
                fields.push(("place_view".to_owned(), place));
            }
            let request = SdkValue::Map(fields);
            worker.call("store.reconcile", request, &mut |_: SdkValue| {})
        }
    }
}
