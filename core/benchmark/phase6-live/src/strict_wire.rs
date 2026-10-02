//! Strict SP1 action allocation. Historical P6META7 requests are incompatible.
//!
//! Prefix P6SP1V2 (7 bytes), action (1), request/reply <=16384 bytes.
//! Reply status is followed by the OS-random daemon engine epoch (8 bytes).
//! Status0 succeeds,1 is definite refusal,2 is unknown mutation custody. Pages
//! contain <=8192 bytes. No operation publishes a Branch or supplies C5 approval.
use crate::strict_catalog::{
    CandidateRow, CatalogScope, Located, LogicalUse, PhysicalBase, PlacementDomain, Reference,
    Registration, SaveContext, StrictCatalog, PAGE,
};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::access::{ObjectLocation, ValueGroupRow};

#[path = "strict_wire_dispatch.rs"]
mod dispatch;

pub const PREFIX: &[u8; 7] = b"P6SP1V2";
pub const FRAME_LIMIT: usize = 16384;
pub const BODY_PAGE: usize = 8192;
pub const ACTIONS: usize = 30;

/// Frozen action order; append new actions only in an explicitly reviewed profile.
pub enum Request<'a> {
    Capture(Option<i64>),                                       // 0
    BeginSave,                                                  // 1
    Descriptor(ObjectId),                                       // 2
    Locations(CatalogScope, PlacementDomain, &'a [ObjectId]),   // 3
    HasUse(CatalogScope, ObjectId, LogicalUse),                 // 4
    BodyPage(CatalogScope, PlacementDomain, i64, usize, usize), // 5
    RegisterPayloadBody(i64, [u8; 32]),                         // 6
    BeginMetadataBody(i64, [u8; 32], usize),                    // 7
    AppendMetadataBody(i64, i64, usize, &'a [u8]),              // 8
    FinishMetadataBody(i64, i64),                               // 9
    RegisterUse(
        CatalogScope,
        i64,
        PlacementDomain,
        ObjectId,
        LogicalUse,
        &'a [Reference],
    ), // 10
    RegisterRows(CatalogScope, i64, &'a [Registration]),        // 11
    ReserveOrdinals(i64, usize),                                // 12
    GroupFor(CatalogScope, u32),                                // 13
    GroupPage(CatalogScope, u32, usize),                        // 14
    WindowStart(CatalogScope),                                  // 15
    FinishStorage(i64),                                         // 16
    Abandon(i64),                                               // 17
    Quarantine(i64),                                            // 18
    InsertGroups(CatalogScope, i64, &'a [ValueGroupRow]),       // 19
    NoteWindow(i64, usize, u32),                                // 20
    BodyDomain(CatalogScope, i64),                              // 21
    ReserveBody(PlacementDomain, i64, [u8; 32]),                // 22
    CompletePayloadBody(i64, i64, [u8; 32]),                    // 23
    DiscardReservedBody(i64, i64),                              // 24
    BeginReservedMetadataBody(i64, i64, [u8; 32], usize),       // 25
    ReleaseOrdinals(i64, u64, u64),                             // 26
    CandidatePage(CatalogScope, PlacementDomain, u64, usize),   // 27
    StageCandidates(CatalogScope, i64, PlacementDomain, &'a [CandidateRow]), // 28
    BeginSaveOwned(&'a SaveContext),                            // 29
}
impl Request<'_> {
    pub fn action(&self) -> u8 {
        match self {
            Self::Capture(_) => 0,
            Self::BeginSave => 1,
            Self::Descriptor(_) => 2,
            Self::Locations(..) => 3,
            Self::HasUse(..) => 4,
            Self::BodyPage(..) => 5,
            Self::RegisterPayloadBody(..) => 6,
            Self::BeginMetadataBody(..) => 7,
            Self::AppendMetadataBody(..) => 8,
            Self::FinishMetadataBody(..) => 9,
            Self::RegisterUse(..) => 10,
            Self::RegisterRows(..) => 11,
            Self::ReserveOrdinals(..) => 12,
            Self::GroupFor(..) => 13,
            Self::GroupPage(..) => 14,
            Self::WindowStart(..) => 15,
            Self::FinishStorage(..) => 16,
            Self::Abandon(..) => 17,
            Self::Quarantine(..) => 18,
            Self::InsertGroups(..) => 19,
            Self::NoteWindow(..) => 20,
            Self::BodyDomain(..) => 21,
            Self::ReserveBody(..) => 22,
            Self::CompletePayloadBody(..) => 23,
            Self::DiscardReservedBody(..) => 24,
            Self::BeginReservedMetadataBody(..) => 25,
            Self::ReleaseOrdinals(..) => 26,
            Self::CandidatePage(..) => 27,
            Self::StageCandidates(..) => 28,
            Self::BeginSaveOwned(..) => 29,
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut out = Encoder::new();
        out.raw(PREFIX)?;
        out.byte(self.action())?;
        match self {
            Self::Capture(save) => out.optional(*save)?,
            Self::BeginSave => {}
            Self::BeginSaveOwned(context) => out.save_context(context)?,
            Self::Descriptor(id) => out.id(*id)?,
            Self::Locations(scope, domain, ids) => {
                out.scope(*scope)?;
                out.domain(*domain)?;
                out.count(ids.len(), PAGE)?;
                for id in *ids {
                    out.id(*id)?;
                }
            }
            Self::HasUse(scope, id, usage) => {
                out.scope(*scope)?;
                out.id(*id)?;
                out.usage(*usage)?;
            }
            Self::BodyPage(scope, domain, order, offset, length) => {
                if *length == 0 || *length > BODY_PAGE {
                    return Err("body page admission".into());
                }
                out.scope(*scope)?;
                out.domain(*domain)?;
                out.signed(*order)?;
                out.size(*offset)?;
                out.size(*length)?;
            }
            Self::RegisterPayloadBody(save, digest) => {
                out.signed(*save)?;
                out.raw(digest)?;
            }
            Self::BeginMetadataBody(save, digest, total) => {
                out.signed(*save)?;
                out.raw(digest)?;
                out.size(*total)?;
            }
            Self::AppendMetadataBody(save, order, offset, bytes) => {
                out.signed(*save)?;
                out.signed(*order)?;
                out.size(*offset)?;
                out.blob(bytes)?;
            }
            Self::FinishMetadataBody(save, order) => {
                out.signed(*save)?;
                out.signed(*order)?;
            }
            Self::RegisterUse(scope, save, domain, id, usage, refs) => {
                out.scope(*scope)?;
                out.signed(*save)?;
                out.domain(*domain)?;
                out.id(*id)?;
                out.usage(*usage)?;
                out.refs(refs)?;
            }
            Self::RegisterRows(scope, save, rows) => {
                out.scope(*scope)?;
                out.signed(*save)?;
                out.count(rows.len(), PAGE)?;
                for row in *rows {
                    out.registration(row)?;
                }
            }
            Self::ReserveOrdinals(save, count) => {
                out.signed(*save)?;
                out.size(*count)?;
            }
            Self::GroupFor(scope, ordinal) => {
                out.scope(*scope)?;
                out.u32(*ordinal)?;
            }
            Self::GroupPage(scope, from, limit) => {
                out.scope(*scope)?;
                out.u32(*from)?;
                out.count(*limit, PAGE)?;
            }
            Self::WindowStart(scope) => out.scope(*scope)?,
            Self::InsertGroups(scope, save, groups) => {
                out.scope(*scope)?;
                out.signed(*save)?;
                out.count(groups.len(), PAGE)?;
                for group in *groups {
                    out.group(*group)?;
                }
            }
            Self::NoteWindow(save, count, first) => {
                out.signed(*save)?;
                out.size(*count)?;
                out.u32(*first)?;
            }
            Self::BodyDomain(scope, order) => {
                out.scope(*scope)?;
                out.signed(*order)?;
            }
            Self::ReserveBody(domain, save, digest) => {
                out.domain(*domain)?;
                out.signed(*save)?;
                out.raw(digest)?;
            }
            Self::CompletePayloadBody(save, order, digest) => {
                out.signed(*save)?;
                out.signed(*order)?;
                out.raw(digest)?;
            }
            Self::DiscardReservedBody(save, order) => {
                out.signed(*save)?;
                out.signed(*order)?;
            }
            Self::BeginReservedMetadataBody(save, order, digest, total) => {
                out.signed(*save)?;
                out.signed(*order)?;
                out.raw(digest)?;
                out.size(*total)?;
            }
            Self::ReleaseOrdinals(save, used, reserved) => {
                out.signed(*save)?;
                out.u64(*used)?;
                out.u64(*reserved)?;
            }
            Self::CandidatePage(scope, domain, after, limit) => {
                out.scope(*scope)?;
                out.domain(*domain)?;
                out.u64(*after)?;
                out.count(*limit, PAGE)?;
            }
            Self::StageCandidates(scope, save, domain, rows) => {
                out.scope(*scope)?;
                out.signed(*save)?;
                out.domain(*domain)?;
                out.count(rows.len(), PAGE)?;
                for row in *rows {
                    out.candidate(*row)?;
                }
            }
            Self::FinishStorage(save) | Self::Abandon(save) | Self::Quarantine(save) => {
                out.signed(*save)?
            }
        }
        Ok(out.finish())
    }
}

/// Handles one complete native Begin body and returns a complete Success body.
pub fn handle(catalog: &StrictCatalog, request: &[u8]) -> Result<Vec<u8>, String> {
    if request.len() > FRAME_LIMIT || !request.starts_with(PREFIX) {
        return Err("strict request profile/capacity".into());
    }
    let action = *request.get(PREFIX.len()).ok_or("strict action EOF")?;
    if action as usize >= ACTIONS {
        return Err("strict action unavailable".into());
    }
    let result = dispatch::run(catalog, action, &request[PREFIX.len() + 1..]);
    let mut out = Encoder::new();
    out.raw(PREFIX)?;
    out.byte(action)?;
    match result {
        Ok(bytes) => {
            out.byte(0)?;
            out.u64(catalog.cache_namespace())?;
            out.raw(&bytes)?;
        }
        Err(error) => {
            let (status, message) = match error {
                dispatch::Error::Validation(e) => (1, e),
                dispatch::Error::Operation(e)
                    if crate::strict_catalog::is_definite_catalog_error(&e) =>
                {
                    (1, e)
                }
                dispatch::Error::Operation(e) => (2, e),
            };
            out.byte(status)?;
            out.u64(catalog.cache_namespace())?;
            out.blob(message.as_bytes())?;
        }
    }
    Ok(out.finish())
}
/// A server refusal's conservative custody classification.
#[derive(Debug)]
pub enum Refusal {
    Definite(String),
    Unknown(String),
}

/// Reads the daemon engine epoch bound into this authenticated strict reply.
pub fn response_epoch(action: u8, bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() < 17
        || bytes.len() > FRAME_LIMIT
        || !bytes.starts_with(PREFIX)
        || bytes.get(7) != Some(&action)
    {
        return Err("strict reply identity/profile/capacity".into());
    }
    let epoch = u64::from_be_bytes(bytes[9..17].try_into().map_err(|_| "strict epoch width")?);
    if epoch == 0 {
        return Err("strict engine epoch unavailable".into());
    }
    Ok(epoch)
}
/// Validates reply status; SQL mutation uncertainty is never labeled definite.
pub fn response(action: u8, bytes: &[u8]) -> Result<Result<&[u8], Refusal>, String> {
    response_epoch(action, bytes)?;
    match bytes.get(8) {
        Some(0) => Ok(Ok(&bytes[17..])),
        Some(status @ (1 | 2)) => {
            let mut b = Bytes::new(&bytes[17..]);
            let message =
                String::from_utf8(b.blob()?.to_vec()).map_err(|_| "strict refusal UTF8")?;
            b.done()?;
            Ok(Err(if *status == 1 {
                Refusal::Definite(message)
            } else {
                Refusal::Unknown(message)
            }))
        }
        _ => Err("strict reply status".into()),
    }
}

/// Exact registration row width in this protocol, including every reference/base.
/// The returned cost is used by the native ordered byte-window dispatcher.
pub fn registration_bytes(row: &Registration) -> Result<usize, String> {
    if row.references.len() > 8191 {
        return Err("registration reference count admission".into());
    }
    54usize
        .checked_add(
            row.references
                .len()
                .checked_mul(34)
                .ok_or("registration size overflow")?,
        )
        .and_then(|n| n.checked_add(if row.base.is_some() { 41 } else { 0 }))
        .ok_or_else(|| "registration size overflow".into())
}

pub struct Encoder {
    bytes: Vec<u8>,
}
impl Encoder {
    pub fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(FRAME_LIMIT),
        }
    }
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
    pub fn raw(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > FRAME_LIMIT)
        {
            return Err("strict encoded frame admission".into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub fn byte(&mut self, n: u8) -> Result<(), String> {
        self.raw(&[n])
    }
    pub fn u32(&mut self, n: u32) -> Result<(), String> {
        self.raw(&n.to_be_bytes())
    }
    pub fn size(&mut self, n: usize) -> Result<(), String> {
        self.u32(u32::try_from(n).map_err(|_| "strict size range")?)
    }
    pub fn u64(&mut self, n: u64) -> Result<(), String> {
        self.raw(&n.to_be_bytes())
    }
    pub fn signed(&mut self, n: i64) -> Result<(), String> {
        self.u64(u64::try_from(n).map_err(|_| "strict negative integer")?)
    }
    pub fn id(&mut self, id: ObjectId) -> Result<(), String> {
        self.raw(id.as_bytes())
    }
    pub fn domain(&mut self, d: PlacementDomain) -> Result<(), String> {
        self.byte(d as u8)
    }
    pub fn usage(&mut self, u: LogicalUse) -> Result<(), String> {
        self.byte(u as u8)
    }
    pub fn count(&mut self, n: usize, limit: usize) -> Result<(), String> {
        if n > limit {
            return Err("strict count admission".into());
        }
        self.raw(
            &u16::try_from(n)
                .map_err(|_| "strict count range")?
                .to_be_bytes(),
        )
    }
    pub fn optional(&mut self, n: Option<i64>) -> Result<(), String> {
        self.byte(u8::from(n.is_some()))?;
        if let Some(n) = n {
            self.signed(n)?;
        }
        Ok(())
    }
    pub fn blob(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.count(bytes.len(), BODY_PAGE)?;
        self.raw(bytes)
    }
    pub fn save_context(&mut self, c: &SaveContext) -> Result<(), String> {
        c.check()?;
        self.blob(&c.owner.workspace)?;
        self.raw(&c.owner.incarnation)?;
        self.raw(&c.owner.project)?;
        self.raw(&c.owner.branch)?;
        self.u64(c.generation)?;
        self.raw(&c.scope)?;
        self.raw(&c.profile)?;
        self.raw(&c.base_root)
    }
    pub fn scope(&mut self, s: CatalogScope) -> Result<(), String> {
        self.u64(s.publication)?;
        self.signed(s.ceiling)?;
        self.optional(s.own_save)
    }
    pub fn location(&mut self, l: ObjectLocation) -> Result<(), String> {
        self.id(l.object_id)?;
        self.byte(l.role.code())?;
        self.size(l.canonical_length)?;
        self.signed(l.pack_id)?;
        self.count(l.group_number, 255)?;
        self.count(l.record_number, 8190)
    }
    pub fn located(&mut self, l: Located) -> Result<(), String> {
        self.location(l.location)?;
        self.domain(l.domain)?;
        self.raw(&l.digest)?;
        self.signed(l.save)
    }
    pub fn optional_located(&mut self, l: Option<Located>) -> Result<(), String> {
        self.byte(u8::from(l.is_some()))?;
        if let Some(l) = l {
            self.located(l)?;
        }
        Ok(())
    }
    pub fn refs(&mut self, refs: &[Reference]) -> Result<(), String> {
        self.count(refs.len(), 8191)?;
        for r in refs {
            self.id(r.id)?;
            self.domain(r.domain)?;
            self.usage(r.logical_use)?;
        }
        Ok(())
    }
    pub fn registration(&mut self, r: &Registration) -> Result<(), String> {
        self.location(r.location)?;
        self.domain(r.domain)?;
        self.usage(r.logical_use)?;
        self.refs(&r.references)?;
        self.byte(u8::from(r.base.is_some()))?;
        if let Some(base) = r.base {
            self.id(base.id)?;
            self.domain(base.domain)?;
            self.signed(base.body)?;
        }
        Ok(())
    }
    pub fn candidate(&mut self, r: CandidateRow) -> Result<(), String> {
        self.count(r.slot as usize, 8191)?;
        self.u64(r.stamp)?;
        self.id(r.id)?;
        self.raw(&r.signature)
    }
    pub fn group(&mut self, g: ValueGroupRow) -> Result<(), String> {
        self.u32(g.first_ordinal)?;
        self.count(g.count, 165)?;
        self.signed(g.pack_id)?;
        self.count(g.group_number, 255)?;
        self.id(g.digest)
    }
}
impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Bytes<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Bytes<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn slice(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("strict field overflow")?;
        let bytes = self.bytes.get(self.at..end).ok_or("strict field EOF")?;
        self.at = end;
        Ok(bytes)
    }
    pub fn take<const N: usize>(&mut self) -> Result<[u8; N], String> {
        self.slice(N)?
            .try_into()
            .map_err(|_| "strict fixed width".into())
    }
    pub fn require(&self, n: usize, width: usize) -> Result<(), String> {
        let bytes = n.checked_mul(width).ok_or("strict count overflow")?;
        if bytes > self.bytes.len() - self.at {
            return Err("strict count exceeds encoded bytes".into());
        }
        Ok(())
    }
    pub fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take::<1>()?[0])
    }
    pub fn bool(&mut self) -> Result<bool, String> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("strict boolean tag".into()),
        }
    }
    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take()?))
    }
    pub fn size(&mut self) -> Result<usize, String> {
        Ok(self.u32()? as usize)
    }
    pub fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.take()?))
    }
    pub fn signed(&mut self) -> Result<i64, String> {
        i64::try_from(self.u64()?).map_err(|_| "strict signed range".into())
    }
    pub fn count(&mut self, limit: usize) -> Result<usize, String> {
        let n = u16::from_be_bytes(self.take()?) as usize;
        if n > limit {
            return Err("strict count admission".into());
        }
        Ok(n)
    }
    pub fn optional(&mut self) -> Result<Option<i64>, String> {
        if self.bool()? {
            Ok(Some(self.signed()?))
        } else {
            Ok(None)
        }
    }
    pub fn blob(&mut self) -> Result<&'a [u8], String> {
        let n = self.count(BODY_PAGE)?;
        self.slice(n)
    }
    pub fn id(&mut self) -> Result<ObjectId, String> {
        ObjectId::from_bytes(&self.take::<32>()?).map_err(|e| e.to_string())
    }
    pub fn domain(&mut self) -> Result<PlacementDomain, String> {
        match self.byte()? {
            0 => Ok(PlacementDomain::FilePayload),
            1 => Ok(PlacementDomain::Metadata),
            _ => Err("strict domain tag".into()),
        }
    }
    pub fn usage(&mut self) -> Result<LogicalUse, String> {
        match self.byte()? {
            0 => Ok(LogicalUse::RegularFileGraph),
            1 => Ok(LogicalUse::MetadataGraph),
            _ => Err("strict use tag".into()),
        }
    }
    pub fn save_context(&mut self) -> Result<SaveContext, String> {
        let workspace = self.blob()?;
        if workspace.is_empty() || workspace.len() > 255 {
            return Err("owned save workspace admission".into());
        }
        let owner = crate::reservations::Owner {
            workspace: workspace.to_vec(),
            incarnation: self.take()?,
            project: self.take()?,
            branch: self.take()?,
        };
        owner.check()?;
        let generation = self.u64()?;
        if generation == 0 || generation > i64::MAX as u64 {
            return Err("owned save generation admission".into());
        }
        let context = SaveContext {
            owner,
            generation,
            scope: self.take()?,
            profile: self.take()?,
            base_root: self.take()?,
        };
        context.check()?;
        Ok(context)
    }
    pub fn scope(&mut self) -> Result<CatalogScope, String> {
        Ok(CatalogScope {
            publication: self.u64()?,
            ceiling: self.signed()?,
            own_save: self.optional()?,
        })
    }
    pub fn location(&mut self) -> Result<ObjectLocation, String> {
        Ok(ObjectLocation {
            object_id: self.id()?,
            role: ObjectRole::from_code(self.byte()?).map_err(|e| e.to_string())?,
            canonical_length: self.size()?,
            pack_id: self.signed()?,
            group_number: self.count(255)?,
            record_number: self.count(8190)?,
        })
    }
    pub fn located(&mut self) -> Result<Located, String> {
        Ok(Located {
            location: self.location()?,
            domain: self.domain()?,
            digest: self.take()?,
            save: self.signed()?,
        })
    }
    pub fn optional_located(&mut self) -> Result<Option<Located>, String> {
        if self.bool()? {
            Ok(Some(self.located()?))
        } else {
            Ok(None)
        }
    }
    pub fn candidate(&mut self) -> Result<CandidateRow, String> {
        Ok(CandidateRow {
            slot: self.count(8191)? as u16,
            stamp: self.u64()?,
            id: self.id()?,
            signature: self.take()?,
        })
    }
    pub fn group(&mut self) -> Result<ValueGroupRow, String> {
        Ok(ValueGroupRow {
            first_ordinal: self.u32()?,
            count: self.count(165)?,
            pack_id: self.signed()?,
            group_number: self.count(255)?,
            digest: self.id()?,
        })
    }
    pub fn refs(&mut self) -> Result<Vec<Reference>, String> {
        let n = self.count(8191)?;
        self.require(n, 34)?;
        let mut refs = Vec::with_capacity(n);
        for _ in 0..n {
            refs.push(Reference {
                id: self.id()?,
                domain: self.domain()?,
                logical_use: self.usage()?,
            });
        }
        Ok(refs)
    }
    pub fn registration(&mut self) -> Result<Registration, String> {
        let location = self.location()?;
        let domain = self.domain()?;
        let logical_use = self.usage()?;
        let references = self.refs()?;
        let base = if self.bool()? {
            Some(PhysicalBase {
                id: self.id()?,
                domain: self.domain()?,
                body: self.signed()?,
            })
        } else {
            None
        };
        Ok(Registration {
            location,
            domain,
            logical_use,
            references,
            base,
        })
    }
    pub fn done(self) -> Result<(), String> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err("strict trailing bytes".into())
        }
    }
}
