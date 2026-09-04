//! Bounded private canonical construction. No persistent admission happens here.
use crate::objects::{
    InitializationObjectSlab, InitializationSlabQueueMetrics, InitializationSlabWriter,
    INITIALIZATION_SLAB_BYTES, INITIALIZATION_SLAB_OBJECTS, INITIALIZATION_SLAB_QUEUE_SLOTS,
};
use crate::{CanonicalObject, ObjectBuffer, Result, StoreError};
use layerfs_content::object::access::ObjectStore;
use layerfs_content::{CoreError, CoreResult, ObjectId};
use std::cell::{Cell, RefCell};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::Instant;

const MAX_WORKERS: usize = 8;
const STACK_BYTES: usize = 256 * 1024;
const STACK_RESERVE_BYTES: usize = 512 * 1024;
const CONTROL_RESERVE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConstructionWorkerMetrics {
    pub runs: u64,
    pub metrics_available: bool,
    pub wall_ns: u64,
    pub thread_cpu_ns: Option<u64>,
    pub idle_ns: u64,
    pub send_wait_ns: u64,
    pub read_wait_ns: u64,
    pub tasks: u64,
    pub objects: u64,
    pub canonical_bytes: u64,
    pub canonical_capacity_bytes: u64,
    pub hash_calls: u64,
    pub copy_bytes: u64,
    pub read_requests: u64,
    pub read_bytes: u64,
    pub slab_handoffs: u64,
    pub slab_owned_peak_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConstructionMetrics {
    pub pools: u64,
    pub reserved_bytes: u64,
    pub pool_wall_ns: u64,
    pub workers_started: usize,
    pub workers_joined: usize,
    pub workers: [ConstructionWorkerMetrics; MAX_WORKERS],
    pub submitted: u64,
    pub results_received: u64,
    pub collector_wall_ns: u64,
    pub consumer_idle_ns: u64,
    pub slab_queue_peak: u64,
    pub slab_queue_peak_bytes: u64,
    /// Reused slab credits include blocked senders. In the mixed event channel
    /// these clipped peaks bound queued slabs; they are not exact queue samples.
    pub slab_queue_peak_is_upper_bound: bool,
}

impl ConstructionMetrics {
    pub(crate) fn accumulate(&mut self, incoming: Self) {
        self.pools += incoming.pools;
        self.reserved_bytes = self.reserved_bytes.max(incoming.reserved_bytes);
        self.pool_wall_ns += incoming.pool_wall_ns;
        self.workers_started += incoming.workers_started;
        self.workers_joined += incoming.workers_joined;
        self.submitted += incoming.submitted;
        self.results_received += incoming.results_received;
        self.collector_wall_ns += incoming.collector_wall_ns;
        self.consumer_idle_ns += incoming.consumer_idle_ns;
        self.slab_queue_peak_is_upper_bound |= incoming.slab_queue_peak_is_upper_bound;
        self.slab_queue_peak = self.slab_queue_peak.max(incoming.slab_queue_peak);
        self.slab_queue_peak_bytes = self
            .slab_queue_peak_bytes
            .max(incoming.slab_queue_peak_bytes);
        for (total, worker) in self.workers.iter_mut().zip(incoming.workers) {
            if worker.runs == 0 {
                continue;
            }
            total.thread_cpu_ns = if total.runs == 0 {
                worker.thread_cpu_ns
            } else {
                total
                    .thread_cpu_ns
                    .zip(worker.thread_cpu_ns)
                    .map(|(a, b)| a.saturating_add(b))
            };
            total.metrics_available =
                (total.runs == 0 || total.metrics_available) && worker.metrics_available;
            total.runs += worker.runs;
            total.wall_ns += worker.wall_ns;
            total.idle_ns += worker.idle_ns;
            total.send_wait_ns += worker.send_wait_ns;
            total.read_wait_ns += worker.read_wait_ns;
            total.tasks += worker.tasks;
            total.objects += worker.objects;
            total.canonical_bytes += worker.canonical_bytes;
            total.canonical_capacity_bytes += worker.canonical_capacity_bytes;
            total.hash_calls += worker.hash_calls;
            total.copy_bytes += worker.copy_bytes;
            total.read_requests += worker.read_requests;
            total.read_bytes += worker.read_bytes;
            total.slab_handoffs += worker.slab_handoffs;
            total.slab_owned_peak_bytes = total
                .slab_owned_peak_bytes
                .max(worker.slab_owned_peak_bytes);
        }
    }
}

// Four global slots hold all events. A response sender is owned by its request;
// dropping the receiver drops every queued sender and wakes waiting workers.
enum Event<R> {
    Slab(InitializationObjectSlab),
    Read(ObjectId, SyncSender<Vec<u8>>),
    Result(R),
    Failure(StoreError),
}

pub struct ConstructionWorkerStore {
    writer: RefCell<InitializationSlabWriter>,
    read: Box<dyn Fn(ObjectId) -> Result<Vec<u8>> + Send>,
    error: RefCell<Option<StoreError>>,
    maximum: usize,
    read_requests: Cell<u64>,
    read_bytes: Cell<u64>,
    read_wait_ns: Cell<u64>,
    copy_bytes: Cell<u64>,
}
impl ConstructionWorkerStore {
    fn fail<T>(&self, error: StoreError) -> CoreResult<T> {
        *self.error.borrow_mut() = Some(error);
        Err(CoreError::Io)
    }
    fn flush(&self) -> Result<()> {
        self.writer.borrow_mut().flush()
    }
    fn measure(&self, mut metrics: ConstructionWorkerMetrics) -> ConstructionWorkerMetrics {
        let slab = self.writer.borrow().metrics();
        metrics.objects = slab.objects;
        metrics.canonical_bytes = slab.payload_bytes;
        metrics.canonical_capacity_bytes = slab.payload_capacity_bytes;
        metrics.hash_calls = slab.canonical_hash_calls;
        metrics.copy_bytes = self.copy_bytes.get();
        metrics.send_wait_ns += slab.blocked_ns;
        metrics.read_requests = self.read_requests.get();
        metrics.read_bytes = self.read_bytes.get();
        metrics.read_wait_ns = self.read_wait_ns.get();
        metrics.slab_handoffs = slab.handoffs;
        metrics.slab_owned_peak_bytes = slab.partial_peak_owned_bytes;
        metrics
    }
}
impl ObjectStore for ConstructionWorkerStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        if let Err(error) = self.flush() {
            return self.fail(error);
        }
        let started = Instant::now();
        self.read_requests.set(self.read_requests.get() + 1);
        let result = (self.read)(id);
        self.read_wait_ns
            .set(self.read_wait_ns.get() + elapsed(started));
        match result {
            Ok(bytes) => {
                self.read_bytes
                    .set(self.read_bytes.get() + bytes.len() as u64);
                Ok(bytes)
            }
            Err(error) => self.fail(error),
        }
    }
    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        if canonical.len() > self.maximum {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.copy_bytes
            .set(self.copy_bytes.get() + canonical.len() as u64);
        self.put_owned(canonical.to_vec())
    }
    fn put_owned(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        if canonical.capacity() > self.maximum {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.writer.get_mut().put_owned(canonical)
    }
}

