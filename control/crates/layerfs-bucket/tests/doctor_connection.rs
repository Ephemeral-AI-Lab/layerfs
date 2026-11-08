//! Conditional version-owned probes and one-page authenticated listing.
#[path = "support/doctor_s3.rs"]
mod peer;
use layerfs_bucket::bucket::RemoteError;
use peer::{Mode, Server, TOKEN};
use std::time::{Duration, Instant};
fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn bounded_prefix_list_and_version_owned_roundtrip_preserve_replacement() {
    let server = Server::new(Mode {
        replace_before_delete: true,
        ..Mode::default()
    });
    let client = server.client();
    client.diagnostic_list(until()).unwrap();
    client.diagnostic_versioning(until()).unwrap();
    let object = client.diagnostic_create(TOKEN, until()).unwrap();
    client.diagnostic_read(&object, until()).unwrap();
    client.diagnostic_delete(&object, until()).unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.requests.len(), 5);
    assert!(!state.versions.contains_key("owned-v1"));
    assert_eq!(
        state.versions.get("replacement-v2").unwrap(),
        b"replacement backup bytes"
    );
}
#[test]
fn collision_and_unsupported_conditional_create_never_fall_back_or_delete() {
    for mode in [
        Mode {
            collision: true,
            ..Mode::default()
        },
        Mode {
            unsupported_put: true,
            ..Mode::default()
        },
    ] {
        let server = Server::new(mode);
        let client = server.client();
        let created = client.diagnostic_create(TOKEN, until());
        if mode.collision {
            assert!(matches!(
                created,
                Err(RemoteError::Refused { status: 412, .. })
            ));
        } else {
            assert!(matches!(created, Err(RemoteError::Unsupported(_))));
        }
        let state = server.state.lock().unwrap();
        assert_eq!(state.requests.len(), 1);
        assert_eq!(state.requests[0].method, "PUT");
        if mode.collision {
            assert_eq!(
                state.versions.get("existing").unwrap(),
                b"existing backup bytes"
            );
        }
    }
}
#[test]
fn lost_put_can_only_reconcile_exact_token_bytes_and_owned_version() {
    let server = Server::new(Mode {
        lost_put: true,
        ..Mode::default()
    });
    let client = server.client();
    assert!(matches!(
        client.diagnostic_create(TOKEN, until()),
        Err(RemoteError::Uncertain(_))
    ));
    let object = client.diagnostic_reconcile(TOKEN, until()).unwrap();
    client.diagnostic_delete(&object, until()).unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(
        state.requests.iter().filter(|r| r.method == "PUT").count(),
        1
    );
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        1
    );
    assert!(state.versions.is_empty());
}
#[test]
fn content_mismatch_and_cleanup_failure_remain_independent() {
    for mode in [
        Mode {
            bad_download: true,
            denied_delete: true,
            ..Mode::default()
        },
        Mode {
            lost_delete: true,
            ..Mode::default()
        },
    ] {
        let server = Server::new(mode);
        let client = server.client();
        let object = client.diagnostic_create(TOKEN, until()).unwrap();
        let read = client.diagnostic_read(&object, until());
        if mode.bad_download {
            assert!(matches!(read, Err(RemoteError::Integrity(_))));
        } else {
            read.unwrap();
        }
        let deleted = client.diagnostic_delete(&object, until());
        if mode.lost_delete {
            assert!(matches!(deleted, Err(RemoteError::Uncertain(_))));
        } else {
            assert!(matches!(
                deleted,
                Err(RemoteError::Refused { status: 403, .. })
            ));
        }
        let state = server.state.lock().unwrap();
        assert_eq!(
            state
                .requests
                .iter()
                .filter(|r| r.method == "DELETE")
                .count(),
            1
        );
        assert_eq!(state.versions.contains_key("owned-v1"), mode.denied_delete);
    }
}
#[test]
fn object_authority_cannot_be_reused_on_another_bucket_or_prefix() {
    let server = Server::new(Mode::default());
    let client = server.client();
    let object = client.diagnostic_create(TOKEN, until()).unwrap();
    let mut config = server.config();
    config.prefix = "another".into();
    let other = layerfs_bucket::bucket::s3_api::S3Client::new(config).unwrap();
    assert!(matches!(
        other.diagnostic_delete(&object, until()),
        Err(RemoteError::Invalid(_))
    ));
    assert_eq!(server.state.lock().unwrap().requests.len(), 1);
    client.diagnostic_delete(&object, until()).unwrap();
}

#[test]
fn r2_auto_region_protocol_can_list_but_unversioned_write_capability_is_unavailable() {
    let server = Server::new(Mode {
        unversioned: true,
        ..Mode::default()
    });
    let mut config = server.config();
    config.region = "auto".into();
    let client = layerfs_bucket::bucket::s3_api::S3Client::new(config).unwrap();
    client.diagnostic_list(until()).unwrap();
    assert!(matches!(
        client.diagnostic_versioning(until()),
        Err(RemoteError::Unsupported(_))
    ));
    let state = server.state.lock().unwrap();
    assert_eq!(state.requests.len(), 2);
    assert!(state.requests.iter().all(|request| request.method == "GET"
        && request
            .headers
            .get("authorization")
            .unwrap()
            .contains("/auto/s3/aws4_request")));
}

#[test]
fn mutation_server_error_does_not_prove_no_effect_and_is_never_resent() {
    let server = Server::new(Mode {
        error_after_put: true,
        error_after_delete: true,
        ..Mode::default()
    });
    let client = server.client();
    assert!(matches!(
        client.diagnostic_create(TOKEN, until()),
        Err(RemoteError::Uncertain(_))
    ));
    let object = client.diagnostic_reconcile(TOKEN, until()).unwrap();
    assert!(matches!(
        client.diagnostic_delete(&object, until()),
        Err(RemoteError::Uncertain(_))
    ));
    let state = server.state.lock().unwrap();
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|request| request.method == "PUT")
            .count(),
        1
    );
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|request| request.method == "DELETE")
            .count(),
        1
    );
    assert!(state.versions.is_empty());
}
