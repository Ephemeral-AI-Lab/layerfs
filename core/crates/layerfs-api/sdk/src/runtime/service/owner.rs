//! Single host dispatch owner; bounded jobs, independent persistent Saves.
use super::{
    credits::{Credit, Ledger},
    execution,
    queue::Queue,
    ConnectionId, DisconnectFence, Request, Response, ServiceCompletion, ServiceConfig,
    ServiceOutcome, ServiceWork, Ticket,
};
use crate::runtime::{Binding, Completion, HistoryReceipts, RuntimeError, RuntimeResult, Sessions};
use layerfs_bridge::native::VerifiedPeer;
use layerfs_history::{BranchId, WorkspaceId};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Instant,
};

struct Client {
    id: ConnectionId,
    binding: Binding,
}
struct Job {
    ticket: Ticket,
    connection: ConnectionId,
    binding: Binding,
    request: Option<Request>,
    outcome: Option<ServiceOutcome>,
    admitted: Instant,
    queue_wait_ns: u64,
    service_ns: u64,
    credit: Credit,
}
/// Bounded fair adapter service borrowing the application-owned Save registry.
///
/// Keep this owner on the host service thread. Transport supplies verified peers
/// and bounded request delivery; it does not own or abort the borrowed Saves.
/// One step invokes one synchronous bounded adapter unit, never a whole Save.
pub struct Service<'s, 'a> {
    sessions: &'s mut Sessions<'a>,
    config: ServiceConfig,
    owner: u64,
    next: u64,
    clients: Vec<Option<Client>>,
    jobs: Vec<Option<Job>>,
    queue: Queue,
    ledger: Rc<RefCell<Ledger>>,
    save_order: BTreeMap<crate::SaveId, VecDeque<Ticket>>,
}
impl<'s, 'a> Service<'s, 'a> {
    /// Allocates fixed registry windows without taking ownership of the Sessions.
    /// Invalid/admission-refused configuration leaves its Saves with the caller.
    pub fn new(sessions: &'s mut Sessions<'a>, config: ServiceConfig) -> RuntimeResult<Self> {
        if sessions.service_owners.get() != 0 {
            return Err(RuntimeError::RetainedCustody);
        }
        let reserved = config
            .read_reserve
            .checked_add(config.control_reserve)
            .ok_or(RuntimeError::Invalid("service byte reserves"))?;
        if config.connections == 0
            || config.jobs < 3
            || config.jobs_per_workspace == 0
            || config.bytes <= reserved
            || config.control_reserve < std::mem::size_of::<Job>()
        {
            return Err(RuntimeError::Invalid("service admission windows"));
        }
        let mut clients = Vec::new();
        clients
            .try_reserve_exact(config.connections)
            .map_err(|_| RuntimeError::AdmissionUnavailable)?;
        clients.resize_with(config.connections, || None);
        let mut jobs = Vec::new();
        jobs.try_reserve_exact(config.jobs)
            .map_err(|_| RuntimeError::AdmissionUnavailable)?;
        jobs.resize_with(config.jobs, || None);
        let registry_capacity_bytes = clients
            .capacity()
            .checked_mul(std::mem::size_of::<Option<Client>>())
            .and_then(|n| {
                jobs.capacity()
                    .checked_mul(std::mem::size_of::<Option<Job>>())
                    .and_then(|j| n.checked_add(j))
            })
            .ok_or(RuntimeError::Invalid("service registry capacity"))?;
        let owner = sessions.capability_serial()?;
        let mut ledger = Ledger::new(config, sessions.service_owners.clone());
        ledger.work.registry_capacity_bytes = registry_capacity_bytes;
        sessions.service_owners.set(1);
        Ok(Self {
            sessions,
            config,
            owner,
            next: 1,
            clients,
            jobs,
            queue: Queue::new(),
            ledger: Rc::new(RefCell::new(ledger)),
            save_order: BTreeMap::new(),
        })
    }

