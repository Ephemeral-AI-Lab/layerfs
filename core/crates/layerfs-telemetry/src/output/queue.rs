//! Bounded best-effort output. In-flight ownership stays charged until released.
use super::retention::Local;
use nix::{
    poll::{poll, PollFd, PollFlags},
    unistd,
};
use std::{
    collections::VecDeque,
    io,
    os::fd::AsFd,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
/// Explicit output destination; a failed route has no fallback spool.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Host-coordinator stderr stream.
    Forward,
    /// Owned local segments.
    Local,
    /// Both under one aggregate rate/queue budget.
    Both,
}
/// Checked producer profile. Thread stacks and OS buffers are additional domains.
pub struct OutputConfig {
    /// Selected destination.
    pub mode: OutputMode,
    /// Fresh exclusively owned namespace for local/both.
    pub directory: Option<PathBuf>,
    /// Maximum queued plus active encoded allocation capacity.
    pub queue_bytes: usize,
    /// Maximum queued plus active record count.
    pub queue_count: usize,
    /// Maximum encoded record bytes.
    pub record_bytes: usize,
    /// Aggregate encoded bytes per second across destinations.
    pub rate: usize,
    /// Aggregate burst capacity.
    pub burst: usize,
    /// Active plus closed segment count.
    pub segments: usize,
    /// Maximum logical length of one segment.
    pub segment_bytes: usize,
    /// Closed/active segment expiry, serviced at most once per second.
    pub expiry: Duration,
}
impl OutputConfig {
    /// Initial forward producer selection, with no file destination.
    pub fn forward() -> Self {
        Self {
            mode: OutputMode::Forward,
            directory: None,
            queue_bytes: 2 * 1024 * 1024,
            queue_count: 128,
            record_bytes: 16384,
            rate: 65536,
            burst: 262144,
            segments: 4,
            segment_bytes: 16 * 1024 * 1024,
            expiry: Duration::from_secs(86400),
        }
    }
    pub(crate) fn validate(&self) -> io::Result<()> {
        if self.record_bytes < 1024
            || self.record_bytes > 16384
            || self.queue_bytes < self.record_bytes
            || self.queue_bytes > 8 * 1024 * 1024
            || self.queue_count == 0
            || self.queue_count > 256
            || self.rate == 0
            || self.rate > 1024 * 1024
            || self.burst < self.record_bytes * if self.mode == OutputMode::Both { 2 } else { 1 }
            || self.burst > 1024 * 1024
            || self.segments == 0
            || self.segments > 16
            || self.segment_bytes < self.record_bytes
            || self.segment_bytes > 16 * 1024 * 1024
            || self.expiry.is_zero()
        {
            return Err(io::Error::other("output limits"));
        }
        if self.directory.as_ref().is_some_and(|p| p.capacity() > 4096) {
            return Err(io::Error::other("output path bound"));
        }
        if self.mode != OutputMode::Forward && self.directory.is_none() {
            return Err(io::Error::other("output path required"));
        }
        Ok(())
    }
}
struct Record {
    bytes: Vec<u8>,
    _producer: Option<Arc<super::collector::Registration>>,
}
#[derive(Default)]
struct Queue {
    records: VecDeque<Record>,
    bytes: usize,
    active: bool,
}
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    stop: AtomicBool,
    closed: AtomicBool,
    dropped: crate::health::Counter,
    failed: crate::health::Counter,
    config: OutputConfig,
}
/// Fixed health counters; no recursive diagnostic queue.
#[derive(Clone, Copy, Debug)]
pub struct Loss {
    /// Dropped records, including capacity/shutdown loss.
    pub dropped: u64,
    /// Failed writes or maintenance attempts.
    pub failed: u64,
    /// At least one loss counter saturated.
    pub overflow: bool,
}
/// Shared producer handle. Clones share one queue and one writer.
#[derive(Clone)]
pub struct Output {
    owner: Arc<Owner>,
}
struct Owner {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Output {
    /// Starts explicit output after validation; failure is independent of product work.
    pub fn start(config: OutputConfig) -> io::Result<Self> {
        config.validate()?;
        let local = if config.mode == OutputMode::Forward {
            None
        } else {
            Some(Local::new(
                config
                    .directory
                    .as_ref()
                    .ok_or_else(|| io::Error::other("output path"))?,
                config.segments,
                config.segment_bytes,
                config.expiry,
            )?)
        };
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                records: VecDeque::with_capacity(config.queue_count),
                ..Queue::default()
            }),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            dropped: crate::health::Counter::new(),
            failed: crate::health::Counter::new(),
            config,
        });
        let owner = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("layerfs-output".into())
            .stack_size(256 * 1024)
            .spawn(move || write_loop(owner, local))?;
        Ok(Self {
            owner: Arc::new(Owner {
                shared,
                worker: Mutex::new(Some(worker)),
            }),
        })
    }
    /// Nonblocking ownership transfer; capacity charges Vec allocation, not length.
    pub fn submit(&self, bytes: Vec<u8>) {
        self.submit_registered(bytes, None);
    }
    pub(crate) fn submit_registered(
        &self,
        bytes: Vec<u8>,
        producer: Option<Arc<super::collector::Registration>>,
    ) {
        let s = &self.owner.shared;
        let charge = bytes.capacity();
        if s.closed.load(Ordering::Acquire)
            || bytes.len() > s.config.record_bytes
            || charge > s.config.queue_bytes
        {
            s.dropped.add(1);
            return;
        }
        let Ok(mut q) = s.queue.try_lock() else {
            s.dropped.add(1);
            return;
        };
        // Shutdown may have begun between the optimistic check and this lock.
        if s.closed.load(Ordering::Acquire) {
            s.dropped.add(1);
            return;
        }
        while q.bytes + charge > s.config.queue_bytes
            || q.records.len() + usize::from(q.active) >= s.config.queue_count
        {
            let Some(old) = q.records.pop_front() else {
                s.dropped.add(1);
                return;
            };
            q.bytes -= old.bytes.capacity();
            s.dropped.add(1);
        }
        q.bytes += charge;
        q.records.push_back(Record {
            bytes,
            _producer: producer,
        });
        drop(q);
        s.wake.notify_one();
    }
    /// Charges omitted detail without allocating a recursive failure report.
    pub fn omit(&self) {
        self.owner.shared.dropped.add(1);
    }
    /// Fixed health snapshot; not a promise that output arrived at its destination.
    pub fn loss(&self) -> Loss {
        Loss {
            dropped: self.owner.shared.dropped.value(),
            failed: self.owner.shared.failed.value(),
            overflow: self.owner.shared.failed.overflowed()
                || self.owner.shared.dropped.overflowed(),
        }
    }
    /// Stops new output, attempts a bounded wait, then releases the join handle.
    /// Blocking local filesystem syscalls are not claimed to be preemptible.
    pub fn shutdown(&self, allowance: Duration) {
        self.owner.shared.closed.store(true, Ordering::Release);
        self.owner.shared.wake.notify_all();
        let end = Instant::now() + allowance.min(Duration::from_secs(2));
        if let Ok(mut slot) = self.owner.worker.try_lock() {
            if let Some(worker) = slot.take() {
                while !worker.is_finished() && Instant::now() < end {
                    thread::park_timeout(Duration::from_millis(10));
                }
                if worker.is_finished() {
                    let _ = worker.join();
                }
            }
        }
        self.owner.shared.stop.store(true, Ordering::Release);
        self.owner.shared.wake.notify_all();
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        // Arc drops this owner exactly once, even when final handles disappear
        // concurrently. The worker owns Shared, never Owner.
        self.shared.closed.store(true, Ordering::Release);
        self.shared.stop.store(true, Ordering::Release);
        self.shared.wake.notify_all();
    }
}
fn write_loop(s: Arc<Shared>, mut local: Option<Local>) {
    let mut forward = s.config.mode != OutputMode::Local;
    let mut tokens = s.config.burst as f64;
    let mut last = Instant::now();
    loop {
        let record = {
            let Ok(mut q) = s.queue.lock() else {
                return;
            };
            if s.stop.load(Ordering::Acquire) {
                s.dropped.add(q.records.len() as u64);
                q.records.clear();
                q.bytes = 0;
                return;
            }
            if s.closed.load(Ordering::Acquire) && q.records.is_empty() {
                return;
            }
            let Some(bytes) = q.records.pop_front() else {
                let _ = s.wake.wait_timeout(q, Duration::from_secs(1));
                if let Some(l) = local.as_mut() {
                    if l.maintain().is_err() {
                        s.failed.add(1);
                        local = None;
                    }
                }
                continue;
            };
            q.active = true;
            bytes
        };
        let bytes = &record.bytes;
        let destinations = usize::from(forward) + usize::from(local.is_some());
        let cost = bytes.len() * destinations;
        let now = Instant::now();
        tokens = (tokens + now.duration_since(last).as_secs_f64() * s.config.rate as f64)
            .min(s.config.burst as f64);
        last = now;
        if destinations == 0 || tokens < cost as f64 {
            s.dropped.add(1);
        } else {
            tokens -= cost as f64;
            if forward && forward_record(bytes, &s.stop).is_err() {
                forward = false;
                s.failed.add(1);
            }
            if let Some(l) = local.as_mut() {
                if l.write(bytes).is_err() {
                    local = None;
                    s.failed.add(1);
                }
            }
        }
        if let Ok(mut q) = s.queue.lock() {
            q.bytes -= bytes.capacity();
            q.active = false;
        }
    }
}
fn forward_record(bytes: &[u8], stop: &AtomicBool) -> io::Result<()> {
    let stderr = io::stderr();
    let end = Instant::now() + Duration::from_secs(1);
    for chunk in bytes.chunks(512) {
        loop {
            if stop.load(Ordering::Acquire) || Instant::now() >= end {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut fds = [PollFd::new(stderr.as_fd(), PollFlags::POLLOUT)];
            if poll(&mut fds, 50u16).map_err(io::Error::other)? > 0 {
                break;
            }
        }
        let n = unistd::write(&stderr, chunk).map_err(io::Error::other)?;
        if n != chunk.len() {
            return Err(io::ErrorKind::WriteZero.into());
        }
    }
    Ok(())
}
