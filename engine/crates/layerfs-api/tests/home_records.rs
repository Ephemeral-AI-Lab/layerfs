//! Record writes in the home from several threads at once (#533 review
//! P1-11): each write has its own temporary file, the home lock is taken
//! again at once by the thread that holds it, and concurrent first calls
//! on a fresh home all succeed.
use layerfs_api::calls;
use layerfs_api::home::{self, lock, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Barrier};
use std::time::Duration;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-records-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn one_record_written_by_many_threads_at_once() {
    let dir = tempdir("same");
    let path = dir.join("record.json");
    let start = Arc::new(Barrier::new(8));
    let writers: Vec<_> = (0..8)
        .map(|thread| {
            let (path, start) = (path.clone(), start.clone());
            std::thread::spawn(move || {
                start.wait();
                for n in 0..25 {
                    write_record(
                        &path,
                        &SdkValue::map([
                            ("thread", SdkValue::Int(thread)),
                            ("n", SdkValue::Int(n)),
                        ]),
                    )
                    .map_err(|r| format!("{}: {}", r.code, r.message))?;
                }
                Ok::<(), String>(())
            })
        })
        .collect();
    for writer in writers {
        writer.join().unwrap().unwrap();
    }
    let record = read_record(&path).unwrap().expect("one whole record");
    assert_eq!(record.get_int("n"), Some(24));
    assert_eq!(names(&dir), ["record.json"], "no temporary file is left");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_holding_thread_takes_the_home_lock_again_at_once() {
    let home = tempdir("lock");
    let outer = lock::take(&home).unwrap();
    let inner = lock::take(&home).expect("the same thread takes it again");
    let (sent, got) = mpsc::channel();
    let other = {
        let home = home.clone();
        std::thread::spawn(move || {
            let held = lock::take(&home).map(|_| ());
            sent.send(held.is_ok()).unwrap();
        })
    };
    drop(inner);
    assert!(
        got.recv_timeout(Duration::from_millis(300)).is_err(),
        "another thread waits while the outer take is held"
    );
    drop(outer);
    assert_eq!(got.recv_timeout(Duration::from_secs(10)), Ok(true));
    other.join().unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn first_calls_on_a_fresh_home_from_several_threads() {
    let root = tempdir("fresh");
    let home = root.join("home");
    let start = Arc::new(Barrier::new(8));
    let callers: Vec<_> = (0..8)
        .map(|n| {
            let (home, start) = (home.clone(), start.clone());
            std::thread::spawn(move || {
                start.wait();
                let (name, args) = if n % 2 == 0 {
                    ("project.list", SdkValue::Map(Vec::new()))
                } else {
                    (
                        "store.create",
                        SdkValue::map([("name", SdkValue::text(format!("s{n}")))]),
                    )
                };
                calls::call(Some(&home), name, args, &mut |_| {})
                    .map(|_| ())
                    .map_err(|r| format!("{name}: {} {}", r.code, r.message))
            })
        })
        .collect();
    for caller in callers {
        caller.join().unwrap().unwrap();
    }
    let file = home::open(&home).unwrap();
    assert_eq!(file.id.len(), 32);
    for n in [1, 3, 5, 7] {
        let record = read_record(&paths::store_record(&home, &format!("s{n}")))
            .unwrap()
            .expect("each Store's record");
        assert_eq!(record.get_text("machine"), Some("native"));
    }
    if cfg!(feature = "machine") {
        for machine in ["native", "docker"] {
            assert!(read_record(&paths::machine_record(&home, machine))
                .unwrap()
                .is_some());
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
