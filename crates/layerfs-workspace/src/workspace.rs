use crate::{
    quiescence, BeginOperationReceipt, EndOperationReceipt, FinalizedCandidate, OperationState,
    Presentation, Result, RuntimeLeases, WorkspaceDriver, WorkspaceError, WorkspaceTicket,
};
use layerfs_core::ObjectId;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::Instant;

#[cfg(feature = "test-hooks")]
std::thread_local! {
    static FAIL_REMOVE_OWNED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub fn inject_remove_owned_failure_for_test() {
    FAIL_REMOVE_OWNED.set(true);
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FreezeObservation {
    pub quiescence_ns: u128,
    pub driver_freeze_ns: u128,
}

const OWNER_MAGIC: &[u8; 8] = b"LFSWORK1";

#[derive(Debug)]
pub struct WorkspacePaths {
    root: PathBuf,
    owner: PathBuf,
    recovery: PathBuf,
    view: PathBuf,
    spool: PathBuf,
    marker: Vec<u8>,
    identity: [u64; 2],
}

impl WorkspacePaths {
    pub fn create(working_root: &Path, ticket: &WorkspaceTicket) -> Result<Self> {
        reject_link(working_root)?;
        let workspaces = working_root.join("workspaces");
        fs::create_dir_all(&workspaces)?;
        reject_link(&workspaces)?;
        set_private(&workspaces)?;
        let name = format!(
            "{}-{}",
            hex(ticket.operation_id.as_bytes()),
            hex(&ticket.nonce)
        );
        let root = workspaces.join(name);
        fs::create_dir(&root)?;
        let identity = directory_identity(&root)?;
        let result = Self::initialize(root.clone(), identity, ticket);
        if result.is_err() {
            let _ = remove_setup_root(&root, identity);
        }
        result
    }

    fn initialize(root: PathBuf, identity: [u64; 2], ticket: &WorkspaceTicket) -> Result<Self> {
        set_private(&root)?;
        let owner = root.join("owner");
        let recovery = root.join("recovery");
        let view = root.join("view");
        let spool = root.join("spool");
        fs::create_dir(&view)?;
        fs::create_dir(&spool)?;
        set_private(&view)?;
        set_private(&spool)?;
        let marker = marker(ticket);
        write_new(&owner, &marker)?;
        write_new(&recovery, &marker)?;
        let paths = Self {
            root,
            owner,
            recovery,
            view,
            spool,
            marker,
            identity,
        };
        paths.validate()?;
        Ok(paths)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn view(&self) -> &Path {
        &self.view
    }

    pub fn spool(&self) -> &Path {
        &self.spool
    }

    pub fn recovery(&self) -> &Path {
        &self.recovery
    }

    pub fn validate(&self) -> Result<()> {
        if directory_identity(&self.root)? != self.identity {
            return Err(WorkspaceError::OwnershipMismatch);
        }
        for path in [&self.root, &self.view, &self.spool] {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(WorkspaceError::OwnershipMismatch);
            }
        }
        let owner = fs::symlink_metadata(&self.owner)?;
        let recovery = fs::symlink_metadata(&self.recovery)?;
        if owner.file_type().is_symlink()
            || !owner.is_file()
            || recovery.file_type().is_symlink()
            || !recovery.is_file()
            || fs::read(&self.owner)? != self.marker
            || fs::read(&self.recovery)? != self.marker
        {
            return Err(WorkspaceError::OwnershipMismatch);
        }
        Ok(())
    }

    pub fn remove_owned(&mut self) -> Result<()> {
        #[cfg(feature = "test-hooks")]
        if FAIL_REMOVE_OWNED.replace(false) {
            return Err(WorkspaceError::OwnershipMismatch);
        }
        self.validate()?;
        let parent = self
            .root
            .parent()
            .ok_or(WorkspaceError::OwnershipMismatch)?;
        let name = self
            .root
            .file_name()
            .ok_or(WorkspaceError::OwnershipMismatch)?
            .to_string_lossy();
        let quarantine = parent.join(format!(".{name}.removing"));
        if fs::symlink_metadata(&quarantine).is_ok() {
            return Err(WorkspaceError::OwnershipMismatch);
        }
        fs::rename(&self.root, &quarantine)?;
        self.root = quarantine;
        self.owner = self.root.join("owner");
        self.recovery = self.root.join("recovery");
        self.view = self.root.join("view");
        self.spool = self.root.join("spool");
        self.validate()?;
        fs::remove_dir_all(&self.root)?;
        Ok(())
    }
}

pub struct OperationWorkspace<D> {
    ticket: WorkspaceTicket,
    state: OperationState,
    driver: D,
    paths: Option<WorkspacePaths>,
    leases: RuntimeLeases,
    candidate_root: Option<ObjectId>,
}

impl<D: WorkspaceDriver> OperationWorkspace<D> {
    pub fn start(
        ticket: WorkspaceTicket,
        driver: D,
        paths: Option<WorkspacePaths>,
    ) -> Result<(Self, BeginOperationReceipt)> {
        if ticket.presentation != driver.presentation()
            || (ticket.presentation == Presentation::Direct) != paths.is_none()
            || driver
                .view_path()
                .is_some_and(|view| paths.as_ref().is_none_or(|paths| paths.view() != view))
        {
            return Err(WorkspaceError::InvalidState);
        }
        if let Some(paths) = &paths {
            paths.validate()?;
        }
        let receipt = BeginOperationReceipt {
            operation_id: ticket.operation_id,
            working_storage_id: ticket.working_storage_id,
            expected_branch_generation: ticket.expected_branch_generation,
            base_root: ticket.base_root,
            presentation: ticket.presentation,
            state: OperationState::Active,
        };
        Ok((
            Self {
                ticket,
                state: OperationState::Active,
                driver,
                paths,
                leases: RuntimeLeases::default(),
                candidate_root: None,
            },
            receipt,
        ))
    }

    pub fn state(&self) -> OperationState {
        self.state
    }

    pub fn leases(&self) -> &RuntimeLeases {
        &self.leases
    }

    pub fn paths(&self) -> Option<&WorkspacePaths> {
        self.paths.as_ref()
    }

    pub fn driver(&self) -> &D {
        &self.driver
    }

    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    pub fn freeze(&mut self, timeout: Duration) -> Result<()> {
        self.freeze_observed(timeout).map(drop)
    }

    pub fn freeze_observed(&mut self, timeout: Duration) -> Result<FreezeObservation> {
        if self.state != OperationState::Active {
            return Err(WorkspaceError::InvalidState);
        }
        let quiescence = Instant::now();
        if quiescence::establish(&self.leases, timeout).is_err() {
            self.state = OperationState::Incomplete;
            return Err(WorkspaceError::Timeout);
        }
        if let Err(error) = self.driver.quiesce(timeout) {
            self.state = OperationState::Incomplete;
            return Err(error);
        }
        let quiescence_ns = quiescence.elapsed().as_nanos();
        let driver_freeze = Instant::now();
        if let Err(error) = self.driver.freeze() {
            self.state = OperationState::Incomplete;
            return Err(error);
        }
        let driver_freeze_ns = driver_freeze.elapsed().as_nanos();
        self.state = OperationState::Frozen;
        Ok(FreezeObservation {
            quiescence_ns,
            driver_freeze_ns,
        })
    }

    /// Binds a candidate already constructed by `layerfs-core::logical`; this
    /// function deliberately performs no Branch publication or synchronization.
    pub fn finalize_candidate(
        &mut self,
        base_root: ObjectId,
        candidate_root: ObjectId,
        normalized_transition: Vec<u8>,
    ) -> Result<FinalizedCandidate> {
        if self.state != OperationState::Frozen || base_root != self.ticket.base_root {
            return Err(WorkspaceError::InvalidState);
        }
        self.state = OperationState::Finalized;
        self.candidate_root = Some(candidate_root);
        Ok(FinalizedCandidate {
            operation_id: self.ticket.operation_id,
            expected_branch_generation: self.ticket.expected_branch_generation,
            base_root,
            candidate_root,
            normalized_transition,
        })
    }

    pub fn cleanup(&mut self) -> Result<EndOperationReceipt> {
        if !matches!(
            self.state,
            OperationState::Active
                | OperationState::Frozen
                | OperationState::Finalized
                | OperationState::Incomplete
        ) {
            return Err(WorkspaceError::InvalidState);
        }
        let runtime_terminal = self.leases.observation()?;
        if runtime_terminal != Default::default() {
            return Err(WorkspaceError::Busy);
        }
        self.driver.cleanup()?;
        if let Some(paths) = self.paths.as_mut() {
            paths.remove_owned()?;
        }
        self.paths = None;
        self.state = OperationState::Cleaned;
        Ok(EndOperationReceipt {
            operation_id: self.ticket.operation_id,
            state: self.state,
            candidate_root: self.candidate_root,
            runtime_terminal,
            cleanup_complete: true,
        })
    }

    pub fn discard(&mut self) -> Result<EndOperationReceipt> {
        self.cleanup()
    }
}

fn marker(ticket: &WorkspaceTicket) -> Vec<u8> {
    let mut marker = Vec::with_capacity(8 + 32 + 32 + 16);
    marker.extend_from_slice(OWNER_MAGIC);
    marker.extend_from_slice(&ticket.working_storage_id);
    marker.extend_from_slice(ticket.operation_id.as_bytes());
    marker.extend_from_slice(&ticket.nonce);
    marker
}

fn remove_setup_root(root: &Path, identity: [u64; 2]) -> Result<()> {
    if directory_identity(root)? != identity {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    let parent = root.parent().ok_or(WorkspaceError::OwnershipMismatch)?;
    let name = root
        .file_name()
        .ok_or(WorkspaceError::OwnershipMismatch)?
        .to_string_lossy();
    let quarantine = parent.join(format!(".{name}.setup-removing"));
    if fs::symlink_metadata(&quarantine).is_ok() {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    fs::rename(root, &quarantine)?;
    if directory_identity(&quarantine)? != identity {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    fs::remove_dir_all(quarantine)?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn reject_link(path: &Path) -> Result<()> {
    if path.exists() && fs::symlink_metadata(path)?.file_type().is_symlink() {
        Err(WorkspaceError::OwnershipMismatch)
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> Result<[u64; 2]> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(WorkspaceError::OwnershipMismatch);
    }
    Ok([metadata.dev(), metadata.ino()])
}

#[cfg(not(unix))]
fn directory_identity(_path: &Path) -> Result<[u64; 2]> {
    Err(WorkspaceError::OwnershipMismatch)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn setup_failure_cleanup_preserves_a_substituted_root() {
        let parent = std::env::temp_dir().join(format!(
            "layerfs-workspace-setup-substitution-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&parent);
        fs::create_dir(&parent).unwrap();
        let root = parent.join("owned");
        fs::create_dir(&root).unwrap();
        let identity = directory_identity(&root).unwrap();
        let displaced = parent.join("displaced");
        fs::rename(&root, &displaced).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("keep"), b"substitute").unwrap();

        assert!(matches!(
            remove_setup_root(&root, identity),
            Err(WorkspaceError::OwnershipMismatch)
        ));
        assert!(root.join("keep").exists());
        assert!(displaced.exists());
        fs::remove_dir_all(parent).unwrap();
    }
}

#[cfg(unix)]
fn set_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private(_path: &Path) -> Result<()> {
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        },
    )
}
