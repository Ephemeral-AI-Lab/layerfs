//! Test-only helpers for the prepared stream: read one, or build one to send.
//!
//! Nothing here is product code. A delivery that is asked to carry a prepared
//! update reads the body its request declared and records the rows, so the
//! oracle of these tests is the update that actually reached the wire rather
//! than a summary of it; a test that sends one builds the same body the
//! workspace would.

#![allow(dead_code)]

use layerfs_bridge::contract::*;
use std::time::Instant;

/// One prepared update as a delivery received it.
#[derive(Clone, Debug)]
pub struct Recorded {
    /// The request header: identity, addressed fields and declared totals.
    pub header: PreparedChanges,
    /// The directory rows, in parent order.
    pub directories: Vec<DirectoryChange>,
    /// The identity rows, in serial order.
    pub identities: Vec<PreparedIdentity>,
}

impl Recorded {
    /// Reads the body one request declared and records its rows.
    pub fn read(
        header: &PreparedChanges,
        input: &mut dyn Source,
        deadline: Instant,
    ) -> Result<Self, Failure> {
        let body = drain(input, deadline)?;
        let mut sink = Recorder::default();
        read_prepared_stream(
            &header.totals,
            header.root_serial,
            &mut &body[..],
            &mut sink,
        )?;
        Ok(Self {
            header: header.clone(),
            directories: sink.directories,
            identities: sink.identities,
        })
    }

    /// The typed rows of this update: serial, kind, content, metadata and whether
    /// the caller's allocator created it.
    pub fn rooted(&self) -> impl Iterator<Item = (u64, u8, Root, Root, bool)> + '_ {
        self.identities.iter().filter_map(|row| match row {
            PreparedIdentity::Rooted {
                serial,
                kind,
                content,
                metadata,
                fresh,
            } => Some((*serial, *kind, *content, *metadata, *fresh)),
            _ => None,
        })
    }

    /// True when this update declares `serial` for the first time.
    pub fn declared(&self, serial: u64) -> bool {
        self.identities.iter().any(
            |row| matches!(row, PreparedIdentity::DirectoryDeclaration { serial: named, .. } if *named == serial),
        )
    }

    /// Every binding of every directory row, as `(name, serial)`.
    pub fn bindings(&self) -> impl Iterator<Item = (&[u8], Option<u64>)> + '_ {
        self.directories
            .iter()
            .flat_map(|row| row.changes.iter())
            .map(|(name, serial)| (name.as_slice(), *serial))
    }
}

#[derive(Default)]
struct Recorder {
    directories: Vec<DirectoryChange>,
    identities: Vec<PreparedIdentity>,
}

impl PreparedRowSink for Recorder {
    fn directory(
        &mut self,
        parent: u64,
        changes: Vec<(Vec<u8>, Option<u64>)>,
    ) -> Result<(), Failure> {
        self.directories.push(DirectoryChange { parent, changes });
        Ok(())
    }

    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure> {
        self.identities.push(row);
        Ok(())
    }
}

/// Drains one cooperative body to its end.
pub fn drain(input: &mut dyn Source, deadline: Instant) -> Result<Vec<u8>, Failure> {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut body = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = input
            .read(&mut buffer, deadline, &cancel)
            .map_err(|_| Failure::from(Code::Io))?;
        if count == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..count]);
    }
    Ok(body)
}

/// One typed row of a prepared update.
pub fn rooted(
    serial: u64,
    kind: u8,
    content: Root,
    metadata: Root,
    fresh: bool,
) -> PreparedIdentity {
    PreparedIdentity::Rooted {
        serial,
        kind,
        content,
        metadata,
        fresh,
    }
}

/// One maintained directory's selected portable fields.
pub fn patch(serial: u64, mode: u32, seconds: i64, nanos: u32) -> PreparedIdentity {
    PreparedIdentity::DirectoryPatch {
        serial,
        mode,
        mtime_seconds: seconds,
        mtime_nanoseconds: nanos,
    }
}

/// One directory the canonical state has not accepted before.
pub fn declaration(serial: u64, mode: u32, seconds: i64, nanos: u32) -> PreparedIdentity {
    PreparedIdentity::DirectoryDeclaration {
        serial,
        mode,
        mtime_seconds: seconds,
        mtime_nanoseconds: nanos,
    }
}

/// The exact totals of these rows.
pub fn totals(directories: &[DirectoryChange], identities: &[PreparedIdentity]) -> PreparedTotals {
    let mut totals = PreparedTotals {
        directories: directories.len() as u64,
        identities: identities.len() as u64,
        ..PreparedTotals::default()
    };
    for row in directories {
        totals.names += row.changes.len() as u64;
        totals.name_bytes += row
            .changes
            .iter()
            .map(|(name, _)| 10 + name.len() as u64)
            .sum::<u64>();
    }
    for row in identities {
        match row {
            PreparedIdentity::Rooted { fresh, .. } => {
                if *fresh {
                    totals.fresh += 1;
                }
            }
            PreparedIdentity::DirectoryPatch { .. } => totals.patches += 1,
            PreparedIdentity::DirectoryDeclaration { .. } => {
                totals.declarations += 1;
                totals.fresh += 1;
            }
        }
    }
    totals
}

/// Encodes one prepared stream body from these rows.
pub fn body(
    directories: &[DirectoryChange],
    identities: &[PreparedIdentity],
) -> Result<Vec<u8>, Failure> {
    let mut out = Vec::new();
    put_stream_tag(&mut out)?;
    for row in directories {
        put_directory_row(&mut out, row.parent, &row.changes)?;
    }
    for row in identities {
        match row {
            PreparedIdentity::Rooted {
                serial,
                kind,
                content,
                metadata,
                fresh,
            } => {
                let role = match (*fresh, *kind) {
                    (false, 1) => ROLE_EXISTING_FILE,
                    (false, _) => ROLE_EXISTING_SYMLINK,
                    (true, 1) => ROLE_FRESH_FILE,
                    (true, _) => ROLE_FRESH_SYMLINK,
                };
                put_rooted_identity(&mut out, role, *serial, content, metadata)?;
            }
            PreparedIdentity::DirectoryPatch {
                serial,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => put_directory_identity(
                &mut out,
                ROLE_DIRECTORY_PATCH,
                *serial,
                *mode,
                *mtime_seconds,
                *mtime_nanoseconds,
            )?,
            PreparedIdentity::DirectoryDeclaration {
                serial,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => put_directory_identity(
                &mut out,
                ROLE_DIRECTORY_DECLARATION,
                *serial,
                *mode,
                *mtime_seconds,
                *mtime_nanoseconds,
            )?,
        }
    }
    Ok(out)
}

/// The request header of a prepared update with these rows.
pub fn header(
    snapshot: &BranchSnapshotWire,
    workspace: [u8; 32],
    generation: u64,
    directories: &[DirectoryChange],
    identities: &[PreparedIdentity],
) -> PreparedChanges {
    PreparedChanges {
        workspace,
        branch: snapshot.branch.branch,
        expected_head: snapshot.branch.head_commit,
        expected_base: snapshot.branch.base_layer,
        generation,
        base: snapshot.effective_root,
        scope: snapshot.scope,
        root_serial: snapshot.root_serial.unwrap_or(1),
        totals: totals(directories, identities),
    }
}