    /// Authenticates a binding through the owning runtime, without a tree scan.
    pub fn bind(
        &self,
        peer: &VerifiedPeer,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> RuntimeResult<Binding> {
        self.sessions.bind(peer, workspace, branch)
    }

    /// Revalidates a parsed fixed wire header before body receive allocation.
    /// This grants only input admission, never operation/publication success.
    /// The dispatched handler revalidates authority and semantic closure later.
    pub fn authorize_header(
        &self,
        connection: ConnectionId,
        header: &crate::client::RequestHeader,
    ) -> RuntimeResult<()> {
        header
            .payload_bytes()
            .map_err(|_| RuntimeError::Invalid("wire header"))?;
        let client = self.client(connection)?;
        self.sessions.check_binding(&client.binding)?;
        if let Some(save) = header.save {
            let slot = self
                .sessions
                .slot(&client.binding, crate::SaveId::from_token(save)?)?;
            if slot.completion.is_some()
                && matches!(
                    header.operation,
                    crate::client::Operation::Accept
                        | crate::client::Operation::Finish
                        | crate::client::Operation::Abort
                )
            {
                return Err(RuntimeError::AlreadyAttempted);
            }
        }
        Ok(())
    }

    /// Attaches an authenticated peer to an existing exact binding. Reconnection
    /// may use the original binding; it never implicitly refreshes a Branch.
    pub fn connect(
        &mut self,
        peer: &VerifiedPeer,
        binding: Binding,
    ) -> RuntimeResult<ConnectionId> {
        self.sessions.check_binding(&binding)?;
        if peer.public_key() != binding.peer() {
            return Err(RuntimeError::Denied);
        }
        let slot = self
            .clients
            .iter()
            .position(Option::is_none)
            .ok_or(RuntimeError::AdmissionUnavailable)?;
        let serial = self.serial()?;
        let id = ConnectionId {
            owner: self.owner,
            slot,
            serial,
        };
        self.clients[slot] = Some(Client { id, binding });
        Ok(id)
    }

    /// Admits a bounded original request or returns it without provider effect.
    /// Credits include the largest response/transient copies and remain owned
    /// through queued, dispatched and caller-retained completion lifetimes.
    pub fn try_submit(
        &mut self,
        connection: ConnectionId,
        request: Request,
    ) -> Result<Ticket, (RuntimeError, Request)> {
        let class = request.class() as usize;
        {
            let mut ledger = self.ledger.borrow_mut();
            ledger.work.submission_attempts[class] =
                ledger.work.submission_attempts[class].saturating_add(1);
        }
        let result = self.admit(connection, request);
        if result.is_err() {
            let mut ledger = self.ledger.borrow_mut();
            ledger.work.submission_refusals[class] =
                ledger.work.submission_refusals[class].saturating_add(1);
        }
        result
    }

    fn admit(
        &mut self,
        connection: ConnectionId,
        request: Request,
    ) -> Result<Ticket, (RuntimeError, Request)> {
        let prepared = (|| {
            let client = self.client(connection)?;
            self.sessions.check_binding(&client.binding)?;
            if let Some(save) = request.save() {
                self.sessions.slot(&client.binding, save)?;
            }
            let bytes = request
                .charge(
                    std::mem::size_of::<Job>()
                        + std::mem::size_of::<ServiceCompletion>()
                        + client.binding.snapshot().branch.name.as_str().len(),
                )
                .ok_or(RuntimeError::Invalid("service request window/charge"))?;
            let slot = self
                .jobs
                .iter()
                .position(Option::is_none)
                .ok_or(RuntimeError::AdmissionUnavailable)?;
            Ok((client.binding.clone(), bytes, slot))
        })();
        let (binding, bytes, slot) = match prepared {
            Ok(value) => value,
            Err(error) => return Err((error, request)),
        };
        let serial = match self.serial() {
            Ok(serial) => serial,
            Err(error) => return Err((error, request)),
        };
        let class = request.class();
        let credit = match Credit::acquire(
            &self.ledger,
            binding.workspace(),
            class,
            bytes,
            request.save(),
        ) {
            Some(credit) => credit,
            None => return Err((RuntimeError::AdmissionUnavailable, request)),
        };
        let ticket = Ticket {
            owner: self.owner,
            slot,
            serial,
        };
        let ready = if let Some(save) = request.save() {
            let order = self.save_order.entry(save).or_default();
            let ready = order.is_empty();
            order.push_back(ticket);
            ready
        } else {
            true
        };
        if ready {
            self.queue.push(binding.workspace(), class as usize, ticket);
        }
        self.jobs[slot] = Some(Job {
            ticket,
            connection,
            binding,
            request: Some(request),
            outcome: None,
            admitted: Instant::now(),
            queue_wait_ns: 0,
            service_ns: 0,
            credit,
        });
        Ok(ticket)
    }

    /// Invokes the next Workspace/class share once. Original error/unknown
    /// results remain retained; delivery happens after all provider work returns.
    pub fn step(&mut self) -> Option<Ticket> {
        let ticket = self.queue.take()?;
        let job = self.jobs[ticket.slot].as_mut().expect("admitted ready job");
        let request = job.request.take().expect("unattempted request");
        let save = request.save();
        let class = request.class();
        job.queue_wait_ns = nanos(job.admitted.elapsed());
        let start = Instant::now();
        let (result, copied) = if let Request::Release { save } = &request {
            if self.ledger.borrow().save_owners(*save) > 1 {
                (Err(RuntimeError::RetainedCustody), 0)
            } else {
                execution::invoke(self.sessions, &job.binding, request)
            }
        } else {
            execution::invoke(self.sessions, &job.binding, request)
        };
        if let Ok(Response::Begun(save)) = &result {
            job.credit.bind_save(*save);
        }
        job.service_ns = nanos(start.elapsed());
        job.outcome = Some(ServiceOutcome::Dispatched(result));
        let mut ledger = self.ledger.borrow_mut();
        let work = &mut ledger.work;
        work.dispatched[class as usize] = work.dispatched[class as usize].saturating_add(1);
        work.queue_wait_ns[class as usize] =
            work.queue_wait_ns[class as usize].saturating_add(job.queue_wait_ns);
        work.service_ns[class as usize] =
            work.service_ns[class as usize].saturating_add(job.service_ns);
        work.copied_bytes = work.copied_bytes.saturating_add(copied);
        drop(ledger);
        if let Some(save) = save {
            let order = self.save_order.get_mut(&save).expect("ordered Save job");
            assert_eq!(order.pop_front(), Some(ticket));
            let next = order.front().copied();
            if let Some(next) = next {
                let job = self.jobs[next.slot].as_ref().expect("ordered successor");
                self.queue.push(
                    job.binding.workspace(),
                    job.request.as_ref().expect("queued successor").class() as usize,
                    next,
                );
            } else {
                self.save_order.remove(&save);
            }
        }
        Some(ticket)
    }

    /// Moves a known original outcome to caller custody, retaining its credit.
    /// An unfinished ticket is observed without invoking or repeating anything.
    pub fn take_completion(&mut self, ticket: Ticket) -> RuntimeResult<Option<ServiceCompletion>> {
        let job = self.job(ticket)?;
        if job.outcome.is_none() {
            return Ok(None);
        }
        let mut job = self.jobs[ticket.slot].take().expect("checked ticket");
        Ok(Some(ServiceCompletion {
            binding: job.binding,
            outcome: job.outcome.take().expect("completed job"),
            _credit: job.credit,
            queue_wait_ns: job.queue_wait_ns,
            service_ns: job.service_ns,
        }))
    }

    /// Reads the registry's exact Save result under current authority. Keeping
    /// this completion alive prevents a queued release from destroying it.
    pub fn save_completion(&self, completion: &ServiceCompletion) -> RuntimeResult<&Completion> {
        self.check_completion(completion)?;
        match completion.outcome() {
            ServiceOutcome::Dispatched(Ok(Response::Completion(save))) => {
                self.sessions.completion(&completion.binding, *save)
            }
            _ => Err(RuntimeError::Invalid("not a Save completion response")),
        }
    }

    /// Reads exact retained stage/transition/discard knowledge, without history replay.
    pub fn history_receipts(
        &self,
        completion: &ServiceCompletion,
    ) -> RuntimeResult<&HistoryReceipts> {
        self.check_completion(completion)?;
        match completion.outcome() {
            ServiceOutcome::Dispatched(Ok(Response::History(save))) => {
                self.sessions.history_receipts(&completion.binding, *save)
            }
            _ => Err(RuntimeError::Invalid("not a history response")),
        }
    }

    /// Fences this attachment after all synchronous dispatch has returned. Only
    /// its queued requests become unattempted; completed results and Saves stay
    /// owned. This is not a resolver for unknown publication or a socket close.
    pub fn disconnect(&mut self, connection: ConnectionId) -> RuntimeResult<DisconnectFence> {
        let workspace = self.client(connection)?.binding.workspace();
        self.clients[connection.slot] = None;
        let mut cancelled = 0;
        let mut completed = 0;
        for job in self
            .jobs
            .iter_mut()
            .flatten()
            .filter(|job| job.connection == connection)
        {
            if let Some(request) = job.request.take() {
                job.outcome = Some(ServiceOutcome::Unattempted(request));
                cancelled += 1;
            } else {
                completed += 1;
            }
        }
        let jobs = &self.jobs;
        self.queue.cancel(workspace, |ticket| {
            jobs[ticket.slot]
                .as_ref()
                .is_some_and(|job| job.ticket == ticket && job.request.is_some())
        });
        // Each blocked job is visited once; no cancellation scans all jobs again.
        self.save_order.retain(|_, order| {
            let before = order.front().copied();
            order.retain(|ticket| {
                jobs[ticket.slot]
                    .as_ref()
                    .is_some_and(|job| job.request.is_some())
            });
            if let Some(next) = order.front().copied() {
                if Some(next) != before {
                    let job = jobs[next.slot].as_ref().expect("queued successor");
                    self.queue.push(
                        job.binding.workspace(),
                        job.request.as_ref().expect("queued successor").class() as usize,
                        next,
                    );
                }
                true
            } else {
                false
            }
        });
        if !self
            .clients
            .iter()
            .flatten()
            .any(|client| client.binding.workspace() == workspace)
        {
            self.queue.forget_workspace(workspace);
        }
        let mut ledger = self.ledger.borrow_mut();
        ledger.work.cancelled = ledger.work.cancelled.saturating_add(cancelled as u64);
        Ok(DisconnectFence {
            connection,
            cancelled,
            completed,
        })
    }

    /// Fixed queue/result ownership and cumulative real invocation/copy observations.
    pub fn work(&self) -> ServiceWork {
        self.ledger.borrow().work
    }

    /// Selected live admission. It does not impose a lifetime flow/Exec cap.
    pub const fn config(&self) -> ServiceConfig {
        self.config
    }

    fn serial(&mut self) -> RuntimeResult<u64> {
        let serial = self.next;
        self.next = serial
            .checked_add(1)
            .ok_or(RuntimeError::Invalid("service capability exhaustion"))?;
        Ok(serial)
    }
    fn client(&self, id: ConnectionId) -> RuntimeResult<&Client> {
        self.clients
            .get(id.slot)
            .and_then(Option::as_ref)
            .filter(|client| id.owner == self.owner && client.id == id)
            .ok_or(RuntimeError::StaleCapability)
    }
    fn job(&self, ticket: Ticket) -> RuntimeResult<&Job> {
        self.jobs
            .get(ticket.slot)
            .and_then(Option::as_ref)
            .filter(|job| ticket.owner == self.owner && job.ticket == ticket)
            .ok_or(RuntimeError::StaleCapability)
    }
    fn check_completion(&self, completion: &ServiceCompletion) -> RuntimeResult<()> {
        if !Rc::ptr_eq(&self.ledger, &completion._credit.ledger) {
            return Err(RuntimeError::StaleCapability);
        }
        self.sessions.check_binding(&completion.binding)
    }
}
fn nanos(duration: std::time::Duration) -> u64 {
    duration.as_nanos().min(u64::MAX as u128) as u64
}

impl Drop for Service<'_, '_> {
    fn drop(&mut self) {
        let ledger = self.ledger.borrow();
        ledger.owners.set(ledger.owners.get() - 1);
    }
}
