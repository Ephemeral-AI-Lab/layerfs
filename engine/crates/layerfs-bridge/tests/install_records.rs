//! Checked control metadata and exact remote publication knowledge.
use layerfs_bridge::{
    control::{InstallPhase, InstallRefusal, InstallReply, RefusalKind},
    provision::{ProviderKind, StoreManifest, StoreProfile},
};
fn manifest() -> StoreManifest {
    StoreManifest {
        provider: ProviderKind::Sqlite,
        locator: "/store/store.sqlite".into(),
        profile: StoreProfile::Disposable,
        binding: b"install-records".to_vec(),
        cursor_key: [7; 32],
        stack: [0x31; 17],
        branch: [0x11; 17],
        root: [3; 32],
        bytes: 5_000_000_000,
        host_sqlite: "3.51.0".into(),
        daemon_sqlite: None,
    }
}
#[test]
fn bounded_manifest_is_exact_and_does_not_cap_stream_length() {
    let value = manifest();
    let encoded = value.encode().unwrap();
    assert_eq!(StoreManifest::decode(&encoded).unwrap(), value);
    for end in 0..encoded.len() {
        assert!(StoreManifest::decode(&encoded[..end]).is_err());
    }
    let mut extra = encoded.clone();
    extra.push(0);
    assert!(StoreManifest::decode(&extra).is_err());
    let mut changed = encoded;
    changed[4] = 2;
    assert!(StoreManifest::decode(&changed).is_err());
    let mut oversized = value;
    oversized.locator = format!("/{}", "s".repeat(4096));
    assert!(oversized.encode().is_err());
}
#[test]
fn installed_and_refused_keep_original_publication_knowledge() {
    let mut installed = manifest();
    installed.daemon_sqlite = Some("3.53.2".into());
    for value in [
        InstallReply::Ready,
        InstallReply::Installed(installed),
        InstallReply::Refused(InstallRefusal {
            phase: InstallPhase::Publish,
            kind: RefusalKind::Unknown,
            claim_acknowledged: true,
            written: 5_000_000_000,
            published: false,
            os_code: Some(5),
            detail: "original rename error".into(),
        }),
    ] {
        let encoded = value.encode().unwrap();
        assert_eq!(InstallReply::decode(&encoded).unwrap(), value);
        for end in 0..encoded.len() {
            assert!(InstallReply::decode(&encoded[..end]).is_err());
        }
        let mut extra = encoded;
        extra.push(0);
        assert!(InstallReply::decode(&extra).is_err());
    }
    // Five billion is metadata only; this does not execute a >4GiB file proof.
}
