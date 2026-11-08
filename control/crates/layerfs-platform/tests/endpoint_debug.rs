//! A machine endpoint's `Debug`, and a `Running` holding one, never show its secret.
use layerfs_platform::{Endpoint, Lasting, Running};

#[test]
fn debug_redacts_the_secret() {
    let endpoint = Endpoint {
        address: "127.0.0.1:7070".parse().unwrap(),
        secret: "s3cr3t-token".into(),
    };
    let running = Running {
        endpoint: endpoint.clone(),
        lasting: Lasting::default(),
    };
    for shown in [format!("{endpoint:?}"), format!("{running:?}")] {
        assert!(!shown.contains("s3cr3t-token"), "{shown}");
        assert!(
            shown.contains("127.0.0.1:7070") && shown.contains("<redacted>"),
            "{shown}"
        );
    }
}
