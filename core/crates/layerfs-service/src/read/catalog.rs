//! Read-only catalog requests.
use crate::{
    error::{catalog as failure, content},
    records::*,
};
use layerfs_bridge::contract::HistoryResult;
use layerfs_bridge::contract::*;
use layerfs_history::*;
use layerfs_storage::{Store, StoreProvider};

/// Answers one read-only history query. No content save is started.
pub(crate) fn query(
    catalog: &dyn HistoryCatalog,
    query: &HistoryQuery,
    store: &Store,
) -> Result<Response, Failure> {
    let result = match query {
        HistoryQuery::GetStack { stack } => HistoryResult::Stack(stack_wire(
            &catalog
                .layer_stack(stack_id(stack)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::ListStacks { cursor, limit } => {
            let page = catalog
                .layer_stacks(&Page {
                    cursor: cursor_option(cursor),
                    limit: *limit,
                })
                .map_err(failure)?;
            HistoryResult::Stacks {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(stack_wire).collect(),
            }
        }
        HistoryQuery::GetBranch { branch } => {
            let snapshot = catalog
                .branch_snapshot(branch_id(branch)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?;
            let provider = StoreProvider::new(store);
            let fs = layerfs_content::FilesystemRead::new(
                &provider,
                layerfs_content::filesystem::root::FilesystemRootId(snapshot.effective_root),
            )
            .map_err(content)?;
            if snapshot.profile != fs.root().profile()
                || fs.root().scope().object() != snapshot.scope
            {
                return Err(Code::Integrity.into());
            }
            let mut wire = snapshot_wire(&snapshot);
            wire.root_serial = Some(fs.root().root_inode().serial());
            HistoryResult::BranchSnapshot(wire)
        }
        HistoryQuery::ListBranches {
            stack,
            cursor,
            limit,
        } => {
            let page = catalog
                .branches(
                    stack_id(stack)?,
                    &Page {
                        cursor: cursor_option(cursor),
                        limit: *limit,
                    },
                )
                .map_err(failure)?;
            HistoryResult::Branches {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(branch_wire).collect(),
            }
        }
        HistoryQuery::GetCommit { commit } => HistoryResult::Commit(commit_wire(
            &catalog
                .commit(commit_id(commit)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::CommitHistory {
            branch,
            start,
            cursor,
            limit,
        } => {
            let page = catalog
                .commit_history(&CommitHistoryRequest {
                    branch: branch_id(branch)?,
                    start: optional_commit(start)?,
                    cursor: cursor_option(cursor),
                    limit: *limit,
                })
                .map_err(failure)?;
            HistoryResult::Commits {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(commit_wire).collect(),
            }
        }
        HistoryQuery::GetLayer { layer } => HistoryResult::Layer(layer_wire(
            &catalog
                .layer(layer_id(layer)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::LayerHistory {
            stack,
            start,
            cursor,
            limit,
        } => {
            let page = catalog
                .layer_history(&LayerHistoryRequest {
                    stack: stack_id(stack)?,
                    start: optional_layer(start)?,
                    cursor: cursor_option(cursor),
                    limit: *limit,
                })
                .map_err(failure)?;
            HistoryResult::Layers {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(layer_wire).collect(),
            }
        }
        HistoryQuery::GetStage { workspace } => HistoryResult::Stage(stage_wire(
            &catalog
                .stage(workspace_id(workspace)?)
                .map_err(failure)?
                .ok_or(Code::NotFound)?,
        )),
        HistoryQuery::ListStages {
            branch,
            cursor,
            limit,
        } => {
            let page = catalog
                .stages(
                    branch_id(branch)?,
                    &Page {
                        cursor: cursor_option(cursor),
                        limit: *limit,
                    },
                )
                .map_err(failure)?;
            HistoryResult::Stages {
                continuation: page.continuation.unwrap_or_default(),
                records: page.records.iter().map(stage_wire).collect(),
            }
        }
    };
    Ok(Response::History(Box::new(result)))
}
