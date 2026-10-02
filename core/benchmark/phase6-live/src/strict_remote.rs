//! One persistent strict metadata connection; uncertain requests quarantine custody.
use crate::{
    strict_catalog::{
        CandidateRow, CatalogScope, Located, LogicalUse, PlacementDomain, Reference, Registration,
        SaveContext, BODY_LIMIT, PAGE,
    },
    strict_wire::{self, Bytes, Request, ACTIONS, BODY_PAGE},
};
use layerfs_bridge::adapters::native::{
    connection::{self, Connection},
    protocol::{Frame, Kind},
};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::access::ValueGroupRow;
use sha2::{Digest, Sha256};
use std::{
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemoteError {
    Definite(String),
    Unknown(String),
}
impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definite(e) => write!(f, "definite strict refusal: {e}"),
            Self::Unknown(e) => write!(f, "unknown strict custody: {e}"),
        }
    }
}
impl std::error::Error for RemoteError {}
type Result<T> = std::result::Result<T, RemoteError>;
/// Fixed-size actual operation observations, independent of the historical eight actions.
#[derive(Clone, Copy, Debug)]
pub struct Statistics {
    pub calls: [u64; ACTIONS],
    pub request_ns: [u64; ACTIONS],
    pub connect_attempts: u64,
    pub connect_ns: u64,
}
impl Default for Statistics {
    fn default() -> Self {
        Self {
            calls: [0; ACTIONS],
            request_ns: [0; ACTIONS],
            connect_attempts: 0,
            connect_ns: 0,
        }
    }
}
#[derive(Default)]
pub struct Session {
    connection: Option<Connection>,
    idle_since: Option<Instant>,
    next: u64,
    quarantined: bool,
    engine: Option<u64>,
    pub statistics: Statistics,
}
impl Session {
    fn close(&mut self) {
        if let Some(c) = self.connection.take() {
            c.send.close();
            c.receive.close();
        }
        self.idle_since = None;
    }
    fn unknown<T>(&mut self, error: String) -> Result<T> {
        self.quarantined = true;
        self.close();
        Err(RemoteError::Unknown(error))
    }
    pub fn call(
        &mut self,
        address: SocketAddr,
        selector: u32,
        private: &[u8; 32],
        server: &[u8; 32],
        request: &Request<'_>,
    ) -> Result<Vec<u8>> {
        if self.quarantined {
            return Err(RemoteError::Unknown(
                "strict session quarantined; no resend".into(),
            ));
        }
        let bytes = request.encode().map_err(RemoteError::Definite)?;
        if self
            .idle_since
            .is_some_and(|last| last.elapsed() > Duration::from_secs(2))
        {
            self.close();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        if self.connection.is_none() {
            let start = Instant::now();
            let result = connection::connect_until(address, selector, private, server, deadline)
                .map_err(|e| e.to_string());
            self.statistics.connect_attempts += 1;
            self.statistics.connect_ns = self
                .statistics
                .connect_ns
                .saturating_add(start.elapsed().as_nanos() as u64);
            match result {
                Ok(c) => self.connection = Some(c),
                Err(error) => return self.unknown(error),
            }
        }
        let Some(id) = self.next.checked_add(1) else {
            return self.unknown("strict request identity exhaustion".into());
        };
        self.next = id;
        let action = request.action();
        let start = Instant::now();
        let result = (|| {
            let connection = self
                .connection
                .as_mut()
                .ok_or("strict connection missing")?;
            connection.send.deadline(deadline);
            connection.receive.deadline(deadline);
            connection
                .send
                .write(&Frame {
                    kind: Kind::Begin,
                    id,
                    bytes,
                })
                .map_err(|e| e.to_string())?;
            let reply = connection.receive.read().map_err(|e| e.to_string())?;
            if reply.id != id || reply.kind != Kind::Success {
                return Err("strict reply native identity/kind".into());
            }
            let engine = strict_wire::response_epoch(action, &reply.bytes)?;
            if self.engine.is_some_and(|pinned| pinned != engine) {
                return Err("strict server engine changed within captured scope".into());
            }
            self.engine = Some(engine);
            Ok(strict_wire::response(action, &reply.bytes)?.map(|bytes| bytes.to_vec()))
        })();
        self.statistics.calls[action as usize] += 1;
        self.statistics.request_ns[action as usize] = self.statistics.request_ns[action as usize]
            .saturating_add(start.elapsed().as_nanos() as u64);
        match result {
            Ok(Ok(reply)) => {
                self.idle_since = Some(Instant::now());
                Ok(reply)
            }
            Ok(Err(strict_wire::Refusal::Definite(error))) => {
                self.idle_since = Some(Instant::now());
                Err(RemoteError::Definite(error))
            }
            Ok(Err(strict_wire::Refusal::Unknown(error))) => self.unknown(error),
            Err(error) => self.unknown(error),
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

/// Remote strict catalog; caller-provided authentication selects the daemon service.
#[derive(Clone)]
pub struct NativeCatalog {
    pub endpoint: String,
    pub selector: u32,
    pub private: [u8; 32],
    pub server: [u8; 32],
    pub session: Arc<Mutex<Session>>,
}
impl NativeCatalog {
    pub fn cache_namespace(&self) -> u64 {
        self.session
            .lock()
            .map(|s| s.engine.unwrap_or(0))
            .unwrap_or(0)
    }
    pub fn statistics(&self) -> Result<Statistics> {
        self.session
            .lock()
            .map(|s| s.statistics)
            .map_err(|_| RemoteError::Unknown("strict session owner".into()))
    }
    pub fn call(&self, request: &Request<'_>) -> Result<Vec<u8>> {
        let address = self
            .endpoint
            .to_socket_addrs()
            .map_err(|e| RemoteError::Definite(e.to_string()))?
            .next()
            .ok_or_else(|| RemoteError::Definite("strict service address".into()))?;
        self.session
            .lock()
            .map_err(|_| RemoteError::Unknown("strict session owner".into()))?
            .call(address, self.selector, &self.private, &self.server, request)
    }
    fn decode<T>(
        &self,
        request: &Request<'_>,
        read: impl FnOnce(&mut Bytes<'_>) -> std::result::Result<T, String>,
    ) -> Result<T> {
        let reply = self.call(request)?;
        let result = (|| {
            let mut b = Bytes::new(&reply);
            let value = read(&mut b)?;
            b.done()?;
            Ok(value)
        })();
        match result {
            Ok(value) => Ok(value),
            Err(error) => self
                .session
                .lock()
                .map_err(|_| RemoteError::Unknown("strict session owner".into()))?
                .unknown(error),
        }
    }
    fn empty(&self, request: &Request<'_>) -> Result<()> {
        self.decode(request, |_| Ok(()))
    }
    pub fn capture(&self, owner: Option<i64>) -> Result<CatalogScope> {
        self.decode(&Request::Capture(owner), |b| b.scope())
    }
    pub fn begin_save(&self) -> Result<i64> {
        self.decode(&Request::BeginSave, |b| b.signed())
    }
    pub fn begin_save_owned(&self, context: &SaveContext) -> Result<i64> {
        self.decode(&Request::BeginSaveOwned(context), |b| b.signed())
    }
    pub fn descriptor(&self, id: ObjectId) -> Result<Option<(ObjectRole, usize)>> {
        self.decode(&Request::Descriptor(id), |b| {
            if b.bool()? {
                Ok(Some((
                    ObjectRole::from_code(b.byte()?).map_err(|e| e.to_string())?,
                    b.size()?,
                )))
            } else {
                Ok(None)
            }
        })
    }
    pub fn locations(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        ids: &[ObjectId],
    ) -> Result<Vec<Option<Located>>> {
        let rows = self.decode(&Request::Locations(scope, domain, ids), |b| {
            let n = b.count(PAGE)?;
            let mut rows = Vec::with_capacity(n);
            for _ in 0..n {
                rows.push(b.optional_located()?);
            }
            Ok(rows)
        })?;
        if rows.len() != ids.len()
            || rows.iter().zip(ids).any(|(r, id)| {
                r.is_some_and(|r| {
                    r.location.object_id != *id
                        || r.domain != domain
                        || r.location.pack_id > scope.ceiling
                })
            })
        {
            return self.bad_reply("strict lookup domain/order/cardinality");
        }
        Ok(rows)
    }
    pub fn location(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        id: ObjectId,
    ) -> Result<Option<Located>> {
        self.locations(scope, domain, &[id])?
            .pop()
            .ok_or_else(|| RemoteError::Unknown("strict lookup cardinality".into()))
    }
    pub fn has_use(&self, scope: CatalogScope, id: ObjectId, usage: LogicalUse) -> Result<bool> {
        self.decode(&Request::HasUse(scope, id, usage), |b| b.bool())
    }
    pub fn body_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        order: i64,
        offset: usize,
        length: usize,
    ) -> Result<([u8; 32], Option<usize>, Vec<u8>)> {
        self.decode(
            &Request::BodyPage(scope, domain, order, offset, length),
            |b| {
                let digest = b.take()?;
                let total = if b.bool()? { Some(b.size()?) } else { None };
                let bytes = b.blob()?.to_vec();
                Ok((digest, total, bytes))
            },
        )
    }
    pub fn body(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        order: i64,
    ) -> Result<([u8; 32], Option<Vec<u8>>)> {
        let (digest, total, first) = self.body_page(scope, domain, order, 0, BODY_PAGE)?;
        if domain == PlacementDomain::FilePayload {
            if total.is_some() || !first.is_empty() {
                return self.bad_reply("payload SQL byte shadow refused");
            }
            return Ok((digest, None));
        }
        let Some(total) = total else {
            return self.bad_reply("metadata body length absent");
        };
        if !(32..=BODY_LIMIT).contains(&total) || first.len() != total.min(BODY_PAGE) {
            return self.bad_reply("metadata body page length");
        }
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(&first);
        drop(first);
        while bytes.len() < total {
            let at = bytes.len();
            let (d, t, page) = self.body_page(scope, domain, order, at, BODY_PAGE)?;
            if d != digest || t != Some(total) || page.len() != (total - at).min(BODY_PAGE) {
                return self.bad_reply("metadata body page identity/continuity");
            }
            bytes.extend_from_slice(&page);
        }
        if Sha256::digest(&bytes).as_slice() != digest {
            return self.bad_reply("metadata body digest");
        }
        Ok((digest, Some(bytes)))
    }
    fn bad_reply<T>(&self, message: &str) -> Result<T> {
        self.session
            .lock()
            .map_err(|_| RemoteError::Unknown("strict session owner".into()))?
            .unknown(message.into())
    }
    pub fn register_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<i64> {
        match (domain, metadata) {
            (PlacementDomain::FilePayload, None) => {
                self.decode(&Request::RegisterPayloadBody(save, digest), |b| b.signed())
            }
            (PlacementDomain::Metadata, Some(bytes)) => {
                if bytes.len() > BODY_LIMIT || Sha256::digest(bytes).as_slice() != digest {
                    return Err(RemoteError::Definite(
                        "metadata transfer local digest/capacity".into(),
                    ));
                }
                let order = self.decode(
                    &Request::BeginMetadataBody(save, digest, bytes.len()),
                    |b| b.signed(),
                )?;
                for (page, chunk) in bytes.chunks(BODY_PAGE).enumerate() {
                    self.empty(&Request::AppendMetadataBody(
                        save,
                        order,
                        page * BODY_PAGE,
                        chunk,
                    ))?;
                }
                self.empty(&Request::FinishMetadataBody(save, order))?;
                Ok(order)
            }
            _ => Err(RemoteError::Definite("strict domain/body admission".into())),
        }
    }
    pub fn register_use(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        id: ObjectId,
        usage: LogicalUse,
        refs: &[Reference],
    ) -> Result<()> {
        self.empty(&Request::RegisterUse(scope, save, domain, id, usage, refs))
    }
    pub fn register(
        &self,
        scope: CatalogScope,
        save: i64,
        rows: &[Registration],
    ) -> Result<Vec<Located>> {
        if rows.is_empty() || rows.len() > PAGE {
            return Err(RemoteError::Definite(
                "registration caller row window".into(),
            ));
        }
        // Prefix/action8, scope17 plus optional own-save8, save8, row count2.
        let header = 35 + usize::from(scope.own_save.is_some()) * 8;
        let costs: Vec<usize> = rows
            .iter()
            .map(strict_wire::registration_bytes)
            .collect::<std::result::Result<_, _>>()
            .map_err(RemoteError::Definite)?;
        if costs.iter().any(|n| header + n > strict_wire::FRAME_LIMIT) {
            return Err(RemoteError::Definite(
                "single registration exceeds encoded frame window".into(),
            ));
        }
        let mut out = Vec::with_capacity(rows.len());
        let mut at = 0usize;
        while at < rows.len() {
            let mut end = at;
            let mut bytes = header;
            while end < rows.len()
                && end - at < PAGE
                && bytes + costs[end] <= strict_wire::FRAME_LIMIT
            {
                bytes += costs[end];
                end += 1;
            }
            let page = &rows[at..end];
            let normalized = self.decode(&Request::RegisterRows(scope, save, page), |b| {
                let n = b.count(PAGE)?;
                let mut out = Vec::with_capacity(n);
                for _ in 0..n {
                    out.push(b.located()?);
                }
                Ok(out)
            })?;
            if normalized.len() != page.len()
                || normalized.iter().zip(page).any(|(a, b)| {
                    a.location.object_id != b.location.object_id
                        || a.location.role != b.location.role
                        || a.location.canonical_length != b.location.canonical_length
                        || a.domain != b.domain
                })
            {
                return self.bad_reply("registration normalized identity/cardinality");
            }
            out.extend(normalized);
            at = end;
        }
        Ok(out)
    }
    pub fn reserve_ordinals(&self, save: i64, count: usize) -> Result<u32> {
        self.decode(&Request::ReserveOrdinals(save, count), |b| b.u32())
    }
    pub fn group_for(&self, scope: CatalogScope, ordinal: u32) -> Result<Option<ValueGroupRow>> {
        self.decode(&Request::GroupFor(scope, ordinal), |b| {
            if b.bool()? {
                Ok(Some(b.group()?))
            } else {
                Ok(None)
            }
        })
    }
    pub fn group_page(
        &self,
        scope: CatalogScope,
        from: u32,
        limit: usize,
    ) -> Result<Vec<ValueGroupRow>> {
        self.decode(&Request::GroupPage(scope, from, limit), |b| {
            let n = b.count(PAGE)?;
            b.require(n, 48)?;
            let mut rows = Vec::with_capacity(n);
            for _ in 0..n {
                rows.push(b.group()?);
            }
            Ok(rows)
        })
    }
    pub fn metadata_window_start(&self, scope: CatalogScope) -> Result<u32> {
        self.decode(&Request::WindowStart(scope), |b| b.u32())
    }
    pub fn finish_storage(&self, save: i64) -> Result<()> {
        self.empty(&Request::FinishStorage(save))
    }
    pub fn abandon(&self, save: i64) -> Result<()> {
        self.empty(&Request::Abandon(save))
    }
    pub fn quarantine(&self, save: i64) -> Result<()> {
        self.empty(&Request::Quarantine(save))
    }
    pub fn insert_groups(
        &self,
        scope: CatalogScope,
        save: i64,
        groups: &[ValueGroupRow],
    ) -> Result<()> {
        self.empty(&Request::InsertGroups(scope, save, groups))
    }
    pub fn note_window(&self, save: i64, used: usize, first: u32) -> Result<()> {
        self.empty(&Request::NoteWindow(save, used, first))
    }
    pub fn body_domain(&self, scope: CatalogScope, order: i64) -> Result<PlacementDomain> {
        self.decode(&Request::BodyDomain(scope, order), |b| b.domain())
    }
    pub fn reserve_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
    ) -> Result<i64> {
        self.decode(&Request::ReserveBody(domain, save, digest), |b| b.signed())
    }
    pub fn complete_payload_body(&self, save: i64, order: i64, digest: [u8; 32]) -> Result<()> {
        self.empty(&Request::CompletePayloadBody(save, order, digest))
    }
    pub fn discard_reserved_body(&self, save: i64, order: i64) -> Result<()> {
        self.empty(&Request::DiscardReservedBody(save, order))
    }
    pub fn acknowledge_body(
        &self,
        save: i64,
        order: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<()> {
        match metadata {
            None => self.complete_payload_body(save, order, digest),
            Some(bytes) => {
                if bytes.len() > BODY_LIMIT || Sha256::digest(bytes).as_slice() != digest {
                    return Err(RemoteError::Definite(
                        "reserved metadata local digest/capacity".into(),
                    ));
                }
                self.empty(&Request::BeginReservedMetadataBody(
                    save,
                    order,
                    digest,
                    bytes.len(),
                ))?;
                for (page, chunk) in bytes.chunks(BODY_PAGE).enumerate() {
                    self.empty(&Request::AppendMetadataBody(
                        save,
                        order,
                        page * BODY_PAGE,
                        chunk,
                    ))?;
                }
                self.empty(&Request::FinishMetadataBody(save, order))
            }
        }
    }
    pub fn release_ordinals(&self, save: i64, used: u64, reserved: u64) -> Result<()> {
        self.empty(&Request::ReleaseOrdinals(save, used, reserved))
    }
    pub fn candidate_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        after: u64,
        limit: usize,
    ) -> Result<Vec<CandidateRow>> {
        self.decode(&Request::CandidatePage(scope, domain, after, limit), |b| {
            let n = b.count(PAGE)?;
            b.require(n, 74)?;
            let mut rows = Vec::with_capacity(n);
            for _ in 0..n {
                rows.push(b.candidate()?);
            }
            Ok(rows)
        })
    }
    pub fn stage_candidates(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        rows: &[CandidateRow],
    ) -> Result<()> {
        self.empty(&Request::StageCandidates(scope, save, domain, rows))
    }
}
