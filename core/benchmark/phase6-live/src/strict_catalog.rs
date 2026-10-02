//! Strict SQL metadata / payload-only provider catalog, with captured read scopes.
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::sqlite::{ObjectLocation, ValueGroupRow};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard,
    },
    time::Duration,
};

#[path = "strict_catalog_candidates.rs"]
mod candidates;
#[path = "strict_catalog_owner.rs"]
mod owner;
#[path = "strict_catalog_profile.rs"]
mod profile;
#[path = "strict_catalog_publication.rs"]
mod publication;
pub use owner::SaveContext;
pub use profile::SCHEMA_FINGERPRINT;
#[path = "strict_catalog_pool.rs"]
mod pool;
#[path = "strict_catalog_registration.rs"]
mod registration;
pub use candidates::{CandidateRow, CandidateSnapshotWork};
#[path = "strict_catalog_transfer.rs"]
mod transfer;
pub use transfer::BodyPage;
// Experimental service-instance identity: one OS-random draw, no retry.
// This scopes disposable caches; it is not a persistence or recovery guarantee.
fn next_cache_namespace() -> Result<u64, String> {
    let bytes = layerfs_sandbox::random::<8>().map_err(|e| e.to_string())?;
    let epoch = u64::from_be_bytes(bytes);
    if epoch == 0 {
        return Err("catalog cache namespace unavailable".into());
    }
    Ok(epoch)
}

pub const APPLICATION_ID: u32 = u32::from_be_bytes(*b"P6S1");
pub const USER_VERSION: u32 = 3;
pub const PAGE: usize = layerfs_storage::policy::LOOKUP_PAGE_IDS;
pub const BODY_LIMIT: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(i64)]
pub enum PlacementDomain {
    FilePayload = 0,
    Metadata = 1,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(i64)]
pub enum LogicalUse {
    RegularFileGraph = 0,
    MetadataGraph = 1,
}
#[derive(Clone, Copy, Debug)]
pub struct CatalogScope {
    pub publication: u64,
    pub ceiling: i64,
    pub own_save: Option<i64>,
}
#[derive(Clone, Copy, Debug)]
pub struct Located {
    pub location: ObjectLocation,
    pub domain: PlacementDomain,
    pub digest: [u8; 32],
    pub save: i64,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Reference {
    pub id: ObjectId,
    pub domain: PlacementDomain,
    pub logical_use: LogicalUse,
}
#[derive(Clone, Copy, Debug)]
pub struct PhysicalBase {
    pub id: ObjectId,
    pub domain: PlacementDomain,
    pub body: i64,
}
#[derive(Clone, Debug)]
pub struct Registration {
    pub location: ObjectLocation,
    pub domain: PlacementDomain,
    pub logical_use: LogicalUse,
    pub references: Vec<Reference>,
    pub base: Option<PhysicalBase>,
}
pub(super) struct TransactionOutcome<'a> {
    catalog: &'a StrictCatalog,
    acknowledged: bool,
}
impl TransactionOutcome<'_> {
    fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}
