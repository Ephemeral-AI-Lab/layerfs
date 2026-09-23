//! Host-bound project initialization through the ordinary authorized Service.
use crate::Service;
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
    time::{Duration, Instant},
};

impl Service {
    /// Import a host-visible directory into one genesis LayerStack.
    /// The authority identity and scope seed are generated inside this call.
    pub fn init_project(
        &self,
        peer: &VerifiedPeer,
        store: u32,
        name: &str,
        source: &Path,
    ) -> Result<StackCreatedWire, Failure> {
        let metadata = std::fs::symlink_metadata(source).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Failure::from(Code::InvalidInput)
            } else {
                error.into()
            }
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(Code::InvalidInput.into());
        }
        let source = source.canonicalize()?;
        let mut authority = [0u8; 56];
        File::open("/dev/urandom")?.read_exact(&mut authority)?;
        let request = Request {
            id: u64::from_le_bytes(authority[..8].try_into().unwrap()).max(1),
            generation: 1,
            store,
            profile: HISTORY_PROFILE,
            deadline_ms: MAX_OPERATION_MS,
            response_bytes: u64::from(MAX_OPERATION_MS.div_ceil(1_000)),
            operation: Operation::HistoryCommand(HistoryCommand::ImportNativeDirectory {
                stack: authority[8..24].try_into().unwrap(),
                name: name.as_bytes().to_vec(),
                scope_seed: authority[24..].try_into().unwrap(),
            }),
        };
        let mut output = Vec::new();
        let (result, _) = self.handle_until_bound(
            peer,
            &request,
            &mut Cursor::new(&[]),
            &mut output,
            Instant::now() + Duration::from_millis(u64::from(MAX_OPERATION_MS)),
            Some(&source),
        );
        match result? {
            Response::History(result) => match *result {
                HistoryResult::StackCreated(created) => Ok(created),
                _ => Err(Code::Integrity.into()),
            },
            _ => Err(Code::Integrity.into()),
        }
    }
}
