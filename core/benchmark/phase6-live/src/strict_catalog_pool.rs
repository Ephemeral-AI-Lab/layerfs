//! Globally reserved pooled ordinals and indexed bounded group access.
use super::*;
use layerfs_storage::policy::VALUES_PER_GROUP;
impl StrictCatalog {
    pub fn reserve_ordinals(&self, save: i64, count: usize) -> Result<u32, String> {
        self.write()?;
        if count == 0 || count > u32::MAX as usize {
            return Err("ordinal reservation admission".into());
        }
        self.with_transaction(|tx| {
            active(tx, save)?;
            let first: i64 = tx
                .query_row(
                    "SELECT next_ordinal FROM catalog_state WHERE singleton=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(err)?;
            let end = first.checked_add(count as i64).ok_or("ordinal overflow")?;
            if end > u32::MAX as i64 + 1 {
                return Err("ordinal space exhausted".into());
            }
            tx.execute(
                "INSERT INTO ordinal_reservations VALUES(?1,?2,?3)",
                params![first, count as i64, save],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE catalog_state SET next_ordinal=?1 WHERE singleton=1",
                [end],
            )
            .map_err(err)?;
            Ok(first as u32)
        })
    }
    pub fn release_ordinals(
        &self,
        save: i64,
        used_end: u64,
        reserved_end: u64,
    ) -> Result<(), String> {
        self.write()?;
        if used_end >= reserved_end {
            return Ok(());
        }
        let used = i64::try_from(used_end).map_err(|_| "ordinal maximum")?;
        let reserved = i64::try_from(reserved_end).map_err(|_| "ordinal maximum")?;
        self.with_transaction(|tx| {
        active(tx, save)?;
        let first:Option<i64>=tx.query_row("SELECT first_ordinal FROM ordinal_reservations WHERE save_id=?1 AND first_ordinal+count=?2 AND first_ordinal<=?3",params![save,reserved,used],|r|r.get(0)).optional().map_err(err)?;
        let Some(first) = first else {
            return Err("ordinal tail ownership".into());
        };
        let in_use:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM metadata_value_groups WHERE first_ordinal<?2 AND first_ordinal+count>?1)",params![used,reserved],|r|r.get(0)).map_err(err)?;
        if in_use {
            return Err("ordinal tail contains staged values".into());
        }
        if tx
            .execute(
                "UPDATE catalog_state SET next_ordinal=?1 WHERE singleton=1 AND next_ordinal=?2",
                params![used, reserved],
            )
            .map_err(err)?
            == 1
        {
            if first == used {
                tx.execute(
                    "DELETE FROM ordinal_reservations WHERE first_ordinal=?1",
                    [first],
                )
                .map_err(err)?;
            } else {
                tx.execute(
                    "UPDATE ordinal_reservations SET count=?2 WHERE first_ordinal=?1",
                    params![first, used - first],
                )
                .map_err(err)?;
            }
        }
        Ok(())
        })
    }
    pub fn insert_groups(
        &self,
        scope: CatalogScope,
        save: i64,
        groups: &[ValueGroupRow],
    ) -> Result<(), String> {
        self.write()?;
        if scope.own_save != Some(save) || groups.len() > PAGE {
            return Err("group owner/page admission".into());
        }
        self.with_transaction(|tx| {
        active(tx, save)?;
        for group in groups {
            if group.count == 0 || group.count > VALUES_PER_GROUP {
                return Err("group count admission".into());
            }
            let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM ordinal_reservations r JOIN bodies b ON b.body_order=?3 WHERE r.first_ordinal=(SELECT MAX(first_ordinal) FROM ordinal_reservations WHERE first_ordinal<=?1) AND r.first_ordinal+r.count>=?1+?2 AND r.save_id=?4 AND b.save_id=?4 AND b.domain=1 AND b.ready=1)",params![group.first_ordinal,group.count as i64,group.pack_id,save],|r|r.get(0)).map_err(err)?;
            if !valid {
                return Err("group reservation/body ownership".into());
            }
            tx.execute(
                "INSERT INTO metadata_value_groups VALUES(?1,?2,?3,?4,?5)",
                params![
                    group.first_ordinal,
                    group.count as i64,
                    group.pack_id,
                    group.group_number as i64,
                    group.digest.as_bytes().as_slice()
                ],
            )
            .map_err(err)?;
        }
        Ok(())
        })
    }
    pub fn group_for(
        &self,
        scope: CatalogScope,
        ordinal: u32,
    ) -> Result<Option<ValueGroupRow>, String> {
        if ordinal == 0 {
            return Err("ordinal zero".into());
        }
        let db = self.connection()?;
        let sql=format!("{} AND g.first_ordinal=(SELECT MAX(first_ordinal) FROM metadata_value_groups WHERE first_ordinal<=?4) AND g.first_ordinal+g.count>?4",group_query());
        let row: Option<(u32, i64, i64, i64, Vec<u8>)> = db
            .query_row(
                &sql,
                params![scope.ceiling, scope.own_save, eligible(scope)?, ordinal],
                decode,
            )
            .optional()
            .map_err(err)?;
        row.map(convert).transpose()
    }
    /// Inclusive keyset page. Holes from abandoned reservations remain holes.
    pub fn group_page(
        &self,
        scope: CatalogScope,
        from: u32,
        limit: usize,
    ) -> Result<Vec<ValueGroupRow>, String> {
        if limit == 0 || limit > PAGE {
            return Err("group page admission".into());
        }
        let db = self.connection()?;
        let sql = format!(
            "{} AND g.first_ordinal>=?4 ORDER BY g.first_ordinal LIMIT ?5",
            group_query()
        );
        let mut q = db.prepare_cached(&sql).map_err(err)?;
        let rows = q
            .query_map(
                params![
                    scope.ceiling,
                    scope.own_save,
                    eligible(scope)?,
                    from,
                    limit as i64
                ],
                decode,
            )
            .map_err(err)?;
        rows.map(|row| convert(row.map_err(err)?)).collect()
    }
    pub fn note_window(&self, save: i64, used: usize, first: u32) -> Result<(), String> {
        self.write()?;
        if used == 0 {
            return Ok(());
        }
        if used > 131072 || first == 0 {
            return Err("pool window admission".into());
        }
        let db = self.connection()?;
        active(&db, save)?;
        self.mutation_result(db.execute("UPDATE catalog_state SET metadata_window_start=CASE WHEN metadata_window_values+?1>131072 THEN ?2 ELSE metadata_window_start END,metadata_window_values=CASE WHEN metadata_window_values+?1>131072 THEN ?1 ELSE metadata_window_values+?1 END WHERE singleton=1",params![used as i64,first]))?;
        Ok(())
    }
    pub fn metadata_window_start(&self, scope: CatalogScope) -> Result<u32, String> {
        let db = self.connection()?;
        let _ = eligible(scope)?;
        db.query_row(
            "SELECT metadata_window_start FROM catalog_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(err)
    }
}
fn group_query() -> &'static str {
    "SELECT g.first_ordinal,g.count,g.body_order,g.group_number,g.digest FROM metadata_value_groups g JOIN bodies b ON b.body_order=g.body_order JOIN saves s USING(save_id) WHERE b.domain=1 AND b.body_order<=?1 AND ((s.status IN(0,1) AND b.save_id=?2) OR(s.status=2 AND s.publication<=?3))"
}
fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<(u32, i64, i64, i64, Vec<u8>)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
    ))
}
fn convert(row: (u32, i64, i64, i64, Vec<u8>)) -> Result<ValueGroupRow, String> {
    Ok(ValueGroupRow {
        first_ordinal: row.0,
        count: usize::try_from(row.1).map_err(|_| "group count")?,
        pack_id: row.2,
        group_number: usize::try_from(row.3).map_err(|_| "group number")?,
        digest: ObjectId::from_bytes(&row.4).map_err(|e| e.to_string())?,
    })
}
