//! Prototype (lanes2 probe, #684; not a product contract): one mounted
//! Workspace's fixed pool of read-ahead fetch threads.
//!
//! Without it, every read-ahead window spawns its other parts' threads and
//! joins them. With `LAYERFS_PROBE_READ_POOL=M` (1 to 16), the Workspace's
//! read session starts M threads at its first window and keeps them until
//! it is dropped; each window hands its other parts to them as jobs. A
//! window's parts, bounds and custody are unchanged: each part is still one
//! demand over its own fresh custody, and every part is waited for.
use super::client::Fresh;
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use std::sync::{mpsc, Arc, Mutex, PoisonError};

type Job = Box<dyn FnOnce() + Send>;

/// Prototype: see the module documentation.
pub struct FetchPool {
    jobs: mpsc::Sender<Job>,
    workers: usize,
}
impl FetchPool {
    /// A pool of `LAYERFS_PROBE_READ_POOL` threads, or `None` when it is
    /// unset, invalid or no thread could start.
    pub fn from_env() -> Option<Arc<Self>> {
        let workers = std::env::var("LAYERFS_PROBE_READ_POOL")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .filter(|workers| (1..=16).contains(workers))?;
        Self::new(workers)
    }
    /// A pool of `workers` threads; `None` when none could start.
    pub fn new(workers: usize) -> Option<Arc<Self>> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let queue = Arc::new(Mutex::new(queue));
        let mut started = 0;
        for _ in 0..workers {
            let queue = queue.clone();
            let spawned = std::thread::Builder::new()
                .name("layerfs-read-pool".into())
                .spawn(move || loop {
                    // The pool's sender is dropped with the pool: then every
                    // queued job has been taken and the thread ends.
                    let job = match queue.lock().unwrap_or_else(PoisonError::into_inner).recv() {
                        Ok(job) => job,
                        Err(_) => return,
                    };
                    // A panicking job drops its reply sender, which its
                    // window reports as a failed part; the thread lives on.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                });
            started += usize::from(spawned.is_ok());
        }
        (started > 0).then(|| {
            Arc::new(Self {
                jobs,
                workers: started,
            })
        })
    }
    /// Threads in the pool.
    pub fn workers(&self) -> usize {
        self.workers
    }
    /// Queues one job; `false` (the job dropped unrun) when no thread is left.
    pub(crate) fn submit(&self, job: Job) -> bool {
        self.jobs.send(job).is_ok()
    }
}

/// Prototype (lanes2): `CanonicalClient::fetch_parts` with the other parts run
/// as jobs on the Workspace's fixed pool. Every queued part is waited
/// for; a part whose job ended without a reply (it panicked, or no pool
/// thread was left) fails the window, as does the first failure in part
/// order.
pub(crate) fn fetch_pooled(
    pool: &FetchPool,
    source: &(dyn AuthenticatedObjects + Send + Sync),
    fresh: &Fresh,
    wanted: &[ObjectId],
    size: usize,
) -> ContentResult<Vec<Vec<u8>>> {
    let started = std::time::Instant::now();
    let (reply, replies) = std::sync::mpsc::channel();
    let mut pieces = wanted.chunks(size);
    let first = pieces.next().unwrap_or(&[]);
    let mut queued = 0;
    for (index, part) in pieces.enumerate() {
        let source = fresh();
        let ids = part.to_vec();
        let reply = reply.clone();
        queued += 1;
        pool.submit(Box::new(move || {
            let part = source.read_canonical_batch(&ids);
            let _ = reply.send((index, part, std::time::Instant::now()));
        }));
    }
    drop(reply);
    let mut values = source.read_canonical_batch(first);
    let mut ends = vec![std::time::Instant::now()];
    let mut others: Vec<Option<ContentResult<Vec<Vec<u8>>>>> = (0..queued).map(|_| None).collect();
    // Each job replies once or drops its sender, so this ends.
    while let Ok((index, part, end)) = replies.recv() {
        others[index] = Some(part);
        ends.push(end);
    }
    window_work(started, &ends);
    for part in others {
        let part = part.unwrap_or(Err(ContentError::ProviderFailure {
            what: "read-ahead part panicked",
        }));
        values = match (values, part) {
            (Ok(mut values), Ok(part)) => {
                values.extend(part);
                Ok(values)
            }
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        };
    }
    values
}
/// Prototype (lanes2): charges one split window's fetch: its wall time from
/// `started` to its last part's end, and each part's idle time before that.
pub(crate) fn window_work(started: std::time::Instant, ends: &[std::time::Instant]) {
    use layerfs_telemetry::timer::stages;
    let Some(last) = ends.iter().max() else {
        return;
    };
    let nanos = |d: std::time::Duration| d.as_nanos().min(u64::MAX as u128) as u64;
    stages::add(stages::Stage::WindowFetch, 1, nanos(*last - started));
    let idle: u64 = ends.iter().map(|end| nanos(*last - *end)).sum();
    stages::add(stages::Stage::PartIdle, ends.len() as u64, idle);
}
