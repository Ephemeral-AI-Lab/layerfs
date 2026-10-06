//! Initialized host composition; provider ownership never moves to I/O workers.
use super::{attachment::Attachment, *};
use crate::runtime::{service::*, Binding, RuntimeError, RuntimeResult, Sessions};
use layerfs_bridge::{codec::ReceiveBudget, contract::FrameError, native::Connection};
use layerfs_history::{BranchId, WorkspaceId};

/// Host-thread supervisor over initialized borrowed Sessions and real native sockets.
/// One connection serves sequential bounded exchanges; other connections progress
/// independently. This is neither a new provider nor a whole-Save connection lock.
pub struct Supervisor<'s, 'a> {
    pub(super) service: Service<'s, 'a>,
    pub(super) input: InputPool,
    pub(super) output: OutputPool,
    pub(super) attachments: Vec<Option<Attachment>>,
    pub(super) next: usize,
    pub(super) work: SupervisorWork,
}
impl<'s, 'a> Supervisor<'s, 'a> {
    /// Selects shared windows and initializes owners once before accepting sockets.
    pub fn new(
        sessions: &'s mut Sessions<'a>,
        config: SupervisorConfig,
    ) -> Result<Self, SupervisorStartError> {
        let connections = config.service.connections;
        if config.output.owners < connections
            || config.output.messages < connections.saturating_mul(3).saturating_add(2)
        {
            return Err(RuntimeError::Invalid("supervisor output owner/packet windows").into());
        }
        let input = InputPool::new(connections, ReceiveBudget::new(config.input)?)?;
        let output = OutputPool::new(config.output)?;
        let mut attachments = Vec::new();
        attachments
            .try_reserve_exact(connections)
            .map_err(|_| RuntimeError::AdmissionUnavailable)?;
        attachments.resize_with(connections, || None);
        let registry_capacity_bytes =
            attachments.capacity() * std::mem::size_of::<Option<Attachment>>();
        Ok(Self {
            service: Service::new(sessions, config.service)?,
            input,
            output,
            attachments,
            next: 0,
            work: SupervisorWork {
                registry_capacity_bytes,
                ..Default::default()
            },
        })
    }
    /// Binds one initial peer/Workspace/Branch, then attaches its original socket.
    /// Existing bindings use attach_bound; neither path retries a failed operation.
    #[allow(clippy::result_large_err)]
    pub fn attach(
        &mut self,
        connection: Connection,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> Result<AttachmentId, AttachFailure> {
        let binding = match self.service.bind(&connection.peer, workspace, branch) {
            Ok(binding) => binding,
            Err(error) => return Err(AttachFailure::Runtime { error, connection }),
        };
        self.attach_bound(connection, binding)
    }
    /// Attaches an exact existing binding without reading/refreshing its Branch.
    #[allow(clippy::result_large_err)]
    pub fn attach_bound(
        &mut self,
        connection: Connection,
        binding: Binding,
    ) -> Result<AttachmentId, AttachFailure> {
        let slot = match self.attachments.iter().position(Option::is_none) {
            Some(slot) => slot,
            None => {
                return Err(AttachFailure::Runtime {
                    error: RuntimeError::AdmissionUnavailable,
                    connection,
                })
            }
        };
        let id = match self.service.connect(&connection.peer, binding) {
            Ok(id) => AttachmentId(id),
            Err(error) => return Err(AttachFailure::Runtime { error, connection }),
        };
        let (mut input, sender, _) = match self.input.start(connection) {
            Ok(parts) => parts,
            Err((error, connection)) => {
                self.service
                    .disconnect(id.0)
                    .expect("new unattempted connection");
                return Err(AttachFailure::Input { error, connection });
            }
        };
        let output = match self.output.start(sender) {
            Ok(output) => output,
            Err((error, sender)) => {
                input.fence();
                self.service
                    .disconnect(id.0)
                    .expect("new unattempted connection");
                return Err(AttachFailure::Output {
                    error,
                    input,
                    sender,
                });
            }
        };
        self.attachments[slot] = Some(Attachment {
            id,
            input,
            output,
            exchange: None,
            service_fence: None,
            input_fence: None,
            output_fence: None,
            failure: None,
            input_events: Vec::new(),
            output_receipts: Vec::new(),
        });
        self.work.attached = self.work.attached.saturating_add(1);
        self.work.attachments += 1;
        Ok(id)
    }
    /// Nonblocking original composition observations.
    pub const fn work(&self) -> SupervisorWork {
        self.work
    }
    /// Existing provider-service observations, including caller-held completions.
    pub fn service_work(&self) -> ServiceWork {
        self.service.work()
    }
    /// Existing shared output credit, including permits and caller-held receipts.
    pub fn output_work(&self) -> Result<OutputWork, FrameError> {
        self.output.work()
    }
    /// Input owner/report admission still retained across all attachments.
    pub fn input_owners(&self) -> usize {
        self.input.outstanding()
    }
    pub(super) fn slot(&self, id: AttachmentId) -> RuntimeResult<usize> {
        self.attachments
            .iter()
            .position(|a| a.as_ref().is_some_and(|a| a.id == id))
            .ok_or(RuntimeError::StaleCapability)
    }
}
