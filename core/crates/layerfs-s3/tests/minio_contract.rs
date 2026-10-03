//! Mandatory live MinIO proof of the three calls and all four outcomes.
mod support;
use layerfs_s3::S3Objects;
use layerfs_storage::port::{ByteRange, ObjectError, ObjectKey, ObjectStore, Put};
#[test]
fn conditional_create_reads_head_and_exact_ranges_on_the_pinned_service() {
    let store = S3Objects::connect(support::config("contract")).unwrap();
    let body = (0..32000).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let key = ObjectKey::for_bytes(&body);
    assert_eq!(store.put_if_absent(key, &body).unwrap(), Put::Created);
    assert_eq!(
        store.put_if_absent(key, &body).unwrap(),
        Put::AlreadyPresent
    );
    assert_eq!(store.head(key).unwrap(), Some(body.len() as u64));
    let mut out = vec![9];
    store.read(key, None, &mut out).unwrap();
    assert_eq!(out, body);
    store
        .read(
            key,
            Some(ByteRange {
                start: 251,
                length: 500,
            }),
            &mut out,
        )
        .unwrap();
    assert_eq!(out, body[251..751]);
    let before = store.diagnostics().unwrap();
    assert_eq!(before.request_work[0].calls, before.puts);
    assert_eq!(before.request_work[1].calls, before.gets);
    assert_eq!(before.request_work[2].calls, before.heads);
    assert_eq!(
        before
            .request_work
            .iter()
            .map(|row| row.body_sent)
            .sum::<u64>(),
        before.body_sent
    );
    assert_eq!(
        before
            .request_work
            .iter()
            .map(|row| row.body_received)
            .sum::<u64>(),
        before.body_received
    );
    assert_eq!(
        before
            .request_work
            .iter()
            .map(|row| row.continue_wait_ns)
            .sum::<u64>(),
        before.continue_wait_ns
    );
    assert_eq!(
        store.put_if_absent(key, b"different body"),
        Err(ObjectError::Malformed)
    );
    assert_eq!(
        store.read(
            key,
            Some(ByteRange {
                start: 0,
                length: 0
            }),
            &mut out
        ),
        Err(ObjectError::Malformed)
    );
    assert_eq!(store.diagnostics().unwrap(), before);
    assert_eq!(before.requests, 5);
    assert_eq!((before.puts, before.gets, before.heads), (2, 2, 1));
    assert_eq!(before.connections, 2);
    println!("DIAGNOSTIC conditional-create {:?}", before);
}
#[test]
fn missing_is_a_single_request_and_head_reports_absence() {
    let store = S3Objects::connect(support::config("missing")).unwrap();
    let key = ObjectKey::for_bytes(b"not written");
    assert_eq!(
        store.read(key, None, &mut Vec::new()),
        Err(ObjectError::Missing)
    );
    assert_eq!(store.head(key).unwrap(), None);
    assert_eq!(store.diagnostics().unwrap().requests, 2);
}
#[test]
fn bad_credentials_are_a_definite_refusal_without_resend() {
    let mut config = support::config("refused");
    config.secret_key = "deliberately-invalid-signing-input".to_owned();
    let store = S3Objects::connect(config).unwrap();
    let result = store.head(ObjectKey::for_bytes(b"denied"));
    assert_eq!(result, Err(ObjectError::Refused { status: 403 }));
    assert_eq!(store.diagnostics().unwrap().requests, 1);
}
#[test]
fn a_persisted_put_with_a_dropped_reply_is_uncertain_and_never_reconnected() {
    let config = support::config("lost-reply");
    let (proxied, proxy) = support::proxy(&config, support::Fault::DropReply);
    let store = S3Objects::connect(proxied).unwrap();
    let body = vec![7; 8192];
    let key = ObjectKey::for_bytes(&body);
    assert_eq!(store.put_if_absent(key, &body), Err(ObjectError::Uncertain));
    proxy.join().unwrap();
    let before = store.diagnostics().unwrap();
    assert_eq!(store.head(key), Err(ObjectError::Uncertain));
    assert_eq!(store.diagnostics().unwrap(), before);
    assert_eq!(before.requests, 1);
    assert_eq!(before.connections, 1);
    let verifier = S3Objects::connect(config).unwrap();
    assert_eq!(verifier.head(key).unwrap(), Some(body.len() as u64));
}
#[test]
fn a_malformed_real_reply_is_refused_once_with_no_alternate_request() {
    let config = support::config("malformed");
    let body = vec![5; 1000];
    let key = ObjectKey::for_bytes(&body);
    let producer = S3Objects::connect(config.clone()).unwrap();
    producer.put_if_absent(key, &body).unwrap();
    let (proxied, proxy) = support::proxy(&config, support::Fault::MalformedLength);
    let store = S3Objects::connect(proxied).unwrap();
    assert_eq!(
        store.read(key, None, &mut Vec::new()),
        Err(ObjectError::Malformed)
    );
    proxy.join().unwrap();
    let before = store.diagnostics().unwrap();
    assert_eq!(store.head(key), Err(ObjectError::Uncertain));
    assert_eq!(store.diagnostics().unwrap(), before);
    assert_eq!(before.requests, 1);
}
#[test]
fn labelled_body_size_diagnostics_have_no_timer_or_repeated_sample() {
    for size in [256 * 1024, 16 * 1024 * 1024] {
        let store = S3Objects::connect(support::config("body-counts")).unwrap();
        let body = (0..size).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let key = ObjectKey::for_bytes(&body);
        assert_eq!(store.put_if_absent(key, &body).unwrap(), Put::Created);
        assert_eq!(store.head(key).unwrap(), Some(size as u64));
        let mut out = Vec::new();
        store.read(key, None, &mut out).unwrap();
        assert_eq!(out, body);
        let counts = store.diagnostics().unwrap();
        assert_eq!(counts.requests, 3);
        assert_eq!((counts.puts, counts.gets, counts.heads), (1, 1, 1));
        assert_eq!(counts.body_sent, size as u64);
        assert_eq!(counts.body_received, size as u64);
        println!("DIAGNOSTIC opaque-body size={} {:?}", size, counts);
    }
}
