use crate::protocol::{read_request_measured, write_response_measured, Request, Response};
use crate::write_metrics::{AtomicFuseReadMetrics, AtomicFuseWriteMetrics};
use crate::{PortError, PortResult, SharedPort};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

pub struct ProxyHost {
    address: SocketAddr,
    capability: [u8; 32],
    stopped: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<(&'static str, PortError)>>>,
    metrics: Arc<AtomicFuseWriteMetrics>,
    read_metrics: Arc<AtomicFuseReadMetrics>,
    control: Arc<RemoteControl>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[derive(Default)]
struct RemoteControl {
    stream: Mutex<Option<TcpStream>>,
    ready: Condvar,
}

impl ProxyHost {
    pub fn start(port: SharedPort) -> std::io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, 0))?;
        let address = listener.local_addr()?;
        let capability = capability()?;
        let stopped = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(Mutex::new(None));
        let metrics = Arc::new(AtomicFuseWriteMetrics::default());
        let read_metrics = Arc::new(AtomicFuseReadMetrics::default());
        let deferred = Arc::new(Mutex::new(None));
        let data_claimed = Arc::new(AtomicBool::new(false));
        let control_claimed = Arc::new(AtomicBool::new(false));
        let control = Arc::new(RemoteControl::default());
        let thread = {
            let stopped = stopped.clone();
            let failed = failed.clone();
            let failure = failure.clone();
            let metrics = metrics.clone();
            let read_metrics = read_metrics.clone();
            let deferred = deferred.clone();
            let control = control.clone();
            std::thread::spawn(move || {
                while !stopped.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if stopped.load(Ordering::Acquire) {
                                break;
                            }
                            let timeout = Some(std::time::Duration::from_secs(1));
                            if stream.set_nonblocking(false).is_err()
                                || stream.set_read_timeout(timeout).is_err()
                                || stream.set_write_timeout(timeout).is_err()
                                || stream.set_nodelay(true).is_err()
                            {
                                continue;
                            }
                            let mut presented = [0; 33];
                            if stream.read_exact(&mut presented).is_err()
                                || !same_capability(
                                    presented[..32].try_into().expect("capability frame"),
                                    capability,
                                )
                            {
                                let _ = stream.write_all(&[0]);
                                continue;
                            }
                            let claimed = match presented[32] {
                                b'd' => &data_claimed,
                                b'c' => &control_claimed,
                                _ => {
                                    let _ = stream.write_all(&[0]);
                                    continue;
                                }
                            };
                            if claimed
                                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                                .is_err()
                            {
                                let _ = stream.write_all(&[0]);
                                continue;
                            }
                            if stream.write_all(&[1]).is_err()
                                || stream.set_read_timeout(None).is_err()
                                || stream.set_write_timeout(None).is_err()
                            {
                                continue;
                            }
                            if presented[32] == b'c' {
                                if let Ok(mut slot) = control.stream.lock() {
                                    *slot = Some(stream);
                                    control.ready.notify_all();
                                }
                                continue;
                            }
                            let port = port.clone();
                            let failed = failed.clone();
                            let failure = failure.clone();
                            let deferred = deferred.clone();
                            let metrics = metrics.clone();
                            let read_metrics = read_metrics.clone();
                            std::thread::spawn(move || {
                                serve(
                                    stream,
                                    port,
                                    failed,
                                    failure,
                                    deferred,
                                    metrics,
                                    read_metrics,
                                )
                            });
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Ok(Self {
            address,
            capability,
            stopped,
            failed,
            failure,
            metrics,
            read_metrics,
            control,
            thread: Some(thread),
        })
    }

    pub fn port(&self) -> u16 {
        self.address.port()
    }

    pub fn capability(&self) -> [u8; 32] {
        self.capability
    }

    pub fn healthy(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
    }

    pub fn failure(&self) -> Option<(&'static str, PortError)> {
        self.failure.lock().ok().and_then(|failure| *failure)
    }

    pub fn control(&self, command: &str) -> PortResult<()> {
        let command = match command {
            "barrier" => b'b',
            "pause" => b'p',
            "resume" => b'r',
            "shutdown" => b's',
            _ => return Err(PortError::Invalid),
        };
        let mut slot = self.control_stream()?;
        let stream = slot.as_mut().expect("control stream");
        stream.write_all(&[command]).map_err(|_| PortError::Io)?;
        let mut accepted = [0];
        stream
            .read_exact(&mut accepted)
            .map_err(|_| PortError::Io)?;
        if accepted == [1] {
            Ok(())
        } else {
            Err(PortError::Io)
        }
    }

    pub fn take_write_metrics(&self) -> PortResult<crate::FuseWriteMetrics> {
        let mut slot = self.control_stream()?;
        let stream = slot.as_mut().expect("control stream");
        stream.write_all(b"m").map_err(|_| PortError::Io)?;
        let mut accepted = [0];
        stream
            .read_exact(&mut accepted)
            .map_err(|_| PortError::Io)?;
        if accepted != [1] {
            return Err(PortError::Io);
        }
        let mut metrics = crate::FuseWriteMetrics::read_from(stream).map_err(|_| PortError::Io)?;
        metrics.merge(self.metrics.take());
        Ok(metrics)
    }

    pub fn take_read_metrics(&self) -> PortResult<crate::FuseReadMetrics> {
        let mut slot = self.control_stream()?;
        let stream = slot.as_mut().expect("control stream");
        stream.write_all(b"n").map_err(|_| PortError::Io)?;
        let mut accepted = [0];
        stream
            .read_exact(&mut accepted)
            .map_err(|_| PortError::Io)?;
        if accepted != [1] {
            return Err(PortError::Io);
        }
        let mut metrics = crate::FuseReadMetrics::read_from(stream).map_err(|_| PortError::Io)?;
        metrics.merge(self.read_metrics.take());
        Ok(metrics)
    }

    fn control_stream(&self) -> PortResult<std::sync::MutexGuard<'_, Option<TcpStream>>> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut slot = self.control.stream.lock().map_err(|_| PortError::Io)?;
        while slot.is_none() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(PortError::Io);
            }
            let (next, timeout) = self
                .control
                .ready
                .wait_timeout(slot, remaining)
                .map_err(|_| PortError::Io)?;
            slot = next;
            if timeout.timed_out() && slot.is_none() {
                return Err(PortError::Io);
            }
        }
        Ok(slot)
    }
}

