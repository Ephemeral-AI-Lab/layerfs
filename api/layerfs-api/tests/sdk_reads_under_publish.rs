//! Object locates and pack reads run without a transaction (#354, #352
//! F-035): each statement takes its own snapshot, which is safe only because
//! the rows they read never change once visible. While another Branch keeps
//! publishing (new packs, new object locations, a moving head), paged and
//! multi-object reads of `main` answer exactly as they did before.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use support::{write, Fixture};

const FILES: usize = 150;
/// Commits the writer publishes at most, and the rounds the reader runs at
/// most: both loops end on their own.
const COMMITS: usize = 40;
const ROUNDS: usize = 60;

/// Every page of one paged call, in order.
fn pages(fixture: &Fixture, method: &str, fields: &[(&str, SdkValue)]) -> Vec<String> {
    let (mut out, mut pos) = (Vec::new(), 0);
    loop {
        let mut call = fields.to_vec();
        call.push(("pageSize", SdkValue::Int(7)));
        call.push(("pagePos", SdkValue::Int(pos)));
        let page = fixture.call(method, call);
        out.push(format!("{page:?}"));
        match page.get_int("nextPagePos") {
            Some(next) => pos = next,
            None => return out,
        }
    }
}

/// The reads checked: paged grep and glob over the tree, a listing, and
/// whole-file reads, all of `main`.
fn battery(fixture: &Fixture) -> Vec<String> {
    let main = ("branch", SdkValue::text("main"));
    let mut out = pages(
        fixture,
        "fs.grep",
        &[main.clone(), ("pattern", SdkValue::text("needle"))],
    );
    out.extend(pages(
        fixture,
        "fs.glob",
        &[main.clone(), ("pattern", SdkValue::text("**"))],
    ));
    out.extend(pages(
        fixture,
        "fs.list",
        &[main.clone(), ("path", SdkValue::text("/d1"))],
    ));
    for f in (0..FILES).step_by(17) {
        let path = format!("/d{}/f{f:03}.txt", f % 5);
        out.push(format!(
            "{:?}",
            fixture.call(
                "fs.read",
                vec![main.clone(), ("path", SdkValue::text(&path))]
            )
        ));
    }
    out
}

#[test]
fn reads_of_main_are_unchanged_while_another_branch_publishes() {
    let fixture = Fixture::new("sdk-reads-under-publish", |source| {
        for f in 0..FILES {
            let dir = source.join(format!("d{}", f % 5));
            std::fs::create_dir_all(&dir).unwrap();
            let mut text = String::new();
            for line in 0..40 {
                if (f + line) % 13 == 0 {
                    text.push_str(&format!("line {line} of {f} has a needle\n"));
                } else {
                    text.push_str(&format!("line {line} of file {f}\n"));
                }
            }
            std::fs::write(dir.join(format!("f{f:03}.txt")), text).unwrap();
        }
        FILES + 5
    });
    fixture.fork("work");
    let before = battery(&fixture);
    assert!(before.len() > 10, "the battery pages");

    let (stop, published, rounds) = (
        AtomicBool::new(false),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    let started = Instant::now();
    let changed = std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            for n in 0..COMMITS {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let new = format!("/d{}/n{n:03}.txt", n % 5);
                let old = format!("/d{}/f{:03}.txt", n % 5, n);
                fixture.apply(
                    "work",
                    vec![
                        write(&new, format!("new {n} needle\n").repeat(50)),
                        write(&old, format!("rewritten {n}\n").repeat(30)),
                    ],
                );
                published.fetch_add(1, Ordering::Relaxed);
            }
        });
        let mut changed = Vec::new();
        // Read until the writer is done, at most ROUNDS rounds.
        while rounds.load(Ordering::Relaxed) < ROUNDS && !writer.is_finished() {
            let now = battery(&fixture);
            if now != before {
                changed.push(rounds.load(Ordering::Relaxed));
            }
            rounds.fetch_add(1, Ordering::Relaxed);
        }
        stop.store(true, Ordering::Relaxed);
        writer.join().expect("writer");
        changed
    });
    let published = published.load(Ordering::Relaxed);
    println!(
        "published {published} commits during {} read rounds",
        rounds.load(Ordering::Relaxed)
    );
    assert!(published > 0, "the writer published while reads ran");
    assert!(
        rounds.load(Ordering::Relaxed) > 0,
        "reads ran while the writer published"
    );
    assert!(
        changed.is_empty(),
        "rounds whose answers changed: {changed:?}"
    );
    // The other Branch did move: its reads see the new files.
    let work = fixture.call(
        "fs.glob",
        vec![
            ("branch", SdkValue::text("work")),
            ("pattern", SdkValue::text("n*.txt")),
        ],
    );
    assert_eq!(support::items(&work).len(), published);
    assert!(started.elapsed() < Duration::from_secs(100));
    fixture.cleanup();
}
