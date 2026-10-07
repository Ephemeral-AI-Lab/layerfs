//! Authenticated one-time file installation and concrete Store bootstrap.
use crate::{
    bootstrap::open_store_observed,
    install_file,
    install_types::{InstallError, InstallFailure, InstallWork, InstalledStore, StoreSettings},
    store::checked_base,
};
use layerfs_bridge::{
    control::{ControlError, InstallPhase, InstallReply, INSTALL_FINISH},
    native::Connection,
    provision::{StoreManifest, StoreProfile},
};
use layerfs_history::{BranchId, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use std::{fs, path::Path, sync::Arc};

/// Installs into the explicitly provisioned local destination. No caller-supplied
/// locator can select another file. Every failure retains partial/output custody.
pub fn receive_install(
    connection: &mut Connection,
    destination: &Path,
    settings: StoreSettings,
) -> Result<InstalledStore, Box<InstallFailure>> {
    let mut retained = InstallFailure {
        destination: destination.to_owned(),
        temporary: None,
        claim_acknowledged: false,
        manifest: None,
        phase: InstallPhase::Request,
        published: false,
        opened: None,
        work: InstallWork::default(),
        error: InstallError::Protocol(ControlError("incomplete install")),
        reply_error: None,
        fence_error: None,
    };
    let result = (|| {
        let mut manifest = StoreManifest::decode(
            connection
                .receive
                .receive()
                .map_err(InstallError::Channel)?,
        )
        .map_err(InstallError::Protocol)?;
        retained.manifest = Some(manifest.clone());
        if Path::new(&manifest.locator) != destination || manifest.daemon_sqlite.is_some() {
            return Err(InstallError::Protocol(ControlError(
                "provisioned install destination",
            )));
        }
        let branch = BranchId::from_bytes(manifest.branch).map_err(InstallError::History)?;
        let stack = LayerStackId::from_bytes(manifest.stack).map_err(InstallError::History)?;
        settings
            .reservations
            .validate()
            .map_err(InstallError::Store)?;
        if settings.read_handles == 0 {
            return Err(InstallError::Protocol(ControlError("empty Store read set")));
        }
        let (target, temporary) = install_file::paths(destination).map_err(InstallError::Io)?;
        retained.destination = target.clone();
        retained.phase = InstallPhase::Claim;
        // Record the exact claim name before attempting create_new; existence is
        // never interpreted as evidence that this attempt created that file.
        retained.temporary = Some(temporary.clone());
        let mut file = install_file::claim(&target, &temporary).map_err(InstallError::Io)?;
        retained.claim_acknowledged = true;
        // A previous owner may have renamed before our create_new. While this
        // acknowledged temporary name exists, another installer cannot own it.
        install_file::absent(&target).map_err(InstallError::Io)?;
        send(connection, InstallReply::Ready)?;
        retained.phase = InstallPhase::Transfer;
        while retained.work.written < manifest.bytes {
            let bytes = connection
                .receive
                .receive()
                .map_err(InstallError::Channel)?;
            if bytes.is_empty() || bytes.len() as u64 > manifest.bytes - retained.work.written {
                return Err(InstallError::Protocol(ControlError(
                    "install stream length",
                )));
            }
            retained.work.records = retained.work.records.saturating_add(1);
            retained.work.peak_record_bytes = retained.work.peak_record_bytes.max(bytes.len());
            install_file::write(&mut file, bytes, &mut retained.work).map_err(InstallError::Io)?;
        }
        if connection
            .receive
            .receive()
            .map_err(InstallError::Channel)?
            != INSTALL_FINISH
        {
            return Err(InstallError::Protocol(ControlError(
                "install finish marker",
            )));
        }
        if manifest.profile == StoreProfile::Durable {
            retained.work.sync_calls += 1;
            file.sync_all().map_err(InstallError::Io)?;
        }
        drop(file);
        retained.phase = InstallPhase::Publish;
        fs::rename(&temporary, &target).map_err(InstallError::Io)?;
        retained.published = true;
        if manifest.profile == StoreProfile::Durable {
            retained.work.sync_calls += 1;
            fs::File::open(target.parent().expect("checked parent"))
                .and_then(|parent| parent.sync_all())
                .map_err(InstallError::Io)?;
        }
        retained.phase = InstallPhase::Open;
        let profile = match manifest.profile {
            StoreProfile::Durable => SqlitePersistenceProfile::Durable,
            StoreProfile::Disposable => SqlitePersistenceProfile::Disposable,
        };
        retained.opened = Some(
            open_store_observed(
                PersistenceConfig::sqlite(target).with_sqlite_profile(profile),
                &manifest.binding,
                manifest.cursor_key,
                settings.read_handles,
                settings.cache_bytes,
                settings.reservations,
            )
            .map_err(InstallError::Store)?,
        );
        let opened = retained.opened.as_ref().expect("opened Store");
        let snapshot = opened
            .store
            .history()
            .branch_snapshot(branch)
            .map_err(InstallError::History)?
            .ok_or(InstallError::Protocol(ControlError(
                "installed Branch missing",
            )))?;
        if snapshot.branch.stack != stack
            || snapshot.effective_root.to_bytes() != manifest.root
            || snapshot.branch.head_commit.is_some()
        {
            return Err(InstallError::Protocol(ControlError(
                "installed initial Branch",
            )));
        }
        let ports = opened.store.ports(snapshot.scope);
        checked_base(ports.client(), &snapshot).map_err(|error| InstallError::Content {
            error,
            provider: ports
                .failure()
                .unwrap_or_else(|error| Some(Arc::new(error))),
        })?;
        manifest.daemon_sqlite = Some(opened.sqlite_version.clone());
        retained.manifest = Some(manifest.clone());
        retained.phase = InstallPhase::Reply;
        send(connection, InstallReply::Installed(manifest))?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(InstalledStore {
            manifest: retained.manifest.expect("acknowledged manifest"),
            opened: retained.opened.expect("opened Store"),
            work: retained.work,
        }),
        Err(error) => {
            retained.error = error;
            if !matches!(retained.error, InstallError::Channel(_)) {
                retained.reply_error =
                    send(connection, InstallReply::Refused(retained.refusal())).err();
                if !matches!(retained.reply_error, Some(InstallError::Channel(_))) {
                    retained.fence_error = connection.send.close().err();
                }
            }
            Err(Box::new(retained))
        }
    }
}
fn send(connection: &mut Connection, reply: InstallReply) -> Result<(), InstallError> {
    connection
        .send
        .send(&reply.encode().map_err(InstallError::Protocol)?)
        .map_err(InstallError::Channel)
}
