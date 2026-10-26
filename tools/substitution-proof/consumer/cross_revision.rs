//! Cross-revision persisted-data check: one consumer, two modes.
//!
//! `create <path>` writes one file into a fresh Store and prints its root.
//! `read <path> <root>` reopens that Store and reads the file back.
//!
//! The same source is compiled into every arm, so running `create` under one
//! dependency selection and `read` under another is exactly the "candidate reads
//! baseline-created data / baseline reads candidate-created data" check the Stage 7
//! scenario requires. A refusal is a typed result, not a panic: the process prints
//! the error and exits 3, so a fail-closed format change is recorded rather than
//! hidden behind an `expect`.
//!
//! Exit codes: 0 success, 2 usage, 3 the Store refused the file, 4 a save or read
//! failed for another typed reason.

mod support;

use layerfs_content::{construct_stream, read_all, ConstructionPolicy, ObjectId};
use layerfs_storage::{SaveHandoff, StorageError, StoragePolicy, Store, StoreProvider};
use support::disabled;

const PAYLOAD_BYTES: usize = 4096;

fn save_one(store: &Store, bytes: &[u8]) -> Result<ObjectId, StorageError> {
    let policy = ConstructionPolicy::frozen_default();
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let constructed = construct_stream(
            policy,
            &policy.capacities(),
            bytes,
            &mut handoff,
            scope.child("content"),
        );
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        let constructed = constructed?;
        operation.finish(scope.child("storage.finish"))?;
        Ok(constructed.root)
    })
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() < 2 {
        eprintln!("usage: cross_revision <create|read> <path> [root]");
        std::process::exit(2);
    }
    let path = std::path::Path::new(&arguments[1]);
    match arguments[0].as_str() {
        "create" => {
            let store = disabled(|scope| {
                Store::create(path, StoragePolicy::frozen_default(), scope.child("store"))
            })
            .expect("store creation");
            let bytes = support::noise(PAYLOAD_BYTES);
            let root = save_one(&store, &bytes).expect("save");
            println!("created root {root} payload {PAYLOAD_BYTES}");
        }
        "read" => {
            let root = arguments
                .get(2)
                .and_then(|text| text.parse::<ObjectId>().ok())
                .expect("root argument");
            let store = match disabled(|scope| Store::open(path, scope.child("store"))) {
                Ok(store) => store,
                Err(error) => {
                    println!("refused-on-open {error}");
                    std::process::exit(3);
                }
            };
            let mut out = Vec::new();
            let read = disabled(|scope| {
                read_all(&StoreProvider::new(&store), root, &mut out, scope.child("content.read"))
            });
            match read {
                Ok(_counters) => println!(
                    "read bytes {} digest {}",
                    out.len(),
                    ObjectId::for_bytes(&out)
                ),
                Err(error) => {
                    println!("refused-on-read {error}");
                    std::process::exit(3);
                }
            }
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    }
}
