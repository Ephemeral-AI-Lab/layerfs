//! What a transfer recovers from (#13 P6, #9): `store remove` deletes the
//! partial copy a certain `store push` left on its target, whether the dead
//! push's intent or, after a certain failure dropped it, the record's
//! `incoming_on` names that machine; and `store pull` asks a machine's worker
//! again for a read-only piece whose answer was lost, never for a step that
//! writes.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{paths, private_dir, write_entity, write_record};
use layerfs_home::names::StoreName;
use layerfs_machine::worker::{read_frame, read_frame_within, write_frame, Hello, MAX_HELLO};
use layerfs_store::record::StoreRecord;
use layerfs_store::sqlite::family;
use layerfs_store::sqlite::transfer::step::{StepFrame, StoreStep};
use layerfs_store::Related;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

static NEXT: AtomicU32 = AtomicU32::new(0);

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn new_dir(tag: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("layerfs-recovery-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The `InUse` checks of a test: nothing holds a Store.
fn idle(_home: &Path, _name: &str) -> Result<(), SdkRefusal> {
    Ok(())
}

fn related() -> Related {
    Related {
        store_idle: idle,
        can_remove_project: idle,
    }
}

/// A home with machine `box` (a Lima VM) whose calls the worker at `address`
/// answers.
fn box_machine(home: &Path, address: &str) {
    let machine = SdkValue::map([("name", t("box")), ("platform", t("lima"))]);
    write_entity(&paths::machine_record(home, "box"), &machine).unwrap();
    let worker = SdkValue::map([("address", t(address)), ("secret", t("k"))]);
    write_record(&paths::worker_record(home, "box"), &worker).unwrap();
}

/// A new home and worker root, with a worker serving `box`'s calls on the
/// root in this process.
fn home_with_box(tag: &str) -> (PathBuf, PathBuf) {
    let home = new_dir(tag);
    layerfs_control::migrate::open(&home).unwrap();
    let root = new_dir(&format!("{tag}-root"));
    private_dir(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), guest)
    });
    box_machine(&home, &address);
    (home, root)
}

/// A certain `store.push` intent to `box` whose owner (pid 0) is gone.
fn certain_push(phase: &str) -> SdkValue {
    SdkValue::map([
        ("id", t("op-00000000000000bb")),
        ("name", t("store.push")),
        ("phase", t(phase)),
        (
            "args",
            SdkValue::map([("from", t("native")), ("to", t("box"))]),
        ),
        ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
        ("started", SdkValue::Int(1)),
        ("updated", SdkValue::Int(1)),
        ("uncertain", SdkValue::Bool(false)),
    ])
}

/// Store `shop` on native with `fields`, and a partial copy on `box`.
fn shop_with_partial(home: &Path, root: &Path, fields: Vec<(&str, SdkValue)>) -> PathBuf {
    let mut record = StoreRecord::from_record(
        "shop",
        SdkValue::map([
            ("name", t("shop")),
            ("kind", t("sqlite")),
            ("machine", t("native")),
        ]),
    );
    for (field, value) in fields {
        record = record.with(field, value);
    }
    record.write(home).unwrap();
    let remote = paths::store_dir(root, "shop");
    private_dir(&remote).unwrap();
    std::fs::write(remote.join("store.sqlite.incoming"), b"partial-bytes").unwrap();
    remote
}

