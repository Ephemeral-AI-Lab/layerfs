use super::state::*;
use crate::backing::{
    directory::Directory,
    payload::{bare_failure, PayloadHost},
};
use crate::{
    backing::budget::{Budget, Charge},
    filesystem::namespace::attributes,
    *,
};
use layerfs_bridge::contract::{
    HistoryQuery, HistoryResult, Inspect, Operation, Request, Response, Source,
};
use std::{
    fs,
    io::Write,
    mem::size_of,
    path::{Component, Path},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

#[derive(Clone)]
pub struct WorkspaceHost {
    pub(crate) inner: Arc<Host>,
}
pub(crate) struct Host {
    pub config: WorkspaceConfig,
    pub budget: Arc<Budget>,
    pub remote: AtomicBool,
    pub frozen: Arc<AtomicBool>,
    pub deliver: OperationDelivery,
    pub request_id: AtomicU64,
    pub registry: Mutex<Registry>,
    pub payloads: Option<Arc<PayloadHost>>,
    pub metadata: Option<Arc<crate::backing::metadata::MetadataHost>>,
    pub _charge: Charge,
}
pub(crate) struct Entry {
    pub id: Box<str>,
    pub incarnation: [u8; 32],
    pub state: Option<Arc<Inner>>,
    pub _directory: Option<Arc<Directory>>,
    pub _name_charge: Charge,
}
pub(crate) struct Registry {
    pub entries: Vec<Entry>,
    capacity_charge: Charge,
}
impl Registry {
    fn reserve_one(
        &mut self,
        budget: &Arc<Budget>,
        max_count: usize,
    ) -> Result<(), WorkspaceError> {
        if self.entries.len() < self.entries.capacity() {
            return Ok(());
        }
        let capacity = self
            .entries
            .capacity()
            .checked_mul(2)
            .unwrap_or(max_count)
            .max(1)
            .min(max_count);
        let bytes = capacity
            .checked_mul(size_of::<Entry>())
            .ok_or(WorkspaceError::Capacity)?;
        // Both old and replacement allocation remain charged throughout the move.
        let mut charge = budget.reserve(bytes)?;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(capacity)
            .map_err(|_| WorkspaceError::Capacity)?;
        charge.resize(
            entries
                .capacity()
                .checked_mul(size_of::<Entry>())
                .ok_or(WorkspaceError::Capacity)?,
        )?;
        entries.append(&mut self.entries);
        drop(std::mem::replace(&mut self.entries, entries));
        self.capacity_charge = charge;
        Ok(())
    }
}
struct Remote<'a>(&'a AtomicBool);
impl Drop for Remote<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl WorkspaceHost {
    pub fn new(
        mut config: WorkspaceConfig,
        deliver: OperationDelivery,
    ) -> Result<Self, WorkspaceError> {
        if !cfg!(unix) {
            return Err(WorkspaceError::Unsupported);
        }
        if config.max_count == 0
            || config.memory_budget_bytes == 0
            || !config.root.is_absolute()
            || config.root.as_os_str().len() > PATH_BYTES
            || config
                .root
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
        {
            return Err(WorkspaceError::InvalidInput);
        }
        // The caller can supply a small path with a much larger spare allocation.
        config.root = config.root.into_boxed_path().into_path_buf();
        let budget = Budget::new(config.memory_budget_bytes);
        let minimum = 16384usize
            .checked_add(
                size_of::<Entry>()
                    + 63
                    + 8192
                    + CALL_SCRATCH
                    + MAX_READ_BYTES
                    + NODE_LIMIT * size_of::<Node>()
                    + HANDLE_LIMIT * size_of::<Handle>()
                    + COOKIE_LIMIT * size_of::<Cookie>(),
            )
            .ok_or(WorkspaceError::Capacity)?;
        let minimum = minimum
            .checked_add(if config.disk_budget_bytes.is_some() {
                5 * crate::backing::segments::WINDOW_BYTES + 6 * 8192
            } else {
                0
            })
            .ok_or(WorkspaceError::Capacity)?;
        if minimum > config.memory_budget_bytes {
            return Err(WorkspaceError::Capacity);
        }
        let charge = budget.reserve(16384)?;
        let payloads = config
            .disk_budget_bytes
            .map(|quota| PayloadHost::new(config.root.join("private-backing"), quota, &budget))
            .transpose()?;
        let metadata = payloads
            .as_ref()
            .map(|p| crate::backing::metadata::MetadataHost::new(p.clone()))
            .transpose()?;
        let registry = Registry {
            entries: Vec::new(),
            capacity_charge: budget.reserve(0)?,
        };
        secure_directory(&config.root, false)?;
        secure_directory(&config.root.join("workspace"), true)?;
        Ok(Self {
            inner: Arc::new(Host {
                config,
                budget,
                remote: AtomicBool::new(false),
                frozen: Arc::new(AtomicBool::new(false)),
                deliver,
                request_id: AtomicU64::new(1),
                registry: Mutex::new(registry),
                payloads,
                metadata,
                _charge: charge,
            }),
        })
    }

