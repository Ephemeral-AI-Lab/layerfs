//! Co-hosted native bridge/service allocation observation, not RSS or a benchmark.
use layerfs_bridge::{
    adapters::native::{
        client::{Client, Source},
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        server::serve,
    },
    contract::*,
};
use layerfs_server::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{
    output::{Identity, OutputConfig},
    runtime::{Configuration, MonitorConfig, Runtime},
    timer::Timing,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, AtomicIsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
static TRACK: AtomicBool = AtomicBool::new(false);
struct Count;
fn change(n: isize) {
    let live = LIVE.fetch_add(n, Ordering::SeqCst) + n;
    if TRACK.load(Ordering::SeqCst) {
        PEAK.fetch_max(live, Ordering::SeqCst);
    }
}
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            change(layout.size() as isize);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        change(-(layout.size() as isize));
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, n: usize) -> *mut u8 {
        let next = unsafe { System.realloc(p, layout, n) };
        if !next.is_null() {
            change(n as isize - layout.size() as isize);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;
struct Generated {
    position: u64,
    length: u64,
    descriptor_at: usize,
}
impl Source for Generated {
    fn read(
        &mut self,
        buffer: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.descriptor_at < 24 {
            let mut descriptor = [0u8; 24];
            descriptor[..8].copy_from_slice(&1u64.to_be_bytes());
            descriptor[16..].copy_from_slice(&self.length.to_be_bytes());
            let n = buffer.len().min(24 - self.descriptor_at);
            buffer[..n].copy_from_slice(&descriptor[self.descriptor_at..self.descriptor_at + n]);
            self.descriptor_at += n;
            return Ok(n);
        }
        let n = buffer.len().min((self.length - self.position) as usize);
        for (offset, byte) in buffer[..n].iter_mut().enumerate() {
            *byte = ((self.position + offset as u64) % 251) as u8;
        }
        self.position += n as u64;
        Ok(n)
    }
}
struct Checked(u64);
impl Write for Checked {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        for (offset, byte) in buffer.iter().enumerate() {
            assert_eq!(*byte, ((self.0 + offset as u64) % 251) as u8);
        }
        self.0 += buffer.len() as u64;
        Ok(buffer.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Held<'a> {
    source: &'a mut dyn Read,
    consumed: usize,
    announced: bool,
    ready: &'a mpsc::SyncSender<()>,
    release: &'a std::sync::Mutex<mpsc::Receiver<()>>,
}
impl Read for Held<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.consumed >= 512 * 1024 && !self.announced {
            self.announced = true;
            self.ready.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
        }
        let n = self.source.read(buffer)?;
        self.consumed += n;
        Ok(n)
    }
}
fn request(id: u64, operation: Operation) -> Request {
    Request {
        id,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 30000,
        response_bytes: if matches!(operation, Operation::SaveFile { .. }) {
            0
        } else {
            MAX_FILE
        },
        operation,
    }
}
#[test]
fn two_native_writers_keep_streaming_heap_bounded_as_input_grows() {
    let path = std::env::temp_dir().join(format!("layerfs-owned-load-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let client_key = [19; 32];
    let server_key = [23; 32];
    let peer = VerifiedPeer::from_private(&client_key).unwrap();
    let server_public = *VerifiedPeer::from_private(&server_key)
        .unwrap()
        .public_key();
    for length in [1024 * 1024u64, 32 * 1024 * 1024] {
        let baseline = LIVE.load(Ordering::SeqCst);
        PEAK.store(baseline, Ordering::SeqCst);
        TRACK.store(true, Ordering::SeqCst);
        let runtime = Runtime::start(Configuration {
            enabled: true,
            timing: true,
            monitor: MonitorConfig {
                cpu: true,
                memory: true,
                interval_ms: 100,
                history: 600,
                windows: 32,
            },
            output: OutputConfig::forward(),
            identity: Identity {
                run: 192,
                pid: std::process::id(),
                role: 1,
                namespace: length,
            },
        })
        .unwrap();
        let store = Timing::disabled("create", |s| {
            Store::create(
                path.join(format!("{length}.sqlite")),
                Store::default_policy(),
                s.child("create"),
            )
        })
        .0
        .unwrap();
        let service = Arc::new(
            Service::new(
                vec![StoreAccess {
                    id: 1,
                    store,
                    grants: vec![Grant {
                        public_key: *peer.public_key(),
                        operations: 31,
                        expires_unix: u64::MAX,
                    }],
                    history: None,
                }],
                runtime.recorder(),
            )
            .unwrap(),
        );
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let (ready_tx, ready_rx) = mpsc::sync_channel(2);
        let (release_tx, release_rx) = mpsc::sync_channel(2);
        let releases = Arc::new(std::sync::Mutex::new(release_rx));
        std::thread::scope(|threads| {
            let service = &service;
            let runtime = &runtime;
            let server = threads.spawn(move || {
                let mut workers = Vec::new();
                for _ in 0..4 {
                    let (socket, _) = listener.accept().unwrap();
                    let ready = ready_tx.clone();
                    let release = Arc::clone(&releases);
                    workers.push(threads.spawn(move || {
                        let peers = [Peer {
                            selector: 1,
                            public: *VerifiedPeer::from_private(&client_key)
                                .unwrap()
                                .public_key(),
                            expires_unix: u64::MAX,
                        }];
                        let connection = accept(socket, &server_key, &peers).unwrap();
                        let _ = serve(connection, |peer, r, input, output, deadline| {
                            let mut input = Held {
                                source: input,
                                consumed: 0,
                                announced: false,
                                ready: &ready,
                                release: &release,
                            };
                            let (result, diagnostic) =
                                service.handle_until(peer, r, &mut input, output, deadline);
                            runtime.publish(diagnostic);
                            result
                        });
                    }));
                }
                for worker in workers {
                    worker.join().unwrap();
                }
            });
            let mut clients: Vec<_> = (0..4)
                .map(|_| {
                    Client::new(connect(address, 1, &client_key, &server_public).unwrap()).unwrap()
                })
                .collect();
            let mut jobs = Vec::new();
            for mut client in clients.drain(..2) {
                jobs.push(threads.spawn(move || {
                    let saved = client
                        .call(
                            &request(
                                1,
                                Operation::SaveFile {
                                    base: None,
                                    base_length: 0,
                                    length,
                                    extents: 1,
                                    replacement: length,
                                },
                            ),
                            &mut Generated {
                                position: 0,
                                length,
                                descriptor_at: 0,
                            },
                            &mut io::sink(),
                        )
                        .unwrap();
                    (client, saved)
                }));
            }
            ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            let held = LIVE.load(Ordering::SeqCst) - baseline;
            assert_eq!(
                clients[0]
                    .call(
                        &request(
                            1,
                            Operation::SaveFile {
                                base: None,
                                base_length: 0,
                                length: 0,
                                extents: 0,
                                replacement: 0
                            }
                        ),
                        &mut &[][..],
                        &mut io::sink()
                    )
                    .unwrap_err()
                    .code,
                Code::Capacity
            );
            release_tx.send(()).unwrap();
            release_tx.send(()).unwrap();
            let mut roots = Vec::new();
            for job in jobs {
                let (mut client, saved) = job.join().unwrap();
                let Response::Saved {
                    root,
                    length: actual,
                    ..
                } = saved
                else {
                    panic!("saved")
                };
                assert_eq!(actual, length);
                roots.push(root);
                let mut output = Checked(0);
                client
                    .call(
                        &request(
                            2,
                            Operation::ReadFile {
                                root,
                                start: 0,
                                end: length,
                            },
                        ),
                        &mut &[][..],
                        &mut output,
                    )
                    .unwrap();
                assert_eq!(output.0, length);
            }
            assert_eq!(roots[0], roots[1]);
            drop(clients);
            server.join().unwrap();
            let peak = PEAK.load(Ordering::SeqCst) - baseline;
            // Instrumentation cap for this construction/read selection, not a
            // universal product RSS bound or coverage of filesystem ordering.
            assert!(
                peak <= 256 * 1024 * 1024,
                "observed Rust allocation demand {peak}"
            );
            println!("owned-load bytes_per_writer={length} held_rust_bytes={held} peak_rust_bytes={peak} server_sessions=4 writers=2 cohosted_client_endpoints=4 native_sqlite_and_OS_memory=separate");
        });
        drop(service);
        drop(runtime);
        TRACK.store(false, Ordering::SeqCst);
    }
    std::fs::remove_dir_all(path).unwrap();
}
