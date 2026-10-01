use phase6_live_probe::{
    metadata_catalog::LocatorDb,
    minio::Minio,
    objects::Locators,
    proof_plan::{Head, Plan},
};
use std::sync::{Arc, Mutex};
fn path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "p6-proof-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ))
}
#[test]
fn known_reply_plan_roundtrip_exact_eof_and_counts() {
    let p = path();
    let plan = Plan {
        scenario: Some([1; 32]),
        branch: [2; 17],
        head: Some([3; 33]),
        roots: vec![[4; 32]],
        commits: vec![Head {
            id: [3; 33],
            root: [4; 32],
            parent: None,
        }],
    };
    plan.write(&p).unwrap();
    let got = Plan::read(&p).unwrap();
    assert_eq!(got.scenario, plan.scenario);
    assert_eq!(got.branch, plan.branch);
    assert_eq!(got.commits[0].root, [4; 32]);
    let mut bytes = std::fs::read(&p).unwrap();
    bytes.push(0);
    std::fs::write(&p, bytes).unwrap();
    assert!(Plan::read(&p).is_err());
    std::fs::remove_file(p).unwrap();
}
#[test]
fn reopened_locator_verifier_refuses_mutations_before_provider_io() {
    let p = path();
    let s3 = Minio {
        authority: "127.0.0.1:9".into(),
        bucket: "fixture".into(),
        access: "fixture".into(),
        secret: "fixture".into(),
        stats: Arc::new(Mutex::new(Default::default())),
    };
    drop(LocatorDb::create(&p, s3.clone()).unwrap());
    let db = LocatorDb::open_read_only(&p, s3.clone()).unwrap();
    assert!(db.register_many(&[]).unwrap_err().contains("readonly"));
    assert!(db
        .certify_file(layerfs_content::ObjectId::for_bytes(b"fixture"))
        .unwrap_err()
        .contains("readonly"));
    assert_eq!(s3.statistics().unwrap().get_calls, 0);
    drop(db);
    std::fs::remove_file(p).unwrap();
}
