//! Development-only raw BLAKE3 provider; no LayerFS code or canonical parser.
use std::io::{self, Read, Write};

fn main() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let mut size = [0; 8];
        if input.read(&mut size[..1])? == 0 {
            return Ok(());
        }
        input.read_exact(&mut size[1..])?;
        let size = u64::from_be_bytes(size);
        if size > 16 * 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "oracle hash size"));
        }
        let mut bytes = vec![0; size as usize];
        input.read_exact(&mut bytes)?;
        output.write_all(blake3::hash(&bytes).as_bytes())?;
        output.flush()?;
    }
}
