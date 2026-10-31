//! Writes `project.record` for a restored copy of a stopped Store, so the SDK
//! can open it from a disposable LayerFS home (issue #347's converter reads the
//! recorded runs' Stores this way). A run's own record is private discovery
//! material and is not always retained; the daemon opens an installed Store
//! with only its catalog binding (read from `history_meta`) and a cursor key,
//! which signs this session's page cursors and is new here.
//!
//!   restore_record <home> <project> <binding hex> <stack hex> <branch hex>
//!                  <root hex> <sealed bytes> <host sqlite> <daemon sqlite>
//!
//! The record says the Store is installed, so no Sandbox installs `sealed.sqlite`
//! over it; the volume must already hold the restored `global/store.sqlite`.
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use layerfs_bridge::provision::{ProviderKind, StoreManifest, StoreProfile};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

fn hex<const N: usize>(text: &str) -> [u8; N] {
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect::<Vec<u8>>();
    bytes.try_into().expect("length")
}

#[cfg(unix)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, project, binding, stack, branch, root, bytes, host, daemon] = &args[..] else {
        eprintln!("usage: restore_record <home> <project> <binding> <stack> <branch> <root> <bytes> <host sqlite> <daemon sqlite>");
        std::process::exit(2);
    };
    let binding = (0..binding.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&binding[i..i + 2], 16).expect("binding hex"))
        .collect();
    let cursor_key = layerfs_bridge::duplex::generate_keypair()
        .expect("random key")
        .public;
    let manifest = StoreManifest {
        provider: ProviderKind::Sqlite,
        locator: "/layerfs-store/global/store.sqlite".into(),
        profile: StoreProfile::Disposable,
        binding,
        cursor_key,
        stack: hex(stack),
        branch: hex(branch),
        root: hex(root),
        bytes: bytes.parse().expect("sealed bytes"),
        host_sqlite: host.clone(),
        daemon_sqlite: None,
    };
    let mut installed = manifest.clone();
    installed.daemon_sqlite = Some(daemon.clone());
    let record = Answer {
        id: 1,
        reply: Reply::Sdk(SdkValue::map([
            (
                "manifest",
                SdkValue::Bytes(manifest.encode().expect("manifest")),
            ),
            (
                "installed",
                SdkValue::Bytes(installed.encode().expect("installed")),
            ),
        ])),
    }
    .encode()
    .expect("record");
    let path = std::path::Path::new(home)
        .join(project)
        .join("project.record");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .expect("new project.record");
    file.write_all(&record).expect("write project.record");
    println!("{}", path.display());
}

// The record format this writes (0o600 mode, unix paths) belongs to the
// Linux restore runbook; Windows discovery saves its own records.
#[cfg(not(unix))]
fn main() {
    eprintln!("restore_record is the Linux restore runbook; not run here");
}