impl Drop for ProxyHost {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, self.address.port()));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    mut stream: TcpStream,
    port: SharedPort,
    failed: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<(&'static str, PortError)>>>,
    deferred: Arc<Mutex<Option<PortError>>>,
    metrics: Arc<AtomicFuseWriteMetrics>,
    read_metrics: Arc<AtomicFuseReadMetrics>,
) {
    while let Ok((request, measured)) = read_request_measured(&mut stream) {
        if measured.logical_bytes != 0 {
            metrics.note_host_frame(
                measured.frame_bytes,
                measured.payload_copy_bytes,
                measured.socket_ns,
                measured.decode_ns,
            );
        }
        let no_reply = request.no_reply();
        let is_read = matches!(&request, Request::Read(..));
        let acknowledges = request.acknowledges_deferred_error();
        let name = request.name();
        let started = std::time::Instant::now();
        let response = if acknowledges {
            acknowledge(&deferred).map_or_else(|| dispatch(port.as_ref(), request), Response::Error)
        } else {
            dispatch(port.as_ref(), request)
        };
        if measured.logical_bytes != 0 {
            metrics.note_host_dispatch(elapsed_ns(started));
        }
        if is_read {
            read_metrics.note_host_dispatch(elapsed_ns(started));
        }
        if no_reply {
            if let Response::Error(error) = response {
                failed.store(true, Ordering::Release);
                if let Ok(mut failure) = failure.lock() {
                    failure.get_or_insert((name, error));
                }
                if let Ok(mut deferred) = deferred.lock() {
                    deferred.get_or_insert(error);
                }
            }
            continue;
        }
        match write_response_measured(&mut stream, &response) {
            Ok(measured) => {
                if is_read {
                    read_metrics.note_host_response(
                        measured.frame_bytes,
                        measured.logical_bytes,
                        measured.payload_copy_bytes,
                        measured.encode_ns,
                        measured.socket_ns,
                    );
                }
            }
            Err(_) => break,
        }
    }
}

