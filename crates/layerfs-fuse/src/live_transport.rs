use crate::live_runtime::{LiveRuntime, Scheduler};
use crate::live_wire::{invalid, MAX_FRAME};
use crate::write_metrics::AtomicFuseWriteMetrics;
use crate::{PortError, PortResult};
use std::io;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Mutex, Semaphore};

type ControlSlot = (Arc<Mutex<Option<TcpStream>>>, Arc<tokio::sync::Notify>);

pub type BackingHandler = dyn Fn(&[u8]) -> PortResult<Vec<u8>> + Send + Sync;

fn carries_append(bytes: &[u8]) -> bool {
    bytes.first() == Some(&crate::live_wire::APPEND)
        || (bytes.first() == Some(&crate::live_wire::BATCH)
            && crate::live_wire::batch_frames(bytes).is_ok_and(|frames| {
                frames
                    .iter()
                    .any(|frame| frame[0] == crate::live_wire::APPEND)
            }))
}

pub struct BackingServer {
    port: u16,
    capability: [u8; 32],
    stop: watch::Sender<bool>,
    listener: Option<tokio::task::JoinHandle<()>>,
    local: Option<crate::live_owner::LiveOwner>,
    failed: Arc<AtomicBool>,
    control: Arc<Mutex<Option<TcpStream>>>,
    connected: Arc<tokio::sync::Notify>,
    observer: Arc<Mutex<Option<TcpStream>>>,
    observed: Arc<tokio::sync::Notify>,
    snapshot: Arc<Mutex<Option<TcpStream>>>,
    snapshotted: Arc<tokio::sync::Notify>,
    metrics: Arc<AtomicFuseWriteMetrics>,
}

