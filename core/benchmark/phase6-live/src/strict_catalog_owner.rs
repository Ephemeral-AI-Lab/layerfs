//! Immutable save capture facts bound by the authenticated native producer context.
use super::*;
use crate::reservations::Owner;
use layerfs_history::{BranchId, LayerStackId};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveContext {
    pub owner: Owner,
    pub generation: u64,
    pub scope: [u8; 32],
    pub profile: [u8; 32],
    pub base_root: [u8; 32],
}
impl SaveContext {
    pub fn check(&self) -> Result<(), String> {
        self.owner.check()?;
        LayerStackId::from_bytes(self.owner.project).map_err(|e| e.to_string())?;
        BranchId::from_bytes(self.owner.branch).map_err(|e| e.to_string())?;
        if self.generation == 0 || self.generation > i64::MAX as u64 {
            return Err("save context generation".into());
        }
        ObjectId::from_bytes(&self.scope).map_err(|e| e.to_string())?;
        ObjectId::from_bytes(&self.profile).map_err(|e| e.to_string())?;
        ObjectId::from_bytes(&self.base_root).map_err(|e| e.to_string())?;
        Ok(())
    }
}
impl StrictCatalog {
    pub fn begin_save_owned(&self, context: &SaveContext) -> Result<i64, String> {
        self.write()?;
        context.check()?;
        self.with_transaction(|tx| {
            tx.execute("INSERT INTO saves DEFAULT VALUES",[]).map_err(err)?;
            let save=tx.last_insert_rowid();
            tx.execute("INSERT INTO save_context(save_id,workspace,incarnation,project,branch,generation,scope,profile,base_root) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![save,context.owner.workspace.as_slice(),context.owner.incarnation.as_slice(),context.owner.project.as_slice(),context.owner.branch.as_slice(),context.generation as i64,context.scope.as_slice(),context.profile.as_slice(),context.base_root.as_slice()]).map_err(err)?;
            Ok(save)
        })
    }

    pub fn matches_save_owner(&self, save: i64, context: &SaveContext) -> Result<bool, String> {
        context.check()?;
        let db = self.connection()?;
        db.query_row("SELECT EXISTS(SELECT 1 FROM save_context c JOIN saves s USING(save_id) WHERE c.save_id=?1 AND s.status IN(0,1) AND c.owned_pending=1 AND c.workspace=?2 AND c.incarnation=?3 AND c.project=?4 AND c.branch=?5 AND c.generation=?6 AND c.scope=?7 AND c.profile=?8 AND c.base_root=?9)",params![save,context.owner.workspace.as_slice(),context.owner.incarnation.as_slice(),context.owner.project.as_slice(),context.owner.branch.as_slice(),context.generation as i64,context.scope.as_slice(),context.profile.as_slice(),context.base_root.as_slice()],|r|r.get(0)).map_err(err)
    }
    /// Host authority calls only after the exact conditional C5 outcome is known.
    pub fn complete_owned_publication(&self, save: i64) -> Result<(), String> {
        self.write()?;
        let db = self.connection()?;
        if self.mutation_result(db.execute("UPDATE save_context SET owned_pending=0 WHERE save_id=?1 AND owned_pending=1 AND EXISTS(SELECT 1 FROM saves WHERE save_id=?1 AND status=2)",[save]))?!=1 {return Err("owned publication completion unavailable".into());}
        Ok(())
    }
}
