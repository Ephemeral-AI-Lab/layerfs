//! A project removed from its Store (`project.remove`, here or in the
//! Store's worker).
use crate::identity::{identity, legacy, save_record, store_dir};
use crate::open_args::OpenArgs;
use crate::project::discovery;
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::hex;
use layerfs_home::helpers::text;
use layerfs_home::outcome::{failed, refused, Outcome};

/// `project.remove`'s work on the Store's machine (here, or in its worker
/// as the worker op `project.remove`), `{ home, store, storePath, project }`:
/// drops the project's stack (its Layers, Branches and Commits) from the
/// Store in one write, then its entry from the home. The other projects
/// stay. The entry of the project a Store was made by (its own Init) holds
/// the Store's identity, so a Store record is saved before that entry goes.
/// Its packs and objects stay in the Store; reclaiming them is A8 (#411).
/// The caller stops the project's Sandboxes and clients first.
pub fn remove_project(args: &SdkValue) -> Outcome {
    let home = discovery::home(args)?;
    let store = text(args, "store")?;
    let name = text(args, "project")?;
    let open = OpenArgs::read(args);
    let identity = identity(&open)?;
    let own = name == store && legacy(&home, store)?;
    let entry = layerfs_home::home::paths::project_dir(&home, store, name);
    let stack = crate::service::with_service(&open, |service| {
        let stacks = service
            .store
            .published(layerfs_project::store::projects)
            .map_err(failed)?;
        let Some(stack) = stacks.into_iter().find(|stack| stack.name.as_str() == name) else {
            return Err(refused(
                "NoSuchProject",
                format!("Store {store} holds no project named {name}"),
            ));
        };
        service
            .store
            .history()
            .remove_layerstack(stack.id)
            .map_err(|error| {
                failed(format!(
                    "removing project {name} from Store {store}: {error:?}"
                ))
            })?;
        Ok(stack.id)
    })?;
    // the own project's entry carried the Store's identity until now
    if own {
        layerfs_home::home::private_dir(&store_dir(&home, store)?)?;
        save_record(&home, store, &identity)?;
    }
    if entry.exists() {
        std::fs::remove_dir_all(&entry).map_err(failed)?;
    }
    Ok(SdkValue::map([
        ("stack", SdkValue::text(hex(&stack.to_bytes()))),
        ("packsKept", SdkValue::Bool(true)),
    ]))
}