impl BackingServer {
    pub fn start(
        handler: impl Fn(&[u8]) -> PortResult<Vec<u8>> + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let runtime = LiveRuntime::shared()?;
        let scheduler = runtime.scheduler();
        let listener = runtime.block_on(TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)))?;
        let port = listener.local_addr()?.port();
        let capability = crate::proxy_host::capability()?;
        let handler = Arc::new(handler);
        let failed = Arc::new(AtomicBool::new(false));
        let (stop, mut stopping) = watch::channel(false);
        let fail = failed.clone();
        let connections = Arc::new(Semaphore::new(3));
        let control = Arc::new(Mutex::new(None));
        let connected = Arc::new(tokio::sync::Notify::new());
        let metrics = Arc::new(AtomicFuseWriteMetrics::default());
        let host_metrics = metrics.clone();
        let observer = Arc::new(Mutex::new(None));
        let observed = Arc::new(tokio::sync::Notify::new());
        let observe_slot = observer.clone();
        let observe_ready = observed.clone();
        let snapshot = Arc::new(Mutex::new(None));
        let snapshotted = Arc::new(tokio::sync::Notify::new());
        let snapshot_slot = snapshot.clone();
        let snapshot_ready = snapshotted.clone();
        let control_slot = control.clone();
        let control_ready = connected.clone();
        let task = scheduler.handle.spawn({let scheduler=scheduler.clone();async move {
            loop {
                let slot = tokio::select! {
                    _ = stopping.changed() => break,
                    slot = connections.clone().acquire_owned() => match slot {Ok(slot)=>slot,Err(_)=>break},
                };
                let (stream,_) = tokio::select! {
                    _ = stopping.changed() => break,
                    accepted = listener.accept() => match accepted {Ok(value)=>value,Err(_)=>{fail.store(true,Ordering::Release);break}},
                };
                let mut closed = stopping.clone();
                let handler = handler.clone(); let scheduler=scheduler.clone(); let fail=fail.clone();
                let control=control_slot.clone(); let connected=control_ready.clone(); let metrics=host_metrics.clone();
                let observer=observe_slot.clone(); let observed=observe_ready.clone();
                let snapshot=snapshot_slot.clone(); let snapshotted=snapshot_ready.clone();
                scheduler.handle.clone().spawn(async move {
                    let _slot=slot;
                    tokio::select! {
                        _ = closed.changed() => {},
                        result = serve(stream,capability,scheduler,handler,(control,connected),(observer,observed),(snapshot,snapshotted),metrics) => {
                            if result.is_err() {fail.store(true,Ordering::Release);}
                        }
                    }
                });
            }
        }});
        Ok(Self {
            port,
            capability,
            stop,
            listener: Some(task),
            local: None,
            failed,
            control,
            connected,
            observer,
            observed,
            snapshot,
            snapshotted,
            metrics,
        })
    }
    pub fn local(owner: crate::live_owner::LiveOwner) -> Self {
        let (stop, _) = watch::channel(false);
        Self {
            port: 0,
            capability: [0; 32],
            stop,
            listener: None,
            local: Some(owner),
            failed: Arc::new(AtomicBool::new(false)),
            control: Arc::new(Mutex::new(None)),
            observer: Arc::new(Mutex::new(None)),
            snapshot: Arc::new(Mutex::new(None)),
            connected: Arc::new(tokio::sync::Notify::new()),
            observed: Arc::new(tokio::sync::Notify::new()),
            snapshotted: Arc::new(tokio::sync::Notify::new()),
            metrics: Arc::new(AtomicFuseWriteMetrics::default()),
        }
    }
    pub fn local_owner(&self) -> Option<crate::live_owner::LiveOwner> {
        self.local.clone()
    }

    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn capability(&self) -> [u8; 32] {
        self.capability
    }
    pub fn healthy(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
    }

    /// Read-only per-owner counters, sampled around an opt-in diagnostic edit.
    pub fn backing_diagnostic_snapshot(&self) -> (u64, u64) {
        self.metrics.backing_snapshot()
    }

    pub fn request(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        self.request_group(std::iter::once(bytes))
    }

    /// Serialize one bounded control transaction so other callers cannot interleave its pages.
    pub fn request_group<B: AsRef<[u8]>>(
        &self,
        frames: impl IntoIterator<Item = B>,
    ) -> PortResult<Vec<u8>> {
        self.request_on(&self.control, &self.connected, frames)
    }

    pub fn observe(&self) -> PortResult<Vec<u8>> {
        self.request_on(
            &self.observer,
            &self.observed,
            std::iter::once(&[crate::live_wire::OBSERVE][..]),
        )
    }

    /// One host-initiated snapshot-lane frame: frozen records or payload
    /// bytes for the active capture. Bulk transfer here cannot occupy the
    /// control lane or the immutable-base data lane.
    pub fn snapshot_request(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        let runtime = LiveRuntime::shared().map_err(|_| PortError::Io)?;
        let outcome: PortResult<Vec<u8>> = runtime.block_on(async {
            match tokio::time::timeout(std::time::Duration::from_secs(120), async {
                if let Some(owner) = &self.local {
                    let _held = self.snapshot.lock().await;
                    return owner.snapshot_request(bytes).await;
                }
                let ready = self.snapshotted.notified();
                if self.snapshot.lock().await.is_none() {
                    ready.await;
                }
                let mut held = self.snapshot.lock().await;
                let mut stream = held.take().ok_or(PortError::Io)?;
                let response = exchange(&mut stream, bytes, None)
                    .await
                    .map_err(|_| PortError::Io)?;
                *held = Some(stream);
                response
            })
            .await
            {
                Ok(result) => result,
                Err(_) => Err(PortError::Io),
            }
        });
        outcome
    }

    fn request_on<B: AsRef<[u8]>>(
        &self,
        slot: &Mutex<Option<TcpStream>>,
        connected: &tokio::sync::Notify,
        frames: impl IntoIterator<Item = B>,
    ) -> PortResult<Vec<u8>> {
        LiveRuntime::shared()
            .map_err(|_| PortError::Io)?
            .block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(120), async {
                    if let Some(owner) = &self.local {
                        let _held = slot.lock().await;
                        let mut response = Vec::new();
                        for bytes in frames {
                            response = owner.local_control(bytes.as_ref()).await?;
                        }
                        return Ok(response);
                    }
                    let ready = connected.notified();
                    if slot.lock().await.is_none() {
                        ready.await;
                    }
                    let mut held = slot.lock().await;
                    let mut stream = held.take().ok_or(PortError::Io)?;
                    let mut response = Ok(Vec::new());
                    for bytes in frames {
                        response = exchange(&mut stream, bytes.as_ref(), None)
                            .await
                            .map_err(|_| PortError::Io)?;
                        if response.is_err() {
                            break;
                        }
                    }
                    *held = Some(stream);
                    response
                })
                .await
                .map_err(|_| PortError::Io)?
            })
    }

    pub fn control(&self, command: &str) -> PortResult<()> {
        let opcode = match command {
            "pause" => crate::live_wire::FREEZE,
            "resume" => crate::live_wire::RESUME,
            "shutdown" => crate::live_wire::SHUTDOWN,
            _ => return Err(PortError::Invalid),
        };
        self.request(&[opcode]).map(drop)
    }

    pub fn failure(&self) -> Option<(&'static str, PortError)> {
        (!self.healthy()).then_some(("live backing", PortError::Io))
    }

    pub fn take_write_metrics(&self) -> PortResult<crate::FuseWriteMetrics> {
        let bytes = self.request(&[crate::live_wire::WRITE_METRICS])?;
        let mut metrics =
            crate::FuseWriteMetrics::read_from(&mut bytes.as_slice()).map_err(|_| PortError::Io)?;
        metrics.merge(self.metrics.take());
        Ok(metrics)
    }

    pub fn take_read_metrics(&self) -> PortResult<crate::FuseReadMetrics> {
        let bytes = self.request(&[crate::live_wire::READ_METRICS])?;
        crate::FuseReadMetrics::read_from(&mut bytes.as_slice()).map_err(|_| PortError::Io)
    }

    pub fn invalidate_file(&self, node: crate::NodeId) -> PortResult<()> {
        let mut request = vec![crate::live_wire::INVALIDATE];
        crate::live_wire::u64_out(&mut request, node.0);
        self.request(&request).map(drop)
    }
}
impl Drop for BackingServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(listener) = &self.listener {
            listener.abort();
        }
    }
}