/// A `store.push` whose process died certain at `send` left the target's
/// partial copy, and `store.remove` deletes it with the Store.
#[test]
fn store_remove_deletes_a_dead_pushes_partial_copy() {
    let (home, root) = home_with_box("certain-op");
    let remote = shop_with_partial(&home, &root, vec![("op", certain_push("send"))]);
    let machine = paths::machine_record(&home, "box");

    let name = StoreName::parse("shop").unwrap();
    let removed = family::remove(&home, &name, related()).unwrap();

    assert_eq!(removed.get_text("removed"), Some("shop"), "{removed:?}");
    assert!(
        !paths::store_dir(&home, "shop").exists(),
        "the Store is left"
    );
    assert!(!remote.exists(), "the target keeps a partial copy");
    assert!(machine.is_file(), "the machine was removed with the Store");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// A certain failure dropped the intent that named `box`: the record keeps
/// the machine, so `store.remove` finds that copy too.
#[test]
fn store_remove_deletes_a_certain_failures_partial_copy() {
    let (home, root) = home_with_box("certain-record");
    let remote = shop_with_partial(&home, &root, vec![("incoming_on", t("box"))]);

    let name = StoreName::parse("shop").unwrap();
    let removed = family::remove(&home, &name, related()).unwrap();

    assert_eq!(removed.get_text("removed"), Some("shop"), "{removed:?}");
    assert!(
        !paths::store_dir(&home, "shop").exists(),
        "the Store is left"
    );
    assert!(!remote.exists(), "the target keeps a partial copy");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// A worker on a new port that runs `store.file` steps on `root` and, as the
/// first piece lands, puts a Store file where `install` would rename into:
/// `install` then refuses `Exists` for certain, with `send` and `verify` behind
/// it.
fn steals_the_target(root: &Path) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let mut stolen = false;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let hello = read_frame_within(&mut stream, MAX_HELLO).ok().flatten();
            if hello.and_then(|h| h.get_text("secret").map(str::to_owned)) != Some("k".to_owned()) {
                continue;
            }
            if write_frame(&mut stream, &Hello::own().to_frame()).is_err() {
                continue;
            }
            let Some(request) = read_frame(&mut stream).ok().flatten() else {
                continue;
            };
            let args = request.get("args").cloned().unwrap_or_default();
            let Some(frame) = StepFrame::decode(&args).ok() else {
                continue;
            };
            let answer = match layerfs_store::sqlite::transfer::steps::run(&root, &frame) {
                Ok(value) => {
                    // another writer takes the target's name between the check
                    // and the install
                    if !stolen && matches!(frame.step, StoreStep::Write { .. }) {
                        std::fs::write(paths::store_file(&root, "shop"), b"taken").unwrap();
                        stolen = true;
                    }
                    SdkValue::map([("value", value)])
                }
                Err(refusal) => SdkValue::map([(
                    "refused",
                    SdkValue::map([
                        ("code", t(&refusal.code)),
                        ("message", t(&refusal.message)),
                        ("detail", refusal.detail),
                    ]),
                )]),
            };
            let _ = write_frame(&mut stream, &answer);
        }
    });
    address
}

/// A certain failure at `install` keeps the target's partial copy as well as
/// the intent did: the record names the machine, `store.remove` deletes that
/// copy, and the Store stays on `native`.
#[test]
fn a_certain_install_failure_keeps_the_target_in_the_record() {
    let home = new_dir("install");
    layerfs_control::migrate::open(&home).unwrap();
    let root = new_dir("install-root");
    private_dir(&root).unwrap();
    let address = steals_the_target(&root);
    box_machine(&home, &address);
    let name = StoreName::parse("shop").unwrap();
    family::create(&home, &name).unwrap();
    let remote = paths::store_dir(&root, "shop");

    let mut sink = |_: SdkValue| {};
    let mut events = Events::new(&mut sink);
    let request = family::StorePush {
        name: "shop".to_owned(),
        machine: "box".to_owned(),
    };
    let refused = family::push(&home, &mut events, &request, related()).unwrap_err();

    assert_eq!(refused.code, "Exists", "{}", refused.message);
    assert!(
        refused.message.contains("store push, install:"),
        "{}",
        refused.message
    );
    assert_eq!(refused.detail.get_text("phase"), Some("install"));
    assert!(
        remote.join("store.sqlite.incoming").is_file(),
        "the target keeps the partial copy it wrote"
    );
    let record = StoreRecord::read(&home, "shop").unwrap().unwrap();
    assert_eq!(record.machine().unwrap(), "native", "the record moved on");
    assert!(record.op().is_none(), "the intent is left");
    assert_eq!(
        record.field("incoming_on"),
        Some(&t("box")),
        "the record does not name the machine holding the copy"
    );

    let removed = family::remove(&home, &name, related()).unwrap();
    assert_eq!(removed.get_text("removed"), Some("shop"), "{removed:?}");
    assert!(!remote.exists(), "the target keeps a partial copy");
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// The Store `shop` as it comes to `box`: `store.create` on native, its file
/// moved under the worker's root, the record naming `box`. Answers the bytes
/// the file had.
fn store_on_box(home: &Path, root: &Path) -> Vec<u8> {
    let name = StoreName::parse("shop").unwrap();
    family::create(home, &name).unwrap();
    let local = paths::store_file(home, "shop");
    let bytes = std::fs::read(&local).unwrap();
    let remote = paths::store_dir(root, "shop");
    private_dir(&remote).unwrap();
    std::fs::write(remote.join("store.sqlite"), &bytes).unwrap();
    std::fs::remove_file(&local).unwrap();
    let record = StoreRecord::read(home, "shop")
        .unwrap()
        .unwrap()
        .with("machine", t("box"));
    record.write(home).unwrap();
    bytes
}

/// A worker on a new port that runs `store.file` steps on `root` and drops the
/// answer to the first `drops` reads, as a lost connection does; answers how
/// many reads it saw.
fn flaky(root: &Path, drops: u32) -> (String, Arc<AtomicU32>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let reads = Arc::new(AtomicU32::new(0));
    let seen = reads.clone();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let mut left = drops;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let hello = read_frame_within(&mut stream, MAX_HELLO).ok().flatten();
            if hello.and_then(|h| h.get_text("secret").map(str::to_owned)) != Some("k".to_owned()) {
                continue;
            }
            if write_frame(&mut stream, &Hello::own().to_frame()).is_err() {
                continue;
            }
            let Some(request) = read_frame(&mut stream).ok().flatten() else {
                continue;
            };
            let args = request.get("args").cloned().unwrap_or_default();
            let Some(frame) = StepFrame::decode(&args).ok() else {
                continue;
            };
            if matches!(frame.step, StoreStep::Read { .. }) {
                seen.fetch_add(1, Ordering::Relaxed);
                if left > 0 {
                    // the answer is lost: the connection closes unanswered
                    left -= 1;
                    continue;
                }
            }
            let answer = match layerfs_store::sqlite::transfer::steps::run(&root, &frame) {
                Ok(value) => SdkValue::map([("value", value)]),
                Err(refusal) => SdkValue::map([(
                    "refused",
                    SdkValue::map([
                        ("code", t(&refusal.code)),
                        ("message", t(&refusal.message)),
                        ("detail", refusal.detail),
                    ]),
                )]),
            };
            let _ = write_frame(&mut stream, &answer);
        }
    });
    (address, reads)
}

