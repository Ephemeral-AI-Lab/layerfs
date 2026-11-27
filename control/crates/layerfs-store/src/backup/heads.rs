//! A sqlite Store's history heads digest where it lives: one read, in this
//! process for a Store on this computer, through its machine's worker
//! (`store.file` step `heads`, within the worker read budget) for any
//! other. Starts nothing; a machine that does not answer is a message.
use crate::backup::copy as remote;
use crate::place::{self, Executor};
use crate::record::{StoreKind, StoreRecord};
use crate::sqlite::transfer::step::{StepFrame, StoreStep};
use layerfs_home::home::paths;
use layerfs_protocol::sdk::hex;
use std::path::Path;

/// Store `record`'s heads digest as hex, or why it could not be read.
pub fn store_heads(home: &Path, record: &StoreRecord) -> Result<String, String> {
    let store = record.name();
    if let Ok(StoreKind::Postgresql) = record.kind() {
        return Err(format!("Store {store} is a PostgreSQL database"));
    }
    let store_ref = record.store_ref().map_err(|refusal| refusal.message)?;
    let executor = place::plan(home, store_ref).map_err(|refusal| refusal.message)?;
    match executor {
        Executor::Local => remote::heads(&paths::store_file(home, store)).map(|heads| hex(&heads)),
        Executor::Worker(mut worker) => {
            let frame = StepFrame::on(store, StoreStep::Heads);
            let answer = worker
                .read("store.file", frame.encode(), place::READ)
                .map_err(|refusal| refusal.message)?;
            answer
                .get_text("heads")
                .map(str::to_owned)
                .ok_or_else(|| format!("the worker read no heads of Store {store}"))
        }
    }
}