async fn serve(
    mut stream: TcpStream,
    capability: [u8; 32],
    scheduler: Scheduler,
    handler: Arc<impl Fn(&[u8]) -> PortResult<Vec<u8>> + Send + Sync + 'static>,
    (control, connected): ControlSlot,
    (observer, observed): ControlSlot,
    (snapshot, snapshotted): ControlSlot,
    metrics: Arc<AtomicFuseWriteMetrics>,
) -> io::Result<()> {
    stream.set_nodelay(true)?;
    let mut presented = [0; 32];
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        stream.read_exact(&mut presented),
    )
    .await??;
    if presented
        .iter()
        .zip(capability)
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "live backing capability",
        ));
    }
    let role = stream.read_u8().await?;
    if role == b'c' || role == b'o' || role == b's' {
        let (control, connected) = match role {
            b'o' => (observer, observed),
            b's' => (snapshot, snapshotted),
            _ => (control, connected),
        };
        let mut slot = control.lock().await;
        if slot.is_some() {
            return Err(invalid());
        }
        stream.write_u8(1).await?;
        *slot = Some(stream);
        connected.notify_one();
        return Ok(());
    }
    if role != b'd' {
        return Err(invalid());
    }
    stream.write_u8(1).await?;
    loop {
        let length = match stream.read_u32().await {
            Ok(length) => length as usize,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        if length == 0 || length > MAX_FRAME {
            return Err(invalid());
        }
        // Input, decoded temporary state and a bounded response are admitted
        // before retaining any frame body or entering the physical queue.
        let admitted = scheduler.admit(length + 2 * MAX_FRAME).await?;
        let mut bytes = vec![0; length];
        let read = Instant::now();
        stream.read_exact(&mut bytes).await?;
        metrics.note_host_frame(
            if carries_append(&bytes) {
                (length + 4) as u64
            } else {
                0
            },
            0,
            ns(read),
            0,
        );
        let handler = handler.clone();
        let metrics = metrics.clone();
        let queued = Instant::now();
        let response = scheduler
            .physical(move || {
                metrics
                    .live_backing_queue_ns
                    .fetch_add(ns(queued), Ordering::Relaxed);
                let started = Instant::now();
                let result = handler(&bytes);
                metrics.note_host_dispatch(ns(started));
                Ok(result)
            })
            .await?;
        match response {
            Ok(bytes) => {
                if bytes.len() > MAX_FRAME {
                    return Err(invalid());
                }
                write_frame(&mut stream, Some(0), &bytes).await?;
            }
            Err(error) => {
                write_frame(&mut stream, Some(1), &[crate::protocol::error_code(error)]).await?;
            }
        }
        drop(admitted);
    }
}