/// A pull whose two read answers are lost asks again for the same piece and
/// finishes: the reads changed nothing, so asking once more is safe.
#[test]
fn a_pull_asks_again_for_a_piece_whose_answer_was_lost() {
    let home = new_dir("retry");
    layerfs_control::migrate::open(&home).unwrap();
    let root = new_dir("retry-root");
    private_dir(&root).unwrap();
    let (address, reads) = flaky(&root, 2);
    box_machine(&home, &address);
    let bytes = store_on_box(&home, &root);

    let mut sink = |_: SdkValue| {};
    let mut events = Events::new(&mut sink);
    let started = std::time::Instant::now();
    let pulled = family::pull(&home, &mut events, "shop", related()).unwrap();
    let took = started.elapsed();

    assert_eq!(pulled.get_text("from"), Some("box"), "{pulled:?}");
    assert_eq!(pulled.get_text("to"), Some("native"), "{pulled:?}");
    assert_eq!(
        pulled.get_int("bytes"),
        Some(bytes.len() as i64),
        "{pulled:?}"
    );
    // one read served, two answers lost and asked again
    assert_eq!(reads.load(Ordering::Relaxed), 3, "the pieces read");
    assert!(took >= Duration::from_millis(90), "two pauses: {took:?}");
    assert_eq!(
        std::fs::read(paths::store_file(&home, "shop")).unwrap(),
        bytes,
        "the Store pulled is not the one sent"
    );
    let record = StoreRecord::read(&home, "shop").unwrap().unwrap();
    assert_eq!(record.machine().unwrap(), "native");
    assert!(record.op().is_none(), "the intent is left");
    assert!(record.field("incoming_on").is_none(), "the hint is left");
    let last = record.field("last_transfer").unwrap();
    assert_eq!(last.get_text("from"), Some("box"));
    assert_eq!(last.get("retired"), Some(&SdkValue::Bool(true)));
    assert!(
        !paths::store_dir(&root, "shop").exists(),
        "the source keeps its store"
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// A worker that loses every read answer fails the pull after three more
/// attempts, and the intent stays unresolved: a step that writes is never
/// replayed.
#[test]
fn a_pull_stops_asking_after_three_more_attempts() {
    let home = new_dir("lost");
    layerfs_control::migrate::open(&home).unwrap();
    let root = new_dir("lost-root");
    private_dir(&root).unwrap();
    let (address, reads) = flaky(&root, 9);
    box_machine(&home, &address);
    store_on_box(&home, &root);

    let mut sink = |_: SdkValue| {};
    let mut events = Events::new(&mut sink);
    let refused = family::pull(&home, &mut events, "shop", related()).unwrap_err();

    assert_eq!(refused.code, "Unknown", "{}", refused.message);
    assert!(
        refused.message.contains("store pull, send:"),
        "{}",
        refused.message
    );
    assert_eq!(refused.detail.get_text("phase"), Some("send"));
    assert_eq!(
        refused.detail.get_int("retries"),
        Some(3),
        "the piece was asked again"
    );
    // the first attempt and three more
    assert_eq!(reads.load(Ordering::Relaxed), 4, "the pieces read");
    let record = StoreRecord::read(&home, "shop").unwrap().unwrap();
    assert_eq!(record.machine().unwrap(), "box", "the record moved on");
    let op = record.op().expect("the intent is kept");
    assert_eq!(op.name(), "store.pull");
    assert_eq!(op.phase(), "send");
    assert!(op.uncertain(), "the piece may have been read");
    assert!(
        !paths::store_file(&home, "shop").exists(),
        "a write ran on native"
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}
