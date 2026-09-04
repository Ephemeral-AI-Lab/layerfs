use crate::records::decode_workspace_stage;
use crate::{BranchId, LayerStackStore, Result, StoreError, WorkspaceStage};
use layerfs_content::ObjectId;
use rusqlite::{OptionalExtension, TransactionBehavior};

impl LayerStackStore {
    pub fn workspace_stage(&self, workspace_id: [u8; 16]) -> Result<Option<WorkspaceStage>> {
        let connection = self.db.reader()?;
        workspace_stage_from_connection(&connection, workspace_id)
    }

    pub fn discard_workspace_stage(&self, workspace_id: [u8; 16]) -> Result<bool> {
        let _operation = self.db.enter_operation()?;
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(stage) = workspace_stage_from_connection(&transaction, workspace_id)? else {
            return Ok(false);
        };
        delete_workspace_stage(&transaction, stage)?;
        transaction.commit()?;
        Ok(true)
    }

    pub(crate) fn stage_workspace_root(
        &self,
        workspace_id: [u8; 16],
        branch_id: BranchId,
        root_id: ObjectId,
    ) -> Result<WorkspaceStage> {
        let requested = WorkspaceStage {
            workspace_id,
            branch_id,
            root_id,
        };
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            crate::statements::workspace::INSERT_STAGE,
            rusqlite::params![
                requested.workspace_id.as_slice(),
                requested.branch_id.as_slice(),
                requested.root_id.as_bytes().as_slice(),
            ],
        )?;
        let actual = workspace_stage_from_connection(&transaction, workspace_id)?
            .ok_or(StoreError::Integrity("Workspace stage insertion"))?;
        if actual != requested {
            return Err(StoreError::InvalidInput("Workspace stage already retained"));
        }
        transaction.commit()?;
        Ok(actual)
    }
}

pub(crate) fn workspace_stage_from_connection(
    connection: &rusqlite::Connection,
    workspace_id: [u8; 16],
) -> Result<Option<WorkspaceStage>> {
    Ok(connection
        .query_row(
            crate::statements::workspace::GET_STAGE,
            [workspace_id.as_slice()],
            decode_workspace_stage,
        )
        .optional()?)
}

pub(crate) fn delete_workspace_stage(
    transaction: &rusqlite::Transaction<'_>,
    stage: WorkspaceStage,
) -> Result<()> {
    if transaction.execute(
        crate::statements::workspace::DELETE_STAGE,
        rusqlite::params![
            stage.workspace_id.as_slice(),
            stage.branch_id.as_slice(),
            stage.root_id.as_bytes().as_slice(),
        ],
    )? != 1
    {
        return Err(StoreError::Integrity("Workspace stage changed"));
    }
    Ok(())
}
