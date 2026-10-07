//! Bounded one-attempt file streaming; the host never opens the installed Store.
use crate::{InstallError, InstallFailure, InstallWork, Installed, SealedProject};
use layerfs_bridge::{
    control::{ControlError, InstallPhase, InstallReply, INSTALL_FINISH},
    native::{Connection, MAX_PLAINTEXT_BYTES},
};
use std::{
    fs::File,
    io::{self, Read},
};

/// Streams one sealed file over an already authenticated native connection.
/// A failed attempt fences that connection and retains original custody. A
/// successful connection remains usable for subsequent control commands.
pub fn install(
    project: &SealedProject,
    connection: &mut Connection,
) -> Result<Installed, Box<InstallFailure>> {
    let mut phase = InstallPhase::Request;
    let mut work = InstallWork::default();
    let mut request_attempted = false;
    let mut accepted = false;
    let mut acknowledged = None;
    let result = (|| {
        if project.manifest.daemon_sqlite.is_some() {
            return Err(InstallError::Protocol(ControlError(
                "already installed manifest",
            )));
        }
        let record = project.manifest.encode().map_err(InstallError::Protocol)?;
        let mut file = File::open(&project.store.path).map_err(InstallError::Io)?;
        let metadata = file.metadata().map_err(InstallError::Io)?;
        if !metadata.is_file()
            || metadata.len() != project.manifest.bytes
            || project.store.bytes != metadata.len()
        {
            return Err(InstallError::Protocol(ControlError(
                "sealed file length or kind",
            )));
        }
        request_attempted = true;
        connection
            .send
            .send(&record)
            .map_err(InstallError::Channel)?;
        match reply(connection)? {
            InstallReply::Ready => accepted = true,
            InstallReply::Refused(error) => return Err(InstallError::Remote(error)),
            InstallReply::Installed(value) => {
                acknowledged = Some(value);
                return Err(InstallError::Protocol(ControlError(
                    "unexpected installed acknowledgement",
                )));
            }
        }
        phase = InstallPhase::Transfer;
        let mut buffer = vec![0; MAX_PLAINTEXT_BYTES];
        work.buffer_bytes = buffer.len();
        let mut remaining = project.manifest.bytes;
        while remaining != 0 {
            let wanted = remaining.min(buffer.len() as u64) as usize;
            work.read_calls = work.read_calls.saturating_add(1);
            let size = file.read(&mut buffer[..wanted]).map_err(InstallError::Io)?;
            if size == 0 {
                return Err(InstallError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "sealed file shortened",
                )));
            }
            work.read_bytes = work.read_bytes.saturating_add(size as u64);
            connection
                .send
                .send(&buffer[..size])
                .map_err(InstallError::Channel)?;
            work.sent_records = work.sent_records.saturating_add(1);
            work.sent_bytes = work.sent_bytes.saturating_add(size as u64);
            remaining -= size as u64;
        }
        work.read_calls = work.read_calls.saturating_add(1);
        let extra = file.read(&mut buffer[..1]).map_err(InstallError::Io)?;
        work.read_bytes = work.read_bytes.saturating_add(extra as u64);
        if extra != 0 {
            return Err(InstallError::Protocol(ControlError("sealed file grew")));
        }
        connection
            .send
            .send(INSTALL_FINISH)
            .map_err(InstallError::Channel)?;
        phase = InstallPhase::Reply;
        match reply(connection)? {
            InstallReply::Installed(value) => {
                acknowledged = Some(value.clone());
                let mut expected = project.manifest.clone();
                expected.daemon_sqlite = value.daemon_sqlite.clone();
                if value.daemon_sqlite.is_none() || value != expected {
                    return Err(InstallError::Protocol(ControlError(
                        "installed manifest mismatch",
                    )));
                }
                Ok(value)
            }
            InstallReply::Refused(error) => Err(InstallError::Remote(error)),
            InstallReply::Ready => Err(InstallError::Protocol(ControlError(
                "duplicate install admission",
            ))),
        }
    })();
    match result {
        Ok(manifest) => Ok(Installed { manifest, work }),
        Err(error) => {
            let fence_error = if !request_attempted || matches!(error, InstallError::Channel(_)) {
                None
            } else {
                connection.send.close().err()
            };
            Err(Box::new(InstallFailure {
                source: project.store.path.clone(),
                manifest: project.manifest.clone(),
                phase,
                request_attempted,
                accepted,
                acknowledged,
                work,
                error,
                fence_error,
            }))
        }
    }
}
fn reply(connection: &mut Connection) -> Result<InstallReply, InstallError> {
    InstallReply::decode(
        connection
            .receive
            .receive()
            .map_err(InstallError::Channel)?,
    )
    .map_err(InstallError::Protocol)
}
