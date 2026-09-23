//! Own the host authority behind an ordinary borrowed SDK client.
use crate::Client;
use layerfs_bridge::{
    adapters::native::{connection::VerifiedPeer, pipe::key},
    contract::{permission_bit, COMMAND_OPCODE},
};
use layerfs_history::{sqlite, HistoryCatalog, HistoryCatalogConfig};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{error::Error, path::Path, sync::Arc};

/// One newly created host Store and history authority for project Init.
pub struct Host {
    service: Service,
    peer: VerifiedPeer,
}

impl Host {
    /// Create a fresh local authority. Setup finishes before an Init timer starts.
    pub fn create(
        store_path: &Path,
        history_path: &Path,
        binding_key: &[u8],
        private_key_hex: &str,
        cursor_key_hex: &str,
    ) -> Result<Self, Box<dyn Error>> {
        let private = key(private_key_hex)?;
        let cursor_key = key(cursor_key_hex)?;
        let peer = VerifiedPeer::from_private(&private)?;
        let store = Timing::disabled("create", |scope| {
            Store::create(store_path, Store::default_policy(), scope.child("store"))
        })
        .0?;
        let history: Arc<dyn HistoryCatalog> = Arc::new(sqlite::create(
            history_path,
            &HistoryCatalogConfig {
                binding_key: binding_key.to_vec(),
                incarnation: 1,
                cursor_key,
            },
        )?);
        let service = Service::new(
            vec![StoreAccess {
                id: 1,
                store,
                history: Some(history),
                grants: vec![Grant {
                    public_key: *peer.public_key(),
                    operations: permission_bit(COMMAND_OPCODE).expect("history command grant"),
                    expires_unix: u64::MAX,
                }],
            }],
            OperationRecorder::disabled(),
        )?;
        Ok(Self { service, peer })
    }

    /// Borrow the authorized agent-facing client.
    pub fn client(&self) -> Client<'_> {
        Client::new(&self.service, &self.peer, 1)
    }
}
