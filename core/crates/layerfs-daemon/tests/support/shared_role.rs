//! Real daemon owner, authenticated control and direct in-process Save work.
use super::{fixture, support};
use layerfs_bridge::{
    control::{Reply, Request, WorkspaceToken},
    native,
    provision::{StoreManifest, StoreProfile},
};
use layerfs_daemon::{bootstrap::open_store_observed, control::Service, Owner, OwnerConfig};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::ReservationBlocks;
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    io::{BufRead, Read, Write},
    net::TcpListener,
    path::PathBuf,
    time::Duration,
};
const PREFIX_BYTES: u64 = 16 * 1024 * 1024;
struct Entropy {
    left: u64,
    state: u64,
}
impl Read for Entropy {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out.len().min(self.left as usize);
        for byte in &mut out[..n] {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            *byte = self.state as u8;
        }
        self.left -= n as u64;
        Ok(n)
    }
}
fn event(value: &str) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}
fn release() {
    let mut line = String::new();
    assert!(std::io::stdin().lock().read_line(&mut line).unwrap() > 0);
    assert_eq!(line, "continue\n");
}
pub fn run() {
    // Whole-child wall fence independent of sockets/stdin and parent success.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(20));
        std::process::exit(124);
    });
    let role = std::env::var("LAYERFS_SHARING_ROLE").unwrap();
    let manifest = StoreManifest::decode(
        &fs::read(std::env::var_os("LAYERFS_SHARING_MANIFEST").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest.profile, StoreProfile::Disposable);
    let opened = open_store_observed(
        PersistenceConfig::sqlite(&manifest.locator)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &manifest.binding,
        manifest.cursor_key,
        4,
        8 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let overlay = PathBuf::from(std::env::var_os("LAYERFS_SHARING_OVERLAY").unwrap());
    let owner = Owner::start(
        &overlay,
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let service = Service::new(opened.store.clone(), &owner);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    event(&format!(
        "SHARING_READY role={role} pid={} overlay={} address={}",
        std::process::id(),
        overlay.display(),
        listener.local_addr().unwrap()
    ));
    let (socket, _) = listener.accept().unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut connection = native::accept(
        socket,
        &support::SERVER_PRIVATE,
        native::public_key(&support::CLIENT_PRIVATE).unwrap(),
    )
    .unwrap();
    let mut attempts = 0;
    let mut mounts = 0;
    let mut current = 0;
    loop {
        let served=service.serve_one(&mut connection,|save,_,snapshot|{
            attempts+=1;event(&format!("SHARING_SAVE_ENTER role={role} attempt={attempts}"));
            if role=="A" && attempts==3 {
                let before=opened.diagnostics().unwrap().writer.write_commits;let policy=opened.store.policy().construction();
                let built=Timing::disabled("crash.prefix",|scope|layerfs_content::construct_stream(policy,&policy.capacities(),Entropy{left:PREFIX_BYTES,state:0x987654321abcdef},&mut save.sink(),scope.child("content"))).0?;
                let after=opened.diagnostics().unwrap().writer.write_commits;assert!(after>before,"a real immutable publication must precede kill");event(&format!("SHARING_PREFIX_ACK role=A bytes={PREFIX_BYTES} writes_before={before} writes_after={after} root={}",built.root));release();panic!("crash actor must be killed before constructor returns");
            }
            release();fixture::construct(save,snapshot,current,opened.store.policy().construction())
        }).unwrap();
        let mut done = false;
        if let Ok(success) = &served.outcome {
            match &success.reply {
                Reply::Bound { token, .. } => {
                    mounts += 1;
                    let tag = match (role.as_str(), mounts) {
                        ("A", 1) => Some(b'A'),
                        ("B", 1) => Some(b'B'),
                        ("B", 2) => Some(b'D'),
                        _ => None,
                    };
                    if let Some(tag) = tag {
                        current = tag;
                        fixture::write(&service, *token, tag);
                    } else {
                        oracle(
                            &service,
                            *token,
                            if role == "C" && mounts == 2 {
                                b'B'
                            } else {
                                b'D'
                            },
                        );
                    }
                }
                Reply::Committed(_) => {
                    let token = match served.call.request {
                        Request::Commit(token) => token,
                        _ => unreachable!(),
                    };
                    oracle(&service, token, current);
                    assert!(opened
                        .store
                        .history()
                        .stage(token.workspace)
                        .unwrap()
                        .is_none());
                    let work = &success.commit.as_ref().unwrap().storage;
                    println!("SHARING_COMMIT role={role} attempt={attempts} writes={} initial={} refills={} publication={} history=1",work.reserve+work.publish+1,work.initial_reservations,work.reservation_refills,work.publish);
                    if role == "A" {
                        current = if attempts == 1 { b'C' } else { b'E' };
                        fixture::write(&service, token, current);
                    }
                }
                Reply::Unmounted(token) => done = token.workspace.to_bytes()[0] == 99,
                _ => (),
            }
        }
        assert!(
            served.outcome.is_ok(),
            "original daemon failure: {:?}",
            served.outcome
        );
        drop(served);
        if done {
            break;
        }
    }
    drop(service);
    owner.stop().unwrap();
    drop(opened);
}
fn oracle(service: &Service, token: WorkspaceToken, tag: u8) {
    let operation = service.operation(token).unwrap();
    let base = operation.workspace().base().unwrap();
    use layerfs_content::filesystem::PathName;
    for name in ["file", "alias"] {
        let file = base
            .child(
                base.root().root_inode().serial(),
                &PathName::new(name).unwrap(),
            )
            .unwrap();
        for offset in (0..support::FILE_BYTES).step_by(16 * 1024) {
            let end = (offset + 16 * 1024).min(support::FILE_BYTES);
            let mut actual = Vec::new();
            base.plan_read(file.serial, offset as u64, (end - offset) as u32)
                .unwrap()
                .emit(&mut actual)
                .unwrap();
            let expected = (offset..end)
                .map(|n| if n == 0 { tag } else { (n % 251) as u8 })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    }
    let git = base
        .child(
            base.root().root_inode().serial(),
            &PathName::new(".git").unwrap(),
        )
        .unwrap();
    let index = base
        .child(git.serial, &PathName::new("index").unwrap())
        .unwrap();
    let mut index_bytes = Vec::new();
    base.plan_read(index.serial, 0, 14)
        .unwrap()
        .emit(&mut index_bytes)
        .unwrap();
    assert_eq!(index_bytes, b"complete index");
    let mut tree = layerfs_content::filesystem::FilesystemRead::new(
        operation.client(),
        layerfs_content::filesystem::FilesystemRootId(base.identity().0),
    )
    .unwrap();
    assert_eq!(
        tree.readlink(&layerfs_content::filesystem::LogicalPath::new(".git/link").unwrap())
            .unwrap()
            .as_bytes(),
        b"../opaque-\xff"
    );
    assert_eq!(
        tree.list(
            &layerfs_content::filesystem::LogicalPath::root(),
            None,
            32,
            4096
        )
        .unwrap()
        .entries
        .len(),
        3
    );
    assert_eq!(
        tree.list(
            &layerfs_content::filesystem::LogicalPath::new(".git").unwrap(),
            None,
            32,
            4096
        )
        .unwrap()
        .entries
        .len(),
        2
    );
    event(&format!(
        "SHARING_ROOT_ORACLE workspace={:?} tag={tag} full_bytes={} root={}",
        token.workspace,
        2 * support::FILE_BYTES + 14,
        base.identity().0
    ));
}
