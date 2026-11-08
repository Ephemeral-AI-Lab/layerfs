//! Actual Control Options cancellation across the second admitted Store open.
use super::{command, sql, BINDING};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    diagnostic::{Options, Outcome},
    PostgresHandles, PostgresSelection,
};
use layerfs_store::{
    identity::{Identity, Kind},
    record::StoreRecord,
};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    selected: PostgresSelection,
    container: String,
    app: String,
    writer: Option<PostgresHandles>,
    database: String,
}
impl Fixture {
    fn new() -> Option<Self> {
        let container = std::env::var("LAYERFS_TEST_DOCTOR_PG_DISPOSABLE_CONTAINER").ok()?;
        assert!(container.starts_with("doctor-pg116-"));
        let schema = format!(
            "doctor_control_cancel_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let app = schema.clone();
        let label = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{index .Config.Labels \"owner\"}}",
                &container,
            ])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(label.stdout).unwrap().trim(),
            "layerfs-doctor-pg-116"
        );
        sql(&container, &format!("CREATE DATABASE {schema}"));
        let url = format!(
            "{}&application_name={app}",
            std::env::var("LAYERFS_TEST_POSTGRES_URL").unwrap()
        );
        let url = url.replace("/postgres?", &format!("/{schema}?"));
        let selected = PostgresSelection::new(url, "layerfs");
        let writer = PostgresHandles::create(
            &selected,
            layerfs_storage::StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: BINDING.to_vec(),
                cursor_key: [71; 32],
                incarnation: 7,
            },
        )
        .unwrap();
        Some(Self {
            selected,
            container,
            app,
            writer: Some(writer),
            database: schema,
        })
    }
    fn record(&self, url: &str) -> StoreRecord {
        let password = std::env::var("LAYERFS_TEST_POSTGRES_PASSWORD").unwrap();
        let url = url.replace(&format!(":{password}@"), "@");
        StoreRecord::connected(
            "saved",
            &url,
            &self.selected.schema,
            "off",
            Some("env:LAYERFS_TEST_POSTGRES_PASSWORD"),
        )
    }
    fn identity(&self) -> Identity {
        Identity {
            kind: Kind::Postgresql {
                schema: self.selected.schema.clone(),
                synchronous_commit: "off".into(),
            },
            binding: BINDING.to_vec(),
            cursor_key: [71; 32],
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.writer.take();
        sql(
            &self.container,
            &format!("DROP DATABASE {} WITH (FORCE)", self.database),
        );
    }
}
fn accept(listener: &TcpListener) -> TcpStream {
    let until = Instant::now() + Duration::from_secs(4);
    loop {
        match listener.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false).unwrap();
                s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                s.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
                return s;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until => {
                std::thread::yield_now()
            }
            Err(_) => panic!("owned diagnostic proxy accept failed"),
        }
    }
}
fn copy(mut source: TcpStream, mut target: TcpStream) {
    source
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(4);
    let mut b = [0; 16384];
    while Instant::now() < until {
        match source.read(&mut b) {
            Ok(0) => break,
            Ok(n) => {
                if target.write_all(&b[..n]).is_err() {
                    break;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => break,
        }
    }
    let _ = source.shutdown(Shutdown::Both);
    let _ = target.shutdown(Shutdown::Both);
}
#[test]
fn full_control_integrity_cancels_second_authentication_after_successful_preflight() {
    let Some(f) = Fixture::new() else { return };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = listener.local_addr().unwrap();
    let authority = f
        .selected
        .url
        .split('@')
        .next_back()
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    let real: std::net::SocketAddr = authority.parse().unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    let signal = flag.clone();
    let peer = std::thread::spawn(move || {
        let client = accept(&listener);
        let server = TcpStream::connect_timeout(&real, Duration::from_secs(2)).unwrap();
        let source = client.try_clone().unwrap();
        let target = server.try_clone().unwrap();
        let forward = std::thread::spawn(move || copy(source, target));
        copy(server, client);
        forward.join().unwrap();
        let mut second = accept(&listener);
        let mut len = [0; 4];
        second.read_exact(&mut len).unwrap();
        let len = u32::from_be_bytes(len) as usize;
        assert!((8..4096).contains(&len));
        let mut startup = vec![0; len - 4];
        second.read_exact(&mut startup).unwrap();
        signal.store(true, Ordering::Relaxed);
        let mut buf = [0; 4096];
        loop {
            match second.read(&mut buf) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    });
    let source_url = f
        .selected
        .url
        .replace(&real.to_string(), &proxy.to_string());
    let record = f.record(&source_url);
    let identity = f.identity();
    let mut out = Vec::new();
    let start = Instant::now();
    let result = layerfs_store::diagnostic::postgresql(
        &record,
        &identity,
        Options {
            cancelled: Some(&flag),
            ..Options::default()
        },
        false,
        &mut |o| out.push(o),
    );
    assert!(result.is_err());
    assert!(
        peer.join().unwrap(),
        "second authentication socket closed after actual startup and cancellation"
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(out
        .iter()
        .any(|o| o.id == "store.connection.identity" && o.outcome == Outcome::Pass));
    assert!(out
        .iter()
        .any(|o| o.id == "store.admission.cancelled" && o.outcome == Outcome::Unknown));
    assert!(!out.iter().any(|o| o.id == "store.snapshot"));
}
struct HeldLock {
    container: String,
    pid: i32,
    child: Child,
}
impl Drop for HeldLock {
    fn drop(&mut self) {
        sql(
            &self.container,
            &format!("SELECT pg_terminate_backend({})", self.pid),
        );
        let _ = self.child.wait();
    }
}
#[test]
fn full_control_integrity_cancels_second_open_schema_query_and_releases_backend() {
    let Some(f) = Fixture::new() else { return };
    let flag = Arc::new(AtomicBool::new(false));
    let signal = flag.clone();
    let mut lock = None;
    let mut monitor = None;
    let mut out = Vec::new();
    let record = f.record(&f.selected.url);
    let identity = f.identity();
    let start = Instant::now();
    let result = layerfs_store::diagnostic::postgresql(
        &record,
        &identity,
        Options {
            cancelled: Some(&flag),
            ..Options::default()
        },
        false,
        &mut |o| {
            if o.id == "store.connection.tls" {
                let db = f.selected.url.clone();
                let db = db
                    .split('/')
                    .next_back()
                    .unwrap()
                    .split('?')
                    .next()
                    .unwrap();
                let lock_app = format!("lock_{}", f.app);
                let child = Command::new("docker").args(["exec","-e",&format!("PGAPPNAME={lock_app}"),"-u","postgres",&f.container,"psql","-XAt","-v","ON_ERROR_STOP=1","-d",db,"-c",&format!("BEGIN;LOCK TABLE {}.history_meta IN ACCESS EXCLUSIVE MODE;SELECT pg_sleep(8);ROLLBACK",f.selected.schema)]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
                let until = Instant::now() + Duration::from_secs(2);
                let pid = loop {
                    let output=command(&f.container,&["psql","-XAt","-c",&format!("SELECT pid FROM pg_stat_activity WHERE application_name='{lock_app}' AND wait_event='PgSleep'")]);
                    if let Ok(pid) = String::from_utf8(output.stdout)
                        .unwrap()
                        .trim()
                        .parse::<i32>()
                    {
                        break pid;
                    }
                    assert!(
                        Instant::now() < until,
                        "owned lock holder not observed by independent server monitor"
                    );
                    std::thread::yield_now();
                };
                lock = Some(HeldLock {
                    container: f.container.clone(),
                    pid,
                    child,
                });
                let container = f.container.clone();
                let app = f.app.clone();
                let signal = signal.clone();
                monitor = Some(std::thread::spawn(move || {
                    let until = Instant::now() + Duration::from_secs(3);
                    loop {
                        let output=command(&container,&["psql","-XAt","-c",&format!("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name='{app}' AND state='active' AND wait_event_type='Lock')")]);
                        if output.stdout == b"t\n" {
                            signal.store(true, Ordering::Relaxed);
                            return true;
                        }
                        if Instant::now() >= until {
                            signal.store(true, Ordering::Relaxed);
                            return false;
                        }
                        std::thread::yield_now();
                    }
                }));
            }
            out.push(o)
        },
    );
    assert!(result.is_err());
    assert!(
        monitor.unwrap().join().unwrap(),
        "second admitted schema query actually blocked before cancellation; observations {out:?}"
    );
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(out
        .iter()
        .any(|o| o.id == "store.connection.identity" && o.outcome == Outcome::Pass));
    assert!(out
        .iter()
        .any(|o| o.id == "store.admission.cancelled" && o.outcome == Outcome::Unknown));
    assert!(!out.iter().any(|o| o.id == "store.snapshot"));
    let output=command(&f.container,&["psql","-XAt","-c",&format!("SELECT count(*) FROM pg_stat_activity WHERE application_name='{}' AND (backend_xmin IS NOT NULL OR wait_event_type='Lock')",f.app)]);
    assert_eq!(
        output.stdout, b"0\n",
        "cancelled admitted session retained no backend work/snapshot"
    );
    drop(lock);
}