impl Drop for TransactionOutcome<'_> {
    fn drop(&mut self) {
        if !self.acknowledged {
            self.catalog.quarantined.store(true, Ordering::Release);
        }
    }
}
pub struct StrictCatalog {
    db: Mutex<Connection>,
    writable: bool,
    quarantined: AtomicBool,
    cache_namespace: u64,
}
pub(super) fn unknown(e: rusqlite::Error) -> String {
    format!("unknown catalog custody: {e}")
}
pub(super) fn err(e: rusqlite::Error) -> String {
    unknown(e)
}
pub(super) fn admission_err(e: rusqlite::Error) -> String {
    match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            format!("definite catalog refusal: {e}")
        }
        _ => unknown(e),
    }
}
/// Only reviewed typed admission or acknowledged-abort paths emit this marker.
pub fn is_definite_catalog_error(message: &str) -> bool {
    message.starts_with("definite catalog refusal: ")
}
pub(super) fn active(db: &Connection, save: i64) -> Result<(), String> {
    let status: Option<i64> = db
        .query_row("SELECT status FROM saves WHERE save_id=?1", [save], |r| {
            r.get(0)
        })
        .optional()
        .map_err(err)?;
    if status != Some(0) {
        return Err("save is not private and writable".into());
    }
    Ok(())
}
pub(super) fn eligible(scope: CatalogScope) -> Result<i64, String> {
    i64::try_from(scope.publication).map_err(|_| "publication overflow".into())
}
impl StrictCatalog {
    pub(super) fn connection(&self) -> Result<MutexGuard<'_, Connection>, String> {
        if self.quarantined.load(Ordering::Acquire) {
            return Err(
                "unknown catalog custody: engine quarantined after unresolved SQL mutation".into(),
            );
        }
        let db = self
            .db
            .lock()
            .map_err(|_| "unknown catalog custody: SQL owner poisoned")?;
        // A caller may have waited while the previous operation quarantined it.
        if self.quarantined.load(Ordering::Acquire) {
            return Err(
                "unknown catalog custody: engine quarantined after unresolved SQL mutation".into(),
            );
        }
        Ok(db)
    }
    pub(super) fn mutation_result(&self, result: rusqlite::Result<usize>) -> Result<usize, String> {
        match result {
            Ok(rows) => Ok(rows),
            Err(cause) => {
                self.quarantined.store(true, Ordering::Release);
                Err(unknown(cause))
            }
        }
    }
    pub(super) fn commit_tx(
        &self,
        mut tx: rusqlite::Transaction<'_>,
        outcome: &mut TransactionOutcome<'_>,
    ) -> Result<(), String> {
        tx.set_drop_behavior(rusqlite::DropBehavior::Ignore);
        match tx.commit() {
            Ok(()) => {
                outcome.acknowledge();
                Ok(())
            }
            Err(cause) => {
                self.quarantined.store(true, Ordering::Release);
                Err(format!("unknown catalog custody: SQL COMMIT: {cause}"))
            }
        }
    }
    pub(super) fn with_transaction<T>(
        &self,
        operation: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        self.write()?;
        let mut db = self.connection()?;
        let mut tx = match db.transaction() {
            Ok(tx) => tx,
            Err(cause) => {
                if !matches!(
                    cause.sqlite_error_code(),
                    Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
                ) {
                    self.quarantined.store(true, Ordering::Release);
                }
                return Err(admission_err(cause));
            }
        };
        tx.set_drop_behavior(rusqlite::DropBehavior::Ignore);
        let mut outcome = TransactionOutcome {
            catalog: self,
            acknowledged: false,
        };
        match operation(&tx) {
            Ok(value)=> {self.commit_tx(tx,&mut outcome)?;Ok(value)},
            Err(cause)=>match tx.rollback() {
                Ok(())=> {outcome.acknowledge();Err(format!("definite catalog refusal: {cause}"))},
                Err(rollback)=>Err(format!("unknown catalog custody: {cause}; abort acknowledgement unavailable: {rollback}")),
            },
        }
    }
    pub fn cache_namespace(&self) -> u64 {
        self.cache_namespace
    }
    pub fn create(path: &Path) -> Result<Self, String> {
        let db = Connection::open(path).map_err(err)?;
        let app: u32 = db
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(err)?;
        let tables: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        if app != 0 || tables != 0 {
            return Err("strict catalog refuses existing database".into());
        }
        configure(&db, true)?;
        db.execute_batch(include_str!("strict_schema.sql"))
            .map_err(err)?;
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .map_err(err)?;
        db.pragma_update(None, "user_version", USER_VERSION)
            .map_err(err)?;
        profile::validate(&db)?;
        Ok(Self {
            db: Mutex::new(db),
            writable: true,
            quarantined: AtomicBool::new(false),
            cache_namespace: next_cache_namespace()?,
        })
    }
    pub fn open_read_only(path: &Path) -> Result<Self, String> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(err)?;
        configure(&db, false)?;
        validate_existing(&db)?;
        Ok(Self {
            db: Mutex::new(db),
            writable: false,
            quarantined: AtomicBool::new(false),
            cache_namespace: next_cache_namespace()?,
        })
    }
    /// Open a caller-qualified, exclusively owned, known-closed catalog.
    /// This preserves committed and abandoned rows and does not recover or adopt
    /// an earlier unknown outcome. The caller must exclude concurrent openers.
    pub fn open_writable(path: &Path) -> Result<Self, String> {
        refuse_sidecars(path)?;
        if !std::fs::metadata(path)
            .map_err(|cause| format!("closed catalog path: {cause}"))?
            .is_file()
        {
            return Err("closed catalog path is not a regular file".into());
        }
        // WAL headers can cause SQLite to create sidecars even during a read.
        // The current MEMORY journal profile leaves ordinary rollback headers.
        let mut header = [0; 20];
        std::fs::File::open(path)
            .and_then(|mut file| file.read_exact(&mut header))
            .map_err(|cause| format!("closed catalog header: {cause}"))?;
        if &header[..16] != b"SQLite format 3\0" || header[18..20] != [1, 1] {
            return Err("closed catalog journal profile refused".into());
        }
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(err)?;
        // These connection-local settings leave main immutable during admission.
        configure(&db, false)?;
        validate_existing(&db)?;
        let unresolved: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM saves INDEXED BY saves_unresolved WHERE status IN(0,1,3) LIMIT 1) OR EXISTS(SELECT 1 FROM save_context INDEXED BY owned_pending_workspace WHERE owned_pending=1 LIMIT 1)",
            [], |row| row.get(0),
        ).map_err(err)?;
        if unresolved {
            return Err("closed catalog has unresolved save or publication custody".into());
        }
        refuse_sidecars(path)?;
        let cache_namespace = next_cache_namespace()?;
        db.execute_batch("PRAGMA query_only=OFF;").map_err(err)?;
        configure(&db, true)?;
        Ok(Self {
            db: Mutex::new(db),
            writable: true,
            quarantined: AtomicBool::new(false),
            cache_namespace,
        })
    }
    pub fn capture(&self, own_save: Option<i64>) -> Result<CatalogScope, String> {
        let db = self.connection()?;
        if let Some(save) = own_save {
            let status: Option<i64> = db
                .query_row("SELECT status FROM saves WHERE save_id=?1", [save], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(err)?;
            if !matches!(status, Some(0 | 1)) {
                return Err("private scope owner unavailable".into());
            }
        }
        let (publication, ceiling): (i64, i64) = db.query_row("SELECT publication,(SELECT COALESCE(MAX(body_order),0) FROM bodies) FROM catalog_state WHERE singleton=1", [], |r| Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        Ok(CatalogScope {
            publication: publication as u64,
            ceiling,
            own_save,
        })
    }
    pub fn begin_save(&self) -> Result<i64, String> {
        self.write()?;
        let db = self.connection()?;
        self.mutation_result(db.execute("INSERT INTO saves DEFAULT VALUES", []))?;
        Ok(db.last_insert_rowid())
    }
    /// Payload registration is authorized only after a definite provider ACK.
    pub fn register_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<i64, String> {
        self.write()?;
        match (domain, metadata) {
            (PlacementDomain::FilePayload, None) => {}
            (PlacementDomain::Metadata, Some(bytes)) if bytes.len() <= BODY_LIMIT => {
                if Sha256::digest(bytes).as_slice() != digest {
                    return Err("metadata body digest".into());
                }
                let declared = layerfs_storage::pack::layout::declared_length(bytes)
                    .map_err(|e| e.to_string())?;
                if declared != bytes.len() {
                    return Err("metadata complete pack length".into());
                }
                let header = layerfs_storage::pack::layout::parse_header(bytes)
                    .map_err(|e| e.to_string())?;
                for group in 0..header.group_count {
                    layerfs_storage::pack::layout::group_view(bytes, header, group)
                        .map_err(|e| e.to_string())?;
                }
            }
            _ => return Err("strict body domain/byte admission".into()),
        }
        let db = self.connection()?;
        active(&db, save)?;
        self.mutation_result(db.execute(
            "INSERT INTO bodies(domain,save_id,digest,metadata,ready) VALUES(?1,?2,?3,?4,1)",
            params![domain as i64, save, digest.as_slice(), metadata],
        ))?;
        Ok(db.last_insert_rowid())
    }
    pub fn body(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        order: i64,
    ) -> Result<([u8; 32], Option<Vec<u8>>), String> {
        let db = self.connection()?;
        let row: Option<(Vec<u8>,Option<Vec<u8>>)> = db.query_row("SELECT b.digest,b.metadata FROM bodies b JOIN saves s USING(save_id) WHERE b.body_order=?1 AND b.domain=?2 AND b.body_order<=?3 AND ((s.status IN (0,1) AND b.save_id=?4) OR (s.status=2 AND s.publication<=?5))", params![order,domain as i64,scope.ceiling,scope.own_save,eligible(scope)?], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(err)?;
        let (digest, bytes) = row.ok_or("body unavailable in captured domain/scope")?;
        Ok((digest.try_into().map_err(|_| "body digest width")?, bytes))
    }
    pub fn descriptor(&self, id: ObjectId) -> Result<Option<(ObjectRole, usize)>, String> {
        let db = self.connection()?;
        descriptor(&db, id)
    }
    pub fn location(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        id: ObjectId,
    ) -> Result<Option<Located>, String> {
        let db = self.connection()?;
        location(&db, scope, domain, id)
    }
    pub fn locations(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        ids: &[ObjectId],
    ) -> Result<Vec<Option<Located>>, String> {
        if ids.len() > PAGE {
            return Err("catalog lookup page admission".into());
        }
        let db = self.connection()?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; ids.len()].join(",");
        let sql=format!("SELECT l.id,i.role,i.canonical_length,l.body_order,l.group_number,l.record_number,b.digest,b.save_id FROM locators l JOIN identities i USING(id) JOIN bodies b ON b.body_order=l.body_order JOIN saves s USING(save_id) WHERE l.domain=?1 AND l.id IN ({marks}) AND l.body_order=(SELECT ll.body_order FROM locators ll JOIN bodies bb ON bb.body_order=ll.body_order JOIN saves ss USING(save_id) WHERE ll.id=l.id AND ll.domain=l.domain AND ll.body_order<=? AND ((ss.status IN(0,1) AND bb.save_id=?) OR(ss.status=2 AND ss.publication<=?)) ORDER BY ll.body_order LIMIT 1)");
        let mut values = vec![rusqlite::types::Value::Integer(domain as i64)];
        values.extend(
            ids.iter()
                .map(|id| rusqlite::types::Value::Blob(id.as_bytes().to_vec())),
        );
        values.push(rusqlite::types::Value::Integer(scope.ceiling));
        values.push(scope.own_save.map_or(
            rusqlite::types::Value::Null,
            rusqlite::types::Value::Integer,
        ));
        values.push(rusqlite::types::Value::Integer(eligible(scope)?));
        let mut q = db.prepare_cached(&sql).map_err(err)?;
        let mut rows = q.query(rusqlite::params_from_iter(values)).map_err(err)?;
        let mut found = std::collections::BTreeMap::new();
        while let Some(r) = rows.next().map_err(err)? {
            let id: Vec<u8> = r.get(0).map_err(err)?;
            let id = ObjectId::from_bytes(&id).map_err(|e| e.to_string())?;
            let role: u8 = r.get(1).map_err(err)?;
            let digest: Vec<u8> = r.get(6).map_err(err)?;
            found.insert(
                id,
                Located {
                    location: ObjectLocation {
                        object_id: id,
                        role: ObjectRole::from_code(role).map_err(|e| e.to_string())?,
                        canonical_length: r.get::<_, i64>(2).map_err(err)? as usize,
                        pack_id: r.get(3).map_err(err)?,
                        group_number: r.get::<_, i64>(4).map_err(err)? as usize,
                        record_number: r.get::<_, i64>(5).map_err(err)? as usize,
                    },
                    domain,
                    digest: digest.try_into().map_err(|_| "locator digest width")?,
                    save: r.get(7).map_err(err)?,
                },
            );
        }
        Ok(ids.iter().map(|id| found.get(id).copied()).collect())
    }
    pub fn has_use(
        &self,
        scope: CatalogScope,
        id: ObjectId,
        usage: LogicalUse,
    ) -> Result<bool, String> {
        let db = self.connection()?;
        has_use(&db, scope, id, usage)
    }
    pub fn finish_storage(&self, save: i64) -> Result<(), String> {
        self.write()?;
        let db = self.connection()?;
        active(&db, save)?;
        if self.mutation_result(db.execute(
            "UPDATE saves SET status=1 WHERE save_id=?1 AND status=0 AND pending=0",
            [save],
        ))? != 1
        {
            return Err("save has unacknowledged bodies".into());
        }
        Ok(())
    }
    pub fn publish(&self, save: i64) -> Result<u64, String> {
        self.write()?;
        self.with_transaction(|tx| {
        let publication: i64 = tx.query_row("UPDATE catalog_state SET publication=publication+1 WHERE singleton=1 AND publication<9223372036854775807 RETURNING publication",[],|r|r.get(0)).map_err(err)?;
        if tx
            .execute(
                "UPDATE saves SET status=2,publication=?2 WHERE save_id=?1 AND status=1",
                params![save, publication],
            )
            .map_err(err)?
            != 1
        {
            return Err("save not storage ready".into());
        }
        Ok(publication as u64)
        })
    }
    pub fn abandon(&self, save: i64) -> Result<(), String> {
        self.stop(save, 4)
    }
    pub fn quarantine(&self, save: i64) -> Result<(), String> {
        self.stop(save, 3)
    }
    fn stop(&self, save: i64, status: i64) -> Result<(), String> {
        self.write()?;
        let db = self.connection()?;
        if self.mutation_result(db.execute(
            "UPDATE saves SET status=?2 WHERE save_id=?1 AND status IN(0,1)",
            params![save, status],
        ))? != 1
        {
            return Err("save cannot be stopped".into());
        }
        Ok(())
    }
    fn write(&self) -> Result<(), String> {
        if self.writable {
            Ok(())
        } else {
            Err("readonly catalog mutation refused".into())
        }
    }
}
fn validate_existing(db: &Connection) -> Result<(), String> {
    let app: u32 = db
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(err)?;
    let version: u32 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(err)?;
    if app != APPLICATION_ID || version != USER_VERSION {
        return Err("strict catalog schema/profile".into());
    }
    profile::validate(db)
}
fn refuse_sidecars(path: &Path) -> Result<(), String> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        match std::fs::symlink_metadata(Path::new(&sidecar)) {
            Ok(_) => return Err(format!("closed catalog sidecar refused: {suffix}")),
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
            Err(cause) => return Err(format!("closed catalog sidecar inspection: {cause}")),
        }
    }
    Ok(())
}
fn configure(db: &Connection, writable: bool) -> Result<(), String> {
    db.set_prepared_statement_cache_capacity(16);
    db.busy_timeout(Duration::ZERO).map_err(err)?;
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA temp_store=MEMORY;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;").map_err(err)?;
    if writable {
        db.execute_batch("PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;")
            .map_err(err)?;
        candidates::initialize_snapshot(db)?;
    } else {
        db.execute_batch("PRAGMA query_only=ON;").map_err(err)?;
    }
    Ok(())
}
pub(super) fn descriptor(
    db: &Connection,
    id: ObjectId,
) -> Result<Option<(ObjectRole, usize)>, String> {
    let row: Option<(u8, i64)> = db
        .query_row(
            "SELECT role,canonical_length FROM identities WHERE id=?1",
            [id.as_bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    row.map(|(role, len)| {
        Ok((
            ObjectRole::from_code(role).map_err(|e| e.to_string())?,
            usize::try_from(len).map_err(|_| "descriptor length")?,
        ))
    })
    .transpose()
}
pub(super) fn location(
    db: &Connection,
    scope: CatalogScope,
    domain: PlacementDomain,
    id: ObjectId,
) -> Result<Option<Located>, String> {
    db.query_row("SELECT i.role,i.canonical_length,l.body_order,l.group_number,l.record_number,b.digest,b.save_id FROM locators l JOIN identities i USING(id) JOIN bodies b ON b.body_order=l.body_order JOIN saves s USING(save_id) WHERE l.id=?1 AND l.domain=?2 AND l.body_order<=?3 AND ((s.status IN(0,1) AND b.save_id=?4) OR (s.status=2 AND s.publication<=?5)) ORDER BY l.body_order LIMIT 1",params![id.as_bytes().as_slice(),domain as i64,scope.ceiling,scope.own_save,eligible(scope)?],|r|Ok((r.get::<_,u8>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,Vec<u8>>(5)?,r.get::<_,i64>(6)?))).optional().map_err(err)?.map(|(role,length,pack,group,record,digest,save)|Ok(Located {location:ObjectLocation {object_id:id,role:ObjectRole::from_code(role).map_err(|e|e.to_string())?,canonical_length:usize::try_from(length).map_err(|_|"canonical length")?,pack_id:pack,group_number:usize::try_from(group).map_err(|_|"group number")?,record_number:usize::try_from(record).map_err(|_|"record number")?},domain,digest:digest.try_into().map_err(|_|"locator digest width")?,save})).transpose()
}
pub(super) fn has_use(
    db: &Connection,
    scope: CatalogScope,
    id: ObjectId,
    usage: LogicalUse,
) -> Result<bool, String> {
    let Some((role, _)) = descriptor(db, id)? else {
        return Ok(false);
    };
    let domain = match usage {
        LogicalUse::MetadataGraph => PlacementDomain::Metadata,
        LogicalUse::RegularFileGraph
            if matches!(role, ObjectRole::WholeFile | ObjectRole::Chunk) =>
        {
            PlacementDomain::FilePayload
        }
        _ => PlacementDomain::Metadata,
    };
    let Some(selected) = location(db, scope, domain, id)? else {
        return Ok(false);
    };
    db.query_row("SELECT EXISTS(SELECT 1 FROM locators l JOIN logical_uses u USING(locator_id) JOIN saves us ON us.save_id=u.save_id WHERE l.id=?1 AND l.domain=?2 AND l.body_order=?3 AND u.logical_use=?4 AND ((us.status IN(0,1) AND us.save_id=?5) OR (us.status=2 AND us.publication<=?6)))",params![id.as_bytes().as_slice(),domain as i64,selected.location.pack_id,usage as i64,scope.own_save,eligible(scope)?],|r|r.get(0)).map_err(err)
}
