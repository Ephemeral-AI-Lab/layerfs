//! Compile-pass fixture: the documented injected-scope usage must build.

use layerfs_telemetry::timer::{Active, Timing, TimingScope};

#[derive(Debug)]
pub enum DomainError {
    Failed,
}

pub fn construct(input: &[u8], scope: TimingScope<'_>) -> Result<usize, DomainError> {
    scope.run(|content| {
        let bytes = content.child("encode").run(|_| Ok(input.len()))?;
        content.child("hash").run(|_| Ok(bytes + 1))
    })
}

pub fn save(scope: TimingScope<'_>) -> Result<usize, DomainError> {
    scope.run(|storage| {
        let packed = storage.child("pack").run(|_| Ok(1_usize))?;
        storage.child("write").run(|_| Ok(packed))
    })
}

fn main() -> Result<(), DomainError> {
    let input = b"input";
    let (_result, report) = Timing::record(
        "object.create",
        |root: &TimingScope<'_, Active>| {
            let bytes = construct(input, root.child("canonical.construct"))?;
            save(root.child("storage.save")).map(|written| written + bytes)
        },
    );
    let mut sink = Vec::new();
    report.write_json(&mut sink).expect("json");
    report.write_text(&mut sink).expect("text");
    Ok(())
}
