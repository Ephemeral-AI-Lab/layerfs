use layerfs_bridge::adapters::native::{connection::VerifiedPeer, pipe::key};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let private = key(&std::env::var("LAYERFS_PRIVATE_KEY")?)?;
    let peer = VerifiedPeer::from_private(&private)?;
    for byte in peer.public_key() {
        print!("{byte:02x}");
    }
    println!();
    Ok(())
}
