use crate::{
    minio::Minio,
    objects::{Locator, Locators},
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
use std::{
    net::{TcpListener, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub use crate::metadata_catalog::LocatorDb;
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
        if payload.len().checked_add(8).is_none_or(|n| n > 16384) || action > 6 {
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
            stats: Arc::new(Mutex::new(Default::default())),
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
    fn lookup_many(&self, ids: &[ObjectId]) -> Result<Vec<Option<Locator>>, String> {
        if ids.is_empty() || ids.len() > 128 {
            return Err("lookup page admission".into());
        }
        let mut p = (ids.len() as u16).to_be_bytes().to_vec();
        for id in ids {
            p.extend_from_slice(id.as_bytes())
        }
        let reply = self.call(5, &p)?;
        let mut b = Bytes::new(&reply);
        if b.count()? != ids.len() {
            return Err("lookup cardinality".into());
        }
        let mut rows = Vec::new();
        for id in ids {
            let row = match b.u8()? {
                0 => None,
                1 => Some(wire::read_locator(&mut b)?),
                _ => return Err("lookup presence".into()),
            };
            if row.as_ref().is_some_and(|r| r.id != *id || r.pack_id == 0) {
                return Err("lookup identity".into());
            }
            rows.push(row);
        }
        b.done()?;
        Ok(rows)
    }
    fn register_many(&self, rows: &[Locator]) -> Result<Vec<Locator>, String> {
        if rows.is_empty() || rows.len() > 128 || rows.iter().any(|r| r.pack_id != 0) {
            return Err("registration page admission".into());
        }
        let mut p = (rows.len() as u16).to_be_bytes().to_vec();
        for row in rows {
            wire::locator(&mut p, row)
        }
        let reply = self.call(6, &p)?;
        let mut b = Bytes::new(&reply);
        if b.count()? != rows.len() {
            return Err("registration cardinality".into());
        }
        let mut result = Vec::new();
        for input in rows {
            let row = wire::read_locator(&mut b)?;
            if row.id != input.id
                || row.role != input.role
                || row.length != input.length
                || row.pack_id == 0
            {
                return Err("registration identity".into());
            }
            result.push(row);
        }
        b.done()?;
        Ok(result)
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
            5 => {
                let count = b.count()?;
                let mut ids = Vec::with_capacity(count);
                for _ in 0..count {
                    ids.push(ObjectId::from_bytes(&b.take::<32>()?).map_err(|e| e.to_string())?)
                }
                b.done()?;
                out.extend_from_slice(&(count as u16).to_be_bytes());
                for row in self.locators.lookup_many(&ids)? {
                    match row {
                        None => out.push(0),
                        Some(row) => {
                            out.push(1);
                            wire::locator(&mut out, &row)
                        }
                    }
                }
            }
            6 => {
                let count = b.count()?;
                let mut rows = Vec::with_capacity(count);
                for _ in 0..count {
                    rows.push(wire::read_locator(&mut b)?)
                }
                b.done()?;
                let rows = self.locators.register_many(&rows)?;
                out.extend_from_slice(&(count as u16).to_be_bytes());
                for row in rows {
                    wire::locator(&mut out, &row)
                }
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
                let reader =
                    crate::objects::Reader::new(self.locators.s3.clone(), self.locators.clone());
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
                    &self.locators,
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
                eprintln!("P6_AUTHORITY_IO {}", self.locators.s3.statistics()?.json());
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
