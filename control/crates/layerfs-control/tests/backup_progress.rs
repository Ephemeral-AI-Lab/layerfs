//! The progress `backup.create` and `backup.restore` send (layerfs-project
//! #5): the bucket library's byte and piece counts as `{ phase, done,
//! total, pieces }`, sent while the transfer runs, each phase ending on its
//! exact count, and a failed transfer ending on what it moved.
use layerfs_bridge::control::SdkValue;
use layerfs_bucket::bucket::progress as bucket;
use layerfs_home::events::Events;
use layerfs_store::backup::progress::Shown;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// `(phase, done, total, pieces)` of one update.
type Seen = (String, Option<i64>, Option<i64>, Option<(i64, i64)>);

fn seen(event: &SdkValue) -> Seen {
    let progress = event.get("progress").expect("a progress event");
    let pieces = match progress.get("pieces") {
        Some(SdkValue::Null) => None,
        Some(p) => Some((p.get_int("done").unwrap(), p.get_int("of").unwrap())),
        None => panic!("no pieces field"),
    };
    (
        progress.get_text("phase").unwrap().to_owned(),
        progress.get_int("done"),
        progress.get_int("total"),
        pieces,
    )
}

/// Runs `work` as one transfer and returns its value and what was sent.
fn transfer<T: Send>(work: impl FnOnce() -> T + Send) -> (T, Vec<Seen>) {
    let mut events = Vec::new();
    let mut sink = |event: SdkValue| events.push(seen(&event));
    let mut stream = Events::new(&mut sink);
    let value = Shown::new().transfer(&mut stream, work);
    (value, events)
}

/// Every phase's updates count up and the phase ends on its total.
fn counts_up(events: &[Seen]) {
    for pair in events.windows(2) {
        if pair[0].0 == pair[1].0 {
            assert!(pair[0].1 <= pair[1].1, "{events:?}");
        }
    }
}

#[test]
fn phases_end_on_their_exact_counts_and_pieces_are_each_sent() {
    let (_, events) = transfer(|| {
        bucket::begin("hash", Some(10));
        bucket::add(4);
        bucket::add(6);
        bucket::begin("upload", Some(10));
        bucket::pieces(0, 2);
        bucket::add(5);
        bucket::pieces(1, 2);
        bucket::add(5);
        bucket::pieces(2, 2);
    });
    counts_up(&events);
    let t = |s: &str| s.to_owned();
    // the hash phase's last update is its total, before upload starts
    let last_hash = events.iter().rposition(|e| e.0 == "hash").unwrap();
    assert_eq!(events[last_hash], (t("hash"), Some(10), Some(10), None));
    assert!(events[last_hash + 1..].iter().all(|e| e.0 == "upload"));
    for piece in [(0, 2, 0), (1, 2, 5), (2, 2, 10)] {
        let want = (
            t("upload"),
            Some(piece.2),
            Some(10),
            Some((piece.0, piece.1)),
        );
        assert!(events.contains(&want), "{want:?} not in {events:?}");
    }
    assert_eq!(
        events.last(),
        Some(&(t("upload"), Some(10), Some(10), Some((2, 2))))
    );
    // nothing is sent twice in a row
    assert!(events.windows(2).all(|p| p[0] != p[1]), "{events:?}");
}

#[test]
fn counts_are_sent_while_the_transfer_runs() {
    let ended = Arc::new(AtomicBool::new(false));
    let mut during = Vec::new();
    let mut sink = |event: SdkValue| {
        if !ended.load(Ordering::SeqCst) {
            during.push(seen(&event));
        }
    };
    let mut stream = Events::new(&mut sink);
    let flag = ended.clone();
    Shown::new().transfer(&mut stream, move || {
        bucket::begin("upload", Some(3));
        for _ in 0..3 {
            bucket::add(1);
            std::thread::sleep(Duration::from_millis(400));
        }
        flag.store(true, Ordering::SeqCst);
    });
    let partial: Vec<_> = during
        .iter()
        .filter(|e| e.1.is_some_and(|done| (1..3).contains(&done)))
        .collect();
    assert!(!partial.is_empty(), "no update before the end: {during:?}");
}

#[test]
fn a_failed_transfer_ends_on_what_it_moved() {
    let (result, events) = transfer(|| {
        bucket::begin("upload", Some(100));
        bucket::add(40);
        Err::<(), _>("connection reset")
    });
    assert_eq!(result, Err("connection reset"));
    assert_eq!(
        events.last(),
        Some(&("upload".to_owned(), Some(40), Some(100), None))
    );
    assert!(events.iter().all(|e| e.1 != Some(100)), "{events:?}");
}

#[test]
fn a_panic_in_the_transfer_is_resumed_after_its_updates() {
    let mut events = Vec::new();
    let mut sink = |event: SdkValue| events.push(seen(&event));
    let mut stream = Events::new(&mut sink);
    let mut shown = Shown::new();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        shown.transfer(&mut stream, || {
            bucket::begin("download", Some(8));
            bucket::add(2);
            panic!("cut");
        })
    }));
    assert!(outcome.is_err());
    assert_eq!(
        events.last(),
        Some(&("download".to_owned(), Some(2), Some(8), None))
    );
}

#[test]
fn the_calls_own_phases_carry_no_bytes_and_are_not_repeated() {
    let mut events = Vec::new();
    let mut sink = |event: SdkValue| events.push(seen(&event));
    let mut stream = Events::new(&mut sink);
    let mut shown = Shown::new();
    shown.phase(&mut stream, "check");
    shown.transfer(&mut stream, || bucket::begin("install", None));
    shown.phase(&mut stream, "install");
    shown.bytes(&mut stream, "send", 8, None);
    let t = |s: &str| s.to_owned();
    assert_eq!(
        events,
        vec![
            (t("check"), None, None, None),
            (t("install"), None, None, None),
            (t("send"), Some(8), None, None),
        ]
    );
}

#[test]
fn reports_outside_a_transfer_go_nowhere() {
    // the observer is the transfer thread's only
    bucket::begin("upload", Some(1));
    let (_, events) = transfer(|| ());
    assert_eq!(events, vec![]);
}
