//! A Store call whose machine's worker is of another build (#536 §9
//! `worker.hello`) is refused before it is sent, naming the calls that
//! replace the worker: `machine stop`, then `machine start` (#535 D11).
use layerfs_api::home::{self, paths, write_record};
use layerfs_api::worker::{read_frame, read_frame_within, write_frame, MAX_HELLO};
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

#[test]
fn a_store_call_to_an_old_worker_names_stop_then_start_and_sends_nothing() {
    let home = std::env::temp_dir().join(format!("layerfs-mismatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    home::open(&home).unwrap();
    let store = SdkValue::map([
        ("name", t("shop")),
        ("kind", t("sqlite")),
        ("binding", SdkValue::Bytes(vec![7; 16])),
        ("cursor", SdkValue::Bytes(vec![8; 32])),
        ("profile", t("disposable")),
        ("machine", t("box")),
    ]);
    write_record(&paths::store_record(&home, "shop"), &store).unwrap();
    let project = SdkValue::map([("name", t("api"))]);
    write_record(&paths::project_record(&home, "shop", "api"), &project).unwrap();
    let machine = SdkValue::map([("name", t("box")), ("platform", t("lima"))]);
    write_record(&paths::machine_record(&home, "box"), &machine).unwrap();

    // an old worker: its hello is `{ ready: true }`, with no protocol
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let _ = read_frame_within(&mut stream, MAX_HELLO);
        let ready = SdkValue::map([("ready", SdkValue::Bool(true))]);
        write_frame(&mut stream, &ready).unwrap();
        let _ = sent.send(read_frame(&mut stream).ok().flatten());
    });
    let worker = SdkValue::map([("address", t(&address.to_string())), ("secret", t("k"))]);
    write_record(&paths::worker_record(&home, "box"), &worker).unwrap();

    let args = SdkValue::map([
        ("project", t("shop/api")),
        ("branch", t("main")),
        ("path", t("/a")),
    ]);
    let refusal =
        layerfs_api::calls::call(Some(&home), "file.read", args, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    let message = &refusal.message;
    assert!(message.contains("`layerfs machine stop box`"), "{message}");
    assert!(message.contains("`layerfs machine start box`"), "{message}");
    assert!(refusal.detail.get("expected").is_some(), "{refusal:?}");
    let request = received.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(request.is_none(), "the old worker got {request:?}");
    std::fs::remove_dir_all(home).unwrap();
}