/// Coordinator drives both submission and collection. Task/result heap ownership
/// is supplied by the caller's frozen-task window; no complete task/result Vec is
/// retained. On any pump failure this object disconnects and joins all workers.
pub struct PrivateContentPool<T: Send + 'static, R: Send + 'static> {
    jobs: Option<SyncSender<T>>,
    events: Option<Receiver<Event<R>>>,
    handles: Vec<(usize, JoinHandle<ConstructionWorkerMetrics>)>,
    cancel: Arc<AtomicBool>,
    queue: Arc<InitializationSlabQueueMetrics>,
    maximum: usize,
    metrics: ConstructionMetrics,
    started: Instant,
}
impl<T: Send + 'static, R: Send + 'static> PrivateContentPool<T, R> {
    /// Includes all configured stacks, blocked/queued/consumer slabs, incoming
    /// canonical values, bounded responses and task/result headers/heap shares,
    /// including the coordinator's frozen task while submission is backpressured.
    /// Existing central candidate and shared file-deferred budgets are separate.
    pub fn reservation_bytes(
        workers: usize,
        task_slots: usize,
        maximum: usize,
        task_heap_bytes: usize,
        result_heap_bytes: usize,
    ) -> Result<u64> {
        validate(workers, task_slots, maximum)?;
        let slabs = workers + INITIALIZATION_SLAB_QUEUE_SLOTS + 1;
        let objects = std::mem::size_of::<CanonicalObject>();
        let task_owned = std::mem::size_of::<T>()
            .checked_add(task_heap_bytes)
            .ok_or(StoreError::InvalidInput("content task reservation"))?;
        let result_owned = std::mem::size_of::<R>()
            .checked_add(result_heap_bytes)
            .ok_or(StoreError::InvalidInput("content result reservation"))?;
        let bytes = workers
            .checked_mul(STACK_RESERVE_BYTES)
            .and_then(|n| {
                n.checked_add(
                    slabs * (INITIALIZATION_SLAB_BYTES + INITIALIZATION_SLAB_OBJECTS * objects),
                )
            })
            .and_then(|n| n.checked_add((2 * workers + 1) * (maximum + objects)))
            .and_then(|n| n.checked_add((workers + task_slots + 1).checked_mul(task_owned)?))
            .and_then(|n| {
                n.checked_add(
                    (workers + INITIALIZATION_SLAB_QUEUE_SLOTS).checked_mul(result_owned)?,
                )
            })
            .and_then(|n| n.checked_add(CONTROL_RESERVE_BYTES))
            .ok_or(StoreError::InvalidInput("content pool reservation"))?;
        Ok(bytes as u64)
    }
    pub fn new(
        workers: usize,
        task_slots: usize,
        maximum: usize,
        process: fn(T, &mut ConstructionWorkerStore) -> Result<R>,
    ) -> Result<Self> {
        validate(workers, task_slots, maximum)?;
        let reserved_bytes = Self::reservation_bytes(workers, task_slots, maximum, 0, 0)?;
        let (jobs, incoming) = mpsc::sync_channel(task_slots);
        let incoming = Arc::new(Mutex::new(incoming));
        let (sender, events) = mpsc::sync_channel(INITIALIZATION_SLAB_QUEUE_SLOTS);
        let mut pool = Self {
            jobs: Some(jobs),
            events: Some(events),
            handles: Vec::with_capacity(workers),
            cancel: Arc::new(AtomicBool::new(false)),
            queue: Arc::new(InitializationSlabQueueMetrics::default()),
            maximum,
            metrics: ConstructionMetrics {
                pools: 1,
                reserved_bytes,
                slab_queue_peak_is_upper_bound: true,
                ..ConstructionMetrics::default()
            },
            started: Instant::now(),
        };
        for index in 0..workers {
            let incoming = incoming.clone();
            let event = sender.clone();
            let cancel = pool.cancel.clone();
            let queue = pool.queue.clone();
            let handle = std::thread::Builder::new()
                .name(format!("layerfs-content-{index}"))
                .stack_size(STACK_BYTES)
                .spawn(move || {
                    let started = Instant::now();
                    let cpu = thread_cpu();
                    let slab_event = event.clone();
                    let read_event = event.clone();
                    let mut store = ConstructionWorkerStore {
                        writer: RefCell::new(InitializationSlabWriter::with_transport(
                            Box::new(move |slab| send_event(&slab_event, Event::Slab(slab))),
                            queue,
                        )),
                        read: Box::new(move |id| {
                            let (sender, receiver) = mpsc::sync_channel(0);
                            send_event(&read_event, Event::Read(id, sender))?;
                            receiver
                                .recv()
                                .map_err(|_| StoreError::Integrity("content read cancelled"))
                        }),
                        error: RefCell::new(None),
                        maximum,
                        read_requests: Cell::new(0),
                        read_bytes: Cell::new(0),
                        read_wait_ns: Cell::new(0),
                        copy_bytes: Cell::new(0),
                    };
                    let mut metrics = ConstructionWorkerMetrics {
                        runs: 1,
                        metrics_available: true,
                        ..ConstructionWorkerMetrics::default()
                    };
                    while !cancel.load(Ordering::Acquire) {
                        let idle = Instant::now();
                        let task = incoming.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        metrics.idle_ns += elapsed(idle);
                        let Ok(task) = task else { break };
                        if cancel.load(Ordering::Acquire) {
                            break;
                        }
                        let processed =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                process(task, &mut store)
                            }))
                            .unwrap_or_else(|_| Err(StoreError::Integrity("content worker panic")));
                        match processed {
                            Ok(result) => {
                                metrics.tasks += 1;
                                match send_event(&event, Event::Result(result)) {
                                    Ok(wait) => metrics.send_wait_ns += wait,
                                    Err(_) => break,
                                }
                            }
                            Err(error) => {
                                cancel.store(true, Ordering::Release);
                                let original = store.error.borrow_mut().take().unwrap_or(error);
                                let _ = send_event(&event, Event::Failure(original));
                                break;
                            }
                        }
                    }
                    if !cancel.load(Ordering::Acquire) {
                        if let Err(error) = store.flush() {
                            let _ = send_event(&event, Event::Failure(error));
                        }
                    }
                    metrics.wall_ns = elapsed(started);
                    metrics.thread_cpu_ns = cpu.zip(thread_cpu()).map(|(a, b)| b.saturating_sub(a));
                    store.measure(metrics)
                });
            match handle {
                Ok(handle) => {
                    pool.handles.push((index, handle));
                    pool.metrics.workers_started += 1;
                }
                Err(error) => {
                    drop(sender);
                    pool.abort();
                    return Err(error.into());
                }
            }
        }
        drop(sender);
        Ok(pool)
    }
    /// Returns an unsent task when the bounded queue is full. Pump before retry.
    pub fn try_submit(&mut self, task: T) -> Result<Option<T>> {
        if self
            .metrics
            .submitted
            .saturating_sub(self.metrics.results_received)
            >= (self.metrics.workers_started * 2) as u64
        {
            return Ok(Some(task));
        }
        let Some(jobs) = &self.jobs else {
            return Err(StoreError::InvalidInput("content jobs closed"));
        };
        match jobs.try_send(task) {
            Ok(()) => {
                self.metrics.submitted += 1;
                Ok(None)
            }
            Err(TrySendError::Full(task)) => Ok(Some(task)),
            Err(TrySendError::Disconnected(_)) => {
                // All receiver owners have exited, so these events cannot grow.
                // Preserve their concrete failure instead of replacing it with
                // the incidental disconnected job channel error.
                let mut failure = None;
                if let Some(events) = &self.events {
                    while let Ok(event) = events.try_recv() {
                        match event {
                            Event::Failure(error) if failure.is_none() => failure = Some(error),
                            Event::Slab(slab) => self.queue.received(slab.payload_bytes),
                            _ => {}
                        }
                    }
                }
                self.abort();
                Err(failure.unwrap_or(StoreError::Integrity("content workers disconnected")))
            }
        }
    }
    /// Results may precede a worker's partial slab. Emit final records here, but
    /// resolve their newly-created content only after finish flushes all slabs.
    pub fn pump(
        &mut self,
        collector: &mut ObjectBuffer<'_>,
        mut result: impl FnMut(R, &mut ObjectBuffer<'_>) -> Result<()>,
    ) -> Result<bool> {
        let wait = Instant::now();
        let event = self
            .events
            .as_ref()
            .ok_or(StoreError::InvalidInput("content pool closed"))?
            .recv();
        self.metrics.consumer_idle_ns += elapsed(wait);
        let Ok(event) = event else {
            return Ok(false);
        };
        let started = Instant::now();
        let processed = match event {
            Event::Slab(slab) => {
                self.queue.received(slab.payload_bytes);
                slab.objects
                    .into_iter()
                    .try_for_each(|object| collector.collect_owned(object))
            }
            Event::Read(id, sender) => collector.read_bounded(id, self.maximum).and_then(|bytes| {
                sender
                    .send(bytes)
                    .map_err(|_| StoreError::Integrity("content read receiver"))
            }),
            Event::Result(value) => {
                self.metrics.results_received += 1;
                result(value, collector)
            }
            Event::Failure(error) => Err(error),
        };
        self.metrics.collector_wall_ns += elapsed(started);
        if let Err(error) = processed {
            self.abort();
            return Err(error);
        }
        Ok(true)
    }
    pub fn finish(
        &mut self,
        collector: &mut ObjectBuffer<'_>,
        mut result: impl FnMut(R, &mut ObjectBuffer<'_>) -> Result<()>,
    ) -> Result<ConstructionMetrics> {
        self.jobs.take();
        while self.pump(collector, &mut result)? {}
        self.events.take();
        let panicked = self.join();
        if panicked {
            return Err(StoreError::Integrity("content worker panic"));
        }
        if self.metrics.submitted != self.metrics.results_received {
            return Err(StoreError::Integrity("content result count"));
        }
        Ok(self.metrics)
    }
    /// Records the caller's preflight envelope, including its bounded task heap
    /// and source/preparation scratch. The caller must reserve it before new().
    pub fn set_reserved_bytes(&mut self, bytes: u64) -> Result<()> {
        if bytes < self.metrics.reserved_bytes {
            return Err(StoreError::InvalidInput("content pool reservation"));
        }
        self.metrics.reserved_bytes = bytes;
        Ok(())
    }
    pub fn metrics(&self) -> ConstructionMetrics {
        self.metrics
    }
    fn join(&mut self) -> bool {
        let mut panicked = false;
        for (index, handle) in self.handles.drain(..) {
            match handle.join() {
                Ok(metrics) => self.metrics.workers[index] = metrics,
                Err(_) => {
                    panicked = true;
                    self.metrics.workers[index].runs = 1;
                    self.metrics.workers[index].metrics_available = false;
                }
            }
            self.metrics.workers_joined += 1;
        }
        self.metrics.pool_wall_ns = elapsed(self.started);
        self.metrics.slab_queue_peak = self.queue.peak();
        self.metrics.slab_queue_peak_bytes = self.queue.peak_bytes();
        crate::telemetry::note_workspace_content_pool(self.metrics);
        panicked
    }
    fn abort(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.jobs.take();
        self.events.take(); // disconnect slabs/results AND every queued read response
        self.join();
    }
}
impl<T: Send + 'static, R: Send + 'static> Drop for PrivateContentPool<T, R> {
    fn drop(&mut self) {
        if !self.handles.is_empty() {
            self.abort();
        }
    }
}
fn validate(workers: usize, slots: usize, maximum: usize) -> Result<()> {
    if workers == 0
        || workers > MAX_WORKERS
        || slots == 0
        || slots > workers
        || maximum == 0
        || maximum > INITIALIZATION_SLAB_BYTES
    {
        return Err(StoreError::InvalidInput("content pool bounds"));
    }
    Ok(())
}
fn send_event<R>(sender: &SyncSender<Event<R>>, event: Event<R>) -> Result<u64> {
    match sender.try_send(event) {
        Ok(()) => Ok(0),
        Err(TrySendError::Full(event)) => {
            let started = Instant::now();
            sender
                .send(event)
                .map_err(|_| StoreError::Integrity("content receiver cancelled"))?;
            Ok(elapsed(started))
        }
        Err(TrySendError::Disconnected(_)) => {
            Err(StoreError::Integrity("content receiver cancelled"))
        }
    }
}
fn elapsed(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
#[cfg(target_os = "linux")]
pub(crate) fn thread_cpu() -> Option<u64> {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    u64::try_from(value.tv_sec)
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(u64::try_from(value.tv_nsec).ok()?)
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn thread_cpu() -> Option<u64> {
    None
}

/// CPU consumed by all threads in this process, including construction workers.
/// This excludes separate daemon/benchmark sampler processes.
#[doc(hidden)]
#[cfg(target_os = "linux")]
pub fn workspace_process_cpu_ns() -> Option<u64> {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    u64::try_from(value.tv_sec)
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(u64::try_from(value.tv_nsec).ok()?)
}
#[doc(hidden)]
#[cfg(not(target_os = "linux"))]
pub fn workspace_process_cpu_ns() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pool_telemetry_accumulates_restarts_and_preserves_unavailable_cpu() {
        let mut total = ConstructionMetrics::default();
        for cpu in [Some(100), Some(200), None] {
            let mut pool = ConstructionMetrics {
                pools: 1,
                reserved_bytes: 2000,
                workers_started: 1,
                workers_joined: 1,
                submitted: 2,
                results_received: 2,
                ..ConstructionMetrics::default()
            };
            pool.workers[0] = ConstructionWorkerMetrics {
                runs: 1,
                metrics_available: true,
                tasks: 2,
                thread_cpu_ns: cpu,
                ..ConstructionWorkerMetrics::default()
            };
            total.accumulate(pool);
        }
        assert_eq!(
            (
                total.pools,
                total.workers_started,
                total.workers_joined,
                total.submitted
            ),
            (3, 3, 3, 6)
        );
        assert_eq!((total.workers[0].runs, total.workers[0].tasks), (3, 6));
        assert_eq!(total.workers[0].thread_cpu_ns, None);
        assert_eq!(total.reserved_bytes, 2000);
    }
    fn echo(task: u8, store: &mut ConstructionWorkerStore) -> Result<ObjectId> {
        let canonical = layerfs_content::encode_bytes_object(&[task; 31])?;
        let id = store.put_owned(canonical.clone())?;
        assert_eq!(store.get(id)?, canonical); // flush-before-read RPC
        Ok(id)
    }
    #[test]
    fn private_pool_preserves_read_own_writes_and_joins() {
        let mut pool = PrivateContentPool::new(2, 2, 65536, echo).unwrap();
        let mut collector = ObjectBuffer::empty().unwrap();
        let mut results = Vec::new();
        for task in 0..8 {
            let mut pending = Some(task);
            while let Some(task) = pending.take() {
                pending = pool.try_submit(task).unwrap();
                if pending.is_some() {
                    assert!(pool
                        .pump(&mut collector, |id, _| {
                            results.push(id);
                            Ok(())
                        })
                        .unwrap());
                }
            }
        }
        let metrics = pool
            .finish(&mut collector, |id, _| {
                results.push(id);
                Ok(())
            })
            .unwrap();
        assert_eq!((metrics.workers_started, metrics.workers_joined), (2, 2));
        assert_eq!(results.len(), 8);
        assert_eq!(
            metrics
                .workers
                .iter()
                .map(|worker| worker.read_requests)
                .sum::<u64>(),
            8
        );
        for id in results {
            assert!(!collector.read_bounded(id, 65536).unwrap().is_empty());
        }
    }
    fn many(task: u8, store: &mut ConstructionWorkerStore) -> Result<()> {
        for index in 0..200_u64 {
            let mut bytes = vec![task; 32768];
            bytes[..8].copy_from_slice(&index.to_be_bytes());
            store.put_owned(layerfs_content::encode_bytes_object(&bytes)?)?;
        }
        Ok(())
    }
    #[test]
    fn private_pool_preserves_collector_collision_and_cancels() {
        let canonical = layerfs_content::encode_bytes_object(&[9; 31]).unwrap();
        let id = ObjectId::for_bytes(&canonical);
        let mut collector = ObjectBuffer::empty().unwrap();
        collector
            .collect_owned(CanonicalObject {
                id,
                bytes: layerfs_content::encode_bytes_object(&[8; 31]).unwrap(),
            })
            .unwrap();
        let mut pool = PrivateContentPool::new(2, 2, 65536, echo).unwrap();
        pool.try_submit(9).unwrap();
        let error = pool.finish(&mut collector, |_, _| Ok(())).unwrap_err();
        assert!(matches!(
            error,
            StoreError::Integrity("candidate object collision")
        ));
        assert_eq!(pool.metrics().workers_joined, 2);
    }
    #[test]
    fn private_pool_drop_cancels_full_slab_queue_and_worker_panic() {
        let mut pool = PrivateContentPool::new(2, 2, 65536, many).unwrap();
        pool.try_submit(3).unwrap();
        pool.try_submit(4).unwrap();
        let start = Instant::now();
        while pool.queue.peak() < 4 {
            assert!(start.elapsed().as_secs() < 5);
            std::thread::yield_now();
        }
        pool.abort();
        assert_eq!(pool.metrics().workers_joined, 2);
        fn panic_task(_: (), _: &mut ConstructionWorkerStore) -> Result<()> {
            panic!("worker test");
        }
        let mut pool = PrivateContentPool::new(2, 2, 65536, panic_task).unwrap();
        pool.try_submit(()).unwrap();
        let mut collector = ObjectBuffer::empty().unwrap();
        assert!(matches!(
            pool.finish(&mut collector, |_, _| Ok(())),
            Err(StoreError::Integrity("content worker panic"))
        ));
        assert_eq!(pool.metrics().workers_joined, 2);
    }
}
