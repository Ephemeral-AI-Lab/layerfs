//! Indexed storage-ready root ownership, without canonical/tree/provider reads.
use super::*;
impl StrictCatalog {
    pub fn ready_root(&self, save: i64, root: ObjectId) -> Result<bool, String> {
        if save <= 0 {
            return Ok(false);
        }
        let db = self.connection()?;
        db.query_row("SELECT EXISTS(SELECT 1 FROM saves owned JOIN logical_uses u ON u.save_id=owned.save_id JOIN locators l ON l.locator_id=u.locator_id JOIN identities i ON i.id=l.id JOIN bodies b ON b.body_order=l.body_order JOIN saves stored ON stored.save_id=b.save_id WHERE owned.save_id=?2 AND owned.status=1 AND u.logical_use=1 AND l.id=?1 AND l.domain=1 AND i.role=?3 AND b.domain=1 AND b.ready=1 AND ((b.save_id=?2 AND stored.status=1) OR stored.status=2) AND l.body_order=(SELECT ll.body_order FROM locators ll JOIN bodies bb ON bb.body_order=ll.body_order JOIN saves ss ON ss.save_id=bb.save_id WHERE ll.id=l.id AND ll.domain=1 AND ((ss.status IN(0,1) AND bb.save_id=?2) OR ss.status=2) ORDER BY ll.body_order LIMIT 1))",params![root.as_bytes().as_slice(),save,ObjectRole::FilesystemRoot.code()],|r|r.get(0)).map_err(err)
    }
}
