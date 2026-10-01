use crate::{
    minio::Minio,
    objects::{read_locator, Locator, Locators, ROLES},
    wire::{self, Bytes, Snapshot},
};
use layerfs_bridge::{
    adapters::native::{
        connection::{self, Peer},
        protocol::{decode_response, encode_response, Frame, Kind},
    },
    contract::*,
};
use layerfs_content::ObjectId;
use layerfs_history::{
    catalog::HistoryCatalog,
    identity::{BranchId, CommitId, LayerId, WorkspaceId},
    records::*,
    sqlite::SqliteCatalog,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    net::{TcpListener, ToSocketAddrs},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub struct LocatorDb {
    db: Mutex<Connection>,
    pub s3: Minio,
}
impl LocatorDb {
    pub fn create(path: &Path, s3: Minio) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;PRAGMA temp_store=MEMORY;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;CREATE TABLE objects(id BLOB PRIMARY KEY,role INTEGER NOT NULL,length INTEGER NOT NULL,pack BLOB NOT NULL) WITHOUT ROWID;").map_err(|e|e.to_string())?;
        db.busy_timeout(Duration::ZERO).map_err(|e| e.to_string())?;
        Ok(Self {
            db: Mutex::new(db),
            s3,
        })
    }
}
impl Locators for LocatorDb {
    fn lookup(&self, id: ObjectId) -> Result<Option<Locator>, String> {
        let db = self.db.lock().map_err(|_| "locator owner")?;
        db.query_row(
            "SELECT role,length,pack FROM objects WHERE id=?1",
            [id.as_bytes().as_slice()],
            |r| {
                let role: i64 = r.get(0)?;
                let length: i64 = r.get(1)?;
                let pack: Vec<u8> = r.get(2)?;
                Ok((role, length, pack))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .map(|(role, length, pack)| {
            Ok(Locator {
                id,
                role: *ROLES.get(role as usize).ok_or("locator role")?,
                length: usize::try_from(length).map_err(|_| "locator length")?,
                pack: pack.try_into().map_err(|_| "pack digest width")?,
            })
        })
        .transpose()
    }
    fn register(&self, row: &Locator) -> Result<(), String> {
        let bytes = read_locator(&self.s3, row)?;
        if let Some(old) = self.lookup(row.id)? {
            if old.role != row.role || read_locator(&self.s3, &old)? != bytes {
                return Err("registered CAS mismatch".into());
            }
            return Ok(());
        }
        let db = self.db.lock().map_err(|_| "locator owner")?;
        db.execute(
            "INSERT INTO objects VALUES (?1,?2,?3,?4)",
            params![
                row.id.as_bytes().as_slice(),
                ROLES.iter().position(|r| *r == row.role).ok_or("role")? as i64,
                row.length as i64,
                row.pack.as_slice()
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
#[derive(Clone)]
pub struct Remote {
    pub endpoint: String,
    pub selector: u32,
    pub private: [u8; 32],
    pub server: [u8; 32],
    pub session: Arc<Mutex<crate::metadata_session::Session>>,
}
impl Remote {
    pub fn statistics(&self) -> Result<crate::transport_stats::Statistics, String> {
        self.session
            .lock()
            .map(|s| s.statistics)
            .map_err(|_| "metadata session owner".into())
    }
    pub fn call(&self, action: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
        if payload.len().checked_add(8).is_none_or(|n| n > 16384) || action > 4 {
            return Err("metadata request capacity/action".into());
        }
        let address = self
            .endpoint
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("service address")?;
        self.session
            .lock()
            .map_err(|_| "metadata session owner")?
            .call(
                address,
                self.selector,
                &self.private,
                &self.server,
                action,
                payload,
            )
    }
    pub fn bootstrap(&self) -> Result<(Minio, Snapshot, u64), String> {
        let reply = self.call(0, &[])?;
        let mut b = Bytes::new(&reply);
        let s3 = Minio {
            authority: b.string()?,
            bucket: b.string()?,
            access: b.string()?,
            secret: b.string()?,
        };
        let snapshot = Snapshot::read(&mut b)?;
        let start = b.u64()?;
        b.done()?;
        Ok((s3, snapshot, start))
    }
    pub fn publish(
        &self,
        workspace: &[u8],
        incarnation: [u8; 32],
        generation: u64,
        revision: u64,
        base: &Snapshot,
        root: [u8; 32],
    ) -> Result<WorkspaceCommitReportWire, String> {
        let mut p = Vec::new();
        wire::blob(&mut p, workspace);
        p.extend_from_slice(&incarnation);
        p.extend_from_slice(&generation.to_be_bytes());
        p.extend_from_slice(&revision.to_be_bytes());
        base.encode(&mut p);
        p.extend_from_slice(&root);
        let response = decode_response(&self.call(4, &p)?).map_err(|e| e.to_string())?;
        match response {
            Response::WorkspaceCommit(r)
                if r.workspace == workspace && r.incarnation == incarnation =>
            {
                match r.outcome {
                    WorkspaceCommitOutcome::Completed(report) => Ok(report),
                    _ => Err("publication failed".into()),
                }
            }
            _ => Err("publication response identity".into()),
        }
    }
}
impl Locators for Remote {
    fn lookup(&self, id: ObjectId) -> Result<Option<Locator>, String> {
        let reply = self.call(1, id.as_bytes())?;
        let mut b = Bytes::new(&reply);
        let row = match b.u8()? {
            0 => None,
            1 => Some(wire::read_locator(&mut b)?),
            _ => return Err("locator presence".into()),
        };
        b.done()?;
        if row.as_ref().is_some_and(|r| r.id != id) {
            return Err("lookup identity".into());
        }
        Ok(row)
    }
    fn register(&self, row: &Locator) -> Result<(), String> {
        let mut p = Vec::new();
        wire::locator(&mut p, row);
        let reply = self.call(2, &p)?;
        Bytes::new(&reply).done()
    }
}
pub struct Authority {
    pub locators: Arc<LocatorDb>,
    pub history: SqliteCatalog,
    pub branch: BranchId,
    pub daemon_s3: Minio,
    pub bootstrap_taken: AtomicBool,
}
impl Authority {
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let s = self
            .history
            .branch_snapshot(self.branch)
            .map_err(|e| e.to_string())?
            .ok_or("Branch absent")?;
        Ok(Snapshot {
            stack: s.branch.stack.to_bytes(),
            branch: s.branch.id.to_bytes(),
            base: s.branch.base_layer.to_bytes(),
            head: s.branch.head_commit.map(|id| id.to_bytes()),
            root: *s.effective_root.as_bytes(),
            scope: *s.scope.as_bytes(),
            profile: *s.profile.as_bytes(),
        })
    }
    fn handle(&self, action: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
        let mut b = Bytes::new(payload);
        let mut out = Vec::new();
        match action {
            0 => {
                b.done()?;
                if self.bootstrap_taken.swap(true, Ordering::SeqCst) {
                    return Err("bootstrap already owned".into());
                }
                let s = self.snapshot()?;
                let reservation = self
                    .history
                    .reserve_inodes(&ReserveRequest {
                        scope: ObjectId::from_bytes(&s.scope).map_err(|e| e.to_string())?,
                        count: 512,
                    })
                    .map_err(|e| e.to_string())?;
                for field in [
                    &self.daemon_s3.authority,
                    &self.daemon_s3.bucket,
                    &self.daemon_s3.access,
                    &self.daemon_s3.secret,
                ] {
                    wire::blob(&mut out, field.as_bytes())
                }
                s.encode(&mut out);
                out.extend_from_slice(&reservation.start.to_be_bytes());
            }
            1 => {
                let id = ObjectId::from_bytes(&b.take::<32>()?).map_err(|e| e.to_string())?;
                b.done()?;
                match self.locators.lookup(id)? {
                    None => out.push(0),
                    Some(row) => {
                        out.push(1);
                        wire::locator(&mut out, &row)
                    }
                }
            }
            2 => {
                let row = wire::read_locator(&mut b)?;
                b.done()?;
                self.locators.register(&row)?;
            }
            3 => {
                b.done()?;
                self.snapshot()?.encode(&mut out)
            }
            4 => {
                let workspace = b.blob()?;
                let incarnation = b.take()?;
                let generation = b.u64()?;
                let revision = b.u64()?;
                let s = Snapshot::read(&mut b)?;
                let root = b.take::<32>()?;
                b.done()?;
                if s.branch != self.branch.to_bytes() {
                    return Err("Branch selector".into());
                }
                let candidate = ObjectId::from_bytes(&root).map_err(|e| e.to_string())?;
                let reader = crate::objects::Reader {
                    s3: self.locators.s3.clone(),
                    locators: self.locators.clone(),
                };
                let fs = layerfs_content::FilesystemRead::new(
                    &reader,
                    layerfs_content::filesystem::FilesystemRootId(candidate),
                )
                .map_err(|e| e.to_string())?;
                if fs.root().scope().object().as_bytes() != &s.scope
                    || fs.root().profile().as_bytes() != &s.profile
                {
                    return Err("candidate scope/profile".into());
                }
                crate::audit::candidate(
                    &reader,
                    candidate,
                    fs.root().scope(),
                    fs.root().profile(),
                )?;
                let id = WorkspaceId::from_authority(incarnation).map_err(|e| e.to_string())?;
                let base = LayerId::from_bytes(s.base).map_err(|e| e.to_string())?;
                let stage = self
                    .history
                    .stage_changes(&StageRequest {
                        workspace: id,
                        branch: self.branch,
                        expected_head: s
                            .head
                            .map(CommitId::from_bytes)
                            .transpose()
                            .map_err(|e| e.to_string())?,
                        expected_base: base,
                        expected_root: ObjectId::from_bytes(&s.root).map_err(|e| e.to_string())?,
                        construction_base_root: ObjectId::from_bytes(&s.root)
                            .map_err(|e| e.to_string())?,
                        intended_commit_base: base,
                        candidate_root: candidate,
                        profile: ObjectId::from_bytes(&s.profile).map_err(|e| e.to_string())?,
                        scope: ObjectId::from_bytes(&s.scope).map_err(|e| e.to_string())?,
                        generation,
                    })
                    .map_err(|e| e.to_string())?;
                let outcome = match self
                    .history
                    .commit_staged(&CommitStagedRequest {
                        workspace: id,
                        token: stage.token,
                    })
                    .map_err(|e| e.to_string())?
                {
                    CommitStagedOutcome::Committed(c) => CommitOutcomeWire::Committed(CommitWire {
                        commit: c.id.to_bytes(),
                        stack: c.stack.to_bytes(),
                        root: *c.root.as_bytes(),
                        parent: c.parent.map(|p| p.to_bytes()),
                        base_layer: c.base_layer.to_bytes(),
                    }),
                    CommitStagedOutcome::UpToDate { head, root } => CommitOutcomeWire::UpToDate {
                        head: head.map(|h| h.to_bytes()),
                        root: *root.as_bytes(),
                    },
                };
                let response = Response::WorkspaceCommit(Box::new(WorkspaceCommitWire {
                    workspace,
                    incarnation,
                    outcome: WorkspaceCommitOutcome::Completed(WorkspaceCommitReportWire {
                        generation,
                        revision,
                        stage_token: Some(stage.token.value()),
                        outcome,
                    }),
                }));
                out = encode_response(&response).map_err(|e| e.to_string())?;
            }
            _ => return Err("metadata action unsupported".into()),
        }
        Ok(out)
    }
}
pub fn serve(
    listener: TcpListener,
    authority: Arc<Authority>,
    private: [u8; 32],
    peer: Peer,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let mut c = connection::accept(stream, &private, std::slice::from_ref(&peer))
                    .map_err(|e| e.to_string())?;
                let mut previous = 0u64;
                while !stop.load(Ordering::SeqCst) {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    c.receive.deadline(deadline);
                    c.send.deadline(deadline);
                    let frame = match c.receive.read() {
                        Ok(f) => f,
                        Err(e) => {
                            eprintln!("metadata session closed: {e}");
                            break;
                        }
                    };
                    if frame.kind != Kind::Begin
                        || frame.id <= previous
                        || frame.bytes.len() < 8
                        || !frame.bytes.starts_with(wire::PREFIX)
                    {
                        return Err("metadata request frame/identity".into());
                    }
                    previous = frame.id;
                    match authority.handle(frame.bytes[7], &frame.bytes[8..]) {
                        Ok(reply) => {
                            let mut bytes = wire::PREFIX.to_vec();
                            bytes.extend(reply);
                            c.send
                                .write(&Frame {
                                    kind: Kind::Success,
                                    id: frame.id,
                                    bytes,
                                })
                                .map_err(|e| e.to_string())?;
                        }
                        Err(e) => {
                            eprintln!("metadata refused: {e}");
                            c.send
                                .write(&Frame {
                                    kind: Kind::Failure,
                                    id: frame.id,
                                    bytes: vec![1, 0, 0],
                                })
                                .map_err(|e| e.to_string())?;
                            break;
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::park_timeout(Duration::from_millis(5))
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
