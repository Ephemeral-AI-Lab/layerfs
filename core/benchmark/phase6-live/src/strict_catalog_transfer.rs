//! Ordered bounded SQL blob transfer; no growing full-body append copies.
use super::*;
use std::io::{Seek, SeekFrom, Write};
pub const TRANSFER_PAGE: usize = 8192;
pub type BodyPage = ([u8; 32], Option<usize>, Vec<u8>);
type RawBodyPage = (Vec<u8>, Option<i64>, Option<Vec<u8>>);
impl StrictCatalog {
    pub fn body_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        order: i64,
        offset: usize,
        length: usize,
    ) -> Result<BodyPage, String> {
        if length == 0 || length > TRANSFER_PAGE || offset > BODY_LIMIT {
            return Err("body page admission".into());
        }
        let db = self.connection()?;
        let row:RawBodyPage=db.query_row("SELECT b.digest,length(b.metadata),substr(b.metadata,?6+1,?7) FROM bodies b JOIN saves s USING(save_id) WHERE b.body_order=?1 AND b.domain=?2 AND b.body_order<=?3 AND ((s.status IN(0,1) AND b.save_id=?4) OR(s.status=2 AND s.publication<=?5)) AND b.ready=1",params![order,domain as i64,scope.ceiling,scope.own_save,eligible(scope)?,offset as i64,length as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(err)?;
        let total = row
            .1
            .map(|n| usize::try_from(n).map_err(|_| "metadata byte length"))
            .transpose()?;
        if total.is_some_and(|n| offset > n) {
            return Err("body page offset".into());
        }
        Ok((
            row.0.try_into().map_err(|_| "body page digest width")?,
            total,
            row.2.unwrap_or_default(),
        ))
    }
    pub fn begin_metadata_body(
        &self,
        save: i64,
        digest: [u8; 32],
        total: usize,
    ) -> Result<i64, String> {
        self.write()?;
        if !(32..=BODY_LIMIT).contains(&total) {
            return Err("metadata transfer body admission".into());
        }
        self.with_transaction(|tx| {
            active(tx, save)?;
            tx.execute(
                "INSERT INTO bodies(domain,save_id,digest,metadata) VALUES(1,?1,?2,zeroblob(?3))",
                params![save, digest.as_slice(), total as i64],
            )
            .map_err(err)?;
            let order = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO body_transfers(body_order,total) VALUES(?1,?2)",
                params![order, total as i64],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE saves SET pending=pending+1 WHERE save_id=?1",
                [save],
            )
            .map_err(err)?;
            Ok(order)
        })
    }
    pub fn append_metadata_body(
        &self,
        save: i64,
        order: i64,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.write()?;
        if bytes.is_empty() || bytes.len() > TRANSFER_PAGE || offset > BODY_LIMIT {
            return Err("metadata transfer page admission".into());
        }
        self.with_transaction(|tx| {
        active(tx, save)?;
        let (received,total):(i64,i64)=tx.query_row("SELECT t.received,t.total FROM body_transfers t JOIN bodies b USING(body_order) WHERE b.body_order=?1 AND b.save_id=?2 AND b.domain=1 AND b.ready=0 AND t.total=length(b.metadata)",params![order,save],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        let end = offset
            .checked_add(bytes.len())
            .ok_or("metadata transfer overflow")?;
        if offset as i64 != received || end as i64 > total {
            return Err("metadata transfer ordered offset/length".into());
        }
        {
            let mut blob = tx
                .blob_open("main", "bodies", "metadata", order, false)
                .map_err(err)?;
            blob.seek(SeekFrom::Start(offset as u64))
                .map_err(|e| e.to_string())?;
            blob.write_all(bytes).map_err(|e| e.to_string())?;
            blob.close().map_err(err)?;
        }
        tx.execute(
            "UPDATE body_transfers SET received=?2 WHERE body_order=?1",
            params![order, end as i64],
        )
        .map_err(err)?;
        Ok(())
        })
    }
    /// Transport authentication and physical envelope validation, never tree recertification.
    pub fn finish_metadata_body(&self, save: i64, order: i64) -> Result<(), String> {
        self.write()?;
        self.with_transaction(|tx| {
        active(tx, save)?;
        let (digest,bytes):(Vec<u8>,Vec<u8>)=tx.query_row("SELECT b.digest,b.metadata FROM bodies b JOIN body_transfers t USING(body_order) WHERE b.body_order=?1 AND b.save_id=?2 AND b.domain=1 AND b.ready=0 AND t.received=t.total AND t.total=length(b.metadata)",params![order,save],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        if bytes.len() > BODY_LIMIT || Sha256::digest(&bytes).as_slice() != digest {
            return Err("metadata transfer digest".into());
        }
        if layerfs_storage::pack::layout::declared_length(&bytes).map_err(|e| e.to_string())?
            != bytes.len()
        {
            return Err("metadata transfer envelope length".into());
        }
        let header =
            layerfs_storage::pack::layout::parse_header(&bytes).map_err(|e| e.to_string())?;
        for group in 0..header.group_count {
            layerfs_storage::pack::layout::group_view(&bytes, header, group)
                .map_err(|e| e.to_string())?;
        }
        tx.execute("UPDATE bodies SET ready=1 WHERE body_order=?1", [order])
            .map_err(err)?;
        tx.execute("DELETE FROM body_transfers WHERE body_order=?1", [order])
            .map_err(err)?;
        tx.execute(
            "UPDATE saves SET pending=pending-1 WHERE save_id=?1 AND pending>0",
            [save],
        )
        .map_err(err)?;
        Ok(())
        })
    }
    /// Discards only a definite unacknowledged local descriptor without dependents.
    pub fn discard_reserved_body(&self, save: i64, order: i64) -> Result<(), String> {
        self.write()?;
        self.with_transaction(|tx| {
        active(tx, save)?;
        let count=tx.execute("DELETE FROM bodies WHERE body_order=?1 AND save_id=?2 AND ready=0 AND NOT EXISTS(SELECT 1 FROM locators WHERE body_order=?1) AND NOT EXISTS(SELECT 1 FROM metadata_value_groups WHERE body_order=?1)",params![order,save]).map_err(err)?;
        if count != 1 {
            return Err("reserved body not definitely discardable".into());
        }
        tx.execute(
            "UPDATE saves SET pending=pending-1 WHERE save_id=?1 AND pending>0",
            [save],
        )
        .map_err(err)?;
        Ok(())
        })
    }
}

impl StrictCatalog {
    /// Starts ordered byte transfer into an existing owned metadata reservation.
    /// Its digest and global body order remain frozen; no new order is allocated.
    pub fn begin_reserved_metadata_body(
        &self,
        save: i64,
        order: i64,
        digest: [u8; 32],
        total: usize,
    ) -> Result<(), String> {
        self.write()?;
        if !(32..=BODY_LIMIT).contains(&total) {
            return Err("reserved metadata transfer body admission".into());
        }
        self.with_transaction(|tx| {
        active(tx, save)?;
        let expected: Vec<u8> = tx.query_row(
            "SELECT digest FROM bodies WHERE body_order=?1 AND save_id=?2 AND domain=1 AND ready=0 AND metadata IS NULL AND NOT EXISTS(SELECT 1 FROM body_transfers WHERE body_order=?1)",
            params![order, save], |r| r.get(0),
        ).map_err(err)?;
        if expected.as_slice() != digest {
            return Err("reserved metadata immutable digest".into());
        }
        if tx.execute("UPDATE bodies SET metadata=zeroblob(?3) WHERE body_order=?1 AND save_id=?2 AND ready=0 AND metadata IS NULL", params![order,save,total as i64]).map_err(err)? != 1 {
            return Err("reserved metadata transfer ownership".into());
        }
        tx.execute(
            "INSERT INTO body_transfers(body_order,total) VALUES(?1,?2)",
            params![order, total as i64],
        )
        .map_err(err)?;
        Ok(())
        })
    }

    /// Acknowledges exactly the immutable reservation digest, without replacement.
    pub fn acknowledge_body_checked(
        &self,
        save: i64,
        order: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<(), String> {
        self.write()?;
        if let Some(bytes) = metadata {
            if !(32..=BODY_LIMIT).contains(&bytes.len())
                || Sha256::digest(bytes).as_slice() != digest
            {
                return Err("acknowledged metadata digest/capacity".into());
            }
            if layerfs_storage::pack::layout::declared_length(bytes).map_err(|e| e.to_string())?
                != bytes.len()
            {
                return Err("acknowledged metadata envelope length".into());
            }
            let header =
                layerfs_storage::pack::layout::parse_header(bytes).map_err(|e| e.to_string())?;
            for group in 0..header.group_count {
                layerfs_storage::pack::layout::group_view(bytes, header, group)
                    .map_err(|e| e.to_string())?;
            }
        }
        self.with_transaction(|tx| {
        active(tx, save)?;
        let (domain,expected):(i64,Vec<u8>)=tx.query_row("SELECT domain,digest FROM bodies WHERE body_order=?1 AND save_id=?2 AND ready=0 AND metadata IS NULL",params![order,save],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        if expected.as_slice() != digest || !matches!((domain, metadata), (0, None) | (1, Some(_)))
        {
            return Err("acknowledged immutable digest/domain".into());
        }
        tx.execute(
            "UPDATE bodies SET metadata=?3,ready=1 WHERE body_order=?1 AND save_id=?2",
            params![order, save, metadata],
        )
        .map_err(err)?;
        if tx
            .execute(
                "UPDATE saves SET pending=pending-1 WHERE save_id=?1 AND pending>0",
                [save],
            )
            .map_err(err)?
            != 1
        {
            return Err("acknowledged body pending ownership".into());
        }
        Ok(())
        })
    }
}
