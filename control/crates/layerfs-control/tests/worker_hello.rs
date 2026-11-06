//! The worker's hello (#536 §9 `worker.hello`, §11 item 7): a worker
//! answers its secret with its protocol and bundle hash, and a caller sends
//! nothing to a worker of another protocol or bundle (an old worker, whose
//! hello names no protocol, included); the matching worker answers.
use layerfs_bridge::control::SdkValue;
use layerfs_machine::worker::{
    self, read_frame, read_frame_within, write_frame, Endpoint, Hello, MAX_HELLO, PROTOCOL,
};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

/// The bundle this build deploys in these tests: one folder whose
/// `bundle.json` names [`DEPLOYED`], set once for the whole binary.
const DEPLOYED: &str = "abcd0123";

fn deployed_bundle() {
    static SET: OnceLock<PathBuf> = OnceLock::new();
    SET.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("layerfs-hello-bundle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = format!("{{\"hash\":\"{DEPLOYED}\"}}");
        std::fs::write(dir.join("bundle.json"), manifest).unwrap();
        std::env::set_var("LAYERFS_BUNDLE", &dir);
        dir
    });
}

/// A stand-in worker answering one connection's secret with `answer`, then
/// reporting the next frame it read (`None` when the caller sent nothing
/// and closed), and answering a request `{ value: "served" }`.
fn stand_in(answer: SdkValue) -> (Endpoint, mpsc::Receiver<Option<SdkValue>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let _ = read_frame_within(&mut stream, MAX_HELLO);
        write_frame(&mut stream, &answer).unwrap();
        let request = read_frame(&mut stream).ok().flatten();
        if request.is_some() {
            let value = SdkValue::map([("value", SdkValue::text("served"))]);
            let _ = write_frame(&mut stream, &value);
        }
        let _ = sent.send(request);
    });
    let endpoint = Endpoint {
        address,
        secret: "k".to_owned(),
    };
    (endpoint, received)
}

fn hello(protocol: Option<i64>, bundle: Option<&str>) -> SdkValue {
    Hello {
        protocol,
        bundle: bundle.map(str::to_owned),
    }
    .to_frame()
}

/// Calls the stand-in once; answers the outcome and what it received.
fn call(answer: SdkValue) -> (Result<SdkValue, (String, String)>, Option<SdkValue>) {
    deployed_bundle();
    let (endpoint, received) = stand_in(answer);
    let outcome = worker::call(&endpoint, "worker.ping", SdkValue::Null, &mut |_| {});
    let refusal = outcome.as_ref().err().cloned();
    let request = received.recv_timeout(Duration::from_secs(15)).unwrap();
    if let Some(refusal) = refusal {
        assert!(worker::is_mismatch(&refusal), "{refusal:?}");
    }
    (outcome.map_err(|r| (r.code, r.message)), request)
}

#[test]
fn a_worker_of_another_protocol_is_refused_and_sent_nothing() {
    let (outcome, request) = call(hello(Some(PROTOCOL + 1), Some(DEPLOYED)));
    let (code, message) = outcome.unwrap_err();
    assert_eq!(code, "Unavailable");
    assert!(message.contains("nothing was sent"), "{message}");
    assert!(request.is_none(), "{request:?}");
}

#[test]
fn an_old_worker_whose_hello_names_no_protocol_is_refused_the_same_way() {
    let old = SdkValue::map([("ready", SdkValue::Bool(true))]);
    let (outcome, request) = call(old);
    assert_eq!(outcome.unwrap_err().0, "Unavailable");
    assert!(request.is_none(), "{request:?}");
}

#[test]
fn a_worker_of_another_bundle_is_refused_and_sent_nothing() {
    let (outcome, request) = call(hello(Some(PROTOCOL), Some("ffff9999")));
    let (code, message) = outcome.unwrap_err();
    assert_eq!(code, "Unavailable");
    assert!(message.contains("ffff9999"), "{message}");
    assert!(message.contains(DEPLOYED), "{message}");
    assert!(request.is_none(), "{request:?}");
}

#[test]
fn the_worker_of_this_protocol_and_bundle_answers() {
    let (outcome, request) = call(hello(Some(PROTOCOL), Some(DEPLOYED)));
    assert_eq!(outcome.unwrap(), SdkValue::text("served"));
    let request = request.expect("the request was sent");
    assert_eq!(request.get_text("op"), Some("worker.ping"));
}

#[test]
fn a_worker_run_from_no_sealed_bundle_is_checked_by_protocol_alone() {
    let (outcome, request) = call(hello(Some(PROTOCOL), None));
    assert_eq!(outcome.unwrap(), SdkValue::text("served"));
    assert!(request.is_some());
}

#[test]
fn a_worker_just_started_of_another_build_names_its_stale_bundle() {
    deployed_bundle();
    let dir = PathBuf::from("/cache/layerfs/bundle/2.0.0/linux-aarch64");
    let old = Hello {
        protocol: None,
        bundle: None,
    };
    let refusal = old.stale_bundle("lima1", &dir, "127.0.0.1:7").unwrap();
    assert_eq!(refusal.code, "Unavailable");
    assert!(worker::is_mismatch(&refusal), "{refusal:?}");
    for said in [
        "machine lima1 started",
        "protocol none",
        "/cache/layerfs/bundle/2.0.0/linux-aarch64 was sealed before this build",
        "tools/linux_bundle.py",
        "`layerfs machine stop lima1`, then `layerfs machine start lima1`",
    ] {
        assert!(
            refusal.message.contains(said),
            "{said}: {}",
            refusal.message
        );
    }
    assert_eq!(
        refusal.detail.get_text("bundle"),
        Some("/cache/layerfs/bundle/2.0.0/linux-aarch64")
    );
    let current = Hello {
        protocol: Some(PROTOCOL),
        bundle: Some(DEPLOYED.to_owned()),
    };
    assert!(current.stale_bundle("lima1", &dir, "127.0.0.1:7").is_none());
}

#[test]
fn the_library_worker_says_its_protocol_and_answers() {
    deployed_bundle();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("layerfs-hello-root-{}", std::process::id()));
    let served = root.clone();
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), served)
    });
    let endpoint = Endpoint {
        address,
        secret: "k".to_owned(),
    };
    // a test binary runs from no sealed bundle
    let said = worker::hello(&endpoint).unwrap();
    assert_eq!(said.protocol, Some(PROTOCOL));
    assert_eq!(said.bundle, None);
    assert_eq!(
        worker::ping(&endpoint).unwrap().get_text("kind"),
        Some("worker")
    );
    // a wrong secret is still the worker's refusal
    let wrong = Endpoint {
        address,
        secret: "no".to_owned(),
    };
    assert_eq!(worker::hello(&wrong).unwrap_err().code, "Denied");
    let _ = std::fs::remove_dir_all(root);
}
