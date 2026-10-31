//! An ordinary command's streams, copied to the caller's sinks as they
//! arrive. Both local backends read the same two pipes and count only what
//! each sink accepted.
use std::io::{self, Read, Write};

/// Copies one stream to its sink until end of stream; the bytes delivered.
pub(crate) fn copy(source: Option<impl Read>, sink: &mut (dyn Write + Send)) -> io::Result<u64> {
    let Some(mut source) = source else {
        return Ok(0);
    };
    let mut total = 0u64;
    let mut window = [0u8; 8192];
    loop {
        let n = match source.read(&mut window) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        sink.write_all(&window[..n])?;
        total = total.saturating_add(n as u64);
    }
}