fn elapsed_ns(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn acknowledge(deferred: &Mutex<Option<PortError>>) -> Option<PortError> {
    deferred
        .lock()
        .map_or(Some(PortError::Io), |mut deferred| deferred.take())
}

fn dispatch(port: &dyn crate::FilesystemPort, request: Request) -> Response {
    match request {
        Request::Lookup(parent, name) => response_attr(port.lookup(parent, &name)),
        Request::Attr(node) => response_attr(port.attr(node)),
        Request::Readlink(node) => response_bytes(port.readlink(node)),
        Request::Readdir(node) => match port.readdir(node) {
            Ok(entries) => Response::Entries(entries),
            Err(error) => Response::Error(error),
        },
        Request::CreateFile(parent, name, mode) => {
            response_attr(port.create_file(parent, &name, mode))
        }
        Request::Mkdir(parent, name, mode) => response_attr(port.mkdir(parent, &name, mode)),
        Request::Symlink(parent, name, target) => {
            response_attr(port.symlink(parent, &name, target))
        }
        Request::Link(node, parent, name) => response_attr(port.link(node, parent, &name)),
        Request::Unlink(parent, name, directory) => {
            response_unit(port.unlink(parent, &name, directory))
        }
        Request::Rename(parent, name, target, target_name, no_replace) => {
            response_unit(port.rename(parent, &name, target, &target_name, no_replace))
        }
        Request::Pin(node, truncate, writable) => response_unit(port.pin(node, truncate, writable)),
        Request::Unpin(node, writable) => response_unit(port.unpin(node, writable)),
        Request::Read(node, offset, size) => response_bytes(port.read(node, offset, size)),
        Request::Write(node, offset, bytes) => match port.write(node, offset, &bytes) {
            Ok(size) => Response::Size(size),
            Err(error) => Response::Error(error),
        },
        Request::Truncate(node, size) => response_unit(port.truncate(node, size)),
        Request::Chmod(node, mode) => response_unit(port.chmod(node, mode)),
        Request::SetMtime(node, seconds, nanos) => {
            response_unit(port.set_mtime(node, seconds, nanos))
        }
        Request::Fsync(node) => response_unit(port.fsync(node)),
        Request::CreateFileOpen(parent, name, mode) => {
            response_attr(port.create_file_open(parent, &name, mode))
        }
        Request::ReserveNodes(count) => match port.reserve_nodes(count) {
            Ok(node) => Response::Node(node),
            Err(error) => Response::Error(error),
        },
        Request::CreateFileOpenReserved(parent, name, mode, node) => {
            response_attr(port.create_file_open_reserved(parent, &name, mode, node))
        }
        Request::CreateFilesClosedReserved(entries) => {
            response_unit(port.create_files_closed_reserved(&entries))
        }
        Request::UnlinkBatch(entries) => response_unit(port.unlink_batch(&entries)),
        Request::WriteZero(node, offset, len) => {
            match port.write_zero(node, offset, len as usize) {
                Ok(size) if size == len as usize => Response::Unit,
                Ok(_) => Response::Error(crate::PortError::Io),
                Err(error) => Response::Error(error),
            }
        }
        Request::Fence => Response::Unit,
        Request::PinRead(node) => response_unit(port.pin(node, false, false)),
        Request::MkdirReserved(parent, name, mode, node) => {
            response_attr(port.mkdir_reserved(parent, &name, mode, node))
        }
    }
}

fn response_attr(result: PortResult<crate::Attr>) -> Response {
    result.map(Response::Attr).unwrap_or_else(Response::Error)
}

fn response_bytes(result: PortResult<Vec<u8>>) -> Response {
    result.map(Response::Bytes).unwrap_or_else(Response::Error)
}

fn response_unit(result: PortResult<()>) -> Response {
    result
        .map(|()| Response::Unit)
        .unwrap_or_else(Response::Error)
}

fn same_capability(left: [u8; 32], right: [u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |different, (left, right)| different | (left ^ right))
        == 0
}

fn capability() -> std::io::Result<[u8; 32]> {
    let mut capability = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut capability)?;
    Ok(capability)
}

#[cfg(test)]
mod tests {
    #[test]
    fn capabilities_use_os_entropy() {
        assert_ne!(super::capability().unwrap(), super::capability().unwrap());
    }
}
