//! A home whose machine `box` is a worker of this build, serving a root
//! that holds sqlite Store `shop`: the home and the root are two folders
//! of one scratch directory, and calls on `shop` take the worker route.
#![allow(dead_code)]
use layerfs_home::disk::{self, paths, read_record, write_record};
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_home::reply::events::Events;
use layerfs_machine::route::{self, Endpoint, MAX_HELLO};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::data::project::init::run::ProjectInit;
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// The worker's secret in these tests.
pub const SECRET: &str = "k";

/// A new empty directory of this test process, under the build's own
/// temporary folder: a Store made in it needs a filesystem the kernel
/// guard admits, which a container's `/tmp` (overlayfs) is not.
pub fn scratch(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "layerfs-wk-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

/// The home, the worker's root and how the worker is reached.
pub struct Placed {
    pub scratch: PathBuf,
    pub home: PathBuf,
    pub root: PathBuf,
    pub endpoint: Endpoint,
}

impl Placed {
    /// Removes everything the fixture made.
    pub fn remove(&self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// A home with machine `box` and Store `shop` on it, its file under the
/// worker's root. `serve` starts the worker on the listener and root it is
/// given. This thread works for the home afterwards (its id is sent).
pub fn placed(tag: &str, serve: impl FnOnce(TcpListener, PathBuf)) -> Placed {
    placed_seeded(tag, |_| {}, serve)
}

/// [`placed`] with `seed`, which runs on the home after its Store `shop`
/// exists and before its file moves under the worker's root: the place a
/// test records the rows it wants the worker to find.
pub fn placed_seeded(
    tag: &str,
    seed: impl FnOnce(&Path),
    serve: impl FnOnce(TcpListener, PathBuf),
) -> Placed {
    let scratch = scratch(tag);
    let (home, root) = (scratch.join("home"), scratch.join("data"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    layerfs_home::disk::open(&home).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-00000000-box")),
        ]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address.to_string())), ("secret", t(SECRET))]),
    )
    .unwrap();
    // made on native, then its file placed under the worker's root
    let shop = StoreName::parse("shop").unwrap();
    layerfs_store::backend::sqlite::create::create(&home, &shop, None).unwrap();
    seed(&home);
    let (from, to) = (
        paths::store_dir(&home, "shop"),
        paths::store_dir(&root, "shop"),
    );
    disk::private_dir(&to).unwrap();
    for entry in std::fs::read_dir(&from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("store.sqlite") {
            std::fs::rename(entry.path(), to.join(&name)).unwrap();
        }
    }
    let record_path = paths::store_record(&home, "shop");
    let record = read_record(&record_path).unwrap().unwrap();
    write_record(&record_path, &disk::with_field(record, "machine", t("box"))).unwrap();
    serve(listener, root.clone());
    let endpoint = Endpoint {
        address,
        secret: SECRET.to_owned(),
    };
    Placed {
        scratch,
        home,
        root,
        endpoint,
    }
}

/// `bytes` bytes that are not one repeated byte.
pub fn pattern(bytes: usize) -> Vec<u8> {
    (0..bytes as u32)
        .map(|i| (i.wrapping_mul(31) % 251) as u8)
        .collect()
}

/// A folder with a small file and one of `big` patterned bytes.
pub fn folder(scratch: &Path, big: usize) -> PathBuf {
    let folder = scratch.join("folder");
    std::fs::create_dir_all(folder.join("src")).unwrap();
    std::fs::write(folder.join("README.md"), "on the caller\n").unwrap();
    std::fs::write(folder.join("src/big.bin"), pattern(big)).unwrap();
    folder
}

/// `project.init` of `folder` as project `shop/<name>`, through the worker;
/// answers the outcome and every event.
pub fn init(
    home: &Path,
    name: &str,
    folder: &Path,
) -> (Result<SdkValue, SdkRefusal>, Vec<SdkValue>) {
    let request = ProjectInit {
        project: ProjectRef::parse(&format!("shop/{name}")).unwrap(),
        folder: folder.display().to_string(),
        timeout: None,
    };
    let mut seen = Vec::new();
    let mut sink = |event| seen.push(event);
    let outcome =
        layerfs_store::data::project::init::run::init(home, &mut Events::new(&mut sink), &request);
    (outcome, seen)
}

/// Store `shop` with its statistics (`store get shop --with stats`): one
/// `store.call` through the worker.
pub fn stats(home: &Path) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_| {};
    let with = layerfs_store::admin::status::get::With {
        live: false,
        stats: true,
        format: false,
    };
    layerfs_store::admin::status::get::get(
        home,
        &mut Events::new(&mut sink),
        "shop",
        with,
        layerfs_sandbox::users::reads(),
    )
}

/// The record a caller sends of Store `shop`.
pub fn store_record(home: &Path) -> SdkValue {
    read_record(&paths::store_record(home, "shop"))
        .unwrap()
        .unwrap()
}

/// One Sandbox operation for a Sandbox that does not exist: the worker
/// settles its records, then refuses.
pub fn sandbox_call(endpoint: &Endpoint) -> Result<SdkValue, SdkRefusal> {
    route::call(endpoint, "sandbox.host", sandbox_args(), &mut |_| {})
}

/// The arguments of [`sandbox_call`]: a Workspace status, which settles
/// the records as every operation does but `sandbox.get`'s two reads
/// (layerfs-project#181), with the Sandbox's name and the Workspace's path
/// where a caller sends them (`more`), so the worker's Sandbox table is
/// asked for the Sandbox and answers `NoSuchSandbox`.
pub fn sandbox_args() -> SdkValue {
    SdkValue::map([
        ("op", t("backend.daemon.workspace.status")),
        ("store", t("shop")),
        ("project", t("api")),
        (
            "store_record",
            SdkValue::map([("name", t("shop")), ("kind", t("sqlite"))]),
        ),
        ("project_record", SdkValue::map([("name", t("api"))])),
        (
            "more",
            SdkValue::map([("sandbox", t("one")), ("path", t("/w"))]),
        ),
    ])
}

/// A connection that sent its secret and request `op` once, as a caller
/// does, and reads nothing by itself.
pub fn sent(endpoint: &Endpoint, op: &str, args: SdkValue) -> TcpStream {
    let mut stream = TcpStream::connect(endpoint.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let secret = SdkValue::map([("secret", t(&endpoint.secret))]);
    route::write_frame(&mut stream, &secret).unwrap();
    let ready = route::read_frame_within(&mut stream, MAX_HELLO)
        .unwrap()
        .unwrap();
    assert!(ready.get("ready").is_some(), "{ready:?}");
    std::io::Write::write_all(&mut stream, &route::request(op, args).unwrap()).unwrap();
    stream
}

/// Waits until `done` holds, at most `within`; answers how long it took.
pub fn until(within: Duration, what: &str, done: impl Fn() -> bool) -> Duration {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < within, "{what}: not within {within:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
    started.elapsed()
}
