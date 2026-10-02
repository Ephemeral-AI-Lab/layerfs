//! Bounded closure and per-use placement staging.
use super::*;
use layerfs_storage::policy::{
    CANONICAL_LIMIT, GROUP_COUNT_LIMIT, RECORD_COUNT_LIMIT, TRANSACTION_CANONICAL_BYTES_LIMIT,
};
use std::collections::BTreeSet;

impl StrictCatalog {
    pub fn register(
        &self,
        scope: CatalogScope,
        save: i64,
        rows: &[Registration],
    ) -> Result<Vec<Located>, String> {
        self.write()?;
        if scope.own_save != Some(save) || rows.is_empty() || rows.len() > PAGE {
            return Err("registration owner/page admission".into());
        }
        let mut canonical = 0u64;
        let mut edges = 0usize;
        for row in rows {
            let l = row.location;
            if l.pack_id <= 0
                || l.canonical_length == 0
                || l.canonical_length > CANONICAL_LIMIT
                || l.group_number >= GROUP_COUNT_LIMIT
                || l.record_number >= RECORD_COUNT_LIMIT
            {
                return Err("registration locator admission".into());
            }
            canonical = canonical
                .checked_add(l.canonical_length as u64)
                .ok_or("registration overflow")?;
            edges = edges
                .checked_add(row.references.len())
                .ok_or("reference overflow")?;
            if canonical > TRANSACTION_CANONICAL_BYTES_LIMIT || edges > 8191 {
                return Err("registration bounded transaction admission".into());
            }
            let expected = match row.logical_use {
                LogicalUse::MetadataGraph => PlacementDomain::Metadata,
                LogicalUse::RegularFileGraph
                    if matches!(l.role, ObjectRole::WholeFile | ObjectRole::Chunk) =>
                {
                    PlacementDomain::FilePayload
                }
                _ => PlacementDomain::Metadata,
            };
            if expected != row.domain {
                return Err("producer provenance/domain mismatch".into());
            }
        }
        self.with_transaction(|tx| {
            active(tx, save)?;
            let mut normalized = Vec::with_capacity(rows.len());
            for row in rows {
                let l = row.location;
                let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bodies WHERE body_order=?1 AND domain=?2 AND save_id=?3)",params![l.pack_id,row.domain as i64,save],|r|r.get(0)).map_err(err)?;
                if !owned {
                    return Err("body not owned by registering save/domain".into());
                }
                if let Some((role, length)) = descriptor(tx, l.object_id)? {
                    if role != l.role || length != l.canonical_length {
                        return Err("immutable canonical descriptor mismatch".into());
                    }
                } else {
                    tx.execute(
                        "INSERT INTO identities VALUES(?1,?2,?3)",
                        params![
                            l.object_id.as_bytes().as_slice(),
                            l.role.code(),
                            l.canonical_length as i64
                        ],
                    )
                    .map_err(err)?;
                }
                // Current owned bodies are admitted for staging, while foreign private
                // rows and newly published rows after the captured scope remain excluded.
                let current = CatalogScope {
                    ceiling: scope.ceiling.max(l.pack_id),
                    ..scope
                };
                let chosen = match location(tx, current, row.domain, l.object_id)? {
                    Some(old) => old,
                    None => {
                        tx.execute("INSERT INTO locators(id,domain,body_order,group_number,record_number) VALUES(?1,?2,?3,?4,?5)",params![l.object_id.as_bytes().as_slice(),row.domain as i64,l.pack_id,l.group_number as i64,l.record_number as i64]).map_err(err)?;
                        location(tx, current, row.domain, l.object_id)?
                            .ok_or("inserted locator missing")?
                    }
                };
                let locator: i64 = tx
                .query_row(
                    "SELECT locator_id FROM locators WHERE id=?1 AND domain=?2 AND body_order=?3",
                    params![
                        l.object_id.as_bytes().as_slice(),
                        row.domain as i64,
                        chosen.location.pack_id
                    ],
                    |r| r.get(0),
                )
                .map_err(err)?;
                let use_save:Option<i64>=tx.query_row("SELECT u.save_id FROM logical_uses u JOIN saves s USING(save_id) WHERE locator_id=?1 AND logical_use=?2 AND ((s.status IN(0,1) AND s.save_id=?3) OR(s.status=2 AND s.publication<=?4)) ORDER BY u.save_id LIMIT 1",params![locator,row.logical_use as i64,scope.own_save,eligible(scope)?],|r|r.get(0)).optional().map_err(err)?;
                if let Some(use_save) = use_save {
                    let old = references(tx, locator, row.logical_use, use_save)?;
                    let submitted: BTreeSet<_> = row
                        .references
                        .iter()
                        .map(|r| (r.id, r.domain as i64, r.logical_use as i64))
                        .collect();
                    if old != submitted {
                        return Err("immutable logical-use reference mismatch".into());
                    }
                } else {
                    tx.execute(
                        "INSERT INTO logical_uses VALUES(?1,?2,?3)",
                        params![locator, row.logical_use as i64, save],
                    )
                    .map_err(err)?;
                    for reference in &row.references {
                        let child = location(tx, current, reference.domain, reference.id)?
                            .ok_or("reference required domain missing")?;
                        let expected = match reference.logical_use {
                            LogicalUse::MetadataGraph => PlacementDomain::Metadata,
                            LogicalUse::RegularFileGraph
                                if matches!(
                                    child.location.role,
                                    ObjectRole::WholeFile | ObjectRole::Chunk
                                ) =>
                            {
                                PlacementDomain::FilePayload
                            }
                            _ => PlacementDomain::Metadata,
                        };
                        if expected != reference.domain {
                            return Err("reference domain/provenance mismatch".into());
                        }
                        if !has_use(tx, current, reference.id, reference.logical_use)? {
                            return Err("reference required use missing".into());
                        }
                        tx.execute(
                            "INSERT OR IGNORE INTO object_references VALUES(?1,?2,?3,?4,?5,?6)",
                            params![
                                locator,
                                row.logical_use as i64,
                                save,
                                reference.id.as_bytes().as_slice(),
                                reference.domain as i64,
                                reference.logical_use as i64
                            ],
                        )
                        .map_err(err)?;
                    }
                }
                if let Some(base) = row.base {
                    if base.domain != row.domain || base.body > l.pack_id {
                        return Err("dependency domain/chronology".into());
                    }
                    let base_loc = location(tx, current, base.domain, base.id)?
                        .ok_or("dependency scope/domain missing")?;
                    if base_loc.location.pack_id != base.body
                        || (
                            base_loc.location.pack_id,
                            base_loc.location.group_number,
                            base_loc.location.record_number,
                        ) >= (l.pack_id, l.group_number, l.record_number)
                    {
                        return Err("dependency physical locator mismatch".into());
                    }
                    // Dependencies describe the selected immutable physical record only.
                    if chosen.location.pack_id == l.pack_id {
                        let prior: Option<(Vec<u8>, i64, i64)> = tx.query_row("SELECT base_id,domain,base_order FROM dependencies WHERE locator_id=?1",[locator],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(err)?;
                        if let Some((id, domain, order)) = prior {
                            if id != base.id.as_bytes().as_slice()
                                || domain != base.domain as i64
                                || order != base.body
                            {
                                return Err("immutable physical dependency mismatch".into());
                            }
                        } else {
                            tx.execute(
                                "INSERT INTO dependencies VALUES(?1,?2,?3,?4)",
                                params![
                                    locator,
                                    base.id.as_bytes().as_slice(),
                                    base.domain as i64,
                                    base.body
                                ],
                            )
                            .map_err(err)?;
                        }
                    }
                }
                normalized.push(chosen);
            }
            Ok(normalized)
        })
    }

