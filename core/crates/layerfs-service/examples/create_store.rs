//! Fresh native-import destination through the public C2 creation API.
use layerfs_storage::Store;
use layerfs_telemetry::timer::Timing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Store path required")?;
    Timing::disabled("create", |scope| {
        Store::create(path, Store::default_policy(), scope.child("store"))
    })
    .0?;
    Ok(())
}