pub struct BackingConnection {
    // A cancelled/failed exchange drops the taken stream. It cannot reuse a
    // partial frame or silently repeat an append after an uncertain reply.
    stream: Mutex<Option<TcpStream>>,
    pub(crate) metrics: Arc<AtomicFuseWriteMetrics>,
    local: Option<(Arc<BackingHandler>, Scheduler)>,
    available: AtomicBool,
}

pub(crate) struct BatchProgress {
    pub completed: usize,
    pub error: Option<PortError>,
    pub uncertain: bool,
}
struct ExchangeFailure {
    error: PortError,
    uncertain: bool,
}
impl BatchProgress {
    pub fn result(&self) -> PortResult<()> {
        self.error.map_or(Ok(()), Err)
    }
}
impl BackingConnection {
    pub async fn connect(
        endpoint: String,
        capability: [u8; 32],
        scheduler: &Scheduler,
    ) -> io::Result<Self> {
        use std::net::ToSocketAddrs;
        let address = scheduler
            .physical(move || endpoint.to_socket_addrs()?.next().ok_or_else(invalid))
            .await?;
        let mut stream = TcpStream::connect(address).await?;
        stream.set_nodelay(true)?;
        stream.write_all(&capability).await?;
        stream.write_u8(b'd').await?;
        if stream.read_u8().await? != 1 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "live backing capability",
            ));
        }
        Ok(Self {
            stream: Mutex::new(Some(stream)),
            metrics: Default::default(),
            local: None,
            available: AtomicBool::new(true),
        })
    }
    pub fn local(handler: Arc<BackingHandler>, scheduler: Scheduler) -> Self {
        Self {
            stream: Mutex::new(None),
            metrics: Default::default(),
            local: Some((handler, scheduler)),
            available: AtomicBool::new(true),
        }
    }
    pub async fn call(&self, bytes: &[u8]) -> PortResult<Vec<u8>> {
        self.call_exchange(bytes)
            .await
            .map_err(|failure| failure.error)?
    }

    /// Preserve acknowledged prefixes even when a later operation fails. Large
    /// transactions keep the original frame-at-a-time route and its bounds.
    pub(crate) async fn call_batch(
        &self,
        frames: &[&[u8]],
        scheduler: &Scheduler,
    ) -> BatchProgress {
        let failure = |completed, error, uncertain| BatchProgress {
            completed,
            error: Some(error),
            uncertain,
        };
        let mut size = 5usize;
        for frame in frames {
            if crate::live_wire::validate_batch_frame(frame).is_err() {
                return failure(0, PortError::Invalid, false);
            }
            let Some(next) = size.checked_add(4).and_then(|n| n.checked_add(frame.len())) else {
                return failure(0, PortError::Invalid, false);
            };
            size = next;
        }
        if frames.len() <= 1
            || frames.len() > crate::live_wire::MAX_BATCH_FRAMES
            || size > MAX_FRAME
        {
            return self.call_serial(frames).await;
        }
        // The copied envelope coexists with its already-owned source frames.
        // Coalescing is optional: lack of extra room must not make a previously
        // valid streaming barrier fail or hold permits while awaiting fallback.
        let envelope = (|| -> io::Result<_> {
            let mut charge = scheduler.reserve_transfer(size)?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(size).map_err(io::Error::other)?;
            charge.merge(scheduler.reserve_transfer(bytes.capacity().saturating_sub(size))?);
            Ok((bytes, charge))
        })();
        let (mut bytes, _charge) = match envelope {
            Ok(envelope) => envelope,
            Err(_) => return self.call_serial(frames).await,
        };
        bytes.push(crate::live_wire::BATCH);
        bytes.extend_from_slice(&(frames.len() as u32).to_be_bytes());
        for frame in frames {
            bytes.extend_from_slice(&(frame.len() as u32).to_be_bytes());
            bytes.extend_from_slice(frame);
        }
        let copied = frames
            .iter()
            .filter(|frame| frame[0] == crate::live_wire::APPEND)
            .map(|frame| frame.len().saturating_sub(21) as u64)
            .sum();
        self.metrics.note_client_frame(0, copied, 0, 0);
        match self.call_exchange(&bytes).await {
            Ok(Ok(response)) => {
                match crate::live_wire::parse_batch_reply(&response, frames.len()) {
                    Ok((completed, error)) => BatchProgress {
                        completed,
                        error,
                        uncertain: false,
                    },
                    Err(_) => {
                        self.poison().await;
                        failure(0, PortError::Io, true)
                    }
                }
            }
            Ok(Err(error)) => failure(0, error, false),
            Err(error) if !error.uncertain && error.error == PortError::NoSpace => {
                drop(bytes);
                drop(_charge);
                self.call_serial(frames).await
            }
            Err(error) => failure(0, error.error, error.uncertain),
        }
    }

    async fn call_serial(&self, frames: &[&[u8]]) -> BatchProgress {
        for (completed, frame) in frames.iter().enumerate() {
            let (error, uncertain) = match self.call_exchange(frame).await {
                Ok(Ok(response)) if response.is_empty() => continue,
                Ok(Err(error)) => (error, false),
                Err(error) => (error.error, error.uncertain),
                _ => {
                    self.poison().await;
                    (PortError::Io, true)
                }
            };
            return BatchProgress {
                completed,
                error: Some(error),
                uncertain,
            };
        }
        BatchProgress {
            completed: frames.len(),
            error: None,
            uncertain: false,
        }
    }

    async fn poison(&self) {
        self.available.store(false, Ordering::Release);
        self.stream.lock().await.take();
    }

    async fn call_exchange(&self, bytes: &[u8]) -> Result<PortResult<Vec<u8>>, ExchangeFailure> {
        let uncertain = || ExchangeFailure {
            error: PortError::Io,
            uncertain: true,
        };
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Ok(Err(PortError::Invalid));
        }
        let started = Instant::now();
        self.metrics
            .live_backing_calls
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .live_backing_request_bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        let mut held = self.stream.lock().await;
        if let Some((handler, scheduler)) = &self.local {
            if !self.available.load(Ordering::Acquire) {
                return Err(uncertain());
            }
            let charge = match scheduler.reserve_transfer(bytes.len() + 2 * MAX_FRAME) {
                Ok(charge) => charge,
                // No request has entered the physical worker or reached a peer.
                Err(_) => {
                    return Err(ExchangeFailure {
                        error: PortError::NoSpace,
                        uncertain: false,
                    })
                }
            };
            self.metrics.note_client_frame(
                0,
                if carries_append(bytes) {
                    bytes.len() as u64
                } else {
                    0
                },
                0,
                0,
            );
            let bytes = bytes.to_vec();
            let handler = handler.clone();
            let metrics = self.metrics.clone();
            self.available.store(false, Ordering::Release);
            let result = scheduler
                .physical(move || {
                    let _charge = charge;
                    metrics
                        .live_backing_queue_ns
                        .fetch_add(ns(started), Ordering::Relaxed);
                    let work = Instant::now();
                    let response = handler(&bytes);
                    metrics.note_host_dispatch(ns(work));
                    Ok(response)
                })
                .await
                .map_err(|_| uncertain())?;
            self.available.store(true, Ordering::Release);
            self.metrics
                .live_backing_wait_ns
                .fetch_add(ns(started), Ordering::Relaxed);
            return Ok(result);
        }
        let mut stream = held.take().ok_or_else(uncertain)?;
        let result = exchange(&mut stream, bytes, Some(&self.metrics)).await;
        self.metrics
            .live_backing_wait_ns
            .fetch_add(ns(started), Ordering::Relaxed);
        match result {
            Ok(result) => {
                *held = Some(stream);
                Ok(result)
            }
            Err(_) => Err(uncertain()),
        }
    }
}