    pub fn reserve_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
    ) -> Result<i64, String> {
        self.write()?;
        self.with_transaction(|tx| {
            active(tx, save)?;
            tx.execute(
                "INSERT INTO bodies(domain,save_id,digest) VALUES(?1,?2,?3)",
                params![domain as i64, save, digest.as_slice()],
            )
            .map_err(err)?;
            let order = tx.last_insert_rowid();
            tx.execute(
                "UPDATE saves SET pending=pending+1 WHERE save_id=?1",
                [save],
            )
            .map_err(err)?;
            Ok(order)
        })
    }
    /// Makes a reserved body storage-ready after definite payload ACK or SQL staging.
    pub fn acknowledge_body(
        &self,
        save: i64,
        order: i64,
        metadata: Option<&[u8]>,
    ) -> Result<(), String> {
        self.write()?;
        self.with_transaction(|tx| {
            active(tx, save)?;
            let (domain, digest): (i64, Vec<u8>) = tx
            .query_row(
                "SELECT domain,digest FROM bodies WHERE body_order=?1 AND save_id=?2 AND ready=0",
                params![order, save],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)?;
            match (domain, metadata) {
                (0, None) => {}
                (1, Some(bytes)) if bytes.len() <= BODY_LIMIT => {
                    if Sha256::digest(bytes).as_slice() != digest {
                        return Err("reserved metadata digest".into());
                    }
                    if layerfs_storage::pack::layout::declared_length(bytes)
                        .map_err(|e| e.to_string())?
                        != bytes.len()
                    {
                        return Err("reserved metadata envelope".into());
                    }
                    let header = layerfs_storage::pack::layout::parse_header(bytes)
                        .map_err(|e| e.to_string())?;
                    for group in 0..header.group_count {
                        layerfs_storage::pack::layout::group_view(bytes, header, group)
                            .map_err(|e| e.to_string())?;
                    }
                }
                _ => return Err("reserved body domain/bytes".into()),
            }
            tx.execute(
                "UPDATE bodies SET metadata=?2,ready=1 WHERE body_order=?1",
                params![order, metadata],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE saves SET pending=pending-1 WHERE save_id=?1 AND pending>0",
                [save],
            )
            .map_err(err)?;
            Ok(())
        })
    }
    pub fn body_domain(&self, scope: CatalogScope, order: i64) -> Result<PlacementDomain, String> {
        let db = self.connection()?;
        let domain:i64=db.query_row("SELECT b.domain FROM bodies b JOIN saves s USING(save_id) WHERE b.body_order=?1 AND b.body_order<=?2 AND ((s.status IN(0,1) AND b.save_id=?3) OR(s.status=2 AND s.publication<=?4))",params![order,scope.ceiling,scope.own_save,eligible(scope)?],|r|r.get(0)).map_err(err)?;
        match domain {
            0 => Ok(PlacementDomain::FilePayload),
            1 => Ok(PlacementDomain::Metadata),
            _ => Err("stored domain invalid".into()),
        }
    }
}
fn references(
    db: &Connection,
    locator: i64,
    usage: LogicalUse,
    save: i64,
) -> Result<BTreeSet<(ObjectId, i64, i64)>, String> {
    let mut q=db.prepare_cached("SELECT child_id,child_domain,child_use FROM object_references WHERE locator_id=?1 AND logical_use=?2 AND save_id=?3").map_err(err)?;
    let mut rows = q.query(params![locator, usage as i64, save]).map_err(err)?;
    let mut refs = BTreeSet::new();
    while let Some(row) = rows.next().map_err(err)? {
        let bytes: Vec<u8> = row.get(0).map_err(err)?;
        refs.insert((
            ObjectId::from_bytes(&bytes).map_err(|e| e.to_string())?,
            row.get(1).map_err(err)?,
            row.get(2).map_err(err)?,
        ));
        if refs.len() > 8191 {
            return Err("reference result window".into());
        }
    }
    Ok(refs)
}
impl StrictCatalog {
    pub fn complete_reserved_body(
        &self,
        save: i64,
        order: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<(), String> {
        self.write()?;
        self.with_transaction(|tx| {
        active(tx, save)?;
        let domain:i64=tx.query_row("SELECT domain FROM bodies WHERE body_order=?1 AND save_id=?2 AND ready=0 AND NOT EXISTS(SELECT 1 FROM locators WHERE body_order=?1)",params![order,save],|r|r.get(0)).map_err(err)?;
        match (domain, metadata) {
            (0, None) => {}
            (1, Some(bytes)) if bytes.len() <= BODY_LIMIT => {
                if Sha256::digest(bytes).as_slice() != digest {
                    return Err("final metadata digest".into());
                }
                if layerfs_storage::pack::layout::declared_length(bytes)
                    .map_err(|e| e.to_string())?
                    != bytes.len()
                {
                    return Err("final metadata envelope".into());
                }
                let header = layerfs_storage::pack::layout::parse_header(bytes)
                    .map_err(|e| e.to_string())?;
                for group in 0..header.group_count {
                    layerfs_storage::pack::layout::group_view(bytes, header, group)
                        .map_err(|e| e.to_string())?;
                }
            }
            _ => return Err("final body domain/bytes".into()),
        }
        tx.execute(
            "UPDATE bodies SET digest=?2,metadata=?3,ready=1 WHERE body_order=?1",
            params![order, digest.as_slice(), metadata],
        )
        .map_err(err)?;
        tx.execute(
            "UPDATE saves SET pending=pending-1 WHERE save_id=?1 AND pending>0",
            [save],
        )
        .map_err(err)?;
        Ok(())
        })
    }
    pub fn register_use(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        id: ObjectId,
        logical_use: LogicalUse,
        refs: &[Reference],
    ) -> Result<(), String> {
        self.write()?;
        if scope.own_save != Some(save) || refs.len() > 8191 {
            return Err("reuse use owner/window".into());
        }
        self.with_transaction(|tx| {
        active(tx, save)?;
        let selected =
            location(tx, scope, domain, id)?.ok_or("reuse required placement missing")?;
        let expected = match logical_use {
            LogicalUse::MetadataGraph => PlacementDomain::Metadata,
            LogicalUse::RegularFileGraph
                if matches!(
                    selected.location.role,
                    ObjectRole::WholeFile | ObjectRole::Chunk
                ) =>
            {
                PlacementDomain::FilePayload
            }
            _ => PlacementDomain::Metadata,
        };
        if domain != expected {
            return Err("reuse domain/provenance mismatch".into());
        }
        let locator: i64 = tx
            .query_row(
                "SELECT locator_id FROM locators WHERE id=?1 AND domain=?2 AND body_order=?3",
                params![
                    id.as_bytes().as_slice(),
                    domain as i64,
                    selected.location.pack_id
                ],
                |r| r.get(0),
            )
            .map_err(err)?;
        let prior:Option<i64>=tx.query_row("SELECT u.save_id FROM logical_uses u JOIN saves s USING(save_id) WHERE locator_id=?1 AND logical_use=?2 AND ((s.status IN(0,1) AND s.save_id=?3) OR(s.status=2 AND s.publication<=?4)) ORDER BY u.save_id LIMIT 1",params![locator,logical_use as i64,save,eligible(scope)?],|r|r.get(0)).optional().map_err(err)?;
        let submitted: BTreeSet<_> = refs
            .iter()
            .map(|r| (r.id, r.domain as i64, r.logical_use as i64))
            .collect();
        if let Some(prior) = prior {
            if references(tx, locator, logical_use, prior)? != submitted {
                return Err("immutable reuse reference mismatch".into());
            }
            tx.execute(
                "INSERT OR IGNORE INTO logical_uses VALUES(?1,?2,?3)",
                params![locator, logical_use as i64, save],
            )
            .map_err(err)?;
            for reference in refs {
                tx.execute(
                    "INSERT OR IGNORE INTO object_references VALUES(?1,?2,?3,?4,?5,?6)",
                    params![
                        locator,
                        logical_use as i64,
                        save,
                        reference.id.as_bytes().as_slice(),
                        reference.domain as i64,
                        reference.logical_use as i64
                    ],
                )
                .map_err(err)?;
            }
        } else {
            for reference in refs {
                let child = location(tx, scope, reference.domain, reference.id)?
                    .ok_or("reuse reference placement missing")?;
                let expected = match reference.logical_use {
                    LogicalUse::MetadataGraph => PlacementDomain::Metadata,
                    LogicalUse::RegularFileGraph
                        if matches!(
                            child.location.role,
                            ObjectRole::WholeFile | ObjectRole::Chunk
                        ) =>
                    {
                        PlacementDomain::FilePayload
                    }
                    _ => PlacementDomain::Metadata,
                };
                if reference.domain != expected
                    || !has_use(tx, scope, reference.id, reference.logical_use)?
                {
                    return Err("reuse reference provenance/use missing".into());
                }
            }
            tx.execute(
                "INSERT INTO logical_uses VALUES(?1,?2,?3)",
                params![locator, logical_use as i64, save],
            )
            .map_err(err)?;
            for reference in refs {
                tx.execute(
                    "INSERT OR IGNORE INTO object_references VALUES(?1,?2,?3,?4,?5,?6)",
                    params![
                        locator,
                        logical_use as i64,
                        save,
                        reference.id.as_bytes().as_slice(),
                        reference.domain as i64,
                        reference.logical_use as i64
                    ],
                )
                .map_err(err)?;
            }
        }
        Ok(())
        })
    }
}
