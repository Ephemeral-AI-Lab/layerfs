//! Exact one-time installation admission and original Store-to-Service transfer.
use super::{Application, ApplicationError, ConnectionFailure};
use crate::install_types::StoreSettings;
use layerfs_bridge::{
    control::{DaemonPhase, InstallPhase, InstallRefusal, InstallReply, RefusalKind},
    native::Connection,
    provision::StoreManifest,
};
use layerfs_storage::ReservationBlocks;
use std::path::Path;

impl Application {
    pub(super) fn install(
        &self,
        connection: &mut Connection,
        manifest: StoreManifest,
        slot: usize,
    ) -> Result<bool, ConnectionFailure> {
        let admitted = {
            let mut state = self.state.lock().map_err(|_| self.poisoned(slot))?;
            match state.startup.phase {
                DaemonPhase::InstallPending => {
                    state.startup.phase = DaemonPhase::Installing;
                    true
                }
                _ => false,
            }
        };
        if !admitted {
            let refusal = InstallReply::Refused(InstallRefusal {
                phase: InstallPhase::Request,
                kind: RefusalKind::Exists,
                claim_acknowledged: false,
                written: 0,
                published: false,
                os_code: None,
                detail: "one original installer already admitted; no resume/replay".into(),
            });
            let bytes = refusal.encode().map_err(|cause| ConnectionFailure {
                slot,
                received: None,
                original: None,
                cause: ApplicationError::Protocol(cause),
                fence_error: None,
                diagnostic_error: None,
            })?;
            connection
                .send
                .send(&bytes)
                .map_err(|cause| ConnectionFailure {
                    slot,
                    received: None,
                    original: None,
                    cause: ApplicationError::Channel(cause),
                    fence_error: None,
                    diagnostic_error: None,
                })?;
            connection.send.close().map_err(|cause| ConnectionFailure {
                slot,
                received: None,
                original: None,
                cause: ApplicationError::Channel(cause),
                fence_error: None,
                diagnostic_error: None,
            })?;
            return Ok(false);
        }
        let l = self.setup.limits;
        let settings = StoreSettings {
            read_handles: usize::from(l.read_handles),
            cache_bytes: usize::try_from(l.cache_bytes).map_err(|_| self.poisoned(slot))?,
            reservations: ReservationBlocks::default(),
            read_limits: super::config::read_limits(l),
        };
        let result = crate::install::receive_install_manifest(
            connection,
            Path::new(&self.setup.store),
            settings,
            manifest,
        );
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                return Err(ConnectionFailure {
                    slot,
                    received: None,
                    original: None,
                    cause: ApplicationError::InstallPublication(Box::new(result)),
                    fence_error: None,
                    diagnostic_error: None,
                })
            }
        };
        match result {
            Ok(installed) => {
                state.startup.service = Some(super::filesystem::service(
                    installed.opened.store.clone(),
                    &self.owner,
                    self.native.as_ref(),
                    l.serial_low_water,
                ));
                state.startup.installed = Some(installed);
                state.startup.phase = DaemonPhase::ControlReady;
                self.changed.notify_all();
                Ok(true)
            }
            Err(failure) => {
                // Only the original final-ack boundary establishes completed
                // Branch/root validation. Open/published alone is insufficient.
                let ready = failure.phase == InstallPhase::Reply
                    && failure.published
                    && failure
                        .manifest
                        .as_ref()
                        .is_some_and(|m| m.daemon_sqlite.is_some())
                    && failure.opened.is_some();
                if ready {
                    let opened = failure.opened.as_ref().expect("validated original Store");
                    state.startup.service = Some(super::filesystem::service(
                        opened.store.clone(),
                        &self.owner,
                        self.native.as_ref(),
                        l.serial_low_water,
                    ));
                    state.startup.phase = DaemonPhase::ControlReady;
                } else {
                    state.startup.phase = DaemonPhase::Retained;
                }
                state.startup.install_failure = Some(failure);
                self.changed.notify_all();
                // The original installer already owns its reply/fence result.
                // Keep it in startup custody, and stop this conversation once.
                Ok(false)
            }
        }
    }
    fn poisoned(&self, slot: usize) -> ConnectionFailure {
        ConnectionFailure {
            slot,
            received: None,
            original: None,
            cause: ApplicationError::Poisoned,
            fence_error: None,
            diagnostic_error: None,
        }
    }
}