async fn exchange(
    stream: &mut TcpStream,
    bytes: &[u8],
    metrics: Option<&AtomicFuseWriteMetrics>,
) -> io::Result<PortResult<Vec<u8>>> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    let written = Instant::now();
    write_frame(stream, None, bytes).await?;
    if let Some(metrics) = metrics {
        metrics.note_client_frame(
            if carries_append(bytes) {
                (bytes.len() + 4) as u64
            } else {
                0
            },
            0,
            0,
            ns(written),
        );
    }
    let length = stream.read_u32().await? as usize;
    if length == 0 || length > MAX_FRAME + 1 {
        return Err(invalid());
    }
    let status = stream.read_u8().await?;
    match status {
        0 => {
            let mut bytes = vec![0; length - 1];
            stream.read_exact(&mut bytes).await?;
            Ok(Ok(bytes))
        }
        1 if length == 2 => Ok(Err(crate::protocol::port_error(stream.read_u8().await?)?)),
        _ => Err(invalid()),
    }
}

fn ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

/// Send the framing prefix and existing payload in the same writev without
/// another payload allocation. Advance both slices on short socket writes.
pub(crate) async fn write_frame(
    output: &mut (impl tokio::io::AsyncWrite + Unpin),
    status: Option<u8>,
    bytes: &[u8],
) -> io::Result<()> {
    use std::io::IoSlice;
    if bytes.len() > MAX_FRAME || (status.is_none() && bytes.is_empty()) {
        return Err(invalid());
    }
    let mut header = [0; 5];
    let length = bytes.len() + usize::from(status.is_some());
    header[..4].copy_from_slice(&(length as u32).to_be_bytes());
    let prefix = if let Some(status) = status {
        header[4] = status;
        &header[..]
    } else {
        &header[..4]
    };
    let mut storage = [IoSlice::new(prefix), IoSlice::new(bytes)];
    let mut slices = &mut storage[..];
    while !slices.is_empty() {
        let written = output.write_vectored(slices).await?;
        if written == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "live frame"));
        }
        IoSlice::advance_slices(&mut slices, written);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backing_batch_preserves_prefix_and_poisoned_reply_cannot_replay() {
        let runtime = LiveRuntime::new().unwrap();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = calls.clone();
        let connection = BackingConnection::local(
            Arc::new(move |bytes| {
                let frames = crate::live_wire::batch_frames(bytes).unwrap();
                let count = seen.fetch_add(1, Ordering::Relaxed);
                Ok(if count == 0 {
                    crate::live_wire::batch_reply(1, Some(PortError::NoSpace))
                } else {
                    // Success with an incomplete prefix is an invalid receipt.
                    crate::live_wire::batch_reply(frames.len() - 1, None)
                })
            }),
            runtime.scheduler(),
        );
        runtime.block_on(async {
            let frames = [&[crate::live_wire::CHECK, 1][..]; 2];
            let result = connection.call_batch(&frames, &runtime.scheduler()).await;
            assert_eq!(result.completed, 1);
            assert_eq!(result.error, Some(PortError::NoSpace));
            assert!(!result.uncertain);
            let result = connection.call_batch(&frames, &runtime.scheduler()).await;
            assert!(result.uncertain);
            assert_eq!(result.error, Some(PortError::Io));
            assert_eq!(connection.call(frames[0]).await, Err(PortError::Io));
            assert_eq!(calls.load(Ordering::Relaxed), 2);
        });
    }

    #[test]
    fn backing_batch_pressure_and_oversize_preserve_streaming() {
        let runtime = LiveRuntime::new().unwrap();
        let budget = LiveRuntime::new().unwrap();
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = calls.clone();
        let connection = BackingConnection::local(
            Arc::new(move |bytes| {
                seen.lock().unwrap().push(bytes[0]);
                assert_ne!(bytes[0], crate::live_wire::BATCH);
                Ok(Vec::new())
            }),
            runtime.scheduler(),
        );
        runtime.block_on(async {
            let scheduler = budget.scheduler();
            let held = scheduler.reserve_transfer(32 * 1024 * 1024).unwrap();
            let small = [&[crate::live_wire::CHECK, 1][..]; 2];
            let result = connection.call_batch(&small, &scheduler).await;
            assert_eq!(result.completed, 2);
            assert_eq!(result.result(), Ok(()));
            drop(held);
            let mut large = vec![crate::live_wire::FACTS_NODE_CHUNK; MAX_FRAME];
            large[0] = crate::live_wire::FACTS_NODE_CHUNK;
            let result = connection
                .call_batch(&[large.as_slice(), small[0]], &scheduler)
                .await;
            assert_eq!(result.completed, 2);
            assert_eq!(result.result(), Ok(()));
            assert_eq!(calls.lock().unwrap().len(), 4);
            // Neither path retains optional envelope transfer capacity.
            assert!(scheduler.reserve_transfer(32 * 1024 * 1024).is_ok());
            // The envelope itself can fit while leaving too little for local
            // dispatch; drop that optional copy and admit the original frames.
            let mut append = vec![crate::live_wire::APPEND];
            crate::live_wire::u64_out(&mut append, 7);
            crate::live_wire::u64_out(&mut append, 0);
            crate::live_wire::bytes_out(&mut append, &vec![42; 640 * 1024]).unwrap();
            let scheduler = runtime.scheduler();
            let available = append.len() + 2 * MAX_FRAME + 64;
            let _held = scheduler
                .reserve_transfer(32 * 1024 * 1024 - available)
                .unwrap();
            let result = connection
                .call_batch(&[&append, small[0]], &scheduler)
                .await;
            assert_eq!(result.result(), Ok(()));
            assert_eq!(result.completed, 2);
        });
    }

    #[test]
    fn backing_batch_lost_socket_reply_is_uncertain_and_not_replayed() {
        let runtime = LiveRuntime::new().unwrap();
        runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = listener.local_addr().unwrap().to_string();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut capability = [0; 32];
                stream.read_exact(&mut capability).await.unwrap();
                assert_eq!(capability, [7; 32]);
                assert_eq!(stream.read_u8().await.unwrap(), b'd');
                stream.write_u8(1).await.unwrap();
                let len = stream.read_u32().await.unwrap() as usize;
                let mut bytes = vec![0; len];
                stream.read_exact(&mut bytes).await.unwrap();
                assert_eq!(crate::live_wire::batch_frames(&bytes).unwrap().len(), 2);
                // The peer consumed the request but closes before acknowledging.
            });
            let connection = BackingConnection::connect(endpoint, [7; 32], &runtime.scheduler())
                .await
                .unwrap();
            let frames = [&[crate::live_wire::CHECK, 1][..]; 2];
            let result = connection.call_batch(&frames, &runtime.scheduler()).await;
            assert_eq!(result.completed, 0);
            assert_eq!(result.error, Some(PortError::Io));
            assert!(result.uncertain);
            assert_eq!(connection.call(frames[0]).await, Err(PortError::Io));
            server.await.unwrap();
        });
    }

    #[test]
    fn vectored_frames_preserve_prefixes_across_short_writes() {
        let runtime = LiveRuntime::new().unwrap();
        runtime.block_on(async {
            let (mut writer, mut reader) = tokio::io::duplex(7);
            let payload = vec![42; 1024];
            let expected = payload.clone();
            let send = async {
                write_frame(&mut writer, None, &payload).await.unwrap();
                write_frame(&mut writer, Some(0), &[]).await.unwrap();
                write_frame(&mut writer, Some(1), &[7]).await.unwrap();
            };
            let receive = async {
                assert_eq!(reader.read_u32().await.unwrap(), 1024);
                let mut received = vec![0; 1024];
                reader.read_exact(&mut received).await.unwrap();
                assert_eq!(received, expected);
                assert_eq!(reader.read_u32().await.unwrap(), 1);
                assert_eq!(reader.read_u8().await.unwrap(), 0);
                assert_eq!(reader.read_u32().await.unwrap(), 2);
                assert_eq!(reader.read_u8().await.unwrap(), 1);
                assert_eq!(reader.read_u8().await.unwrap(), 7);
            };
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                tokio::join!(send, receive);
            })
            .await
            .unwrap();
        });
    }
}