    pub fn attach(
        &self,
        options: AttachOptions,
        deadline: Instant,
    ) -> Result<Workspace, WorkspaceError> {
        validate_id(&options.id)?;
        if options.access == WorkspaceAccess::LocalEdit
            && (!matches!(options.base, Base::Branch(_)) || self.inner.metadata.is_none())
        {
            return Err(WorkspaceError::Unsupported);
        }
        if options.incarnation == [0; 32] {
            return Err(WorkspaceError::InvalidInput);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if fs::metadata(self.inner.config.root.join("workspace"))?.uid() != options.owner_uid
                || fs::metadata(&self.inner.config.root)?.uid() != options.owner_uid
            {
                return Err(WorkspaceError::Unsupported);
            }
            for ancestor in self.inner.config.root.ancestors() {
                let metadata = fs::metadata(ancestor)?;
                if (metadata.uid() != 0 && metadata.uid() != options.owner_uid)
                    || (metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0)
                {
                    return Err(WorkspaceError::Denied);
                }
            }
        }
        let directory = {
            let mut registry = self.inner.registry.lock().map_err(|_| WorkspaceError::Io)?;
            if registry.entries.len() == self.inner.config.max_count {
                return Err(WorkspaceError::Capacity);
            }
            if registry.entries.iter().any(|entry| {
                entry.id.as_ref() == options.id || entry.incarnation == options.incarnation
            }) {
                return Err(WorkspaceError::Busy);
            }
            let name_charge = self.inner.budget.reserve(options.id.len())?;
            registry.reserve_one(&self.inner.budget, self.inner.config.max_count)?;
            let directory = self
                .inner
                .payloads
                .as_ref()
                .map(|host| {
                    host.directory(
                        self.inner
                            .config
                            .root
                            .join("private-backing")
                            .join(&options.id),
                        options.incarnation,
                    )
                })
                .transpose()?;
            registry.entries.push(Entry {
                id: options.id.as_str().into(),
                incarnation: options.incarnation,
                state: None,
                _directory: directory.clone(),
                _name_charge: name_charge,
            });
            directory
        };
        let mut path = self.inner.config.root.join("workspace");
        path.push(&options.id);
        let mut owned_directory = false;
        let mut result = (|| {
            let _scratch = self.inner.budget.reserve(CALL_SCRATCH)?;
            self.inner
                .remote
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| WorkspaceError::Busy)?;
            let _remote = Remote(&self.inner.remote);
            let deadline = Workspace::callback_deadline(deadline);
            let (base, expected_serial, branch_snapshot) = match &options.base {
                Base::Root(root) => (*root, None, None),
                Base::Branch(branch) => {
                    let result = self.inner.call(
                        options.store,
                        Operation::HistoryQuery(HistoryQuery::GetBranch { branch: *branch }),
                        16384,
                        &mut std::io::sink(),
                        deadline,
                    )?;
                    let Response::History(result) = result else {
                        return Err(WorkspaceError::InvalidInput);
                    };
                    let HistoryResult::BranchSnapshot(snapshot) = *result else {
                        return Err(WorkspaceError::InvalidInput);
                    };
                    if snapshot.branch.branch.as_slice() != branch.as_slice()
                        || snapshot.root_serial.is_none()
                    {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    (
                        snapshot.effective_root,
                        snapshot.root_serial,
                        Some(snapshot),
                    )
                }
            };
            let result = self.inner.call(
                options.store,
                Operation::Inspect {
                    root: base,
                    query: Inspect::Attributes { path: Vec::new() },
                },
                0,
                &mut std::io::sink(),
                deadline,
            )?;
            let (attr, content, metadata) =
                attributes(result, true, options.owner_uid, options.owner_gid)?;
            if expected_serial.is_some_and(|serial| serial != attr.serial) {
                return Err(WorkspaceError::InvalidInput);
            }
            let charge = self.inner.budget.reserve(8192)?;
            let tables = self.inner.budget.reserve(
                NODE_LIMIT * size_of::<Node>()
                    + HANDLE_LIMIT * size_of::<Handle>()
                    + COOKIE_LIMIT * size_of::<Cookie>(),
            )?;
            let mut nodes = Vec::new();
            let mut handles = Vec::new();
            let mut cookies = Vec::new();
            nodes
                .try_reserve_exact(NODE_LIMIT)
                .map_err(|_| WorkspaceError::Capacity)?;
            handles
                .try_reserve_exact(HANDLE_LIMIT)
                .map_err(|_| WorkspaceError::Capacity)?;
            cookies
                .try_reserve_exact(COOKIE_LIMIT)
                .map_err(|_| WorkspaceError::Capacity)?;
            nodes.push(Node::new(attr, content, metadata, &[], attr.serial));
            if let (Some(host), Some(directory)) = (&self.inner.payloads, &directory) {
                host.initialize(directory)
                    .map_err(|error| bare_failure(BackingPhase::Acquire, error.kind()))?;
            }
            create_owned_directory(&path)?;
            owned_directory = true;
            let arena = if options.access == WorkspaceAccess::LocalEdit {
                Some(
                    self.inner
                        .metadata
                        .as_ref()
                        .ok_or(WorkspaceError::Unsupported)?
                        .arena(directory.clone().ok_or(WorkspaceError::Unsupported)?)?,
                )
            } else {
                None
            };
            let inner = Arc::new(Inner {
                id: options.id.clone(),
                incarnation: options.incarnation,
                store: options.store,
                base,
                access: options.access,
                branch: branch_snapshot.map(Arc::new),
                arena,
                root: attr,
                mount_path: path.clone().into_boxed_path().into_path_buf(),
                directory: directory.clone(),
                stopping: AtomicBool::new(false),
                _charge: charge,
                state: Mutex::new(State {
                    nodes,
                    overlay: None,
                    completion: None,
                    submission: None,
                    generation: 1,
                    revision: 0,
                    dirty_inodes: 0,
                    handles,
                    cookies,
                    next_handle: 1,
                    next_cookie: 1,
                    mounted: false,
                    closed: false,
                    active: 0,
                    tables: Some(tables),
                }),
            });
            let mut registry = self.inner.registry.lock().map_err(|_| WorkspaceError::Io)?;
            registry
                .entries
                .iter_mut()
                .find(|entry| entry.id.as_ref() == options.id)
                .ok_or(WorkspaceError::Io)?
                .state = Some(inner.clone());
            Ok(Workspace {
                inner,
                host: self.inner.clone(),
            })
        })();
        if result.is_err() {
            let mount_released = !owned_directory || fs::remove_dir(&path).is_ok();
            let backing_released = directory
                .as_ref()
                .is_none_or(|directory| directory.close().is_ok());
            if mount_released && backing_released {
                if let Ok(mut registry) = self.inner.registry.lock() {
                    registry
                        .entries
                        .retain(|entry| entry.id.as_ref() != options.id);
                }
            } else if let Err(WorkspaceError::Backing(failure)) = &mut result {
                failure.cleanup_failed = true;
            }
        }
        result
    }
}
impl Host {
    pub fn call(
        &self,
        store: u32,
        operation: Operation,
        bytes: u64,
        output: &mut dyn Write,
        deadline: Instant,
    ) -> Result<Response, WorkspaceError> {
        self.call_input((store, 0), operation, &mut &[][..], bytes, output, deadline)
    }
    pub fn call_input(
        &self,
        target: (u32, u64),
        operation: Operation,
        input: &mut dyn Source,
        bytes: u64,
        output: &mut dyn Write,
        deadline: Instant,
    ) -> Result<Response, WorkspaceError> {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(WorkspaceError::Deadline)?;
        let deadline_ms =
            u32::try_from(remaining.as_millis()).map_err(|_| WorkspaceError::InvalidInput)?;
        if deadline_ms == 0 {
            return Err(WorkspaceError::Deadline);
        }
        let id = self
            .request_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| WorkspaceError::Capacity)?;
        let profile = if matches!(
            operation,
            Operation::HistoryQuery(_) | Operation::HistoryCommand(_)
        ) {
            2
        } else {
            1
        };
        let request = Request {
            id,
            generation: target.1,
            store: target.0,
            profile,
            deadline_ms,
            response_bytes: bytes,
            operation,
        };
        request.validate()?;
        (self.deliver)(&request, input, output, deadline).map_err(WorkspaceError::Service)
    }
}
pub(crate) fn validate_id(id: &str) -> Result<(), WorkspaceError> {
    if id.is_empty()
        || id.len() > 63
        || !id
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(WorkspaceError::InvalidInput);
    }
    Ok(())
}
fn secure_directory(path: &Path, create: bool) -> Result<(), WorkspaceError> {
    if create && !path.try_exists()? {
        create_owned_directory(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || fs::canonicalize(path)? != path {
        return Err(WorkspaceError::InvalidInput);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(WorkspaceError::Denied);
        }
    }
    Ok(())
}
fn create_owned_directory(path: &Path) -> Result<(), WorkspaceError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(WorkspaceError::from)
}
