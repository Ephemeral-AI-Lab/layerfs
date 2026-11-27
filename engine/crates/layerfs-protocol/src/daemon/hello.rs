//! Additive Hello fields within the existing Call/Answer control format.
use crate::{
    provision::install::ControlError,
    wire::{Reader, Writer},
};
pub(crate) fn put_request(out: &mut Writer, request: &HelloRequest) -> Result<(), ControlError> {
    if request.expected_instance == Some([0; 32]) {
        return Err(ControlError("daemon incarnation"));
    }
    out.byte(u8::from(request.expected_instance.is_some()))?;
    if let Some(instance) = request.expected_instance {
        out.put(&instance)?;
    }
    out.byte(u8::from(request.wait_for_store))
}
pub(crate) fn request(input: &mut Reader<'_>) -> Result<HelloRequest, ControlError> {
    let expected_instance = if boolean(input.byte()?)? {
        Some(input.array()?)
    } else {
        None
    };
    if expected_instance == Some([0; 32]) {
        return Err(ControlError("daemon incarnation"));
    }
    Ok(HelloRequest {
        expected_instance,
        wait_for_store: boolean(input.byte()?)?,
    })
}
pub(crate) fn put_status(out: &mut Writer, status: &DaemonStatus) -> Result<(), ControlError> {
    check(status)?;
    out.put(&status.instance)?;
    out.byte(status.phase as u8)?;
    out.blob(status.overlay_sqlite.as_deref().unwrap_or("").as_bytes())?;
    out.blob(status.store_sqlite.as_deref().unwrap_or("").as_bytes())
}
pub(crate) fn status(input: &mut Reader<'_>) -> Result<DaemonStatus, ControlError> {
    let instance = input.array()?;
    let phase = match input.byte()? {
        1 => DaemonPhase::InstallPending,
        2 => DaemonPhase::Installing,
        3 => DaemonPhase::Retained,
        4 => DaemonPhase::ControlReady,
        _ => return Err(ControlError("daemon phase")),
    };
    let overlay = input.text(64)?;
    let store = input.text(64)?;
    let value = DaemonStatus {
        instance,
        phase,
        overlay_sqlite: (!overlay.is_empty()).then_some(overlay),
        store_sqlite: (!store.is_empty()).then_some(store),
    };
    check(&value)?;
    Ok(value)
}
fn check(status: &DaemonStatus) -> Result<(), ControlError> {
    if status.instance == [0; 32]
        || [
            status.overlay_sqlite.as_deref(),
            status.store_sqlite.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|v| v.is_empty() || v.len() > 64 || !v.bytes().all(|b| b.is_ascii_graphic()))
    {
        return Err(ControlError("daemon observation facts"));
    }
    if status.phase == DaemonPhase::ControlReady
        && (status.overlay_sqlite.is_none() || status.store_sqlite.is_none())
    {
        return Err(ControlError("daemon readiness facts"));
    }
    Ok(())
}
fn boolean(value: u8) -> Result<bool, ControlError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ControlError("daemon boolean")),
    }
}

// Authenticated daemon startup observation, separate from kernel Workspace Ready.
/// Maintained application phase; observing it never settles an earlier unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DaemonPhase {
    /// Overlay/listener exist; no installed Store or control Service is claimed.
    InstallPending = 1,
    /// One original installation conversation is admitted.
    Installing = 2,
    /// Original startup failure/custody is retained, without a fallback open.
    Retained = 3,
    /// Actual direct Store and Overlay-backed control Service are published.
    ControlReady = 4,
}
/// One correlated startup observation or explicitly requested startup wait.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelloRequest {
    /// Previously observed incarnation, or None for first authenticated discovery.
    pub expected_instance: Option<[u8; 32]>,
    /// Wait for the one startup outcome event; no attempted operation is replayed.
    pub wait_for_store: bool,
}
/// Actual daemon lifecycle and provider facts, with no credentials or FUSE claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonStatus {
    /// Fresh process incarnation; unrelated to history/PID/time allocation.
    pub instance: [u8; 32],
    /// Maintained startup/service phase.
    pub phase: DaemonPhase,
    /// Actual initialized Overlay version when established.
    pub overlay_sqlite: Option<String>,
    /// Actual validated global Store version when established.
    pub store_sqlite: Option<String>,
}
