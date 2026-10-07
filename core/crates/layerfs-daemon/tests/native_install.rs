//! Native install uses the product SDK/channel/daemon path and real Store ports.
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::{
    control::{InstallPhase, InstallReply, RefusalKind},
    native::MAX_PLAINTEXT_BYTES,
};
use layerfs_daemon::{install::receive_install, install_types::StoreSettings};
use std::{fs, path::PathBuf};

#[test]
fn authenticated_install_opens_one_store_and_reads_the_complete_root() {
    let fixture = support::Fixture::new("complete", None);
    let target = PathBuf::from(&fixture.project.manifest.locator);
    let (mut client, worker) = support::pair(move |connection| {
        receive_install(connection, &target, StoreSettings::default())
    });
    let ack = layerfs_sdk::install(&fixture.project, &mut client).unwrap();
    let installed = worker.join().unwrap();
    assert_eq!(ack.manifest, installed.manifest);
    assert_eq!(ack.work.sent_bytes, installed.work.written);
    assert_eq!(ack.work.sent_records, installed.work.records);
    assert_eq!(ack.work.sent_bytes, fixture.project.store.bytes);
    assert_eq!(ack.work.buffer_bytes, MAX_PLAINTEXT_BYTES);
    assert!(installed.work.peak_record_bytes <= MAX_PLAINTEXT_BYTES);
    assert_eq!(installed.work.sync_calls, 0);
    assert!(!PathBuf::from(format!("{}.installing", installed.manifest.locator)).exists());
    assert_eq!(
        fs::read(&fixture.project.store.path).unwrap(),
        fs::read(&installed.manifest.locator).unwrap()
    );
    fs::remove_file(&fixture.project.store.path).unwrap();
    support::oracle(&installed, &fixture.directory.join("overlay.sqlite"));
    drop(installed);
    fixture.cleanup();
}
#[test]
fn existing_store_is_refused_before_upload_and_preserved() {
    let fixture = support::Fixture::new("existing", None);
    let target = PathBuf::from(&fixture.project.manifest.locator);
    fs::write(&target, b"original").unwrap();
    let destination = target.clone();
    let (mut client, worker) = support::pair(move |connection| {
        receive_install(connection, &destination, StoreSettings::default())
    });
    let failure = layerfs_sdk::install(&fixture.project, &mut client).unwrap_err();
    assert!(
        matches!(failure.error,layerfs_sdk::InstallError::Remote(ref remote) if remote.kind==RefusalKind::Exists && !remote.published && !remote.claim_acknowledged && remote.written==0)
    );
    assert!(!failure.accepted);
    assert_eq!(failure.work.sent_bytes, 0);
    let server = worker.join().err().unwrap();
    assert_eq!(server.phase, InstallPhase::Claim);
    assert!(!server.claim_acknowledged && !server.published);
    assert_eq!(fs::read(&target).unwrap(), b"original");
    assert!(!server.temporary.as_ref().unwrap().exists());
    drop(server);
    fixture.cleanup();
}
#[test]
fn interrupted_transfer_keeps_original_partial_file_and_claim_refuses_another_installer() {
    let fixture = support::Fixture::new("partial", None);
    let target = PathBuf::from(&fixture.project.manifest.locator);
    let destination = target.clone();
    let (mut first, worker) = support::pair(move |connection| {
        receive_install(connection, &destination, StoreSettings::default())
    });
    first
        .send
        .send(&fixture.project.manifest.encode().unwrap())
        .unwrap();
    assert_eq!(
        InstallReply::decode(first.receive.receive().unwrap()).unwrap(),
        InstallReply::Ready
    );
    first.send.send(&[42; 31]).unwrap();
    let (mut second, other) = support::pair(move |connection| {
        receive_install(connection, &target, StoreSettings::default())
    });
    let refusal = layerfs_sdk::install(&fixture.project, &mut second).unwrap_err();
    assert!(
        matches!(refusal.error,layerfs_sdk::InstallError::Remote(ref value) if value.kind==RefusalKind::Exists)
    );
    let refused = other.join().err().unwrap();
    assert!(!refused.claim_acknowledged && !refused.published);
    first.send.close().unwrap();
    let partial = worker.join().err().unwrap();
    assert_eq!(partial.phase, InstallPhase::Transfer);
    assert!(partial.claim_acknowledged && !partial.published);
    assert_eq!(partial.work.written, 31);
    assert!(!partial.destination.exists());
    assert_eq!(
        fs::read(partial.temporary.as_ref().unwrap()).unwrap(),
        [42; 31]
    );
    println!("INSTALL_PARTIAL original_bytes=31 published=false claim_acknowledged=true no_product_cleanup=true");
    drop((partial, refused));
    fixture.cleanup();
}
