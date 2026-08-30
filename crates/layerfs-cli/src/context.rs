use crate::{CliError, CliResult};
use layerfs_sdk::{BranchStore, Client, ConnectionContext, LayerStackEndpoint, LayerStackStore};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SavedContext {
    pub layerstack: PathBuf,
    pub branch: PathBuf,
}

pub fn default_context_location() -> PathBuf {
    std::env::var_os("LAYERFS_CONTEXT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".layerfs/context"))
}

pub(crate) fn save(path: &Path, context: &SavedContext) -> CliResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CliError::Context("context path".to_owned()))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".context-{}.tmp", std::process::id()));
    let contents = format!(
        "layerstack={}\nbranch={}\n",
        context.layerstack.display(),
        context.branch.display()
    );
    std::fs::write(&temporary, contents)?;
    std::fs::rename(&temporary, path)?;
    Ok(())
}

pub(crate) fn load(path: &Path) -> CliResult<SavedContext> {
    let contents = std::fs::read_to_string(path)?;
    let mut layerstack = None;
    let mut branch = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("layerstack=") {
            layerstack = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("branch=") {
            branch = Some(PathBuf::from(value));
        } else if !line.is_empty() {
            return Err(CliError::Context("invalid context".to_owned()));
        }
    }
    Ok(SavedContext {
        layerstack: layerstack.ok_or_else(|| CliError::Context("missing layerstack".to_owned()))?,
        branch: branch.ok_or_else(|| CliError::Context("missing branch".to_owned()))?,
    })
}

pub(crate) fn client(path: &Path) -> CliResult<Client> {
    let context = load(path)?;
    let layerstack = Arc::new(LayerStackStore::connect(&context.layerstack)?);
    let branches = BranchStore::connect(&context.branch, layerstack.store_id())?;
    Ok(Client::connect(ConnectionContext {
        layerstack: LayerStackEndpoint::local(layerstack),
        branches,
    })?)
}
