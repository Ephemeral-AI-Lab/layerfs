//! One owner and one consumed C5 range; no historical reservation registry.
use crate::wire::{self, Bytes, Snapshot};
use layerfs_content::ObjectId;
use layerfs_history::{catalog::HistoryCatalog, identity::WorkspaceId, records::ReserveRequest};

pub const PAGE: u64 = 64;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Owner {
    pub workspace: Vec<u8>,
    pub incarnation: [u8; 32],
    pub project: [u8; 17],
    pub branch: [u8; 17],
}
impl Owner {
    pub fn check(&self) -> Result<(), String> {
        if self.workspace.is_empty()
            || self.workspace.len() > 255
            || self
                .workspace
                .iter()
                .any(|c| !c.is_ascii_alphanumeric() && !b"_-".contains(c))
        {
            return Err("reservation workspace identity".into());
        }
        WorkspaceId::from_authority(self.incarnation).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn encode(&self, out: &mut Vec<u8>) {
        wire::blob(out, &self.workspace);
        out.extend_from_slice(&self.incarnation);
        out.extend_from_slice(&self.project);
        out.extend_from_slice(&self.branch);
    }
    pub fn read(b: &mut Bytes<'_>) -> Result<Self, String> {
        let owner = Self {
            workspace: b.blob()?,
            incarnation: b.take()?,
            project: b.take()?,
            branch: b.take()?,
        };
        owner.check()?;
        Ok(owner)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Grant {
    pub owner: Owner,
    pub scope: [u8; 32],
    pub profile: [u8; 32],
    pub sequence: u64,
    pub start: u64,
    pub count: u64,
    pub end: u64,
}
impl Grant {
    pub fn check(&self) -> Result<(), String> {
        self.owner.check()?;
        ObjectId::from_bytes(&self.scope).map_err(|e| e.to_string())?;
        ObjectId::from_bytes(&self.profile).map_err(|e| e.to_string())?;
        if self.sequence == 0
            || self.start <= 1
            || self.count != PAGE
            || self.start.checked_add(self.count) != Some(self.end)
            || self.end > i64::MAX as u64
        {
            return Err("reservation range profile".into());
        }
        Ok(())
    }
    pub fn encode(&self, out: &mut Vec<u8>) {
        self.owner.encode(out);
        out.extend_from_slice(&self.scope);
        out.extend_from_slice(&self.profile);
        for n in [self.sequence, self.start, self.count, self.end] {
            out.extend_from_slice(&n.to_be_bytes());
        }
    }
    pub fn read(b: &mut Bytes<'_>) -> Result<Self, String> {
        let grant = Self {
            owner: Owner::read(b)?,
            scope: b.take()?,
            profile: b.take()?,
            sequence: b.u64()?,
            start: b.u64()?,
            count: b.u64()?,
            end: b.u64()?,
        };
        grant.check()?;
        Ok(grant)
    }
    pub fn matches(&self, owner: &Owner, snapshot: &Snapshot) -> Result<(), String> {
        if &self.owner != owner
            || self.scope != snapshot.scope
            || self.profile != snapshot.profile
            || owner.project != snapshot.stack
            || owner.branch != snapshot.branch
        {
            return Err("reservation owner/context".into());
        }
        self.check()
    }
}
#[derive(Default)]
pub struct Book {
    last: Option<Grant>,
    pending: bool,
}
impl Book {
    pub fn bootstrap(
        &mut self,
        history: &impl HistoryCatalog,
        owner: Owner,
        snapshot: &Snapshot,
    ) -> Result<Grant, String> {
        owner.check()?;
        if self.pending || self.last.is_some() {
            return Err("bootstrap already owned".into());
        }
        if owner.project != snapshot.stack || owner.branch != snapshot.branch {
            return Err("reservation selected Branch".into());
        }
        self.allocate(history, owner, snapshot.scope, snapshot.profile, 1, 2)
    }
    pub fn next(
        &mut self,
        history: &impl HistoryCatalog,
        owner: &Owner,
        scope: [u8; 32],
        profile: [u8; 32],
        sequence: u64,
        previous: u64,
    ) -> Result<Grant, String> {
        self.authorize(owner, scope, profile)?;
        let last = self.last.as_ref().ok_or("reservation owner absent")?;
        if last.sequence.checked_add(1) != Some(sequence) || last.end != previous {
            return Err("reservation sequence/endpoint".into());
        }
        self.allocate(history, owner.clone(), scope, profile, sequence, previous)
    }
    pub fn authorize(
        &self,
        owner: &Owner,
        scope: [u8; 32],
        profile: [u8; 32],
    ) -> Result<(), String> {
        owner.check()?;
        if self.pending {
            return Err("reservation pending; no resend".into());
        }
        let last = self.last.as_ref().ok_or("reservation owner absent")?;
        if &last.owner != owner || last.scope != scope || last.profile != profile {
            return Err("reservation owner/context".into());
        }
        Ok(())
    }
    fn allocate(
        &mut self,
        history: &impl HistoryCatalog,
        owner: Owner,
        scope: [u8; 32],
        profile: [u8; 32],
        sequence: u64,
        previous: u64,
    ) -> Result<Grant, String> {
        let scope_id = ObjectId::from_bytes(&scope).map_err(|e| e.to_string())?;
        ObjectId::from_bytes(&profile).map_err(|e| e.to_string())?;
        self.pending = true;
        let range = history
            .reserve_inodes(&ReserveRequest {
                scope: scope_id,
                count: PAGE,
            })
            .map_err(|e| e.to_string())?;
        let grant = Grant {
            owner,
            scope,
            profile,
            sequence,
            start: range.start,
            count: range.count,
            end: range.end().map_err(|e| e.to_string())?,
        };
        grant.check()?;
        if *range.scope.as_bytes() != scope || grant.start < previous {
            return Err("reservation overlap/context".into());
        }
        self.last = Some(grant.clone());
        self.pending = false;
        Ok(grant)
    }
}
/// An actual provider returns one complete next range, or retains uncertain custody.
pub trait Pages: Send + Sync {
    fn next(&self, previous: u64) -> Result<Grant, String>;
}

pub struct NativePages {
    pub remote: crate::metadata::Remote,
    pub initial: Grant,
    pub cursor: std::sync::Mutex<(u64, u64, bool)>,
}
impl NativePages {
    pub fn new(remote: crate::metadata::Remote, initial: Grant) -> Self {
        let cursor = std::sync::Mutex::new((initial.sequence, initial.end, false));
        Self {
            remote,
            initial,
            cursor,
        }
    }
}
impl Pages for NativePages {
    fn next(&self, previous: u64) -> Result<Grant, String> {
        let mut cursor = self.cursor.lock().map_err(|_| "reservation cursor owner")?;
        if cursor.2 {
            return Err("reservation client pending; no resend".into());
        }
        if previous != cursor.1 {
            return Err("reservation client endpoint".into());
        }
        let sequence = cursor
            .0
            .checked_add(1)
            .ok_or("reservation sequence exhaustion")?;
        let mut out = Vec::new();
        self.initial.owner.encode(&mut out);
        out.extend_from_slice(&self.initial.scope);
        out.extend_from_slice(&self.initial.profile);
        out.extend_from_slice(&sequence.to_be_bytes());
        out.extend_from_slice(&previous.to_be_bytes());
        cursor.2 = true;
        let reply = self.remote.call(7, &out)?;
        let mut b = Bytes::new(&reply);
        let grant = Grant::read(&mut b)?;
        b.done()?;
        if grant.owner != self.initial.owner
            || grant.scope != self.initial.scope
            || grant.profile != self.initial.profile
            || grant.sequence != sequence
            || grant.start < previous
        {
            return Err("reservation reply identity/overlap".into());
        }
        *cursor = (sequence, grant.end, false);
        Ok(grant)
    }
}
