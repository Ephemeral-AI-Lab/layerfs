//! Compose independently recorded reports through ordinary function calls.
//!
//! Every helper records its own operation and returns its original result
//! together with its completed report. The caller measures the call, attaches
//! the owned report below its own measured node, and finally saves the
//! assembled tree to a caller-selected path. The save result stays separate
//! from the product result.
//!
//! Run with:
//!
//! ```text
//! cargo run --manifest-path core/Cargo.toml --example timer_composition
//! cargo run --manifest-path core/Cargo.toml --example timer_composition -- /tmp/run-1/timing.json
//! ```

use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use layerfs_telemetry::timer::{Timing, TimingReport};

/// Error type of this example's storage operations.
#[derive(Debug)]
enum StoreError {
    /// The storage engine was busy.
    Busy,
}

fn compress(rows: usize) -> Result<usize, StoreError> {
    if rows == 0 {
        return Err(StoreError::Busy);
    }
    Ok(rows * 2)
}

fn insert(rows: usize) -> Result<usize, StoreError> {
    Ok(rows + 1)
}

/// An independently recorded compression step.
fn pack_rows(rows: usize) -> (Result<usize, StoreError>, TimingReport) {
    Timing::record("pack", |root| {
        root.child("compress").run(|_| compress(rows))
    })
}

/// An independently recorded storage execution that composes the packing report.
fn storage_execute(rows: usize) -> (Result<usize, StoreError>, TimingReport) {
    Timing::record("storage.execute", |root| {
        let (packed, packing) = pack_rows(rows);
        root.attach(Some(packing));
        root.child("sqlite.insert").run(|_| insert(packed?))
    })
}

/// Saves the assembled report; the caller owns the path and the result.
fn save_report(report: &TimingReport, path: &Path) -> io::Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = BufWriter::new(file);
    report.write_json(&mut writer)?;
    writer.flush()
}

fn main() -> io::Result<()> {
    let (result, timings) = Timing::record("object.create", |root| {
        let rows = root.child("storage.save").run(|call| {
            let (result, execution) = storage_execute(3);
            call.attach(Some(execution));
            result
        })?;
        root.child("finalize")
            .run(|_| Ok::<usize, StoreError>(rows))
    });

    println!("product result: {result:?}");
    timings.write_text(io::stdout())?;

    match std::env::args_os().nth(1) {
        None => println!("no output path given; the report stays in memory"),
        Some(path) => {
            let path = Path::new(&path);
            match save_report(&timings, path) {
                Ok(()) => println!("saved report to {}", path.display()),
                Err(error) => println!("save failed, product result unchanged: {error}"),
            }
        }
    }
    Ok(())
}
