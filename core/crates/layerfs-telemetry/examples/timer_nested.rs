//! Nested timing with child scopes injected into local components.
//!
//! The same component functions run under `Timing::record` and under
//! `Timing::disabled`; only the injected scope changes.
//!
//! Run with:
//!
//! ```text
//! cargo run --manifest-path core/Cargo.toml --example timer_nested
//! ```

use std::io;

use layerfs_telemetry::timer::{Timing, TimingReport, TimingScope};

/// Error type of this example's domain operations.
#[derive(Debug)]
enum ExampleError {
    /// There was no input to canonicalize.
    EmptyInput,
    /// Storage rejected an object without a canonical identity.
    Rejected,
}

/// A canonical object handed from construction to storage.
struct Object {
    id: String,
    bytes: Vec<u8>,
}

fn encode(input: &[u8]) -> Result<Vec<u8>, ExampleError> {
    if input.is_empty() {
        return Err(ExampleError::EmptyInput);
    }
    Ok(input.iter().map(|byte| byte.wrapping_add(1)).collect())
}

fn identify(bytes: &[u8]) -> Result<String, ExampleError> {
    let sum: u32 = bytes.iter().map(|byte| u32::from(*byte)).sum();
    Ok(format!("object-{sum:08x}"))
}

/// Constructs one object with the caller's timing scope.
fn construct(input: &[u8], scope: TimingScope<'_>) -> Result<Object, ExampleError> {
    scope.run(|content| {
        let bytes = content.child("encode").run(|_| encode(input))?;
        let id = content.child("hash").run(|_| identify(&bytes))?;
        Ok(Object { id, bytes })
    })
}

/// Stores one object with the caller's timing scope.
fn save(object: &Object, scope: TimingScope<'_>) -> Result<usize, ExampleError> {
    scope.run(|storage| {
        let packed = storage.child("pack").run(|_| Ok(object.bytes.clone()))?;
        storage.child("write").run(|_| {
            if object.id.is_empty() {
                return Err(ExampleError::Rejected);
            }
            Ok(packed.len())
        })
    })
}

fn show(
    label: &str,
    result: Result<usize, ExampleError>,
    timings: &TimingReport,
) -> io::Result<()> {
    println!("{label}: {result:?}");
    timings.write_text(io::stdout())
}

fn main() -> io::Result<()> {
    let input = b"layerfs telemetry";

    let (result, timings) = Timing::record("object.create", |root| {
        let object = construct(input, root.child("canonical.construct"))?;
        save(&object, root.child("storage.save"))
    });
    show("measured", result, &timings)?;
    println!();
    timings.write_json(io::stdout())?;

    let (failed, failed_timings) = Timing::record("object.create", |root| {
        let object = construct(b"", root.child("canonical.construct"))?;
        save(&object, root.child("storage.save"))
    });
    println!();
    show("early return", failed, &failed_timings)?;

    let (disabled_result, disabled_timings) = Timing::disabled("object.create", |root| {
        let object = construct(input, root.child("canonical.construct"))?;
        save(&object, root.child("storage.save"))
    });
    println!();
    println!(
        "disabled: {disabled_result:?} (root present: {})",
        disabled_timings.has_root()
    );
    disabled_timings.write_json(io::stdout())
}
