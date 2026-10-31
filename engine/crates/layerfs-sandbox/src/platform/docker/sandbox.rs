//! One owned container and its lifecycle: stop and delete, each attempted
//! once; the borrowed shared Store volume is never cleanup-owned.
use super::Docker;
use crate::{ContainerId, DockerRecord, RuntimeError, SandboxRecord, WireFailure};
use std::io::Read;
/// Explicit immutable image, existing VM volume, daemon TCP port and labels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxRequest {
    pub image: String,
    /// The Store volume mounted at `/layerfs-store`; None mounts nothing (a
    /// PostgreSQL Store, #506).
    pub store_volume: Option<String>,
    pub port: u16,
    /// Container labels (`layerfs.project`, for instance): each key once.
    /// A reopened container must carry every one; the image may add more.
    pub labels: Vec<(String, String)>,
}
/// A Docker volume name, short enough for the inspection to compare it whole.
fn volume_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphanumeric() || (i != 0 && matches!(b, b'_' | b'-' | b'.')))
}
/// A label key or value of plain characters that need no JSON escape.
fn label_text(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}
impl SandboxRequest {
    pub(super) fn check(&self) -> Result<(), RuntimeError> {
        let digest = self
            .image
            .strip_prefix("sha256:")
            .ok_or(RuntimeError::Protocol("immutable image digest"))?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.port == 0
        {
            return Err(RuntimeError::Protocol("image digest/daemon port"));
        }
        if self
            .store_volume
            .as_deref()
            .is_some_and(|v| !volume_name(v))
        {
            return Err(RuntimeError::Protocol("existing named VM volume"));
        }
        for (i, (key, value)) in self.labels.iter().enumerate() {
            if !label_text(key)
                || !label_text(value)
                || self.labels[..i].iter().any(|(k, _)| k == key)
            {
                return Err(RuntimeError::Protocol("container label"));
            }
        }
        Ok(())
    }
}
/// Original failed create retains request and partial decoded ID separately.
#[derive(Debug)]
pub struct SandboxCreateFailure {
    pub request: SandboxRequest,
    pub phase: CreatePhase,
    pub transfer: Box<WireFailure>,
}
/// A volume read has no create effect; Create may have an uncertain container effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreatePhase {
    Validate,
    Volume,
    Create,
}
/// One acknowledged container and its exact attempts. No Drop cleanup or replay.
#[derive(Debug)]
pub struct Sandbox {
    pub(super) docker: Docker,
    pub(super) id: ContainerId,
    pub(super) request: SandboxRequest,
    pub(super) upload_attempted: bool,
    pub(super) uploaded: bool,
    pub(super) start_attempted: bool,
    pub(super) started: bool,
    pub(super) listen_attempted: bool,
    pub(super) stop_attempted: bool,
    pub(super) stopped: bool,
    pub(super) delete_attempted: bool,
    pub(super) deleted: bool,
}
impl Sandbox {
    /// Original full acknowledged identity. Names and partial IDs are never adopted.
    pub fn identity(&self) -> ContainerId {
        self.id
    }
    /// Original immutable deployment request.
    pub fn request(&self) -> &SandboxRequest {
        &self.request
    }
    /// Positive complete original acknowledgments; failures leave uncertainty explicit.
    pub fn disposition(&self) -> SandboxDisposition {
        SandboxDisposition {
            uploaded: self.uploaded,
            started: self.started,
            stopped: self.stopped,
            deleted: self.deleted,
        }
    }
    pub(super) fn invalid(&self, message: &'static str) -> Box<WireFailure> {
        let mut f =
            super::client::http::failure(false, 0, None, RuntimeError::Protocol(message), None);
        f.requested_container = Some(self.id);
        f
    }
    pub(super) fn selected(&self, mut f: Box<WireFailure>) -> Box<WireFailure> {
        f.requested_container = Some(self.id);
        f
    }
    /// Explicit whole-Sandbox process stop, once. This is crash teardown, not graceful FS drain.
    /// The Engine's 304 (not running: its process already exited, or it never started)
    /// is the same acknowledged stopped state.
    pub fn stop(&mut self, seconds: u16) -> Result<(), Box<WireFailure>> {
        if self.stop_attempted || self.delete_attempted {
            return Err(self.invalid("original container Stop admission"));
        }
        self.stop_attempted = true;
        let path = format!("/containers/{}/stop?t={seconds}", self.id);
        self.empty("POST", &path, 204)
            .or_else(|failure| match failure.cause {
                RuntimeError::Http(304) => Ok(()),
                _ => Err(failure),
            })?;
        self.stopped = true;
        Ok(())
    }
    /// Deletes the exact acknowledged container/local writable layer once. Shared Store volume is borrowed.
    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        if self.delete_attempted || (self.start_attempted && !self.stopped) {
            return Err(self.invalid("original container Delete admission; known stop required"));
        }
        self.delete_attempted = true;
        self.empty(
            "DELETE",
            &format!("/containers/{}?force=false&v=false", self.id),
            204,
        )?;
        self.deleted = true;
        Ok(())
    }
    pub(super) fn empty(
        &self,
        method: &str,
        path: &str,
        status: u16,
    ) -> Result<(), Box<WireFailure>> {
        let mut response = self
            .docker
            .exchange(method, path, false, |_| Ok(()))
            .map_err(|f| self.selected(f))?;
        if response.status != status {
            return Err(self.selected(response.fail(RuntimeError::Http(response.status), None)));
        }
        let mut byte = [0];
        match response.body.read(&mut byte) {
            Ok(0) => Ok(()),
            Ok(_) => Err(self.selected(response.fail(
                RuntimeError::Protocol("unexpected lifecycle response body"),
                None,
            ))),
            Err(e) => Err(self.selected(response.fail(e.into(), None))),
        }
    }
}
/// Known acknowledgments only, not process or filesystem drain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SandboxDisposition {
    pub uploaded: bool,
    pub started: bool,
    pub stopped: bool,
    pub deleted: bool,
}
impl From<SandboxRequest> for DockerRecord {
    fn from(request: SandboxRequest) -> Self {
        Self {
            image: request.image,
            store_volume: request.store_volume,
            port: request.port,
            labels: request.labels,
        }
    }
}
impl From<&SandboxRequest> for DockerRecord {
    fn from(request: &SandboxRequest) -> Self {
        Self {
            image: request.image.clone(),
            store_volume: request.store_volume.clone(),
            port: request.port,
            labels: request.labels.clone(),
        }
    }
}
impl From<DockerRecord> for SandboxRequest {
    fn from(record: DockerRecord) -> Self {
        Self {
            image: record.image,
            store_volume: record.store_volume,
            port: record.port,
            labels: record.labels,
        }
    }
}
impl From<SandboxRequest> for SandboxRecord {
    fn from(request: SandboxRequest) -> Self {
        Self::Docker(request.into())
    }
}
impl From<&SandboxRequest> for SandboxRecord {
    fn from(request: &SandboxRequest) -> Self {
        Self::Docker(request.into())
    }
}
